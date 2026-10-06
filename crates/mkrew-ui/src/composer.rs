//! The prompt composer docked at an agent pane's bottom (SPEC §20B.1): one draft per session,
//! kept in UI memory while the composer is hidden or another pane has the keyboard, sent with
//! `session.send` + Enter, with the session's send queue (§20B.2). Pure and tested headlessly;
//! `main.rs` renders it.

use std::collections::HashMap;

use gpui::ImageFormat;
use mkrew_protocol::{Event, QueuedSend, SessionSendParams};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::{self, TextBox};

/// One session's composer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub text: TextBox,
    /// Docked under the pane (Esc hides it, the draft stays).
    pub shown: bool,
    /// A `session.send` is in flight.
    pub busy: bool,
    /// Why the last send failed.
    pub error: Option<String>,
}

/// Every session's draft.
#[derive(Debug, Clone, Default)]
pub struct Composers {
    drafts: HashMap<Ulid, Draft>,
    /// Each session's texts waiting to be typed (`session.queue`), in delivery order.
    queues: HashMap<Ulid, Vec<QueuedSend>>,
}

impl Composers {
    /// Shows `session`'s composer with its draft (a new one the first time).
    pub fn open(&mut self, session: Ulid) {
        self.drafts.entry(session).or_default().shown = true;
    }

    /// Hides `session`'s composer, keeping its draft.
    pub fn hide(&mut self, session: Ulid) {
        if let Some(draft) = self.drafts.get_mut(&session) {
            draft.shown = false;
        }
    }

    /// `session`'s draft while its composer is shown.
    pub fn shown(&self, session: Ulid) -> Option<&Draft> {
        self.drafts.get(&session).filter(|draft| draft.shown)
    }

    /// Applies a key typed in `session`'s composer (ignored while sending, except Esc).
    pub fn key(&mut self, session: Ulid, key: DialogKey) -> text_box::Outcome {
        let Some(draft) = self.drafts.get_mut(&session) else {
            return text_box::Outcome::Edited;
        };
        match key {
            DialogKey::Cancel => text_box::Outcome::Cancel,
            _ if draft.busy => text_box::Outcome::Edited,
            key => {
                let outcome = draft.text.key(key);
                if outcome == text_box::Outcome::Edited {
                    draft.error = None;
                }
                outcome
            }
        }
    }

    /// Pastes `text` into `session`'s draft (ignored while sending).
    pub fn insert(&mut self, session: Ulid, text: &str) {
        if let Some(draft) = self.drafts.get_mut(&session).filter(|d| !d.busy) {
            draft.text.insert(text);
            draft.error = None;
        }
    }

    /// Marks `session`'s draft as sending and returns its `session.send` (text + Enter);
    /// `None` while one is in flight or when nothing but blanks was typed.
    pub fn begin_send(&mut self, session: Ulid) -> Option<SessionSendParams> {
        let draft = self
            .drafts
            .get_mut(&session)
            .filter(|draft| !draft.busy && !draft.text.is_blank())?;
        draft.busy = true;
        draft.error = None;
        Some(SessionSendParams {
            session_id: session,
            text: without_blank_edges(draft.text.text()).to_owned(),
            submit: true,
        })
    }

    /// Keeps the queue of a `session.queue` event. Returns `true` when it changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::SessionQueue(queue) = event else {
            return false;
        };
        if queue.items.is_empty() {
            self.queues.remove(&queue.session_id).is_some()
        } else {
            self.queues
                .insert(queue.session_id, queue.items.clone())
                .as_ref()
                != Some(&queue.items)
        }
    }

    /// `session`'s texts waiting to be typed.
    pub fn queue(&self, session: Ulid) -> &[QueuedSend] {
        self.queues.get(&session).map_or(&[], Vec::as_slice)
    }

    /// The send finished: written or queued empties the draft, a failure keeps it.
    pub fn sent(&mut self, session: Ulid, result: Result<(), String>) {
        let Some(draft) = self.drafts.get_mut(&session) else {
            return;
        };
        draft.busy = false;
        match result {
            Ok(()) => draft.text.clear(),
            Err(error) => draft.error = Some(format!("Cannot send: {error}")),
        }
    }
}

