//! The composer field of an omp pane, edited by Mkrew itself (ADR 0113): a normal text field
//! (caret, selection, clipboard, undo) whose changes go to omp as `edit` events, while omp keeps
//! its draft and draws the decorations (skills, mentions) back. Pure; `shell/tsp_view.rs` draws
//! it and routes the keys.

use std::collections::VecDeque;
use std::ops::Range;

use gpui::Keystroke;
use mkrew_core::tsp::doc::{Document, Node};
use serde_json::{Value, json};

use super::{Decor, byte_at_utf16, completing, composer_editor};
use crate::editor::Motion;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

/// Texts sent and not drawn back by omp that are remembered; older ones go first.
const SENT_LIMIT: usize = 64;

/// An editor's text as drawn: the `prompt` lead, then the pieces that are not hidden. Byte offsets
/// of the text map to the drawn string and back (the caret, the selection, a click).
#[derive(Debug, Clone, PartialEq)]
pub struct EditorView {
    /// The drawn string.
    pub shown: String,
    /// Each visible piece: its range in `shown` and its decoration.
    pub pieces: Vec<(Range<usize>, Decor)>,
    map: ShownMap,
}

/// The byte mapping of an [`EditorView`], cheap to clone into a mouse handler.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShownMap {
    lead: usize,
    /// Hidden byte ranges of the text, in order.
    hidden: Vec<Range<usize>>,
    len: usize,
}

impl ShownMap {
    /// Where byte `at` of the text is drawn; inside a hidden range, where that range starts.
    pub fn to_shown(&self, at: usize) -> usize {
        let at = at.min(self.len);
        let gone: usize = (self.hidden.iter())
            .map(|h| at.clamp(h.start, h.end) - h.start)
            .sum();
        self.lead + at - gone
    }

    /// The byte of the text drawn at `shown` (the lead counts as the text's start).
    pub fn to_text(&self, shown: usize) -> usize {
        let mut at = shown.saturating_sub(self.lead);
        for h in &self.hidden {
            if at >= h.start {
                at += h.len();
            }
        }
        at.min(self.len)
    }
}

impl EditorView {
    /// `text` cut by `pieces` ([`super::decor_pieces`]) after `lead`.
    pub fn new(lead: &str, text: &str, pieces: &[(Range<usize>, Decor)]) -> Self {
        let mut shown = lead.to_owned();
        let mut visible = Vec::new();
        let mut hidden = Vec::new();
        for (range, decor) in pieces {
            if decor.hide {
                hidden.push(range.clone());
            } else {
                let start = shown.len();
                shown.push_str(&text[range.clone()]);
                visible.push((start..shown.len(), *decor));
            }
        }
        Self {
            shown,
            pieces: visible,
            map: ShownMap {
                lead: lead.len(),
                hidden,
                len: text.len(),
            },
        }
    }

    pub fn map(&self) -> &ShownMap {
        &self.map
    }
}

/// The `editor` node Mkrew edits itself: the composer's, while omp has its focus there (or says
/// nothing about focus), no modal is open and the editor is not in vim mode. `None`: every key
/// goes to omp through the PTY as in a terminal.
pub fn local_editor_node(doc: &Document) -> Option<&Node> {
    let node = composer_editor(doc)?;
    let modal = doc
        .children("layer")
        .any(|n| n.props.get("hidden").and_then(Value::as_bool) != Some(true));
    let focused = doc.focus().is_none_or(|id| id == node.id);
    let vim = node.props.get("mode").is_some_and(|m| !m.is_null());
    (!modal && focused && !vim).then_some(node)
}

/// The composer's text while Mkrew edits it: the field, and what omp has.
#[derive(Debug, Clone, Default)]
pub struct LocalEditor {
    pub input: Input,
    /// The `editor` node mirrored.
    pub node: String,
    /// The text and caret (bytes) omp has once it applies the edits sent so far.
    told: (String, usize),
    /// The text omp drew last, then the texts sent since, oldest first. omp draws a text again
    /// (its ghost, a decoration) before a later edit lands, so the last one drawn stays.
    drawn: VecDeque<String>,
}

