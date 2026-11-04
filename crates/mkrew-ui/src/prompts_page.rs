//! The Settings → Prompts page's model (SPEC §14, §20B.3): the library as listed, the draft being
//! edited (a prompt of the folder or a new one), when it counts as changed and which file write
//! saving it takes. `settings_dialog` routes the keys, `shell/prompts.rs` shows it and asks the
//! user. Tested headlessly.

use std::path::Path;

use mkrew_core::prompt::{self, PromptError, PromptLibrary};

use crate::text_box::{Input, TextBox};

/// Where the keyboard is on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// The prompt list.
    List,
    /// The Name field of the draft.
    Name,
    /// The prompt text of the draft.
    Body,
}

/// What the shell does for the page after a key or click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Edit the prompt of this name (asks first when the draft has unsaved changes).
    Open(String),
    /// Start a new prompt (asks first, like `Open`).
    New,
    /// Ctrl+S, Ctrl+Enter, the Save button.
    Save,
    /// Delete the open prompt (the shell asks first).
    Delete,
    OpenFolder,
}

/// The file work saving a draft takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Create {
        name: String,
        text: String,
    },
    Save {
        name: String,
        text: String,
    },
    /// Rename first, then write `text` under the new name when it was edited too. `body` is the
    /// whole draft: when the old file is gone (deleted outside Mkrew) the new name is created
    /// from it.
    Rename {
        old: String,
        new: String,
        text: Option<String>,
        body: String,
    },
}

/// What a [`Write`] did.
#[derive(Debug)]
pub struct Written {
    /// The file has its new name already (a later step may still have failed).
    pub renamed: bool,
    /// The name the write carried (the file's name after it).
    pub name: String,
    /// The text the write carried; `None` when it wrote none (a rename alone).
    pub text: Option<String>,
    pub result: Result<(), PromptError>,
}

impl Write {
    /// Does the file work in `dir`. A rename that worked stays done when the text write after it
    /// fails, and [`Written::renamed`] says so.
    pub fn run(&self, dir: &Path) -> Written {
        match self {
            Write::Create { name, text } => Written {
                renamed: false,
                name: name.clone(),
                text: Some(text.clone()),
                result: prompt::create(dir, name, text),
            },
            Write::Save { name, text } => Written {
                renamed: false,
                name: name.clone(),
                text: Some(text.clone()),
                result: prompt::save(dir, name, text),
            },
            Write::Rename {
                old,
                new,
                text,
                body,
            } => {
                match prompt::rename(dir, old, new) {
                    Ok(()) => {}
                    Err(PromptError::Missing(_)) => {
                        return Written {
                            renamed: true,
                            name: new.clone(),
                            text: Some(body.clone()),
                            result: prompt::create(dir, new, body),
                        };
                    }
                    Err(error) => {
                        return Written {
                            renamed: false,
                            name: new.clone(),
                            text: None,
                            result: Err(error),
                        };
                    }
                }
                let result = match text {
                    Some(text) => prompt::save(dir, new, text),
                    None => Ok(()),
                };
                Written {
                    renamed: true,
                    name: new.clone(),
                    text: text.clone(),
                    result,
                }
            }
        }
    }
}

/// The prompt being edited: one of the folder, or a new one.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The name of the file this edits; `None` = a new prompt not written yet.
    pub origin: Option<String>,
    pub name: Input,
    /// The whole file, `# Title` line included.
    pub body: TextBox,
    /// Why the last Save refused the name, shown under the Name field.
    pub name_error: Option<String>,
    /// What the file held when it was read or last written.
    saved_name: String,
    saved_text: String,
}

impl Draft {
    pub fn new_prompt() -> Self {
        Self {
            origin: None,
            name: Input::default(),
            body: TextBox::default(),
            name_error: None,
            saved_name: String::new(),
            saved_text: String::new(),
        }
    }

    /// `text` is the whole file as `prompt::read` gives it.
    pub fn open(name: &str, text: &str) -> Self {
        let mut body = TextBox::default();
        body.insert(text);
        Self {
            origin: Some(name.to_owned()),
            name: Input::from(name),
            name_error: None,
            saved_name: name.to_owned(),
            saved_text: body.text().to_owned(),
            body,
        }
    }

