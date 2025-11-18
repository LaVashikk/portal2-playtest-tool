//! custom_windows: The crate that defines the UI of the overlay.
//!
//! This crate is responsible for defining the UI of the overlay. It contains the `Window` trait,
//! which every window must implement, and the `regist_windows` function, which assembles and
//! returns a collection of all active UI windows.
use source_sdk::Engine;

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
pub fn regist_windows() -> Vec<Box<dyn Window + Send>> {
    survey::init_survey();
    log::info!("UI components initialized.");

    vec![
        Box::new(toasts::ToastWindow::new()),
        Box::new(survey::SurveyWin::new()),
        Box::new(survey::BugReportWin::new("survey/bug_report.json")),
    ]
}


// ---------------------- \\
//      YOUR WINDOWS      \\
// ---------------------- \\
mod survey;
pub mod toasts;
