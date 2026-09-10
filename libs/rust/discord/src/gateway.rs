use anyhow::Result;
use std::sync::Arc;
use tokio_graceful::ShutdownGuard;
use twilight_gateway::{
    CloseFrame, Config, Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};
use twilight_model::application::interaction::Interaction;

use crate::{DiscordBot, DiscordInteractionHandler};

const GATEWAY_EVENTS: EventTypeFlags =
    EventTypeFlags::INTERACTION_CREATE.union(EventTypeFlags::READY);

/// Runs the Discord gateway loop until shutdown is requested or the stream ends.
pub async fn run_gateway<H>(bot: DiscordBot, handler: Arc<H>, guard: ShutdownGuard) -> Result<()>
where
    H: DiscordInteractionHandler,
{
    let config = Config::new(bot.gateway_token(), Intents::GUILDS);
    let mut shard = Shard::with_config(ShardId::ONE, config);
    let mut closing = false;

    tracing::info!("Discord Gateway Starting");
    loop {
        tokio::select! {
            () = guard.shutdown_signal_triggered(), if !closing => {
                closing = true;
                shard.close(CloseFrame::NORMAL);
                tracing::info!("Discord Gateway Shutdown Initiated");
            }
            event = shard.next_event(GATEWAY_EVENTS) => {
                let Some(event) = event else {
                    tracing::warn!("Discord Gateway Stream Ended");
                    break;
                };
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!(error = %error, "Discord Gateway Event Error");
                        continue;
                    }
                };

                if matches!(event, Event::GatewayClose(_)) && closing {
                    tracing::info!("Discord Gateway Closed");
                    break;
                }

                handle_event(&handler, &bot, event).await;
            }
        }
    }

    Ok(())
}

async fn handle_event<H>(handler: &Arc<H>, bot: &DiscordBot, event: Event)
where
    H: DiscordInteractionHandler,
{
    match event {
        Event::Ready(ready) => {
            let guild_ids = ready
                .guilds
                .iter()
                .map(|guild| guild.id)
                .collect::<Vec<_>>();
            tracing::info!(
                user_id = %ready.user.id,
                username = %ready.user.name,
                guilds = guild_ids.len(),
                "Discord Gateway Ready",
            );
            if let Err(error) = bot.clear_guild_commands(guild_ids).await {
                tracing::warn!(error = %error, "Discord Guild Commands Cleanup Failed");
            }
        }
        Event::InteractionCreate(interaction) => {
            tracing::debug!(
                interaction_id = %interaction.id,
                kind = ?interaction.kind,
                "Discord Gateway Interaction Received",
            );
            respond_to_interaction(handler.as_ref(), bot, interaction.0).await;
        }
        Event::GatewayClose(frame) => {
            tracing::warn!(?frame, "Discord Gateway Closed Unexpectedly");
        }
        _ => {}
    }
}

async fn respond_to_interaction(
    handler: &impl DiscordInteractionHandler,
    bot: &DiscordBot,
    interaction: Interaction,
) {
    let response = match handler.handle_interaction(&interaction).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(
                error = %error,
                interaction_id = %interaction.id,
                "Discord Command Failed",
            );
            handler.error_response(&interaction)
        }
    };

    if let Err(error) = bot
        .create_interaction_response(&interaction, &response)
        .await
    {
        tracing::error!(
            error = %error,
            interaction_id = %interaction.id,
            "Discord Interaction Response Failed",
        );
    }
}