impl LocalEditor {
    /// omp drew `text` with its caret at UTF-16 unit `cursor` in `node`. The text it drew last,
    /// or one sent since, is skipped (the field is ahead of it); any other text is omp's own
    /// change (a completion, the history, a send that emptied it) and replaces the field's, as
    /// one undo step. The first text fills a new field. `true` when omp's text replaced the
    /// field's.
    pub fn sync(&mut self, node: &str, text: &str, cursor: Option<usize>) -> bool {
        let cursor = cursor.map_or(text.len(), |units| byte_at_utf16(text, units));
        if self.node.is_empty() {
            let mut input = Input::from(text);
            input.click(cursor, false, 1);
            *self = Self {
                input,
                node: node.to_owned(),
                told: (text.to_owned(), cursor),
                drawn: VecDeque::from([text.to_owned()]),
            };
            return false;
        }
        // omp may re-key the same editor: the edits sent still apply to it.
        if self.node != node {
            node.clone_into(&mut self.node);
        }
        if let Some(at) = self.drawn.iter().position(|drawn| drawn == text) {
            self.drawn.drain(..at);
            return false;
        }
        self.drawn = VecDeque::from([text.to_owned()]);
        self.told = (text.to_owned(), cursor);
        if text == self.input.as_str() {
            return false;
        }
        self.input.replace_all(text, cursor);
        true
    }

    /// The `edit` event that brings omp from what it has to the field (one splice and the
    /// caret); `None` when omp has both already.
    pub fn edit(&mut self) -> Option<Value> {
        let (text, cursor) = (self.input.as_str(), self.input.cursor());
        if text == self.told.0 && cursor == self.told.1 {
            return None;
        }
        let event = edit_event(&self.node, &self.told.0, text, cursor);
        if text != self.told.0 {
            if self.drawn.len() == SENT_LIMIT {
                self.drawn.pop_front();
            }
            self.drawn.push_back(text.to_owned());
        }
        self.told = (text.to_owned(), cursor);
        Some(event)
    }
}

/// The `edit` event (reference 5) turning `old` into `new`: the changed middle as one splice,
/// the caret at byte `cursor` of `new`, offsets in UTF-16 units of `old` as omp checks them.
pub fn edit_event(id: &str, old: &str, new: &str, cursor: usize) -> Value {
    let prefix = (old.char_indices().zip(new.chars()))
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((at, _), _)| at);
    let mut old_end = old.len();
    let mut new_end = new.len();
    while old_end > prefix && new_end > prefix {
        let (Some(a), Some(b)) = (
            old[..old_end].chars().next_back(),
            new[..new_end].chars().next_back(),
        ) else {
            break;
        };
        if a != b {
            break;
        }
        old_end -= a.len_utf8();
        new_end -= b.len_utf8();
    }
    let units = |s: &str| s.encode_utf16().count();
    json!({
        "ev": "edit",
        "id": id,
        "from": units(&old[..prefix]),
        "to": units(&old[..old_end]),
        "text": &new[prefix..new_end],
        "cursor": units(&new[..cursor.min(new.len())]),
        "len": units(old),
    })
}

/// What a key does in a locally edited composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeKey {
    /// omp's: Enter sends, Tab, Esc, the completion list, the history, its shortcuts.
    Pty,
    /// An edit or a caret move in the field.
    Edit(DialogKey),
    /// The selection goes to the clipboard (Cut also removes it).
    Copy,
    Cut,
    /// The clipboard's text is typed; an image goes to omp (its `#n` attachment).
    Paste,
}

