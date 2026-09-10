use crate::{DiscordServerState, embeds};

mod components;
mod formatting;
mod validation;

use anyhow::Result;
use autometrics::autometrics;
use backend_core::settings::SettingKey;
use components::{embed_response, message_response};
use formatting::{
    blank_as_uncategorized, format_command_update, format_counts, format_download_item,
    format_library_item, format_settings_list, format_source_item, format_update_sections,
    is_active_download_status, library_matches_query,
};
use std::{collections::HashMap, sync::Arc};
use twilight_interactions::command::{
    CommandInputData, CommandModel, CommandOption, CreateCommand, CreateOption,
};
use twilight_model::{
    application::command::Command,
    application::interaction::{
        Interaction, InteractionData, InteractionType, application_command::CommandData,
        message_component::MessageComponentInteractionData,
    },
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::{Id, marker::InteractionMarker},
};
use validation::{next_update_interval, normalize_setting_value};

const MAX_COMMAND_ITEMS: usize = 5;
const MAX_COMMAND_SOURCES: usize = 8;
const MAX_COMMAND_UPDATES: usize = 5;
const COMPONENT_STATUS: &str = "manga:status";
const COMPONENT_LIBRARY: &str = "manga:library";
const COMPONENT_DOWNLOADS: &str = "manga:downloads";
const COMPONENT_SOURCES: &str = "manga:sources";
const COMPONENT_UPDATES: &str = "manga:updates";
const COMPONENT_REFRESH: &str = "manga:refresh";
const COMPONENT_HELP: &str = "manga:help";
const COMPONENT_SETTINGS: &str = "settings:overview";
const COMPONENT_TOGGLE_AUTO_DOWNLOAD: &str = "settings:toggle_auto_download";
const COMPONENT_CYCLE_UPDATE_INTERVAL: &str = "settings:cycle_update_interval";

const SETTING_UPDATE_INTERVAL: &str = SettingKey::UpdateIntervalHours.as_str();
const SETTING_AUTO_DOWNLOAD: &str = SettingKey::AutoDownloadNewChapters.as_str();
const SETTING_AUTO_DOWNLOAD_CATEGORY: &str = SettingKey::AutoDownloadCategory.as_str();
const SETTING_DOWNLOAD_PATH: &str = SettingKey::DownloadPath.as_str();
const SETTING_DOWNLOAD_CONCURRENT_CHAPTERS: &str = SettingKey::DownloadConcurrentChapters.as_str();
const SETTING_DOWNLOAD_PAGE_FETCH_CONCURRENCY: &str =
    SettingKey::DownloadPageFetchConcurrency.as_str();
