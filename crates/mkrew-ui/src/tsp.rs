//! The pure half of the omp native surface in the UI (ADR 0107, spec 7.2): the theme mapping of
//! TSP tokens, tones and icons, the ANSI line parser, the pane state table, and the `Mirror`
//! the UI keeps of the daemon's documents. `shell/tsp_view.rs` draws it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use mkrew_core::tsp::doc::{Document, Node};
use mkrew_protocol::{TspBlob, TspChange, TspSnapshot, TspState, TspSurface, TspUpdate};
use serde_json::{Map, Value, json};

use crate::icons::Icon;
use crate::theme::{self, Theme, space};

/// The one table from omp's span tokens and node tones to theme fields (spec 7.2). `None`: not a
/// color token (a modifier, omp's own palette name, or nothing), the text draws in `theme.text`.
pub fn color(token: &str, theme: &Theme) -> Option<u32> {
    Some(match token {
        "accent" | "info" | "link" | "user" => theme.accent,
        "muted" | "dim" => theme.text_muted,
        "error" => theme.danger_text,
        "warning" | "pending" => theme.status_needs_input,
        "success" => theme.status_done,
        "text" | "neutral" => theme.text,
        _ => return None,
    })
}

/// The foreground of a node `tone`; an unknown tone is plain text.
pub fn tone_color(tone: &str, theme: &Theme) -> u32 {
    color(tone, theme).unwrap_or(theme.text)
}

/// How one span draws. `mono` is the terminal font with the glyph fallbacks (code, paths and the
/// private-use icon glyphs omp forces while a surface is live).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub color: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub mono: bool,
    pub mark: bool,
}

/// A span's space separated `s` tokens (reference 6.1).
pub fn span_style(tokens: &str, theme: &Theme) -> SpanStyle {
    let mut style = SpanStyle::default();
    for token in tokens.split_whitespace() {
        match token {
            "strong" => style.bold = true,
            "em" => style.italic = true,
            "del" => style.strike = true,
            "mark" => style.mark = true,
            "mono" | "code" | "path" | "icon" => style.mono = true,
            _ => {}
        }
        if style.color.is_none() {
            style.color = color(token, theme);
        }
    }
    style
}

/// The Mkrew icon of a TSP icon name; a name without an entry draws nothing.
pub fn icon(name: &str) -> Option<Icon> {
    Some(match name {
        "copy" => Icon::Copy,
        "x" => Icon::X,
        "eye" => Icon::Eye,
        "model" | "wand" => Icon::Sparkles,
        "chev" => Icon::ChevronDown,
        "jobs" | "context.compaction" => Icon::Layers,
        "job" => Icon::Zap,
        "history" => Icon::History,
        "link" => Icon::Link,
        "open" => Icon::ExternalLink,
        "rewind" => Icon::Undo2,
        "check" => Icon::Check,
        "lightbulb" => Icon::Lightbulb,
        "loop" => Icon::RotateCw,
        "back" => Icon::ChevronLeft,
        "users" | "agents" => Icon::Users,
        "agent" => Icon::Bot,
        "bash" => Icon::Terminal,
        "session" => Icon::SquareTerminal,
        "time" => Icon::Clock,
        "cache" => Icon::Database,
        "input" => Icon::ArrowDown,
        "output" => Icon::ArrowUp,
        "folder" => Icon::Folder,
        "plus" => Icon::Plus,
        "wrench" => Icon::Settings,
        "x-circle" => Icon::SquareX,
        "send" => Icon::Send,
        _ => return None,
    })
}

/// A string prop, or an array of `{t, s?}` spans, as styled runs. Entries without `t` are skipped.
pub fn spans_of(value: &Value, theme: &Theme) -> Vec<(String, SpanStyle)> {
    match value {
        Value::String(text) => vec![(text.clone(), SpanStyle::default())],
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let text = item.get("t")?.as_str()?;
                let tokens = item.get("s").and_then(Value::as_str).unwrap_or("");
                Some((text.to_owned(), span_style(tokens, theme)))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The text of a text-like node: its `spans`, else its `text`.
pub fn node_spans(props: &Map<String, Value>, theme: &Theme) -> Vec<(String, SpanStyle)> {
    match props.get("spans").or_else(|| props.get("text")) {
        Some(value) => spans_of(value, theme),
        None => Vec::new(),
    }
}

/// The one muted line an unknown kind draws (spec 7.1): its first non-blank text-like prop.
pub fn generic_line(props: &Map<String, Value>, theme: &Theme) -> Option<String> {
    ["text", "title", "head", "label", "name"]
        .iter()
        .filter_map(|key| props.get(*key))
        .map(|value| {
            spans_of(value, theme)
                .into_iter()
                .map(|(text, _)| text)
                .collect::<String>()
        })
        .find(|line| !line.trim().is_empty())
}

/// A `gap` token in px.
pub fn gap_px(name: Option<&str>) -> f32 {
    match name {
        Some("xs") => space::XS,
        Some("sm") => space::S,
        Some("md") => space::M,
        Some("lg") => space::L,
        _ => 0.0,
    }
}

/// The gap of a `col`: its `gap` token, and at least `space::S` inside an assistant turn
/// (role `omp.assistant*`), where omp sends thinking, text and tool rows with no gap.
pub fn col_gap(role: Option<&str>, gap: Option<&str>) -> f32 {
    let gap = gap_px(gap);
    if role.is_some_and(|role| role.starts_with("omp.assistant")) {
        gap.max(space::S)
    } else {
        gap
    }
}

/// The pen of the SGR parser.
#[derive(Clone, Copy, Default)]
struct Pen {
    fg: Option<u32>,
    bold: bool,
    dim: bool,
    italic: bool,
    strike: bool,
    reverse: bool,
}

impl Pen {
    fn style(self, theme: &Theme) -> SpanStyle {
        SpanStyle {
            color: self.fg.or(self.dim.then_some(theme.text_muted)),
            bold: self.bold,
            italic: self.italic,
            strike: self.strike,
            mono: false,
            mark: self.reverse,
        }
    }
}

/// The color of palette index `n`: 0..16 are the theme's ANSI colors, then the 6x6x6 cube and the
/// 24 grays of xterm.
fn indexed(n: u32, theme: &Theme) -> u32 {
    match n {
        0..=15 => theme.ansi[n as usize],
        16..=231 => {
            let cube = n - 16;
            let level = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
            (level(cube / 36) << 16) | (level(cube % 36 / 6) << 8) | level(cube % 6)
        }
        _ => {
            let gray = (8 + 10 * (n.min(255) - 232)).min(255);
            (gray << 16) | (gray << 8) | gray
        }
    }
}

/// The color after `38` or `48`: `5;n` or `2;r;g;b`. Consumes exactly its own parameters.
fn extended(params: &mut impl Iterator<Item = u32>, theme: &Theme) -> Option<u32> {
    match params.next()? {
        5 => Some(indexed(params.next()?, theme)),
        2 => {
            let (r, g, b) = (params.next()?, params.next()?, params.next()?);
            Some((r.min(255) << 16) | (g.min(255) << 8) | b.min(255))
        }
        _ => None,
    }
}

/// The color of a colon form `38:...` (the part after the `38`): `5:n`, `2:r:g:b` or the ITU
/// `2:<colorspace>:r:g:b`.
fn colon_color(rest: &[u32], theme: &Theme) -> Option<u32> {
    let rgb = |r: u32, g: u32, b: u32| (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
    match *rest {
        [5, n, ..] => Some(indexed(n, theme)),
        [2, r, g, b] | [2, _, r, g, b, ..] => Some(rgb(r, g, b)),
        _ => None,
    }
}

fn sgr(pen: &mut Pen, params: &str, theme: &Theme) {
    if params.is_empty() {
        *pen = Pen::default();
        return;
    }
    let number = |p: &str| p.parse::<u32>().unwrap_or(0);
    let mut groups = params.split(';');
    while let Some(group) = groups.next() {
        // A colon group is one parameter; only its colors mean anything here (`4:3` is a curly
        // underline, not italic).
        if group.contains(':') {
            let parts: Vec<u32> = group.split(':').map(number).collect();
            if let [38, rest @ ..] = parts.as_slice()
                && let Some(color) = colon_color(rest, theme)
            {
                pen.fg = Some(color);
            }
            continue;
        }
        match number(group) {
            0 => *pen = Pen::default(),
            1 => pen.bold = true,
            2 => pen.dim = true,
            3 => pen.italic = true,
            7 => pen.reverse = true,
            9 => pen.strike = true,
            22 => {
                pen.bold = false;
                pen.dim = false;
            }
            23 => pen.italic = false,
            27 => pen.reverse = false,
            29 => pen.strike = false,
            code @ 30..=37 => pen.fg = Some(theme.ansi[(code - 30) as usize]),
            code @ 90..=97 => pen.fg = Some(theme.ansi[(code - 90) as usize + 8]),
            39 => pen.fg = None,
            38 => pen.fg = extended(&mut groups.by_ref().map(number), theme),
            // Backgrounds are dropped (reference 6.1) but their parameters are consumed.
            48 => {
                extended(&mut groups.by_ref().map(number), theme);
            }
            _ => {}
        }
    }
}

fn push_run(runs: &mut Vec<(String, SpanStyle)>, text: &mut String, style: SpanStyle) {
    if text.is_empty() {
        return;
    }
    if let Some((last, last_style)) = runs.last_mut()
        && *last_style == style
    {
        last.push_str(text);
    } else {
        runs.push((text.clone(), style));
    }
    text.clear();
}

/// One line of ANSI text as styled runs: SGR 0 1 2 3 7 9 22 23 27 29 30-37 38 39 90-97 are
/// applied, every other CSI, string sequence (OSC, DCS, APC, PM, SOS) and escape is dropped,
/// equal neighbours merge.
pub fn ansi_runs(line: &str, theme: &Theme) -> Vec<(String, SpanStyle)> {
    let mut runs = Vec::new();
    let mut text = String::new();
    let mut pen = Pen::default();
    let mut chars = line.chars().peekable();
    // An ESC inside a string that is not the start of ST ends the string and begins the next
    // sequence: it is replayed instead of eating the character after it.
    let mut again = false;
    loop {
        let c = if std::mem::take(&mut again) {
            '\x1b'
        } else if let Some(c) = chars.next() {
            c
        } else {
            break;
        };
        if c != '\x1b' {
            text.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                let mut params = String::new();
                let mut last = None;
                // An ESC ends an unterminated CSI and is left for the next round to read.
                while let Some(c) = chars.next_if(|c| *c != '\x1b') {
                    if ('\x40'..='\x7e').contains(&c) {
                        last = Some(c);
                        break;
                    }
                    params.push(c);
                }
                if last == Some('m') {
                    push_run(&mut runs, &mut text, pen.style(theme));
                    sgr(&mut pen, &params, theme);
                }
            }
            Some(lead @ (']' | 'P' | '_' | '^' | 'X')) => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' && lead == ']' {
                        break;
                    }
                    if c == '\x1b' {
                        again = chars.next_if_eq(&'\\').is_none();
                        break;
                    }
                }
            }
            // Intermediates, then one final byte: `ESC ( B`, `ESC 7`, `ESC \`.
            Some(_) => {
                while chars.next_if(|c| ('\x20'..='\x2f').contains(c)).is_some() {}
                chars.next_if(|c| ('\x30'..='\x7e').contains(c));
            }
            None => {}
        }
    }
    push_run(&mut runs, &mut text, pen.style(theme));
    runs
}

/// The PTY columns that fit `width_px` at `cell_px` per cell; never below 20 (omp's layout needs
/// room) and never wrapping around.
pub fn cols_for(width_px: f32, cell_px: f32) -> u16 {
    if cell_px.is_nan() || cell_px <= 0.0 || width_px.is_nan() || width_px <= 0.0 {
        return MIN_COLS;
    }
    // `as` saturates for floats, NaN is already excluded.
    ((width_px / cell_px).floor() as u16).max(MIN_COLS)
}

const MIN_COLS: u16 = 20;

/// What a pane of an agent shows (spec 7.3 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneView {
    /// Today's terminal grid.
    Terminal,
    /// TSP is on but no surface yet: the starting state.
    Starting,
    /// The live surface.
    Surface,
    /// The last document of an exited session.
    ReadOnly,
}

/// An exited session keeps its last `tsp` value, so `exited` is part of the gate: a waiting
/// session that exited shows the terminal, a live one that exited keeps its last document.
pub fn pane_view(state: TspState, exited: bool, has_surface: bool) -> PaneView {
    match (state, exited, has_surface) {
        (TspState::Waiting, false, _) => PaneView::Starting,
        (TspState::Live, false, true) => PaneView::Surface,
        // Live, the snapshot is still on its way.
        (TspState::Live, false, false) => PaneView::Starting,
        (TspState::Live, true, true) => PaneView::ReadOnly,
        _ => PaneView::Terminal,
    }
}

/// Whether a session went from waiting for, or showing, a surface to drawing as text.
pub fn enters_fallback(before: Option<TspState>, after: TspState) -> bool {
    matches!(before, Some(TspState::Waiting | TspState::Live)) && after == TspState::Fallback
}

/// The toast text for a fallback; `reason` is the daemon's (`TspSnapshot.reason`). The timer
/// fallback is provisional (a late hello still wins), so it says so instead of the reason.
pub fn fallback_toast(reason: Option<&str>) -> String {
    match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(mkrew_core::tsp::engine::NO_REPLY_REASON) => {
            "omp is drawing as text for now. It may switch to the native view if omp answers within a minute."
                .to_owned()
        }
        Some(reason) => format!("omp is drawing as text: {reason}"),
        None => "omp is drawing as text".to_owned(),
    }
}

/// Whether applying `change` may remove nodes (or surfaces), so UI caches keyed by node id are
/// worth pruning.
pub fn removes_nodes(change: &TspChange) -> bool {
    match change {
        TspChange::Frame { ops, .. } => ops
            .iter()
            .any(|op| op.get(0).and_then(Value::as_str) == Some("del")),
        TspChange::Closed { keep } => !keep,
        TspChange::Open { .. } => false,
    }
}

/// Whether a [`Mirror::apply`] changed the mirror or needs a new snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorResult {
    Applied,
    Gap,
}

/// The UI's copy of a session's surfaces: loaded from `tsp.snapshot`, kept current by
/// `tsp.update` events (spec 6.4).
#[derive(Debug, Default)]
pub struct Mirror {
    surfaces: BTreeMap<String, (TspSurface, Document)>,
    /// When each surface was opened (a counter, bigger is newer): the newest `screen` covers.
    opened: HashMap<String, u64>,
    next_open: u64,
    blobs: HashMap<String, TspBlob>,
    /// Per surface, the revision of the nodes a frame touched (see [`Mirror::rev`]).
    node_revs: HashMap<String, HashMap<String, u64>>,
    /// Per surface, the revision it was (re)loaded at: every node is at least this new.
    bases: HashMap<String, u64>,
    /// Surfaces closed without keep: a late frame for one is ignored, not a gap. Ids are tiny and
    /// omp numbers them, so the set is not bounded.
    closed: HashSet<String>,
    /// The rows (direct children of `main`) of the last applied frame's surface that its ops
    /// named, before and after they ran (ADR 0114); taken by the page list.
    dirty_rows: HashSet<String>,
}

/// Revisions are unique across mirrors (a snapshot builds a new one): a counter, never hashing.
static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_revision() -> u64 {
    REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Every node id a frame's `add`, `set`, `text` and `splice` ops write (an added tree counts all
/// its nodes).
fn touched_ids(ops: &[Value]) -> Vec<String> {
    fn tree(node: &Value, out: &mut Vec<String>) {
        if let Some(id) = node.get("id").and_then(Value::as_str) {
            out.push(id.to_owned());
        }
        for child in node
            .get("c")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            tree(child, out);
        }
    }
    let mut out = Vec::new();
    for op in ops {
        let Some(op) = op.as_array() else { continue };
        match op.first().and_then(Value::as_str) {
            Some("add") => {
                if let Some(node) = op.get(4) {
                    tree(node, &mut out);
                }
                out.extend(op.get(1).and_then(Value::as_str).map(str::to_owned));
            }
            Some("set" | "text" | "splice") => {
                out.extend(op.get(1).and_then(Value::as_str).map(str::to_owned));
            }
            _ => {}
        }
    }
    out
}

impl Mirror {
    pub fn from_snapshot(snapshot: TspSnapshot) -> Self {
        let mut mirror = Self::default();
        // ponytail: the daemon lists surfaces in id-string order, so the open order before a
        // reattach is inferred from omp's increasing `s:<n>` ids; keep the real order in the
        // snapshot if omp ever stops numbering that way.
        let mut surfaces = snapshot.surfaces;
        surfaces.sort_by_key(|s| {
            let number = s.id.rsplit(':').next().and_then(|n| n.parse::<u64>().ok());
            (number, s.id.clone())
        });
        for surface in surfaces {
            let doc = Document::from_tree(&surface.root);
            mirror.insert(surface, doc);
        }
        mirror.blobs = snapshot
            .blobs
            .into_iter()
            .map(|b| (b.id.clone(), b))
            .collect();
        mirror
    }

    fn insert(&mut self, surface: TspSurface, doc: Document) {
        self.closed.remove(&surface.id);
        self.opened.insert(surface.id.clone(), self.next_open);
        self.next_open += 1;
        self.bases.insert(surface.id.clone(), next_revision());
        self.node_revs.remove(&surface.id);
        self.surfaces.insert(surface.id.clone(), (surface, doc));
    }

    /// Drops the blobs no `image` node of any surface names.
    fn prune_blobs(&mut self) {
        let mut used = HashSet::new();
        for (_, doc) in self.surfaces.values() {
            let mut stack = vec![doc.root()];
            while let Some(id) = stack.pop() {
                for node in doc.children(id) {
                    if node.kind == "image"
                        && let Some(blob) = node.props.get("blob").and_then(Value::as_str)
                    {
                        used.insert(blob);
                    }
                    stack.push(&node.id);
                }
            }
        }
        self.blobs.retain(|id, _| used.contains(id.as_str()));
    }

    /// Applies one update. A frame at or below the surface's seq was already in the snapshot and
    /// is ignored; the next one applies; any other is a [`MirrorResult::Gap`] (ask for a new
    /// snapshot, nothing is applied).
    pub fn apply(&mut self, update: &TspUpdate) -> MirrorResult {
        self.dirty_rows.clear();
        match &update.change {
            TspChange::Frame { seq, ops, blobs } => {
                let Some((meta, doc)) = self.surfaces.get_mut(&update.surface) else {
                    // A frame already on its way when the surface closed is not a gap.
                    return if self.closed.contains(&update.surface) {
                        MirrorResult::Applied
                    } else {
                        MirrorResult::Gap
                    };
                };
                if *seq <= meta.seq {
                    return MirrorResult::Applied;
                }
                if *seq != meta.seq + 1 {
                    return MirrorResult::Gap;
                }
                // A failing op was already skipped by the daemon; the rest still run here too.
                // The page list hears which rows changed: a `del` or `move` is found where the
                // node was, everything else where it is now.
                let mut dirty = rows_named(doc, ops);
                doc.apply(ops);
                dirty.extend(rows_named(doc, ops));
                self.dirty_rows = dirty;
                meta.seq = *seq;
                let revs = self.node_revs.entry(update.surface.clone()).or_default();
                for id in touched_ids(ops) {
                    revs.insert(id, next_revision());
                }
                for blob in blobs {
                    self.blobs.insert(blob.id.clone(), blob.clone());
                }
                if !blobs.is_empty() {
                    self.prune_blobs();
                }
            }
            TspChange::Open { surface } => {
                let doc = Document::from_tree(&surface.root);
                self.insert(surface.clone(), doc);
                self.prune_blobs();
            }
            TspChange::Closed { keep: true } => {
                if let Some((meta, _)) = self.surfaces.get_mut(&update.surface) {
                    meta.read_only = true;
                }
            }
            TspChange::Closed { keep: false } => {
                self.surfaces.remove(&update.surface);
                self.opened.remove(&update.surface);
                self.bases.remove(&update.surface);
                self.node_revs.remove(&update.surface);
                self.closed.insert(update.surface.clone());
                self.prune_blobs();
            }
        }
        MirrorResult::Applied
    }

