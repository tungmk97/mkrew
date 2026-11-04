//! The prompt composer docked at an agent pane's bottom (SPEC §20B.1): one draft per session,
//! kept in UI memory while the composer is hidden or another pane has the keyboard, sent with
//! `session.send` + Enter, with the session's send queue (§20B.2). Its prompt editing (`@`
//! files, library prompts, ✦) is shared with the task dialog's prompt ([`Prompt`],
//! [`Target`]). Pure and tested headlessly; `main.rs` renders it.

use std::collections::HashMap;

use gpui::ImageFormat;
use mkrew_protocol::{ErrorBody, ErrorCode, Event, QueuedSend, SessionSendParams};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::{self, Input, TextBox};

/// A prompt field the pickers type into: a session's composer draft or the task dialog's
/// prompt. Both take `@` files, library prompts and ✦ the same way (§20B.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The session's composer draft.
    Composer(Ulid),
    /// The open task dialog's prompt.
    TaskDialog,
}

/// A prompt being written and its ✦ rewrite (SPEC §20B.1): the composer's draft and the task
/// dialog's prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    pub text: TextBox,
    /// The `ai.generate` rewriting the text, by id; typing waits for it.
    pub enhancing: Option<Ulid>,
    /// Text inserted (picked prompt or file, selection, paste) while rewriting; added after
    /// the answer so nothing is lost.
    held: String,
}

impl Prompt {
    pub fn new(text: &str) -> Self {
        let mut prompt = Self::default();
        prompt.text.insert(text);
        prompt
    }

    /// The agent rewrites it now: keys wait for the answer.
    pub fn rewriting(&self) -> bool {
        self.enhancing.is_some()
    }

    /// Types `text` at the caret; held until the answer while rewriting.
    pub fn insert(&mut self, text: &str) {
        if self.rewriting() {
            self.held.push_str(text);
        } else {
            self.text.insert(text);
        }
    }

    /// Starts rewriting as generation `generation_id`; returns the text to improve, or `None`
    /// while rewriting or when only blanks were typed.
    pub fn begin_enhance(&mut self, generation_id: Ulid) -> Option<String> {
        if self.rewriting() || self.text.is_blank() {
            return None;
        }
        self.enhancing = Some(generation_id);
        Some(without_blank_edges(self.text.text()).to_owned())
    }

    /// The rewrite's answer: the new text replaces the old, a failure keeps it; text inserted
    /// meanwhile follows either way. Returns why it failed, to show.
    pub fn enhanced(&mut self, result: Result<String, String>) -> Option<String> {
        self.enhancing = None;
        let failure = match result {
            Ok(text) => {
                self.text.clear();
                self.text.insert(&text);
                None
            }
            Err(error) => Some(format!("Cannot improve the prompt: {error}")),
        };
        self.text.insert(&std::mem::take(&mut self.held));
        failure
    }
}

