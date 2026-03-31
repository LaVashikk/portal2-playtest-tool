//! custom_windows: The crate that defines the UI of the overlay.
//!
//! This crate is responsible for defining the UI of the overlay. It contains the `Window` trait,
//! which every window must implement, and the `regist_windows` function, which assembles and
//! returns a collection of all active UI windows.
use std::sync::{LazyLock, Mutex, OnceLock};
use portal2_sdk::Engine;

// TODO: Temp workaround. fix later `portal2-sdk` crate!!!
pub static ENGINE: OnceLock<Engine> = OnceLock::new();

/// Base font scale factor
pub const BASE_TEXT_SCALE: f32 = 1.25;

/// Shared state accessible to all windows.
#[derive(Debug, Default, Clone)]
pub struct SharedState {
    pub is_overlay_focused: bool,
    pub surver_is_opened: bool,
}

/// Trait that every window must implement.
#[allow(dead_code)]
pub trait Window {
    /// The name of the window, used for the title.
    fn name(&self) -> &'static str;

    /// Shows or hides the window.
    fn toggle(&mut self);

    /// Returns whether the window is open.
    fn is_open(&self) -> bool;

    /// Determines if the window should be rendered in the current frame.
    /// This is checked before calling `draw()`.
    fn is_should_render(&self, _shared_state: &SharedState, _engine: &Engine) -> bool { true }

    /// The drawing logic of the window.
    fn draw(&mut self, ctx: &egui::Context, shared_state: &mut SharedState, engine: &Engine);

    /// Raw input signal processing, optional.
    /// # Returns
    /// * `true` - if the input should be passed to the game.
    /// * `false` - if the input should be "eaten" (blocked).
    fn on_raw_input(&mut self, _umsg: u32, _wparam: u16) -> bool { true }
}


/// Assembles and returns a collection of all active UI windows.
///
/// This function is the designated discovery point for UI components. The core
/// application calls it to populate the `UiManager`'s window list.
pub fn regist_windows(engine: &Engine) -> Vec<Box<dyn Window + Send>> {
    // regist_events(engine); // todo: remove this
    survey::init_survey();
    log::info!("UI components initialized.");

    let config = survey::GLOBAL_SURVEY_CONFIG.get().unwrap();
    let bug_report_config = config.bug_report_config.clone();

    vec![
        Box::new(OverlayText::default()),
        Box::new(toasts::ToastWindow::new()),
        Box::new(survey::SurveyWin::new()),
        Box::new(survey::BugReportWin::new(&bug_report_config)),
    ]
}

fn regist_events(engine: &Engine) {
    // https://developer.valvesoftware.com/wiki/Logic_eventlistener

    engine.game_event_manager().listen("player_say", |event| { // todo: change logic here
        let command = event.get_string("text", "").to_lowercase();
        match command.as_str() {
            // "!survey" => survey::open_survey(),
            // "!bug" => survey::open_bug_report(),
            "!record" => {recorder::RECORDER.get().unwrap().lock().unwrap().start_recording("test_recording.mp4", *recorder::RECORDING_FPS.get().unwrap());}, // TODO!
            "!stop" => {recorder::RECORDER.get().unwrap().lock().unwrap().stop_recording();}
            "!debug" => {
                // TODO: DEV-CODE. debug _FILES data:
                log::warn!("VIDEO_FILE: {:?}", survey::VIDEO_FILE.lock().unwrap());
                log::warn!("LOG_FILE: {:?}", survey::LOGS_FILE.lock().unwrap());
                log::warn!("DEMOS_FILE: {:?}", survey::DEMO_FILES.lock().unwrap());
            }
            _ => {}
        }
    });
}


// ---------------------- \\
//      YOUR WINDOWS      \\
// ---------------------- \\
mod survey;
pub mod toasts;

#[derive(Default)] // default text info
pub struct OverlayText;
impl Window for OverlayText {
    fn name(&self) -> &'static str { "Overlay Text" }
    fn toggle(&mut self) {}
    fn is_open(&self) -> bool { true }

    fn draw(&mut self, ctx: &egui::Context, shared_state: &mut SharedState, _engine: &Engine) {
        let screen_rect = ctx.screen_rect();
        let painter = ctx.debug_painter();

        if shared_state.is_overlay_focused {
            let text = "Focus Captured; Press F3 to toggle";
            let font_id = egui::FontId::proportional(24.0);
            let text_color = egui::Color32::WHITE;
            let shadow_color = egui::Color32::BLACK;
            let pos = egui::pos2(screen_rect.center().x, screen_rect.bottom() - 50.0);
            let anchor = egui::Align2::CENTER_BOTTOM;

            // Shadow
            painter.text(
                pos + egui::vec2(2.0, 2.0),
                anchor,
                text,
                font_id.clone(),
                shadow_color,
            );

            // Foreground text
            painter.text(pos, anchor, text, font_id, text_color);
        }
    }
}