/// How a picked file is typed (§20B.1): `template` (`composer.file_ref`) with `{path}` replaced
/// by the project-relative `path`, `/`-separated, quoted with `"` if it contains spaces.
pub fn file_ref(template: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path
    };
    template.replace("{path}", &path)
}

/// What "Send selection" (§20B.4) opens the composer with: `` `<path>:<a>-<b>` `` for picked
/// lines `a..=b` of a file (`lines`; `None` = terminal text), then `text` in a fenced block
/// (fenced longer than any backtick run inside), ending on a new line for the instruction.
pub fn selection_prefill(lines: Option<(&str, u32, u32)>, text: &str) -> String {
    let mut prefill = String::new();
    if let Some((path, start, end)) = lines {
        let path = path.replace('\\', "/");
        prefill.push_str(&format!("`{path}:{start}-{end}`\n"));
    }
    let text = without_blank_edges(text);
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest.max(2) + 1);
    prefill.push_str(&format!("{fence}\n{text}\n{fence}\n"));
    prefill
}

/// How dropped files and pasted images are typed into a pane (§20B.5): the absolute paths,
/// each quoted with `"` if it contains whitespace, separated by spaces, no Enter.
pub fn typed_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> String {
    paths
        .into_iter()
        .map(|path| {
            if path.contains(char::is_whitespace) {
                format!("\"{path}\"")
            } else {
                path.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The `attachment.save` extension of a clipboard image (§20B.5); `None` for formats the
/// daemon does not store (svg, bmp, tiff, …).
pub fn attachment_ext(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Webp => Some("webp"),
        _ => None,
    }
}

/// The first line of a queued text, as listed in the composer.
pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// `text` without trailing blanks and leading blank lines; the first line keeps its indent
/// (pasted code, a list).
fn without_blank_edges(text: &str) -> &str {
    let text = text.trim_end();
    let first = text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let start = text[..first].rfind('\n').map_or(0, |newline| newline + 1);
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_box::Outcome;
    use mkrew_protocol::SessionQueueEvent;

    fn type_text(composers: &mut Composers, session: Ulid, text: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                composers.key(session, DialogKey::Enter);
            }
            composers.key(session, DialogKey::Insert(line.into()));
        }
    }

    #[test]
    fn drafts_survive_hiding_and_switching_sessions() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut composers = Composers::default();
        assert_eq!(composers.shown(a), None);
        composers.open(a);
        type_text(&mut composers, a, "first\nsecond");
        composers.open(b);
        type_text(&mut composers, b, "other");
        assert_eq!(composers.key(a, DialogKey::Cancel), Outcome::Cancel);
        composers.hide(a);
        assert_eq!(composers.shown(a), None);
        assert_eq!(composers.shown(b).map(|d| d.text.text()), Some("other"));

        composers.open(a);
        assert_eq!(
            composers.shown(a).map(|d| d.text.text()),
            Some("first\nsecond")
        );
    }

    #[test]
    fn a_multi_line_send_goes_as_one_text_and_empties_the_draft() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        assert_eq!(composers.begin_send(session), None, "nothing typed");
        type_text(&mut composers, session, " \n\n  explain\nthis\n");
        assert_eq!(composers.key(session, DialogKey::Save), Outcome::Submit);
        assert_eq!(
            composers.begin_send(session),
            Some(SessionSendParams {
                session_id: session,
                text: "  explain\nthis".into(),
                submit: true,
            })
        );
        assert_eq!(composers.begin_send(session), None, "already sending");
        composers.key(session, DialogKey::Insert("x".into()));
        composers.insert(session, "y");
        assert_eq!(
            composers.shown(session).map(|d| d.text.text()),
            Some(" \n\n  explain\nthis\n"),
            "no edits while sending"
        );

        composers.sent(session, Ok(()));
        let draft = composers.shown(session).unwrap();
        assert_eq!((draft.text.text(), draft.busy), ("", false));
    }

    #[test]
    fn a_failed_send_keeps_the_draft_until_edited() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        composers.insert(session, "hi");
        composers.begin_send(session);
        composers.sent(session, Err("session exited".into()));
        let draft = composers.shown(session).unwrap();
        assert_eq!(draft.text.text(), "hi");
        assert_eq!(draft.error.as_deref(), Some("Cannot send: session exited"));

        composers.key(session, DialogKey::Insert("!".into()));
        assert_eq!(composers.shown(session).unwrap().error, None);
    }

    #[test]
    fn file_refs_are_relative_slashed_and_quoted_with_spaces() {
        assert_eq!(file_ref("@{path}", "src/main.rs"), "@src/main.rs");
        assert_eq!(file_ref("@{path}", "src\\ui\\app.rs"), "@src/ui/app.rs");
        assert_eq!(
            file_ref("@{path}", "docs/my notes.md"),
            "@\"docs/my notes.md\""
        );
        assert_eq!(file_ref("{path}", "a b\\c.txt"), "\"a b/c.txt\"");
        assert_eq!(file_ref("see {path} ", "x.rs"), "see x.rs ");
    }

    #[test]
    fn typed_paths_are_space_separated_and_quoted_with_whitespace() {
        assert_eq!(typed_paths([]), "");
        assert_eq!(typed_paths(["C:\\work\\a.png"]), "C:\\work\\a.png");
        assert_eq!(
            typed_paths(["C:\\my docs\\a.txt", "/tmp/b.rs", "/x/tab\there"]),
            "\"C:\\my docs\\a.txt\" /tmp/b.rs \"/x/tab\there\""
        );
        assert!(
            !typed_paths(["/a", "/b c"]).contains('\n'),
            "never an Enter"
        );
    }

    #[test]
    fn only_images_the_daemon_stores_get_an_attachment_ext() {
        assert_eq!(attachment_ext(ImageFormat::Png), Some("png"));
        assert_eq!(attachment_ext(ImageFormat::Jpeg), Some("jpg"));
        assert_eq!(attachment_ext(ImageFormat::Gif), Some("gif"));
        assert_eq!(attachment_ext(ImageFormat::Webp), Some("webp"));
        assert_eq!(attachment_ext(ImageFormat::Svg), None);
        assert_eq!(attachment_ext(ImageFormat::Bmp), None);
    }

    #[test]
    fn selection_prefills_cite_the_lines_then_fence_the_text() {
        assert_eq!(
            selection_prefill(Some(("src\\ui\\app.rs", 3, 5)), "    let a = 1;\n    a\n\n"),
            "`src/ui/app.rs:3-5`\n```\n    let a = 1;\n    a\n```\n"
        );
        assert_eq!(
            selection_prefill(Some(("a.md", 7, 7)), "x"),
            "`a.md:7-7`\n```\nx\n```\n"
        );
        // A terminal: the fenced block only.
        assert_eq!(selection_prefill(None, "\n$ ls\n"), "```\n$ ls\n```\n");
        // A fence inside the text gets a longer one around it.
        assert_eq!(
            selection_prefill(None, "```rs\nfn f() {}\n```"),
            "````\n```rs\nfn f() {}\n```\n````\n"
        );
    }

    #[test]
    fn queues_follow_session_queue_events() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        assert!(composers.queue(session).is_empty());
        let items = vec![
            QueuedSend {
                send_id: Ulid::new(),
                text: "fix it\nplease".into(),
            },
            QueuedSend {
                send_id: Ulid::new(),
                text: "then test".into(),
            },
        ];
        let event = |items: Vec<QueuedSend>| {
            Event::SessionQueue(SessionQueueEvent {
                session_id: session,
                items,
            })
        };
        assert!(composers.apply(&event(items.clone())));
        assert!(!composers.apply(&event(items.clone())), "unchanged");
        assert_eq!(composers.queue(session), items.as_slice());
        assert_eq!(first_line(&composers.queue(session)[0].text), "fix it");

        assert!(composers.apply(&event(items[1..].to_vec())));
        assert_eq!(composers.queue(session).len(), 1);
        assert!(
            composers.apply(&event(Vec::new())),
            "emptied (sent or exited)"
        );
        assert!(composers.queue(session).is_empty());
        assert!(!composers.apply(&event(Vec::new())));
        assert_eq!(first_line(""), "");
    }
}
