use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use backend_persistence::{ChapterRow, Database, MangaRow};
use backend_plugin_host::PluginManager;
use tokio::sync::RwLock;
use tokio_graceful::ShutdownGuard;
use twilight_model::{
    application::interaction::Interaction, http::interaction::InteractionResponse,
};

use crate::{
    ChapterSummary, DiscordBot, DiscordInteractionHandler, LibraryUpdateNotification, commands,
    run_gateway as run_discord_gateway,
};

/// Application state required by the Discord server integration.
pub trait DiscordServerState: Send + Sync + 'static {
    /// Returns the configured Discord bot, if Discord integration is enabled.
    fn discord_bot(&self) -> Option<DiscordBot>;

    /// Returns the persistence handle used by Discord commands.
    fn db(&self) -> &Database;

    /// Returns the plugin manager used by Discord commands.
    fn plugin_manager(&self) -> &RwLock<PluginManager>;

    /// Records a Discord notification metric.
    fn record_discord_notification(&self, kind: &str, outcome: &str);

    /// Triggers a library update from a Discord command.
    fn check_for_updates(
        self: Arc<Self>,
        trigger: &'static str,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send>>;
}

/// Registers the server Discord command set.
pub async fn register_commands(bot: &DiscordBot) -> Result<()> {
    bot.register_commands(&commands::application_commands())
        .await
}

/// Runs the server Discord gateway when a bot is configured.
pub async fn run_server_gateway<S>(state: Arc<S>, guard: ShutdownGuard) -> Result<()>
where
    S: DiscordServerState,
{
    let Some(bot) = state.discord_bot() else {
        return Ok(());
    };

    Box::pin(run_discord_gateway(
        bot,
        Arc::new(ServerInteractionHandler { state }),
        guard,
    ))
    .await
}

/// Sends or records the outcome for a library update notification.
pub async fn notify_library_update<S>(state: &Arc<S>, notification: &LibraryUpdateNotification)
where
    S: DiscordServerState,
{
    let Some(discord) = state.discord_bot() else {
        state.record_discord_notification("library_update", "disabled");
        return;
    };

    if let Err(error) = discord.send_library_update(notification).await {
        state.record_discord_notification("library_update", "error");
        tracing::warn!(
            error = %error,
            manga_title = %notification.manga_title,
            source = %notification.source,
            "Discord Library Update Notification Failed",
        );
    } else {
        state.record_discord_notification("library_update", "sent");
    }
}

#[must_use]
/// Builds a notification payload for newly discovered chapters.
pub fn build_library_update_notification(
    manga: &MangaRow,
    chapters: Vec<ChapterRow>,
    new_ids: &[String],
    enqueued_downloads: usize,
) -> Option<LibraryUpdateNotification> {
    if new_ids.is_empty() {
        return None;
    }

    let new_ids = new_ids.iter().collect::<std::collections::HashSet<_>>();
    let chapters = chapters
        .into_iter()
        .filter(|chapter| new_ids.contains(&chapter.id))
        .map(|chapter| ChapterSummary {
            title: chapter.title,
            number: chapter.chapter_number,
        })
        .collect::<Vec<_>>();

    if chapters.is_empty() {
        return None;
    }

    Some(LibraryUpdateNotification {
        manga_title: manga.title.clone(),
        source: manga.source.clone(),
        cover_url: manga.cover_url.clone(),
        chapters,
        enqueued_downloads,
    })
}

struct ServerInteractionHandler<S> {
    state: Arc<S>,
}

impl<S> DiscordInteractionHandler for ServerInteractionHandler<S>
where
    S: DiscordServerState,
{
    fn handle_interaction<'a>(
        &'a self,
        interaction: &'a Interaction,
    ) -> Pin<Box<dyn Future<Output = Result<InteractionResponse>> + Send + 'a>> {
        Box::pin(commands::handle_interaction(&self.state, interaction))
    }

    fn error_response(&self, interaction: &Interaction) -> InteractionResponse {
        commands::error_response(interaction)
    }
}