/// One session's composer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub prompt: Prompt,
    /// Docked under the pane (Esc hides it, the draft stays).
    pub shown: bool,
    /// A `session.send` is in flight.
    pub busy: bool,
    /// Why the last send or rewrite failed.
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

    /// The picker typing `key` opens in `session`'s composer (§20B.1); none while it is hidden
    /// or sending.
    pub fn trigger(&self, session: Ulid, key: &DialogKey) -> Option<Trigger> {
        let draft = self.shown(session).filter(|draft| !draft.busy)?;
        trigger(key, draft.prompt.text.input())
    }

    /// Applies a key typed in `session`'s composer (ignored while sending or rewriting, except
    /// Esc).
    pub fn key(&mut self, session: Ulid, key: DialogKey) -> text_box::Outcome {
        let Some(draft) = self.drafts.get_mut(&session) else {
            return text_box::Outcome::Edited;
        };
        match key {
            DialogKey::Cancel => text_box::Outcome::Cancel,
            _ if draft.busy || draft.prompt.rewriting() => text_box::Outcome::Edited,
            key => {
                let outcome = draft.prompt.text.key(key);
                if outcome == text_box::Outcome::Edited {
                    draft.error = None;
                }
                outcome
            }
        }
    }

    /// Pastes `text` into `session`'s draft (ignored while sending; held until the answer
    /// while rewriting).
    pub fn insert(&mut self, session: Ulid, text: &str) {
        let Some(draft) = self.drafts.get_mut(&session).filter(|d| !d.busy) else {
            return;
        };
        draft.prompt.insert(text);
        draft.error = None;
    }

    /// A click or drag in `session`'s draft: the caret (or selection) goes under the pointer.
    pub fn click(&mut self, session: Ulid, at: usize, extend: bool, clicks: usize) {
        if let Some(draft) = self.drafts.get_mut(&session) {
            draft.prompt.text.click(at, extend, clicks);
        }
    }

    /// Marks `session`'s draft as sending and returns its `session.send` (text + Enter);
    /// `None` while one is in flight or when nothing but blanks was typed.
    pub fn begin_send(&mut self, session: Ulid) -> Option<SessionSendParams> {
        let draft = self.drafts.get_mut(&session).filter(|draft| {
            !draft.busy && !draft.prompt.rewriting() && !draft.prompt.text.is_blank()
        })?;
        draft.busy = true;
        draft.error = None;
        Some(SessionSendParams {
            session_id: session,
            text: without_blank_edges(draft.prompt.text.text()).to_owned(),
            submit: true,
        })
    }

    /// Starts rewriting `session`'s draft as generation `generation_id`; returns the text to
    /// improve, or `None` while sending or rewriting or when only blanks were typed.
    pub fn begin_enhance(&mut self, session: Ulid, generation_id: Ulid) -> Option<String> {
        let draft = self.drafts.get_mut(&session).filter(|draft| !draft.busy)?;
        let input = draft.prompt.begin_enhance(generation_id)?;
        draft.error = None;
        Some(input)
    }

    /// The answer of generation `generation_id`: the rewrite replaces its draft, a failure
    /// keeps the draft and says why. `false` if no draft asked for it.
    pub fn enhanced(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool {
        let Some(draft) = self
            .drafts
            .values_mut()
            .find(|draft| draft.prompt.enhancing == Some(generation_id))
        else {
            return false;
        };
        draft.error = draft.prompt.enhanced(result);
        true
    }

    /// The connection closed: no ✦ answer can come any more, so every rewrite in flight fails
    /// with `error`; the drafts and the text inserted meanwhile stay.
    pub fn abandon_rewrites(&mut self, error: &str) {
        for draft in self.drafts.values_mut() {
            if draft.prompt.rewriting() {
                draft.error = draft.prompt.enhanced(Err(error.to_owned()));
            }
        }
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
            Ok(()) => draft.prompt.text.clear(),
            Err(error) => draft.error = Some(format!("Cannot send: {error}")),
        }
    }
}

/// What a typed char opens in a prompt box (§20B.1, §20B.7): `@` the file picker, `/` the
/// agent's command picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    File,
    Slash,
}

/// Whether a char typed at byte `cursor` of `text` starts something: the cursor is at the start
/// of the text or right after whitespace (a line break included). Anywhere else `@` and `/` are
/// part of a word (`a@b.com`, `src/main.rs`). `text` and `cursor` are as they are before the
/// char goes in; a `cursor` past the end or inside a char is not a start.
pub fn trigger_at(text: &str, cursor: usize) -> bool {
    text.get(..cursor)
        .is_some_and(|before| before.chars().next_back().is_none_or(char::is_whitespace))
}