/// The key's job (ADR 0113). `completing`: omp shows its completion list, whose ↑ ↓ are omp's;
/// ↑ on the first line and ↓ on the last are omp's history. Ctrl+C / Ctrl+X without a selection
/// stay omp's (interrupt, clear).
pub fn compose_key(keystroke: &Keystroke, input: &Input, completing: bool) -> ComposeKey {
    let m = keystroke.modifiers;
    if keystroke.key == "enter" && (m.shift || m.alt) && !m.control && !m.platform {
        return ComposeKey::Edit(DialogKey::Insert("\n".into()));
    }
    let Some(key) = dialog_key(keystroke) else {
        return ComposeKey::Pty;
    };
    let (text, at) = (input.as_str(), input.cursor());
    let selected = input.selection().is_some();
    match key {
        DialogKey::Cancel | DialogKey::Save | DialogKey::Enter | DialogKey::SwitchField => {
            ComposeKey::Pty
        }
        DialogKey::Move {
            motion: Motion::Up,
            extend: false,
        } if completing || !text[..at].contains('\n') => ComposeKey::Pty,
        DialogKey::Move {
            motion: Motion::Down,
            extend: false,
        } if completing || !text[at..].contains('\n') => ComposeKey::Pty,
        DialogKey::Copy if selected => ComposeKey::Copy,
        DialogKey::Cut if selected => ComposeKey::Cut,
        DialogKey::Copy | DialogKey::Cut => ComposeKey::Pty,
        DialogKey::Paste => ComposeKey::Paste,
        key => ComposeKey::Edit(key),
    }
}

