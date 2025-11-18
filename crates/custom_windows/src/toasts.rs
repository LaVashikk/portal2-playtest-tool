use crate::{SharedState, Window};
use egui::{Color32, RichText};
use egui_notify::{Anchor, Toasts};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// The global, thread-safe queue for toast messages.
/// The logger in `overlay_runtime` will write to this queue.
pub static TOAST_QUEUE: LazyLock<Mutex<VecDeque<(log::Level, String)>>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(32)));


pub struct ToastWindow {
    toasts: Toasts,
}

impl std::fmt::Debug for ToastWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToastWindow").finish_non_exhaustive()
    }
}

impl ToastWindow {
    pub fn new() -> Self {
        Self {
            toasts: Toasts::new()
                .with_anchor(Anchor::TopRight)
                .with_margin(egui::vec2(10.0, 10.0)),
        }
    }

    /// Checks the global queue for new log messages and adds them to the display.
    fn check_for_new_logs(&mut self) {
        if let Ok(mut queue) = TOAST_QUEUE.try_lock() {
            for (level, message) in queue.drain(..) {
                // Add a prefix for context, with a special one for debug messages.
                let formatted_message = if level >= log::Level::Debug {
                    format!("[DEBUG] {}", message)
                } else {
                    format!("[PLAYTEST_TOOL] {}", message)
                };

                const TOAST_FONT_SIZE: f32 = 18.0;
                let caption = RichText::new(formatted_message)
                    .size(TOAST_FONT_SIZE)
                    .color(Color32::WHITE);

                // Create the appropriate toast type.
                let toast = match level {
                    log::Level::Error => self.toasts.error(caption),
                    log::Level::Warn => self.toasts.warning(caption),
                    log::Level::Info => self.toasts.info(caption),
                    // Use the 'basic' type for debug/trace to avoid the success icon.
                    log::Level::Debug | log::Level::Trace => self.toasts.basic(caption),
                };

                // Set custom duration based on the log level.
                let duration = match level {
                    log::Level::Error | log::Level::Warn => Duration::from_secs(8),
                    log::Level::Info => Duration::from_secs(3),
                    log::Level::Debug | log::Level::Trace => Duration::from_millis(400),
                };

                toast.duration(Some(duration)).closable(true);
            }
        }
    }
}

impl Window for ToastWindow {
    fn name(&self) -> &'static str { "Toast Notifications" }

    fn toggle(&mut self) {}
    fn is_open(&self) -> bool { true }

    fn draw(&mut self, ctx: &egui::Context, _shared_state: &mut SharedState, _engine: &source_sdk::Engine) {
        self.check_for_new_logs();
        self.toasts.show(ctx);
    }
}