const SETTING_MAX_DOWNLOAD_STORAGE: &str = SettingKey::MaxDownloadStorageBytes.as_str();
const SETTING_LIBRARY_CATEGORIES: &str = SettingKey::LibraryCategories.as_str();
const SETTING_CACHE_MEMORY: &str = SettingKey::CacheMaxMemoryBytes.as_str();
const SETTING_AVIF_WORKERS: &str = SettingKey::AvifConversionWorkers.as_str();
const UPDATE_INTERVAL_CHOICES: &[&str] = &["0", "0.5", "1", "3", "6", "12", "24"];
const SETTINGS_LIST: [(&str, &str, &str); 10] = [
    (
        SETTING_UPDATE_INTERVAL,
        "Update interval hours",
        SettingKey::UpdateIntervalHours.default_value(),
    ),
    (
        SETTING_AUTO_DOWNLOAD,
        "Auto-download new chapters",
        SettingKey::AutoDownloadNewChapters.default_value(),
    ),
    (
        SETTING_AUTO_DOWNLOAD_CATEGORY,
        "Auto-download category",
        SettingKey::AutoDownloadCategory.default_value(),
    ),
    (
        SETTING_DOWNLOAD_PATH,
        "Download path",
        SettingKey::DownloadPath.default_value(),
    ),
    (
        SETTING_DOWNLOAD_CONCURRENT_CHAPTERS,
        "Concurrent chapter downloads",
        SettingKey::DownloadConcurrentChapters.default_value(),
    ),
    (
        SETTING_DOWNLOAD_PAGE_FETCH_CONCURRENCY,
        "Page fetch concurrency",
        SettingKey::DownloadPageFetchConcurrency.default_value(),
    ),
    (
        SETTING_MAX_DOWNLOAD_STORAGE,
        "Max storage bytes",
        SettingKey::MaxDownloadStorageBytes.default_value(),
    ),
    (
        SETTING_LIBRARY_CATEGORIES,
        "Library categories",
        SettingKey::LibraryCategories.default_value(),
    ),
    (
        SETTING_CACHE_MEMORY,
        "Cache memory bytes",
        SettingKey::CacheMaxMemoryBytes.default_value(),
    ),
    (
        SETTING_AVIF_WORKERS,
        "AVIF workers",
        SettingKey::AvifConversionWorkers.default_value(),
    ),
];

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "manga",
    desc = "Open manga-server controls or search the library"
)]
struct MangaCommand {
    /// Search your library by title, source, or category
    #[command(min_length = 1, max_length = 100)]
    query: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Show or update manga-server settings")]
struct SettingsCommand {
    /// Setting to show or update
    setting: Option<SettingsKey>,
    /// New value for the selected setting
    #[command(max_length = 500)]
    value: Option<String>,
}

#[derive(CommandOption, CreateOption)]
enum SettingsKey {
    #[option(name = "Update interval hours", value = "update_interval_hours")]
    UpdateIntervalHours,
    #[option(
        name = "Auto-download new chapters",
        value = "auto_download_new_chapters"
    )]
    AutoDownloadNewChapters,
    #[option(name = "Auto-download category", value = "auto_download_category")]
    AutoDownloadCategory,
    #[option(name = "Download path", value = "download_path")]
    DownloadPath,
    #[option(
        name = "Concurrent chapter downloads",
        value = "download_concurrent_chapters"
    )]
    DownloadConcurrentChapters,
    #[option(
        name = "Page fetch concurrency",
        value = "download_page_fetch_concurrency"
    )]
    DownloadPageFetchConcurrency,
    #[option(
        name = "Max download storage bytes",
        value = "max_download_storage_bytes"
    )]
    MaxDownloadStorageBytes,
    #[option(name = "Library categories", value = "library_categories")]
    LibraryCategories,
    #[option(name = "Cache max memory bytes", value = "cache_max_memory_bytes")]
    CacheMemory,
    #[option(name = "AVIF conversion workers", value = "avif_conversion_workers")]
    AvifWorkers,
}

impl SettingsKey {
    fn key(&self) -> &'static str {
        self.value()
    }
}

struct CommandMangaSummary {
    title: String,
    cover_url: String,
}

pub(super) fn application_commands() -> [Command; 2] {
    [
        MangaCommand::create_command().into(),
        SettingsCommand::create_command().into(),
    ]
}

#[autometrics(track_concurrency)]
pub(super) async fn handle_interaction<S>(
    state: &Arc<S>,
    interaction: &Interaction,
) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    match interaction.kind {
        InteractionType::Ping => Ok(InteractionResponse {
            kind: InteractionResponseType::Pong,
            data: None,
        }),
        InteractionType::ApplicationCommand => {
            let Some(InteractionData::ApplicationCommand(data)) = interaction.data.as_ref() else {
                anyhow::bail!("application command interaction missing command data");
            };
            handle_command(state, data, interaction.id).await
        }
        InteractionType::MessageComponent => {
            let Some(InteractionData::MessageComponent(data)) = interaction.data.as_ref() else {
                anyhow::bail!("message component interaction missing component data");
            };
            handle_component(state, data, interaction.id).await
        }
        _ => Ok(message_response(
            "Unsupported interaction",
            "That interaction type is not supported.",
            embeds::COLOR_WARNING,
        )),
    }
}

pub(super) fn error_response(interaction: &Interaction) -> InteractionResponse {
    let mut response = message_response(
        "Command failed",
        "Check the server logs for details.",
        embeds::COLOR_ERROR,
    );
    if matches!(interaction.kind, InteractionType::MessageComponent) {
        response.kind = InteractionResponseType::UpdateMessage;
    }
    response
}

#[autometrics(track_concurrency)]
async fn handle_command<S>(
    state: &Arc<S>,
    data: &CommandData,
    interaction_id: Id<InteractionMarker>,
) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    tracing::debug!(
        command = %data.name,
        interaction_id = %interaction_id,
        "Discord Application Command",
    );

    match data.name.as_str() {
        MangaCommand::NAME => manga_command(state, data).await,
        SettingsCommand::NAME => settings_command(state, data).await,
        _ => Ok(message_response(
            "Unknown command",
            "That manga-server command is not registered here.",
            embeds::COLOR_WARNING,
        )),
    }
}