    /// The rows the last applied frame dirtied (see [`rows_named`]); empty after this call.
    pub fn take_dirty_rows(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.dirty_rows)
    }

    /// A number that changes whenever node `id` of `surface` is added or its props or text are
    /// written (or the surface is loaded again), and never otherwise: caches key on it instead of
    /// hashing the text every frame.
    pub fn rev(&self, surface: &str, id: &str) -> u64 {
        let base = self.bases.get(surface).copied().unwrap_or(0);
        let node = (self.node_revs.get(surface))
            .and_then(|revs| revs.get(id))
            .copied()
            .unwrap_or(0);
        base.max(node)
    }

    pub fn surface(&self, id: &str) -> Option<&(TspSurface, Document)> {
        self.surfaces.get(id)
    }

    /// The most recently opened surface of `mode` (`only_live`: not a kept, read-only one).
    fn newest(&self, mode: &str, only_live: bool) -> Option<&(TspSurface, Document)> {
        self.surfaces
            .iter()
            .filter(|(_, (meta, _))| meta.mode == mode && !(only_live && meta.read_only))
            .max_by_key(|(id, _)| self.opened.get(*id))
            .map(|(_, surface)| surface)
    }

    /// The surface the pane draws: the newest live `screen` one covers the inline one. A closed
    /// (read-only) screen is history: it shows only when nothing else is left, which is an exited
    /// session's last document.
    pub fn shown(&self) -> Option<&(TspSurface, Document)> {
        (self.newest("screen", true))
            .or_else(|| self.newest("inline", false))
            .or_else(|| self.newest("screen", false))
    }

    /// Whether any surface has a node `id`.
    pub fn has_node(&self, id: &str) -> bool {
        self.surfaces.values().any(|(_, doc)| doc.get(id).is_some())
    }

    /// The inline surface's id: the target of `theme` events.
    pub fn inline_id(&self) -> Option<&str> {
        self.newest("inline", false)
            .map(|(meta, _)| meta.id.as_str())
    }

    pub fn blob(&self, id: &str) -> Option<&TspBlob> {
        self.blobs.get(id)
    }
}

/// Whether anything in `doc` changes with time: a spinner, a running `elapsed` (no `stopped`), a
/// bar without an amount or a checklist with an active item. A `rate` is a static value omp
/// updates itself, and a shimmer draws as plain text (spec 7.1). The status tick redraws only
/// while this is true (ADR 0086).
pub fn animating(doc: &Document) -> bool {
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        for node in doc.children(id) {
            let live = match node.kind.as_str() {
                "spinner" => true,
                // A bar without an amount draws the loading dots; a ring (the ask timeout) is
                // a label.
                "progress" => !has_value(&node.props),
                "meter" => {
                    node.props.get("style").and_then(Value::as_str) != Some("ring")
                        && !has_value(&node.props)
                }
                "elapsed" => !node.props.contains_key("stopped"),
                "checklist" => has_active_item(&node.props),
                _ => false,
            };
            if live {
                return true;
            }
            stack.push(&node.id);
        }
    }
    false
}

fn has_value(props: &Map<String, Value>) -> bool {
    props.get("value").is_some_and(Value::is_number)
}

/// Whether a checklist's props hold an `active` item, read in place (the tick runs this).
fn has_active_item(props: &Map<String, Value>) -> bool {
    fn list(value: Option<&Value>) -> impl Iterator<Item = &Value> {
        value.and_then(Value::as_array).into_iter().flatten()
    }
    list(props.get("phases")).any(|phase| {
        list(phase.get("items"))
            .any(|item| item.get("status").and_then(Value::as_str) == Some("active"))
    })
}

/// The surface roots omp adds itself; a node whose id is `<root>.<keypath>` belongs to the root,
/// not to a component, so omp cannot route an event for it (reference section 5).
const ROOTS: [&str; 3] = ["main", "dock", "layer"];

pub fn root_owned(id: &str) -> bool {
    id.contains('.')
        && id
            .split('.')
            .next()
            .is_some_and(|base| ROOTS.contains(&base))
}

/// `select` (a click) or `activate` (a double click) of `item` in `list`.
pub fn select_event(list: &str, item: &str, double: bool) -> Value {
    json!({"ev": if double { "activate" } else { "select" }, "id": list, "item": item})
}

/// `toggle` of a collapsible node; `None` for a node omp cannot route.
pub fn toggle_event(id: &str, collapsed: bool) -> Option<Value> {
    (!root_owned(id)).then(|| json!({"ev": "toggle", "id": id, "collapsed": collapsed}))
}

/// The event a click (or double click) on `node` sends (spec 7.4); `None` when the node has no
/// action or omp cannot route it. The daemon adds `sf`.
pub fn click_event(doc: &Document, node: &Node, double: bool) -> Option<Value> {
    if root_owned(&node.id) {
        return None;
    }
    if node.kind == "item"
        && let Some(list) = node
            .parent
            .as_deref()
            .and_then(|id| doc.get(id))
            .filter(|list| list.kind == "list")
    {
        return (!root_owned(&list.id)).then(|| select_event(&list.id, &node.id, double));
    }
    if matches!(node.kind.as_str(), "editor" | "input") {
        return Some(json!({"ev": "focus", "id": node.id}));
    }
    let actions = node.props.get("actions")?;
    // A quick second click on a node with only a `click` action is that click again.
    let act = double
        .then(|| actions.get("dblclick"))
        .flatten()
        .or_else(|| actions.get("click"))?
        .as_str()?;
    Some(json!({"ev": "action", "id": node.id, "act": act}))
}

/// Whether [`click_event`] gives an event for `node` (either click), without building one: the
/// pane asks this of every node on every frame.
pub fn has_click(doc: &Document, node: &Node) -> bool {
    if root_owned(&node.id) {
        return false;
    }
    if node.kind == "item"
        && let Some(list) = node
            .parent
            .as_deref()
            .and_then(|id| doc.get(id))
            .filter(|list| list.kind == "list")
    {
        return !root_owned(&list.id);
    }
    if matches!(node.kind.as_str(), "editor" | "input") {
        return true;
    }
    node.props.get("actions").is_some_and(|actions| {
        ["click", "dblclick"]
            .iter()
            .any(|key| actions.get(key).is_some_and(Value::is_string))
    })
}

/// How long to wait before asking for a snapshot again after `failures` failed ones in a row:
/// 2 s, doubling, at most 30 s.
pub fn retry_wait(failures: u32) -> std::time::Duration {
    let secs = match failures {
        0 => 0,
        n => 2u64.saturating_mul(1u64 << (n - 1).min(5)).min(30),
    };
    std::time::Duration::from_secs(secs)
}

/// The direct child of `main` that is `id` or holds it; `None` for `main` itself and for nodes
/// outside it (the dock, a layer).
pub fn row_of(doc: &Document, id: &str) -> Option<String> {
    let mut at = doc.get(id)?;
    loop {
        let parent = at.parent.as_deref()?;
        if parent == "main" {
            return Some(at.id.clone());
        }
        at = doc.get(parent)?;
    }
}

/// Index among `main`'s children of the row that holds `id`: what a `reveal` op scrolls to.
pub fn reveal_index(doc: &Document, id: &str) -> Option<usize> {
    let row = row_of(doc, id)?;
    doc.children("main").position(|node| node.id == row)
}

/// The rows holding the nodes `ops` name in `doc`: each op's node, and the parent an `add` or a
/// `move` names. `reveal`, `scroll` and `focus` only point at a node and change nothing drawn.
pub fn rows_named(doc: &Document, ops: &[Value]) -> HashSet<String> {
    let mut rows = HashSet::new();
    for op in ops {
        let Some(args) = op.as_array() else { continue };
        let named: &[usize] = match args.first().and_then(Value::as_str) {
            Some("add" | "move") => &[1, 2],
            Some("set" | "text" | "splice" | "del" | "settle") => &[1],
            _ => &[],
        };
        for &at in named {
            if let Some(row) = (args.get(at).and_then(Value::as_str)).and_then(|id| row_of(doc, id))
            {
                rows.insert(row);
            }
        }
    }
    rows
}

/// The rows holding an `image` node that names one of `blobs`: an image that just arrived
/// changes its row's height.
pub fn image_rows(doc: &Document, blobs: &[TspBlob]) -> HashSet<String> {
    let wanted: HashSet<&str> = blobs.iter().map(|blob| blob.id.as_str()).collect();
    let mut rows = HashSet::new();
    for row in doc.children("main") {
        let mut stack = vec![row];
        while let Some(node) = stack.pop() {
            let blob = node.props.get("blob").and_then(Value::as_str);
            if node.kind == "image" && blob.is_some_and(|id| wanted.contains(id)) {
                rows.insert(row.id.clone());
                break;
            }
            stack.extend(doc.children(&node.id));
        }
    }
    rows
}

/// What the page list must hear when `main`'s rows go from `old` to `new` (ADR 0114).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowChange {
    /// Rows `range` of the old list became `count` new rows.
    Splice {
        range: std::ops::Range<usize>,
        count: usize,
    },
    /// Row `.0` (numbered as in the new list) changed its content: measure it again.
    Remeasure(usize),
}

/// The common prefix and suffix of `old` and `new` stay; the middle is one splice. A kept row in
/// `dirty` is remeasured. Apply the changes in order.
pub fn row_changes(old: &[String], new: &[String], dirty: &HashSet<String>) -> Vec<RowChange> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = (old.iter().rev())
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let mut changes = Vec::new();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    if old_end > prefix || new_end > prefix {
        changes.push(RowChange::Splice {
            range: prefix..old_end,
            count: new_end - prefix,
        });
    }
    for (ix, id) in new.iter().enumerate() {
        if (ix < prefix || ix >= new_end) && dirty.contains(id) {
            changes.push(RowChange::Remeasure(ix));
        }
    }
    changes
}

/// What a `main` row counts as when thinking rows are grouped ([`thought_plan`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Draws nothing: `hidden` (omp hides a finished tool row), or a column without shown children.
    Hidden,
    /// Its only shown content is one collapsed `omp.thinking` section, with its `took` in ms.
    Thought(Option<u64>),
    /// Anything else: it breaks a run.
    Other,
}

/// A section or card whose role is a thinking one (`omp.thinking`, `omp.thinking.*`).
pub fn is_thinking(node: &Node) -> bool {
    role_of(node).is_some_and(|role| role.starts_with("omp.thinking"))
}

/// How row `node` of `main` counts for a run of thoughts.
pub fn row_kind(doc: &Document, node: &Node) -> RowKind {
    let hidden = |n: &Node| n.props.get("hidden").and_then(Value::as_bool) == Some(true);
    if hidden(node) {
        return RowKind::Hidden;
    }
    let section = match node.kind.as_str() {
        "section" => node,
        "col" => {
            let mut shown = doc.children(&node.id).filter(|child| !hidden(child));
            match (shown.next(), shown.next()) {
                (None, _) => return RowKind::Hidden,
                (Some(only), None) => only,
                _ => return RowKind::Other,
            }
        }
        _ => return RowKind::Other,
    };
    let collapsed = section.props.get("collapsed").and_then(Value::as_bool) == Some(true);
    if section.kind != "section" || role_of(section) != Some("omp.thinking") || !collapsed {
        return RowKind::Other;
    }
    let took = section.props.get("took").and_then(Value::as_f64);
    RowKind::Thought(took.map(|ms| ms.max(0.0) as u64))
}

/// How one row of a run of thoughts is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowPlan {
    /// The run's first row: the group head `label`; `open` draws the row itself under it.
    Lead { label: String, open: bool },
    /// A later row of a closed run: nothing.
    Folded,
}

/// "Thought for 4s": the sum of the `took`s (ms) rounded up to whole seconds, at least 1 s, like
/// omp; "Thought 3 times" when a member has no `took`.
fn thought_label(tooks: &[Option<u64>]) -> String {
    match tooks.iter().copied().sum::<Option<u64>>() {
        Some(ms) => format!(
            "Thought for {}",
            crate::status::format_duration(ms.div_ceil(1000).max(1) as i64 * 1000)
        ),
        None => format!("Thought {} times", tooks.len()),
    }
}

fn close_run(
    plan: &mut HashMap<String, RowPlan>,
    run: &mut Vec<(&str, Option<u64>)>,
    open: &HashSet<String>,
) {
    if let [(lead, _), rest @ ..] = run.as_slice()
        && !rest.is_empty()
    {
        let tooks: Vec<Option<u64>> = run.iter().map(|(_, took)| *took).collect();
        let is_open = open.contains(*lead);
        if !is_open {
            for (id, _) in rest {
                plan.insert((*id).to_owned(), RowPlan::Folded);
            }
        }
        let label = thought_label(&tooks);
        plan.insert(
            (*lead).to_owned(),
            RowPlan::Lead {
                label,
                open: is_open,
            },
        );
    }
    run.clear();
}

/// Groups the thinking rows of `main` (`rows`: id and kind, in order): two or more thoughts with
/// only hidden rows between them are one run, led by the first (`open`: lead ids the user opened).
/// A row without an entry draws as it is.
pub fn thought_plan<'a>(
    rows: impl IntoIterator<Item = (&'a str, RowKind)>,
    open: &HashSet<String>,
) -> HashMap<String, RowPlan> {
    let mut plan = HashMap::new();
    let mut run = Vec::new();
    for (id, kind) in rows {
        match kind {
            RowKind::Hidden => {}
            RowKind::Thought(took) => run.push((id, took)),
            RowKind::Other => close_run(&mut plan, &mut run, open),
        }
    }
    close_run(&mut plan, &mut run, open);
    plan
}

/// The space below a top-level row: [`theme::TSP_ROW_GAP`] when the row draws something, none
/// for a hidden row or a thought folded into its run's head (it draws nothing, so it must not
/// add a gap of its own). `plan` is the row's entry of [`thought_plan`].
pub fn row_gap(kind: RowKind, plan: Option<&RowPlan>) -> f32 {
    if kind == RowKind::Hidden || matches!(plan, Some(RowPlan::Folded)) {
        0.0
    } else {
        theme::TSP_ROW_GAP
    }
}

/// Where a `card` or `section` gets its header (reference 4, row 3): a string or spans, or the
/// child whose id the `head` prop names (omp sends `head: "<id>/head"` and a `text` child with
/// that id; a spinner or elapsed child can sit there too).
#[derive(Debug, Clone, Copy)]
pub enum Head<'a> {
    None,
    Spans(&'a Value),
    Child(&'a Node),
}

pub fn head_of<'a>(doc: &'a Document, node: &'a Node) -> Head<'a> {
    let Some(head) = node.props.get("head") else {
        return Head::None;
    };
    if let Some(name) = head.as_str() {
        let child = doc.children(&node.id).find(|child| {
            child.id == name || (name.ends_with("/head") && child.id.ends_with("/head"))
        });
        if let Some(child) = child {
            return Head::Child(child);
        }
    }
    Head::Spans(head)
}

/// The list holding node `id` (an item or something inside one) and the index of that item among
/// the list's children: what a `reveal` op scrolls inside a list (`virtual` lists keep their own
/// scroll). `None` when no ancestor is a list.
pub fn list_reveal(doc: &Document, id: &str) -> Option<(String, usize)> {
    let mut at = doc.get(id)?;
    loop {
        let parent = doc.get(at.parent.as_deref()?)?;
        if parent.kind == "list" {
            let ix = doc.children(&parent.id).position(|row| row.id == at.id)?;
            return Some((parent.id.clone(), ix));
        }
        at = parent;
    }
}

/// The index among `list`'s children of its `selected` item.
pub fn selected_index(doc: &Document, list: &Node) -> Option<usize> {
    let selected = list.props.get("selected")?.as_str()?;
    doc.children(&list.id).position(|row| row.id == selected)
}

/// A Nerd Font / private-use glyph (omp's icons): drawn in the terminal font, which carries them.
pub fn is_icon_glyph(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}')
}

