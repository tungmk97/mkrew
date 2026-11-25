//! Every `window.prompt` drawn inside the window in Mkrew's style (SPEC §14) instead of the OS
//! message box. `main` registers [`build`] once with `App::set_prompt_builder`; the call sites
//! keep calling `window.prompt` and reading the answer index.

use crate::*;
use gpui::{
    EventEmitter, Focusable, PromptButton, PromptHandle, PromptResponse, RenderablePromptHandle,
};
use mkrew_ui::confirm;

/// Element ids of the buttons, by answer index.
// ponytail: fixed ids because `button()` takes `&'static str`; no prompt has more than 3
// answers today. Raise the count if one ever needs more (extra answers are not drawn).
const BUTTON_IDS: [&str; 4] = ["confirm-0", "confirm-1", "confirm-2", "confirm-3"];

pub(crate) struct ConfirmPrompt {
    level: PromptLevel,
    message: SharedString,
    detail: Option<SharedString>,
    buttons: Vec<PromptButton>,
    /// Answer indices left to right ([`confirm::visual_order`]).
    order: Vec<usize>,
    /// The answer Enter picks; it shows the keyboard ring.
    selected: usize,
    focus: FocusHandle,
}

/// The `App::set_prompt_builder` hook.
pub(crate) fn build(
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let buttons: Vec<PromptButton> = actions.iter().take(BUTTON_IDS.len()).cloned().collect();
    let order = confirm::visual_order(&buttons);
    let view = cx.new(|cx| ConfirmPrompt {
        level,
        message: message.to_owned().into(),
        detail: detail.map(|detail| detail.to_owned().into()),
        buttons,
        order,
        selected: 0,
        focus: cx.focus_handle(),
    });
    handle.with_view(view, window, cx)
}

impl EventEmitter<PromptResponse> for ConfirmPrompt {}

impl Focusable for ConfirmPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ConfirmPrompt {
    /// Only the keys the dialog uses are handled; the rest stay unhandled, so the OS still gets
    /// Alt+F4 / Alt+Space. Nothing else sees them: the prompt is the whole dispatch path.
    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<'_, Self>) {
        let cancel = confirm::cancel_index(&self.buttons);
        let key = confirm::key(
            &event.keystroke,
            event.is_held,
            &self.order,
            self.selected,
            cancel,
        );
        match key {
            confirm::Key::Answer(ix) => {
                cx.stop_propagation();
                cx.emit(PromptResponse(ix));
            }
            confirm::Key::Select(ix) => {
                cx.stop_propagation();
                self.selected = ix;
                cx.notify();
            }
            confirm::Key::Ignore => {}
        }
    }
}

impl Render for ConfirmPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let critical = self.level == PromptLevel::Critical;
        let primary_key = (self.selected == 0 && !critical).then(|| "↵".to_owned());
        let buttons: Vec<gpui::AnyElement> = self
            .order
            .iter()
            .map(|&ix| {
                // `order` only holds indices of `buttons`.
                let answer = &self.buttons[ix];
                let id = BUTTON_IDS[ix];
                let label = answer.label().clone();
                let secondary = ix != 0 && !answer.is_cancel();
                let filled = ix == 0 && !critical;
                let frame = if ix == 0 && critical {
                    danger_button(id, label, &theme)
                } else if ix == 0 {
                    primary_button(id, label, primary_key.clone(), &theme)
                } else if secondary {
                    secondary_button(id, label, &theme)
                } else {
                    // No tooltip: gpui draws none while a prompt is open.
                    button(id, label, &theme, false)
                };
                // Every button keeps a 1 px border, so the keyboard ring never shifts the row:
                // the ring is the accent plus its halo, or `focus_ring` plus the halo on the accent fill.
                let frame = if ix == self.selected {
                    let ringed = frame.border(px(theme::BORDER_WIDTH));
                    if filled {
                        ringed
                            .border_color(rgb(theme.focus_ring))
                            .shadow(focus_halo(theme.accent))
                    } else {
                        ringed
                            .border_color(rgb(theme.accent))
                            .shadow(focus_halo(theme.accent))
                    }
                } else if secondary || (ix == 0 && critical) {
                    frame
                } else {
                    frame
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(gpui::transparent_black())
                };
                frame
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(ix))))
                    .into_any_element()
            })
            .collect();
        let body = self
            .detail
            .clone()
            .map(|detail| dialog_body().child(dialog_text(detail, &theme)));
        let panel = dialog_frame(
            floating_panel("confirm-dialog", &theme).w(px(theme::CONFIRM_DIALOG_WIDTH)),
            DialogHead {
                icon: if critical {
                    Icon::TriangleAlert
                } else {
                    Icon::CircleHelp
                },
                tone: if critical {
                    DialogTone::Danger
                } else {
                    DialogTone::Normal
                },
                title: self.message.clone(),
                sub: None,
            },
            None,
            body,
            Some(dialog_foot(&theme).children(buttons)),
            &theme,
        );
        // The shade blocks the window behind it; a click on it answers nothing (Esc or a
        // button does). The window stretches this view over its whole area.
        div()
            .id("confirm-shade")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(theme.scrim))
            .occlude()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(super::widgets::appear("confirm-panel", panel, false))
    }
}
