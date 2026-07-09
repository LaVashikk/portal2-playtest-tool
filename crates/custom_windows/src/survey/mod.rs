use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use std::fs;
use std::path::PathBuf;

mod types;
mod survey;
mod save_files;
mod bug_report;
use overlay_types::events::OverlayEvent;
pub use save_files::*; // TODO: remove this. temp for debuggind purpose
pub use survey::SurveyWin;
pub use bug_report::BugReportWin;

const DEFAULT_SURVEY: &str = "default.json";
const SERVER_URL: &str = "https://lab.lavashik.dev/p2_survey/submit";
const SERVER_URL_FILE: &str = "https://lab.lavashik.dev/p2_survey/upload";
// Global, write-once container for the moderator key, loaded from config.json.
pub static GLOBAL_SURVEY_CONFIG: OnceLock<ClientConfig> = OnceLock::new();
// Global, thread-safe, mutable string to hold the current status of the network request.
pub static REQUEST_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::from("idle")));

const WATERMARK_TEXT: &str = concat!(
    "portal2-playtest-assistant (v",
    env!("CARGO_PKG_VERSION"),
    ") - Open-Source by laVashik"
);
const WATERMARK_FONT: egui::FontId = egui::FontId::proportional(24.0);
const WATERMARK_COLOR: egui::Color32 = egui::Color32::from_gray(200);
const WATERMARK_ALIGN: egui::Align2 = egui::Align2::LEFT_BOTTOM;

/// Helper struct to deserialize the client configuration.
#[derive(serde::Deserialize, Debug)]
pub struct ClientConfig {
    pub mod_key: String,

    pub bug_report_config: String,
    pub bug_report_icon: String,

    pub save_demos: bool,
    pub save_console_logs: bool,
    pub save_recordings: bool,

    pub recording_fps: i32,
    pub recording_frame_skip: u32,
    pub recording_resolution: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            mod_key: Default::default(),
            bug_report_config: "bug_report.json".to_string(),
            bug_report_icon: "❗".to_string(),
            save_demos: false,
            save_console_logs: false,
            save_recordings: false,
            recording_fps: 15,
            recording_frame_skip: 24,
            recording_resolution: 520,
        }
    }
}

/// Sets the global request status in a thread-safe manner.
fn set_request_status(s: impl Into<String>) {
    if let Ok(mut guard) = REQUEST_STATUS.lock() {
        *guard = s.into();
    }
}

/// Gets a copy of the global request status in a thread-safe manner.
pub fn get_request_status() -> String {
    REQUEST_STATUS.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn get_addon_dir() -> PathBuf {
    portal2_sdk::utils::get_dll_directory().unwrap_or_default()
}

pub fn get_survey_dir() -> PathBuf {
    portal2_sdk::utils::get_dll_directory().unwrap_or_default().join("survey")
}

pub const SURVEY_ANSWERS_RELATIVE: &str = "survey_answers";
pub fn get_answer_dir() -> PathBuf {
    let engine = portal2_sdk::get_engine();
    let game_dir: PathBuf = engine.engine_server().get_game_dir().into();
    game_dir.join(SURVEY_ANSWERS_RELATIVE)
}

pub fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())
        .unwrap_or_default()
        .as_secs()
}

#[allow(dead_code)]
extern "C" fn survey_callback(cmd: &portal2_sdk::CCommand) {
    let event = match cmd.arg(1).unwrap_or("") {
        "" | "0" => OverlayEvent::SetWindowState("Survey", false),
        "1" => OverlayEvent::Command(DEFAULT_SURVEY.to_string()),
        name => OverlayEvent::Command(name.to_string()),
    };

    // send survey name to Survey struct
    // only runtime has ownership of every window, so we cannot here do job
    overlay_types::events::push_event( event );
}

#[allow(dead_code)]
extern "C" fn bug_report_callback(_cmd: &portal2_sdk::CCommand) {
    overlay_types::events::push_event(
        OverlayEvent::SetWindowState("Bug Report", true)
    );
}

// INIT SURVEY SYSTEM
pub fn init_survey() -> bool {
    // Reads 'SURVEY/config.json' and initializes the global mod-key
    let path = get_survey_dir().join("config.json");
    let Some(config) = std::fs::read_to_string(path.clone())
        .ok()
        .and_then(|s| serde_json::from_str::<ClientConfig>(&s).ok())
    else {
        log::warn!(
            target: "toast",
            "Could not read or parse 'survey/config.json'. Failed init survey-assistant-plugin",
        );
        return false;
    };

    if config.mod_key.is_empty() {
        log::warn!(target: "toast", "Moderator key is empty. Surveys will be offline.");
    }

    let _ = bug_report::BUG_ICON.set(config.bug_report_icon.clone());

    // let's create all necessary directories
    let answers_dir = get_answer_dir();
    ["demos", "logs", "records"].iter().for_each(|subdir| {
        fs::create_dir_all(answers_dir.join(subdir))
            .unwrap_or_else(|e| log::error!("Failed to create '{}' folder: {}", subdir, e));
    });

    // Initialize recorder
    let ffmpeg_path = get_addon_dir().join(recorder::FFMPEG_PATH);
    recorder::Recorder::init(ffmpeg_path, config.recording_frame_skip as usize, config.recording_fps, config.recording_resolution);

    // Initialize file-saver logic
    save_files::init_saver(&config);

    // Regist CVar-command
    portal2_sdk::ConCommand::register_new(
        "survey_open_ui",
        "Controls the survey UI. Usage: survey_open_ui <0 to close | 1 for default | config_path>",
        portal2_sdk::CvarFlags::NONE,
        survey_callback
    ).unwrap();

    portal2_sdk::ConCommand::register_new(
        "survey_open_bug_report",
        "Open the bug report UI",
        portal2_sdk::CvarFlags::NONE,
        bug_report_callback
    ).unwrap();

    // This will only succeed on the first call.
    let _ = GLOBAL_SURVEY_CONFIG.set(config);
    set_request_status("idle");

    true
}

mod form_shared;
pub use form_shared::*;
