use crate::{
    Database, now_timestamp,
    schema::{AuthSession, PublicShare},
};
use anyhow::Result;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct AuthUser {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
}

pub struct AuthSessionRecord {
    pub config_hash: String,
    pub user: AuthUser,
}

#[derive(Clone, Debug)]
pub struct PublicShareRecord {
    pub id: String,
    pub series_id: String,
}

impl Database {
    pub async fn create_auth_session(
        &self,
        token_hash: &str,
        config_hash: &str,
        user: &AuthUser,
        expires_at: &str,
    ) -> Result<()> {
        let _guard = self.write_guard().await;
        let mut db = self.executor();
        AuthSession::filter(AuthSession::fields().expires_at().le(now_timestamp()))
            .delete()
            .exec(&mut db)
            .await?;
        AuthSession::create()
            .token_hash(token_hash.to_owned())
            .config_hash(config_hash.to_owned())
            .subject(user.id.clone())
            .name(user.name.clone())
            .email(user.email.clone())
            .expires_at(expires_at.to_owned())
            .exec(&mut db)
            .await?;
        Ok(())
    }

    pub async fn auth_session(&self, token_hash: &str) -> Result<Option<AuthSessionRecord>> {
        let mut db = self.executor();
        Ok(AuthSession::filter(
            AuthSession::fields()
                .token_hash()
                .eq(token_hash)
                .and(AuthSession::fields().expires_at().gt(now_timestamp())),
        )
        .first()
        .exec(&mut db)
        .await?
        .map(|session| AuthSessionRecord {
            config_hash: session.config_hash,
            user: AuthUser {
                id: session.subject,
                name: session.name,
                email: session.email,
            },
        }))
    }

    pub async fn delete_auth_session(&self, token_hash: &str) -> Result<()> {
        let _guard = self.write_guard().await;
        AuthSession::filter(AuthSession::fields().token_hash().eq(token_hash))
            .delete()
            .exec(&mut self.executor())
            .await?;
        Ok(())
    }

    pub async fn public_share_for_series(
        &self,
        series_id: &str,
    ) -> Result<Option<PublicShareRecord>> {
        let mut db = self.executor();
        Ok(
            PublicShare::filter(PublicShare::fields().series_id().eq(series_id))
                .first()
                .exec(&mut db)
                .await?
                .map(|share| PublicShareRecord {
                    id: share.id,
                    series_id: share.series_id,
                }),
        )
    }

    pub async fn public_share(&self, id: &str) -> Result<Option<PublicShareRecord>> {
        let mut db = self.executor();
        Ok(PublicShare::filter(PublicShare::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
            .map(|share| PublicShareRecord {
                id: share.id,
                series_id: share.series_id,
            }))
    }

    pub async fn create_public_share(&self, series_id: &str) -> Result<PublicShareRecord> {
        let _guard = self.write_guard().await;
        if let Some(share) = self.public_share_for_series(series_id).await? {
            return Ok(share);
        }
        let share = PublicShare::create()
            .series_id(series_id.to_owned())
            .exec(&mut self.executor())
            .await?;
        Ok(PublicShareRecord {
            id: share.id,
            series_id: share.series_id,
        })
    }

    pub async fn delete_public_share(&self, series_id: &str) -> Result<()> {
        let _guard = self.write_guard().await;
        PublicShare::filter(PublicShare::fields().series_id().eq(series_id))
            .delete()
            .exec(&mut self.executor())
            .await?;
        Ok(())
    }
}
