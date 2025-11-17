use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FormSubmission {
    pub survey_id: String,
    pub user_name: String,
    pub user_xuid: String,
    pub map_name: String,
    pub game_timestamp: f32,
    pub submission_timestamp: u64,
    pub answers: IndexMap<String, String>,

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
}

// The event we pass internally after a submission is successful
#[derive(Debug, Clone)]
pub struct SubmissionEvent {
    pub submission_id: uuid::Uuid,
    pub destination: ModeratorKeyData,
    pub submission: FormSubmission,
    pub submission_bytes: Vec<u8>,
    pub filename: String,
    // pub files: Vec<PathBuf>
}
