//! The prompt picker (SPEC §20B.3): the library's prompts, the picked one rendered with the
//! variables known where it was opened, a muted note naming the ones left empty, and a last row
//! that opens Settings → Prompts. Opened from
//! the palette ("Insert prompt…"), the composer ("Prompts") and the task dialog ("From
//! prompt…"); the pick is typed into the composer or the task prompt. Pure and tested
//! headlessly; `main.rs` renders it and reads keys with [`crate::quick_open::key`].

use mkrew_core::prompt::{self, PromptLibrary, PromptVars, Rendered};

use crate::composer::Target;

/// The open picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptPicker {
    /// Where the picked prompt is typed.
    pub target: Target,
    pub library: PromptLibrary,
    pub vars: PromptVars,
    /// Index into `library.prompts` of the picked one (the Manage row when it equals their count).
    pub selected: usize,
}

impl PromptPicker {
    pub fn new(target: Target, library: PromptLibrary, vars: PromptVars) -> Self {
        Self {
            target,
            library,
            vars,
            selected: 0,
        }
    }

    /// The row after the prompts: "Manage prompts…", which opens Settings → Prompts instead of
    /// typing anything.
    pub fn manage_row(&self) -> usize {
        self.library.prompts.len()
    }

    /// Whether the pick is the Manage row.
    pub fn on_manage(&self) -> bool {
        self.selected == self.manage_row()
    }

    /// Moves the pick by `by`, wrapping around the prompts and the Manage row.
    pub fn step(&mut self, by: i32) {
        let rows = i64::try_from(self.manage_row() + 1).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(rows)).unwrap_or(0);
    }

    /// Picks row `index` (a click); the Manage row counts.
    pub fn pick(&mut self, index: usize) {
        if index <= self.manage_row() {
            self.selected = index;
        }
    }

    /// The picked prompt rendered with the known variables (also what gets typed).
    pub fn preview(&self) -> Option<Rendered> {
        let picked = self.library.prompts.get(self.selected)?;
        Some(prompt::render(&picked.body, &self.vars))
    }
}

/// `text` as typed at the end of `before`: on a line of its own unless `before` is empty or
/// already ends with a line break.
pub fn on_new_line(before: &str, text: &str) -> String {
    if before.is_empty() || before.ends_with('\n') {
        text.to_owned()
    } else {
        format!("\n{text}")
    }
}

/// The muted note under the preview naming the variables rendered empty; `None` if none.
pub fn missing_note(missing: &[String]) -> Option<String> {
    if missing.is_empty() {
        return None;
    }
    let names: Vec<String> = missing.iter().map(|name| format!("{{{name}}}")).collect();
    Some(format!(
        "Left empty (not available here): {}",
        names.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use mkrew_core::prompt::Prompt;
    use ulid::Ulid;

    use super::*;

    fn library() -> PromptLibrary {
        let prompt = |name: &str, body: &str| Prompt {
            name: name.into(),
            title: name.to_uppercase(),
            body: body.into(),
        };
        PromptLibrary {
            prompts: vec![
                prompt("explain", "Explain {file} in {project}."),
                prompt("review", "Review {task}: {selection}{nope}"),
            ],
            errors: Vec::new(),
        }
    }

    #[test]
    fn the_pick_wraps_and_renders_with_the_known_variables() {
        let vars = PromptVars {
            project: Some("mkrew".into()),
            task: Some("Fix login".into()),
            ..PromptVars::default()
        };
        let mut picker = PromptPicker::new(Target::TaskDialog, library(), vars);
        let preview = picker.preview().unwrap();
        assert_eq!(preview.text, "Explain  in mkrew.");
        assert_eq!(preview.missing, ["file"]);

        picker.step(-1);
        assert!(
            picker.on_manage(),
            "Up from the first prompt wraps to the Manage row"
        );
        assert_eq!(picker.preview(), None);
        picker.step(-1);
        assert_eq!(picker.selected, 1);
        let preview = picker.preview().unwrap();
        assert_eq!(preview.text, "Review Fix login: ");
        assert_eq!(preview.missing, ["selection", "nope"]);
        picker.step(1);
        picker.step(1);
        assert_eq!(picker.selected, 0);

        picker.pick(1);
        assert_eq!(picker.selected, 1);
        picker.pick(7);
        assert_eq!(picker.selected, 1, "out of range is ignored");
    }

    #[test]
    fn an_empty_library_has_no_preview() {
        let mut picker = PromptPicker::new(
            Target::Composer(Ulid::new()),
            PromptLibrary::default(),
            PromptVars::default(),
        );
        picker.step(1);
        assert_eq!((picker.selected, picker.preview()), (0, None));
        assert!(
            picker.on_manage(),
            "the Manage row is all an empty library has"
        );
    }

    #[test]
    fn the_manage_row_is_the_last_stop_of_the_keyboard_in_both_pickers() {
        // The composer's picker and the New task dialog's "From prompt…" are the same picker.
        for target in [Target::Composer(Ulid::new()), Target::TaskDialog] {
            let mut picker = PromptPicker::new(target, library(), PromptVars::default());
            assert_eq!(picker.manage_row(), 2);
            picker.step(1);
            picker.step(1);
            assert!(picker.on_manage());
            assert_eq!(picker.preview(), None, "nothing is typed for it");
            picker.step(1);
            assert_eq!(
                (picker.selected, picker.on_manage()),
                (0, false),
                "wraps to the first prompt"
            );
            picker.step(-1);
            assert!(picker.on_manage());
            picker.pick(0);
            picker.pick(2);
            assert!(picker.on_manage(), "a click on the row picks it");
            picker.pick(3);
            assert_eq!(picker.selected, 2, "past the Manage row is ignored");
        }
    }

    #[test]
    fn a_pick_after_typed_text_starts_a_new_line() {
        assert_eq!(on_new_line("", "Explain."), "Explain.");
        assert_eq!(on_new_line("Fix the bug", "Explain."), "\nExplain.");
        assert_eq!(on_new_line("Fix the bug\n", "Explain."), "Explain.");
    }

    #[test]
    fn the_note_names_the_variables_left_empty() {
        assert_eq!(missing_note(&[]), None);
        assert_eq!(
            missing_note(&["file".into(), "selection".into()]).as_deref(),
            Some("Left empty (not available here): {file}, {selection}")
        );
    }
}
