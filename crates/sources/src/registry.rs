use crate::{
    Chapter, Manga, Page, SearchQuery, SearchResults, Source, SourceCapability, SourceHttpClient,
    SourceMediaClient,
};
use anyhow::Result;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use tokio::sync::watch;

#[allow(clippy::struct_excessive_bools)]
#[derive(serde::Serialize, serde::Deserialize, Clone, utoipa::ToSchema)]
pub struct SourceInfo {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub plugin_version: String,
    pub plugin_api_version: u32,
    pub capabilities: Vec<String>,
    pub search_categories: Vec<String>,
    pub default_search_category: Option<String>,
    pub supports_search_popularity: bool,
    pub homepage: Option<String>,
    pub source_repository: Option<String>,
    pub build_metadata: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceRegistryError {
    #[error("Source '{name}' not found")]
    NotFound { name: String },
    #[error("Source '{name}' is disabled")]
    Disabled { name: String },
    #[error("Source '{name}' does not support '{capability}'")]
    UnsupportedCapability { name: String, capability: String },
}
#[derive(Debug, Clone, Copy)]
pub struct SourceHealth {
    pub loaded_sources: usize,
    pub enabled_sources: usize,
}

#[derive(Clone)]
pub struct SourceActivity {
    active: watch::Sender<usize>,
}
impl SourceActivity {
    pub async fn wait_for_idle(&self) {
        let mut receiver = self.active.subscribe();
        loop {
            if *receiver.borrow_and_update() == 0 {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
    fn begin(&self) -> CallPermit {
        self.active.send_modify(|count| *count += 1);
        CallPermit(self.clone())
    }
}
struct CallPermit(SourceActivity);
impl Drop for CallPermit {
    fn drop(&mut self) {
        self.0.active.send_modify(|count| *count -= 1);
    }
}

pub struct SourceRegistry {
    sources: BTreeMap<String, Arc<dyn Source>>,
    disabled: HashSet<String>,
    http: SourceHttpClient,
    activity: SourceActivity,
}
impl SourceRegistry {
    pub fn new() -> Result<Self> {
        let http = SourceHttpClient::new()?;
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(crate::comix::ComixSource::new(http.clone())),
            Arc::new(crate::rawkuma::RawkumaSource::new(http.clone())),
        ];
        Ok(Self::from_sources(http, sources))
    }
    pub fn from_sources(http: SourceHttpClient, sources: Vec<Arc<dyn Source>>) -> Self {
        Self {
            sources: sources
                .into_iter()
                .map(|source| (source.metadata().id.clone(), source))
                .collect(),
            disabled: HashSet::new(),
            http,
            activity: SourceActivity {
                active: watch::channel(0).0,
            },
        }
    }
    pub fn sources(&self) -> Vec<SourceInfo> {
        self.sources
            .values()
            .map(|source| {
                let m = source.metadata();
                SourceInfo {
                    name: m.id.clone(),
                    display_name: m.name.clone(),
                    base_url: m.base_url.clone(),
                    plugin_version: m.version.clone(),
                    plugin_api_version: m.api_version,
                    capabilities: m
                        .capabilities
                        .iter()
                        .map(|c| c.wire_name().into())
                        .collect(),
                    search_categories: m.search_options.categories.clone(),
                    default_search_category: m.search_options.default_category.clone(),
                    supports_search_popularity: m.search_options.supports_popular_sort,
                    homepage: m.homepage.clone(),
                    source_repository: m.repository.clone(),
                    build_metadata: m.build_metadata.clone(),
                    enabled: !self.disabled.contains(&m.id),
                }
            })
            .collect()
    }
    fn source(&self, name: &str) -> Result<&dyn Source> {
        self.sources
            .get(name)
            .map(AsRef::as_ref)
            .ok_or_else(|| SourceRegistryError::NotFound { name: name.into() }.into())
    }
    fn require(
        &self,
        name: &str,
        capability: SourceCapability,
    ) -> Result<(&dyn Source, CallPermit)> {
        let source = self.source(name)?;
        if self.disabled.contains(name) {
            return Err(SourceRegistryError::Disabled { name: name.into() }.into());
        }
        if !source.metadata().capabilities.contains(&capability) {
            return Err(SourceRegistryError::UnsupportedCapability {
                name: name.into(),
                capability: capability.wire_name().into(),
            }
            .into());
        }
        Ok((source, self.activity.begin()))
    }
    pub fn source_base_url(&self, name: &str) -> Result<String> {
        Ok(self.source(name)?.metadata().base_url.clone())
    }
    pub fn set_source_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        self.source(name)?;
        if enabled {
            self.disabled.remove(name);
        } else {
            self.disabled.insert(name.into());
        }
        Ok(())
    }
    pub fn set_disabled_sources(&mut self, names: &[String]) {
        self.disabled = names
            .iter()
            .filter(|name| self.sources.contains_key(*name))
            .cloned()
            .collect();
    }
    pub fn health_check(&self) -> Result<SourceHealth> {
        Ok(SourceHealth {
            loaded_sources: self.sources.len(),
            enabled_sources: self.sources.len() - self.disabled.len(),
        })
    }
    pub fn runtime_activity(&self) -> SourceActivity {
        self.activity.clone()
    }
    pub fn media_client(&self, name: &str) -> Result<SourceMediaClient> {
        self.source(name)?;
        Ok(self.unscoped_media_client())
    }
    pub fn unscoped_media_client(&self) -> SourceMediaClient {
        SourceMediaClient {
            http: self.http.clone(),
        }
    }
    pub async fn search_manga(
        &self,
        name: &str,
        query: &str,
        page: u32,
        category: Option<&str>,
        popular: bool,
    ) -> Result<SearchResults> {
        let (source, _permit) = self.require(name, SourceCapability::Search)?;
        Ok(source
            .search(SearchQuery {
                text: query.into(),
                page,
                category: category.map(Into::into),
                popular,
            })
            .await?)
    }
    pub async fn get_manga_details(&self, name: &str, id: &str) -> Result<Manga> {
        let (source, _permit) = self.require(name, SourceCapability::MangaDetails)?;
        Ok(source.manga(id).await?)
    }
    pub async fn get_chapter_list(&self, name: &str, id: &str) -> Result<Vec<Chapter>> {
        let (source, _permit) = self.require(name, SourceCapability::ChapterList)?;
        Ok(source.chapters(id).await?)
    }
    pub async fn get_page_list(&self, name: &str, id: &str) -> Result<Vec<Page>> {
        let (source, _permit) = self.require(name, SourceCapability::PageList)?;
        Ok(source.pages(id).await?)
    }
}
impl SourceCapability {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::MangaDetails => "manga_details",
            Self::ChapterList => "chapter_list",
            Self::PageList => "page_list",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn disabled_source_rejects_calls_without_network() {
        let mut registry = SourceRegistry::new().unwrap();
        assert_eq!(
            registry
                .sources()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["comix", "rawkuma"]
        );
        registry.set_disabled_sources(&["rawkuma".into(), "unknown".into()]);
        let error = registry
            .get_page_list("rawkuma", "anything")
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<SourceRegistryError>(),
            Some(SourceRegistryError::Disabled { .. })
        ));
        assert_eq!(registry.health_check().unwrap().enabled_sources, 1);
        registry.set_source_enabled("rawkuma", true).unwrap();
        assert!(registry.sources().iter().all(|s| s.enabled));
        assert!(registry.set_source_enabled("unknown", true).is_err());
    }

    #[tokio::test]
    async fn cancelled_call_releases_shutdown_waiter() {
        let activity = SourceActivity {
            active: watch::channel(0).0,
        };
        let permit = activity.begin();
        let waiter = tokio::spawn({
            let activity = activity.clone();
            async move {
                activity.wait_for_idle().await;
            }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        // Dropping the call future (including cancellation/panic) drops its permit.
        drop(permit);
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
    }
}