/// `runs` with every private-use glyph cut out into its own `mono` run (the rest unchanged).
pub fn with_icon_glyphs(
    runs: &[(String, SpanStyle)],
) -> std::borrow::Cow<'_, [(String, SpanStyle)]> {
    if !runs
        .iter()
        .any(|(text, style)| !style.mono && text.chars().any(is_icon_glyph))
    {
        return std::borrow::Cow::Borrowed(runs);
    }
    let mut out = Vec::new();
    for (text, style) in runs {
        if style.mono {
            out.push((text.clone(), *style));
            continue;
        }
        let mut rest = text.as_str();
        while !rest.is_empty() {
            let glyph = is_icon_glyph(rest.chars().next().unwrap_or(' '));
            let end = rest
                .char_indices()
                .find(|(_, c)| is_icon_glyph(*c) != glyph)
                .map_or(rest.len(), |(at, _)| at);
            out.push((
                rest[..end].to_owned(),
                SpanStyle {
                    mono: style.mono || glyph,
                    italic: style.italic && !glyph,
                    bold: style.bold && !glyph,
                    ..*style
                },
            ));
            rest = &rest[end..];
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The texts of the toasts a frame's ops add or change (`toast` goes to Mkrew's toast layer, spec
/// 7.1). `doc` is the document after the ops, used to tell a toast from another node.
pub fn toast_texts(doc: &Document, ops: &[Value]) -> Vec<String> {
    toasts_in(Some(doc), ops)
}

/// The toasts a frame *adds*, for a frame that could not be applied (a gap): without the
/// document a `set` cannot be told from a change to another node, so those are not found.
pub fn added_toast_texts(ops: &[Value]) -> Vec<String> {
    toasts_in(None, ops)
}

fn toasts_in(doc: Option<&Document>, ops: &[Value]) -> Vec<String> {
    ops.iter()
        .filter_map(|op| {
            let op = op.as_array()?;
            let text = match op.first()?.as_str()? {
                "add" => {
                    let node = op.get(4)?;
                    if node.get("k")?.as_str()? != "toast" {
                        return None;
                    }
                    node.get("p")?.get("text")?
                }
                "set" => {
                    if doc?.get(op.get(1)?.as_str()?)?.kind != "toast" {
                        return None;
                    }
                    op.get(2)?.get("text")?
                }
                _ => return None,
            };
            Some(text.as_str()?.to_owned())
        })
        .filter(|text| !text.is_empty())
        .collect()
}

/// What a `scroll` op does to the list (spec 7.3): `["scroll", id, by]` with `by` `start`, `end`,
/// or a step up or down. omp only sends it when the hello lists the `scroll` feature (P1 does
/// not), so this is the reading of the reference's op table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListScroll {
    Top,
    Bottom,
    Page { up: bool },
}

/// The scroll the last `scroll` op of a frame asks for.
pub fn scroll_in(ops: &[Value]) -> Option<ListScroll> {
    ops.iter().rev().find_map(|op| {
        let op = op.as_array()?;
        if op.first()?.as_str()? != "scroll" {
            return None;
        }
        match op.get(2)?.as_str()? {
            "start" => Some(ListScroll::Top),
            "end" => Some(ListScroll::Bottom),
            "up" | "pageup" => Some(ListScroll::Page { up: true }),
            "down" | "pagedown" => Some(ListScroll::Page { up: false }),
            _ => None,
        }
    })
}

/// The widest the transcript of an omp surface is: the Chat column (`CHAT_COLUMN_WIDTH`,
/// centered) when its pane is the only one shown, `None` (as wide as the pane, like Tern) among
/// `several` (ADR 0112).
pub fn column_max(several: bool) -> Option<f32> {
    (!several).then_some(theme::CHAT_COLUMN_WIDTH)
}

/// The width in px of the text column of a pane `pane_width` wide: the transcript's content
/// ([`column_max`] less `space::XL` at both sides), what the PTY is sized to.
pub fn column_px(pane_width: f32, several: bool) -> f32 {
    column_max(several).map_or(pane_width, |max| pane_width.min(max))
        - 2.0 * theme::CHAT_COLUMN_PADDING
}

/// The PTY columns for a pane `pane_width` wide; `None` while the width is unknown (not laid out
/// yet, or no room for one cell): the PTY keeps its size then, it is never sized to a guess.
pub fn fit_width(pane_width: f32, cell_px: f32, several: bool) -> Option<u16> {
    let column = column_px(pane_width, several);
    (pane_width > 0.0 && cell_px > 0.0 && column >= cell_px).then(|| cols_for(column, cell_px))
}

/// The widest an `overlay` of `size` is.
pub fn overlay_width(size: Option<&str>) -> f32 {
    match size {
        Some("full" | "lg") => theme::CHAT_COLUMN_WIDTH,
        Some("md") => theme::TSP_OVERLAY_MD_WIDTH,
        _ => theme::TSP_OVERLAY_SM_WIDTH,
    }
}

/// Parsed values by node id, kept while the node's revision ([`Mirror::rev`]) is unchanged: a
/// streaming `text append` re-parses only the node it grew, and nothing is hashed per frame.
// Entries of deleted nodes are dropped by `retain` after a frame that deletes and after a snapshot.
pub struct TextCache<T> {
    entries: HashMap<String, (u64, Rc<T>)>,
}

impl<T> Default for TextCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> TextCache<T> {
    /// The value of node `id` at revision `rev`; `make` runs on a miss only.
    pub fn get(&mut self, id: &str, rev: u64, make: impl FnOnce() -> T) -> Rc<T> {
        if let Some((have, value)) = self.entries.get(id)
            && *have == rev
        {
            return value.clone();
        }
        let value = Rc::new(make());
        self.entries.insert(id.to_owned(), (rev, value.clone()));
        value
    }

    /// The parse last made for node `id`, whatever its text was.
    pub fn peek(&self, id: &str) -> Option<Rc<T>> {
        self.entries.get(id).map(|(_, value)| value.clone())
    }

    /// Drops the entries whose node id `keep` rejects.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.entries.retain(|id, _| keep(id));
    }
}

/// Drops the entries of `map` (keyed by node or blob id) whose id `keep` rejects.
pub fn retain_known<V>(map: &mut HashMap<String, V>, keep: impl Fn(&str) -> bool) {
    map.retain(|id, _| keep(id));
}

/// When a mirror needs a snapshot: it has none yet, or a gap was seen after the last request
/// began (a reply already in flight cannot clear a later gap).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Resync {
    stale: bool,
}

impl Resync {
    /// A frame could not be applied.
    pub fn gap(&mut self) {
        self.stale = true;
    }

    /// A snapshot request starts: it answers the gaps seen so far.
    pub fn begin(&mut self) {
        self.stale = false;
    }

    /// The request failed: the gap it was to answer is still open.
    pub fn failed(&mut self) {
        self.stale = true;
    }

    pub fn wanted(self, has_mirror: bool) -> bool {
        !has_mirror || self.stale
    }
}

/// A fenced code block of `text`, its fence longer than any backtick run inside.
pub fn code_fence(lang: Option<&str>, text: &str) -> String {
    let (mut longest, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}{}\n{text}\n{fence}", lang.unwrap_or(""))
}

/// The byte offset of UTF-16 unit `index` of `text` (omp's editor offsets); inside a surrogate
/// pair it snaps to the character's start, past the end it clamps.
pub fn byte_at_utf16(text: &str, index: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        units += c.len_utf16();
        if index < units {
            return byte;
        }
    }
    text.len()
}

/// What an `editor` decoration does to a range of its text (`decor` entries `{from, to, s}`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decor {
    pub style: SpanStyle,
    /// The range is not drawn (omp's own markers).
    pub hide: bool,
    /// A spelling mistake: wavy underline.
    pub typo: bool,
}

impl Decor {
    fn parse(tokens: &str, theme: &Theme) -> Self {
        let mut decor = Self {
            style: span_style(tokens, theme),
            ..Self::default()
        };
        for token in tokens.split_whitespace() {
            match token {
                "hide" => decor.hide = true,
                "typo" => decor.typo = true,
                // Skills, mentions and references: one accent look for all.
                "customMessageLabel" | "statusLineModel" => {
                    decor.style.color.get_or_insert(theme.accent);
                }
                _ => {}
            }
        }
        decor
    }

    /// Both decorations at once (ranges that overlap): the first color stays, flags add up.
    fn merged(self, over: Self) -> Self {
        Self {
            style: SpanStyle {
                color: self.style.color.or(over.style.color),
                bold: self.style.bold || over.style.bold,
                italic: self.style.italic || over.style.italic,
                strike: self.style.strike || over.style.strike,
                mono: self.style.mono || over.style.mono,
                mark: self.style.mark || over.style.mark,
            },
            hide: self.hide || over.hide,
            typo: self.typo || over.typo,
        }
    }
}

/// `text` cut into byte ranges that cover all of it, each with the decorations over it. `decor`
/// is the `decor` prop: `{from, to, s}` in UTF-16 units of the whole text.
pub fn decor_pieces(
    text: &str,
    decor: &[Value],
    theme: &Theme,
) -> Vec<(std::ops::Range<usize>, Decor)> {
    let spans: Vec<(usize, usize, Decor)> = decor
        .iter()
        .filter_map(|entry| {
            let at = |key: &str| {
                let units = usize::try_from(entry.get(key)?.as_u64()?).ok()?;
                Some(byte_at_utf16(text, units))
            };
            let (from, to) = (at("from")?, at("to")?);
            let tokens = entry.get("s").and_then(Value::as_str).unwrap_or("");
            (from < to).then(|| (from, to, Decor::parse(tokens, theme)))
        })
        .collect();
    let mut cuts = vec![0, text.len()];
    cuts.extend(spans.iter().flat_map(|(from, to, _)| [*from, *to]));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|cut| {
            let decor = (spans.iter())
                .filter(|(from, to, _)| *from <= cut[0] && cut[1] <= *to)
                .fold(Decor::default(), |all, (_, _, one)| all.merged(*one));
            (cut[0]..cut[1], decor)
        })
        .collect()
}

/// Filled blocks of 4 for a thinking level.
pub fn effort_steps(level: &str) -> usize {
    match level {
        "minimal" | "low" => 1,
        "medium" => 2,
        "high" => 3,
        "xhigh" | "max" => 4,
        _ => 0,
    }
}

/// The tone of a meter at `value` given omp's `{warn, bad}` thresholds.
pub fn meter_tone(value: f64, thresholds: Option<&Value>) -> &'static str {
    let at = |key: &str| thresholds.and_then(|t| t.get(key)).and_then(Value::as_f64);
    match (at("bad"), at("warn")) {
        (Some(bad), _) if value >= bad => "error",
        (_, Some(warn)) if value >= warn => "warning",
        _ => "accent",
    }
}

pub fn rate_label(value: f64, unit: &str) -> String {
    if value >= 10.0 {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

/// An `elapsed` node's value: `stopped` once set, else `age` plus the time since this UI first
/// saw that `age`.
pub fn elapsed_ms(age: u64, stopped: Option<u64>, since_seen_ms: u64) -> u64 {
    stopped.unwrap_or(age.saturating_add(since_seen_ms))
}

pub fn elapsed_label(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3600, secs % 3600 / 60),
    }
}

/// What a key does in a TSP pane that may hold a selection (Ctrl+C): copy it, or let the key
/// reach omp as in a terminal pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOrSend {
    Copy,
    Send,
}

pub fn copy_decision(has_selection: bool, is_copy_key: bool) -> CopyOrSend {
    if has_selection && is_copy_key {
        CopyOrSend::Copy
    } else {
        CopyOrSend::Send
    }
}

/// Esc clears a selection (and is not sent); without one omp gets it.
pub fn escape_clears(has_selection: bool, is_escape: bool) -> bool {
    has_selection && is_escape
}

/// A press on node `key`: a double click takes the word, Shift extends the selection of the same
/// node, anything else puts a caret (the Chat rule, `chat::press`, with a node id as the key).
pub fn press(
    current: Option<(String, crate::preview_select::Selection)>,
    key: &str,
    shown: &crate::docs::Shown,
    at: crate::preview_select::Point,
    extend: bool,
    clicks: usize,
) -> (String, crate::preview_select::Selection) {
    use crate::preview_select::Selection;
    let selection = match current {
        _ if clicks == 2 => crate::preview_select::word_at(shown, at),
        Some((was, selection)) if extend && was == key => Selection {
            head: at,
            ..selection
        },
        _ => Selection::caret(at),
    };
    (key.to_owned(), selection)
}

/// Status of one todo item as omp sends it (`vbe` in `tools/todo.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Active,
    Pending,
    Done,
    Blocked,
    Dropped,
}

impl TodoStatus {
    /// An unknown status is pending.
    pub fn parse(status: &str) -> Self {
        match status {
            "active" => Self::Active,
            "done" => Self::Done,
            "blocked" => Self::Blocked,
            "dropped" => Self::Dropped,
            _ => Self::Pending,
        }
    }

    /// Counted in `done/total` and collapses its phase.
    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Dropped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: TodoStatus,
    /// A blocked item's blocker.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoPhase {
    pub id: String,
    pub title: String,
    /// omp's own `collapsed`.
    pub collapsed: bool,
    pub items: Vec<TodoItem>,
}

/// What a phase's header mark shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseMark {
    /// Loading dots: an item is active.
    Working,
    /// A filled `status_done` circle: every item is finished.
    Done,
    /// A `text_muted` ring.
    Open,
}

impl TodoPhase {
    /// `(finished, all)` items.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.items.iter().filter(|i| i.status.finished()).count(),
            self.items.len(),
        )
    }

    fn all_finished(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|i| i.status.finished())
    }

    pub fn mark(&self) -> PhaseMark {
        if self.items.iter().any(|i| i.status == TodoStatus::Active) {
            PhaseMark::Working
        } else if self.all_finished() {
            PhaseMark::Done
        } else {
            PhaseMark::Open
        }
    }

    /// The reminder's one phase has a blank name: no header is drawn.
    pub fn headerless(&self) -> bool {
        self.title.trim().is_empty()
    }

    /// Header only, before the user clicks it: omp said `collapsed`, or every item is finished.
    /// A phase without a header is never collapsed (nothing could reopen it).
    pub fn default_collapsed(&self) -> bool {
        !self.headerless() && (self.collapsed || self.all_finished())
    }
}

/// A click on a header (UI state only) flips the default.
pub fn collapsed_now(default: bool, toggled: bool) -> bool {
    default != toggled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecklistMode {
    Full,
    Reminder,
    Hud,
}

/// The `checklist` node of omp's todo (spec 7.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    pub mode: ChecklistMode,
    pub phases: Vec<TodoPhase>,
    pub note: Option<String>,
}

/// The hud's mark: the active todo mark (working), a ring (waiting), a filled circle (nothing left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudMark {
    Active,
    Ring,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudLine {
    pub mark: HudMark,
    pub text: String,
    pub done: usize,
    pub total: usize,
}

pub const ALL_DONE: &str = "All done";

