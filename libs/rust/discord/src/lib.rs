#![allow(clippy::missing_errors_doc)]

mod commands;
pub mod embeds;
mod gateway;
pub mod notifications;
mod server;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{
    application::{command::Command, interaction::Interaction},
    http::interaction::InteractionResponse,
    id::{
        Id,
        marker::{ApplicationMarker, ChannelMarker, GuildMarker},
    },
};

pub use gateway::run_gateway;
pub use notifications::{ChapterSummary, LibraryUpdateNotification, library_update_embed};
pub use server::{
    DiscordServerState, build_library_update_notification, notify_library_update,
    register_commands, run_server_gateway,
};

#[derive(Clone)]
pub struct DiscordBot {
    token: SecretString,
    http: Arc<DiscordHttpClient>,
    channel_id: Option<Id<ChannelMarker>>,
}

pub struct DiscordConfig {
    pub bot_token: Option<SecretString>,
    pub channel_id: Option<u64>,
}

/// Handles Discord gateway interactions for the bot runtime.
pub trait DiscordInteractionHandler: Send + Sync + 'static {
    /// Handles one Discord interaction and returns the interaction response.
    fn handle_interaction<'a>(
        &'a self,
        interaction: &'a Interaction,
    ) -> Pin<Box<dyn Future<Output = Result<InteractionResponse>> + Send + 'a>>;

    /// Builds the fallback response used when interaction handling fails.
    fn error_response(&self, interaction: &Interaction) -> InteractionResponse;
}

const NO_GUILD_COMMANDS: &[Command] = &[];

impl DiscordBot {
    #[must_use]
    /// Builds a Discord bot from configuration, returning `None` when no token is configured.
    pub fn from_config(config: DiscordConfig) -> Option<Self> {
        let token = config
            .bot_token
            .filter(|token| !token.expose_secret().trim().is_empty())?;

        let http = Arc::new(DiscordHttpClient::new(token.expose_secret().to_string()));

        Some(Self {
            token,
            http,
            channel_id: config.channel_id.map(Id::new),
        })
    }

    /// Upserts the global Discord application commands for this bot.
    pub async fn register_commands(&self, commands: &[Command]) -> Result<()> {
        let application_id = self.application_id().await?;
        let interaction = self.http.interaction(application_id);
        interaction
            .set_global_commands(commands)
            .await
            .context("failed to upsert discord global commands")?;

        tracing::info!(
            command_count = commands.len(),
            scope = "global",
            "Discord Application Commands Upserted",
        );
        Ok(())
    }

    /// Clears guild-scoped commands for the supplied guilds.
    pub async fn clear_guild_commands(
        &self,
        guild_ids: impl IntoIterator<Item = Id<GuildMarker>>,
    ) -> Result<usize> {
        let application_id = self.application_id().await?;
        let interaction = self.http.interaction(application_id);
        let mut cleared = 0;

        for guild_id in guild_ids {
            match interaction
                .set_guild_commands(guild_id, NO_GUILD_COMMANDS)
                .await
            {
                Ok(_) => cleared += 1,
                Err(error) => tracing::warn!(
                    error = %error,
                    guild_id = %guild_id,
                    "Discord Guild Commands Clear Failed",
                ),
            }
        }

        tracing::info!(
            guild_count = cleared,
            scope = "guild",
            "Discord Guild Commands Cleared",
        );

        Ok(cleared)
    }

    /// Sends a response for a Discord interaction.
    pub async fn create_interaction_response(
        &self,
        interaction: &Interaction,
        response: &InteractionResponse,
    ) -> Result<()> {
        self.http
            .interaction(interaction.application_id)
            .create_response(interaction.id, &interaction.token, response)
            .await
            .context("failed to respond to discord interaction")?;
        Ok(())
    }

    /// Sends a library-update notification to the configured channel.
    pub async fn send_library_update(
        &self,
        notification: &LibraryUpdateNotification,
    ) -> Result<()> {
        let Some(channel_id) = self.channel_id else {
            return Ok(());
        };

        let embeds = [notifications::library_update_embed(notification)];
        self.http
            .create_message(channel_id)
            .embeds(&embeds)
            .await
            .context("failed to send discord library update message")?;
        Ok(())
    }

    #[must_use]
    /// Returns the gateway token used to connect to Discord.
    pub fn gateway_token(&self) -> String {
        self.token.expose_secret().to_string()
    }

    #[must_use]
    /// Returns whether this bot has a notification channel configured.
    pub const fn notifications_enabled(&self) -> bool {
        self.channel_id.is_some()
    }

    async fn application_id(&self) -> Result<Id<ApplicationMarker>> {
        Ok(self
            .http
            .current_user_application()
            .await
            .context("failed to get discord application")?
            .model()
            .await
            .context("failed to decode discord application")?
            .id)
    }
}
