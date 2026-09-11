use crate::{Database, new_id, now_timestamp, schema::ApiToken};
use anyhow::Result;

/// Never carries the token itself. Only the hash is stored, so a database read cannot
/// hand anyone a working credential; the plaintext exists once, at creation.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ApiTokenRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<ApiToken> for ApiTokenRow {
    fn from(row: ApiToken) -> Self {
        Self {
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

impl Database {
    pub async fn create_api_token(&self, name: &str, token_hash: &str) -> Result<ApiTokenRow> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let row = ApiToken::create()
            .id(new_id())
            .name(name)
            .token_hash(token_hash)
            .created_at(now_timestamp())
            .last_used_at(None::<String>)
            .exec(&mut db)
            .await?;
        Ok(row.into())
    }

    pub async fn list_api_tokens(&self) -> Result<Vec<ApiTokenRow>> {
        let mut db = self.executor();
        let mut rows: Vec<ApiTokenRow> = ApiToken::all()
            .exec(&mut db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        rows.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(rows)
    }

    pub async fn delete_api_token(&self, id: &str) -> Result<bool> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(row) = ApiToken::filter(ApiToken::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(false);
        };
        row.delete().exec(&mut db).await?;
        Ok(true)
    }

    /// Records use as a side effect so an abandoned device is identifiable before it is
    /// revoked. A token that never expires makes that the only signal there is.
    pub async fn consume_api_token(&self, token_hash: &str) -> Result<bool> {
        let mut db = self.executor();
        let Some(mut row) = ApiToken::filter(ApiToken::fields().token_hash().eq(token_hash))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(false);
        };
        let _write = self.write_guard().await;
        row.update()
            .last_used_at(Some(now_timestamp()))
            .exec(&mut db)
            .await?;
        Ok(true)
    }
}