impl Checklist {
    pub fn from_props(props: &Map<String, Value>) -> Self {
        let mode = match props.get("mode").and_then(Value::as_str) {
            Some("hud") => ChecklistMode::Hud,
            Some("reminder") => ChecklistMode::Reminder,
            _ => ChecklistMode::Full,
        };
        let text =
            |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let phases = (props.get("phases").and_then(Value::as_array))
            .map(|phases| {
                phases
                    .iter()
                    .enumerate()
                    .filter(|(_, phase)| phase.is_object())
                    .map(|(at, phase)| TodoPhase {
                        id: text(phase, "id").unwrap_or_else(|| format!("p{at}")),
                        title: text(phase, "title").unwrap_or_default(),
                        collapsed: phase.get("collapsed").and_then(Value::as_bool) == Some(true),
                        items: (phase.get("items").and_then(Value::as_array))
                            .map(|items| {
                                items
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, item)| item.is_object())
                                    .map(|(at, item)| TodoItem {
                                        id: text(item, "id").unwrap_or_else(|| format!("i{at}")),
                                        text: text(item, "text").unwrap_or_default(),
                                        status: TodoStatus::parse(
                                            item.get("status")
                                                .and_then(Value::as_str)
                                                .unwrap_or(""),
                                        ),
                                        note: text(item, "note").filter(|n| !n.trim().is_empty()),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            mode,
            phases,
            note: props
                .get("note")
                .and_then(Value::as_str)
                .filter(|n| !n.trim().is_empty())
                .map(str::to_owned),
        }
    }

    /// `(finished, all)` over every phase.
    pub fn counts(&self) -> (usize, usize) {
        self.phases
            .iter()
            .map(TodoPhase::counts)
            .fold((0, 0), |(d, t), (pd, pt)| (d + pd, t + pt))
    }

    fn first(&self, status: TodoStatus) -> Option<&TodoItem> {
        self.phases
            .iter()
            .flat_map(|p| p.items.iter())
            .find(|i| i.status == status)
    }

    /// The hud's one line (spec 7.7): the active item, else the first pending one, else (a
    /// blocked item must not read as done) the first blocked one, else "All done".
    pub fn hud_line(&self) -> Option<HudLine> {
        let (done, total) = self.counts();
        if total == 0 {
            return None;
        }
        let (mark, text) = if let Some(item) = self.first(TodoStatus::Active) {
            (HudMark::Active, item.text.clone())
        } else if let Some(item) = self.first(TodoStatus::Pending) {
            (HudMark::Ring, item.text.clone())
        } else if let Some(item) = self.first(TodoStatus::Blocked) {
            (HudMark::Ring, item.text.clone())
        } else {
            (HudMark::Done, ALL_DONE.to_owned())
        };
        Some(HudLine {
            mark,
            text,
            done,
            total,
        })
    }
}

/// A phase number in Roman numerals; 0 and numbers past 3999 stay digits.
pub fn roman(n: usize) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    const TABLE: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let (mut n, mut out) = (n, String::new());
    for (value, symbol) in TABLE {
        while n >= value {
            out.push_str(symbol);
            n -= value;
        }
    }
    out
}

/// The mark of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemMark {
    /// A ring around a dot.
    Active,
    Ring,
    Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemLook {
    pub mark: ItemMark,
    pub mark_color: u32,
    pub text_color: u32,
    pub strike: bool,
    /// Appended to the text.
    pub suffix: &'static str,
}

pub fn item_look(status: TodoStatus, theme: &Theme) -> ItemLook {
    let (mark, mark_color, text_color, strike, suffix) = match status {
        TodoStatus::Active => (ItemMark::Active, theme.accent, theme.accent, false, ""),
        TodoStatus::Pending => (ItemMark::Ring, theme.text_muted, theme.text, false, ""),
        TodoStatus::Done => (
            ItemMark::Check,
            theme.status_done,
            theme.text_muted,
            false,
            "",
        ),
        TodoStatus::Dropped => (ItemMark::Ring, theme.text_muted, theme.text_muted, true, ""),
        TodoStatus::Blocked => (
            ItemMark::Ring,
            theme.status_needs_input,
            theme.status_needs_input,
            false,
            " (blocked)",
        ),
    };
    ItemLook {
        mark,
        mark_color,
        text_color,
        strike,
        suffix,
    }
}

/// `grow` > 0: the node takes the free space of its row or column.
pub fn grows(node: &Node) -> bool {
    node.props
        .get("grow")
        .and_then(Value::as_f64)
        .is_some_and(|grow| grow > 0.0)
}

/// Whether a growing `list` sits under `node` where it can have a bounded height: a child, or
/// in a growing `col` (a row gives its children no bounded height).
fn has_grow_list(doc: &Document, node: &Node) -> bool {
    doc.children(&node.id)
        .any(|child| grows(child) && has_grow_list_in(doc, child))
}

fn has_grow_list_in(doc: &Document, node: &Node) -> bool {
    match node.kind.as_str() {
        "list" => true,
        "col" => has_grow_list(doc, node),
        _ => false,
    }
}

/// The children of a card bounded to its pane (an overlay, or the card of a `screen` surface)
/// by where they sit when the body scrolls: leading `tabs` and the trailing hints row stay put,
/// the rest scrolls. `None` when a growing list scrolls inside the card instead.
pub struct BoundedSplit<'a> {
    pub top: Vec<&'a Node>,
    pub body: Vec<&'a Node>,
    pub bottom: Vec<&'a Node>,
}

/// Whether a `screen` surface is drawn bounded by the pane (no page scroll): only when `main`
/// holds exactly one card, the Agent Hub, which scrolls its own body. Any other content would be
/// clipped, so that screen keeps the scrolling page.
pub fn screen_bounded(doc: &Document) -> bool {
    let mut main = doc.children("main");
    matches!((main.next(), main.next()), (Some(only), None) if only.kind == "card")
}

/// The head child of a card or section is drawn as its head, not among the split nodes.
pub fn bounded_split<'a>(doc: &'a Document, node: &'a Node) -> Option<BoundedSplit<'a>> {
    if has_grow_list(doc, node) {
        return None;
    }
    // An overlay draws no head of its own: a child named by its `head` stays content.
    let head = match head_of(doc, node) {
        Head::Child(child) if node.kind != "overlay" => Some(child.id.as_str()),
        _ => None,
    };
    let kids: Vec<&Node> = doc
        .children(&node.id)
        .filter(|n| Some(n.id.as_str()) != head)
        .collect();
    let top = kids.iter().take_while(|n| n.kind == "tabs").count();
    let bottom = (kids[top..].iter().rev())
        .take_while(|n| role_of(n) == Some("omp.overlay.hints"))
        .count();
    let end = kids.len() - bottom;
    Some(BoundedSplit {
        top: kids[..top].to_vec(),
        body: kids[top..end].to_vec(),
        bottom: kids[end..].to_vec(),
    })
}

/// A node's `role` prop.
pub fn role_of(node: &Node) -> Option<&str> {
    node.props.get("role").and_then(Value::as_str)
}

/// The role specific looks of omp's `ask` card (spec 7.6). Roles match whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskLook {
    /// `omp.editor.ask`: the composer card.
    Card,
    /// `omp.editor.ask.hidden`: the one line version.
    Hidden,
    /// `omp.ask.head`: the row of the tabs.
    Head,
    /// `omp.ask.questions`.
    Tabs,
    /// `omp.ask.question`.
    Question,
    /// `omp.ask.options`.
    Options,
    /// `omp.ask.option`: a radio row.
    Choice,
    /// `omp.ask.check`: a check box row.
    Check,
    /// `omp.ask.other`: the radio row for a typed answer.
    Other,
    /// `omp.ask.other.check`.
    OtherCheck,
    /// `omp.ask.preview`.
    Preview,
    /// `omp.ask.notice`.
    Notice,
    /// `omp.ask.answers`.
    Answers,
    /// `omp.ask.summary`: the one line "Question ..." of the hidden card.
    Summary,
    /// `omp.actions`: the bottom row.
    Actions,
    /// `omp.btn`: a pill button.
    Button,
    /// `omp.hook-editor.head`: the eyebrow and the question of a note or custom answer.
    HookHead,
    /// `omp.hook-editor.eyebrow`: a muted line above the question ("Note for red").
    HookEyebrow,
    /// `omp.hook-editor.line`: the framed field. The card itself is `omp.overlay.hook-editor`,
    /// drawn as [`AskLook::Card`].
    HookLine,
}

pub fn ask_look(role: &str) -> Option<AskLook> {
    Some(match role {
        "omp.editor.ask" => AskLook::Card,
        "omp.editor.ask.hidden" => AskLook::Hidden,
        "omp.ask.head" => AskLook::Head,
        "omp.ask.questions" => AskLook::Tabs,
        "omp.ask.question" => AskLook::Question,
        "omp.ask.options" => AskLook::Options,
        "omp.ask.option" => AskLook::Choice,
        "omp.ask.check" => AskLook::Check,
        "omp.ask.other" => AskLook::Other,
        "omp.ask.other.check" => AskLook::OtherCheck,
        "omp.ask.preview" => AskLook::Preview,
        "omp.ask.notice" => AskLook::Notice,
        "omp.ask.answers" => AskLook::Answers,
        "omp.ask.summary" => AskLook::Summary,
        "omp.actions" => AskLook::Actions,
        "omp.btn" => AskLook::Button,
        "omp.overlay.hook-editor" => AskLook::Card,
        "omp.hook-editor.head" => AskLook::HookHead,
        "omp.hook-editor.eyebrow" => AskLook::HookEyebrow,
        "omp.hook-editor.line" => AskLook::HookLine,
        _ => return None,
    })
}

impl AskLook {
    /// A row of the options list.
    pub fn is_choice(self) -> bool {
        matches!(
            self,
            Self::Choice | Self::Check | Self::Other | Self::OtherCheck
        )
    }

    /// A check box (several answers allowed) rather than a radio.
    pub fn boxed(self) -> bool {
        matches!(self, Self::Check | Self::OtherCheck)
    }
}

/// omp marks the picked rows with `tone: "accent"`.
pub fn choice_filled(node: &Node) -> bool {
    node.props.get("tone").and_then(Value::as_str) == Some("accent")
}

mod dock;
pub use dock::*;
mod editor;
pub use editor::*;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::theme::{Theme, space};

    #[test]
    fn tokens_map_to_theme_fields_and_unknown_ones_to_none() {
        let t = Theme::dark();
        assert_eq!(color("accent", &t), Some(t.accent));
        assert_eq!(color("info", &t), Some(t.accent));
        assert_eq!(color("muted", &t), Some(t.text_muted));
        assert_eq!(color("dim", &t), Some(t.text_muted));
        assert_eq!(color("error", &t), Some(t.danger_text));
        assert_eq!(color("warning", &t), Some(t.status_needs_input));
        assert_eq!(color("pending", &t), Some(t.status_needs_input));
        assert_eq!(color("success", &t), Some(t.status_done));
        assert_eq!(color("text", &t), Some(t.text));
        assert_eq!(
            color("statusLineModel", &t),
            None,
            "omp's own palette names"
        );
        assert_eq!(color("strong", &t), None, "a modifier is not a color");
        assert_eq!(color("", &t), None);
    }

    #[test]
    fn tones_follow_the_reference_table_and_an_unknown_tone_is_text() {
        let t = Theme::light();
        assert_eq!(tone_color("success", &t), t.status_done);
        assert_eq!(tone_color("warning", &t), t.status_needs_input);
        assert_eq!(tone_color("pending", &t), t.status_needs_input);
        assert_eq!(tone_color("error", &t), t.danger_text);
        assert_eq!(tone_color("muted", &t), t.text_muted);
        assert_eq!(tone_color("accent", &t), t.accent);
        assert_eq!(tone_color("info", &t), t.accent);
        assert_eq!(tone_color("user", &t), t.accent);
        assert_eq!(tone_color("neutral", &t), t.text);
        assert_eq!(tone_color("no-such-tone", &t), t.text);
    }

    #[test]
    fn span_tokens_set_weight_slant_strike_mark_font_and_the_first_color() {
        let t = Theme::dark();
        let s = span_style("strong em del mark", &t);
        assert!(s.bold && s.italic && s.strike && s.mark && !s.mono);
        assert_eq!(s.color, None);
        for token in ["mono", "code", "path", "icon"] {
            assert!(span_style(token, &t).mono, "{token}");
        }
        let s = span_style("dim mono", &t);
        assert_eq!((s.color, s.mono), (Some(t.text_muted), true));
        let s = span_style("error accent", &t);
        assert_eq!(s.color, Some(t.danger_text), "the first color token wins");
        assert_eq!(span_style("nonsense", &t), SpanStyle::default());
        assert_eq!(span_style("", &t), SpanStyle::default());
    }

    #[test]
    fn every_icon_name_of_the_fixture_and_the_reference_resolves_and_others_do_not() {
        use crate::icons::Icon;
        // Names in the committed startup record (node icons and seg icons).
        for name in [
            "lightbulb",
            "model",
            "chev",
            "session",
            "time",
            "context.compaction",
        ] {
            assert!(icon(name).is_some(), "{name}");
        }
        // Names of the reference list (section 4, kind `icon`, and item icons).
        for name in [
            "users", "agents", "agent", "job", "jobs", "model", "chev", "copy", "x", "eye",
            "check", "history", "link", "open", "rewind", "loop", "back", "wand", "bash", "folder",
            "plus", "wrench", "x-circle", "send",
        ] {
            assert!(icon(name).is_some(), "{name}");
        }
        assert_eq!(icon("users").map(Icon::path), Some(Icon::Users.path()));
        assert_eq!(icon("chev").map(Icon::path), Some(Icon::ChevronDown.path()));
        assert_eq!(icon("x").map(Icon::path), Some(Icon::X.path()));
        for name in ["eye-off", "gauge", "pause", "definitely-not-an-icon", ""] {
            assert!(icon(name).is_none(), "{name}");
        }
    }

    #[test]
    fn spans_accept_a_string_or_an_array_and_skip_malformed_entries() {
        let t = Theme::dark();
        assert_eq!(
            spans_of(&json!("plain"), &t),
            vec![("plain".to_owned(), SpanStyle::default())]
        );
        let spans = spans_of(
            &json!([{"t": "a", "s": "strong"}, {"t": "b"}, {"s": "em"}, 7, {"t": "c", "s": "error"}]),
            &t,
        );
        assert_eq!(spans.len(), 3);
        assert!(spans[0].1.bold);
        assert_eq!(spans[1], ("b".to_owned(), SpanStyle::default()));
        assert_eq!(spans[2].1.color, Some(t.danger_text));
        assert!(spans_of(&json!(null), &t).is_empty());
        assert!(spans_of(&json!(12), &t).is_empty());
    }

    #[test]
    fn node_spans_prefer_spans_over_text() {
        let t = Theme::dark();
        let props = json!({"spans": [{"t": "from spans"}], "text": "from text"});
        let props = props.as_object().cloned().unwrap_or_default();
        assert_eq!(node_spans(&props, &t)[0].0, "from spans");
        let props = json!({"text": "from text"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        assert_eq!(node_spans(&props, &t)[0].0, "from text");
    }

    #[test]
    fn an_unknown_kind_draws_its_first_text_like_prop() {
        let t = Theme::dark();
        let props = |v: serde_json::Value| v.as_object().cloned().unwrap_or_default();
        assert_eq!(
            generic_line(&props(json!({"name": "n", "title": "T"})), &t).as_deref(),
            Some("T"),
            "text, title, head, label, name in that order"
        );
        assert_eq!(
            generic_line(
                &props(json!({"head": [{"t": "He"}, {"t": "ad", "s": "strong"}]})),
                &t
            )
            .as_deref(),
            Some("Head")
        );
        assert_eq!(
            generic_line(&props(json!({"text": "  ", "label": "L"})), &t).as_deref(),
            Some("L"),
            "a blank prop is skipped"
        );
        assert_eq!(generic_line(&props(json!({"size": 3})), &t), None);
    }

    #[test]
    fn gap_tokens_come_from_the_spacing_scale() {
        assert_eq!(gap_px(None), 0.0);
        assert_eq!(gap_px(Some("none")), 0.0);
        assert_eq!(gap_px(Some("xs")), space::XS);
        assert_eq!(gap_px(Some("sm")), space::S);
        assert_eq!(gap_px(Some("md")), space::M);
        assert_eq!(gap_px(Some("lg")), space::L);
        assert_eq!(gap_px(Some("huge")), 0.0);
    }

    fn run(text: &str, style: SpanStyle) -> (String, SpanStyle) {
        (text.to_owned(), style)
    }

    #[test]
    fn sgr_sets_attributes_and_zero_resets_them() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("\x1b[1;31mred\x1b[0m ok", &t),
            vec![
                run(
                    "red",
                    SpanStyle {
                        bold: true,
                        color: Some(t.ansi[1]),
                        ..Default::default()
                    }
                ),
                run(" ok", SpanStyle::default()),
            ]
        );
        assert_eq!(
            ansi_runs("\x1b[1mA\x1b[mB", &t),
            vec![
                run(
                    "A",
                    SpanStyle {
                        bold: true,
                        ..Default::default()
                    }
                ),
                run("B", SpanStyle::default()),
            ],
            "an empty parameter list is a reset"
        );
    }

    #[test]
    fn sgr_italic_reverse_strike_and_their_off_codes() {
        let t = Theme::dark();
        let runs = ansi_runs("\x1b[3mI\x1b[23mn\x1b[7;9mM\x1b[27;29mm", &t);
        assert_eq!(
            runs,
            vec![
                run(
                    "I",
                    SpanStyle {
                        italic: true,
                        ..Default::default()
                    }
                ),
                run("n", SpanStyle::default()),
                run(
                    "M",
                    SpanStyle {
                        mark: true,
                        strike: true,
                        ..Default::default()
                    }
                ),
                run("m", SpanStyle::default()),
            ]
        );
    }

    #[test]
    fn sgr_dim_uses_the_muted_color_and_22_clears_bold_and_dim() {
        let t = Theme::dark();
        let runs = ansi_runs("\x1b[1;2mD\x1b[22mN", &t);
        assert_eq!(
            runs,
            vec![
                run(
                    "D",
                    SpanStyle {
                        bold: true,
                        color: Some(t.text_muted),
                        ..Default::default()
                    }
                ),
                run("N", SpanStyle::default()),
            ]
        );
        let runs = ansi_runs("\x1b[2;31mX", &t);
        assert_eq!(
            runs[0].1.color,
            Some(t.ansi[1]),
            "an explicit color beats dim"
        );
    }

    #[test]
    fn sgr_basic_bright_and_default_foregrounds() {
        let t = Theme::dark();
        let colors: Vec<_> = ansi_runs("\x1b[30mA\x1b[37mB\x1b[90mC\x1b[97mD\x1b[39mE", &t)
            .into_iter()
            .map(|(_, s)| s.color)
            .collect();
        assert_eq!(
            colors,
            vec![
                Some(t.ansi[0]),
                Some(t.ansi[7]),
                Some(t.ansi[8]),
                Some(t.ansi[15]),
                None
            ]
        );
    }

    #[test]
    fn sgr_256_and_true_color() {
        let t = Theme::dark();
        let colors: Vec<_> = ansi_runs(
            "\x1b[38;5;196mA\x1b[38;5;1mB\x1b[38;5;244mC\x1b[38;2;1;2;3mD",
            &t,
        )
        .into_iter()
        .map(|(_, s)| s.color)
        .collect();
        assert_eq!(
            colors,
            vec![
                Some(0xFF0000),
                Some(t.ansi[1]),
                Some(0x808080),
                Some(0x010203)
            ]
        );
    }

    #[test]
    fn a_background_sequence_swallows_its_own_parameters_only() {
        let t = Theme::dark();
        let runs = ansi_runs("\x1b[48;5;4;31mX\x1b[48;2;9;9;9;1mY", &t);
        assert_eq!(
            runs[0].1.color,
            Some(t.ansi[1]),
            "48;5;4 must not eat the 31"
        );
        assert!(runs[1].1.bold, "48;2;r;g;b must not eat the 1");
        assert_eq!(
            runs[1].1.color, runs[0].1.color,
            "backgrounds are dropped, the pen stays"
        );
    }

    #[test]
    fn other_csi_and_osc_sequences_are_stripped() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("a\x1b[2Kb\x1b[10;5Hc\x1b[?25ld", &t),
            vec![run("abcd", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs(
                "\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\ \x1b]0;title\x07end",
                &t
            ),
            vec![run("link end", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs("a\x1b7b\x1b8c", &t),
            vec![run("abc", SpanStyle::default())]
        );
    }

    #[test]
    fn equal_neighbouring_styles_merge_and_empty_input_is_empty() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("\x1b[1ma\x1b[1mb\x1b[0m\x1b[1mc", &t),
            vec![run(
                "abc",
                SpanStyle {
                    bold: true,
                    ..Default::default()
                }
            )]
        );
        assert!(ansi_runs("", &t).is_empty());
        assert!(ansi_runs("\x1b[31m\x1b[0m", &t).is_empty());
    }

    #[test]
    fn a_truncated_escape_never_panics_and_drops_only_the_escape() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("ok\x1b", &t),
            vec![run("ok", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs("ok\x1b[31", &t),
            vec![run("ok", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs("ok\x1b]0;never ends", &t),
            vec![run("ok", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs("\x1b[999;?;;mtext", &t),
            vec![run("text", SpanStyle::default())],
            "garbage parameters are zeros, i.e. a reset"
        );
    }

    use mkrew_core::tsp::doc::Document;
    use mkrew_protocol::{TspChange, TspSnapshot, TspState, TspSurface, TspUpdate};
    use ulid::Ulid;

    fn doc(ops: Vec<serde_json::Value>) -> Document {
        let mut d = Document::new("s:1");
        let applied = d.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        d
    }

    fn main_added() -> Vec<serde_json::Value> {
        vec![json!(["add", "main", "s:1", null, {"id": "main", "k": "col"}])]
    }

    /// A document with `main` and one node under it.
    fn with_node(node: serde_json::Value) -> Document {
        let mut ops = main_added();
        ops.push(json!(["add", node["id"].clone(), "main", null, node]));
        doc(ops)
    }

    fn surface(id: &str, mode: &str, seq: u64, d: &Document) -> TspSurface {
        TspSurface {
            id: id.into(),
            mode: mode.into(),
            role: None,
            title: None,
            read_only: false,
            seq,
            root: d.to_tree(),
        }
    }

    fn mirror(surfaces: Vec<TspSurface>) -> Mirror {
        Mirror::from_snapshot(TspSnapshot {
            state: TspState::Live,
            surfaces,
            blobs: Vec::new(),
            reason: None,
        })
    }

    fn update(surface: &str, change: TspChange) -> TspUpdate {
        TspUpdate {
            session_id: Ulid::new(),
            surface: surface.into(),
            change,
        }
    }

    fn frame(surface: &str, seq: u64, ops: Vec<serde_json::Value>) -> TspUpdate {
        update(
            surface,
            TspChange::Frame {
                seq,
                ops,
                blobs: Vec::new(),
            },
        )
    }

    fn add_x() -> Vec<serde_json::Value> {
        vec![json!(["add", "x", "main", null, {"id": "x", "k": "text", "p": {"text": "hi"}}])]
    }

    /// `main` with rows `a` (holding `a1`) and `b` (holding `b1`), and a dock holding `d1`.
    fn two_rows() -> Vec<serde_json::Value> {
        let mut ops = main_added();
        ops.push(
            json!(["add", "a", "main", null, {"id": "a", "k": "col", "c": [
                {"id": "a1", "k": "text", "p": {"text": "x"}}
            ]}]),
        );
        ops.push(
            json!(["add", "b", "main", null, {"id": "b", "k": "col", "c": [
                {"id": "b1", "k": "text", "p": {"text": "y"}}
            ]}]),
        );
        ops.push(
            json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col", "c": [
                {"id": "d1", "k": "text", "p": {"text": "z"}}
            ]}]),
        );
        ops
    }

    /// The rows one frame of `ops` dirties in a mirror of [`two_rows`].
    fn dirtied(ops: Vec<serde_json::Value>) -> Vec<String> {
        let d = doc(two_rows());
        let mut m = mirror(vec![surface("s:1", "inline", 0, &d)]);
        assert_eq!(m.apply(&frame("s:1", 1, ops)), MirrorResult::Applied);
        let mut rows: Vec<String> = m.take_dirty_rows().into_iter().collect();
        rows.sort();
        assert!(m.take_dirty_rows().is_empty(), "taking clears them");
        rows
    }

    #[test]
    fn a_frame_dirties_the_rows_its_ops_name_before_and_after() {
        assert_eq!(dirtied(vec![json!(["set", "a1", {"text": "w"}])]), ["a"]);
        assert_eq!(
            dirtied(vec![
                json!(["add", "b2", "b", null, {"id": "b2", "k": "text"}])
            ]),
            ["b"]
        );
        assert_eq!(
            dirtied(vec![
                json!(["add", "c", "main", null, {"id": "c", "k": "text"}])
            ]),
            ["c"],
            "a new row names itself; `main` names no row"
        );
        assert_eq!(dirtied(vec![json!(["move", "a1", "b", null])]), ["a", "b"]);
        assert_eq!(
            dirtied(vec![json!(["del", "a1"])]),
            ["a"],
            "found before the delete"
        );
        assert_eq!(dirtied(vec![json!(["del", "a"])]), ["a"]);
        assert_eq!(
            dirtied(vec![json!(["set", "d1", {"text": "w"}])]),
            Vec::<String>::new()
        );
        assert_eq!(
            dirtied(vec![json!(["set", "main", {}])]),
            Vec::<String>::new()
        );
        assert_eq!(dirtied(vec![json!(["reveal", "a1"])]), Vec::<String>::new());
    }

    #[test]
    fn row_changes_splice_the_middle_and_remeasure_kept_dirty_rows() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let set = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        use RowChange::{Remeasure, Splice};
        let none = HashSet::new();
        assert_eq!(row_changes(&ids(&["a", "b"]), &ids(&["a", "b"]), &none), []);
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["a", "b", "c"]), &none),
            [Splice {
                range: 2..2,
                count: 1
            }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["a", "x", "b", "c"]), &none),
            [Splice {
                range: 1..1,
                count: 1
            }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["b", "c"]), &none),
            [Splice {
                range: 0..1,
                count: 0
            }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["a", "y", "c"]), &none),
            [Splice {
                range: 1..2,
                count: 1
            }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["c"]), &none),
            [Splice {
                range: 0..2,
                count: 1
            }]
        );
        assert_eq!(
            row_changes(
                &ids(&["a", "b", "c"]),
                &ids(&["a", "b", "c", "d"]),
                &set(&["a", "c", "d"])
            ),
            [
                Splice {
                    range: 3..3,
                    count: 1
                },
                Remeasure(0),
                Remeasure(2)
            ],
            "a new row is measured anyway"
        );
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["a"]), &set(&["b"])),
            [Splice {
                range: 1..2,
                count: 0
            }],
            "a dirty row that left is not remeasured"
        );
    }

    #[test]
    fn image_rows_are_the_rows_holding_an_image_of_the_new_blobs() {
        let mut ops = main_added();
        ops.push(
            json!(["add", "a", "main", null, {"id": "a", "k": "col", "c": [
                {"id": "a1", "k": "image", "p": {"blob": "p1"}}
            ]}]),
        );
        ops.push(json!(["add", "b", "main", null, {"id": "b", "k": "image", "p": {"blob": "p2"}}]));
        let d = doc(ops);
        let blob = |id: &str| TspBlob {
            id: id.into(),
            mime: "image/png".into(),
            data: Vec::new(),
        };
        assert_eq!(
            image_rows(&d, &[blob("p1")]),
            HashSet::from(["a".to_owned()])
        );
        assert_eq!(
            image_rows(&d, &[blob("p2")]),
            HashSet::from(["b".to_owned()])
        );
        assert!(image_rows(&d, &[blob("p9")]).is_empty());
    }

    #[test]
    fn cols_follow_the_width_and_never_go_below_twenty() {
        assert_eq!(cols_for(800.0, 8.0), 100);
        assert_eq!(cols_for(805.0, 8.0), 100, "whole columns only");
        assert_eq!(cols_for(100.0, 8.0), 20, "12 columns is below the minimum");
        assert_eq!(cols_for(0.0, 8.0), 20);
        assert_eq!(cols_for(-5.0, 8.0), 20);
        assert_eq!(cols_for(800.0, 0.0), 20, "no cell width measured yet");
        assert_eq!(cols_for(f32::NAN, 8.0), 20);
        assert_eq!(cols_for(1.0e9, 1.0), u16::MAX, "never wraps around");
    }

    #[test]
    fn the_pane_shows_what_the_session_state_says() {
        use PaneView::{ReadOnly, Starting, Surface, Terminal};
        use TspState::{Fallback, Live, Off, Waiting};
        let table = [
            // (state, exited, has_surface, shown)
            (Off, false, false, Terminal),
            (Off, false, true, Terminal),
            (Off, true, true, Terminal),
            (Waiting, false, false, Starting),
            (Waiting, false, true, Starting),
            (Waiting, true, false, Terminal),
            (Live, false, true, Surface),
            (Live, false, false, Starting),
            (Live, true, true, ReadOnly),
            (Live, true, false, Terminal),
            (Fallback, false, false, Terminal),
            (Fallback, false, true, Terminal),
            (Fallback, true, true, Terminal),
            (Off, true, false, Terminal),
            (Waiting, true, true, Terminal),
            (Fallback, true, false, Terminal),
        ];
        assert_eq!(table.len(), 16, "4 states x exited x has_surface");
        for (state, exited, has_surface, shown) in table {
            assert_eq!(
                pane_view(state, exited, has_surface),
                shown,
                "{state:?} {exited} {has_surface}"
            );
        }
    }

    #[test]
    fn fallback_is_announced_only_on_the_transition_into_it() {
        use TspState::{Fallback, Live, Off, Waiting};
        assert!(enters_fallback(Some(Waiting), Fallback));
        assert!(enters_fallback(Some(Live), Fallback));
        assert!(!enters_fallback(Some(Fallback), Fallback), "already told");
        assert!(!enters_fallback(Some(Off), Fallback));
        assert!(
            !enters_fallback(None, Fallback),
            "first time the session is listed"
        );
        assert!(!enters_fallback(Some(Waiting), Live));
        assert!(!enters_fallback(Some(Live), Off));
    }

    #[test]
    fn the_fallback_toast_names_the_reason_when_there_is_one() {
        assert_eq!(
            fallback_toast(Some("omp offered TSP versions [2], Mkrew speaks 1")),
            "omp is drawing as text: omp offered TSP versions [2], Mkrew speaks 1"
        );
        assert_eq!(fallback_toast(None), "omp is drawing as text");
        assert_eq!(
            fallback_toast(Some("  ")),
            "omp is drawing as text",
            "a blank reason is none"
        );
    }

    #[test]
    fn a_frame_already_in_the_snapshot_is_ignored_not_applied_twice_and_not_a_gap() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 5, &d)]);
        assert_eq!(m.apply(&frame("s:1", 5, add_x())), MirrorResult::Applied);
        assert_eq!(m.apply(&frame("s:1", 3, add_x())), MirrorResult::Applied);
        let (meta, doc) = m.surface("s:1").unwrap();
        assert!(doc.get("x").is_none(), "a stale frame changes nothing");
        assert_eq!(meta.seq, 5);
    }

    #[test]
    fn the_next_frame_applies_and_a_skipped_one_is_a_gap_that_changes_nothing() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 5, &d)]);
        assert_eq!(m.apply(&frame("s:1", 6, add_x())), MirrorResult::Applied);
        let (meta, doc) = m.surface("s:1").unwrap();
        assert!(doc.get("x").is_some());
        assert_eq!(meta.seq, 6);
        assert_eq!(
            m.apply(&frame("s:1", 8, vec![json!(["del", "x"])])),
            MirrorResult::Gap
        );
        let (meta, doc) = m.surface("s:1").unwrap();
        assert!(doc.get("x").is_some(), "a gap frame is not applied");
        assert_eq!(meta.seq, 6);
        assert_eq!(
            m.apply(&frame("s:1", 7, vec![json!(["del", "x"])])),
            MirrorResult::Applied
        );
        assert!(m.surface("s:1").unwrap().1.get("x").is_none());
    }

    #[test]
    fn a_frame_for_an_unknown_surface_is_a_gap() {
        let mut m = mirror(Vec::new());
        assert_eq!(m.apply(&frame("s:9", 1, Vec::new())), MirrorResult::Gap);
    }

    #[test]
    fn a_failing_op_does_not_stop_the_rest_of_the_frame() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 0, &d)]);
        let ops = vec![json!(["del", "nope"]), add_x().remove(0)];
        assert_eq!(m.apply(&frame("s:1", 1, ops)), MirrorResult::Applied);
        assert!(m.surface("s:1").unwrap().1.get("x").is_some());
    }

    #[test]
    fn open_of_an_existing_surface_replaces_its_document_and_resets_the_seq() {
        let mut old = doc(main_added());
        old.apply(&add_x());
        let mut m = mirror(vec![surface("s:1", "inline", 4, &old)]);
        assert!(m.surface("s:1").unwrap().1.get("x").is_some());
        // omp restarted in the pane (daemon startup resume): the same id, an empty root.
        let fresh = Document::new("s:1");
        let open = TspChange::Open {
            surface: surface("s:1", "inline", 0, &fresh),
        };
        assert_eq!(m.apply(&update("s:1", open)), MirrorResult::Applied);
        let (meta, doc) = m.surface("s:1").unwrap();
        assert!(
            doc.get("x").is_none() && doc.get("main").is_none(),
            "replaced, not merged"
        );
        assert_eq!(meta.seq, 0);
        assert_eq!(
            m.apply(&frame("s:1", 1, main_added())),
            MirrorResult::Applied
        );
        assert!(m.surface("s:1").unwrap().1.get("main").is_some());
    }

    #[test]
    fn closed_without_keep_removes_the_surface_and_with_keep_makes_it_read_only() {
        let d = doc(main_added());
        let mut m = mirror(vec![
            surface("s:1", "inline", 1, &d),
            surface("s:2", "screen", 1, &Document::new("s:2")),
        ]);
        assert_eq!(
            m.apply(&update("s:2", TspChange::Closed { keep: false })),
            MirrorResult::Applied
        );
        assert!(m.surface("s:2").is_none());
        assert!(m.surface("s:1").is_some());
        assert!(!m.surface("s:1").unwrap().0.read_only);
        assert_eq!(
            m.apply(&update("s:1", TspChange::Closed { keep: true })),
            MirrorResult::Applied
        );
        let (meta, doc) = m.surface("s:1").unwrap();
        assert!(meta.read_only, "kept read-only");
        assert!(doc.get("main").is_some(), "its document stays");
        assert_eq!(
            m.apply(&update("s:7", TspChange::Closed { keep: false })),
            MirrorResult::Applied,
            "closing an unknown surface is harmless"
        );
    }

    #[test]
    fn a_screen_surface_covers_the_inline_one_until_it_closes() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 1, &d)]);
        assert_eq!(m.shown().unwrap().0.id, "s:1");
        assert_eq!(m.inline_id(), Some("s:1"));
        let open = TspChange::Open {
            surface: surface("s:2", "screen", 0, &Document::new("s:2")),
        };
        m.apply(&update("s:2", open));
        assert_eq!(m.shown().unwrap().0.id, "s:2");
        assert_eq!(
            m.inline_id(),
            Some("s:1"),
            "theme events still go to the inline surface"
        );
        m.apply(&update("s:2", TspChange::Closed { keep: false }));
        assert_eq!(m.shown().unwrap().0.id, "s:1");
        assert!(mirror(Vec::new()).shown().is_none());
    }

    #[test]
    fn blobs_arrive_with_the_snapshot_and_with_applied_frames_only() {
        let d = doc(vec![
            main_added().remove(0),
            json!(["add", "ia", "main", null, {"id": "ia", "k": "image", "p": {"blob": "a"}}]),
            json!(["add", "ib", "main", null, {"id": "ib", "k": "image", "p": {"blob": "b"}}]),
        ]);
        let blob = |id: &str| TspBlob {
            id: id.into(),
            mime: "image/png".into(),
            data: vec![1, 2],
        };
        let mut m = Mirror::from_snapshot(TspSnapshot {
            state: TspState::Live,
            surfaces: vec![surface("s:1", "inline", 3, &d)],
            blobs: vec![blob("a")],
            reason: None,
        });
        assert!(m.blob("a").is_some());
        let with_blob = |seq, id: &str| {
            update(
                "s:1",
                TspChange::Frame {
                    seq,
                    ops: Vec::new(),
                    blobs: vec![blob(id)],
                },
            )
        };
        m.apply(&with_blob(2, "stale"));
        assert!(m.blob("stale").is_none());
        m.apply(&with_blob(4, "b"));
        assert!(m.blob("b").is_some());
        assert!(m.blob("a").is_some(), "still named by an image");
    }

    #[test]
    fn a_bounded_card_scrolls_its_body_unless_a_growing_list_scrolls() {
        let ids = |nodes: Vec<&Node>| nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
        let split = |node| {
            let d = with_node(node);
            bounded_split(&d, d.get("n").unwrap()).map(|s| (ids(s.top), ids(s.body), ids(s.bottom)))
        };
        // The Agent Hub: its growing list is in a row, which bounds nothing.
        let hub = json!({"id": "n", "k": "card", "c": [
            {"id": "t", "k": "tabs"},
            {"id": "h", "k": "row"},
            {"id": "b", "k": "row", "p": {"grow": 1}, "c": [
                {"id": "l", "k": "list", "p": {"grow": 3}}
            ]},
            {"id": "k", "k": "row", "p": {"role": "omp.overlay.hints"}}
        ]});
        let owned = |v: [&str; 1]| v.map(str::to_owned).to_vec();
        assert_eq!(
            split(hub),
            Some((
                owned(["t"]),
                vec!["h".to_owned(), "b".to_owned()],
                owned(["k"])
            ))
        );
        let plain = json!({"id": "n", "k": "card", "c": [{"id": "x", "k": "text"}]});
        assert_eq!(split(plain), Some((vec![], owned(["x"]), vec![])));
        let list = json!({"id": "n", "k": "card", "c": [
            {"id": "x", "k": "text"}, {"id": "l", "k": "list", "p": {"grow": 1}}
        ]});
        assert_eq!(split(list), None, "the list scrolls itself");
        let col = json!({"id": "n", "k": "card", "c": [
            {"id": "c", "k": "col", "p": {"grow": 1}, "c": [
                {"id": "l", "k": "list", "p": {"grow": 1}}
            ]}
        ]});
        assert_eq!(split(col), None, "a growing column bounds its list");
        let still = json!({"id": "n", "k": "card", "c": [{"id": "l", "k": "list"}]});
        assert!(
            split(still).is_some(),
            "a list that does not grow is just content"
        );
    }

    #[test]
    fn a_head_child_listed_first_does_not_hide_the_tabs_in_the_body() {
        let d = with_node(
            json!({"id": "n", "k": "card", "p": {"head": "n/head"}, "c": [
                {"id": "n/head", "k": "text"},
                {"id": "t", "k": "tabs"},
                {"id": "x", "k": "text"},
                {"id": "k", "k": "row", "p": {"role": "omp.overlay.hints"}}
            ]}),
        );
        let s = bounded_split(&d, d.get("n").unwrap()).expect("split");
        let ids = |nodes: Vec<&Node>| nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(s.top), ["t"], "the head child is drawn as the head");
        assert_eq!(ids(s.body), ["x"]);
        assert_eq!(ids(s.bottom), ["k"]);
    }

    #[test]
    fn a_screen_is_bounded_only_around_exactly_one_card() {
        let bounded = |nodes: serde_json::Value| {
            let mut ops = main_added();
            for node in nodes.as_array().unwrap() {
                ops.push(json!(["add", node["id"].clone(), "main", null, node]));
            }
            screen_bounded(&doc(ops))
        };
        assert!(bounded(json!([{"id": "a", "k": "card"}])), "the hub");
        assert!(!bounded(json!([])), "nothing in main");
        assert!(!bounded(json!([{"id": "a", "k": "text"}])), "not a card");
        assert!(
            !bounded(json!([{"id": "a", "k": "card"}, {"id": "b", "k": "text"}])),
            "other content would be clipped"
        );
    }

    #[test]
    fn only_spinner_running_elapsed_and_bars_without_an_amount_animate() {
        let anim = |node| animating(&with_node(node));
        assert!(anim(json!({"id": "n", "k": "spinner"})));
        assert!(anim(json!({"id": "n", "k": "elapsed", "p": {"age": 1000}})));
        assert!(!anim(
            json!({"id": "n", "k": "elapsed", "p": {"age": 1000, "stopped": 5000}})
        ));
        assert!(
            !anim(json!({"id": "n", "k": "shimmer", "p": {"text": "x"}})),
            "a shimmer draws as plain text"
        );
        assert!(
            !anim(json!({"id": "n", "k": "rate", "p": {"value": 3.0, "unit": "tok/s"}})),
            "a rate is a static value omp updates itself"
        );
        assert!(
            !animating(&with_node(json!({
                "id": "n", "k": "row",
                "c": [
                    {"id": "r", "k": "rate", "p": {"value": 3.0, "unit": "tok/s"}},
                    {"id": "e", "k": "elapsed", "p": {"age": 1000, "stopped": 5000}}
                ]
            }))),
            "only a rate and a stopped elapsed: nothing moves"
        );
        assert!(!anim(json!({"id": "n", "k": "text", "p": {"text": "x"}})));
        assert!(!anim(json!({"id": "n", "k": "meter", "p": {"value": 0.5}})));
        assert!(
            anim(json!({
                "id": "n", "k": "col",
                "c": [{"id": "n1", "k": "col", "c": [{"id": "n2", "k": "spinner"}]}]
            })),
            "found deep in the tree"
        );
        assert!(!animating(&doc(main_added())));
    }

    /// `dock` holding a send button, a list with an item, a root-owned row and an editor.
    fn dock_doc() -> Document {
        doc(vec![
            json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col"}]),
            json!(["add", "3.bar/send", "dock", null, {
                "id": "3.bar/send", "k": "kbd",
                "p": {"keys": ["enter"], "actions": {"click": "submit"}}
            }]),
            json!(["add", "3.list", "dock", null, {
                "id": "3.list", "k": "list",
                "c": [{"id": "3.list/a", "k": "item"}]
            }]),
            json!(["add", "dock.x", "dock", null, {
                "id": "dock.x", "k": "row", "p": {"actions": {"click": "x"}}
            }]),
            json!(["add", "3.line/input", "dock", null, {"id": "3.line/input", "k": "editor", "p": {"text": ""}}]),
            json!(["add", "3.plain", "dock", null, {"id": "3.plain", "k": "col"}]),
            json!(["add", "dock.list", "dock", null, {
                "id": "dock.list", "k": "list",
                "c": [{"id": "dock.list/a", "k": "item"}]
            }]),
        ])
    }

    #[test]
    fn nodes_owned_by_a_surface_root_have_no_component() {
        assert!(root_owned("main.0"));
        assert!(root_owned("dock.x/y"));
        assert!(root_owned("layer.^ov"));
        assert!(!root_owned("3.bar/send"));
        assert!(!root_owned("k.line/input"));
        assert!(!root_owned("mainframe.x"), "only the exact base counts");
        assert!(
            !root_owned("main"),
            "the root itself has no dot: not a node of a component either"
        );
    }

    #[test]
    fn a_click_sends_the_nodes_own_action_and_only_that() {
        let d = dock_doc();
        let send = d.get("3.bar/send").unwrap();
        assert_eq!(
            click_event(&d, send, false),
            Some(json!({"ev": "action", "id": "3.bar/send", "act": "submit"}))
        );
        assert_eq!(
            click_event(&d, send, true),
            click_event(&d, send, false),
            "a quick second click on a node without a dblclick action is its click"
        );
        let both = doc(vec![
            json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col"}]),
            json!(["add", "3.two", "dock", null, {
                "id": "3.two", "k": "row",
                "p": {"actions": {"click": "one", "dblclick": "two"}}
            }]),
        ]);
        let two = both.get("3.two").unwrap();
        assert_eq!(
            click_event(&both, two, true),
            Some(json!({"ev": "action", "id": "3.two", "act": "two"})),
            "a dblclick action wins on a double click"
        );
        assert_eq!(
            click_event(&both, two, false),
            Some(json!({"ev": "action", "id": "3.two", "act": "one"}))
        );
        assert_eq!(
            click_event(&d, d.get("3.plain").unwrap(), false),
            None,
            "no actions"
        );
    }

    #[test]
    fn has_click_tells_a_clickable_node_without_building_the_event() {
        let d = dock_doc();
        for (id, clickable) in [
            ("3.bar/send", true),
            ("3.list/a", true),
            ("3.line/input", true),
            ("3.plain", false),
            ("dock.x", false),
            ("dock.list/a", false),
        ] {
            let node = d.get(id).unwrap();
            assert_eq!(has_click(&d, node), clickable, "{id}");
            assert_eq!(
                has_click(&d, node),
                click_event(&d, node, false).is_some() || click_event(&d, node, true).is_some(),
                "{id}: agrees with the events"
            );
        }
    }

    #[test]
    fn a_bar_with_no_amount_animates_but_a_ring_or_a_known_one_does_not() {
        let anim = |node| animating(&with_node(node));
        assert!(anim(json!({"id": "n", "k": "progress"})));
        assert!(anim(json!({"id": "n", "k": "meter", "p": {"label": "x"}})));
        assert!(!anim(
            json!({"id": "n", "k": "progress", "p": {"value": 0.2}})
        ));
        assert!(!anim(
            json!({"id": "n", "k": "meter", "p": {"style": "ring", "label": "9s"}})
        ));
    }

    #[test]
    fn a_failed_snapshot_is_retried_after_a_doubling_wait_up_to_30_seconds() {
        let waits: Vec<u64> = (1..=7).map(|n| retry_wait(n).as_secs()).collect();
        assert_eq!(waits, [2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(retry_wait(0).as_secs(), 0, "no failure, no wait");
        assert_eq!(retry_wait(u32::MAX).as_secs(), 30, "never overflows");
        assert_eq!(retry_wait(10).as_secs(), 30);
    }

    #[test]
    fn a_gap_frame_still_toasts_what_it_adds() {
        let ops = vec![
            json!(["add", "t", "main", null, {"id": "t", "k": "toast", "p": {"text": "saved"}}]),
            json!(["add", "n", "main", null, {"id": "n", "k": "text", "p": {"text": "x"}}]),
            json!(["set", "t", {"text": "unknown without the document"}]),
        ];
        assert_eq!(added_toast_texts(&ops), vec!["saved".to_owned()]);
    }

    #[test]
    fn the_last_scroll_op_of_a_frame_moves_the_list() {
        let ops = |by: &str| vec![json!(["scroll", "main", by])];
        assert_eq!(scroll_in(&ops("start")), Some(ListScroll::Top));
        assert_eq!(scroll_in(&ops("end")), Some(ListScroll::Bottom));
        assert_eq!(scroll_in(&ops("up")), Some(ListScroll::Page { up: true }));
        assert_eq!(
            scroll_in(&ops("pagedown")),
            Some(ListScroll::Page { up: false })
        );
        assert_eq!(scroll_in(&ops("sideways")), None);
        assert_eq!(scroll_in(&[json!(["reveal", "a", "end"])]), None);
        assert_eq!(
            scroll_in(&[
                json!(["scroll", "a", "start"]),
                json!(["scroll", "a", "end"])
            ]),
            Some(ListScroll::Bottom),
            "the last one wins"
        );
    }

    #[test]
    fn the_pty_is_sized_to_the_chat_column_alone_and_to_the_pane_among_several() {
        let pad = 2.0 * theme::CHAT_COLUMN_PADDING;
        assert_eq!(column_px(400.0, false), 400.0 - pad);
        assert_eq!(column_px(5000.0, false), theme::CHAT_COLUMN_WIDTH - pad);
        assert_eq!(column_px(5000.0, true), 5000.0 - pad);
    }

    #[test]
    fn decorations_cut_the_text_into_pieces_by_utf16_offsets() {
        let t = Theme::dark();
        // "é" is 1 UTF-16 unit and 2 bytes: bytes 0..2, then "@bob" at 2..6.
        let text = "é @bob go";
        let pieces = decor_pieces(
            text,
            &[
                json!({"from": 2, "to": 6, "s": "statusLineModel strong"}),
                json!({"from": 7, "to": 9, "s": "typo"}),
                json!({"from": 5, "to": 5, "s": "hide"}),
                json!({"s": "hide"}),
            ],
            &t,
        );
        let ranges: Vec<_> = pieces.iter().map(|(r, _)| r.clone()).collect();
        assert_eq!(ranges, [0..3, 3..7, 7..8, 8..10]);
        assert_eq!(pieces[0].1, Decor::default());
        assert_eq!(pieces[1].1.style.color, Some(t.accent));
        assert!(pieces[1].1.style.bold);
        assert!(pieces[3].1.typo && !pieces[2].1.typo);
        assert_eq!(
            ranges.last().map(|r| r.end),
            Some(text.len()),
            "covers all of it"
        );
        assert!(decor_pieces("", &[], &t).is_empty());
    }

    #[test]
    fn overlapping_decorations_add_up_and_the_first_color_stays() {
        let t = Theme::dark();
        let pieces = decor_pieces(
            "abcd",
            &[
                json!({"from": 0, "to": 4, "s": "accent"}),
                json!({"from": 1, "to": 3, "s": "error strong hide"}),
            ],
            &t,
        );
        assert_eq!(pieces.len(), 3);
        let mid = pieces[1].1;
        assert_eq!(mid.style.color, Some(t.accent));
        assert!(mid.style.bold && mid.hide);
        assert!(!pieces[0].1.hide && !pieces[2].1.hide);
    }

    #[test]
    fn an_overlay_is_as_wide_as_its_size_and_never_wider_than_the_column() {
        let sm = overlay_width(None);
        let md = overlay_width(Some("md"));
        let lg = overlay_width(Some("lg"));
        assert!(sm < md && md < lg);
        assert_eq!(overlay_width(Some("full")), lg);
        assert_eq!(lg, theme::CHAT_COLUMN_WIDTH);
    }

    #[test]
    fn items_of_a_list_select_on_click_and_activate_on_double_click() {
        let d = dock_doc();
        let item = d.get("3.list/a").unwrap();
        assert_eq!(
            click_event(&d, item, false),
            Some(json!({"ev": "select", "id": "3.list", "item": "3.list/a"}))
        );
        assert_eq!(
            click_event(&d, item, true),
            Some(json!({"ev": "activate", "id": "3.list", "item": "3.list/a"}))
        );
        assert_eq!(
            select_event("t", "a", false),
            json!({"ev": "select", "id": "t", "item": "a"})
        );
    }

    #[test]
    fn events_on_root_owned_nodes_are_not_sent_and_editors_send_focus() {
        let d = dock_doc();
        assert_eq!(click_event(&d, d.get("dock.x").unwrap(), false), None);
        assert_eq!(click_event(&d, d.get("dock.list/a").unwrap(), false), None);
        assert_eq!(
            click_event(&d, d.get("3.line/input").unwrap(), false),
            Some(json!({"ev": "focus", "id": "3.line/input"}))
        );
        assert_eq!(toggle_event("main.4", true), None);
        assert_eq!(
            toggle_event("k.card", true),
            Some(json!({"ev": "toggle", "id": "k.card", "collapsed": true}))
        );
    }

    #[test]
    fn reveal_targets_the_top_level_row_of_main_that_holds_the_node() {
        let d = doc(vec![
            json!(["add", "main", "s:1", null, {"id": "main", "k": "col"}]),
            json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col"}]),
            json!(["add", "0", "main", null, {"id": "0", "k": "spacer"}]),
            json!(["add", "1", "main", null, {"id": "1", "k": "col", "c": [{"id": "1.x", "k": "col", "c": [{"id": "1.x/y", "k": "text"}]}]}]),
            json!(["add", "d1", "dock", null, {"id": "d1", "k": "text"}]),
        ]);
        assert_eq!(reveal_index(&d, "0"), Some(0));
        assert_eq!(reveal_index(&d, "1"), Some(1));
        assert_eq!(reveal_index(&d, "1.x/y"), Some(1));
        assert_eq!(reveal_index(&d, "d1"), None, "the dock does not scroll");
        assert_eq!(reveal_index(&d, "main"), None);
        assert_eq!(reveal_index(&d, "nope"), None);
    }

    #[test]
    fn toasts_come_from_added_toasts_and_text_sets_on_toasts_only() {
        let mut d = doc(main_added());
        let ops = vec![
            json!(["add", "u", "main", null, {"id": "u", "k": "toast", "p": {"text": "hello"}}]),
            json!(["add", "q", "main", null, {"id": "q", "k": "toast", "p": {"text": ""}}]),
            json!(["add", "t", "main", null, {"id": "t", "k": "text", "p": {"text": "not a toast"}}]),
            json!(["set", "u", {"text": "bye", "ttl": 1}]),
            json!(["set", "t", {"text": "still not"}]),
            json!(["set", "u", {"ttl": 5}]),
            json!(["set", "u", {"text": null}]),
            json!(["set", "gone", {"text": "unknown id"}]),
        ];
        d.apply(&ops);
        assert_eq!(
            toast_texts(&d, &ops),
            vec!["hello".to_owned(), "bye".to_owned()]
        );
    }

    #[test]
    fn the_text_cache_reparses_only_when_the_revision_of_that_node_changed() {
        let mut cache = TextCache::default();
        let mut parses = 0;
        let mut get = |cache: &mut TextCache<usize>, id: &str, rev: u64| {
            cache.get(id, rev, || {
                parses += 1;
                0
            })
        };
        let a = get(&mut cache, "n", 1);
        let b = get(&mut cache, "n", 1);
        assert!(Rc::ptr_eq(&a, &b), "same revision is the same parse");
        get(&mut cache, "n", 2);
        get(&mut cache, "other", 2);
        assert_eq!(parses, 3);
    }

    fn rev_mirror() -> Mirror {
        let d = doc(vec![
            json!(["add", "main", "s:1", null, {"id": "main", "k": "col"}]),
            json!(["add", "a", "main", null, {"id": "a", "k": "md", "p": {"text": "x"}}]),
            json!(["add", "b", "main", null, {"id": "b", "k": "md", "p": {"text": "y"}}]),
        ]);
        mirror(vec![surface("s:1", "inline", 1, &d)])
    }

    fn apply_frame(m: &mut Mirror, seq: u64, ops: Vec<Value>) {
        assert_eq!(m.apply(&frame("s:1", seq, ops)), MirrorResult::Applied);
    }

    #[test]
    fn a_revision_changes_for_the_node_a_frame_writes_and_no_other() {
        let mut m = rev_mirror();
        let (a0, b0) = (m.rev("s:1", "a"), m.rev("s:1", "b"));
        apply_frame(&mut m, 2, vec![json!(["text", "a", "append", "z"])]);
        let (a1, b1) = (m.rev("s:1", "a"), m.rev("s:1", "b"));
        assert!(a1 > a0, "text");
        assert_eq!(b1, b0);
        apply_frame(&mut m, 3, vec![json!(["splice", "a", 0, 1, "q"])]);
        assert!(m.rev("s:1", "a") > a1, "splice");
        let a2 = m.rev("s:1", "a");
        apply_frame(&mut m, 4, vec![json!(["set", "b", {"text": "w"}])]);
        assert!(m.rev("s:1", "b") > b1, "set");
        assert_eq!(m.rev("s:1", "a"), a2);
        apply_frame(
            &mut m,
            5,
            vec![
                json!(["add", "c", "main", null, {"id": "c", "k": "col", "c": [
                    {"id": "c1", "k": "md", "p": {"text": "n"}}
                ]}]),
            ],
        );
        assert!(m.rev("s:1", "c1") > b1, "a node inside an added tree");
        assert_eq!(m.rev("s:1", "a"), a2);
    }

    #[test]
    fn a_new_snapshot_never_reuses_a_revision() {
        let (m1, m2) = (rev_mirror(), rev_mirror());
        assert_ne!(m1.rev("s:1", "a"), m2.rev("s:1", "a"));
    }

    #[test]
    fn a_gap_during_a_snapshot_request_keeps_a_new_one_wanted() {
        let mut r = Resync::default();
        assert!(r.wanted(false), "no mirror");
        assert!(!r.wanted(true));
        r.gap();
        assert!(r.wanted(true));
        r.begin();
        assert!(!r.wanted(true), "the request answers it");
        r.gap();
        assert!(r.wanted(true), "a gap while the reply is in flight stays");
        r.begin();
        r.failed();
        assert!(r.wanted(true), "a failed request leaves it open");
    }

    #[test]
    fn the_timer_fallback_is_provisional_and_the_toast_says_so() {
        let toast = fallback_toast(Some(mkrew_core::tsp::engine::NO_REPLY_REASON));
        assert!(toast.contains("for now"), "{toast}");
        assert!(toast.contains("may switch to the native view"), "{toast}");
        assert!(
            !toast.contains("no TSP reply"),
            "the reason is not repeated"
        );
        assert_eq!(
            fallback_toast(Some("omp closed its surface and draws rows again")),
            "omp is drawing as text: omp closed its surface and draws rows again"
        );
    }

    #[test]
    fn a_read_only_screen_never_covers_the_live_inline_surface() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 1, &d)]);
        m.apply(&update(
            "s:2",
            TspChange::Open {
                surface: surface("s:2", "screen", 0, &Document::new("s:2")),
            },
        ));
        assert_eq!(m.shown().unwrap().0.id, "s:2");
        m.apply(&update("s:2", TspChange::Closed { keep: true }));
        assert_eq!(
            m.shown().unwrap().0.id,
            "s:1",
            "the closed screen is history"
        );
        // Only a kept screen left (the session exited): its document is still what shows.
        m.apply(&update("s:1", TspChange::Closed { keep: false }));
        assert_eq!(m.shown().unwrap().0.id, "s:2");
    }

    #[test]
    fn a_late_frame_for_a_closed_surface_is_ignored_not_a_gap() {
        let d = doc(main_added());
        let mut m = mirror(vec![
            surface("s:1", "inline", 1, &d),
            surface("s:2", "screen", 1, &d),
        ]);
        m.apply(&update("s:2", TspChange::Closed { keep: false }));
        assert_eq!(
            m.apply(&frame("s:2", 2, add_x())),
            MirrorResult::Applied,
            "closed, then a frame that was already on its way"
        );
        assert!(m.surface("s:2").is_none());
        assert_eq!(
            m.apply(&frame("s:9", 1, add_x())),
            MirrorResult::Gap,
            "a surface never seen is still a gap"
        );
        let reopen = TspChange::Open {
            surface: surface("s:2", "screen", 0, &d),
        };
        m.apply(&update("s:2", reopen));
        assert_eq!(m.apply(&frame("s:2", 1, add_x())), MirrorResult::Applied);
        assert!(
            m.surface("s:2").unwrap().1.get("x").is_some(),
            "reopened id works"
        );
    }

    #[test]
    fn caches_drop_the_entries_of_nodes_that_are_gone() {
        let mut cache = TextCache::<u8>::default();
        cache.get("a", 1, || 1);
        cache.get("b", 1, || 2);
        cache.retain(|id| id == "a");
        assert!(cache.peek("a").is_some());
        assert!(cache.peek("b").is_none());
        let mut map = HashMap::from([("a".to_owned(), 1), ("b".to_owned(), 2)]);
        retain_known(&mut map, |id| id == "b");
        assert_eq!(map.keys().collect::<Vec<_>>(), ["b"]);

        let d = with_node(json!({"id": "x", "k": "text"}));
        let mut m = mirror(vec![surface("s:1", "inline", 1, &d)]);
        assert!(m.has_node("x") && m.has_node("main"));
        m.apply(&frame("s:1", 2, vec![json!(["del", "x"])]));
        assert!(!m.has_node("x"));
    }

    #[test]
    fn only_deleting_changes_make_the_caches_worth_pruning() {
        let del = TspChange::Frame {
            seq: 2,
            ops: vec![json!(["set", "x", {}]), json!(["del", "x"])],
            blobs: Vec::new(),
        };
        let set = TspChange::Frame {
            seq: 2,
            ops: vec![json!(["set", "x", {}])],
            blobs: Vec::new(),
        };
        assert!(removes_nodes(&del));
        assert!(!removes_nodes(&set));
        assert!(removes_nodes(&TspChange::Closed { keep: false }));
        assert!(!removes_nodes(&TspChange::Closed { keep: true }));
    }

    #[test]
    fn the_pty_is_never_fitted_to_an_unknown_width() {
        assert_eq!(
            fit_width(-24.0, 8.0, false),
            None,
            "before the first layout"
        );
        assert_eq!(fit_width(0.0, 8.0, false), None);
        assert_eq!(fit_width(40.0, 8.0, true), None, "no room for one cell");
        assert_eq!(fit_width(500.0, 0.0, false), None);
        assert_eq!(fit_width(500.0, f32::NAN, false), None);
        let cols = fit_width(500.0, 8.0, false).unwrap();
        assert_eq!(cols, cols_for(column_px(500.0, false), 8.0));
        assert!(cols >= 20);
    }

    #[test]
    fn a_section_head_names_a_child_or_is_its_own_text() {
        let d = with_node(
            json!({"id": "y", "k": "section", "p": {"head": "y/head"}, "c": [
                {"id": "y/head", "k": "text", "p": {"text": "Thought"}},
                {"id": "y/body", "k": "md", "p": {"text": "b"}}
            ]}),
        );
        let y = d.get("y").unwrap();
        assert!(matches!(head_of(&d, y), Head::Child(c) if c.id == "y/head"));
        let d = with_node(
            json!({"id": "z", "k": "card", "p": {"head": "Title"}, "c": [
                {"id": "z/0", "k": "text", "p": {"text": "b"}}
            ]}),
        );
        assert!(matches!(head_of(&d, d.get("z").unwrap()), Head::Spans(v) if v == "Title"));
        let d = with_node(json!({"id": "w", "k": "card", "c": []}));
        assert!(matches!(head_of(&d, d.get("w").unwrap()), Head::None));
        let d = with_node(
            json!({"id": "v", "k": "card", "p": {"head": "q/head"}, "c": [
                {"id": "v/head", "k": "spinner"}
            ]}),
        );
        assert!(matches!(head_of(&d, d.get("v").unwrap()), Head::Child(c) if c.kind == "spinner"));
    }

    fn picker() -> Document {
        with_node(
            json!({"id": "13", "k": "card", "p": {"head": "Switch Model"}, "c": [
                {"id": "14", "k": "col", "c": [
                    {"id": "14.list", "k": "list", "p": {"selected": "b", "virtual": true, "grow": 1}, "c": [
                        {"id": "a", "k": "item"},
                        {"id": "14.list/separator", "k": "rule"},
                        {"id": "b", "k": "item", "c": [{"id": "b0", "k": "text"}]}
                    ]}
                ]}
            ]}),
        )
    }

    #[test]
    fn a_reveal_inside_a_list_scrolls_that_list_to_the_item() {
        let d = picker();
        assert_eq!(list_reveal(&d, "b"), Some(("14.list".to_owned(), 2)));
        assert_eq!(list_reveal(&d, "b0"), Some(("14.list".to_owned(), 2)));
        assert_eq!(list_reveal(&d, "14"), None, "not in a list");
        assert_eq!(selected_index(&d, d.get("14.list").unwrap()), Some(2));
        assert_eq!(selected_index(&d, d.get("14").unwrap()), None);
    }

    #[test]
    fn private_use_glyphs_are_cut_out_as_mono_runs() {
        let plain = SpanStyle::default();
        let runs = vec![
            ("272k\u{f111}ok".to_owned(), plain),
            ("x".to_owned(), plain),
        ];
        let out = with_icon_glyphs(&runs);
        let texts: Vec<_> = out.iter().map(|(t, s)| (t.as_str(), s.mono)).collect();
        assert_eq!(
            texts,
            [
                ("272k", false),
                ("\u{f111}", true),
                ("ok", false),
                ("x", false)
            ]
        );
        let none = vec![("abc é".to_owned(), plain)];
        assert!(matches!(
            with_icon_glyphs(&none),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(is_icon_glyph('\u{e70f}') && !is_icon_glyph('A') && is_icon_glyph('\u{f0a9f}'));
    }

    #[test]
    fn a_private_use_glyph_in_an_italic_span_is_drawn_upright() {
        // omp's placeholder: a glyph inside an `em` span. The Nerd Font fallbacks have no
        // italic or bold face, so the glyph run keeps the weight and slant of the terminal font.
        let em = SpanStyle {
            italic: true,
            bold: true,
            ..SpanStyle::default()
        };
        let runs = [("“\u{f0219} README”".to_owned(), em)];
        let out = with_icon_glyphs(&runs);
        let shown: Vec<_> = out
            .iter()
            .map(|(t, s)| (t.as_str(), s.mono, s.italic, s.bold))
            .collect();
        assert_eq!(
            shown,
            [
                ("“", false, true, true),
                ("\u{f0219}", true, false, false),
                (" README”", false, true, true)
            ]
        );
    }

    #[test]
    fn a_code_fence_is_longer_than_any_backtick_run_inside() {
        assert_eq!(code_fence(Some("bash"), "ls"), "```bash\nls\n```");
        assert_eq!(code_fence(None, "ls"), "```\nls\n```");
        assert_eq!(code_fence(Some("md"), "a ``` b"), "````md\na ``` b\n````");
        assert_eq!(
            code_fence(Some("md"), "``````"),
            "```````md\n``````\n```````"
        );
    }

    #[test]
    fn utf16_offsets_map_to_byte_offsets_and_never_split_a_character() {
        assert_eq!(byte_at_utf16("abc", 0), 0);
        assert_eq!(byte_at_utf16("abc", 2), 2);
        assert_eq!(byte_at_utf16("abc", 3), 3);
        assert_eq!(byte_at_utf16("abc", 99), 3, "past the end clamps");
        // "é" is 2 bytes, 1 unit; "😀" is 4 bytes, 2 units.
        assert_eq!(byte_at_utf16("éa", 1), 2);
        assert_eq!(byte_at_utf16("a😀b", 1), 1);
        assert_eq!(
            byte_at_utf16("a😀b", 2),
            1,
            "inside the pair snaps to its start"
        );
        assert_eq!(byte_at_utf16("a😀b", 3), 5);
        assert_eq!(byte_at_utf16("", 4), 0);
    }

    #[test]
    fn effort_fills_four_blocks_by_level() {
        assert_eq!(effort_steps("off"), 0);
        assert_eq!(effort_steps("minimal"), 1);
        assert_eq!(effort_steps("low"), 1);
        assert_eq!(effort_steps("medium"), 2);
        assert_eq!(effort_steps("high"), 3);
        assert_eq!(effort_steps("xhigh"), 4);
        assert_eq!(effort_steps("max"), 4);
        assert_eq!(effort_steps("surprise"), 0);
    }

    #[test]
    fn a_meter_changes_tone_at_its_thresholds() {
        let thresholds = json!({"warn": 0.15, "bad": 0.5});
        assert_eq!(meter_tone(0.014, Some(&thresholds)), "accent");
        assert_eq!(meter_tone(0.15, Some(&thresholds)), "warning");
        assert_eq!(meter_tone(0.49, Some(&thresholds)), "warning");
        assert_eq!(meter_tone(0.5, Some(&thresholds)), "error");
        assert_eq!(meter_tone(0.99, None), "accent");
        assert_eq!(meter_tone(0.99, Some(&json!({"warn": "x"}))), "accent");
    }

    #[test]
    fn rate_and_elapsed_labels() {
        assert_eq!(rate_label(3.26, "tok/s"), "3.3 tok/s");
        assert_eq!(rate_label(9.96, "tok/s"), "10.0 tok/s");
        assert_eq!(rate_label(42.4, "tok/s"), "42 tok/s");
        assert_eq!(
            elapsed_ms(1000, None, 500),
            1500,
            "ticks locally from when it was seen"
        );
        assert_eq!(elapsed_ms(1000, Some(4000), 9999), 4000, "stopped is final");
        assert_eq!(elapsed_label(0), "0s");
        assert_eq!(elapsed_label(59_999), "59s");
        assert_eq!(elapsed_label(60_000), "1m 00s");
        assert_eq!(elapsed_label(3_725_000), "1h 02m");
    }

    #[test]
    fn ctrl_c_copies_only_with_a_selection_otherwise_it_reaches_omp() {
        assert_eq!(copy_decision(true, true), CopyOrSend::Copy);
        assert_eq!(
            copy_decision(false, true),
            CopyOrSend::Send,
            "an interrupt for omp"
        );
        assert_eq!(
            copy_decision(true, false),
            CopyOrSend::Send,
            "other keys are never swallowed"
        );
        assert_eq!(copy_decision(false, false), CopyOrSend::Send);
    }

    #[test]
    fn escape_clears_a_selection_and_otherwise_reaches_omp() {
        assert!(escape_clears(true, true));
        assert!(!escape_clears(false, true), "omp needs its Esc");
        assert!(!escape_clears(true, false));
    }

    #[test]
    fn a_press_makes_a_caret_shift_extends_in_the_same_node_and_a_double_click_takes_a_word() {
        use crate::preview_select::{Point, Selection};
        let shown = crate::docs::Shown::from_text("hello world");
        let at = |byte| Point {
            block: 0,
            piece: 0,
            byte,
        };
        let (key, first) = press(None, "n", &shown, at(2), false, 1);
        assert_eq!(key, "n");
        assert_eq!(first, Selection::caret(at(2)));
        let (_, extended) = press(Some((key.clone(), first)), "n", &shown, at(8), true, 1);
        assert_eq!((extended.anchor, extended.head), (at(2), at(8)));
        let (other, fresh) = press(Some((key, extended)), "m", &shown, at(8), true, 1);
        assert_eq!(other, "m");
        assert_eq!(
            fresh,
            Selection::caret(at(8)),
            "shift does not extend across nodes"
        );
        let (_, word) = press(None, "n", &shown, at(7), false, 2);
        assert!(!word.is_empty(), "a word is selected");
        assert!(word.anchor <= at(7) && at(7) <= word.head);
    }

    #[test]
    fn the_cache_can_be_read_without_parsing() {
        let mut cache = TextCache::default();
        assert!(cache.peek("n").is_none());
        let made = cache.get("n", 1, || 3);
        assert!(Rc::ptr_eq(&made, &cache.peek("n").unwrap()));
        assert!(cache.peek("other").is_none());
    }

    fn checklist(props: serde_json::Value) -> Checklist {
        let props = props.as_object().cloned().unwrap_or_default();
        Checklist::from_props(&props)
    }

    /// The hud props as `interactive-mode.ts` builds them (`collapsed` only on a finished phase).
    fn hud_props() -> serde_json::Value {
        json!({
            "mode": "hud", "role": "omp.hud.todo",
            "phases": [
                {"id": "0", "title": "Thu thập context", "collapsed": true, "items": [
                    {"id": "0.0", "text": "Đọc spec", "status": "done"},
                    {"id": "0.1", "text": "Đọc code", "status": "dropped"}
                ]},
                {"id": "1", "title": "Cài đặt", "items": [
                    {"id": "1.0", "text": "Viết test", "status": "done"},
                    {"id": "1.1", "text": "Viết code", "status": "active"},
                    {"id": "1.2", "text": "Chạy verify", "status": "pending"},
                    {"id": "1.3", "text": "Release", "status": "blocked", "note": "waiting for review"}
                ]}
            ]
        })
    }

    /// The reminder as `todo-reminder.ts` builds it: one phase with a blank name.
    fn reminder_props() -> serde_json::Value {
        json!({
            "mode": "reminder", "role": "omp.notice.todo", "note": "reminder 1/3",
            "phases": [{"id": "p0", "title": "", "collapsed": false, "items": [
                {"id": "p0.0", "text": "Write the plan", "status": "pending"},
                {"id": "p0.1", "text": "Run it", "status": "pending"}
            ]}]
        })
    }

    #[test]
    fn a_checklist_is_read_from_the_props_omp_sends() {
        let list = checklist(hud_props());
        assert_eq!(list.mode, ChecklistMode::Hud);
        assert_eq!(list.note, None);
        assert_eq!(list.phases.len(), 2);
        assert_eq!(list.phases[0].title, "Thu thập context");
        assert!(list.phases[0].collapsed && !list.phases[1].collapsed);
        let items = &list.phases[1].items;
        assert_eq!(
            items.iter().map(|i| i.status).collect::<Vec<_>>(),
            [
                TodoStatus::Done,
                TodoStatus::Active,
                TodoStatus::Pending,
                TodoStatus::Blocked
            ]
        );
        assert_eq!(items[3].note.as_deref(), Some("waiting for review"));
        assert_eq!(items[0].note, None);
        let list = checklist(reminder_props());
        assert_eq!(
            (list.mode, list.note.as_deref()),
            (ChecklistMode::Reminder, Some("reminder 1/3"))
        );
    }

    #[test]
    fn a_checklist_with_missing_or_odd_props_still_reads() {
        assert_eq!(
            checklist(json!({"phases": []})).mode,
            ChecklistMode::Full,
            "no mode is full"
        );
        assert_eq!(
            checklist(json!({"mode": "future", "phases": []})).mode,
            ChecklistMode::Full
        );
        let list = checklist(json!({"mode": "full", "phases": [
            "junk", 7,
            {"title": "T", "items": [{"text": "a", "status": "wat"}, {"id": "x", "status": "done"}, 3]}
        ]}));
        assert_eq!(list.phases.len(), 1, "non-object phases are skipped");
        assert_eq!(
            list.phases[0].id, "p2",
            "a missing id falls back to the entry's index"
        );
        let items = &list.phases[0].items;
        assert_eq!(items.len(), 2);
        assert_eq!(
            (items[0].id.as_str(), items[0].status),
            ("i0", TodoStatus::Pending),
            "unknown status is pending"
        );
        assert_eq!(
            (items[1].text.as_str(), items[1].status),
            ("", TodoStatus::Done)
        );
        assert!(checklist(json!({})).phases.is_empty());
        assert_eq!(
            checklist(json!({"note": ""})).note,
            None,
            "a blank note is none"
        );
    }

    #[test]
    fn done_and_dropped_count_as_finished() {
        let list = checklist(hud_props());
        assert_eq!(list.counts(), (3, 6));
        assert_eq!(list.phases[0].counts(), (2, 2));
        assert_eq!(list.phases[1].counts(), (1, 4));
        assert_eq!(checklist(json!({"phases": []})).counts(), (0, 0));
    }

    #[test]
    fn the_hud_line_names_the_active_item_then_the_first_pending_one_then_done() {
        let line = checklist(hud_props()).hud_line().unwrap();
        assert_eq!(
            line,
            HudLine {
                mark: HudMark::Active,
                text: "Viết code".into(),
                done: 3,
                total: 6
            }
        );
        // No active item: the first pending one, in phase order, with a ring.
        let list = checklist(json!({"mode": "hud", "phases": [
            {"id": "0", "title": "A", "items": [{"id": "0.0", "text": "first", "status": "done"}]},
            {"id": "1", "title": "B", "items": [
                {"id": "1.0", "text": "second", "status": "pending"},
                {"id": "1.1", "text": "third", "status": "pending"}
            ]}
        ]}));
        let line = list.hud_line().unwrap();
        assert_eq!(
            (line.mark, line.text.as_str(), line.done, line.total),
            (HudMark::Ring, "second", 1, 3)
        );
        // Active beats an earlier pending item.
        let list = checklist(
            json!({"mode": "hud", "phases": [{"id": "0", "title": "A", "items": [
                {"id": "0.0", "text": "later", "status": "pending"},
                {"id": "0.1", "text": "now", "status": "active"}
            ]}]}),
        );
        assert_eq!(list.hud_line().unwrap().text, "now");
    }

    #[test]
    fn the_hud_line_says_all_done_only_when_nothing_is_left() {
        let list = checklist(
            json!({"mode": "hud", "phases": [{"id": "0", "title": "A", "collapsed": true, "items": [
                {"id": "0.0", "text": "a", "status": "done"},
                {"id": "0.1", "text": "b", "status": "dropped"}
            ]}]}),
        );
        assert_eq!(
            list.hud_line().unwrap(),
            HudLine {
                mark: HudMark::Done,
                text: "All done".into(),
                done: 2,
                total: 2
            }
        );
        // A blocked item is not done: it is named, not hidden behind "All done".
        let list = checklist(
            json!({"mode": "hud", "phases": [{"id": "0", "title": "A", "items": [
                {"id": "0.0", "text": "a", "status": "done"},
                {"id": "0.1", "text": "stuck", "status": "blocked", "note": "why"}
            ]}]}),
        );
        let line = list.hud_line().unwrap();
        assert_eq!((line.mark, line.text.as_str()), (HudMark::Ring, "stuck"));
        assert_eq!(
            checklist(json!({"mode": "hud", "phases": []})).hud_line(),
            None
        );
        assert_eq!(
            checklist(json!({"mode": "hud", "phases": [{"id": "0", "title": "A", "items": []}]}))
                .hud_line(),
            None,
            "a phase with no items has nothing to show"
        );
    }

    #[test]
    fn roman_numerals_cover_the_phase_numbers_and_fall_back_to_digits() {
        let cases = [
            (1, "I"),
            (2, "II"),
            (3, "III"),
            (4, "IV"),
            (5, "V"),
            (9, "IX"),
            (14, "XIV"),
            (40, "XL"),
            (90, "XC"),
            (400, "CD"),
            (1994, "MCMXCIV"),
            (3999, "MMMCMXCIX"),
        ];
        for (n, want) in cases {
            assert_eq!(roman(n), want, "{n}");
        }
        assert_eq!(roman(0), "0");
        assert_eq!(roman(4000), "4000");
    }

    #[test]
    fn a_phase_shows_working_done_or_open() {
        let list = checklist(hud_props());
        assert_eq!(list.phases[0].mark(), PhaseMark::Done);
        assert_eq!(
            list.phases[1].mark(),
            PhaseMark::Working,
            "it has an active item"
        );
        let list = checklist(json!({"phases": [
            {"id": "a", "title": "A", "items": [{"id": "a0", "text": "x", "status": "pending"}]},
            {"id": "b", "title": "B", "items": [{"id": "b0", "text": "x", "status": "blocked"}]},
            {"id": "c", "title": "C", "items": []}
        ]}));
        assert!(
            list.phases.iter().all(|p| p.mark() == PhaseMark::Open),
            "an empty phase is not done"
        );
    }

    #[test]
    fn a_phase_starts_collapsed_when_omp_says_so_or_when_it_is_finished() {
        let list = checklist(hud_props());
        assert!(list.phases[0].default_collapsed());
        assert!(!list.phases[1].default_collapsed());
        // omp's `full` mode sends `collapsed` itself; a finished phase collapses even without it.
        let list = checklist(json!({"mode": "full", "phases": [
            {"id": "a", "title": "A", "items": [{"id": "a0", "text": "x", "status": "done"}]},
            {"id": "b", "title": "B", "collapsed": true, "items": [{"id": "b0", "text": "x", "status": "pending"}]},
            {"id": "c", "title": "C", "items": []}
        ]}));
        let collapsed: Vec<_> = list
            .phases
            .iter()
            .map(TodoPhase::default_collapsed)
            .collect();
        assert_eq!(collapsed, [true, true, false], "an empty phase stays open");
    }

    #[test]
    fn the_reminders_untitled_phase_has_no_header_and_is_never_collapsed() {
        let list = checklist(reminder_props());
        assert!(list.phases[0].headerless());
        assert!(!list.phases[0].default_collapsed());
        let all_done = checklist(
            json!({"phases": [{"id": "p0", "title": "  ", "collapsed": true, "items": [
                {"id": "p0.0", "text": "x", "status": "done"}
            ]}]}),
        );
        assert!(
            !all_done.phases[0].default_collapsed(),
            "no header, nothing to click to reopen it"
        );
        let line = list.hud_line().unwrap();
        assert_eq!(
            (line.mark, line.text.as_str()),
            (HudMark::Ring, "Write the plan")
        );
    }

    #[test]
    fn a_click_on_a_header_flips_its_default_and_a_second_click_flips_it_back() {
        assert!(collapsed_now(true, false));
        assert!(
            !collapsed_now(true, true),
            "a finished phase opened by the user"
        );
        assert!(
            collapsed_now(false, true),
            "an open phase closed by the user"
        );
        assert!(!collapsed_now(false, false));
    }

    #[test]
    fn each_item_status_has_its_own_mark_color_and_text() {
        let t = Theme::dark();
        let look = |s| item_look(s, &t);
        let active = look(TodoStatus::Active);
        assert_eq!(
            (active.mark, active.mark_color, active.text_color),
            (ItemMark::Active, t.accent, t.accent)
        );
        assert!(!active.strike && active.suffix.is_empty());
        let pending = look(TodoStatus::Pending);
        assert_eq!(
            (pending.mark, pending.mark_color, pending.text_color),
            (ItemMark::Ring, t.text_muted, t.text)
        );
        let done = look(TodoStatus::Done);
        assert_eq!(
            (done.mark, done.mark_color, done.text_color),
            (ItemMark::Check, t.status_done, t.text_muted)
        );
        assert!(!done.strike);
        let dropped = look(TodoStatus::Dropped);
        assert_eq!(
            (dropped.mark, dropped.text_color),
            (ItemMark::Ring, t.text_muted)
        );
        assert!(dropped.strike, "dropped is struck through");
        let blocked = look(TodoStatus::Blocked);
        assert_eq!(
            (blocked.mark_color, blocked.text_color, blocked.suffix),
            (t.status_needs_input, t.status_needs_input, " (blocked)")
        );
    }

    #[test]
    fn a_checklist_with_an_active_item_animates_and_a_finished_one_does_not() {
        let node = |props: serde_json::Value| json!({"id": "n", "k": "checklist", "p": props});
        assert!(
            animating(&with_node(node(hud_props()))),
            "the dots step on the status tick"
        );
        let calm = json!({"mode": "full", "phases": [{"id": "p0", "title": "A", "items": [
            {"id": "p0.0", "text": "x", "status": "pending"}, {"id": "p0.1", "text": "y", "status": "done"}
        ]}]});
        assert!(!animating(&with_node(node(calm))));
    }

    /// The dock card of a one-question `ask`, as `ask-dialog.ts` and `overlay.ts` build it.
    fn ask_tree() -> Document {
        with_node(
            json!({"id": "7", "k": "col", "p": {"role": "omp.editor.ask", "gap": "md"}, "c": [
                {"id": "7.head", "k": "row", "p": {"role": "omp.ask.head", "align": "center"}, "c": [
                    {"id": "7.head/tabs", "k": "tabs", "p": {
                        "items": [{"id": "0", "label": [{"t": "✓ ", "s": "success"}, {"t": "Color"}]}, {"id": "submit", "label": [{"t": "Review"}]}],
                        "active": "0", "role": "omp.ask.questions", "actions": {"click": "select"}
                    }}
                ]},
                {"id": "7.question", "k": "md", "p": {"text": "Which color?", "role": "omp.ask.question"}},
                {"id": "7.q0", "k": "list", "p": {"selected": "b", "role": "omp.ask.options"}, "c": [
                    {"id": "7.q0/a", "k": "item", "p": {"label": [{"t": "Red"}], "role": "omp.ask.option", "value": [{"t": "Recommended"}]}},
                    {"id": "7.q0/b", "k": "item", "p": {"label": [{"t": "Blue"}], "role": "omp.ask.option", "detail": [{"t": "calm"}], "tone": "accent"}},
                    {"id": "7.q0/c", "k": "item", "p": {"label": [{"t": "Green"}], "role": "omp.ask.check", "tone": "accent", "icon": "check"}},
                    {"id": "7.q0/other", "k": "item", "p": {"label": [{"t": "Other"}], "role": "omp.ask.other"}},
                    {"id": "7.q0/otherc", "k": "item", "p": {"label": [{"t": "Other"}], "role": "omp.ask.other.check"}}
                ]},
                {"id": "7.preview", "k": "md", "p": {"text": "preview", "role": "omp.ask.preview"}},
                {"id": "7.notice", "k": "text", "p": {"spans": [{"t": "1 unanswered", "s": "warning"}], "role": "omp.ask.notice"}},
                {"id": "7.answers", "k": "kv", "p": {"items": [{"k": "Color", "v": "Blue"}], "role": "omp.ask.answers"}},
                {"id": "7.actions", "k": "row", "p": {"role": "omp.actions", "gap": "sm", "align": "center"}, "c": [
                    {"id": "7.actions/timer", "k": "meter", "p": {"style": "ring", "size": "sm", "label": "12s", "value": 0.4}},
                    {"id": "7.actions/hide", "k": "row", "p": {"role": "omp.btn", "gap": "xs", "align": "center", "actions": {"click": "hide"}, "title": "Hide  ctrl+h"}, "c": [
                        {"id": "7.actions/hide/0", "k": "text", "p": {"text": "Hide"}}
                    ]},
                    {"id": "7.actions/sp", "k": "spacer", "p": {"grow": 1}},
                    {"id": "7.actions/submit", "k": "row", "p": {"role": "omp.btn", "gap": "xs", "align": "center", "actions": {"click": "submit"}, "title": "Submit  enter", "tone": "accent"}, "c": [
                        {"id": "7.actions/submit/0", "k": "text", "p": {"text": "Submit"}},
                        {"id": "7.actions/submit/1", "k": "kbd", "p": {"keys": ["enter"]}}
                    ]}
                ]}
            ]}),
        )
    }

    fn look_of(d: &Document, id: &str) -> Option<AskLook> {
        d.get(id).and_then(role_of).and_then(ask_look)
    }

    #[test]
    fn every_node_of_omps_ask_card_has_its_look() {
        let d = ask_tree();
        let want = [
            ("7", Some(AskLook::Card)),
            ("7.head", Some(AskLook::Head)),
            ("7.head/tabs", Some(AskLook::Tabs)),
            ("7.question", Some(AskLook::Question)),
            ("7.q0", Some(AskLook::Options)),
            ("7.q0/a", Some(AskLook::Choice)),
            ("7.q0/b", Some(AskLook::Choice)),
            ("7.q0/c", Some(AskLook::Check)),
            ("7.q0/other", Some(AskLook::Other)),
            ("7.q0/otherc", Some(AskLook::OtherCheck)),
            ("7.preview", Some(AskLook::Preview)),
            ("7.notice", Some(AskLook::Notice)),
            ("7.answers", Some(AskLook::Answers)),
            ("7.actions", Some(AskLook::Actions)),
            ("7.actions/hide", Some(AskLook::Button)),
            ("7.actions/submit", Some(AskLook::Button)),
            ("7.actions/timer", None),
            ("7.actions/sp", None),
        ];
        for (id, look) in want {
            assert_eq!(look_of(&d, id), look, "{id}");
        }
    }

    #[test]
    fn the_roles_are_matched_whole_so_neighbours_do_not_collide() {
        assert_eq!(ask_look("omp.editor.ask.hidden"), Some(AskLook::Hidden));
        assert_eq!(ask_look("omp.editor.ask"), Some(AskLook::Card));
        assert_eq!(
            ask_look("omp.overlay.hook-editor"),
            Some(AskLook::Card),
            "a note or custom answer is the ask card"
        );
        assert_eq!(
            ask_look("omp.hook-editor.eyebrow"),
            Some(AskLook::HookEyebrow)
        );
        assert_eq!(ask_look("omp.hook-editor.line"), Some(AskLook::HookLine));
        assert_eq!(ask_look("omp.hook-editor.head"), Some(AskLook::HookHead));
        assert_eq!(ask_look("omp.ask.summary"), Some(AskLook::Summary));
        assert_eq!(
            ask_look("omp.editor"),
            None,
            "the ordinary composer is not an ask card"
        );
        assert_eq!(ask_look("omp.editor.bash"), None);
        assert_eq!(
            ask_look("omp.ask.future"),
            None,
            "an unknown ask role draws as its kind"
        );
        assert_eq!(ask_look("omp.ask.other.checkbox"), None);
        assert_eq!(ask_look("omp.hud.todo"), None);
        assert_eq!(ask_look(""), None);
    }

    #[test]
    fn choice_rows_are_radios_or_check_boxes_and_filled_by_tone() {
        assert!(AskLook::Choice.is_choice() && AskLook::Other.is_choice());
        assert!(AskLook::Check.is_choice() && AskLook::OtherCheck.is_choice());
        assert!(!AskLook::Button.is_choice() && !AskLook::Options.is_choice());
        assert!(AskLook::Check.boxed() && AskLook::OtherCheck.boxed());
        assert!(!AskLook::Choice.boxed() && !AskLook::Other.boxed());
        let d = ask_tree();
        assert!(!choice_filled(d.get("7.q0/a").unwrap()), "not picked");
        assert!(choice_filled(d.get("7.q0/b").unwrap()));
        assert!(choice_filled(d.get("7.q0/c").unwrap()));
    }

    #[test]
    fn the_ask_card_sends_what_omp_reads_back() {
        let d = ask_tree();
        assert_eq!(
            click_event(&d, d.get("7.q0/a").unwrap(), false),
            Some(json!({"ev": "select", "id": "7.q0", "item": "7.q0/a"}))
        );
        assert_eq!(
            click_event(&d, d.get("7.q0/c").unwrap(), true),
            Some(json!({"ev": "activate", "id": "7.q0", "item": "7.q0/c"}))
        );
        assert_eq!(
            click_event(&d, d.get("7.actions/submit").unwrap(), false),
            Some(json!({"ev": "action", "id": "7.actions/submit", "act": "submit"}))
        );
        assert_eq!(
            click_event(&d, d.get("7.actions/hide").unwrap(), false),
            Some(json!({"ev": "action", "id": "7.actions/hide", "act": "hide"}))
        );
        assert_eq!(
            click_event(&d, d.get("7.question").unwrap(), false),
            None,
            "text sends nothing"
        );
        assert_eq!(
            click_event(&d, d.get("7.actions/timer").unwrap(), false),
            None
        );
    }

    #[test]
    fn a_charset_or_other_short_escape_leaves_nothing_and_tput_sgr0_resets() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("a\x1b(Bb", &t),
            vec![run("ab", SpanStyle::default())]
        );
        assert_eq!(
            ansi_runs("\x1b[1mx\x1b(B\x1b[my", &t),
            vec![
                run(
                    "x",
                    SpanStyle {
                        bold: true,
                        ..Default::default()
                    }
                ),
                run("y", SpanStyle::default()),
            ],
            "tput sgr0 is ESC ( B ESC [ m"
        );
        assert_eq!(
            ansi_runs("a\x1b#8b\x1b%Gc", &t),
            vec![run("abc", SpanStyle::default())]
        );
    }

    #[test]
    fn string_sequences_are_skipped_up_to_st_and_an_inner_escape_eats_nothing() {
        let t = Theme::dark();
        for lead in ['P', '_', '^', 'X', ']'] {
            assert_eq!(
                ansi_runs(&format!("a\x1b{lead}payload\x1b\\b"), &t),
                vec![run("ab", SpanStyle::default())],
                "{lead}"
            );
        }
        assert_eq!(
            ansi_runs("a\x1b]0;t\x07b", &t),
            vec![run("ab", SpanStyle::default())],
            "BEL ends an OSC"
        );
        assert_eq!(
            ansi_runs("a\x1bPq\x07b\x1b\\c", &t),
            vec![run("ac", SpanStyle::default())],
            "BEL does not end a DCS"
        );
        assert_eq!(
            ansi_runs("\x1b]0;ti\x1b[31mred", &t),
            vec![run(
                "red",
                SpanStyle {
                    color: Some(t.ansi[1]),
                    ..Default::default()
                }
            )],
            "an ESC that is not ST ends the string and starts the next sequence"
        );
        assert_eq!(
            ansi_runs("a\x1b_never ends\x1b", &t),
            vec![run("a", SpanStyle::default())]
        );
    }

    #[test]
    fn a_colon_group_is_one_parameter_and_only_colors_are_read_from_it() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("\x1b[4:3mU", &t),
            vec![run("U", SpanStyle::default())],
            "curly underline is not italic"
        );
        let runs = ansi_runs(
            "\x1b[38:2::1:2:3mA\x1b[38:2:4:5:6mB\x1b[38:5:196mC\x1b[48:2::9:9:9mD\x1b[4:3;1mE",
            &t,
        );
        let got: Vec<_> = runs
            .iter()
            .map(|(s, st)| (s.as_str(), st.color, st.bold))
            .collect();
        assert_eq!(
            got,
            vec![
                ("A", Some(0x010203), false),
                ("B", Some(0x040506), false),
                ("CD", Some(0xFF0000), false),
                ("E", Some(0xFF0000), true),
            ]
        );
    }

    #[test]
    fn the_newest_opened_screen_wins_not_the_largest_id() {
        let d = doc(main_added());
        let mut m = mirror(vec![surface("s:1", "inline", 1, &d)]);
        let open = |m: &mut Mirror, id: &str| {
            let change = TspChange::Open {
                surface: surface(id, "screen", 0, &Document::new(id)),
            };
            m.apply(&update(id, change));
        };
        open(&mut m, "s:9");
        open(&mut m, "s:10");
        assert_eq!(m.shown().unwrap().0.id, "s:10");
        m.apply(&update("s:10", TspChange::Closed { keep: false }));
        assert_eq!(m.shown().unwrap().0.id, "s:9");
        open(&mut m, "s:10");
        open(&mut m, "s:9");
        assert_eq!(m.shown().unwrap().0.id, "s:9", "reopened last");
    }

    #[test]
    fn after_a_reattach_the_snapshot_is_ordered_by_omps_increasing_ids() {
        let screen = |id: &str| surface(id, "screen", 0, &Document::new(id));
        // The daemon lists surfaces in id-string order: "s:10" before "s:9".
        let snap = mirror(vec![screen("s:10"), screen("s:9")]);
        assert_eq!(snap.shown().unwrap().0.id, "s:10");
        let snap = mirror(vec![screen("x:b"), screen("x:a")]);
        assert_eq!(snap.shown().unwrap().0.id, "x:b", "no number: string order");
    }

    #[test]
    fn an_escape_inside_an_unterminated_csi_ends_it_and_starts_the_next() {
        let t = Theme::dark();
        assert_eq!(
            ansi_runs("\x1b[1mA\x1b[31\x1b[0mtext", &t),
            vec![
                run(
                    "A",
                    SpanStyle {
                        bold: true,
                        ..Default::default()
                    }
                ),
                run("text", SpanStyle::default()),
            ]
        );
    }

    #[test]
    fn blobs_no_image_names_are_dropped_when_blobs_arrive_or_a_surface_closes() {
        let d = doc(vec![
            main_added().remove(0),
            json!(["add", "i", "main", null, {"id": "i", "k": "image", "p": {"blob": "keep"}}]),
        ]);
        let blob = |id: &str| TspBlob {
            id: id.into(),
            mime: "image/png".into(),
            data: vec![1],
        };
        let mut m = mirror(vec![surface("s:1", "inline", 0, &d)]);
        let frame_with = |seq, ids: &[&str]| {
            update(
                "s:1",
                TspChange::Frame {
                    seq,
                    ops: Vec::new(),
                    blobs: ids.iter().map(|i| blob(i)).collect(),
                },
            )
        };
        m.apply(&frame_with(1, &["keep", "orphan"]));
        assert!(m.blob("keep").is_some());
        assert!(m.blob("orphan").is_none(), "no image names it");
        m.apply(&update("s:1", TspChange::Closed { keep: true }));
        assert!(m.blob("keep").is_some(), "a kept surface still shows it");
        m.apply(&update("s:1", TspChange::Closed { keep: false }));
        assert!(m.blob("keep").is_none(), "its surface is gone");
    }

    #[test]
    fn a_huge_age_does_not_overflow() {
        assert_eq!(elapsed_ms(u64::MAX, None, 5), u64::MAX);
    }
}

