//! Picking lines with the keyboard (SPEC §12, §12B): Shift+↓ / Shift+↑ in Docs and the Git
//! diff, and Esc to drop the pick in Docs.

use crate::*;

impl Shell {
    /// Shift+↓ / Shift+↑ (`PickDown` / `PickUp`): picks the first line or block in view (Git:
    /// the first line of the current hunk), or grows / shrinks the pick one line or block, then
    /// scrolls the moved end into view. A text field never gets here: `run` hands the key to it.
    pub(crate) fn pick_step(&mut self, down: bool, window: &Window, cx: &mut Context<'_, Self>) {
        let (Some(project_id), Some(context)) =
            (self.workspace_in(window), self.comment_context(window))
        else {
            return;
        };
        let moved = match context {
            CommentContext::Doc => {
                let theme = cx.theme();
                let row = theme.terminal_font_size * theme.code_line_height;
                let view = self.host_view(self.space_host(window));
                let offset = f32::from(view.doc_source_scroll.0.borrow().base_handle.offset().y);
                let page_top = view.doc_page.top();
                let Some(docs) = self.docs.get_mut(&project_id) else {
                    return;
                };
                let Doc::Shown(shown) = &docs.doc else {
                    return;
                };
                let (units, start) = match docs.mode {
                    Mode::Source => (shown.line_units(), mkrew_ui::pick::first_row(offset, row)),
                    Mode::Preview => (shown.block_units(), page_top),
                };
                docs.comments.step_pick(&units, down, start)
            }
            CommentContext::Diff => self
                .git
                .get_mut(&project_id)
                .is_some_and(|view| view.step_pick(down)),
        };
        if moved {
            self.reveal_pick(project_id, context, cx);
        }
    }

    /// Scrolls the moved end of the shown pick (`Comments::head_line`) into view: a Docs source
    /// line, a Docs preview block, or a Git diff row (scrolled once the list is laid out, after
    /// the pick bar changed its heights). Safe to call right after `pick_lines`.
    pub(crate) fn reveal_pick(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        cx: &mut Context<'_, Self>,
    ) {
        match context {
            CommentContext::Doc => {
                if let Some(docs) = self.docs.get(&project_id)
                    && let Some(line) = docs.comments.head_line()
                    && let Doc::Shown(shown) = &docs.doc
                    && let Some(view) = self.project_view(project_id)
                {
                    match docs.mode {
                        Mode::Source => view.doc_source_scroll.scroll_to_item(
                            (line as usize).saturating_sub(1),
                            ScrollStrategy::Nearest,
                        ),
                        Mode::Preview => {
                            let block = shown
                                .blocks
                                .iter()
                                .position(|b| b.lines.start <= line && line <= b.lines.end);
                            if let Some(block) = block {
                                view.doc_page.reveal(block);
                            }
                        }
                    }
                }
            }
            CommentContext::Diff => {
                let row = self.git.get(&project_id).and_then(|view| match &view.diff {
                    git_view::Diff::Shown(shown) => {
                        shown.line_row(view.comments.head_line()?, self.diff_mode)
                    }
                    _ => None,
                });
                if row.is_some()
                    && let Some(state) = self.diff_views.get(&project_id)
                {
                    state.reveal.set(row);
                }
            }
        }
        cx.notify();
    }

    /// Esc in Docs drops the picked lines, while nothing else has the keyboard (the filter and
    /// the comment draft handle their own Esc and stop it first). Git does the same in
    /// `git_key_down`.
    pub(crate) fn docs_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let plain_escape = event.keystroke.key == "escape" && !event.keystroke.modifiers.modified();
        if !plain_escape
            || self.view(self.space_host(window)) != View::Docs
            || self.modal_open()
            || !self.focus_handle.is_focused(window)
        {
            return;
        }
        let picked = (self.workspace_in(window))
            .and_then(|project_id| self.docs.get_mut(&project_id))
            .map(|docs| &mut docs.comments)
            .filter(|comments| comments.selection.is_some());
        if let Some(comments) = picked {
            comments.selection = None;
            cx.stop_propagation();
            cx.notify();
        }
    }
}