#[autometrics(track_concurrency)]
async fn manga_command<S>(state: &Arc<S>, data: &CommandData) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let command = MangaCommand::from_interaction(CommandInputData {
        options: data.options.clone(),
        resolved: data.resolved.as_ref().map(std::borrow::Cow::Borrowed),
    })?;

    if let Some(query) = command.query {
        return library_search_command(state, &query).await;
    }

    Ok(manga_home_command())
}

#[autometrics(track_concurrency)]
async fn settings_command<S>(state: &Arc<S>, data: &CommandData) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let command = SettingsCommand::from_interaction(CommandInputData {
        options: data.options.clone(),
        resolved: data.resolved.as_ref().map(std::borrow::Cow::Borrowed),
    })?;
    let setting = command.setting.as_ref().map(SettingsKey::key);
    let value = command.value.as_deref();

    match (setting, value) {
        (Some(setting), Some(value)) => update_setting_command(state, setting, value).await,
        (Some(setting), None) => {
            settings_overview_command(state, COMPONENT_SETTINGS, Some(setting)).await
        }
        (None, Some(_)) => Ok(message_response(
            "Choose a setting",
            "Pass a setting name with the new value.",
            embeds::COLOR_WARNING,
        )),
        (None, None) => settings_overview_command(state, COMPONENT_SETTINGS, None).await,
    }
}

#[autometrics(track_concurrency)]
async fn handle_component<S>(
    state: &Arc<S>,
    data: &MessageComponentInteractionData,
    interaction_id: Id<InteractionMarker>,
) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    tracing::debug!(
        custom_id = %data.custom_id,
        interaction_id = %interaction_id,
        "Discord Message Component",
    );

    let mut response = match data.custom_id.as_str() {
        COMPONENT_STATUS => status_command(state, COMPONENT_STATUS).await,
        COMPONENT_LIBRARY => library_command(state, COMPONENT_LIBRARY).await,
        COMPONENT_DOWNLOADS => downloads_command(state, COMPONENT_DOWNLOADS).await,
        COMPONENT_SOURCES => sources_command(state, COMPONENT_SOURCES).await,
        COMPONENT_UPDATES => updates_command(state, COMPONENT_UPDATES).await,
        COMPONENT_REFRESH => Ok(refresh_command(state, COMPONENT_REFRESH)),
        COMPONENT_HELP => Ok(help_command(COMPONENT_HELP)),
        COMPONENT_SETTINGS => settings_overview_command(state, COMPONENT_SETTINGS, None).await,
        COMPONENT_TOGGLE_AUTO_DOWNLOAD => toggle_auto_download_setting(state).await,
        COMPONENT_CYCLE_UPDATE_INTERVAL => cycle_update_interval_setting(state).await,
        _ => Ok(message_response(
            "Unknown component",
            "That Discord button is not registered here.",
            embeds::COLOR_WARNING,
        )),
    }?;

    response.kind = InteractionResponseType::UpdateMessage;
    Ok(response)
}

fn manga_home_command() -> InteractionResponse {
    let mut embed = embeds::rich(
        "Manga controls",
        Some("Choose a section below, refresh the library, or search with `/manga query:<title>`."),
        embeds::COLOR_INFO,
    );
    embed.fields = vec![embeds::field(
        "Sections",
        "`Status`, `Library`, `Downloads`, `Sources`, and `Updates` are available from the buttons below.",
        false,
    )];
    embed_response(embed, "")
}

#[autometrics]
async fn library_search_command<S>(state: &Arc<S>, query: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let query = query.trim();
    if query.is_empty() {
        return Ok(manga_home_command());
    }

    let normalized_query = query.to_lowercase();
    let mut matches = state
        .db()
        .get_library_manga()
        .await?
        .into_iter()
        .filter(|manga| library_matches_query(manga, &normalized_query))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| right.last_updated.cmp(&left.last_updated));

    if matches.is_empty() {
        return Ok(message_response(
            "No library matches",
            "No saved manga matched that search.",
            embeds::COLOR_WARNING,
        ));
    }

    let result_count = matches.len();
    let results = matches
        .iter()
        .take(MAX_COMMAND_ITEMS)
        .map(format_library_item)
        .collect::<Vec<_>>()
        .join("\n");
    let footer = if result_count > MAX_COMMAND_ITEMS {
        format!("{result_count} matches. Showing first {MAX_COMMAND_ITEMS}.")
    } else {
        format!("{result_count} matches.")
    };

    let mut embed = embeds::rich(
        "Library search",
        Some(&format!("Results for `{query}`.")),
        embeds::COLOR_INFO,
    );
    embed.fields = vec![embeds::field("Matches", &results, false)];
    embed.footer = Some(embeds::footer(&footer));
    embed.thumbnail = matches
        .iter()
        .find_map(|manga| embeds::thumbnail(&manga.cover_url));

    Ok(embed_response(embed, COMPONENT_LIBRARY))
}