#[cfg(test)]
mod thought_tests {
    use serde_json::json;

    use super::*;

    fn open_none() -> HashSet<String> {
        HashSet::new()
    }

    fn plan(kinds: &[RowKind], open: &HashSet<String>) -> HashMap<String, RowPlan> {
        let ids: Vec<String> = (0..kinds.len()).map(|i| format!("r{i}")).collect();
        thought_plan(
            ids.iter().map(String::as_str).zip(kinds.iter().copied()),
            open,
        )
    }

    fn lead(label: &str, open: bool) -> RowPlan {
        RowPlan::Lead {
            label: label.into(),
            open,
        }
    }

    use RowKind::{Hidden, Other, Thought};

    #[test]
    fn thoughts_across_hidden_rows_are_one_run() {
        let p = plan(
            &[
                Thought(Some(1000)),
                Hidden,
                Hidden,
                Thought(Some(1500)),
                Hidden,
                Thought(Some(2000)),
            ],
            &open_none(),
        );
        assert_eq!(p["r0"], lead("Thought for 5s", false));
        assert_eq!(p["r3"], RowPlan::Folded);
        assert_eq!(p["r5"], RowPlan::Folded);
        assert_eq!(p.len(), 3, "hidden rows have no entry");
    }

    #[test]
    fn a_shown_row_breaks_the_run_and_a_single_thought_stays_as_it_is() {
        let p = plan(
            &[
                Thought(Some(1000)),
                Other,
                Thought(Some(1000)),
                Thought(Some(1000)),
                Other,
                Thought(Some(1000)),
            ],
            &open_none(),
        );
        assert_eq!(p.get("r0"), None, "alone before a shown row");
        assert_eq!(p["r2"], lead("Thought for 2s", false));
        assert_eq!(p["r3"], RowPlan::Folded);
        assert_eq!(p.get("r5"), None, "alone at the end");
    }

