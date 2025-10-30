mod discord_bot;
mod http_server;
mod models;
mod state;

use crate::state::ServerState;
use serenity::prelude::*;
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();
    let app_state = ServerState::new();

    // --- Start Discord Bot ---
    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILDS | GatewayIntents::MESSAGE_CONTENT;

    // We need to clone the context for the notification listener task
    let discord_state_clone = app_state.clone();
    let mut client = Client::builder(&discord_token, intents)
        .event_handler(discord_bot::BotHandler)
        .await
        .expect("Err creating client");

    { // add server state to discord client state
        let mut data = client.data.write().await;
        data.insert::<ServerState>(discord_state_clone);
    }

    // Clone the HTTP context to pass to the listener task
    let http_arc = client.http.clone();
    let listener_state_clone = app_state.clone();
    tokio::spawn(async move {
        discord_bot::notification_listener(listener_state_clone, http_arc).await;
    });

    // Start the bot in a separate task
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        if let Err(why) = client.start().await {
            println!("Client error: {:?}", why);
        }
    });

    // --- Start HTTP Server ---
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("{}:{}", host, port);

    let app = http_server::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("HTTP server listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}