#[autometrics]
async fn settings_overview_command<S>(
    state: &Arc<S>,
    active_component: &str,
    highlighted_setting: Option<&str>,
) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let settings = state.db().get_all_settings().await?;
    let notifications = state
        .discord_bot()
        .is_some_and(|bot| bot.notifications_enabled());
    let title = highlighted_setting.map_or_else(
        || "Manga settings".to_owned(),
        |setting| format!("Setting: {setting}"),
    );
    let description = highlighted_setting
        .and_then(|setting| {
            settings
                .get(setting)
                .map(|value| format!("Current value: `{value}`"))
        })
        .unwrap_or_else(|| "Current settings used by manga-server.".to_owned());

    let mut embed = embeds::rich(&title, Some(&description), embeds::COLOR_INFO);
    embed.fields = vec![
        embeds::field("Settings", &format_settings_list(&settings), false),
        embeds::field(
            "Runtime",
            &format!(
                "Discord notifications: `{}`",
                if notifications { "enabled" } else { "disabled" },
            ),
            false,
        ),
    ];
    embed.footer = Some(embeds::footer(
        "Use /settings setting:<key> value:<value> to update a setting.",
    ));

    Ok(embed_response(embed, active_component))
}

#[autometrics]
async fn update_setting_command<S>(
    state: &Arc<S>,
    setting: &str,
    value: &str,
) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let value = match normalize_setting_value(setting, value) {
        Ok(value) => value,
        Err(message) => {
            return Ok(message_response(
                "Invalid setting value",
                &message,
                embeds::COLOR_WARNING,
            ));
        }
    };
    state.db().set_setting(setting, &value).await?;
    settings_overview_command(state, COMPONENT_SETTINGS, Some(setting)).await
}

#[autometrics]
async fn toggle_auto_download_setting<S>(state: &Arc<S>) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let current = state
        .db()
        .get_setting(SETTING_AUTO_DOWNLOAD)
        .await?
        .is_some_and(|value| value == "true");
    state
        .db()
        .set_setting(
            SETTING_AUTO_DOWNLOAD,
            if current { "false" } else { "true" },
        )
        .await?;
    settings_overview_command(
        state,
        COMPONENT_TOGGLE_AUTO_DOWNLOAD,
        Some(SETTING_AUTO_DOWNLOAD),
    )
    .await
}

#[autometrics]
async fn cycle_update_interval_setting<S>(state: &Arc<S>) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let current = state
        .db()
        .get_setting(SETTING_UPDATE_INTERVAL)
        .await?
        .unwrap_or_else(|| "1".to_owned());
    let next = next_update_interval(&current);
    state
        .db()
        .set_setting(SETTING_UPDATE_INTERVAL, next)
        .await?;
    settings_overview_command(
        state,
        COMPONENT_CYCLE_UPDATE_INTERVAL,
        Some(SETTING_UPDATE_INTERVAL),
    )
    .await
}

#[autometrics]
async fn status_command<S>(state: &Arc<S>, active_component: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let library = state.db().get_library_manga().await?;
    let downloads = state.db().get_downloads().await?;
    let queued = downloads
        .iter()
        .filter(|download| download.status == "queued")
        .count();
    let active = downloads
        .iter()
        .filter(|download| is_active_download_status(&download.status))
        .count();
    let notifications = state
        .discord_bot()
        .is_some_and(|bot| bot.notifications_enabled());

    let mut embed = embeds::rich(
        "Manga server status",
        Some("Current library, download queue, and notification state."),
        embeds::COLOR_INFO,
    );
    let library_count = format!("{} series", library.len());
    let queued_count = queued.to_string();
    let active_count = active.to_string();
    embed.fields = vec![
        embeds::field("Library", &library_count, true),
        embeds::field("Queued", &queued_count, true),
        embeds::field("Active", &active_count, true),
        embeds::field(
            "Update notifications",
            if notifications { "Enabled" } else { "Disabled" },
            false,
        ),
    ];
    Ok(embed_response(embed, active_component))
}