    #[test]
    fn the_sum_rounds_up_to_whole_seconds_with_one_second_at_least() {
        let label = |tooks: &[u64]| {
            let kinds: Vec<RowKind> = tooks.iter().map(|ms| Thought(Some(*ms))).collect();
            match &plan(&kinds, &open_none())["r0"] {
                RowPlan::Lead { label, .. } => label.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(label(&[300, 400]), "Thought for 1s");
        assert_eq!(label(&[0, 0]), "Thought for 1s");
        assert_eq!(label(&[1200, 1200]), "Thought for 3s");
        assert_eq!(label(&[30_000, 31_000]), "Thought for 1m");
    }

    #[test]
    fn a_member_without_took_makes_it_n_times() {
        let p = plan(
            &[
                Thought(Some(1000)),
                Thought(None),
                Hidden,
                Thought(Some(1000)),
            ],
            &open_none(),
        );
        assert_eq!(p["r0"], lead("Thought 3 times", false));
    }

    #[test]
    fn an_open_run_draws_every_member() {
        let open = HashSet::from(["r0".to_owned()]);
        let p = plan(&[Thought(Some(1000)), Thought(Some(1000))], &open);
        assert_eq!(p["r0"], lead("Thought for 2s", true));
        assert_eq!(p.get("r1"), None);
    }

    #[test]
    fn rows_are_classified_from_the_document() {
        let thinking = |id: &str, collapsed: bool, hidden: bool| {
            json!({"id": id, "k": "section", "p": {
                "role": "omp.thinking", "collapsible": true, "collapsed": collapsed,
                "hidden": hidden, "took": 1200,
            }})
        };
        let col = |id: &str, kids: Vec<serde_json::Value>| json!({"id": id, "k": "col", "c": kids});
        let tree = json!({"id": "main", "k": "col", "c": [
            col("a", vec![thinking("a/t", true, false)]),
            col("b", vec![thinking("b/t", false, false)]),
            col("c", vec![thinking("c/t", true, true)]),
            col("d", vec![thinking("d/t", true, false), json!({"id": "d/m", "k": "md"})]),
            json!({"id": "e", "k": "card", "p": {"role": "omp.tool.edit", "hidden": true}}),
            json!({"id": "f", "k": "card", "p": {"role": "omp.tool.edit"}}),
            thinking("g", true, false),
        ]});
        let mut d = Document::new("s:1");
        let applied = d.apply(&[json!(["add", "main", "s:1", null, tree])]);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        let kind = |id: &str| row_kind(&d, d.get(id).unwrap());
        assert_eq!(kind("a"), Thought(Some(1200)));
        assert_eq!(kind("b"), Other, "expanded");
        assert_eq!(kind("c"), Hidden, "its only content is hidden");
        assert_eq!(kind("d"), Other, "text beside the thought");
        assert_eq!(kind("e"), Hidden);
        assert_eq!(kind("f"), Other, "a shown tool");
        assert_eq!(kind("g"), Thought(Some(1200)), "a bare section row");
    }
}

#[cfg(test)]
mod gap_tests {
    use super::RowKind::{Hidden, Other, Thought};
    use super::*;
    use crate::theme::{TSP_ROW_GAP, space};

    #[test]
    fn rows_that_draw_nothing_leave_no_gap() {
        let kinds = [Thought(Some(1000)), Hidden, Thought(Some(1000)), Other];
        let ids = ["r0", "r1", "r2", "r3"];
        let plan = thought_plan(ids.iter().copied().zip(kinds), &HashSet::new());
        let gap = |i: usize| row_gap(kinds[i], plan.get(ids[i]));
        assert_eq!(gap(0), TSP_ROW_GAP, "the lead draws the group head");
        assert_eq!(gap(1), 0.0, "a hidden row");
        assert_eq!(gap(2), 0.0, "folded into the lead");
        assert_eq!(gap(3), TSP_ROW_GAP);

        let open = HashSet::from(["r0".to_owned()]);
        let plan = thought_plan(ids.iter().copied().zip(kinds), &open);
        assert_eq!(
            row_gap(kinds[2], plan.get("r2")),
            TSP_ROW_GAP,
            "an open run draws every member"
        );
    }

    #[test]
    fn an_assistant_column_gets_at_least_a_small_gap() {
        assert_eq!(col_gap(Some("omp.assistant"), None), space::S);
        assert_eq!(col_gap(Some("omp.assistant.turn"), Some("xs")), space::S);
        assert_eq!(
            col_gap(Some("omp.assistant"), Some("lg")),
            space::L,
            "never less than asked"
        );
        assert_eq!(col_gap(Some("omp.tool.bash"), None), 0.0);
        assert_eq!(col_gap(None, Some("md")), space::M);
    }
}
