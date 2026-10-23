//! The Files view's Markdown preview (SPEC §12A, ADR 0072): the page, text selection with copy,
//! and editing one block's Markdown in place.

use crate::*;

impl Shell {
    /// A `.md` file of the Files view as the Docs preview: the page of the editor's text
    /// (unsaved edits included), selectable.
    pub(crate) fn render_md_preview(
        &self,
        project_id: Ulid,
        view: &FilesView,
        _window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let shown = match view.preview() {
            None => {
                return message(theme, None, "Rendering the preview…".into(), None)
                    .flex_1()
                    .into_any_element();
            }
            Some(shown) if shown.blocks.is_empty() => {
                return message(theme, None, "This file is empty.".into(), None)
                    .flex_1()
                    .into_any_element();
            }
            Some(shown) => shown,
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let select = Some(preview::Selecting(view.preview_selection));
        preview::page(
            &self.file_page,
            view.opening,
            shown,
            None,
            select,
            &fonts,
            theme,
        )
        .track_focus(&self.file_editor_focus)
        .on_key_down(cx.listener(Self::file_preview_key_down))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                shell.preview_press(project_id, event, window, cx);
            }),
        )
        .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
            if event.dragging() {
                shell.preview_drag_to(project_id, event.position, cx);
            }
        }))
        .into_any_element()
    }

    /// A press on the page: the keyboard comes here (Ctrl+C, Ctrl+A, Esc), the caret goes to
    /// the text under the pointer (Shift extends, a double click takes the word).
    fn preview_press(
        &mut self,
        project_id: Ulid,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.file_editor_focus, cx);
        let at = self.file_page.hit(event.position);
        let view = self.files_view(project_id);
        match at {
            Some(at) => view.preview_click(at, event.modifiers.shift, event.click_count),
            None => {
                view.clear_preview_selection();
            }
        }
        cx.notify();
    }

    fn preview_drag_to(
        &mut self,
        project_id: Ulid,
        position: Point<Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(at) = self.file_page.hit(position) {
            self.files_view(project_id).preview_drag(at);
            cx.notify();
        }
    }

    /// Keys of the preview: Ctrl+C copies the selection as shown, Ctrl+A selects the page, Esc
    /// clears it. Nothing edits here (the editor is hidden); other keys go on to the keymap.
    pub(crate) fn file_preview_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let plain = !event.keystroke.modifiers.modified();
        if plain && event.keystroke.key == "escape" {
            if self.files_view(project_id).clear_preview_selection() {
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        match editor::edit_key(&event.keystroke) {
            Some(EditKey::Copy) => {
                cx.stop_propagation();
                if let Some(text) = self.files_view(project_id).preview_copy() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Some(EditKey::SelectAll) => {
                cx.stop_propagation();
                self.files_view(project_id).preview_select_all();
                cx.notify();
            }
            _ => {}
        }
    }
}
