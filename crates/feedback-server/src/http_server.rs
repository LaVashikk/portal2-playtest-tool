use crate::models::{FormSubmission, SubmissionEvent};
use crate::state::ServerState;
use crate::file_manager::{FileMetadata, FileStatus};
use axum::extract::DefaultBodyLimit;
use axum::{
    debug_handler,
    extract::{Json, Path, State},
    http::{Request, StatusCode, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::services::ServeFile;
use tower::ServiceExt;
use uuid::Uuid;
use std::fs;
use std::path::{Path as StdPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/submit", post(handle_submission))
        .route("/upload", post(upload_file))
        .route("/data/:id", get(serve_data))
        .route("/exports/:filename", get(serve_export))
        .route("/healthy", get(health_check))
        // Set maximum body limit to 120MB for file uploads
        .layer(DefaultBodyLimit::max(120 * 1024 * 1024))
        .with_state(state)
}

async fn handle_submission(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<FormSubmission>,
) -> StatusCode {
    // Validate the moderator key from headers
    let key = match headers.get("X-Moderator-Key").and_then(|h| h.to_str().ok()) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    let destination = match state.key_store.get(key) {
        Some(data) => data.clone(),
        None => return StatusCode::FORBIDDEN,
    };

    let is_priority = destination.is_priority;

    // Sanitize user_xuid to prevent path traversal attacks
    let safe_user_xuid = StdPath::new(&payload.user_xuid)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown_user");

    // Generate a safe filename for the submission JSON
    let filename = format!(
        "{}_{}.json",
        PathBuf::from(&payload.survey_id).file_stem().and_then(|s| s.to_str()).unwrap_or("form"),
        payload.submission_timestamp
    );

    // Ensure the destination directory exists
    let answer_dir = state.file_manager.base_dir.join("ANSWERS").join(key).join(safe_user_xuid);
    if let Err(e) = fs::create_dir_all(&answer_dir) {
        error!("Failed to create directory {:?}: {}", answer_dir, e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let json_bytes = match serde_json::to_string_pretty(&payload) {
        Ok(s) => s.into_bytes(),
        Err(e) => {
            error!("Failed to serialize payload to JSON: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR
        },
    };

    let file_path = answer_dir.join(&filename);
    if let Err(e) = fs::write(&file_path, &json_bytes) {
        error!("Failed to write to file {:?}: {}", file_path, e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Register the submission JSON in the file manager
    let submission_id = Uuid::new_v4();
    state.file_manager.commit_file(submission_id, key, &filename, true, file_path, json_bytes.len() as u64);

    // Process and commit attached files from temporary storage
    let mut attached_files = Vec::new();
    for (file_id_str, _) in &payload.files {
        if let Ok(file_uuid) = Uuid::parse_str(file_id_str) {
            let original_name = state.file_manager.temp_file_names
                .get(&file_uuid)
                .map(|r| r.value().clone())
                .unwrap_or_else(|| "unknown.bin".to_string());

            // Move the file from TEMP_UPLOADS to the final destination
            match state.file_manager.commit_temp_file(file_uuid, key, safe_user_xuid, &original_name, is_priority) {
                Ok(meta) => attached_files.push(meta),
                Err(e) => warn!("Failed to commit file {}: {}", file_uuid, e),
            }
        }
    }

    state.file_manager.save_to_disk();

    // Trigger internal submission event
    let event = SubmissionEvent {
        submission_id,
        destination,
        submission: payload,
        submission_bytes: json_bytes,
        filename,
        attached_files,
    };

    let _ = state.submission_sender.send(event);

    StatusCode::OK
}

async fn upload_file(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: axum::body::Bytes, // Raw binary file data
) -> Result<Json<serde_json::Value>, StatusCode> {
    let key = headers.get("X-Moderator-Key").and_then(|h| h.to_str().ok()).ok_or(StatusCode::UNAUTHORIZED)?;

    if !state.key_store.contains_key(key) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Extract and sanitize the original filename from headers to prevent path traversal
    let raw_name = headers.get("X-File-Name")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown.bin");

    let original_name = StdPath::new(raw_name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.bin")
        .to_string();

    let temp_id = Uuid::new_v4();
    let temp_path = state.file_manager.base_dir.join("TEMP_UPLOADS").join(temp_id.to_string());

    // Store temp_id -> original_name mapping for later use during submission
    state.file_manager.temp_file_names.insert(temp_id, original_name.clone());

    if let Err(e) = fs::write(&temp_path, body) {
        error!("Failed to write temp file {:?}: {}", temp_path, e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(serde_json::json!({ "file_id": temp_id.to_string() })))
}

#[debug_handler]
async fn serve_data(
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    request: Request<axum::body::Body>,
) -> Response {
    let file_meta = match state.file_manager.files.get(&id) {
        Some(meta) => meta.clone(),
        None => return (StatusCode::NOT_FOUND, "File not found.").into_response(),
    };

    match file_meta.status {
        FileStatus::Expired { deleted_at } => {
            let msg = format!("This file is expired and was automatically deleted (Deleted at timestamp: {}).", deleted_at);
            (StatusCode::GONE, msg).into_response()
        }
        FileStatus::Active => {
            let service = ServeFile::new(&file_meta.path);
            match service.oneshot(request).await {
                Ok(response) => response.into_response(),
                Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serve file: {}", err)).into_response(),
            }
        }
    }
}

#[debug_handler]
async fn serve_export(
    State(state): State<ServerState>,
    Path(filename): Path<String>,
    request: Request<axum::body::Body>,
) -> Response {
    let export_dir = state.file_manager.base_dir.join("EXPORTS");

    // Sanitize export filename to prevent path traversal
    let safe_filename = StdPath::new(&filename)
        .file_name()
        .and_then(|s| s.to_str());

    let file_path = match safe_filename {
        Some(name) => export_dir.join(name),
        None => return (StatusCode::BAD_REQUEST, "Invalid filename.").into_response(),
    };

    if !file_path.exists() {
        return (StatusCode::NOT_FOUND, "Archive not found.").into_response();
    }

    let service = ServeFile::new(&file_path);
    match service.oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serve archive: {}", err)).into_response(),
    }
}

async fn health_check(State(state): State<ServerState>) -> StatusCode {
    let mut total_active_bytes: u64 = 0;

    for entry in state.file_manager.files.iter() {
        if let FileStatus::Active = entry.value().status {
            total_active_bytes += entry.value().size_bytes;
        }
    }

    if total_active_bytes > state.file_manager.max_storage_bytes {
        StatusCode::INSUFFICIENT_STORAGE
    } else {
        StatusCode::OK
    }
}
