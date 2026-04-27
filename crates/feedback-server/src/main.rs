mod discord_bot;
mod http_server;
mod models;
mod state;
mod file_manager;

use crate::state::ServerState;
use serenity::prelude::*;
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app_state = ServerState::new();

    // Start background tasks for file management (cleanup, expiration, storage enforcement)
    let fm_clone = app_state.file_manager.clone();
    tokio::spawn(async move {
        fm_clone.run_background_tasks().await;
    });

    // --- Start Discord Bot ---
    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILDS | GatewayIntents::MESSAGE_CONTENT;

    let discord_state_clone = app_state.clone();
    let mut client = Client::builder(&discord_token, intents)
        .event_handler(discord_bot::BotHandler)
        .await
        .expect("Error creating Discord client");

    {
        // Inject server state into the Discord client's data map
        let mut data = client.data.write().await;
        data.insert::<ServerState>(discord_state_clone);
    }

    // Start the notification listener to forward submissions to Discord
    let http_arc = client.http.clone();
    let listener_state_clone = app_state.clone();
    tokio::spawn(async move {
        discord_bot::notification_listener(listener_state_clone, http_arc).await;
    });

    // Start the Discord bot client
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        if let Err(why) = client.start().await {
            tracing::error!("Discord client error: {:?}", why);
        }
    });

    // --- Start HTTP Server ---
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("{}:{}", host, port);

    let app = http_server::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}
