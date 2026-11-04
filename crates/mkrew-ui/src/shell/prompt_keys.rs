//! Keys typed in a prompt box (SPEC §20B.1): the composer's draft and the task dialog's fields
//! share one path for typing, pasting, copying and the `@` picker.

use crate::*;

impl Shell {
    /// Applies `event` to the prompt `target`: `@` opens the file picker and `/` the agent's
    /// command picker (at a line start or after a space), a paste takes the clipboard's text
    /// (an image is saved as an attachment and its path typed), Ctrl+C / Ctrl+X copy the
    /// selection, every other key edits. Returns `true` when the key was typed or opened a
    /// picker, `false` when it asks the box to leave (Esc, Ctrl+Enter, Enter in the dialog's
    /// title): nothing changed and the caller answers it.
    pub(crate) fn prompt_key_down(
        &mut self,
        target: composer::Target,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return true;
        };
        if let Some(trigger) = self.prompt_trigger(target, &key) {
            match trigger {
                composer::Trigger::File => self.open_file_ref_picker(target, window, cx),
                composer::Trigger::Slash => self.open_slash_picker(target, window, cx),
            }
            return true;
        }
        if key == DialogKey::Paste
            && let Some(item) = cx.read_from_clipboard()
        {
            match item.text() {
                Some(text) => self.prompt_insert(target, &text),
                None => self.prompt_paste_image(target, item, window, cx),
            }
        }
        if let Some(input) = self.prompt_input(target) {
            copy_selection(&key, input, cx);
        }
        let typed = match target {
            composer::Target::Composer(session) => {
                self.composers.key(session, key) == text_box::Outcome::Edited
            }
            composer::Target::TaskDialog => self
                .task_dialog
                .as_mut()
                .is_some_and(|dialog| dialog.key(key) == task_dialog::Outcome::Edited),
        };
        if typed {
            self.prompt_follow_caret(target);
            cx.notify();
        }
        typed
    }

    /// The picker typing `key` would open in `target`.
    fn prompt_trigger(
        &self,
        target: composer::Target,
        key: &DialogKey,
    ) -> Option<composer::Trigger> {
        match target {
            composer::Target::Composer(session) => self.composers.trigger(session, key),
            composer::Target::TaskDialog => {
                (self.task_dialog.as_ref()).and_then(|dialog| dialog.trigger(key))
            }
        }
    }

    /// Pasted text goes where the caret is (the dialog's title stays on one line).
    fn prompt_insert(&mut self, target: composer::Target, text: &str) {
        match target {
            composer::Target::Composer(session) => self.composers.insert(session, text),
            composer::Target::TaskDialog => {
                if let Some(dialog) = self.task_dialog.as_mut() {
                    dialog.insert(text);
                }
            }
        }
    }

    /// The text field that has the keys: the draft, or the dialog's focused field.
    fn prompt_input(&self, target: composer::Target) -> Option<&text_box::Input> {
        match target {
            composer::Target::Composer(session) => {
                Some(self.composers.shown(session)?.prompt.text.input())
            }
            composer::Target::TaskDialog => {
                let dialog = self.task_dialog.as_ref()?;
                Some(match dialog.field {
                    Field::Title => &dialog.title,
                    Field::Prompt => dialog.prompt.text.input(),
                })
            }
        }
    }

    /// Keeps the end of the text in view while typing at the end.
    // ponytail: follows the caret only at the end; a caret moved above the visible lines is not
    // scrolled into view (needs line-height maths).
    fn prompt_follow_caret(&self, target: composer::Target) {
        match target {
            composer::Target::Composer(session) => {
                let at_end = self.composers.shown(session).is_none_or(|draft| {
                    let input = draft.prompt.text.input();
                    input.cursor() == input.len()
                });
                if at_end {
                    self.composer_scroll.scroll_to_bottom();
                }
            }
            composer::Target::TaskDialog => {
                let at_end = self.task_dialog.as_ref().is_some_and(|dialog| {
                    let input = dialog.prompt.text.input();
                    dialog.field == Field::Prompt && input.cursor() == input.len()
                });
                if at_end {
                    self.prompt_scroll.scroll_to_bottom();
                }
            }
        }
    }
}
