use crate::models::{ModeratorKeyData, SubmissionEvent};
use crate::state::ServerState;
use serenity::all::{Command, CommandDataOptionValue, CommandOptionType, CreateCommand, CreateCommandOption, CreateMessage, Interaction, Permissions, ResolvedOption, ResolvedValue};
use serenity::async_trait;
use serenity::builder::{CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::http::Http;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use zip::write::FileOptions;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct BotHandler;

#[async_trait]
impl EventHandler for BotHandler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match command.data.name.as_str() {
                "generate_key" => {
                    if let Err(e) = handle_generate_key(&ctx, &command).await {
                        error!("Failed to handle generate_key command: {}", e);
                    }
                }
                "export_data" => {
                    if let Err(e) = handle_export_data(&ctx, &command).await {
                        error!("Failed to handle export_data command: {}", e);
                    }
                }
                "stats" => {
                    if let Err(e) = handle_stats(&ctx, &command).await {
                        error!("Failed to handle stats command: {}", e);
                    }
                }
                _ => {}
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Discord bot {} is connected!", ready.user.name);

        let commands = vec![
            CreateCommand::new("generate_key")
                .description("Generates a new moderator key for this server and channel."),
            // CreateCommand::new("export_data")
            //     .description("Exports all survey data and files for this channel into a ZIP archive."),
            // CreateCommand::new("stats")
            //     .description("Analyzes numerical answers for a specific survey.")
            //     .add_option(CreateCommandOption::new(
            //         CommandOptionType::String,
            //         "survey_id",
            //         "The survey ID to analyze (default: survey/default.json)"
            //     ).required(false)),
        ];

        if let Err(e) = Command::set_global_commands(&ctx.http, commands).await {
            error!("Failed to register slash commands: {}", e);
        }
    }
}

async fn handle_generate_key(ctx: &Context, command: &serenity::all::CommandInteraction) -> Result<(), serenity::Error> {
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

    let channel_id_str = command.channel_id.to_string();

    let mut existing_key = None;
    for entry in state.key_store.iter() {
        if entry.value().channel_id == channel_id_str {
            existing_key = Some(entry.key().clone());
            break;
        }
    }

    // Already have a key for this channel
    if let Some(key) = existing_key {
        let response_content = format!(
            "⚠️ **You already have a key for this channel!**\n\n\
            Your mod-key:\n```\n{}\n```",
            key
        );
        let builder = CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(response_content).ephemeral(true));
        command.create_response(&ctx.http, builder).await?;
        return Ok(());
    }

    let new_key = Uuid::new_v4().to_string();
    let guild_name = guild_id.to_guild_cached(&ctx.cache).map(|g| g.name.clone()).unwrap_or_else(|| "Unknown Server".to_string());
    let channel_name = command.channel_id.name(&ctx).await.unwrap_or_else(|_| "this channel".to_string());

    // Check if the bot has these permissions in the channel
    let required_permissions = Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::EMBED_LINKS;

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
                - `Embed Links`",
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
        error!("Failed to save state to disk: {}", e); // lol.. what?
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

async fn handle_export_data(ctx: &Context, command: &serenity::all::CommandInteraction) -> Result<(), serenity::Error> {
    // TODO
    todo!()
}

async fn handle_stats(ctx: &Context, command: &serenity::all::CommandInteraction) -> Result<(), serenity::Error> {
    todo!()
}

pub async fn notification_listener(state: ServerState, http: Arc<Http>) {
    let mut receiver = state.submission_sender.subscribe();
    info!("Notification listener started.");

    while let Ok(event) = receiver.recv().await {
        info!("Received event for guild {}", event.destination.guild_id);
        let submission = &event.submission;
        let base_url = std::env::var("BASE_URL").expect("Expected BASE_URL in the environment");

        // --- METADATA ---
        // time in game:
        let game_seconds = submission.game_timestamp;
        let formatted_game_time = format!("{:.0} min {:.2} sec", (game_seconds / 60.0).floor(), game_seconds % 60.0);
        // survey name
        let survey_filename = submission.survey_id.split('/').last().unwrap_or("Survey");

        // color
        let embed_color = match submission.custom_embed_color {
            Some(color) => color as u32,
            None => 0x00BFFF
        };

        // --- CREATE EMBED ---
        let mut embed = CreateEmbed::new()
            .title(format!("New Submission: {}", survey_filename))
            .description(format!("From user **{}** (`{}`)", submission.user_name, submission.user_xuid))
            .color(embed_color)
            .field("Map", format!("`{}`", submission.map_name), true)
            .field("Game Timestamp", formatted_game_time, true);

        // section 1: Metadata
        if !submission.extra_data.is_empty() {
            for (key, value) in &submission.extra_data {
                // Format the value nicely, removing quotes from strings.
                let value_str = format!("`{}`", value.to_string().trim_matches('"'));
                embed = embed.field(key, value_str, true);
            }
        }

        // section 2: Survey Answers
        embed = embed.field("\u{200B}", "**Survey Answers**", false)
            .fields(submission.answers.iter().map(|(q, a)| (q.clone(), a.clone(), false)));

        // section 3: files
        let mut files_text = String::new();
        files_text.push_str(&format!("📄 [Raw JSON]({}/data/{})\n", base_url, event.submission_id));
        for file in &event.attached_files {
            files_text.push_str(&format!("📎 [{}]({}/data/{})\n", file.original_name, base_url, file.id));
        }

        embed = embed.field("**Files:**", &files_text, false);

        // --- SEND MESSAGE ---
        let channel_id_u64 = event.destination.channel_id.parse::<u64>().unwrap_or(0);
        let channel_id = serenity::model::id::ChannelId::new(channel_id_u64);
        let builder = CreateMessage::new().embed(embed);

        if let Err(why) = channel_id.send_message(&http, builder).await {
            warn!("Failed to send notification embed to channel {}: {:?}", channel_id, why);

            // If sending the embed fails (e.g., too large), send a fallback message.
            let fallback_embed = CreateEmbed::new()
                .title("📄 Submission Received (Manual View Required)")
                .color(0x99AAB5)
                .description(format!(
                    "The full submission for `{}` was received successfully, but it is too large to be displayed as a summary here.",
                    survey_filename
                ))
                .field(
                    "Submitted By",
                    format!("**{}** (`{}`)", submission.user_name, submission.user_xuid),
                    false
                )
                .field("Links", files_text, false);


            let fallback_builder = CreateMessage::new().embed(fallback_embed);
            if let Err(fallback_why) = channel_id.send_message(&http, fallback_builder).await {
                error!(
                    "Failed to send fallback notification to channel {}: {:?}",
                    channel_id, fallback_why
                );
            } else {
                info!("Successfully sent fallback message to channel {}", channel_id);
            }
        } else {
            info!("Successfully sent message to channel {}", channel_id);
        }
    }
}
