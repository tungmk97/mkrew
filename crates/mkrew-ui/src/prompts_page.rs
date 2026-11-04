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
    /// Rename first, then write `text` under the new name when it was edited too.
    Rename {
        old: String,
        new: String,
        text: Option<String>,
    },
}

/// What a [`Write`] did.
#[derive(Debug)]
pub struct Written {
    /// The file has its new name already (a later step may still have failed).
    pub renamed: bool,
    pub result: Result<(), PromptError>,
}

impl Write {
    /// Does the file work in `dir`. A rename that worked stays done when the text write after it
    /// fails, and [`Written::renamed`] says so.
    pub fn run(&self, dir: &Path) -> Written {
        match self {
            Write::Create { name, text } => Written {
                renamed: false,
                result: prompt::create(dir, name, text),
            },
            Write::Save { name, text } => Written {
                renamed: false,
                result: prompt::save(dir, name, text),
            },
            Write::Rename { old, new, text } => {
                if let Err(error) = prompt::rename(dir, old, new) {
                    return Written {
                        renamed: false,
                        result: Err(error),
                    };
                }
                let result = match text {
                    Some(text) => prompt::save(dir, new, text),
                    None => Ok(()),
                };
                Written {
                    renamed: true,
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
                text: (text != self.saved_text).then_some(text),
                new: name,
            },
        })
    }

    /// The file has the name in the field now.
    fn renamed(&mut self) {
        let name = self.name().to_owned();
        self.origin = Some(name.clone());
        self.saved_name = name;
    }

    /// The file holds what the draft shows now.
    fn saved(&mut self) {
        self.renamed();
        let text = self.body.text().to_owned();
        self.saved_text = text;
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
        let Some(draft) = self.draft.as_mut() else {
            return false;
        };
        if written.renamed {
            draft.renamed();
        }
        match written.result {
            Ok(()) => {
                draft.saved();
                self.error = None;
                true
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
                text: None
            }),
            "only the name changed: the text is not written"
        );
        renamed.body.insert("!");
        assert_eq!(
            renamed.write(),
            Some(Write::Rename {
                old: "old".into(),
                new: "new".into(),
                text: Some("text!".into())
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
        assert_eq!(
            prompt::validate_name(dir.path(), "", None),
            Err(NameError::Empty)
        );
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
