//! The Files view's Markdown preview (SPEC §12A, ADR 0072): the page, text selection with copy,
//! and editing one block's Markdown in place.

use crate::*;
use mkrew_ui::documents::{BlockEditOutcome, StaleEdit};

impl Shell {
    /// A `.md` file pane as the Docs preview: the page of the editor's text
    /// (unsaved edits included), selectable. `split`: beside the editor (Split view), with its
    /// own keyboard focus.
    pub(crate) fn render_md_preview(
        &self,
        project_id: Ulid,
        doc: &documents::Doc,
        split: bool,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(view) = self.doc_view(project_id, &doc.file) else {
            return div().into_any_element();
        };
        let path = doc.file.clone();
        let shown = match doc.preview() {
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
        let select = Some(preview::Selecting(doc.preview_selection));
        let wrap = Some(self.file_block_wrap(project_id, doc, window, theme, cx));
        let focus = if split {
            &view.preview_focus
        } else {
            &view.editor_focus
        };
        preview::page(
            &view.page,
            doc.opening,
            shown,
            wrap,
            select,
            preview::Flavor::Doc,
            &fonts,
            theme,
        )
        .track_focus(focus)
        .on_key_down(cx.listener(Self::file_preview_key_down))
        .on_mouse_down(MouseButton::Left, {
            let path = path.clone();
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                shell.preview_press(project_id, &path, split, event, window, cx);
            })
        })
        .on_mouse_move({
            cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
                if event.dragging() {
                    shell.preview_drag_to(project_id, &path, event.position, cx);
                }
            })
        })
        .into_any_element()
    }

    /// A press on the page: the keyboard comes here (Ctrl+C, Ctrl+A, Esc), the caret goes to
    /// the text under the pointer (Shift extends, a double click takes the word). In the Split
    /// view the editor's caret also goes to the pressed block's first line.
    fn preview_press(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        split: bool,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let which = if split {
            DocFocus::Preview
        } else {
            DocFocus::Editor
        };
        self.focus_doc(project_id, path, which, window, cx);
        let at = (self.doc_view(project_id, path)).and_then(|view| view.page.hit(event.position));
        if split && let Some(at) = at {
            self.split_caret_to_block(project_id, path, at.block, cx);
        }
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        match at {
            Some(at) => doc.preview_click(at, event.modifiers.shift, event.click_count),
            None => {
                doc.clear_preview_selection();
            }
        }
        cx.notify();
    }

    fn preview_drag_to(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        position: Point<Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(at) = (self.doc_view(project_id, path)).and_then(|view| view.page.hit(position))
            && let Some(doc) = self.doc_mut(project_id, path)
        {
            doc.preview_drag(at);
            cx.notify();
        }
    }

    /// Keys of the preview: Ctrl+C copies the selection as shown, Ctrl+A selects the page, Esc
    /// clears it. Nothing edits here (the editor is hidden); other keys go on to the keymap.
    pub(crate) fn file_preview_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        let plain = !event.keystroke.modifiers.modified();
        if plain && event.keystroke.key == "escape" {
            if (self.doc_mut(project_id, &path))
                .is_some_and(documents::Doc::clear_preview_selection)
            {
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        match editor::edit_key(&event.keystroke) {
            Some(EditKey::Copy) => {
                cx.stop_propagation();
                if let Some(text) =
                    (self.doc_mut(project_id, &path)).and_then(|doc| doc.preview_copy())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Some(EditKey::SelectAll) => {
                cx.stop_propagation();
                if let Some(doc) = self.doc_mut(project_id, &path) {
                    doc.preview_select_all();
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
        doc: &documents::Doc,
        window: &Window,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> preview::Wrap {
        let this = cx.entity();
        let theme = theme.clone();
        let path = doc.file.clone();
        let edit = doc
            .block_edit
            .as_ref()
            .map(|edit| (edit.block, edit.text.input().clone()));
        let block_focus =
            (self.doc_view(project_id, &doc.file)).map(|view| view.block_focus.clone());
        let Some(focus) = block_focus else {
            return Rc::new(|_, _, element| element);
        };
        let focused = focus.is_focused(window);
        let (marked, target) = (self.ime_marked.clone(), self.typing_target.clone());
        Rc::new(move |index, _block, element| {
            if let Some((_, input)) = edit.as_ref().filter(|(block, _)| *block == index) {
                return block_edit_field(
                    project_id, &path, &this, &focus, input, focused, &marked, &target, &theme,
                )
                .into_any_element();
            }
            let group = SharedString::from(format!("file-block-{index}"));
            let this = this.clone();
            let path = path.clone();
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
                                        shell.open_block_edit(project_id, &path, index, window, cx);
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
        path: &FileKey,
        block: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if (self.doc_mut(project_id, path)).is_some_and(|doc| doc.open_block_edit(block)) {
            if let Some(view) = self.doc_view(project_id, path) {
                view.page.remeasure();
            }
            self.focus_doc(project_id, path, DocFocus::Block, window, cx);
        }
        cx.notify();
    }

    fn close_block_edit(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let closed = (self.doc_mut(project_id, path)).and_then(|doc| doc.block_edit.take());
        if closed.is_some() {
            if let Some(view) = self.doc_view(project_id, path) {
                view.page.remeasure();
            }
            self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
        }
        cx.notify();
    }

    fn apply_block_edit(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let applied = (self.doc_mut(project_id, path)).map(documents::Doc::apply_block_edit);
        match applied.unwrap_or(Ok(())) {
            Ok(()) => {
                if let Some(view) = self.doc_view(project_id, path) {
                    view.page.remeasure();
                }
                self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
                self.file_edited(project_id, path, window, cx);
            }
            Err(StaleEdit) => self.toast_error("The file changed; edit again", cx),
        }
        cx.notify();
    }

    fn click_block_edit(&mut self, project_id: Ulid, path: &FileKey, caret: Caret) {
        if let Some(edit) = (self.doc_mut(project_id, path)).and_then(|doc| doc.block_edit.as_mut())
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
        let (Some(key), Some((project_id, path))) = (
            task_dialog::dialog_key(&event.keystroke),
            self.focused_doc(window),
        ) else {
            return;
        };
        let pasted = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(doc) = self.doc_mut(project_id, &path) else {
            return;
        };
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && let Some(text) = doc
                .block_edit
                .as_ref()
                .and_then(|edit| edit.text.input().selected_text().map(str::to_owned))
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if let (Some(text), Some(edit)) = (pasted, &mut doc.block_edit) {
            edit.text.insert(&text);
        }
        match doc.block_edit_key(key) {
            BlockEditOutcome::Edited => cx.notify(),
            BlockEditOutcome::Cancel => self.close_block_edit(project_id, &path, window, cx),
            BlockEditOutcome::Apply => self.apply_block_edit(project_id, &path, window, cx),
        }
    }
}

/// The edited block: its Markdown in a caret field, with how to apply / cancel under it.
/// Presses and drags inside stay here (the page would take the keyboard and move its own
/// selection); a press outside cancels.
#[allow(clippy::too_many_arguments)]
fn block_edit_field(
    project_id: Ulid,
    path: &FileKey,
    this: &Entity<Shell>,
    focus: &FocusHandle,
    input: &Input,
    focused: bool,
    marked: &Rc<ime::Composing<FocusHandle>>,
    target: &ime::FocusedField<Shell>,
    theme: &Theme,
) -> gpui::Div {
    let (keys, out, grab) = (this.clone(), this.clone(), focus.clone());
    let (click_path, out_path) = (path.clone(), path.clone());
    let field = caret_field_for(
        this,
        "file-block-field",
        input,
        "Markdown…",
        focused,
        theme,
        move |shell, caret| shell.click_block_edit(project_id, &click_path, caret),
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
                shell.close_block_edit(project_id, &out_path, window, cx)
            });
        })
        .child(field)
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(mkrew_ui::os_text!(
                    "Ctrl+Enter apply · Esc cancel",
                    "⌘↩ apply · Esc cancel"
                )),
        )
}
