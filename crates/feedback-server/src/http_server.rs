use crate::models::{FormSubmission, SubmissionEvent};
use crate::state::ServerState;
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
use tracing::{error, info};

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/submit", post(handle_submission))
        .route("/data/:id", get(serve_data))
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
    let answer_dir = PathBuf::from("ANSWERS").join(&payload.user_xuid);
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

    let submission_id = Uuid::new_v4();
    state.data_index.insert(submission_id, file_path); // TODO: use vec<pathbuf> for multiple files! OR create a function (AddFile) what return link, and add link to embed!

    // Fire the event
    let event = SubmissionEvent {
        submission_id,
        destination,
        submission: payload,
        submission_bytes: json_bytes,
        filename
    };

    // If no one is listening, it's not an error.
    let _ = state.submission_sender.send(event);

    StatusCode::OK
}

#[debug_handler]
async fn serve_data(
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    request: Request<axum::body::Body>,
) -> Response {
    if let Some(path_ref) = state.data_index.get(&id) {
        let path = path_ref.value();
        let service = ServeFile::new(path);

        match service.oneshot(request).await {
            Ok(response) => {
                response.into_response()
            }
            Err(err) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serve file: {}", err))
                    .into_response()
            }
        }
    } else {
        (StatusCode::NOT_FOUND, "File not found.").into_response()
    }
}
