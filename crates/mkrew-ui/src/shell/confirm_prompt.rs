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
        let buttons: Vec<_> = self
            .order
            .iter()
            .map(|&ix| {
                // `order` only holds indices of `buttons`.
                let answer = &self.buttons[ix];
                let id = BUTTON_IDS[ix];
                let label = answer.label().clone();
                let secondary = ix != 0 && !answer.is_cancel();
                let frame = if ix == 0 && critical {
                    danger_button(id, label, &theme)
                } else if ix == 0 {
                    button(id, label, &theme, true)
                } else if secondary {
                    secondary_button(id, label, &theme)
                } else {
                    // No tooltip: gpui draws none while a prompt is open.
                    button(id, label, &theme, false)
                };
                // Every button keeps a 1 px border, so the keyboard ring never shifts the row.
                let ring = (ix == self.selected).then_some(if ix == 0 {
                    theme.text
                } else {
                    theme.accent
                });
                let frame = match ring {
                    Some(color) => frame
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(color)),
                    None if secondary => frame,
                    None => frame
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(gpui::transparent_black()),
                };
                frame
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(ix))))
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("confirm-dialog")
            .w(px(theme::CONFIRM_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(self.message.clone()),
            )
            .children(self.detail.clone().map(|detail| {
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text_muted))
                    .child(detail)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .children(buttons),
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
            .child(panel)
    }
}