/// [`compose_key`] with omp's completion state read from `doc`.
pub fn compose_key_in(doc: &Document, keystroke: &Keystroke, input: &Input) -> ComposeKey {
    compose_key(keystroke, input, completing(doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decor(hide: bool) -> Decor {
        Decor {
            hide,
            ..Decor::default()
        }
    }

    #[test]
    fn hidden_text_is_not_drawn_and_offsets_map_both_ways() {
        // omp's bash mode hides the `!` prefix: "!ls -a" draws "ls -a" after a "> " lead.
        let text = "!ls -a";
        let view = EditorView::new("> ", text, &[(0..1, decor(true)), (1..6, decor(false))]);
        assert_eq!(view.shown, "> ls -a");
        assert_eq!(view.pieces, [(2..7, decor(false))]);
        let map = view.map();
        assert_eq!(
            map.to_shown(0),
            2,
            "inside the hidden prefix: where it starts"
        );
        assert_eq!(map.to_shown(1), 2);
        assert_eq!(map.to_shown(6), 7);
        assert_eq!(
            map.to_text(2),
            1,
            "the drawn start is after the hidden prefix"
        );
        assert_eq!(map.to_text(0), 1, "a click on the lead");
        assert_eq!(map.to_text(7), 6);
        assert_eq!(map.to_text(99), 6, "past the end clamps");
    }

    #[test]
    fn an_edit_is_one_splice_in_utf16_units_of_omps_text() {
        // "é" is one UTF-16 unit and two bytes; "😀" is two units.
        let typed = edit_event("e", "é", "éa", 3);
        assert_eq!(
            typed,
            json!({"ev": "edit", "id": "e", "from": 1, "to": 1, "text": "a", "cursor": 2, "len": 1})
        );
        let replaced = edit_event("e", "a😀b", "aXb", 2);
        assert_eq!(
            (
                &replaced["from"],
                &replaced["to"],
                &replaced["text"],
                &replaced["len"]
            ),
            (&json!(1), &json!(3), &json!("X"), &json!(4))
        );
        // A repeated letter: the splice is where the texts differ, never past either end.
        let doubled = edit_event("e", "aa", "aaa", 3);
        assert_eq!(
            (&doubled["from"], &doubled["to"], &doubled["text"]),
            (&json!(2), &json!(2), &json!("a"))
        );
        let cleared = edit_event("e", "abc", "", 0);
        assert_eq!(
            (&cleared["from"], &cleared["to"], &cleared["text"]),
            (&json!(0), &json!(3), &json!(""))
        );
        let moved = edit_event("e", "abc", "abc", 1);
        assert_eq!(
            (&moved["from"], &moved["to"], &moved["cursor"]),
            (&json!(3), &json!(3), &json!(1))
        );
    }

    #[test]
    fn the_field_runs_ahead_of_omp_and_takes_omps_own_changes() {
        let mut ed = LocalEditor::default();
        ed.sync("n", "", None);
        ed.input.insert("h");
        let first = ed.edit().expect("an edit");
        assert_eq!(first["text"], "h");
        ed.input.insert("i");
        assert_eq!(
            ed.edit().expect("an edit")["len"],
            1,
            "based on what omp will have"
        );
        assert_eq!(ed.edit(), None, "nothing new");
        // omp draws the first text late: the field keeps what was typed since.
        ed.sync("n", "h", Some(1));
        assert_eq!(ed.input.as_str(), "hi");
        ed.sync("n", "hi", Some(2));
        assert_eq!(ed.input.as_str(), "hi");
        // A completion accepted in omp: its text replaces the field, caret where omp put it,
        // and Ctrl+Z brings the typed text back.
        ed.sync("n", "/skill:hi ", Some(10));
        assert_eq!((ed.input.as_str(), ed.input.cursor()), ("/skill:hi ", 10));
        assert!(ed.input.undo());
        assert_eq!(ed.input.as_str(), "hi");
        // omp re-keys its editor while edits are on their way: the field keeps what was typed.
        let mut ed = LocalEditor::default();
        ed.sync("n", "hi", None);
        ed.input.insert("!");
        assert!(ed.edit().is_some());
        ed.input.insert("?");
        assert!(ed.edit().is_some());
        ed.sync("m", "hi!", Some(3));
        assert_eq!((ed.node.as_str(), ed.input.as_str()), ("m", "hi!?"));
    }

    /// omp draws a text a second time (its ghost arrives) before the next edit lands: the
    /// letters typed since stay (seen as "docuent", "lgin" when typing fast).
    #[test]
    fn a_text_omp_draws_again_does_not_undo_what_was_typed_since() {
        let mut ed = LocalEditor::default();
        ed.sync("n", "", None);
        ed.sync("n", "", None);
        for c in ["d", "o", "c"] {
            ed.input.insert(c);
            assert!(ed.edit().is_some());
        }
        ed.sync("n", "do", Some(2));
        ed.sync("n", "do", Some(2));
        assert_eq!(ed.input.as_str(), "doc");
        ed.sync("n", "doc", Some(3));
        ed.input.insert("u");
        assert!(ed.edit().is_some());
        ed.sync("n", "doc", Some(3));
        assert_eq!(ed.input.as_str(), "docu");
        // A text omp never had from the field is still omp's own.
        assert!(ed.sync("n", "", Some(0)));
        assert_eq!(ed.input.as_str(), "");
    }

    fn key(spec: &str) -> Keystroke {
        // `cmd-` is the text keys' modifier of the OS: Cmd on macOS, Ctrl elsewhere.
        let mac = if cfg!(target_os = "macos") {
            "cmd-"
        } else {
            "ctrl-"
        };
        Keystroke::parse(&spec.replace("cmd-", mac)).expect("keystroke")
    }

    #[test]
    fn plain_editing_keys_stay_local_and_omps_keys_go_to_it() {
        let input = Input::from("ab\ncd");
        let edit = |spec| compose_key(&key(spec), &input, false);
        assert_eq!(edit("cmd-a"), ComposeKey::Edit(DialogKey::SelectAll));
        assert_eq!(edit("cmd-z"), ComposeKey::Edit(DialogKey::Undo));
        assert_eq!(edit("cmd-v"), ComposeKey::Paste);
        assert_eq!(
            edit("cmd-c"),
            ComposeKey::Pty,
            "nothing selected: omp's interrupt"
        );
        assert_eq!(edit("enter"), ComposeKey::Pty);
        assert_eq!(
            edit("shift-enter"),
            ComposeKey::Edit(DialogKey::Insert("\n".into()))
        );
        assert_eq!(edit("escape"), ComposeKey::Pty);
        assert_eq!(edit("tab"), ComposeKey::Pty);
        assert_eq!(edit("ctrl-l"), ComposeKey::Pty, "omp's shortcuts");
        // The caret is at the end, on the last line: ↑ moves in the text, ↓ is omp's history.
        assert!(matches!(
            edit("up"),
            ComposeKey::Edit(DialogKey::Move { .. })
        ));
        assert_eq!(edit("down"), ComposeKey::Pty);
        assert_eq!(
            compose_key(&key("up"), &input, true),
            ComposeKey::Pty,
            "the completion list"
        );
        let mut all = input.clone();
        all.select_all();
        assert_eq!(compose_key(&key("cmd-c"), &all, false), ComposeKey::Copy);
        assert_eq!(compose_key(&key("cmd-x"), &all, false), ComposeKey::Cut);
    }
}
