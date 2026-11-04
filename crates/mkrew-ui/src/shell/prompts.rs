//! Settings → Prompts (SPEC §14, §20B.3): reads and writes `<config>/mkrew/prompts` through
//! `mkrew_core::prompt` for the page model (`mkrew_ui::prompts_page`, driven by the dialog's
//! `Tab::Prompts`) and asks before unsaved text is left. The page is drawn in `prompts_render`.
//! A machine (remote) window edits this computer's library too (spec 7.3): nothing here goes
//! through the daemon.

use crate::*;
use mkrew_core::prompt::{self, PromptError};
use mkrew_ui::prompts_page::{Draft, PromptAction};
use mkrew_ui::settings_dialog::Tab;

impl Shell {
    /// The palette's "Global: Manage prompts…" and the prompt picker's last row: Settings on the
    /// Prompts page (an open Settings dialog keeps its state, an edited draft included).
    #[expect(
        dead_code,
        reason = "the palette and the prompt picker call it from Task C5"
    )]
    pub(crate) fn open_prompts_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.settings_dialog.is_none() {
            self.open_settings_dialog(window, cx);
        }
        let effect = (self.settings_dialog.as_mut()).map(|dialog| dialog.set_tab(Tab::Prompts));
        window.focus(&self.dialog_focus, cx);
        if let Some(effect) = effect {
            self.apply_settings_effect(effect, window, cx);
        }
    }

    /// Runs what the page asked for.
    pub(crate) fn prompts_action(
        &mut self,
        action: PromptAction,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match action {
            PromptAction::Open(name) => {
                let then: AfterSave = Box::new(move |shell, _, _| shell.open_prompt_now(&name));
                self.leave_prompt_draft(then, window, cx);
            }
            PromptAction::New => {
                let then: AfterSave = Box::new(|shell, _, _| {
                    if let Some(dialog) = shell.settings_dialog.as_mut() {
                        dialog.prompts_new();
                    }
                });
                self.leave_prompt_draft(then, window, cx);
            }
            PromptAction::Save => self.save_prompt(None, window, cx),
            PromptAction::Delete => self.ask_delete_prompt(window, cx),
            PromptAction::OpenFolder => self.open_prompts_folder(cx),
        }
    }

    /// Reads the folder when the page asks for it (it opened, or something was written): the
    /// list, and the text of a clean open prompt (an edit made in Zed shows; typed text is never
    /// replaced). The first read seeds the three samples, like the prompt picker.
    pub(crate) fn sync_prompts(&mut self) {
        let stale = (self.settings_dialog.as_ref())
            .is_some_and(|dialog| dialog.tab == Tab::Prompts && dialog.prompts.stale);
        if !stale {
            return;
        }
        let library = prompt::load_or_seed(&self.prompts_dir);
        let open = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.clean_origin().map(str::to_owned));
        let text = open.map(|name| {
            let read = prompt::read(&self.prompts_dir, &name);
            (name, read)
        });
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        dialog.prompts_loaded(library);
        if let Some((name, Ok(text))) = text {
            dialog.prompts.refresh(&name, &text);
        }
    }

    /// Edits the prompt `name`: its file is read now.
    fn open_prompt_now(&mut self, name: &str) {
        let read = prompt::read(&self.prompts_dir, name);
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        match read {
            Ok(text) => dialog.prompts_open(name, &text),
            Err(error) => {
                dialog.prompts.error = Some(error.to_string());
                // The folder changed under the list.
                dialog.prompts.stale = true;
            }
        }
        let at = dialog
            .prompts
            .library
            .prompts
            .iter()
            .position(|p| p.name == name);
        if let Some(at) = at {
            self.prompts_scroll.scroll_to_item(at);
        }
    }

    /// Runs `then` once the unsaved changes of the open prompt are dealt with: at once when
    /// there are none; after "Save" and a successful write, or "Don't save". "Cancel" (or Esc)
    /// stops: `then` does not run.
    pub(crate) fn leave_prompt_draft(
        &mut self,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let label = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .filter(|draft| draft.is_dirty())
            .map(|draft| match draft.name() {
                "" => "the new prompt".to_owned(),
                name => format!("“{name}”"),
            });
        let Some(label) = label else {
            return then(self, window, cx);
        };
        // Shown first, so a refused save or name is visible (like Files shows its tab).
        let other_tab =
            (self.settings_dialog.as_ref()).is_some_and(|dialog| dialog.tab != Tab::Prompts);
        if other_tab {
            let effect = (self.settings_dialog.as_mut()).map(|dialog| dialog.set_tab(Tab::Prompts));
            if let Some(effect) = effect {
                self.apply_settings_effect(effect, window, cx);
            }
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {label}?"),
            Some("Your changes are lost if you don't save them."),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                match choice {
                    Some(files_view::Leave::Save) => shell.save_prompt(Some(then), window, cx),
                    Some(files_view::Leave::DontSave) => {
                        then(shell, window, cx);
                        // A prompt whose file cannot be read leaves the list at once.
                        shell.sync_prompts();
                    }
                    None => window.focus(&shell.dialog_focus, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Ctrl+S, the Save button and "Save" of the question: writes the draft (nothing happens
    /// when it is clean), reads the folder again, then runs `then`. A refused name or a failed
    /// write shows on the page and `then` does not run; neither does it when keys typed since
    /// the write began are still unsaved.
    pub(crate) fn save_prompt(
        &mut self,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let write = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .and_then(Draft::write);
        let Some(write) = write else {
            if let Some(then) = then {
                then(self, window, cx);
            }
            return;
        };
        let written = write.run(&self.prompts_dir);
        let saved =
            (self.settings_dialog.as_mut()).is_some_and(|dialog| dialog.prompts_written(written));
        self.sync_prompts();
        if saved && let Some(then) = then {
            then(self, window, cx);
        }
        cx.notify();
    }

    /// Delete: asks, then removes the open prompt's file.
    fn ask_delete_prompt(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let name = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .and_then(|draft| draft.origin.clone());
        let Some(name) = name else {
            return;
        };
        let answer = window.prompt(
            PromptLevel::Critical,
            &format!("Delete “{name}”?"),
            Some("Its file is deleted from the prompts folder. This cannot be undone."),
            &task_dialog::DELETE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let confirmed = task_dialog::confirms_delete(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                if confirmed {
                    shell.delete_prompt(&name);
                }
                window.focus(&shell.dialog_focus, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_prompt(&mut self, name: &str) {
        let result = prompt::delete(&self.prompts_dir, name);
        if let Some(dialog) = self.settings_dialog.as_mut() {
            match result {
                // A prompt already gone is what Delete wanted.
                Ok(()) | Err(PromptError::Missing(_)) => {
                    dialog.prompts_deleted();
                    dialog.prompts.error = None;
                }
                Err(error) => dialog.prompts.error = Some(error.to_string()),
            }
            dialog.prompts.stale = true;
        }
        self.sync_prompts();
    }
}