    /// The name as it is written: what was typed, without the blanks around it.
    pub fn name(&self) -> &str {
        self.name.trim()
    }

    pub fn is_dirty(&self) -> bool {
        self.name() != self.saved_name || self.body.text() != self.saved_text
    }

    /// The file work a Save takes; `None` when nothing changed.
    pub fn write(&self) -> Option<Write> {
        if !self.is_dirty() {
            return None;
        }
        let name = self.name().to_owned();
        let text = self.body.text().to_owned();
        Some(match &self.origin {
            None => Write::Create { name, text },
            Some(old) if *old == name => Write::Save { name, text },
            Some(old) => Write::Rename {
                old: old.clone(),
                text: (text != self.saved_text).then(|| text.clone()),
                body: text,
                new: name,
            },
        })
    }

    /// The file has `name` now.
    fn renamed(&mut self, name: &str) {
        self.origin = Some(name.to_owned());
        self.saved_name = name.to_owned();
    }

    /// The file holds `text` (when written) under `name`: what the write carried, not what was
    /// typed since.
    fn saved(&mut self, name: &str, text: Option<String>) {
        self.renamed(name);
        if let Some(text) = text {
            self.saved_text = text;
        }
        self.name_error = None;
    }
}

/// The page: the library as last read and the draft.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptsPage {
    pub library: PromptLibrary,
    pub draft: Option<Draft>,
    /// Why the last read, write or delete failed; shown at the top of the page.
    pub error: Option<String>,
    /// The folder has to be read again (the page opened, or something was written).
    pub stale: bool,
}

impl Default for PromptsPage {
    fn default() -> Self {
        Self {
            library: PromptLibrary::default(),
            draft: None,
            error: None,
            stale: true,
        }
    }
}