/// The picker typing `key` opens in a prompt whose text is `input`; `None` = it types as usual.
/// A selection is replaced by the typed char, so the char goes where the selection starts.
pub fn trigger(key: &DialogKey, input: &Input) -> Option<Trigger> {
    let DialogKey::Insert(text) = key else {
        return None;
    };
    let picker = match text.as_str() {
        "@" => Trigger::File,
        "/" => Trigger::Slash,
        _ => return None,
    };
    let at = input
        .selection()
        .map_or(input.cursor(), |selection| selection.start);
    trigger_at(input.as_str(), at).then_some(picker)
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

/// What a tree row dropped on a pane types (§20B.5, ADR 0081): `composer.file_ref` on an
/// agent pane, the bare path on a shell (it has no `@`); a folder ends with `/`. One space
/// after it keeps two drops in a row apart; never an Enter.
pub fn dropped_ref(template: &str, path: &str, is_dir: bool, agent: bool) -> String {
    let path = if is_dir {
        format!("{}/", path.trim_end_matches('/'))
    } else {
        path.to_owned()
    };
    let template = if agent { template } else { "{path}" };
    format!("{} ", file_ref(template, &path))
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

/// What an OS file drop types (§20B.5), on an agent pane or on the task dialog's prompt:
/// the absolute paths as [`typed_paths`] writes them.
pub fn dropped_files(paths: &[std::path::PathBuf]) -> String {
    let paths: Vec<_> = paths.iter().map(|path| path.to_string_lossy()).collect();
    typed_paths(paths.iter().map(|path| &**path))
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

/// A clipboard image as `attachment.save` input (extension, bytes). Windows bitmaps
/// (`CF_DIB`, e.g. PrintScreen) arrive as BMP, which agents do not read: re-encoded as PNG.
pub fn attachment(image: gpui::Image) -> Option<(&'static str, Vec<u8>)> {
    if image.format == ImageFormat::Bmp {
        let decoded =
            image::load_from_memory_with_format(&image.bytes, image::ImageFormat::Bmp).ok()?;
        let mut png = Vec::new();
        decoded
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        return Some(("png", png));
    }
    attachment_ext(image.format).map(|ext| (ext, image.bytes))
}

/// `attachment.save` refuses larger images (§20B.5); not worth sending.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Why a clipboard image of `len` bytes is not sent (a toast); `None` when it fits.
pub fn attachment_refusal(len: usize) -> Option<String> {
    (len > MAX_ATTACHMENT_BYTES).then(|| {
        format!(
            "The image is over {} MiB, so it was not attached.",
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        )
    })
}

/// The toast for a failed `session.unqueue`. `None` for `not_found`: the text was written
/// meanwhile and the `session.queue` event already dropped it from the list.
pub fn unqueue_failure(body: &ErrorBody) -> Option<String> {
    (body.code != ErrorCode::NotFound)
        .then(|| format!("Cannot remove the queued text: {}", body.message))
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
    use crate::editor::Motion;
    use crate::text_box::Input;
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
    fn a_bmp_clipboard_image_is_stored_as_png() {
        let mut bmp = Vec::new();
        image::RgbImage::new(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageFormat::Bmp)
            .unwrap();
        let (ext, bytes) = attachment(gpui::Image::from_bytes(ImageFormat::Bmp, bmp)).unwrap();
        assert_eq!(ext, "png");
        assert!(bytes.starts_with(b"\x89PNG"));
        let png = gpui::Image::from_bytes(ImageFormat::Png, b"png".to_vec());
        assert_eq!(attachment(png), Some(("png", b"png".to_vec())));
        let svg = gpui::Image::from_bytes(ImageFormat::Svg, b"<svg/>".to_vec());
        assert_eq!(attachment(svg), None);
    }

    /// Text that arrives while ✦ rewrites (a picked prompt or file, a Docs selection, a
    /// paste) is never lost: it follows the rewrite, or the kept draft on a failure.
    #[test]
    fn text_inserted_during_a_rewrite_is_kept_after_it() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        composers.insert(session, "fix bug");
        let id = Ulid::new();
        composers.begin_enhance(session, id);
        composers.insert(session, " `src/a.rs`");
        assert_eq!(
            composers.shown(session).unwrap().prompt.text.text(),
            "fix bug"
        );
        assert!(composers.enhanced(id, Ok("Fix the crash".into())));
        assert_eq!(
            composers.shown(session).unwrap().prompt.text.text(),
            "Fix the crash `src/a.rs`"
        );

        let failed = Ulid::new();
        composers.begin_enhance(session, failed);
        composers.insert(session, "!");
        composers.enhanced(failed, Err("no".into()));
        assert_eq!(
            composers.shown(session).unwrap().prompt.text.text(),
            "Fix the crash `src/a.rs`!"
        );
    }

    #[test]
    fn an_improved_prompt_replaces_only_its_draft() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut composers = Composers::default();
        for session in [a, b] {
            composers.open(session);
            composers.insert(session, "fix bug");
        }
        let id = Ulid::new();
        assert_eq!(composers.begin_enhance(a, id).as_deref(), Some("fix bug"));
        assert_eq!(
            composers.begin_enhance(a, Ulid::new()),
            None,
            "one rewrite at a time"
        );
        composers.key(a, DialogKey::Insert("!".into()));
        composers.key(a, DialogKey::Backspace);
        assert!(
            composers.begin_send(a).is_none(),
            "no send while the agent rewrites"
        );
        assert!(composers.enhanced(id, Ok("Fix the crash in parse()".into())));
        assert_eq!(
            composers.shown(a).unwrap().prompt.text.text(),
            "Fix the crash in parse()"
        );
        assert_eq!(composers.shown(b).unwrap().prompt.text.text(), "fix bug");
        assert!(!composers.enhanced(id, Ok("again".into())), "answered once");
    }

    #[test]
    fn a_failed_rewrite_keeps_the_draft_and_blank_drafts_are_not_rewritten() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        assert_eq!(
            composers.begin_enhance(session, Ulid::new()),
            None,
            "nothing typed"
        );
        composers.insert(session, "add tests");
        let id = Ulid::new();
        assert!(composers.begin_enhance(session, id).is_some());
        assert!(composers.enhanced(id, Err("the agent did not answer within 120 s".into())));
        let draft = composers.shown(session).unwrap();
        assert_eq!(draft.prompt.text.text(), "add tests");
        assert_eq!(
            draft.error.as_deref(),
            Some("Cannot improve the prompt: the agent did not answer within 120 s")
        );
        assert_eq!(draft.prompt.enhancing, None);
    }

    /// A lost connection never delivers the ✦ answer: the waiting draft takes keys again,
    /// keeps its text and what was inserted meanwhile, and says why.
    #[test]
    fn a_lost_connection_ends_the_rewrites_in_flight() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut composers = Composers::default();
        for session in [a, b] {
            composers.open(session);
            composers.insert(session, "fix bug");
        }
        composers.begin_enhance(a, Ulid::new());
        composers.insert(a, " `src/a.rs`");
        composers.abandon_rewrites("the connection closed");
        let draft = composers.shown(a).unwrap();
        assert_eq!(draft.prompt.text.text(), "fix bug `src/a.rs`");
        assert_eq!(
            draft.error.as_deref(),
            Some("Cannot improve the prompt: the connection closed")
        );
        composers.key(a, DialogKey::Insert("!".into()));
        assert_eq!(
            composers.shown(a).unwrap().prompt.text.text(),
            "fix bug `src/a.rs`!"
        );
        assert_eq!(composers.shown(b).unwrap().error, None, "not rewriting");
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
        assert_eq!(
            composers.shown(b).map(|d| d.prompt.text.text()),
            Some("other")
        );

        composers.open(a);
        assert_eq!(
            composers.shown(a).map(|d| d.prompt.text.text()),
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
            composers.shown(session).map(|d| d.prompt.text.text()),
            Some(" \n\n  explain\nthis\n"),
            "no edits while sending"
        );

        composers.sent(session, Ok(()));
        let draft = composers.shown(session).unwrap();
        assert_eq!((draft.prompt.text.text(), draft.busy), ("", false));
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
        assert_eq!(draft.prompt.text.text(), "hi");
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
    fn dropped_refs_follow_the_pane_kind_and_mark_folders() {
        assert_eq!(
            dropped_ref("@{path}", "src/main.rs", false, true),
            "@src/main.rs "
        );
        assert_eq!(dropped_ref("@{path}", "src", true, true), "@src/ ");
        assert_eq!(
            dropped_ref("@{path}", "src/main.rs", false, false),
            "src/main.rs "
        );
        assert_eq!(
            dropped_ref("@{path}", "docs/my notes", true, false),
            "\"docs/my notes/\" "
        );
        assert!(
            !dropped_ref("@{path}", "a.rs", false, true).contains('\n'),
            "never an Enter"
        );
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

    /// A drop on the task dialog types what a drop on a pane types (§20B.5): absolute paths,
    /// a path with whitespace quoted, separated by one space, no Enter, no trailing space.
    #[test]
    fn dropped_files_type_their_paths_like_a_pane_drop() {
        use std::path::PathBuf;
        let paths = [
            PathBuf::from("/tmp/a.png"),
            PathBuf::from(r"C:\Users\Nguyen Tung\Pictures\ảnh 1.png"),
        ];
        assert_eq!(
            dropped_files(&paths),
            "/tmp/a.png \"C:\\Users\\Nguyen Tung\\Pictures\\ảnh 1.png\""
        );
        assert_eq!(dropped_files(&paths[..1]), "/tmp/a.png");
        assert_eq!(dropped_files(&[]), "");
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

    #[test]
    fn an_image_over_the_daemon_limit_is_refused_with_a_reason() {
        assert_eq!(attachment_refusal(0), None);
        assert_eq!(
            attachment_refusal(MAX_ATTACHMENT_BYTES),
            None,
            "the limit itself fits"
        );
        assert_eq!(
            attachment_refusal(MAX_ATTACHMENT_BYTES + 1).as_deref(),
            Some("The image is over 10 MiB, so it was not attached.")
        );
    }

    #[test]
    fn a_text_written_meanwhile_is_not_an_unqueue_failure() {
        use mkrew_protocol::{ErrorBody, ErrorCode};
        let body = |code, message: &str| ErrorBody {
            code,
            message: message.into(),
        };
        // The `session.queue` event already dropped it: nothing to tell the user.
        assert_eq!(
            unqueue_failure(&body(
                ErrorCode::NotFound,
                "send 01 is not queued (already written?)"
            )),
            None
        );
        assert_eq!(
            unqueue_failure(&body(ErrorCode::InvalidState, "session exited")).as_deref(),
            Some("Cannot remove the queued text: session exited")
        );
    }

    /// A `/` in a path, a URL or "and/or", and an `@` in an address, are typed as they are.
    #[test]
    fn a_trigger_needs_a_line_start_or_whitespace_before_it() {
        for (text, cursor, opens) in [
            ("", 0, true),
            ("fix ", 4, true),
            ("fix\n", 4, true),
            ("fix\t", 4, true),
            ("a b", 2, true),
            ("héllo ", "héllo ".len(), true),
            ("src", 3, false),
            ("see http:", 9, false),
            ("a@b", 3, false),
            ("ab", 1, false),
            ("héllo", "héllo".len(), false),
            // Never a panic: past the end, or inside a multi-byte char.
            ("ab", 5, false),
            ("é", 1, false),
        ] {
            assert_eq!(trigger_at(text, cursor), opens, "{text:?} at {cursor}");
        }
    }

    #[test]
    fn a_trigger_is_picked_by_the_typed_char_and_a_selection_moves_the_spot() {
        let (at, slash) = (DialogKey::Insert("@".into()), DialogKey::Insert("/".into()));
        let mut input = Input::default();
        assert_eq!(trigger(&at, &input), Some(Trigger::File));
        assert_eq!(trigger(&slash, &input), Some(Trigger::Slash));
        assert_eq!(trigger(&DialogKey::Insert("a".into()), &input), None);
        assert_eq!(trigger(&DialogKey::Backspace, &input), None);

        input.insert("src");
        assert_eq!(trigger(&slash, &input), None, "`src/` is a path");
        input.insert(" b");
        assert_eq!(trigger(&slash, &input), None, "after `b`");
        // `b` selected: the typed char replaces it, so the spot is after the space.
        input.edit(&DialogKey::Move {
            motion: Motion::Left,
            extend: true,
        });
        assert_eq!(input.selected_text(), Some("b"));
        assert_eq!(trigger(&slash, &input), Some(Trigger::Slash));
    }

    #[test]
    fn a_composer_trigger_needs_a_shown_idle_draft() {
        let session = Ulid::new();
        let slash = DialogKey::Insert("/".into());
        let mut composers = Composers::default();
        assert_eq!(composers.trigger(session, &slash), None, "no draft");
        composers.open(session);
        assert_eq!(composers.trigger(session, &slash), Some(Trigger::Slash));
        composers.insert(session, "fix");
        assert_eq!(composers.trigger(session, &slash), None, "after a word");
        composers.insert(session, " ");
        assert_eq!(composers.trigger(session, &slash), Some(Trigger::Slash));

        composers.begin_send(session);
        assert_eq!(composers.trigger(session, &slash), None, "sending");
        composers.sent(session, Err("session exited".into()));
        composers.hide(session);
        assert_eq!(composers.trigger(session, &slash), None, "hidden");
    }
}
