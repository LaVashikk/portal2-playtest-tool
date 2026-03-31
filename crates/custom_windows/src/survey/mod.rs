use std::io::Read;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use std::{fs, thread};
use std::path::PathBuf;

use anyhow::{Context, bail};
use indexmap::IndexMap;
use portal2_sdk::Engine;

mod types;
use types::*;

mod survey;
mod save_files;
mod bug_report;
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
        log::error!("Triggered new config");
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
    portal2_sdk::utils::get_dll_directory().unwrap_or_default().join("survey")
}
pub const SURVEY_ANSWERS_RELATIVE: &str = "survey_answers";

pub fn get_answer_dir() -> PathBuf {
    let engine = crate::ENGINE.get().unwrap();
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

pub fn init_survey() {
    // Reads 'SURVEY/config.json' and initializes the global mod-key
    let path = get_addon_dir().join("config.json");
    let config = std::fs::read_to_string(path.clone())
        .ok()
        .and_then(|s| serde_json::from_str::<ClientConfig>(&s).ok())
        .unwrap_or_else(|| {
            log::warn!(
                "Could not read or parse 'survey/config.json'. Surveys will be offline.",
            );
            ClientConfig::default()
        });

    // todo: check mod-key here

    let _ = bug_report::BUG_ICON.set(config.bug_report_icon.clone());

    // let's create all necessary directories
    let answers_dir = get_answer_dir();
    ["demos", "logs", "records"].iter().for_each(|subdir| {
        fs::create_dir_all(answers_dir.join(subdir))
            .unwrap_or_else(|e| log::error!("Failed to create '{}' folder: {}", subdir, e));
    });

    recorder::init_recorder_const(config.recording_frame_skip as usize, config.recording_fps, config.recording_resolution);
    save_files::init_saver(&config);
    // This will only succeed on the first call.
    let _ = GLOBAL_SURVEY_CONFIG.set(config);
    set_request_status("idle");
}

#[derive(Debug, PartialEq, Eq)]
pub enum FormAction {
    Submitted,
    Closed,
    None,
}

#[derive(Debug, Default)]
pub struct WidgetForm {
    config: FormConfig,
    state: Vec<WidgetState>,
    pub opened: bool,
    config_path: String,
    scroll_to_top: bool,
}

impl WidgetForm {
    pub fn new(config_path: &str) -> Self {
        let mut app = Self::default();
        app.load_form(config_path)
            .expect("Failed to load a default survey. This is a critical error.");
        app
    }

    /// Loads and initializes a FORM from a configuration file.
    /// This method resets all previous states.
    pub fn load_form(&mut self, config_path_str: &str) -> Result<(), String> {
        let mut relative_path = std::path::PathBuf::from(config_path_str);
        if relative_path.extension().is_none() {
            relative_path.set_extension("json");
        }

        let final_config_path_str = relative_path.to_string_lossy().into_owned();
        let config_path = get_addon_dir().join(&final_config_path_str);

        // Read the configuration file into a string
        let json_str = match fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!("Failed to read survey file '{}': {}", config_path.display(), e);
                log::error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        // Parse the JSON string into the FormConfig struct
        let config: FormConfig = match serde_json::from_str(&json_str) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to parse survey file '{}': {}", config_path.display(), e);
                log::error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        // Initialize the state based on the loaded config
        let state = Self::create_initial_state(&config.widgets);

        // Update the object's state
        self.config = config;
        self.state = state;
        self.config_path = final_config_path_str;
        self.opened = false; // Reset the flag so that the `if !self.opened` trigger works
        Ok(())
    }

    fn are_all_required_filled(&self) -> bool {
        self.config
            .widgets
            .iter()
            .zip(self.state.iter())
            .all(|(config, state)| !config.is_required() || state.is_answered())
    }

    fn create_initial_state(widgets: &[WidgetConfig]) -> Vec<WidgetState> {
        widgets.iter().map(|w_config| match w_config {
            WidgetConfig::OneToTen(_) => WidgetState::OneToTen(None),
            WidgetConfig::Essay(_) => WidgetState::Essay(String::new()),
            WidgetConfig::RadioChoices(_) => WidgetState::RadioChoices(None),
            WidgetConfig::Checkboxes(_) => WidgetState::Checkboxes(Vec::new()),
            WidgetConfig::TextBlock(_) => WidgetState::TextBlock,
            WidgetConfig::Header(_) => WidgetState::TextBlock,
            WidgetConfig::Separator => WidgetState::Separator,
        }).collect()
    }

    pub fn reset_state(&mut self) {
        self.state = Self::create_initial_state(&self.config.widgets);
        self.scroll_to_top = true;
    }

    fn send_file(mod_key: &str, file_path: &PathBuf) -> anyhow::Result<(String, String)> {
        let file = fs::File::open(file_path)?;
        let file_name = file_path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let len = file.metadata().context("Failed to read metadata")?.len();

        let upload_res = ureq::post(SERVER_URL_FILE)
            .header("X-Moderator-Key", mod_key)
            .header("X-File-Name", file_name)
            .header("Content-Length", &len.to_string())
            .send(ureq::SendBody::from_owned_reader(file.take(len)))
            .context("Failed to upload file")?;

        let upload_json: serde_json::Value = upload_res
            .into_body()
            .read_json()
            .context("Failed to parse response JSON")?;

        let file_id = upload_json["file_id"]
            .as_str()
            .context("No file_id in response")?;

        Ok((file_id.to_string(), file_name.to_string()))
    }

    /// Collects all data and saves it to a structured JSON file.
    /// The provided `base_data` HashMap is used as a base, and common information
    /// (user, answers, etc.) is added to it before serialization.
    pub fn save_results(
        &self,
        engine: &Engine,
        extra_data: Option<IndexMap<String, serde_json::Value>>,
    ) -> anyhow::Result<()> {
        // Collect common metadata
        let client = engine.client();
        let local_player_idx = client.get_local_player();
        let (user_name, user_xuid) = client
            .get_player_info(local_player_idx)
            .map(|info| (info.name().to_string(), info.xuid.to_string()))
            .unwrap_or_else(|| ("<unknown>".to_string(), "0".to_string()));
        let submission_timestamp = get_timestamp();

        // Format answers as "question: answer"
        let mut answers = IndexMap::new();
        for (config, state) in self.config.widgets.iter().zip(self.state.iter()) {
            if !matches!(config, WidgetConfig::TextBlock(_) | WidgetConfig::Header(_) | WidgetConfig::Separator) {
                answers.insert(config.text().to_string(), state.to_string());
            }
        }

        let custom_embed_color = self.config.embed_color.clone().map(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 3 {
                let r = parts[0].parse::<u8>().unwrap_or_default() as i32;
                let g = parts[1].parse::<u8>().unwrap_or_default() as i32;
                let b = parts[2].parse::<u8>().unwrap_or_default() as i32;
                (r << 16) + (g << 8) + b
            } else {
                0
            }
        });

        let map_name = client.get_level_name_short()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', ""); // Sanitize map name
        let map_name_string = client.get_level_name();
        let game_timestamp = client.get_last_time_stamp();

        let config_path = self.config_path.clone();
        let (survey_with_demo, survey_with_logs, survey_with_recording) = (
            self.config.send_with_demo, self.config.send_with_logs, self.config.send_with_recording
        );

        // hook command
        if let Some(hook_cmd) = &self.config.post_hook_command {
            client.execute_client_cmd_unrestricted(hook_cmd);
        }

        // Peace of shit, but it's... works.. kinda
        thread::spawn(move || {
            let config = GLOBAL_SURVEY_CONFIG.get()
                .expect("Unreachable: Global survey config not set.");

            // First, process files!
            let mut handles = Vec::with_capacity(3);
            let mut files = Vec::with_capacity(3);
            if !config.mod_key.is_empty() {
                if config.save_console_logs && survey_with_logs {
                    let handle = thread::spawn(|| {
                        if let Some(log_file) = save_files::LOGS_FILE.lock().unwrap().as_ref() {
                            return Self::send_file(&config.mod_key, log_file)
                        }
                        bail!("Does not have logs!")
                    });
                    handles.push(handle);
                }

                if config.save_demos && survey_with_demo {
                    save_files::stop_demo_recording();
                    let handle = thread::spawn(move || {
                        if let Ok(zip_file) = save_files::pack_demos() {
                            return Self::send_file(&config.mod_key, &zip_file)
                        }
                        bail!("Failed to pack demos!")
                    });
                    handles.push(handle);
                }

                if config.save_recordings && survey_with_recording {
                    save_files::stop_recording();

                    let handle = thread::spawn(|| {
                        if let Some(video_file) = save_files::VIDEO_FILE.lock().unwrap().take() {
                            return Self::send_file(&config.mod_key, &video_file)
                        }
                        bail!("Does not have video!")
                    });
                    handles.push(handle);
                }
            }

            for handle in handles {
                let file_uuid = handle.join().unwrap();
                match file_uuid {
                    Ok(uuid) => files.push(uuid),
                    Err(e) => log::error!("Failed to send file: {}", e),
                }
            }

            // Generate dynamic filename and path
            let config_stem = PathBuf::from(&config_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown_config")
                .to_string();

            // Create the final structure
            let submission = FormSubmission {
                survey_id: config_path,
                user_name,
                user_xuid,
                map_name: map_name_string,
                game_timestamp,
                submission_timestamp,
                answers,
                custom_embed_color,
                files,

                extra_data: extra_data.unwrap_or_default(),
            };

            let filename = format!(
                "{}_{}_{}.json",
                config_stem, map_name, submission_timestamp
            );

            let output_path = get_answer_dir().join(filename);

            // Serialize the final combined map and save
            let json_data = serde_json::to_string_pretty(&submission).map_err(|e| e.to_string()).unwrap();
            fs::write(output_path, &json_data).map_err(|e| e.to_string()).unwrap();

            // Send to server
            let body_for_thread = json_data;
            if config.mod_key.is_empty() {
                log::warn!("Failed to send survey to server: The mod-key is not configured.");
                return;
            }

            set_request_status("sending...");
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into();

            let result = agent.post(SERVER_URL)
                .header("Content-Type", "application/json")
                .header("X-Moderator-Key", &config.mod_key)
                .send(&body_for_thread);

            match result {
                Ok(response) => {
                    let code = response.status().as_u16();
                    if response.status().is_success() {
                        set_request_status("sent successfully");
                        log::info!("Survey submitted successfully!");
                    } else {
                        set_request_status(format!("error: HTTP {}", code));
                        let response_text = response.into_body().read_to_string().unwrap_or_default();

                        let user_message = match code {
                            401 | 403 => "Survey submission failed: Invalid Moderator Key.".to_string(),
                            502 => "Survey submission failed: The server is temporarily unavailable (Bad Gateway).".to_string(),
                            500..=599 => format!("Survey submission failed: The server encountered an internal error (Code: {}).", code),
                            400..=499 => format!("Survey submission failed: There was a problem with the request (Code: {}). Please report this.", code),
                            _ => format!("Survey submission failed with an unexpected error (Code: {}). Please report this.", code),
                        };
                        log::error!("{}", user_message);
                        log::debug!(
                            "Survey submission failed with status code {}. Response: {}",
                            code,
                            response_text
                        );
                    }
                }
                Err(e) => { // This is for transport errors (network issues)
                    set_request_status("error: network issue".to_string());
                    log::error!("Survey submission failed: A network error occurred. Please check your connection.");
                    log::debug!("Survey submission transport error: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Renders the title of a widget, like the question text and a required `*` mark.
    fn draw_widget_header(ui: &mut egui::Ui, config: &WidgetConfig, state: &WidgetState) {
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.horizontal_wrapped(|ui| {
                let heading = egui::RichText::new(config.text()).strong();
                ui.label(heading);
                if config.is_required() && !state.is_answered() {
                    ui.colored_label(egui::Color32::RED, " *");
                }
            });
        });
        ui.add_space(5.0);
    }

    /// Renders the interactive part of a widget (the actual input controls).
    fn draw_widget_body(ui: &mut egui::Ui, config: &WidgetConfig, state: &mut WidgetState) {
        match (config, state) {
            (WidgetConfig::OneToTen(config), WidgetState::OneToTen(value)) => {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(&config.label_at_one);
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| { ui.label(&config.label_at_ten); },
                        );
                    });
                    ui.add_space(5.0);
                    ui.columns(10, |columns| {
                        for i in 0..10 {
                            let num = (i + 1) as u8;
                            columns[i].vertical_centered(|ui| {
                                ui.selectable_value(value, Some(num), (i + 1).to_string());
                            });
                        }
                    });
                });
            }
            (WidgetConfig::Essay(_), WidgetState::Essay(text)) => {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_width(f32::INFINITY)
                        .desired_rows(5),
                );
            }
            (WidgetConfig::RadioChoices(config), WidgetState::RadioChoices(selected)) => {
                ui.vertical(|ui| {
                    for choice in &config.choices {
                        ui.radio_value(selected, Some(choice.clone()), choice);
                    }
                });
            }
            (WidgetConfig::Checkboxes(config), WidgetState::Checkboxes(selected)) => {
                ui.vertical(|ui| {
                    for choice in &config.choices {
                        let mut is_selected = selected.contains(choice);
                        if ui.checkbox(&mut is_selected, choice.clone()).clicked() {
                            if is_selected {
                                selected.push(choice.clone());
                            } else {
                                selected.retain(|c| c != choice);
                            }
                        }
                    }
                });
            }
            // This function is only called for interactive widgets, so other arms are not needed.
            _ => {}
        }
    }

    /// The main rendering loop, now much cleaner and acting as a dispatcher.
    fn render_widgets(&mut self, ui: &mut egui::Ui) {
        for (widget_config, widget_state) in self.config.widgets.iter().zip(self.state.iter_mut()) {
            match widget_config {
                // Handle simple, visual-only widgets first.
                WidgetConfig::Separator => {
                    ui.add_space(100.0);
                }
                WidgetConfig::TextBlock(config) => {
                    ui.add_space(5.0);
                    egui::Frame::NONE
                        .fill(egui::Color32::from_gray(40))
                        .inner_margin(egui::Margin::same(10))
                        .corner_radius(egui::CornerRadius::same(4))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new(&config.text).strong());
                            });
                        });
                    ui.add_space(15.0);
                }
                WidgetConfig::Header(config) => {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(15, 10))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new(&config.text).heading());
                            });
                        });
                }
                // Handle all interactive widgets that have a header and a body.
                _ => {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(15, 0))
                        .show(ui, |ui| {
                            Self::draw_widget_header(ui, widget_config, widget_state);
                            Self::draw_widget_body(ui, widget_config, widget_state);
                        });
                }
            }
            ui.separator();
        }
    }

    pub fn draw_modal_window(
        &mut self,
        ctx: &egui::Context,
        _engine: &Engine,
        is_closable: bool,
    ) -> FormAction {
        let modal_id = egui::Id::new("widget_form_modal");
        let area = egui::Modal::default_area(modal_id)
            .default_size(ctx.screen_rect().size() * 0.7);
        let modal = egui::Modal::new(modal_id)
            .frame(egui::Frame::NONE)
            .area(area);

        let mut action = FormAction::None;

        ctx.input_mut(|i| {
            // Increase the scroll speed with the mouse wheel
            i.smooth_scroll_delta *= 10.0;
        });

        modal.show(ctx, |ui| {
            egui::Frame::window(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if is_closable {
                        // ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("❌").on_hover_text("Close").clicked() {
                            action = FormAction::Closed;
                        }
                    }
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(&self.config.title).heading().strong());
                    });

                });

                ui.add_space(10.0);
                ui.separator();

                let min_scroll = ctx.screen_rect().size().y * 0.7;
                let mut scroll_area = egui::ScrollArea::vertical()
                    .min_scrolled_height(min_scroll);

                if self.scroll_to_top {
                    scroll_area = scroll_area.vertical_scroll_offset(0.0);
                    self.scroll_to_top = false;
                }

                scroll_area.show(ui, |ui| {
                    self.render_widgets(ui);
                    ui.add_space(20.0);

                    ui.vertical_centered(|ui| {
                        let all_required_filled = self.are_all_required_filled();
                        let submit_button = egui::Button::new("Submit").min_size(egui::vec2(120.0, 30.0));
                        if ui.add_enabled(all_required_filled, submit_button).clicked() {
                            action = FormAction::Submitted;
                        }
                    });

                });
            });
        });

        action
    }
}
