use crate::models::{ModeratorKeyData, SubmissionEvent};
use crate::state::ServerState;
use serenity::all::{Command, CreateAttachment, CreateCommand, CreateMessage, Interaction, Permissions, ResolvedOption, ResolvedValue};
use serenity::async_trait;
use serenity::builder::{CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::http::Http;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

pub struct BotHandler;

#[async_trait]
impl EventHandler for BotHandler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            if command.data.name == "generate_key" {
                if let Err(e) = handle_generate_key(ctx, &command).await {
                    error!("Failed to handle generate_key command: {}", e);
                }
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Discord bot {} is connected!", ready.user.name);

        if let Err(e) = Command::create_global_command(&ctx.http,
            CreateCommand::new("generate_key")
                .description("Generates a new moderator key for this server and channel.")
        ).await {
            error!("Failed to register slash command: {}", e);
        }
    }
}

async fn handle_generate_key(ctx: Context, command: &serenity::all::CommandInteraction) -> Result<(), serenity::Error> {
    let state = {
        let data = ctx.data.read().await;
        data.get::<ServerState>().cloned().expect("ServerState not found in TypeMap")
    };

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            let response_content = "This command can only be used in a server.";
            let builder = CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(response_content).ephemeral(true));
            command.create_response(&ctx.http, builder).await?;
            return Ok(());
        }
    };

    let new_key = Uuid::new_v4().to_string();
    let guild_name = guild_id.to_guild_cached(&ctx.cache).map(|g| g.name.clone()).unwrap_or_else(|| "Unknown Server".to_string());
    let channel_name = command.channel_id.name(&ctx).await.unwrap_or_else(|_| "this channel".to_string());

    // Check if the bot has these permissions in the channel
    let required_permissions = Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::EMBED_LINKS
        | Permissions::ATTACH_FILES;

    let has_permissions = command.app_permissions.map_or(false, |p| p.contains(required_permissions));
    if !has_permissions {
        let embed = CreateEmbed::new()
            .title("❌ Permission Error")
            .description(format!(
                "I'm missing the required permissions in the `#{}` channel. \
                The key could not be generated.",
                channel_name
            ))
            .field(
                "Required Permissions:",
                "- `View Channel`\n\
                - `Send Messages`\n\
                - `Embed Links`\n\
                - `Attach Files`",
                false
            )
            .color(0xFF0000);

        let response_message = CreateInteractionResponseMessage::new()
            .add_embed(embed)
            .ephemeral(true);

        let builder = CreateInteractionResponse::Message(response_message);
        command.create_response(&ctx.http, builder).await?;

        return Ok(());

    }


    let key_data = ModeratorKeyData {
        owner_id: command.user.id.to_string(),
        guild_id: guild_id.to_string(),
        channel_id: command.channel_id.to_string(),
        server_name: guild_name.clone(),
    };

    state.key_store.insert(new_key.clone(), key_data);
    if let Err(e) = state.save_state_to_disk() {
        error!("Failed to save state to disk: {}", e); // lol
    }


    let response_content = format!(
        "✅ **New key generated!**\n\
        This key is bound to this channel (`#{}` in `{}`).\n\n\
        Your new mod-key is:\n\
        ```\n{}\n```\n\
        Keep it safe! Add it to surver's global config.",
        channel_name,
        guild_name,
        new_key
    );

    let builder = CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(response_content).ephemeral(false));
    command.create_response(&ctx.http, builder).await?;

    Ok(())
}


// background listener
pub async fn notification_listener(state: ServerState, http: Arc<Http>) {
    let mut receiver = state.submission_sender.subscribe();
    info!("Notification listener started.");
    while let Ok(event) = receiver.recv().await {
        info!("Received event for guild {}", event.destination.guild_id);
        let submission = &event.submission;

        // --- METADATA ---
        // time in game:
        let game_seconds = submission.game_timestamp;
        let minutes = (game_seconds / 60.0).floor();
        let seconds = game_seconds % 60.0;
        let formatted_game_time = format!("{:.0} min {:.2} sec", minutes, seconds);
        // survey name
        let survey_filename = submission.survey_id.split('/').last().unwrap_or("Survey");
        // color
        let embed_color = if survey_filename == "bug_report.json" { 0xFFA500 } else { 0x00BFFF };
        // json* link
        let base_url = std::env::var("BASE_URL").expect("Expected BASE_URL in the environment");
        let data_url = format!("{}/data/{}", base_url, event.submission_id); // todo

        // --- CREATE EMBED ---
        let mut embed = CreateEmbed::new()
            .title(format!("New Submission: {}", survey_filename))
            .description(format!("From user **{}** (`{}`)", submission.user_name, submission.user_xuid))
            .color(embed_color);

        // section 1: Metadata
        embed = embed
            .field("Map", format!("`{}`", submission.map_name), false)
            .field("Game Timestamp", formatted_game_time, false);
        if !submission.extra_data.is_empty() {
            for (key, value) in &submission.extra_data {
                // Format the value nicely, removing quotes from strings.
                let value_str = format!("`{}`", value.to_string().trim_matches('"'));
                embed = embed.field(key, value_str, true); // Use inline fields to keep it compact
            }
        }

        // section 2: Survey Answers
        embed = embed.field("\u{200B}", "**Survey Answers**", false)
            .fields(submission.answers.iter().map(|(q, a)| (q.clone(), a.clone(), false)));

        // section 3: files (todo)
        embed = embed.field("\u{200B}", "**Files:**", false)
            .field("Raw json", data_url, false);

        let channel_id_u64 = match event.destination.channel_id.parse::<u64>() {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to parse channel_id '{}': {}", event.destination.channel_id, e);
                continue;
            }
        };
        let channel_id = serenity::model::id::ChannelId::new(channel_id_u64);

        let builder = CreateMessage::new().embed(embed);

        info!("Attempting to send message to channel {}", channel_id);
        if let Err(why) = channel_id.send_message(&http, builder).await {
            error!("Failed to send notification to channel {}: {:?}", channel_id, why);
        } else {
            info!("Successfully sent message to channel {}", channel_id);
        }
    }
}
