use crate::models::{ModeratorKeyData, SubmissionEvent};
use crate::state::ServerState;
use serenity::all::{Colour, Command, CommandDataOptionValue, CommandOptionType, CreateCommand, CreateCommandOption, CreateMessage, Interaction, Permissions, ResolvedOption, ResolvedValue};
use serenity::async_trait;
use serenity::builder::{CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::http::Http;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use zip::write::FileOptions;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::{HashMap, HashSet};
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
            CreateCommand::new("export_data")
                .description("Exports all survey data and files for this channel into a ZIP archive."),
            CreateCommand::new("stats")
                .description("Analyzes numerical answers for a specific survey.")
                .add_option(CreateCommandOption::new(
                    CommandOptionType::String,
                    "survey_id",
                    "The survey ID to analyze (default: default.json)"
                ).required(false))
                .add_option(CreateCommandOption::new(
                    CommandOptionType::String,
                    "group_by",
                    "Group the statistics by a specific field"
                ).add_string_choice("Map", "map").add_string_choice("User", "user").required(false))
                .add_option(CreateCommandOption::new(
                    CommandOptionType::String,
                    "map_name",
                    "Filter stats by a specific map (e.g., maps/pcap_a1_04.bsp)"
                ).required(false))
                .add_option(CreateCommandOption::new(
                    CommandOptionType::String,
                    "user_xuid",
                    "Filter stats by a specific user's XUID"
                ).required(false)),
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
        is_priority: false,
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
    // We acknowledge the command immediately because zipping takes time
    let defer_builder = CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new().ephemeral(false));
    command.create_response(&ctx.http, defer_builder).await?;

    let state = {
        let data = ctx.data.read().await;
        data.get::<ServerState>().cloned().expect("ServerState not found in TypeMap")
    };

    // Find the mod_key associated with this channel
    let mut target_key = None;
    for entry in state.key_store.iter() {
        if entry.value().channel_id == command.channel_id.to_string() {
            target_key = Some(entry.key().clone());
            break;
        }
    }

    let mod_key = match target_key {
        Some(k) => k,
        None => {
            command.edit_response(&ctx.http, serenity::builder::EditInteractionResponse::new().content("❌ No moderator key is bound to this channel.")).await?;
            return Ok(());
        }
    };

    let base_dir = state.file_manager.base_dir.clone();
    let export_dir = base_dir.join("EXPORTS");
    
    let export_id = Uuid::new_v4();
    let zip_filename = format!("{}.zip", export_id);
    let zip_path = export_dir.join(&zip_filename);
    let answers_dir = base_dir.join("ANSWERS").join(&mod_key);

    if !answers_dir.exists() {
        command.edit_response(&ctx.http, serenity::builder::EditInteractionResponse::new().content("❌ No data has been collected for this key yet.")).await?;
        return Ok(());
    }

    // Move heavy zip operation to a blocking thread to avoid freezing the async runtime
    let zip_result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        let walker = walkdir::WalkDir::new(&answers_dir);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = path.strip_prefix(&answers_dir).unwrap().to_str().unwrap();

            if path.is_file() {
                zip.start_file(name, options).map_err(|e| e.to_string())?;
                let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
            } else if !name.is_empty() {
                zip.add_directory(name, options).map_err(|e| e.to_string())?;
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    }).await.unwrap_or_else(|e| Err(format!("Task panicked: {}", e)));

    match zip_result {
        Ok(_) => {
            let base_url = std::env::var("BASE_URL").expect("Expected BASE_URL in the environment");
            // Since EXPORTS is served directly or via a special endpoint, we just construct the URL
            let download_url = format!("{}/exports/{}", base_url, zip_filename);

            let embed = CreateEmbed::new()
                .title("📦 Data Export Complete")
                .color(0x00FF00)
                .description("Your data has been successfully packaged.")
                .field("Download Link", format!("[Click here to download ZIP]({})", download_url), false)
                .footer(serenity::builder::CreateEmbedFooter::new("This link will expire in 14 days."));

            command.edit_response(&ctx.http, serenity::builder::EditInteractionResponse::new().add_embed(embed)).await?;
        }
        Err(e) => {
            command.edit_response(&ctx.http, serenity::builder::EditInteractionResponse::new().content(format!("❌ Failed to create zip: {}", e))).await?;
        }
    }
    Ok(())
}

