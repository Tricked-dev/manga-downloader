pub const CURRENT_PLUGIN_API_VERSION: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCapability {
    Search,
    MangaDetails,
    ChapterList,
    PageList,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub categories: Vec<String>,
    pub default_category: Option<String>,
    pub supports_popular_sort: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceManifest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub version: String,
    pub api_version: u32,
    pub capabilities: Vec<SourceCapability>,
    pub search_options: SearchOptions,
    pub build_metadata: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
}

pub struct SourceManifestBuilder {
    manifest: SourceManifest,
}

impl SourceManifestBuilder {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            manifest: SourceManifest {
                id: id.into(),
                name: name.into(),
                base_url: base_url.into(),
                version: version.into(),
                api_version: CURRENT_PLUGIN_API_VERSION,
                capabilities: Vec::new(),
                search_options: SearchOptions::default(),
                build_metadata: None,
                homepage: None,
                repository: None,
            },
        }
    }

    #[must_use]
    pub fn search(mut self) -> Self {
        self.push_capability(SourceCapability::Search);
        self
    }

    #[must_use]
    pub fn manga_details(mut self) -> Self {
        self.push_capability(SourceCapability::MangaDetails);
        self
    }

    #[must_use]
    pub fn chapter_list(mut self) -> Self {
        self.push_capability(SourceCapability::ChapterList);
        self
    }

    #[must_use]
    pub fn page_list(mut self) -> Self {
        self.push_capability(SourceCapability::PageList);
        self
    }

    #[must_use]
    pub fn search_categories(
        mut self,
        categories: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.manifest.search_options.categories = categories.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn default_search_category(mut self, category: impl Into<String>) -> Self {
        self.manifest.search_options.default_category = Some(category.into());
        self
    }

    #[must_use]
    pub fn supports_popular_sort(mut self) -> Self {
        self.manifest.search_options.supports_popular_sort = true;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn build_metadata(mut self, build_metadata: impl Into<String>) -> Self {
        self.manifest.build_metadata = Some(build_metadata.into());
        self
    }

    #[must_use]
    pub fn homepage(mut self, homepage: impl Into<String>) -> Self {
        self.manifest.homepage = Some(homepage.into());
        self
    }

    #[must_use]
    pub fn repository(mut self, repository: impl Into<String>) -> Self {
        self.manifest.repository = Some(repository.into());
        self
    }

    #[must_use]
    pub fn build(self) -> SourceManifest {
        self.manifest
    }

    fn push_capability(&mut self, capability: SourceCapability) {
        if !self.manifest.capabilities.contains(&capability) {
            self.manifest.capabilities.push(capability);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_manifest_builder_composes_plugin_contract() {
        let manifest = SourceManifestBuilder::new(
            "comix",
            "Comix",
            "https://comix.to",
            env!("CARGO_PKG_VERSION"),
        )
        .search()
        .search()
        .manga_details()
        .chapter_list()
        .page_list()
        .search_categories(["Popular", "Latest"])
        .default_search_category("Popular")
        .supports_popular_sort()
        .build_metadata("test-build")
        .homepage("https://comix.to")
        .repository("https://example.test/repo")
        .build();

        assert_eq!(manifest.id, "comix");
        assert_eq!(manifest.api_version, CURRENT_PLUGIN_API_VERSION);
        assert_eq!(
            manifest.capabilities,
            vec![
                SourceCapability::Search,
                SourceCapability::MangaDetails,
                SourceCapability::ChapterList,
                SourceCapability::PageList,
            ]
        );
        assert_eq!(
            manifest.search_options.categories,
            vec!["Popular".to_string(), "Latest".to_string()]
        );
        assert_eq!(
            manifest.search_options.default_category.as_deref(),
            Some("Popular")
        );
        assert!(manifest.search_options.supports_popular_sort);
        assert_eq!(manifest.build_metadata.as_deref(), Some("test-build"));
        assert_eq!(manifest.homepage.as_deref(), Some("https://comix.to"));
        assert_eq!(
            manifest.repository.as_deref(),
            Some("https://example.test/repo")
        );
    }
}
