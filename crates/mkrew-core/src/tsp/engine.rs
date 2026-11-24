//! The per-session TSP engine the daemon owns (spec 2026-11-23 sections 6.2 to 6.4).
//!
//! Pure: PTY output goes in with a timestamp, a list of [`Step`]s comes out, in the order the daemon must carry them out.
//! The engine owns the splitter, one [`Document`] per open surface and the inline blobs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use mkrew_protocol::{TspBlob, TspChange, TspNode, TspSnapshot, TspState, TspSurface};
use serde_json::{Value, json};

#[cfg(test)]
use super::doc::TRIM_ID;
use super::doc::{Applied, Document, MAX_NODES, MAX_TEXT};
use super::wire::{self, Incoming, Piece, Splitter};

/// How long after the first output a hello may take before the pane falls back to the terminal grid.
/// Resuming a session and several omp starting together take over 2 s to send it; the pane shows its
/// Starting view meanwhile, and only a terminal layer that drops TSP pays the whole wait.
pub const HELLO_WAIT: Duration = Duration::from_secs(10);

/// After a hello-wait fallback the engine keeps listening for a late hello this long (from the first output), then the fallback is final.
pub const PROVISIONAL_WAIT: Duration = Duration::from_secs(60);

/// Why the hello timer made the engine fall back; the UI words its toast differently for this (provisional) reason.
pub const NO_REPLY_REASON: &str = "no TSP reply from the terminal layer";

/// Why an answered hello that never opened a surface falls back, final.
const NO_SURFACE_REASON: &str = "omp opened no surface";
const LAST_CLOSED_REASON: &str = "omp closed its last surface";

/// Open surfaces kept. With the document bounds this keeps a `tsp.snapshot` under the 16 MiB frame
/// (see `MAX_TEXT`); the oldest read-only one is dropped first. omp opens one surface, plus a screen or two.
const MAX_SURFACES: usize = 3;

/// The kinds advertised in the hello (spec 7.1).
pub const P1_KINDS: &[&str] = &[
    "col",
    "row",
    "card",
    "section",
    "rule",
    "spacer",
    "overlay",
    "toast",
    "text",
    "md",
    "code",
    "ansi",
    "rows",
    "shimmer",
    "kv",
    "table",
    "list",
    "item",
    "tabs",
    "badge",
    "kbd",
    "icon",
    "status",
    "seg",
    "spinner",
    "elapsed",
    "progress",
    "rate",
    "meter",
    "effort",
    "editor",
    "input",
    "image",
    "checklist",
];

/// The events a client may send in P1 (spec 7.4).
pub const P1_EVENTS: &[&str] = &["action", "toggle", "select", "activate", "focus", "theme"];

/// Largest body the engine writes; the hello's `apc` field tells omp the same number.
const APC_LIMIT: usize = 65_536;

/// Inline blob bytes the engine keeps (the oldest are dropped past it).
pub const MAX_BLOB_BYTES: usize = 4 * 1024 * 1024;