#[autometrics]
async fn library_command<S>(state: &Arc<S>, active_component: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let mut library = state.db().get_library_manga().await?;
    if library.is_empty() {
        return Ok(message_response(
            "Library is empty",
            "Add manga from the web UI or an external client and they will appear here.",
            embeds::COLOR_WARNING,
        ));
    }

    library.sort_by(|left, right| right.last_updated.cmp(&left.last_updated));

    let total_chapters = library
        .iter()
        .map(|manga| manga.total_chapters)
        .sum::<usize>();
    let downloaded_chapters = library
        .iter()
        .map(|manga| manga.downloaded_chapters)
        .sum::<usize>();
    let categories = format_counts(
        library
            .iter()
            .map(|manga| blank_as_uncategorized(&manga.category)),
        MAX_COMMAND_ITEMS,
    );
    let sources = format_counts(
        library.iter().map(|manga| manga.source.as_str()),
        MAX_COMMAND_ITEMS,
    );
    let recently_updated = library
        .iter()
        .take(MAX_COMMAND_ITEMS)
        .map(format_library_item)
        .collect::<Vec<_>>()
        .join("\n");

    let mut embed = embeds::rich(
        "Library overview",
        Some("Series coverage, source mix, and the most recently updated library entries."),
        embeds::COLOR_INFO,
    );
    let series_count = format!("{} series", library.len());
    let chapter_count = format!("{downloaded_chapters}/{total_chapters} downloaded");
    embed.fields = vec![
        embeds::field("Series", &series_count, true),
        embeds::field("Chapters", &chapter_count, true),
        embeds::field("Categories", &categories, false),
        embeds::field("Sources", &sources, false),
        embeds::field("Recently updated", &recently_updated, false),
    ];
    embed.thumbnail = library
        .iter()
        .find_map(|manga| embeds::thumbnail(&manga.cover_url));

    Ok(embed_response(embed, active_component))
}

#[autometrics]
async fn downloads_command<S>(state: &Arc<S>, active_component: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let downloads = state.db().get_downloads().await?;
    if downloads.is_empty() {
        return Ok(message_response(
            "No downloads yet",
            "Queued and completed chapter downloads will appear here.",
            embeds::COLOR_WARNING,
        ));
    }

    let status_summary = format_counts(
        downloads.iter().map(|download| download.status.as_str()),
        MAX_COMMAND_ITEMS,
    );
    let active_count = downloads
        .iter()
        .filter(|download| is_active_download_status(&download.status))
        .count();
    let queued_count = downloads
        .iter()
        .filter(|download| download.status == "queued")
        .count();
    let error_count = downloads
        .iter()
        .filter(|download| download.status == "error")
        .count();
    let visible_downloads = downloads
        .iter()
        .filter(|download| download.status != "completed")
        .take(MAX_COMMAND_ITEMS)
        .collect::<Vec<_>>();
    let visible_downloads = if visible_downloads.is_empty() {
        downloads.iter().take(MAX_COMMAND_ITEMS).collect::<Vec<_>>()
    } else {
        visible_downloads
    };
    let queue = visible_downloads
        .iter()
        .map(|download| format_download_item(download))
        .collect::<Vec<_>>()
        .join("\n");

    let mut embed = embeds::rich(
        "Download activity",
        Some("Queue depth, active work, and the latest non-completed download records."),
        if error_count > 0 {
            embeds::COLOR_WARNING
        } else {
            embeds::COLOR_INFO
        },
    );
    let active_count = active_count.to_string();
    let queued_count = queued_count.to_string();
    let error_count = error_count.to_string();
    embed.fields = vec![
        embeds::field("Active", &active_count, true),
        embeds::field("Queued", &queued_count, true),
        embeds::field("Errors", &error_count, true),
        embeds::field("By status", &status_summary, false),
        embeds::field("Latest work", &queue, false),
    ];
    Ok(embed_response(embed, active_component))
}