async fn handle_stats(ctx: &Context, command: &serenity::all::CommandInteraction) -> Result<(), serenity::Error> {
    let state = {
        let data = ctx.data.read().await;
        data.get::<ServerState>().cloned().expect("ServerState not found in TypeMap")
    };

    let mut survey_id = "default.json".to_string();
    let mut target_map = None;
    let mut target_user = None;
    let mut group_by = None;

    for opt in &command.data.options {
        match opt.name.as_str() {
            "survey_id" => {
                if let serenity::all::CommandDataOptionValue::String(s) = &opt.value {
                    survey_id = s.to_string();
                }
            }
            "map_name" => {
                if let serenity::all::CommandDataOptionValue::String(s) = &opt.value {
                    target_map = Some(s.to_string());
                }
            }
            "user_xuid" => {
                if let serenity::all::CommandDataOptionValue::String(s) = &opt.value {
                    target_user = Some(s.to_string());
                }
            }
            "group_by" => {
                if let serenity::all::CommandDataOptionValue::String(s) = &opt.value {
                    group_by = Some(s.to_string());
                }
            }
            _ => {}
        }
    }

    let mut target_key = None;
    for entry in state.key_store.iter() {
        if entry.value().channel_id == command.channel_id.to_string() {
            target_key = Some(entry.key().clone());
            break;
        }
    }

    let mod_key = match target_key {
        Some(k) => k,
        None => {
            let builder = CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("❌ No moderator key is bound to this channel.").ephemeral(true));
            command.create_response(&ctx.http, builder).await?;
            return Ok(());
        }
    };

    let answers_dir = state.file_manager.base_dir.join("ANSWERS").join(&mod_key);
    let mut total_surveys = 0;

    // Question -> Group -> Values
    let mut num_stats: HashMap< String, HashMap<String, Vec<f64>> > = HashMap::new();
    let mut unique_maps = HashSet::new();
    let mut group_totals: HashMap<String, usize> = HashMap::new();

    // yeah, call me "the bullshit"
    if answers_dir.exists() {
        let walker = walkdir::WalkDir::new(&answers_dir);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if json["survey_id"].as_str() != Some(&survey_id) {
                            continue;
                        }

                        let map_str = json["map_name"].as_str().unwrap_or("unknown").to_string();
                        let user_str = json["user_name"].as_str().unwrap_or("unknown").to_string();

                        // Apply filters
                        if let Some(ref m) = target_map {
                            if &map_str != m { continue; }
                        }
                        if let Some(ref u) = target_user {
                            if &user_str != u { continue; }
                        }

                        total_surveys += 1;
                        unique_maps.insert(map_str.clone());

                        // collect stats by group key
                        let group_key = match group_by.as_deref() {
                            Some("map") => map_str,
                            Some("user") => user_str,
                            _ => "Overall".to_string(),
                        };

                        *group_totals.entry(group_key.clone()).or_insert(0) += 1;

                        if let Some(answers) = json["answers"].as_object() {
                            for (q, a) in answers {
                                if let Some(val_str) = a.as_str() {
                                    if let Ok(num) = val_str.parse::<f64>() {
                                        num_stats.entry(q.clone())
                                            .or_default()
                                            .entry(group_key.clone())
                                            .or_default()
                                            .push(num);                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if total_surveys == 0 {
        let builder = CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new().content(format!("No data found for `{}`.", survey_id))
            .ephemeral(true)
        );
        command.create_response(&ctx.http, builder).await?;
        return Ok(());
    }


    // Build cool embed decoration
    let mut description = format!("Analyzed **{}** submissions.", total_surveys);

    if let Some(ref g) = group_by {
        description.push_str(&format!("\n**Grouped By:** `{}`", g.to_uppercase()));
    }
    if let Some(ref m) = target_map {
        description.push_str(&format!("\n**Filtered by Map:** `{}`", m));
    }
    if let Some(ref u) = target_user {
        description.push_str(&format!("\n**Filtered by User:** `{}`", u));
    }

    let mut embed = CreateEmbed::new()
        .title(format!("📊 Statistics: {}", survey_id.split('/').last().unwrap_or(&survey_id)))
        .color(Colour::DARK_TEAL)
        .description(description);

    let mut added_fields = 0;
    for (q, groups) in &num_stats {
        let mut field_text = String::new();

        // todo: how to sort the order of groups?
        let mut sorted_groups: Vec<_> = groups.iter().collect();
        sorted_groups.sort_by(|a, b| a.0.cmp(b.0));

        let mut lines_added = 0;

        for (group_name, values) in sorted_groups {
            let total_in_group = group_totals.get(group_name).unwrap_or(&0);

            // Only show stats if more than 50% of the group has numerical data for this question
            if values.len() as f64 > (*total_in_group as f64 * 0.5) {
                let sum: f64 = values.iter().sum();
                let avg = sum / values.len() as f64;
                let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                let line = if group_name == "Overall" {
                    format!("**Avg:** {:.2} | Min: {} | Max: {}\n", avg, min, max)
                } else {
                    format!("🔹 `{}`: **{:.2}** ({} - {})\n", group_name, avg, min, max)
                };

                if field_text.len() + line.len() > 1000 {
                    field_text.push_str("...and more\n");
                    lines_added += 1;
                    break;
                }

                field_text.push_str(&line);
                lines_added += 1;
            }
        }

        if lines_added > 0 {
            embed = embed.field(q, field_text, false);
            added_fields += 1;
        }
    }

    if added_fields == 0 {
        embed = embed.field("Notice", "No numerical answers found to analyze. All answers seem to be text.", false);
    }

    let builder = CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().add_embed(embed));
    command.create_response(&ctx.http, builder).await?;
    Ok(())
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
