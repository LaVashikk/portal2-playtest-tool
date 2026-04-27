use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use crate::file_manager::FileMetadata;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FormSubmission {
    pub survey_id: String,
    pub user_name: String,
    pub user_xuid: String,
    pub map_name: String,
    pub game_timestamp: f32,
    pub submission_timestamp: u64,
    pub answers: IndexMap<String, String>,
    pub custom_embed_color: Option<i32>,
    pub files: Vec<(String, String)>, // (file_id, file_name)

    #[serde(flatten)]
    pub extra_data: IndexMap<String, serde_json::Value>,
}

// Data associated with a moderator key
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ModeratorKeyData {
    pub owner_id: String,       // User ID of the person who generated it
    pub guild_id: String,       // Server ID
    pub channel_id: String,     // Channel ID to send messages to
    pub server_name: String,    // For display purposes
    #[serde(default)]
    pub is_priority: bool,      // Whether this key has priority status for storage
}

// The event passed internally after a submission is successfully processed
#[derive(Debug, Clone)]
pub struct SubmissionEvent {
    pub submission_id: uuid::Uuid,
    pub destination: ModeratorKeyData,
    pub submission: FormSubmission,
    pub submission_bytes: Vec<u8>,
    pub filename: String,
    pub attached_files: Vec<FileMetadata>,
}