/// What the daemon must do, in this order.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // transient: steps live in one Vec per chunk, and the contract names the unboxed variant
pub enum Step {
    /// Bytes for the emulator, the session log and the clients, as today.
    Plain(Vec<u8>),
    /// Bytes for the PTY input (an encoded `r` or `e` message).
    Reply(Vec<u8>),
    /// A `tsp.update` event to broadcast.
    Update(TspChangeFor),
    /// The session's TSP state changed.
    State(TspState),
    /// A `warn` log line; after a fall back it is also the reason shown to the user.
    Warn(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TspChangeFor {
    pub surface: String,
    pub change: TspChange,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("the event is not a JSON object with a string `ev`")]
    Malformed,
    #[error("the event `{0}` is not sent in P1")]
    Unsupported(String),
    #[error("a theme event needs a boolean `dark`")]
    BadTheme,
    #[error(transparent)]
    Wire(#[from] wire::WireError),
}

struct Surface {
    /// `root` is stale here; [`Engine::snapshot`] and the `open` update fill it from the document.
    meta: TspSurface,
    doc: Document,
    /// Order of opening, to find the oldest.
    opened: u64,
}

pub struct Engine {
    splitter: Splitter,
    surfaces: BTreeMap<String, Surface>,
    blobs: BTreeMap<String, TspBlob>,
    /// Blob ids, oldest first, and their total size.
    blob_order: VecDeque<String>,
    blob_bytes: usize,
    state: TspState,
    /// A supported hello was answered.
    hello_ok: bool,
    cols: u16,
    dark: bool,
    first_output: Option<Instant>,
    /// `(max_nodes, max_text)` handed to every new document.
    limits: (usize, usize),
    /// Inline blobs kept in memory; the oldest are dropped past this many bytes.
    max_blob_bytes: usize,
    /// Why the engine fell back, for the UI's toast (`TspSnapshot.reason`).
    fallback_reason: Option<String>,
    /// The fallback came from the hello timer only: a hello that still arrives is answered and the
    /// engine goes Live. Every other fallback is final.
    provisional: bool,
    /// Surfaces opened so far, for [`Surface::opened`].
    next_open: u64,
}

impl Engine {
    pub fn new(cols: u16, dark: bool) -> Self {
        Self {
            splitter: Splitter::new(),
            surfaces: BTreeMap::new(),
            blobs: BTreeMap::new(),
            blob_order: VecDeque::new(),
            blob_bytes: 0,
            state: TspState::Waiting,
            hello_ok: false,
            cols,
            dark,
            first_output: None,
            limits: (MAX_NODES, MAX_TEXT),
            max_blob_bytes: MAX_BLOB_BYTES,
            fallback_reason: None,
            provisional: false,
            next_open: 0,
        }
    }

    /// Tests force eviction with a tiny limit.
    #[cfg(test)]
    pub(crate) fn with_limits(cols: u16, dark: bool, max_nodes: usize, max_text: usize) -> Self {
        Self {
            limits: (max_nodes, max_text),
            ..Self::new(cols, dark)
        }
    }

    pub fn state(&self) -> TspState {
        self.state
    }

    /// Fallen back for good: the daemon copies output as plain and the engine answers nothing.
    pub fn is_passthrough(&self) -> bool {
        self.state == TspState::Fallback && !self.provisional
    }

    /// Splits output; answers hello (Reply before any later Plain), applies frames (Update then ack Reply), opens and closes surfaces, evicts.
    /// After a final fallback everything passes as Plain (no replies); after the timer's, a late hello is still answered.
    pub fn on_output(&mut self, now: Instant, bytes: &[u8]) -> Vec<Step> {
        if bytes.is_empty() {
            return Vec::new();
        }
        if self.is_passthrough() {
            return vec![Step::Plain(bytes.to_vec())];
        }
        self.first_output.get_or_insert(now);
        let mut steps = Vec::new();
        for piece in self.splitter.feed(bytes) {
            if self.is_passthrough() {
                // The engine fell back inside this chunk: TSP messages still queued are dropped, plain bytes go on.
                if let Piece::Plain(b) = piece {
                    steps.push(Step::Plain(b));
                }
                continue;
            }
            match piece {
                Piece::Plain(b) => steps.push(Step::Plain(b)),
                Piece::Message(raw) => match wire::decode(&raw) {
                    Ok(msg) => self.handle(msg, &mut steps),
                    Err(e) => steps.push(Step::Warn(format!("tsp: message dropped: {e}"))),
                },
            }
        }
        let dropped = self.splitter.take_dropped();
        if dropped > 0 {
            steps.push(Step::Warn(format!(
                "tsp: {dropped} oversized message(s) dropped"
            )));
        }
        if self.is_passthrough() {
            self.pass_pending(&mut steps);
        }
        steps
    }

    /// The splitter's held tail was plain output after all; it must not be lost when TSP stops.
    fn pass_pending(&mut self, steps: &mut Vec<Step>) {
        let tail = self.splitter.take_pending();
        if !tail.is_empty() {
            steps.push(Step::Plain(tail));
        }
    }

    /// When to call [`Engine::on_deadline`]: the hello wait while no hello was answered, then the end of the provisional window; both count from the first output.
    /// An answered hello without a surface yet has the same end: omp must open one by then.
    pub fn deadline(&self) -> Option<Instant> {
        let first = self.first_output?;
        match (self.state, self.provisional) {
            (TspState::Waiting, _) if !self.hello_ok => Some(first + HELLO_WAIT),
            (TspState::Fallback, true) => Some(first + PROVISIONAL_WAIT),
            (TspState::Waiting | TspState::Live, _) if self.surfaces.is_empty() => {
                Some(first + PROVISIONAL_WAIT)
            }
            _ => None,
        }
    }

    /// Waiting -> provisional Fallback once the hello wait passed (a late hello still wins; held bytes stay held
    /// so a hello split across the deadline still parses). Past `PROVISIONAL_WAIT` the fallback turns final and
    /// the held bytes come out as plain; so does an answered hello that never got a surface.
    pub fn on_deadline(&mut self, now: Instant) -> Vec<Step> {
        let mut steps = Vec::new();
        if self.deadline().is_some_and(|d| now >= d) {
            if self.provisional {
                self.provisional = false;
                self.pass_pending(&mut steps);
            } else if self.hello_ok {
                self.fall_back(NO_SURFACE_REASON.into(), &mut steps);
                self.pass_pending(&mut steps);
            } else {
                self.fall_back(NO_REPLY_REASON.into(), &mut steps);
                self.provisional = true;
            }
        }
        steps
    }

    /// The `e resize` bytes while a surface is live; the new width is kept for the next hello either way.
    pub fn resize(&mut self, cols: u16) -> Option<Vec<u8>> {
        self.cols = cols;
        if self.state != TspState::Live {
            return None;
        }
        wire::encode(b'e', &json!({"ev": "resize", "cols": cols}), APC_LIMIT).ok()
    }

    /// Validates `ev` against [`P1_EVENTS`], sets `sf`, encodes; `theme` also updates `dark`.
    /// The caller forwards the bytes only while the session is live.
    pub fn event(&mut self, surface: &str, mut event: Value) -> Result<Vec<u8>, EngineError> {
        let ev = event
            .get("ev")
            .and_then(Value::as_str)
            .ok_or(EngineError::Malformed)?
            .to_owned();
        if !P1_EVENTS.contains(&ev.as_str()) {
            return Err(EngineError::Unsupported(ev));
        }
        if ev == "theme" {
            self.dark = event
                .get("dark")
                .and_then(Value::as_bool)
                .ok_or(EngineError::BadTheme)?;
        }
        event["sf"] = json!(surface);
        Ok(wire::encode(b'e', &event, APC_LIMIT)?)
    }

    /// Surfaces plus the blobs some `image` node still names.
    pub fn snapshot(&self) -> TspSnapshot {
        let surfaces: Vec<TspSurface> = self
            .surfaces
            .values()
            .map(|s| TspSurface {
                root: s.doc.to_tree(),
                ..s.meta.clone()
            })
            .collect();
        let mut used = BTreeSet::new();
        for s in &surfaces {
            referenced(&s.root, &mut used);
        }
        let blobs = self
            .blobs
            .values()
            .filter(|b| used.contains(b.id.as_str()))
            .cloned()
            .collect();
        TspSnapshot {
            state: self.state,
            surfaces,
            blobs,
            reason: self.fallback_reason.clone(),
        }
    }

    /// Keeps the newest blobs: past `max_blob_bytes` the oldest go first.
    fn store_blob(&mut self, blob: TspBlob) {
        let id = blob.id.clone();
        self.blob_bytes += blob.data.len();
        if let Some(old) = self.blobs.insert(id.clone(), blob) {
            self.blob_bytes -= old.data.len();
            self.blob_order.retain(|k| *k != id);
        }
        self.blob_order.push_back(id);
        while self.blob_bytes > self.max_blob_bytes {
            let Some(oldest) = self.blob_order.pop_front() else {
                break;
            };
            if let Some(gone) = self.blobs.remove(&oldest) {
                self.blob_bytes -= gone.data.len();
            }
        }
    }

    fn handle(&mut self, msg: Incoming, steps: &mut Vec<Step>) {
        if !self.hello_ok && !matches!(msg, Incoming::Hello { .. }) {
            return; // omp does not send anything else before the reply; ignore stray messages
        }
        match msg {
            Incoming::Hello { versions, .. } => self.hello(&versions, steps),
            Incoming::BlobsQuery { ids } => {
                let have: Vec<&String> = ids
                    .iter()
                    .filter(|id| self.blobs.contains_key(*id))
                    .collect();
                push_reply(steps, b'r', &json!({"r": "blobs", "have": have}));
            }
            Incoming::Open(meta) => self.open(meta, steps),
            Incoming::Close { surface, keep } => self.close(&surface, keep, steps),
            Incoming::Frame { surface, seq, ops } => self.frame(&surface, seq, &ops, steps),
            Incoming::Blob(blob) => self.store_blob(blob),
            // The palette is ignored (spec 7.2); unknown verbs are ignored (spec 6.1).
            Incoming::Theme | Incoming::Unknown { .. } => {}
        }
    }

    fn hello(&mut self, versions: &[u64], steps: &mut Vec<Step>) {
        if !versions.contains(&1) {
            self.fall_back(
                format!("omp offered TSP versions {versions:?}, Mkrew speaks 1"),
                steps,
            );
            return;
        }
        self.hello_ok = true;
        let reply = json!({
            "r": "hello", "v": 1, "term": "mkrew", "kinds": P1_KINDS, "features": [],
            "apc": APC_LIMIT, "credits": 2, "cols": self.cols, "dark": self.dark,
            "reduceMotion": false, "hour12": false
        });
        push_reply(steps, b'r', &reply);
        if self.state == TspState::Fallback {
            // A hello after the timer's fallback: the reply is out, the surface follows.
            self.provisional = false;
            self.fallback_reason = None;
            self.state = TspState::Live;
            steps.push(Step::State(TspState::Live));
        }
    }

    fn open(&mut self, meta: TspSurface, steps: &mut Vec<Step>) {
        let mut doc = Document::new(&meta.id);
        doc.set_limits(self.limits.0, self.limits.1);
        let meta = TspSurface {
            seq: 0,
            read_only: false,
            ..meta
        };
        let shown = TspSurface {
            root: doc.to_tree(),
            ..meta.clone()
        };
        // An existing id is replaced: omp re-opens `s:1` after an adopt, and clients replace too.
        self.surfaces.remove(&meta.id);
        // Past the cap the oldest read-only surface goes, else the oldest: the snapshot stays under the frame limit.
        while self.surfaces.len() >= MAX_SURFACES {
            let Some(victim) = self
                .surfaces
                .iter()
                .min_by_key(|(_, s)| (!s.meta.read_only, s.opened))
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.surfaces.remove(&victim);
            steps.push(Step::Update(TspChangeFor {
                surface: victim,
                change: TspChange::Closed { keep: false },
            }));
        }
        steps.push(Step::Update(TspChangeFor {
            surface: meta.id.clone(),
            change: TspChange::Open { surface: shown },
        }));
        self.next_open += 1;
        let opened = self.next_open;
        self.surfaces
            .insert(meta.id.clone(), Surface { meta, doc, opened });
        if self.state == TspState::Waiting {
            self.state = TspState::Live;
            steps.push(Step::State(TspState::Live));
        }
    }

    fn close(&mut self, id: &str, keep: bool, steps: &mut Vec<Step>) {
        let Some(surface) = self.surfaces.get_mut(id) else {
            steps.push(Step::Warn(format!("tsp: close of unknown surface {id}")));
            return;
        };
        steps.push(Step::Update(TspChangeFor {
            surface: id.to_owned(),
            change: TspChange::Closed { keep },
        }));
        if keep {
            surface.meta.read_only = true;
            return;
        }
        let inline = surface.meta.mode == "inline";
        self.surfaces.remove(id);
        if inline {
            self.fall_back("omp closed its surface and draws rows again".into(), steps);
        } else if self.surfaces.is_empty() {
            self.fall_back(LAST_CLOSED_REASON.into(), steps);
        }
    }

    fn frame(&mut self, id: &str, seq: u64, ops: &[Value], steps: &mut Vec<Step>) {
        let Some(surface) = self.surfaces.get_mut(id) else {
            // Acked anyway: omp holds one list of unacked frames and stalls without the ack (reference 3.4).
            steps.push(Step::Warn(format!(
                "tsp: frame {seq} for unknown surface {id}"
            )));
            push_reply(steps, b'e', &ack(id, seq));
            return;
        };
        let Applied {
            ops: mut applied,
            errors,
        } = surface.doc.apply(ops);
        let (gone, evicted) = surface.doc.evict();
        if surface.doc.over_bounds() {
            // `dock` and `layer` are never evicted; what they hold cannot be shown or mirrored.
            self.fall_back("document over bounds".into(), steps);
            return;
        }
        applied.extend(evicted);
        surface.meta.seq = seq;

        let blobs: Vec<TspBlob> = blob_ids(&applied)
            .into_iter()
            .filter_map(|b| self.blobs.get(b).cloned())
            .collect();
        steps.push(Step::Update(TspChangeFor {
            surface: id.to_owned(),
            change: TspChange::Frame {
                seq,
                ops: applied,
                blobs,
            },
        }));
        // One `error` event per frame, however many ops failed (a late frame naming evicted ids stays one event).
        if !errors.is_empty() {
            let msg = format!("frame {seq}: {}", errors.join("; "));
            push_reply(steps, b'e', &json!({"ev": "error", "msg": msg}));
            steps.push(Step::Warn(format!("tsp {id}: {msg}")));
        }
        if !gone.is_empty() {
            push_reply(steps, b'e', &json!({"ev": "gone", "sf": id, "ids": gone}));
        }
        push_reply(steps, b'e', &ack(id, seq));
    }

    fn fall_back(&mut self, reason: String, steps: &mut Vec<Step>) {
        self.state = TspState::Fallback;
        self.provisional = false;
        self.surfaces.clear();
        self.blobs.clear();
        self.blob_order.clear();
        self.blob_bytes = 0;
        self.fallback_reason = Some(reason.clone());
        steps.push(Step::State(TspState::Fallback));
        steps.push(Step::Warn(reason));
    }
}

fn ack(surface: &str, seq: u64) -> Value {
    json!({"ev": "ack", "sf": surface, "s": seq})
}

/// Ids of the blobs named by `image` nodes in a tree.
fn referenced<'a>(node: &'a TspNode, ids: &mut BTreeSet<&'a str>) {
    if node.k == "image"
        && let Some(id) = node.p.get("blob").and_then(Value::as_str)
    {
        ids.insert(id);
    }
    for child in &node.c {
        referenced(child, ids);
    }
}

fn push_reply(steps: &mut Vec<Step>, verb: u8, body: &Value) {
    match wire::encode(verb, body, APC_LIMIT) {
        Ok(bytes) => steps.push(Step::Reply(bytes)),
        Err(e) => steps.push(Step::Warn(format!("tsp: reply not sent: {e}"))),
    }
}

/// Blob ids named by `image` nodes in `add` ops and by `blob` props in `set` ops, first use first.
fn blob_ids(ops: &[Value]) -> Vec<&str> {
    fn walk<'a>(node: &'a Value, ids: &mut Vec<&'a str>) {
        if node["k"] == "image"
            && let Some(id) = node["p"]["blob"].as_str()
            && !ids.contains(&id)
        {
            ids.push(id);
        }
        for child in node["c"].as_array().into_iter().flatten() {
            walk(child, ids);
        }
    }
    let mut ids = Vec::new();
    for op in ops {
        match op[0].as_str() {
            Some("add") => walk(&op[4], &mut ids),
            Some("set") => {
                if let Some(id) = op[2]["blob"].as_str()
                    && !ids.contains(&id)
                {
                    ids.push(id);
                }
            }
            _ => {}
        }
    }
    ids
}

#[cfg(test)]
mod tests;
