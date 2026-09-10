use anyhow::{Result, anyhow};

use crate::schema::Source;
use crate::{Database, SourceRecordInput};

impl Database {
    /// Inserts or updates all source records from plugin metadata.
    pub async fn sync_sources(&self, sources: &[SourceRecordInput]) -> Result<()> {
        for source in sources {
            self.upsert_source(source).await?;
        }

        Ok(())
    }

    /// Lists persisted source records in stable source-key order.
    pub async fn list_sources(&self) -> Result<Vec<SourceRecordInput>> {
        let mut db = self.executor();
        let mut sources = Source::all().exec(&mut db).await?;
        sources.sort_by(|left, right| left.key.cmp(&right.key));
        sources
            .into_iter()
            .map(|source| {
                Ok(SourceRecordInput {
                    key: source.key,
                    display_name: source.display_name,
                    base_url: source.base_url,
                    version: source.version,
                    plugin_api_version: u32::try_from(source.plugin_api_version)?,
                    capabilities: source.capabilities.to_vec(),
                    enabled: source.enabled,
                })
            })
            .collect()
    }

    /// Lists disabled source keys in stable sorted order.
    pub async fn get_disabled_source_keys(&self) -> Result<Vec<String>> {
        let mut db = self.executor();
        let mut sources = Source::filter(Source::fields().enabled().eq(false))
            .exec(&mut db)
            .await?;
        sources.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(sources.into_iter().map(|source| source.key).collect())
    }

    /// Enables or disables a source, creating a stub row when needed.
    pub async fn set_source_enabled(&self, key: &str, enabled: bool) -> Result<()> {
        self.ensure_source_stub(key, None).await?;

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut source) = find_source(&mut db, key).await? else {
            return Err(anyhow!("source {key} should exist after stub creation"));
        };

        source.update().enabled(enabled).exec(&mut db).await?;

        Ok(())
    }

    /// Returns whether a source is configured to hide NSFW results.
    pub async fn get_source_hide_nsfw(&self, key: &str) -> Result<bool> {
        let mut db = self.executor();
        Ok(find_source(&mut db, key)
            .await?
            .is_some_and(|source| source.hide_nsfw))
    }

    /// Updates the NSFW visibility setting for a source.
    pub async fn set_source_hide_nsfw(&self, key: &str, hide_nsfw: bool) -> Result<()> {
        self.ensure_source_stub(key, None).await?;

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut source) = find_source(&mut db, key).await? else {
            return Err(anyhow!("source {key} should exist after stub creation"));
        };

        source.update().hide_nsfw(hide_nsfw).exec(&mut db).await?;
        Ok(())
    }

    pub(crate) async fn ensure_source_stub(
        &self,
        key: &str,
        base_url: Option<&str>,
    ) -> Result<String> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        if let Some(source) = find_source(&mut db, key).await? {
            return Ok(source.key);
        }

        Source::create()
            .key(key.to_string())
            .display_name(key.to_string())
            .base_url(base_url.unwrap_or_default().to_string())
            .version(String::new())
            .enabled(true)
            .capabilities(Vec::new())
            .plugin_api_version(0)
            .hide_nsfw(false)
            .exec(&mut db)
            .await?;

        Ok(key.to_string())
    }

    async fn upsert_source(&self, source: &SourceRecordInput) -> Result<()> {
        self.ensure_source_stub(&source.key, Some(&source.base_url))
            .await?;

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut existing) = find_source(&mut db, &source.key).await? else {
            return Err(anyhow!(
                "source {} should exist after stub creation",
                source.key
            ));
        };

        existing
            .update()
            .display_name(source.display_name.clone())
            .base_url(source.base_url.clone())
            .version(source.version.clone())
            .enabled(source.enabled)
            .capabilities(source.capabilities.clone())
            .plugin_api_version(i64::from(source.plugin_api_version))
            .exec(&mut db)
            .await?;

        Ok(())
    }
}

async fn find_source(db: &mut toasty::Db, key: &str) -> Result<Option<Source>> {
    Source::filter(Source::fields().key().eq(key))
        .first()
        .exec(db)
        .await
        .map_err(Into::into)
}