#[autometrics]
async fn sources_command<S>(state: &Arc<S>, active_component: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let mut sources = {
        let plugins = state.plugin_manager().read().await;
        plugins.sources()
    };
    if sources.is_empty() {
        return Ok(message_response(
            "No source plugins loaded",
            "Install a source plugin so manga-server can search and refresh manga.",
            embeds::COLOR_WARNING,
        ));
    }

    sources.sort_by(|left, right| left.display_name.cmp(&right.display_name));

    let enabled_count = sources.iter().filter(|source| source.enabled).count();
    let search_count = sources
        .iter()
        .filter(|source| {
            source
                .capabilities
                .iter()
                .any(|capability| capability == "search")
        })
        .count();
    let source_list = sources
        .iter()
        .take(MAX_COMMAND_SOURCES)
        .map(format_source_item)
        .collect::<Vec<_>>()
        .join("\n");
    let mut embed = embeds::rich(
        "Source plugins",
        Some("Loaded plugin health, enabled state, and exposed source capabilities."),
        embeds::COLOR_INFO,
    );
    let loaded_count = sources.len().to_string();
    let enabled_count = format!("{enabled_count}/{}", sources.len());
    let search_count = search_count.to_string();
    embed.fields = vec![
        embeds::field("Loaded", &loaded_count, true),
        embeds::field("Enabled", &enabled_count, true),
        embeds::field("Search capable", &search_count, true),
        embeds::field("Sources", &source_list, false),
    ];
    if sources.len() > MAX_COMMAND_SOURCES {
        embed.footer = Some(embeds::footer(&format!(
            "...and {} more.",
            sources.len() - MAX_COMMAND_SOURCES
        )));
    }

    Ok(embed_response(embed, active_component))
}

#[autometrics]
async fn updates_command<S>(state: &Arc<S>, active_component: &str) -> Result<InteractionResponse>
where
    S: DiscordServerState,
{
    let updates = state.db().get_library_updates().await?;
    let library_by_id = state
        .db()
        .get_library_manga()
        .await?
        .into_iter()
        .map(|manga| {
            (
                manga.id,
                CommandMangaSummary {
                    title: manga.title,
                    cover_url: manga.cover_url,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    if updates.is_empty() {
        return Ok(message_response(
            "No recent updates",
            "No library updates are currently tracked.",
            embeds::COLOR_WARNING,
        ));
    }

    let lines = format_update_sections(updates.iter().take(MAX_COMMAND_UPDATES).map(|chapter| {
        format_command_update(
            chapter,
            library_by_id
                .get(&chapter.manga_id)
                .map(|manga| manga.title.as_str()),
        )
    }));
    let suffix = if updates.len() > MAX_COMMAND_UPDATES {
        format!("\n...and {} more.", updates.len() - MAX_COMMAND_UPDATES)
    } else {
        String::new()
    };

    let mut embed = embeds::rich(
        "Recent library updates",
        Some(&lines),
        embeds::COLOR_SUCCESS,
    );
    if !suffix.is_empty() {
        embed.footer = Some(embeds::footer(suffix.trim()));
    }
    if let Some(manga) = updates
        .iter()
        .find_map(|chapter| library_by_id.get(&chapter.manga_id))
    {
        embed.thumbnail = embeds::thumbnail(&manga.cover_url);
    }

    Ok(embed_response(embed, active_component))
}

fn refresh_command<S>(state: &Arc<S>, active_component: &str) -> InteractionResponse
where
    S: DiscordServerState,
{
    let state = Arc::clone(state);
    tokio::spawn(async move {
        match state.check_for_updates("discord_command").await {
            Ok(new_chapters) => {
                tracing::info!(
                    trigger = "discord_command",
                    new_chapters,
                    "Discord Library Refresh Completed",
                );
            }
            Err(error) => {
                tracing::error!(
                    trigger = "discord_command",
                    error = %error,
                    "Discord Library Refresh Failed",
                );
            }
        }
    });

    embed_response(
        embeds::rich(
            "Library refresh started",
            Some("New chapter notifications will post if updates are found."),
            embeds::COLOR_SUCCESS,
        ),
        active_component,
    )
}

fn help_command(active_component: &str) -> InteractionResponse {
    let commands = [
        format!(
            "`/{}` - Open manga-server controls or search the library",
            MangaCommand::NAME
        ),
        format!(
            "`/{}` - Show or update manga-server settings",
            SettingsCommand::NAME
        ),
    ]
    .join("\n");
    let mut embed = embeds::rich(
        "Manga server commands",
        Some("Slash commands available from this Discord bot."),
        embeds::COLOR_INFO,
    );
    embed.fields = vec![embeds::field("Commands", &commands, false)];

    embed_response(embed, active_component)
}
