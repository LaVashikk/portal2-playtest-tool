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
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/submit", post(handle_submission))
        .route("/upload", post(upload_file))
        .route("/data/:id", get(serve_data))
        .layer(DefaultBodyLimit::max(120 * 1024 * 1024))
        .with_state(state)
}

async fn handle_submission(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<FormSubmission>,
) -> StatusCode {
    // Validate the moderator key
    let key = match headers.get("X-Moderator-Key").and_then(|h| h.to_str().ok()) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    let destination = match state.key_store.get(key) {
        Some(data) => data.clone(),
        None => return StatusCode::FORBIDDEN,
    };

    // Persist the data
    let filename = format!(
        "{}_{}.json",
        PathBuf::from(&payload.survey_id).file_stem().and_then(|s| s.to_str()).unwrap_or("form"),
        payload.submission_timestamp
    );

    // junky shit
    let answer_dir = state.file_manager.base_dir.join("ANSWERS").join(key).join(&payload.user_xuid);
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

    // Register the JSON itself in the File Manager
    let submission_id = Uuid::new_v4();
    state.file_manager.commit_file(submission_id, key, &filename, true, file_path, json_bytes.len() as u64);

    // Process attached files
    let mut attached_files = Vec::new();
    for (file_id_str, _) in &payload.files {
        if let Ok(file_uuid) = Uuid::parse_str(file_id_str) {
            let original_name = state.file_manager.temp_file_names
                .get(&file_uuid)
                .map(|r| r.value().clone())
                .unwrap_or_else(|| "unknown.bin".to_string());

            // TODO: Implement priority logic later
            let is_priority = key == "17a72e0f-fd9a-45e9-a7f2-c060c743ff8e";

            match state.file_manager.commit_temp_file(file_uuid, key, &payload.user_xuid, &original_name, is_priority) {
                Ok(meta) => attached_files.push(meta),
                Err(e) => warn!("Failed to commit file {}: {}", file_uuid, e),
            }
        }
    }

    state.file_manager.save_to_disk();

    // Fire the event
    let event = SubmissionEvent {
        submission_id,
        destination,
        submission: payload,
        submission_bytes: json_bytes,
        filename,
        attached_files,
    };

    // If no one is listening, it's not an error.
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

    // TODO: check body size!!!!!

    let original_name = headers.get("X-File-Name")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown.bin")
        .to_string();


    let temp_id = Uuid::new_v4();
    let temp_path = state.file_manager.base_dir.join("TEMP_UPLOADS").join(temp_id.to_string());
    // save temp_id -> original_name mapping
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
    (StatusCode::NOT_IMPLEMENTED, "TODO.").into_response()
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