impl PromptsPage {
    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(Draft::is_dirty)
    }

    /// The name of the open prompt while the draft equals its file.
    pub fn clean_origin(&self) -> Option<&str> {
        (self.draft.as_ref())
            .filter(|draft| !draft.is_dirty())
            .and_then(|draft| draft.origin.as_deref())
    }

    /// "New prompt": an empty draft.
    pub fn start_new(&mut self) {
        self.draft = Some(Draft::new_prompt());
        self.error = None;
    }

    /// Edits the prompt `name` whose file holds `text`.
    pub fn open(&mut self, name: &str, text: &str) {
        self.draft = Some(Draft::open(name, text));
        self.error = None;
    }

    /// The file `name` was read again: a clean draft of it shows the new text; typed text is
    /// never replaced, and an unchanged text keeps the caret where it is.
    pub fn refresh(&mut self, name: &str, text: &str) {
        let fresh = Draft::open(name, text);
        let replace = (self.draft.as_ref()).is_some_and(|draft| {
            !draft.is_dirty()
                && draft.origin.as_deref() == Some(name)
                && draft.saved_text != fresh.saved_text
        });
        if replace {
            self.draft = Some(fresh);
        }
    }

    /// The folder was read. A clean draft whose file is gone closes; an edited one stays (its
    /// Save writes the file again).
    pub fn reload(&mut self, library: PromptLibrary) {
        self.library = library;
        self.stale = false;
        let gone = self.clean_origin().is_some_and(|name| {
            !self
                .library
                .prompts
                .iter()
                .any(|prompt| prompt.name == name)
        });
        if gone {
            self.draft = None;
        }
    }

    /// The name of the prompt `by` rows from the open one (the first, or the last when going
    /// back, while none is open); `None` at the ends of the list.
    pub fn neighbour(&self, by: i32) -> Option<String> {
        let prompts = &self.library.prompts;
        let last = prompts.len().checked_sub(1)?;
        let open = self
            .draft
            .as_ref()
            .and_then(|draft| draft.origin.as_deref());
        let at = open.and_then(|name| prompts.iter().position(|prompt| prompt.name == name));
        let step = usize::try_from(by.unsigned_abs()).unwrap_or(usize::MAX);
        let to = match at {
            None if by > 0 => 0,
            None => last,
            Some(at) if by < 0 => at.saturating_sub(step),
            Some(at) => at.saturating_add(step).min(last),
        };
        (prompts.get(to))
            .filter(|prompt| Some(prompt.name.as_str()) != open)
            .map(|prompt| prompt.name.clone())
    }

    /// Takes the result of a [`Write`]: the draft follows what happened to the file. `true` when
    /// the file now holds the draft. A refused name shows under the Name field, any other
    /// failure at the top of the page.
    pub fn written(&mut self, written: Written) -> bool {
        self.stale |= written.renamed || written.result.is_ok();
        let Some(draft) = self.draft.as_mut() else {
            return false;
        };
        if written.renamed {
            draft.renamed(&written.name);
        }
        match written.result {
            Ok(()) => {
                draft.saved(&written.name, written.text);
                self.error = None;
                !draft.is_dirty()
            }
            Err(PromptError::Name(error)) => {
                draft.name_error = Some(error.to_string());
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mkrew_core::prompt::{NameError, Prompt, PromptError};

    use super::*;

    fn prompt(name: &str) -> Prompt {
        Prompt {
            name: name.into(),
            title: name.to_uppercase(),
            body: String::new(),
        }
    }

    fn page(names: &[&str]) -> PromptsPage {
        PromptsPage {
            library: PromptLibrary {
                prompts: names.iter().map(|name| prompt(name)).collect(),
                errors: Vec::new(),
            },
            ..PromptsPage::default()
        }
    }

    #[test]
    fn a_new_draft_is_clean_until_something_is_typed() {
        let mut draft = Draft::new_prompt();
        assert!(!draft.is_dirty());
        assert_eq!(draft.write(), None, "nothing to save");
        draft.name.insert("idea");
        assert!(draft.is_dirty());
        assert_eq!(
            draft.write(),
            Some(Write::Create {
                name: "idea".into(),
                text: String::new()
            })
        );
        draft.name.clear();
        assert!(!draft.is_dirty(), "typed text deleted again");
        draft.body.insert("only text");
        assert!(
            draft.is_dirty(),
            "a body without a name is a change the Save refuses by name"
        );
    }

    #[test]
    fn an_opened_prompt_is_dirty_when_name_or_text_changes_and_clean_again_when_reverted() {
        let mut draft = Draft::open("review", "# Review\nbody\n");
        assert!(!draft.is_dirty());
        assert_eq!(draft.origin.as_deref(), Some("review"));
        draft.body.insert("more");
        assert!(draft.is_dirty());
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        assert!(!draft.is_dirty(), "text typed and deleted again");
        // Blanks around the name are not part of it.
        draft.name.insert("  ");
        assert!(!draft.is_dirty());
        assert_eq!(draft.name(), "review");
        draft.name.insert("x");
        assert!(draft.is_dirty());
    }

    #[test]
    fn write_picks_create_save_or_rename() {
        let mut draft = Draft::open("old", "text");
        assert_eq!(draft.write(), None);
        draft.body.insert("!");
        assert_eq!(
            draft.write(),
            Some(Write::Save {
                name: "old".into(),
                text: "text!".into()
            })
        );
        let mut renamed = Draft::open("old", "text");
        renamed.name.clear();
        renamed.name.insert(" new ");
        assert_eq!(
            renamed.write(),
            Some(Write::Rename {
                old: "old".into(),
                new: "new".into(),
                text: None,
                body: "text".into()
            }),
            "only the name changed: the text is not written"
        );
        renamed.body.insert("!");
        assert_eq!(
            renamed.write(),
            Some(Write::Rename {
                old: "old".into(),
                new: "new".into(),
                text: Some("text!".into()),
                body: "text!".into()
            })
        );
    }

    #[test]
    fn a_rename_with_edited_text_moves_the_file_then_writes_the_text() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "old", "# Old\nbody\n").unwrap();
        let mut draft = Draft::open("old", "# Old\nbody\n");
        draft.name.clear();
        draft.name.insert("new");
        draft.body.insert("more ");
        let write = draft.write().unwrap();
        let done = write.run(dir.path());
        assert!(done.renamed && done.result.is_ok(), "{:?}", done.result);
        assert_eq!(
            prompt::read(dir.path(), "new").unwrap(),
            "# Old\nbody\nmore "
        );
        assert!(matches!(
            prompt::read(dir.path(), "old"),
            Err(PromptError::Missing(_))
        ));
    }

    #[test]
    fn a_rename_of_a_file_deleted_outside_creates_the_new_name_from_the_draft() {
        let dir = tempfile::tempdir().unwrap();
        let mut draft = Draft::open("gone", "# Gone\nbody\n");
        draft.name.clear();
        draft.name.insert("again");
        let done = draft.write().unwrap().run(dir.path());
        assert!(done.result.is_ok(), "{:?}", done.result);
        assert_eq!(done.text.as_deref(), Some("# Gone\nbody\n"));
        assert_eq!(prompt::read(dir.path(), "again").unwrap(), "# Gone\nbody\n");
    }

    #[test]
    fn a_refused_name_keeps_the_file_and_the_draft_and_says_why_under_the_field() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "a", "text a").unwrap();
        prompt::create(dir.path(), "b", "text b").unwrap();
        let mut page = PromptsPage::default();
        page.open("a", "text a");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("B");
        draft.body.insert("!");
        let write = page.draft.as_ref().unwrap().write().unwrap();
        let done = write.run(dir.path());
        assert!(!done.renamed);
        assert!(!page.written(done));
        let draft = page.draft.as_ref().unwrap();
        assert_eq!(
            draft.name_error.as_deref(),
            Some("A prompt named \"b\" exists already.")
        );
        assert_eq!(page.error, None, "a name problem is shown under the field");
        assert!(draft.is_dirty(), "the typed text is still there");
        assert_eq!(prompt::read(dir.path(), "a").unwrap(), "text a");
    }

    #[test]
    fn a_failed_save_after_a_rename_keeps_the_new_name_for_the_retry() {
        let mut page = PromptsPage::default();
        page.open("old", "text");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("new");
        draft.body.insert("!");
        let io = PromptError::Io {
            action: "write",
            path: "prompts/new.md".into(),
            source: std::io::Error::other("disk full"),
        };
        assert!(!page.written(Written {
            renamed: true,
            name: "new".into(),
            text: Some("text!".into()),
            result: Err(io)
        }));
        assert_eq!(
            page.error.as_deref(),
            Some("Cannot write prompts/new.md: disk full")
        );
        let draft = page.draft.as_ref().unwrap();
        assert_eq!(
            draft.origin.as_deref(),
            Some("new"),
            "the file has its new name"
        );
        assert_eq!(
            draft.write(),
            Some(Write::Save {
                name: "new".into(),
                text: "text!".into()
            }),
            "the retry only writes the text"
        );
    }

    #[test]
    fn a_write_marks_the_folder_stale_only_when_it_changed_something() {
        let mut page = PromptsPage::default();
        page.open("old", "text");
        page.stale = false;
        let io = PromptError::Io {
            action: "write",
            path: "prompts/new.md".into(),
            source: std::io::Error::other("disk full"),
        };
        page.draft.as_mut().unwrap().name.insert("x");
        assert!(!page.written(Written {
            renamed: true,
            name: "oldx".into(),
            text: Some("text".into()),
            result: Err(io)
        }));
        assert!(page.stale, "the rename happened");
        page.stale = false;
        assert!(!page.written(Written {
            renamed: false,
            name: "b".into(),
            text: None,
            result: Err(PromptError::Name(NameError::Duplicate("b".into())))
        }));
        assert!(!page.stale, "a refused name wrote nothing");
        assert_eq!(
            page.draft.as_ref().unwrap().origin.as_deref(),
            Some("oldx"),
            "the earlier rename stays; the refusal changed no origin"
        );
    }

    #[test]
    fn a_name_error_on_a_rename_leaves_the_origin_alone() {
        let mut page = PromptsPage::default();
        page.open("old", "text");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("b");
        assert!(!page.written(Written {
            renamed: false,
            name: "b".into(),
            text: None,
            result: Err(PromptError::Name(NameError::Duplicate("b".into())))
        }));
        let draft = page.draft.as_ref().unwrap();
        assert_eq!(draft.origin.as_deref(), Some("old"));
        assert!(draft.name_error.is_some());
        assert!(draft.is_dirty());
    }

    #[test]
    fn typing_while_a_write_runs_is_not_marked_saved() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "a", "x").unwrap();
        let mut page = PromptsPage::default();
        page.open("a", "x");
        page.draft.as_mut().unwrap().body.insert("1");
        let done = page
            .draft
            .as_ref()
            .unwrap()
            .write()
            .unwrap()
            .run(dir.path());
        page.draft.as_mut().unwrap().body.insert("2");
        assert!(!page.written(done), "the file does not hold the newer text");
        let draft = page.draft.as_ref().unwrap();
        assert!(draft.is_dirty());
        assert_eq!(
            draft.write(),
            Some(Write::Save {
                name: "a".into(),
                text: "x12".into()
            })
        );
        assert_eq!(prompt::read(dir.path(), "a").unwrap(), "x1");
    }

    #[test]
    fn a_case_only_rename_keeps_the_draft_after_reload() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "a", "text").unwrap();
        let mut page = PromptsPage::default();
        page.open("a", "text");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("A");
        let done = page
            .draft
            .as_ref()
            .unwrap()
            .write()
            .unwrap()
            .run(dir.path());
        assert!(page.written(done));
        page.reload(prompt::load_dir(dir.path()));
        assert_eq!(page.draft.as_ref().unwrap().origin.as_deref(), Some("A"));
        assert_eq!(page.library.prompts[0].name, "A");
    }

    #[test]
    fn a_successful_write_makes_the_draft_clean_and_clears_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut page = PromptsPage::default();
        page.start_new();
        page.error = Some("old problem".into());
        let draft = page.draft.as_mut().unwrap();
        draft.name.insert("fresh");
        draft.body.insert("# Fresh\nhello");
        let done = page
            .draft
            .as_ref()
            .unwrap()
            .write()
            .unwrap()
            .run(dir.path());
        assert!(page.written(done));
        assert!(!page.is_dirty());
        assert_eq!(page.error, None);
        assert_eq!(
            page.draft.as_ref().unwrap().origin.as_deref(),
            Some("fresh")
        );
        assert_eq!(
            page.draft.as_ref().unwrap().write(),
            None,
            "a second Save has nothing to write"
        );
    }

    #[test]
    fn neighbour_moves_through_the_prompts_and_stops_at_the_ends() {
        let mut page = page(&["a", "b", "c"]);
        assert_eq!(
            page.neighbour(1).as_deref(),
            Some("a"),
            "nothing open: the first"
        );
        assert_eq!(
            page.neighbour(-1).as_deref(),
            Some("c"),
            "nothing open: the last"
        );
        page.open("b", "x");
        assert_eq!(page.neighbour(1).as_deref(), Some("c"));
        assert_eq!(page.neighbour(-1).as_deref(), Some("a"));
        page.open("c", "x");
        assert_eq!(page.neighbour(1), None, "no wrap past the last");
        page.open("a", "x");
        assert_eq!(page.neighbour(-1), None, "no wrap before the first");
        assert_eq!(
            super::PromptsPage::default().neighbour(1),
            None,
            "empty library"
        );
    }

    #[test]
    fn reload_drops_a_clean_draft_whose_file_is_gone_and_keeps_an_edited_one() {
        let mut page = page(&["a", "b"]);
        page.open("a", "x");
        page.reload(PromptLibrary {
            prompts: vec![prompt("b")],
            errors: Vec::new(),
        });
        assert!(page.draft.is_none());
        assert!(!page.stale);
        page.open("b", "x");
        page.draft.as_mut().unwrap().body.insert("!");
        page.reload(PromptLibrary::default());
        assert!(page.draft.is_some(), "the typed text outlives its file");
        // A new, empty draft has no file to lose.
        page.start_new();
        page.reload(PromptLibrary::default());
        assert!(page.draft.is_some());
    }

    #[test]
    fn a_clean_draft_follows_the_file_and_an_edited_one_does_not() {
        let mut page = PromptsPage::default();
        page.open("a", "old");
        page.refresh("a", "old");
        assert_eq!(page.draft.as_ref().unwrap().body.text(), "old");
        page.refresh("a", "changed in Zed");
        assert_eq!(page.draft.as_ref().unwrap().body.text(), "changed in Zed");
        assert!(!page.is_dirty());
        page.draft.as_mut().unwrap().body.insert("!");
        page.refresh("a", "changed again");
        assert_eq!(
            page.draft.as_ref().unwrap().body.text(),
            "changed in Zed!",
            "typed text is never replaced"
        );
        assert_eq!(page.clean_origin(), None);
    }
}
