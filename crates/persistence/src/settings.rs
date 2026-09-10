use std::collections::HashMap;

use anyhow::Result;
use backend_core::settings::SETTING_DEFINITIONS;

use crate::Database;
use crate::schema::AppSetting;

impl Database {
    /// Reads one application setting by key.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut db = self.executor();
        AppSetting::filter(AppSetting::fields().key().eq(key))
            .first()
            .exec(&mut db)
            .await
            .map(|setting| setting.map(|setting| setting.value))
            .map_err(Into::into)
    }

    /// Inserts or updates one application setting.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let sql = match self.backend() {
            crate::DatabaseBackend::Sqlite => {
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value"
            }
            crate::DatabaseBackend::Postgres => {
                "INSERT INTO app_settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value"
            }
        };
        toasty::sql::statement(sql)
            .bind(key)
            .bind(value)
            .exec(&mut db)
            .await?;

        Ok(())
    }

    /// Reads all persisted application settings into a key-value map.
    pub async fn get_all_settings(&self) -> Result<HashMap<String, String>> {
        let mut db = self.executor();
        let settings = AppSetting::all().exec(&mut db).await?;
        Ok(settings
            .into_iter()
            .map(|setting| (setting.key, setting.value))
            .collect())
    }

    pub(crate) async fn seed_default_settings(&self) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let sql = match self.backend() {
            crate::DatabaseBackend::Sqlite => {
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO NOTHING"
            }
            crate::DatabaseBackend::Postgres => {
                "INSERT INTO app_settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING"
            }
        };
        for definition in SETTING_DEFINITIONS {
            toasty::sql::statement(sql)
                .bind(definition.key)
                .bind(definition.default)
                .exec(&mut db)
                .await?;
        }

        Ok(())
    }
}
