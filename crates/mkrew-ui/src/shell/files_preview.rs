//! The Files view's Markdown preview (SPEC §12A, ADR 0072): the page, text selection with copy,
//! and editing one block's Markdown in place.

use crate::*;
use mkrew_ui::files_view::{BlockEditOutcome, StaleEdit};

impl Shell {
    /// Preview ↔ Source of the active Markdown tab (the Preview / Source switch, Ctrl+Shift+V).
    /// Nothing for other files.
    pub(crate) fn toggle_file_preview(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab) = self.active_file_tab(project_id) else {
            return;
        };
        let editable = tab
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        if !(tab.is_markdown() && editable) {
            return;
        }
        tab.toggle_mode();
        self.build_preview(project_id, window, cx);
        // The find fields are not drawn in Preview: give the keyboard back.
        if (self.files.get(&project_id))
            .and_then(FilesView::tab)
            .is_some_and(FileTab::previewing)
            && (self.file_find_focus.is_focused(window)
                || self.file_replace_focus.is_focused(window))
        {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    /// A `.md` file of the Files view as the Docs preview: the page of the editor's text
    /// (unsaved edits included), selectable.
    pub(crate) fn render_md_preview(
        &self,
        project_id: Ulid,
        tab: &FileTab,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let shown = match tab.preview() {
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
        let select = Some(preview::Selecting(tab.preview_selection));
        preview::page(
            &self.file_page,
            tab.opening,
            shown,
            Some(self.file_block_wrap(project_id, tab, window, theme, cx)),
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
        let Some(tab) = self.active_file_tab(project_id) else {
            return;
        };
        match at {
            Some(at) => tab.preview_click(at, event.modifiers.shift, event.click_count),
            None => {
                tab.clear_preview_selection();
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
        if let Some(at) = self.file_page.hit(position)
            && let Some(tab) = self.active_file_tab(project_id)
        {
            tab.preview_drag(at);
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
            if (self.active_file_tab(project_id)).is_some_and(FileTab::clear_preview_selection) {
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        match editor::edit_key(&event.keystroke) {
            Some(EditKey::Copy) => {
                cx.stop_propagation();
                if let Some(text) = self
                    .active_file_tab(project_id)
                    .and_then(|tab| tab.preview_copy())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Some(EditKey::SelectAll) => {
                cx.stop_propagation();
                if let Some(tab) = self.active_file_tab(project_id) {
                    tab.preview_select_all();
                }
                cx.notify();
            }
            _ => {}
        }
    }

    /// Every top-level block gets ✎ on hover; the edited one is replaced by its field.
    fn file_block_wrap(
        &self,
        project_id: Ulid,
        tab: &FileTab,
        window: &Window,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> preview::Wrap {
        let this = cx.entity();
        let theme = theme.clone();
        let edit = tab
            .block_edit
            .as_ref()
            .map(|edit| (edit.block, edit.text.input().clone()));
        let focused = self.file_block_focus.is_focused(window);
        let focus = self.file_block_focus.clone();
        let (marked, target) = (self.ime_marked.clone(), self.typing_target.clone());
        Rc::new(move |index, _block, element| {
            if let Some((_, input)) = edit.as_ref().filter(|(block, _)| *block == index) {
                return block_edit_field(
                    project_id, &this, &focus, input, focused, &marked, &target, &theme,
                )
                .into_any_element();
            }
            let group = SharedString::from(format!("file-block-{index}"));
            let this = this.clone();
            div()
                .id(("file-block", index))
                .group(group.clone())
                .relative()
                .child(element)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .rounded(px(radius::SMALL))
                        .bg(rgb(theme.surface_raised))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                        .child(
                            icon_button(("file-block-edit", index), Icon::Pencil, &theme)
                                .tooltip(tooltip(&theme, "Edit this block's Markdown", ""))
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |shell, cx| {
                                        shell.open_block_edit(project_id, index, window, cx);
                                    });
                                }),
                        ),
                )
                .into_any_element()
        })
    }

    fn open_block_edit(
        &mut self,
        project_id: Ulid,
        block: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if (self.active_file_tab(project_id)).is_some_and(|tab| tab.open_block_edit(block)) {
            self.file_page.remeasure();
            window.focus(&self.file_block_focus, cx);
        }
        cx.notify();
    }

    fn close_block_edit(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let closed = (self.active_file_tab(project_id)).and_then(|tab| tab.block_edit.take());
        if closed.is_some() {
            self.file_page.remeasure();
            window.focus(&self.file_editor_focus, cx);
        }
        cx.notify();
    }

    fn apply_block_edit(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let applied = (self.active_file_tab(project_id)).map(FileTab::apply_block_edit);
        match applied.unwrap_or(Ok(())) {
            Ok(()) => {
                self.file_page.remeasure();
                window.focus(&self.file_editor_focus, cx);
                self.file_edited(project_id, window, cx);
                self.build_preview(project_id, window, cx);
            }
            Err(StaleEdit) => self.toast_error("The file changed; edit again", cx),
        }
        cx.notify();
    }

    fn click_block_edit(&mut self, project_id: Ulid, caret: Caret) {
        if let Some(edit) =
            (self.active_file_tab(project_id)).and_then(|tab| tab.block_edit.as_mut())
        {
            edit.text.click(caret.at, caret.extend, caret.clicks);
        }
    }

    /// Keys of the block edit field: Enter is a new line, Ctrl+Enter applies, Esc cancels;
    /// Ctrl+C / X / V use the clipboard.
    pub(crate) fn file_block_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(key), Some(project_id)) =
            (task_dialog::dialog_key(&event.keystroke), self.workspace)
        else {
            return;
        };
        let pasted = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(tab) = self.active_file_tab(project_id) else {
            return;
        };
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && let Some(text) = tab
                .block_edit
                .as_ref()
                .and_then(|edit| edit.text.input().selected_text().map(str::to_owned))
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if let (Some(text), Some(edit)) = (pasted, &mut tab.block_edit) {
            edit.text.insert(&text);
        }
        match tab.block_edit_key(key) {
            BlockEditOutcome::Edited => cx.notify(),
            BlockEditOutcome::Cancel => self.close_block_edit(project_id, window, cx),
            BlockEditOutcome::Apply => self.apply_block_edit(project_id, window, cx),
        }
    }
}

/// The edited block: its Markdown in a caret field, with how to apply / cancel under it.
/// Presses and drags inside stay here (the page would take the keyboard and move its own
/// selection); a press outside cancels.
#[allow(clippy::too_many_arguments)]
fn block_edit_field(
    project_id: Ulid,
    this: &Entity<Shell>,
    focus: &FocusHandle,
    input: &Input,
    focused: bool,
    marked: &Rc<ime::Composing<FocusHandle>>,
    target: &ime::FocusedField<Shell>,
    theme: &Theme,
) -> gpui::Div {
    let (keys, out, grab) = (this.clone(), this.clone(), focus.clone());
    let field = caret_field_for(
        this,
        "file-block-field",
        input,
        "Markdown…",
        focused,
        theme,
        move |shell, caret| shell.click_block_edit(project_id, caret),
    )
    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
    .overflow_y_scroll()
    .track_focus(focus)
    .on_key_down(move |event, window, cx| {
        keys.update(cx, |shell, cx| shell.file_block_key_down(event, window, cx));
    })
    .child(typing_input_for(
        this.clone(),
        focus,
        Shell::file_block_key_down,
        marked,
        target,
    ));
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&grab, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(move |_, window, cx| {
            out.update(cx, |shell, cx| {
                shell.close_block_edit(project_id, window, cx)
            });
        })
        .child(field)
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("Ctrl+Enter apply · Esc cancel"),
        )
}
