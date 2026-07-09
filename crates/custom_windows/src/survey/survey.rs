use overlay_types::{events::OverlayEvent, toasts};

use crate::{survey::DEFAULT_SURVEY, SharedState, Window};
use super::{FormAction, WidgetForm};

#[derive(Debug)]
pub struct SurveyWin {
    form: WidgetForm,
    is_opened: bool,
}

impl SurveyWin {
    pub fn new() -> Self {
        Self {
            form: WidgetForm::new(DEFAULT_SURVEY),
            is_opened: false,
        }
    }
}

impl Window for SurveyWin {
    fn name(&self) -> &'static str { "Survey" }

    fn on_event(&mut self, event: &overlay_types::events::OverlayEvent, _shared_state: &mut SharedState) {
        match event {
            OverlayEvent::Command(name) => {
                if self.form.load_form(name.as_str()).is_err() {
                    return
                }

                self.set_open(true);
                portal2_sdk::con_print!("Opening {}...\n", name);
            }

            _ => {}
        }
    }

    fn draw(
        &mut self,
        ctx: &egui::Context,
        _shared_state: &mut SharedState,
        engine: &portal2_sdk::Engine,
    ) {
        if self.form.draw_modal_window(ctx, engine, false) == FormAction::Submitted {
            match self.form.save_results(engine, None) {
                Ok(_) => {
                    // reset SURVEY!
                    self.set_open(false);
                    self.form.reset_state();

                    let client = engine.client();
                    if client.is_in_game() { client.client_cmd("unpause"); }
                }
                Err(e) => {
                    toasts::error("Failed to save survey to disk", 1500);
                    log::error!("Error saving survey to disk: {}", e);
                }
            }
        }
    }

    fn is_open(&self) -> bool { self.is_opened }

    fn set_open(&mut self, open: bool) {
        if self.is_opened == open {
            return
        }

        let client = portal2_sdk::get_engine().client();
        if open {
            client.client_cmd("pause");
        } else {
            client.client_cmd("unpause");
        }

        self.is_opened = open;
        crate::edit_shared_state(move |state| {
            state.is_overlay_focused = open;
            state.surver_is_opened = open;
        });
    }
}
