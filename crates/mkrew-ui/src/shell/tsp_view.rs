//! The pane of an omp session whose native surface is live (ADR 0107, spec 7.3): omp's node tree
//! drawn with Mkrew's theme, clicks sent back as TSP events. The model is `mkrew_ui::tsp`; this
//! file only draws and wires it. The pane's `TerminalPane` stays alive but out of the tree: it is
//! fed the plain bytes (input mode stays right) and every key goes through it to the PTY.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    Anchor, AnyElement, Bounds, ClickEvent, Div, FontStyle, FontWeight, ObjectFit,
    StrikethroughStyle, StyledImage, StyledText, TextRun, anchored, deferred,
};
use mkrew_core::tsp::doc::{Document, Node};
use mkrew_protocol::{
    Request, TspChange, TspEventParams, TspSnapshot, TspSnapshotParams, TspState, TspUpdate,
};
use mkrew_ui::icons::{Icon, icon_svg};
use mkrew_ui::terminal::view::{cell_size_in, exited_notice, terminal_font};
use mkrew_ui::tsp::{
    self, CopyOrSend, ListScroll, Mirror, MirrorResult, PaneView, SpanStyle, TextCache,
};
use mkrew_ui::{docs, preview};
use serde_json::{Value, json};

mod picker;

use super::chat_view::{page_column, prompt_bubble};
use super::composer::{
    composer_card_frame, composer_room, meter_bar, model_pill, permission_chip, placeholder_row,
    quiet_chip,
};
use crate::*;

/// What a pane in TSP mode remembers: the mirror of the daemon's documents and the UI-only state.
pub(crate) struct TspUi {
    /// `None` until the first `tsp.snapshot` reply.
    pub(crate) mirror: Option<Mirror>,
    /// Markdown and code parsed once per text (the preview parse is slow).
    md: RefCell<TextCache<docs::Shown>>,
    /// Decoded `image` blobs by id, with their size in px when the header could be read.
    images: RefCell<HashMap<String, TspImage>>,
    /// `elapsed` nodes: id -> (the `age` last seen, `now` when it was first seen).
    elapsed_seen: RefCell<HashMap<String, (u64, i64)>>,
    missing_icons: RefCell<HashSet<String>>,
    /// The `dark` omp last accepted, and the one on its way (`None`: nothing yet).
    sent_dark: Option<bool>,
    pending_dark: Option<bool>,
    fetch: Option<gpui::Task<()>>,
    /// Snapshot calls that failed in a row, and when the next one may start.
    failures: u32,
    retry_at: Option<Instant>,
    /// A frame could not be applied (a gap): the mirror is behind and wants a snapshot.
    resync: tsp::Resync,
    /// A selection inside one `md` or `code` node (key: node id) and where each was painted.
    selecting: Option<(String, mkrew_ui::preview_select::Selection)>,
    hits: RefCell<HashMap<String, preview::Hits>>,
    /// The hud `checklist` node whose popover is open (UI only, nothing is sent to omp).
    todo_open: Option<String>,
    /// Where the hud chip was painted: a press on it is not "outside" the popover.
    todo_chip: Rc<Cell<Bounds<Pixels>>>,
    /// Phase headers the user clicked: (checklist node id, phase id). Flips the phase's default.
    todo_toggled: HashSet<(String, String)>,
    /// How `main`'s runs of collapsed thoughts are drawn (`tsp::thought_plan`), and the leads
    /// the user opened (UI only).
    thoughts: HashMap<String, tsp::RowPlan>,
    thought_open: HashSet<String>,
    /// The page list (ADR 0114): only the rows on screen (and `TSP_LIST_OVERDRAW` around them)
    /// are laid out. Item 0 and item `n + 1` are spacers; item `i + 1` is row `i` of `main`.
    rows: gpui::ListState,
    /// The ids of `main`'s children the list was last told about, and their surface.
    row_ids: Vec<String>,
    rows_surface: Option<String>,
    /// The theme, fonts and column cap the rows were measured with: a change measures them
    /// again (the list itself only notices a new pane width).
    measured: RefCell<Option<(Theme, theme::Fonts, Option<f32>)>>,
    /// The scroll of each growing `list` (a picker's items scroll inside their card).
    lists: RefCell<HashMap<String, ListState>>,
    /// Each `picker` sheet's list (ADR 0119), by node id.
    pickers: RefCell<HashMap<String, picker::PickerList>>,
    /// The composer field Mkrew edits itself (ADR 0113); `None` while omp edits its own (another
    /// focus, a modal, vim mode, a read-only surface).
    editor: Option<tsp::LocalEditor>,
    /// The attachment chip (`#n`) whose preview is open, and where that chip was painted.
    chip_open: Option<String>,
    chip_at: Rc<Cell<Bounds<Pixels>>>,
}

/// Keeps the composer field Mkrew edits in step with omp's editor of the shown surface, for as
/// long as omp has a composer (keys reach it only while `tsp::local_editor_node` says so).
fn sync_editor(mirror: Option<&Mirror>, editor: &mut Option<tsp::LocalEditor>) {
    let node = mirror
        .and_then(Mirror::shown)
        .filter(|(meta, _)| !meta.read_only)
        .and_then(|(_, doc)| tsp::composer_editor(doc));
    let Some(node) = node else {
        *editor = None;
        return;
    };
    let cursor = (node.props.get("cursor").and_then(Value::as_u64))
        .and_then(|units| usize::try_from(units).ok());
    let text = str_prop(node, "text").unwrap_or("");
    let field = editor.get_or_insert_with(tsp::LocalEditor::default);
    if field.sync(&node.id, text, cursor) {
        tracing::debug!(node = %node.id, len = text.len(), "omp changed the composer text");
    }
}

/// One growing list's scroll and the item it last scrolled to.
struct ListState {
    handle: ScrollHandle,
    selected: Option<String>,
}

/// The scroll handle of list `id`, made on first use.
fn list_handle(lists: &RefCell<HashMap<String, ListState>>, id: &str) -> ScrollHandle {
    let mut lists = lists.borrow_mut();
    let state = lists.entry(id.to_owned()).or_insert_with(|| ListState {
        handle: ScrollHandle::new(),
        selected: None,
    });
    state.handle.clone()
}

/// An empty page list (its two spacers) that follows the end.
fn new_rows() -> gpui::ListState {
    let rows = gpui::ListState::new(2, gpui::ListAlignment::Top, px(theme::TSP_LIST_OVERDRAW));
    rows.set_follow_mode(gpui::FollowMode::Tail);
    rows
}

impl TspUi {
    pub(crate) fn new() -> Self {
        Self {
            mirror: None,
            md: RefCell::new(TextCache::default()),
            images: RefCell::new(HashMap::new()),
            elapsed_seen: RefCell::new(HashMap::new()),
            missing_icons: RefCell::new(HashSet::new()),
            sent_dark: None,
            pending_dark: None,
            fetch: None,
            failures: 0,
            retry_at: None,
            resync: tsp::Resync::default(),
            selecting: None,
            hits: RefCell::new(HashMap::new()),
            todo_open: None,
            todo_chip: Rc::new(Cell::new(Bounds::default())),
            todo_toggled: HashSet::new(),
            thoughts: HashMap::new(),
            thought_open: HashSet::new(),
            rows: new_rows(),
            row_ids: Vec::new(),
            rows_surface: None,
            measured: RefCell::new(None),
            lists: RefCell::new(HashMap::new()),
            pickers: RefCell::new(HashMap::new()),
            editor: None,
            chip_open: None,
            chip_at: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// Scrolls the page list a page (the height it shows) up or down.
    fn page(&self, up: bool) {
        let height = self.rows.viewport_bounds().size.height;
        self.rows.scroll_by(if up { -height } else { height });
    }

    /// Back to the end of the page, following it again.
    fn follow_end(&self) {
        self.rows.set_follow_mode(gpui::FollowMode::Tail);
    }

    /// The page list index of the row holding node `id` of the shown surface.
    fn row_index(&self, id: &str) -> Option<usize> {
        let (_, doc) = self.mirror.as_ref()?.shown()?;
        let row = tsp::row_of(doc, id)?;
        let at = self.row_ids.iter().position(|known| *known == row)?;
        Some(at + 1)
    }

    /// Tells the page list what the mirror's last change did to `main` (ADR 0114). `surface`:
    /// the surface a frame, open or close was for; `None` after a snapshot or when the shown
    /// surface opened again. A reset or a different shown surface resets the list; a change of
    /// the shown surface splices and remeasures; a change of a hidden surface leaves it.
    fn sync_rows(&mut self, surface: Option<&str>, dirty: &HashSet<String>) {
        let Some((meta, doc)) = self.mirror.as_ref().and_then(Mirror::shown) else {
            self.row_ids.clear();
            self.thoughts.clear();
            self.rows_surface = None;
            self.rows.reset(2);
            return;
        };
        let shown = meta.id.clone();
        let same = self.rows_surface.as_deref() == Some(shown.as_str());
        // A frame of a hidden surface leaves the list; a frame that named no row and left `main`
        // alone (the dock, the hud, the working row) has nothing to splice, remeasure or regroup.
        // Neither collects the ids (ADR 0118).
        if same
            && surface.is_some_and(|surface| {
                surface != shown || tsp::rows_unchanged(doc, &self.row_ids, dirty)
            })
        {
            return;
        }
        let new: Vec<String> = doc.children("main").map(|node| node.id.clone()).collect();
        if surface.is_none() || !same {
            // A reset forgets the scroll: a reader of the same surface keeps the row at the top
            // of the page; anyone else follows the end.
            let top = self.rows.logical_scroll_top();
            let kept = (same && !self.rows.is_following_tail())
                .then(|| top.item_ix.checked_sub(1))
                .flatten()
                .and_then(|row| self.row_ids.get(row))
                .and_then(|id| new.iter().position(|known| known == id));
            self.rows.reset(new.len() + 2);
            match kept {
                Some(row) => self.rows.scroll_to(gpui::ListOffset {
                    item_ix: row + 1,
                    offset_in_item: top.offset_in_item,
                }),
                None if !(same && top.item_ix == 0) => self.follow_end(),
                None => {}
            }
        } else {
            for change in tsp::row_changes(&self.row_ids, &new, dirty) {
                match change {
                    tsp::RowChange::Splice { range, count } => {
                        self.rows.splice(range.start + 1..range.end + 1, count);
                    }
                    tsp::RowChange::Remeasure(ix) => self.rows.remeasure_items(ix + 1..ix + 2),
                }
            }
        }
        self.rows_surface = Some(shown);
        self.row_ids = new;
        self.refresh_thoughts();
    }

    /// Groups the shown rows' thoughts again ([`tsp::thought_plan`]). The row count stays: a
    /// folded row draws nothing, so a row whose plan changed is only measured again.
    fn refresh_thoughts(&mut self) {
        let Some((_, doc)) = self.mirror.as_ref().and_then(Mirror::shown) else {
            self.thoughts.clear();
            return;
        };
        let kinds = (self.row_ids.iter())
            .filter_map(|id| doc.get(id))
            .map(|node| (node.id.as_str(), tsp::row_kind(doc, node)));
        let plan = tsp::thought_plan(kinds, &self.thought_open);
        for (ix, id) in self.row_ids.iter().enumerate() {
            if plan.get(id) != self.thoughts.get(id) {
                self.rows.remeasure_items(ix + 1..ix + 2);
            }
        }
        self.thoughts = plan;
    }

    /// Scrolls the page list so its `item` shows. Known heights (the item is laid out or above
    /// the top) reveal it exactly; an item not measured yet would count as 0 px there, so it is
    /// put at the top instead, unless the list follows the end: then it was just added at the
    /// end, and following shows it.
    fn reveal_item(&self, item: usize) {
        let known = self.rows.bounds_for_item(item).is_some()
            || self.rows.item_is_above_viewport(item) == Some(true);
        if known {
            self.rows.pause_following_tail();
            self.rows.scroll_to_reveal_item(item);
        } else if !self.rows.is_following_tail() {
            self.rows.scroll_to(gpui::ListOffset {
                item_ix: item,
                offset_in_item: px(0.),
            });
        }
    }

    /// Drops the cache entries of nodes and blobs the mirror no longer has (after a frame that
    /// deletes, a closed surface or a snapshot), so a long session does not keep them.
    fn prune(&mut self) {
        let Some(mirror) = &self.mirror else { return };
        let known = |id: &str| mirror.has_node(id);
        self.md.get_mut().retain(known);
        tsp::retain_known(self.elapsed_seen.get_mut(), known);
        tsp::retain_known(self.hits.get_mut(), known);
        tsp::retain_known(self.lists.get_mut(), known);
        tsp::retain_known(self.pickers.get_mut(), known);
        self.thought_open.retain(|id| known(id));
        tsp::retain_known(self.images.get_mut(), |id| mirror.blob(id).is_some());
    }
}

impl Shell {
    /// What `info`'s pane shows (spec 7.3 table).
    pub(crate) fn tsp_pane_view(&self, info: &Session) -> PaneView {
        if info.tsp == TspState::Off {
            return PaneView::Terminal;
        }
        let has_surface = (self.tsps.get(&info.id))
            .and_then(|ui| ui.mirror.as_ref())
            .is_some_and(|mirror| mirror.shown().is_some());
        tsp::pane_view(info.tsp, self.is_exited(info.id), has_surface)
    }

    /// The pane's terminal grid is not drawn (the surface, the starting state or the read-only
    /// document is): there is no paint to wait for.
    pub(crate) fn tsp_covers(&self, session: Ulid) -> bool {
        (self.sidebar.sessions().iter())
            .find(|s| s.id == session)
            .is_some_and(|s| self.tsp_pane_view(s) != PaneView::Terminal)
    }

    /// Asks the daemon for the session's surfaces (once at a time; after a failure not before its
    /// back-off has passed; never for an exited session, the daemon answers `invalid_state`).
    fn fetch_tsp(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.is_exited(session) {
            return;
        }
        let ui = self.tsps.entry(session).or_insert_with(TspUi::new);
        if ui.fetch.is_some() || ui.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        ui.resync.begin();
        ui.fetch = Some(cx.spawn(async move |this, cx| {
            let reply = connection
                .call::<TspSnapshot>(Request::TspSnapshot(TspSnapshotParams {
                    session_id: session,
                }))
                .await;
            let _ = this.update(cx, |shell, cx| {
                let Some(ui) = shell.tsps.get_mut(&session) else {
                    return;
                };
                ui.fetch = None;
                match reply {
                    Ok(snapshot) => {
                        ui.mirror = Some(Mirror::from_snapshot(snapshot));
                        sync_editor(ui.mirror.as_ref(), &mut ui.editor);
                        ui.prune();
                        ui.sync_rows(None, &HashSet::new());
                        ui.failures = 0;
                        ui.retry_at = None;
                    }
                    Err(error) => {
                        ui.resync.failed();
                        ui.failures = ui.failures.saturating_add(1);
                        ui.retry_at = Some(Instant::now() + tsp::retry_wait(ui.failures));
                        tracing::warn!(%session, "tsp.snapshot failed: {error:#}");
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Applies a `tsp.update` to the mirror; a gap asks for a new snapshot. Updates for a session
    /// without a mirror are dropped: the snapshot that creates it carries their effect.
    pub(crate) fn tsp_update(&mut self, update: &TspUpdate, cx: &mut Context<'_, Self>) {
        let session = update.session_id;
        let Some(ui) = self.tsps.get_mut(&session) else {
            return;
        };
        let Some(mirror) = ui.mirror.as_mut() else {
            return;
        };
        if mirror.apply(update) == MirrorResult::Gap {
            ui.resync.gap();
            // The frame is lost with the gap and the snapshot does not replay it: the toasts it
            // adds are shown now (a `set` on a toast cannot be told from another node's).
            let toasts = match &update.change {
                TspChange::Frame { ops, .. } => tsp::added_toast_texts(ops),
                _ => Vec::new(),
            };
            self.fetch_tsp(session, cx);
            for text in toasts {
                self.toast_info(text, cx);
            }
            return;
        }
        let mut dirty = (ui.mirror.as_mut())
            .map(Mirror::take_dirty_rows)
            .unwrap_or_default();
        if let TspChange::Frame { blobs, .. } = &update.change
            && !blobs.is_empty()
            && let Some((_, doc)) = (ui.mirror.as_ref()).and_then(|m| m.surface(&update.surface))
        {
            dirty.extend(tsp::image_rows(doc, blobs));
        }
        sync_editor(ui.mirror.as_ref(), &mut ui.editor);
        if (ui.chip_open.as_deref())
            .is_some_and(|id| !ui.mirror.as_ref().is_some_and(|m| m.has_node(id)))
        {
            ui.chip_open = None;
        }
        if tsp::removes_nodes(&update.change) {
            ui.prune();
        }
        // omp opening the shown surface again replaces its rows, ids kept: a reset.
        let shown = (ui.mirror.as_ref())
            .and_then(Mirror::shown)
            .is_some_and(|(meta, _)| meta.id == update.surface);
        let reopened = shown && matches!(update.change, TspChange::Open { .. });
        ui.sync_rows((!reopened).then_some(update.surface.as_str()), &dirty);
        let TspChange::Frame { ops, .. } = &update.change else {
            return;
        };
        let Some((_, doc)) = (ui.mirror.as_ref()).and_then(|m| m.surface(&update.surface)) else {
            return;
        };
        let toasts = tsp::toast_texts(doc, ops);
        if (ui.todo_open.as_deref()).is_some_and(|id| doc.get(id).is_none()) {
            ui.todo_open = None;
        }
        let reveal = ops.iter().rev().find_map(|op| {
            let op = op.as_array()?;
            if op.first()?.as_str()? != "reveal" {
                return None;
            }
            tsp::reveal_index(doc, op.get(1)?.as_str()?)
        });
        // A reveal inside a list moves that list; elsewhere the page list.
        let in_list = ops.iter().rev().find_map(|op| {
            let op = op.as_array()?;
            if op.first()?.as_str()? != "reveal" {
                return None;
            }
            tsp::list_reveal(doc, op.get(1)?.as_str()?)
        });
        if let Some((list, ix)) = in_list {
            list_handle(&ui.lists, &list).scroll_to_item(ix);
        } else if let Some(ix) = reveal.filter(|_| shown) {
            // Item 0 of the page list is the spacer under the pane's drag strip.
            ui.reveal_item(ix + 1);
        }
        // The page list is the shown surface's: a hidden surface's scroll leaves it.
        match tsp::scroll_in(ops).filter(|_| shown) {
            Some(ListScroll::Top) => ui.rows.scroll_to(gpui::ListOffset::default()),
            Some(ListScroll::Bottom) => ui.follow_end(),
            Some(ListScroll::Page { up }) => ui.page(up),
            None => {}
        }
        for text in toasts {
            self.toast_info(text, cx);
        }
    }

    /// The request of one event to omp through the daemon (`tsp.event`), resolving to whether omp
    /// took it; a failure is logged, the pane keeps working. `None` without a connection.
    fn tsp_request(
        &self,
        session: Ulid,
        surface: &str,
        event: Value,
    ) -> Option<std::pin::Pin<Box<dyn Future<Output = bool> + Send>>> {
        let reply = self
            .connection
            .as_ref()?
            .request(Request::TspEvent(TspEventParams {
                session_id: session,
                surface: surface.to_owned(),
                event,
            }));
        Some(Box::pin(async move {
            match reply.await {
                Ok(Outcome::Result(_)) => true,
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session, "tsp.event failed: {}", body.message);
                    false
                }
                Err(error) => {
                    tracing::warn!(%session, "tsp.event failed: {error:#}");
                    false
                }
            }
        }))
    }

    /// Sends one event to omp, fire and forget.
    pub(crate) fn tsp_send(
        &self,
        session: Ulid,
        surface: &str,
        event: Value,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(request) = self.tsp_request(session, surface, event) {
            cx.background_executor().spawn(request).detach();
        }
    }

    /// Tells omp the theme; `sent_dark` is set only when omp took it, so a failure is retried on
    /// the next tick.
    fn tsp_send_theme(
        &mut self,
        session: Ulid,
        surface: &str,
        dark: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(request) =
            self.tsp_request(session, surface, json!({"ev": "theme", "dark": dark}))
        else {
            return;
        };
        if let Some(ui) = self.tsps.get_mut(&session) {
            ui.pending_dark = Some(dark);
        }
        cx.spawn(async move |this, cx| {
            let sent = request.await;
            let _ = this.update(cx, |shell, _| {
                if let Some(ui) = shell.tsps.get_mut(&session) {
                    ui.pending_dark = None;
                    if sent {
                        ui.sent_dark = Some(dark);
                    }
                }
            });
        })
        .detach();
    }

    /// `tsp_send` to the surface the pane draws; nothing for a read-only one or an exited session.
    fn tsp_emit(&self, session: Ulid, event: Value, cx: &mut Context<'_, Self>) {
        if self.is_exited(session) {
            return;
        }
        let Some((meta, _)) = (self.tsps.get(&session))
            .and_then(|ui| ui.mirror.as_ref())
            .and_then(Mirror::shown)
        else {
            return;
        };
        if !meta.read_only {
            self.tsp_send(session, &meta.id, event, cx);
        }
    }

    /// A click (or double click) on node `id` of the pane.
    fn tsp_click(&self, session: Ulid, id: &str, double: bool, cx: &mut Context<'_, Self>) {
        let Some((_, doc)) = (self.tsps.get(&session))
            .and_then(|ui| ui.mirror.as_ref())
            .and_then(Mirror::shown)
        else {
            return;
        };
        if let Some(event) = doc
            .get(id)
            .and_then(|node| tsp::click_event(doc, node, double))
        {
            self.tsp_emit(session, event, cx);
        }
    }

    /// Keeps the TSP state of the panes: drops what a session no longer needs, fetches the
    /// snapshot of each pane on screen that has none (or fell behind), and tells omp the theme.
    /// Called on the status tick. The PTY's width follows the pane in `render_tsp`.
    pub(crate) fn sync_tsps(&mut self, cx: &mut Context<'_, Self>) {
        // Most sessions have no TSP: nothing to keep, nothing to allocate.
        if self.tsps.is_empty()
            && !(self.sidebar.sessions().iter()).any(|s| s.tsp == TspState::Live)
        {
            return;
        }
        let live: HashSet<Ulid> = (self.sidebar.sessions().iter())
            .filter(|s| s.tsp == TspState::Live)
            .map(|s| s.id)
            .collect();
        self.tsps.retain(|id, _| live.contains(id));
        let dark = cx.theme().dark;
        for session in self.visible_sessions() {
            if !live.contains(&session) || self.is_exited(session) {
                continue;
            }
            let Some(ui) = self.tsps.get(&session) else {
                self.fetch_tsp(session, cx);
                continue;
            };
            let fetch = ui.resync.wanted(ui.mirror.is_some());
            let theme_to = (ui.sent_dark != Some(dark) && ui.pending_dark != Some(dark))
                .then(|| ui.mirror.as_ref().and_then(Mirror::inline_id))
                .flatten()
                .map(str::to_owned);
            if fetch {
                self.fetch_tsp(session, cx);
            }
            if let Some(surface) = theme_to {
                self.tsp_send_theme(session, &surface, dark, cx);
            }
        }
    }

    /// The connection is new (the daemon restarted or the link dropped): every mirror may have
    /// missed frames, and a fetch or back-off from the old connection means nothing. Each mirror
    /// stays on screen, marked stale, and the next tick fetches a fresh snapshot over it (no
    /// flash, unlike clearing `tsps`).
    pub(crate) fn tsp_reconnected(&mut self) {
        for ui in self.tsps.values_mut() {
            ui.resync.gap();
            ui.fetch = None;
            ui.retry_at = None;
            ui.failures = 0;
        }
    }

    /// The panes on screen whose content changes with time (the starting dots, a spinner, a
    /// running elapsed, a bar without an amount): the status tick renders them again (ADR 0086,
    /// ADR 0118).
    pub(crate) fn tsp_animating(&self) -> Vec<Ulid> {
        let sessions = self.sidebar.sessions();
        if !sessions
            .iter()
            .any(|s| matches!(s.tsp, TspState::Waiting | TspState::Live))
        {
            return Vec::new();
        }
        (self.visible_sessions().into_iter())
            .filter(|id| {
                sessions
                    .iter()
                    .find(|s| s.id == *id)
                    .is_some_and(|s| match self.tsp_pane_view(s) {
                        PaneView::Starting => true,
                        PaneView::Surface => (self.tsps.get(&s.id))
                            .and_then(|ui| ui.mirror.as_ref())
                            .and_then(Mirror::shown)
                            .is_some_and(|(_, doc)| tsp::animating(doc)),
                        _ => false,
                    })
            })
            .collect()
    }

    /// Mkrew's terminal commands in a pane that shows a surface. The pane's `TerminalPane` is out
    /// of the tree, so its action handler never runs and the shell would swallow the keys: Copy
    /// copies the selection of an `md` or `code` node, Paste goes to the PTY (text, an image or
    /// file paths, as in a terminal pane), PageUp and PageDown scroll the list. Copy mode and link
    /// hints have no grid to work on.
    pub(crate) fn tsp_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self
            .focused_session(window)
            .filter(|session| self.tsp_covers(*session))
        else {
            return;
        };
        match command {
            Command::Copy => {
                if let Some(text) = self.tsp_selected_text(session) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Command::Paste if !self.is_exited(session) => {
                if let Some(pane) = self.panes.get(&session).cloned() {
                    pane.update(cx, |pane, cx| {
                        pane.paste(cx);
                    });
                }
            }
            Command::PageUp | Command::PageDown => {
                if let Some(ui) = self.tsps.get(&session) {
                    ui.page(command == Command::PageUp);
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    /// The pane's keys go to its `TerminalPane` (hidden but alive), exactly like a terminal pane,
    /// except Ctrl+C with a selection (copies), Esc with a selection or a popover (clears it),
    /// and the keys of the composer field Mkrew edits itself (`tsp_compose_key`).
    pub(crate) fn tsp_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self.focused_session(window) else {
            return;
        };
        let selected = self.tsp_selected_text(session);
        let copy_key = matches!(editor::edit_key(&event.keystroke), Some(EditKey::Copy));
        if tsp::copy_decision(selected.is_some(), copy_key) == CopyOrSend::Copy {
            cx.stop_propagation();
            if let Some(text) = selected {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return;
        }
        let escape = !event.keystroke.modifiers.modified() && event.keystroke.key == "escape";
        let popover = (self.tsps.get(&session))
            .is_some_and(|ui| ui.todo_open.is_some() || ui.chip_open.is_some());
        if tsp::escape_clears(popover, escape) {
            cx.stop_propagation();
            if let Some(ui) = self.tsps.get_mut(&session) {
                ui.todo_open = None;
                ui.chip_open = None;
            }
            cx.notify();
            return;
        }
        if tsp::escape_clears(selected.is_some(), escape) {
            cx.stop_propagation();
            if let Some(ui) = self.tsps.get_mut(&session) {
                ui.selecting = None;
            }
            cx.notify();
            return;
        }
        if self.tsp_compose_key(session, event, cx) {
            return;
        }
        let Some(pane) = self.panes.get(&session).cloned() else {
            return;
        };
        pane.update(cx, |pane, cx| pane.key_down(event, window, cx));
    }

    /// A key in the composer field Mkrew edits (ADR 0113): typing, caret moves, the selection,
    /// the clipboard and undo stay in the field, and omp gets the change as one `edit`. `false`
    /// for a key omp takes through the PTY (Enter, Tab, Esc, its completion list and history,
    /// its shortcuts, an image paste), after the field's caret went to omp so the key acts there.
    fn tsp_compose_key(
        &mut self,
        session: Ulid,
        event: &KeyDownEvent,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.is_exited(session) {
            return false;
        }
        let Some(ui) = self.tsps.get(&session) else {
            return false;
        };
        let (Some(field), Some((_, doc))) = (
            ui.editor.as_ref(),
            ui.mirror.as_ref().and_then(Mirror::shown),
        ) else {
            return false;
        };
        // omp edits its own field while its focus is elsewhere, a modal is open or vim mode is on.
        if tsp::local_editor_node(doc).is_none_or(|node| node.id != field.node) {
            return false;
        }
        let key = tsp::compose_key_in(doc, &event.keystroke, &field.input);
        // An image on the clipboard goes to omp, which makes it an attachment (`#n`).
        let pasted = (key == tsp::ComposeKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(field) = (self.tsps.get_mut(&session)).and_then(|ui| ui.editor.as_mut()) else {
            return false;
        };
        let local = match key {
            tsp::ComposeKey::Pty => false,
            tsp::ComposeKey::Paste => pasted.is_some_and(|text| {
                field
                    .input
                    .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
                true
            }),
            tsp::ComposeKey::Copy | tsp::ComposeKey::Cut => {
                if let Some(text) = field.input.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                }
                if key == tsp::ComposeKey::Cut {
                    field.input.cut();
                }
                true
            }
            tsp::ComposeKey::Edit(DialogKey::Insert(text)) => {
                field.input.insert(&text);
                true
            }
            tsp::ComposeKey::Edit(key) => {
                field.input.edit(&key);
                true
            }
        };
        if let Some(edit) = field.edit() {
            self.tsp_emit(session, edit, cx);
        }
        if local {
            cx.stop_propagation();
            cx.notify();
        }
        local
    }

    /// Types `text` into the omp composer field Mkrew edits (a Files row dropped on it): omp gets
    /// the edit. `false` (nothing typed) when the session exited or omp edits its own field
    /// (its focus is elsewhere, a modal is open, vim mode).
    pub(crate) fn tsp_type_in_field(
        &mut self,
        session: Ulid,
        text: &str,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.is_exited(session) {
            return false;
        }
        let Some(ui) = self.tsps.get_mut(&session) else {
            return false;
        };
        let local = (ui.mirror.as_ref().and_then(Mirror::shown))
            .and_then(|(_, doc)| tsp::local_editor_node(doc))
            .map(|node| node.id.clone());
        let Some(field) = (ui.editor.as_mut()).filter(|field| local.as_ref() == Some(&field.node))
        else {
            return false;
        };
        if let Some(edit) = field.type_text(text) {
            self.tsp_emit(session, edit, cx);
        }
        true
    }

    /// A press or drag in the composer field Mkrew edits: the caret goes to byte `at` (omp gets
    /// it with the next edit or key).
    fn tsp_caret(&mut self, session: Ulid, caret: mkrew_ui::field::Caret) {
        if let Some(field) = (self.tsps.get_mut(&session)).and_then(|ui| ui.editor.as_mut()) {
            field.input.click(caret.at, caret.extend, caret.clicks);
        }
    }

    /// The selected text as shown; `None` for no, an empty or a stale selection.
    fn tsp_selected_text(&self, session: Ulid) -> Option<String> {
        let ui = self.tsps.get(&session)?;
        let (id, selection) = ui.selecting.as_ref()?;
        let shown = ui.md.borrow().peek(id)?;
        let text = mkrew_ui::preview_select::copy_text(&shown, selection);
        (!text.is_empty()).then_some(text)
    }

    /// A press on an `md` or `code` node: the pane gets the keyboard (Ctrl+C, Esc, typing), the
    /// caret goes under the pointer (Shift extends, a double click takes the word).
    pub(crate) fn tsp_press(
        &mut self,
        session: Ulid,
        id: String,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        self.focus_session_pane(session);
        if let Some(pane) = self.panes.get(&session) {
            let handle = pane.read(cx).focus_handle();
            window.focus(&handle, cx);
        }
        let Some(ui) = self.tsps.get_mut(&session) else {
            return;
        };
        let at = ui
            .hits
            .borrow()
            .get(&id)
            .and_then(|hits| hits.hit(event.position));
        let shown = ui.md.borrow().peek(&id);
        let current = ui.selecting.take();
        ui.selecting = at.zip(shown).map(|(at, shown)| {
            tsp::press(
                current,
                &id,
                &shown,
                at,
                event.modifiers.shift,
                event.click_count,
            )
        });
        cx.notify();
    }

    /// A press in the list that is not on a text node: no selection to drag from.
    fn tsp_clear_selection(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(ui) = self.tsps.get_mut(&session)
            && ui.selecting.take().is_some()
        {
            cx.notify();
        }
    }

    /// A drag over the list extends the selection inside its own node.
    fn tsp_drag(&mut self, session: Ulid, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        let Some(ui) = self.tsps.get_mut(&session) else {
            return;
        };
        let Some((id, selection)) = ui.selecting.as_mut() else {
            return;
        };
        let at = ui
            .hits
            .borrow()
            .get(id.as_str())
            .and_then(|hits| hits.hit(position));
        if let Some(at) = at {
            selection.head = at;
            cx.notify();
        }
    }

    /// The session fell back to drawing text: asks the daemon why and toasts it.
    pub(crate) fn toast_fallback(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        // The daemon answers `invalid_state` to the snapshot of an exited session.
        let connection = self.connection.clone().filter(|_| !self.is_exited(session));
        let Some(connection) = connection else {
            self.toast_info(tsp::fallback_toast(None), cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let reply = connection
                .call::<TspSnapshot>(Request::TspSnapshot(TspSnapshotParams {
                    session_id: session,
                }))
                .await;
            let reason = reply.ok().and_then(|snapshot| snapshot.reason);
            let _ = this.update(cx, |shell, cx| {
                shell.toast_info(tsp::fallback_toast(reason.as_deref()), cx);
            });
        })
        .detach();
    }
}

impl Shell {
    /// A press on the hud chip: opens its popover, or closes it when it is open (UI only). The
    /// open state is read live: the outside press handler skips the chip, so it has not closed it.
    fn tsp_todo_toggle(&mut self, session: Ulid, id: &str, cx: &mut Context<'_, Self>) {
        if let Some(ui) = self.tsps.get_mut(&session) {
            ui.todo_open = match ui.todo_open {
                Some(_) => None,
                None => Some(id.to_owned()),
            };
            cx.notify();
        }
    }

    fn tsp_todo_close(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(ui) = self.tsps.get_mut(&session)
            && ui.todo_open.take().is_some()
        {
            cx.notify();
        }
    }

    /// A click on a phase header: opens or closes that phase (UI only).
    fn tsp_todo_phase(
        &mut self,
        session: Ulid,
        node: &str,
        phase: &str,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(ui) = self.tsps.get_mut(&session) else {
            return;
        };
        let key = (node.to_owned(), phase.to_owned());
        if !ui.todo_toggled.remove(&key) {
            ui.todo_toggled.insert(key);
        }
        if let Some(ix) = ui.row_index(node) {
            ui.rows.remeasure_items(ix..ix + 1);
        }
        cx.notify();
    }

    /// A click on the head of a run of thoughts: opens or closes the run (UI only).
    fn tsp_thought_toggle(&mut self, session: Ulid, row: &str, cx: &mut Context<'_, Self>) {
        let Some(ui) = self.tsps.get_mut(&session) else {
            return;
        };
        if !ui.thought_open.remove(row) {
            ui.thought_open.insert(row.to_owned());
        }
        ui.refresh_thoughts();
        cx.notify();
    }
}

/// Everything one node builder needs.
struct Paint<'a> {
    session: Ulid,
    doc: &'a Document,
    mirror: &'a Mirror,
    /// The id of the surface `doc` belongs to.
    shown_id: &'a str,
    ui: &'a TspUi,
    now: i64,
    theme: &'a Theme,
    focused: bool,
    /// A modal overlay is open over the page.
    modal: bool,
    /// The node is drawn inside the overlay layer.
    in_layer: bool,
    /// The node's height is bounded by the pane (the overlay layer, a `screen` surface): columns
    /// and rows clip, so a growing list scrolls inside instead of the page growing.
    bounded: bool,
    /// The pane shares its height with others and has no keyboard: the composer is its field
    /// alone (the status row, the meter, the chips and the bar come back with the keyboard).
    compact: bool,
    /// The pane's width in px (a picker hides fact columns that do not fit); unbounded for a
    /// row of the page list, which is laid out without it.
    width: f32,
    /// The px the transcript's column leaves its content ([`tsp::column_px`]): an image is at
    /// most this wide.
    column: f32,
}

impl Paint<'_> {
    /// Tooltips of the page under a modal overlay would float over it.
    fn tips(&self) -> bool {
        self.in_layer || !self.modal
    }
}

/// The width of one terminal cell, measured once per font and size.
fn cell_width(text_system: &gpui::TextSystem, theme: &Theme) -> Pixels {
    thread_local! {
        static CELL: RefCell<Option<(String, u32, Pixels)>> = const { RefCell::new(None) };
    }
    let size = theme.terminal_font_size.to_bits();
    CELL.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some((family, have, width)) = cell.as_ref()
            && *have == size
            && *family == *theme.terminal_font_family
        {
            return *width;
        }
        let (width, _) = cell_size_in(text_system, theme);
        *cell = Some((theme.terminal_font_family.to_string(), size, width));
        width
    })
}

fn str_prop<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.props.get(key).and_then(Value::as_str)
}

fn bool_prop(node: &Node, key: &str) -> bool {
    node.props.get(key).and_then(Value::as_bool) == Some(true)
}

fn num_prop(node: &Node, key: &str) -> Option<f64> {
    node.props.get(key).and_then(Value::as_f64)
}

/// An array prop (`items`, `cols`, `rows`), read in place; empty when absent.
fn array_prop<'a>(node: &'a Node, key: &str) -> &'a [Value] {
    node.props
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// `runs` as one styled text: weights, slants, strike and mark per run; `mono` runs (and all runs
/// when `all_mono`) in the terminal font with the glyph fallbacks, the rest in the UI font.
fn styled_runs(
    runs: &[(String, SpanStyle)],
    base: u32,
    all_mono: bool,
    theme: &Theme,
) -> StyledText {
    styled_runs_w(runs, base, all_mono, weight::REGULAR, theme)
}

/// [`styled_runs`] with `regular` as the weight of runs that are not `strong`.
fn styled_runs_w(
    runs: &[(String, SpanStyle)],
    base: u32,
    all_mono: bool,
    regular: f32,
    theme: &Theme,
) -> StyledText {
    let (mut text, mut text_runs) = (String::new(), Vec::new());
    push_runs(
        &mut text,
        &mut text_runs,
        runs,
        base,
        all_mono,
        regular,
        false,
        theme,
    );
    StyledText::new(text).with_runs(text_runs)
}

/// Appends `runs` to `text` and its `text_runs` (see [`styled_runs`]), with a wavy `danger_text`
/// underline under every run when `typo`.
#[allow(clippy::too_many_arguments)]
fn push_runs(
    text: &mut String,
    text_runs: &mut Vec<TextRun>,
    runs: &[(String, SpanStyle)],
    base: u32,
    all_mono: bool,
    regular: f32,
    typo: bool,
    theme: &Theme,
) {
    // Private-use glyphs (omp's icons) are cut into runs of their own and drawn in the bundled
    // icon font itself: on Windows gpui applies only the first run's font fallbacks to a whole
    // line, so a glyph after UI text (omp's placeholder `“<icon> title”`) would be a box.
    let runs = tsp::with_icon_glyphs(runs);
    let ui_font = gpui::font(theme.ui_font_family.clone());
    let mono_font = terminal_font(theme);
    let icon_font = gpui::font(theme::ICON_FONT);
    for (part, style) in runs.iter().filter(|(part, _)| !part.is_empty()) {
        let mut font = if part.chars().all(tsp::is_icon_glyph) {
            icon_font.clone()
        } else if style.mono || all_mono {
            mono_font.clone()
        } else {
            ui_font.clone()
        };
        font.weight = FontWeight(regular);
        if style.bold {
            font.weight = FontWeight(weight::SEMIBOLD);
        }
        if style.italic {
            font.style = FontStyle::Italic;
        }
        text_runs.push(TextRun {
            len: part.len(),
            font,
            color: rgb(style.color.unwrap_or(base)).into(),
            background_color: style.mark.then(|| rgb(theme.accent_bg).into()),
            underline: typo.then(|| gpui::UnderlineStyle {
                thickness: px(theme::BORDER_WIDTH),
                color: Some(rgb(theme.danger_text).into()),
                wavy: true,
            }),
            strikethrough: style.strike.then(|| StrikethroughStyle {
                thickness: px(theme::BORDER_WIDTH),
                color: None,
            }),
        });
        text.push_str(part);
    }
}

fn image_format(mime: &str) -> Option<gpui::ImageFormat> {
    match mime {
        "image/png" => Some(gpui::ImageFormat::Png),
        "image/jpeg" => Some(gpui::ImageFormat::Jpeg),
        "image/gif" => Some(gpui::ImageFormat::Gif),
        "image/webp" => Some(gpui::ImageFormat::Webp),
        _ => None,
    }
}

impl Shell {
    /// The root of a TSP pane: it has the keyboard through the hidden terminal's focus handle, so
    /// keys (and the platform's text input) reach omp from the starting state on.
    fn tsp_root(&self, handle: &FocusHandle, theme: &Theme, cx: &mut Context<'_, Self>) -> Div {
        let focus = handle.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            .key_context(mkrew_ui::keymap::TERMINAL_CONTEXT)
            .track_focus(handle)
            .on_key_down(cx.listener(Self::tsp_key_down))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
    }

    /// The pane of an omp session in TSP mode: the surface, the starting state, or (exited) the
    /// last document read-only. `stacked`: the pane shares the tab's height with another, so its
    /// composer is compact while the pane has no keyboard. `several`: other panes are shown
    /// beside it, so the transcript spans the pane (`tsp::column_max`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_tsp(
        &self,
        session: Ulid,
        is_focused: bool,
        stacked: bool,
        width: f32,
        several: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let starting = || {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(space::M))
                .child(loading_dots(now, rgb(theme.status_working)))
                .child(message(theme, None, "Starting omp…".into(), None))
        };
        let Some(pane) = self.panes.get(&session) else {
            return starting().size_full().into_any_element();
        };
        // The grid is never painted, so nothing else sizes the PTY: omp gets its `resize` from the
        // pane's width as soon as the pane has one (the starting state included).
        let cell_width = cell_width(cx.text_system(), theme);
        if let Some(cols) = tsp::fit_width(width, f32::from(cell_width), several) {
            pane.update(cx, |pane, cx| pane.fit_cols(cols, cx));
        }
        let handle = pane.read(cx).focus_handle();
        let exited = self
            .is_exited(session)
            .then(|| pane.read(cx).exit_code().flatten());
        let root = self.tsp_root(&handle, theme, cx);
        let shown = self
            .tsps
            .get(&session)
            .and_then(|ui| (ui.mirror.as_ref()).and_then(|m| m.shown().map(|s| (ui, m, s))));
        let Some((ui, mirror, (meta, doc))) = shown else {
            return root
                .items_center()
                .justify_center()
                .child(starting())
                // Platform text input (IME, Unikey) feeds the same key path.
                .child(self.typing(&handle, Self::tsp_key_down, cx))
                .into_any_element();
        };
        // The rows were measured with another theme, font or column cap: measure them again
        // (ADR 0114).
        let fonts = cx.global::<theme::Fonts>();
        let max = tsp::column_max(several);
        let column = tsp::column_px(width, several);
        let stale = (ui.measured.borrow().as_ref())
            .is_none_or(|(t, f, m)| t != theme || f != fonts || *m != max);
        if stale {
            ui.rows.remeasure();
            ui.measured
                .replace(Some((theme.clone(), fonts.clone(), max)));
        }
        // Text hit areas are recorded again by the rows painted in this frame; a row scrolled
        // out must not keep its old ones.
        ui.hits.borrow_mut().clear();
        let paint = Paint {
            session,
            doc,
            mirror,
            shown_id: &meta.id,
            ui,
            now,
            theme,
            focused: is_focused,
            modal: doc.children("layer").any(|n| !bool_prop(n, "hidden")),
            in_layer: false,
            bounded: false,
            compact: stacked && !is_focused,
            width,
            column,
        };
        let layer_paint = Paint {
            in_layer: true,
            bounded: true,
            ..paint
        };
        // A `screen` surface of one card (the Agent Hub) is bounded by the pane, not scrolled; any
        // other screen keeps the scrolling page.
        let screen = meta.mode == "screen" && tsp::screen_bounded(doc);
        let screen_paint = Paint {
            bounded: true,
            ..paint
        };
        // The page is a list (ADR 0114): only the rows on screen are laid out. The top spacer
        // keeps the first row clear of the pane's 8 px drag strip (`PANE_GRIP_HEIGHT`), the 2 px
        // color line and the badges, like the Chat list's `py(space::L)`; the bottom one parts
        // the last message from the status row and the meter.
        // Alone, the transcript is in the Chat column; among several panes it spans the pane like
        // Tern (`tsp::column_max`). The dock below uses the same `max`.
        let screen_rows: Vec<AnyElement> = if screen {
            (doc.children("main"))
                .map(|node| {
                    page_column(self.tsp_node(&screen_paint, node, cx), max, true)
                        .into_any_element()
                })
                .collect()
        } else {
            Vec::new()
        };
        // The dock as the Chat composer when omp sent its composer roles. With the ask card or a
        // note editor where the editor stood, the status row stays Tern's and the card sits where
        // the composer would. Any other dock (another omp version) is drawn generically.
        let dock_model = tsp::dock_model(doc, theme);
        let status_dock = dock_model
            .is_none()
            .then(|| tsp::status_dock(doc, theme))
            .flatten();
        let card_dock = dock_model.is_some() || status_dock.is_some();
        let dock: Vec<AnyElement> = match (dock_model.as_ref(), status_dock.as_ref()) {
            (Some(m), _) => self.tsp_dock(&paint, m, tsp::column_px(width, several), cx),
            (None, Some(d)) => self.tsp_dock_card(&paint, d, cx),
            (None, None) => (doc.children("dock"))
                .map(|node| self.tsp_node(&paint, node, cx))
                .collect(),
        };
        let layer: Vec<AnyElement> = (doc.children("layer"))
            .filter(|node| !bool_prop(node, "hidden"))
            .map(|node| self.tsp_layer_child(&layer_paint, node, cx))
            .collect();
        root.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child({
                    let main = div()
                        .id(SharedString::from(format!("tsp-main-{session}")))
                        .size_full()
                        // A drag extends the selection; a press off any text clears it.
                        .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
                            if event.dragging() {
                                shell.tsp_drag(session, event.position, cx);
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |shell, _: &MouseDownEvent, _, cx| {
                                shell.tsp_clear_selection(session, cx);
                            }),
                        );
                    if screen {
                        main.flex()
                            .flex_col()
                            .pt(px(space::L))
                            .pb(px(space::M))
                            .overflow_hidden()
                            .children(screen_rows)
                    } else {
                        let shell = cx.entity().downgrade();
                        let count = ui.row_ids.len();
                        let row_theme = theme.clone();
                        let compact = paint.compact;
                        main.child(
                            gpui::list(ui.rows.clone(), move |ix, _, cx| {
                                // The top spacer keeps the first row clear of the drag strip;
                                // the bottom one is what is left of the dock gap (every row
                                // that draws something carries `TSP_ROW_GAP` below it).
                                if ix == 0 {
                                    return div().h(px(space::L)).into_any_element();
                                }
                                if ix > count {
                                    return div().h(px(space::S)).into_any_element();
                                }
                                shell
                                    .update(cx, |shell, cx| {
                                        shell.tsp_page_row(
                                            session,
                                            ix - 1,
                                            is_focused,
                                            compact,
                                            max,
                                            column,
                                            now,
                                            &row_theme,
                                            cx,
                                        )
                                    })
                                    .unwrap_or_else(|_| div().into_any_element())
                            })
                            .size_full(),
                        )
                    }
                })
                .when(!screen, |el| {
                    // The list follows the end again only while it lays out (the user scrolled
                    // back to the bottom); the button below was drawn before that: draw again.
                    let rows = ui.rows.clone();
                    let following = rows.is_following_tail();
                    el.child(
                        canvas(
                            move |_, window, _| {
                                if rows.is_following_tail() != following {
                                    window.request_animation_frame();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_0(),
                    )
                })
                .when(!ui.rows.is_following_tail() && !screen, |el| {
                    el.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(space::S))
                            .child(page_column(
                                div().flex().justify_end().child(
                                    scroll_end_button("tsp-scroll-end", theme)
                                        .tooltip(tooltip(
                                            theme,
                                            "Scroll to the latest message",
                                            "End",
                                        ))
                                        .on_press(
                                            true,
                                            cx.listener(move |shell, _, _, cx| {
                                                if let Some(ui) = shell.tsps.get(&session) {
                                                    ui.follow_end();
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                ),
                                max,
                                false,
                            )),
                    )
                }),
        )
        .child(
            page_column(
                div()
                    .flex()
                    .flex_col()
                    .when(!card_dock, |el| el.pb(px(space::S)))
                    .children(dock),
                max,
                false,
            )
            // The card sits `space::L` above the pane's bottom edge and spans the transcript
            // column like the rows above it.
            .when(card_dock, |el| el.pb(px(space::L))),
        )
        // The session ended: the last document stays, read-only, under the usual notice.
        .children(exited.map(|code| exited_notice(code, theme)))
        .when(!layer.is_empty(), |el| {
            el.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(gpui::rgba(theme.scrim))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(space::L))
                    .gap(px(space::M))
                    .children(layer),
            )
        })
        // Platform text input (IME, Unikey) feeds the same key path.
        .child(self.typing(&handle, Self::tsp_key_down, cx))
        .into_any_element()
    }

    /// Row `index` of the shown surface's `main`, in its column: one item of the page list.
    #[allow(clippy::too_many_arguments)]
    fn tsp_page_row(
        &self,
        session: Ulid,
        index: usize,
        focused: bool,
        compact: bool,
        max: Option<f32>,
        column: f32,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let shown = (self.tsps.get(&session))
            .and_then(|ui| (ui.mirror.as_ref()).and_then(|m| m.shown().map(|s| (ui, m, s))));
        let Some((ui, mirror, (meta, doc))) = shown else {
            return div().into_any_element();
        };
        let Some(node) = ui.row_ids.get(index).and_then(|id| doc.get(id)) else {
            return div().into_any_element();
        };
        let paint = Paint {
            session,
            doc,
            mirror,
            shown_id: &meta.id,
            ui,
            now,
            theme,
            focused,
            modal: doc.children("layer").any(|n| !bool_prop(n, "hidden")),
            in_layer: false,
            bounded: false,
            compact,
            width: f32::INFINITY,
            column,
        };
        let plan = ui.thoughts.get(&node.id);
        let el = match plan {
            // Folded into the lead's head: the row draws nothing.
            Some(tsp::RowPlan::Folded) => return div().into_any_element(),
            Some(tsp::RowPlan::Lead { label, open }) => {
                let (session, id) = (session, node.id.clone());
                let head = thinking_head_el(
                    theme,
                    format!("tsp-run-{session}-{id}"),
                    label.clone(),
                    Some(!open),
                )
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| shell.tsp_thought_toggle(session, &id, cx)),
                );
                let el = div().flex().flex_col().gap(px(space::XS)).child(head);
                if *open {
                    el.child(self.tsp_node(&paint, node, cx))
                } else {
                    el
                }
                .into_any_element()
            }
            None => self.tsp_node(&paint, node, cx),
        };
        let gap = tsp::row_gap(tsp::row_kind(doc, node), plan);
        page_column(el, max, false).pb(px(gap)).into_any_element()
    }

    /// A child of `layer`: an `overlay` is a modal card placed by its `anchor` and `size`; any other
    /// node (a sheet omp was not asked for) is drawn as is.
    #[inline(never)]
    fn tsp_layer_child(
        &self,
        p: &Paint<'_>,
        node: &Node,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        if node.kind != "overlay" {
            return self.tsp_node(p, node, cx);
        }
        let kids = match tsp::bounded_split(p.doc, node) {
            Some(split) => self.tsp_scroll_body(p, node, split, cx),
            None => self.tsp_children(p, node, cx),
        };
        Self::tsp_overlay(p, node, kids)
    }

    /// The modal card of an `overlay` around its built children.
    #[inline(never)]
    fn tsp_overlay(p: &Paint<'_>, node: &Node, kids: Vec<AnyElement>) -> AnyElement {
        // Bounded to the pane: the card keeps its head and hints, a growing list scrolls inside.
        let holder = div().w_full().h_full().flex().flex_col().items_center();
        let holder = match str_prop(node, "anchor") {
            Some("top") => holder.justify_start(),
            Some("bottom") => holder.justify_end(),
            _ => holder.justify_center(),
        };
        holder
            .child(
                div()
                    .w_full()
                    .max_w(px(tsp::overlay_width(str_prop(node, "size"))))
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .p(px(space::M))
                    .rounded(px(theme::radius::MEDIUM))
                    // `surface`, so the picked row (`selected` = `surface_raised`) shows on it.
                    .bg(rgb(p.theme.surface))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(p.theme.border))
                    .children(kids),
            )
            .into_any_element()
    }

    /// The children of a card bounded to its pane whose body scrolls ([`tsp::bounded_split`]):
    /// the leading tabs and the hints stay in place, the rest is one scroll box.
    #[inline(never)]
    fn tsp_scroll_body(
        &self,
        p: &Paint<'_>,
        node: &Node,
        split: tsp::BoundedSplit<'_>,
        cx: &mut Context<'_, Self>,
    ) -> Vec<AnyElement> {
        let mut build = |nodes: Vec<&Node>| -> Vec<AnyElement> {
            nodes.into_iter().map(|n| self.tsp_node(p, n, cx)).collect()
        };
        let top = build(split.top);
        let body = build(split.body);
        let bottom = build(split.bottom);
        Self::tsp_scroller(p, node, top, body, bottom)
    }

    /// `top` and `bottom` keep their height; the scroll box is the only part of the card that
    /// grows or shrinks. Its one inner column does not clip and does not shrink its children, so
    /// a body taller than the box overflows it and scrolls.
    #[inline(never)]
    fn tsp_scroller(
        p: &Paint<'_>,
        node: &Node,
        top: Vec<AnyElement>,
        body: Vec<AnyElement>,
        bottom: Vec<AnyElement>,
    ) -> Vec<AnyElement> {
        let fixed = |el: AnyElement| div().flex_none().child(el).into_any_element();
        let scroller = div()
            .id(SharedString::from(format!(
                "tsp-body-{}-{}",
                p.session, node.id
            )))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(div().flex().flex_col().gap(px(space::S)).children(body))
            .into_any_element();
        (top.into_iter().map(fixed))
            .chain(std::iter::once(scroller))
            .chain(bottom.into_iter().map(fixed))
            .collect()
    }
}

impl Shell {
    /// One node. One builder per kind group; a kind outside P1's list draws generically.
    fn tsp_node(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        if bool_prop(node, "hidden") {
            return div().into_any_element();
        }
        // The ask card's roles (spec 7.6) and the status row that holds the todo hud (7.7).
        if let Some(look) = tsp::role_of(node).and_then(tsp::ask_look) {
            return self.tsp_ask(p, node, look, cx);
        }
        if node.kind == "col"
            && tsp::role_of(node) == Some("omp.hud.status")
            && p.doc
                .children(&node.id)
                .any(|child| child.kind == "checklist")
        {
            return self.tsp_status_hud(p, node, cx);
        }
        match node.kind.as_str() {
            "col" | "row" | "card" | "section" | "rule" | "spacer" | "overlay" | "toast" => {
                self.tsp_layout(p, node, cx)
            }
            "text" | "md" | "code" | "ansi" | "rows" | "shimmer" => self.tsp_text(p, node, cx),
            "kv" | "table" | "list" | "item" | "tabs" | "badge" | "kbd" | "icon" | "status"
            | "seg" => self.tsp_data(p, node, cx),
            "checklist" => self.tsp_checklist(p, node, cx),
            "spinner" | "elapsed" | "progress" | "rate" | "meter" | "effort" => {
                self.tsp_live(p, node, cx)
            }
            "editor" | "input" | "image" => self.tsp_input(p, node, cx),
            "picker" => self.tsp_picker(p, node, cx),
            _ => self.tsp_generic(p, node, cx),
        }
    }

    fn tsp_children(
        &self,
        p: &Paint<'_>,
        node: &Node,
        cx: &mut Context<'_, Self>,
    ) -> Vec<AnyElement> {
        p.doc
            .children(&node.id)
            .filter(|child| !bool_prop(child, "hidden"))
            .map(|child| self.tsp_node(p, child, cx))
            .collect()
    }

    /// Adds the node's `title` tooltip and its click: an element with either is a `Stateful` div
    /// that follows the mkrew-ui-design press rules (`.pressable` + `.on_press`).
    fn tsp_decorate(
        &self,
        el: Div,
        p: &Paint<'_>,
        node: &Node,
        look: Look,
        picked: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let title = str_prop(node, "title");
        let clickable = tsp::has_click(p.doc, node);
        let title = title.filter(|_| p.tips());
        if title.is_none() && !clickable {
            return el.into_any_element();
        }
        let mut el = el.id(SharedString::from(format!("tsp-{}-{}", p.session, node.id)));
        if clickable {
            let (session, id) = (p.session, node.id.clone());
            el = el.pressable_unless(picked, look, p.theme).on_press(
                true,
                cx.listener(move |shell, event: &ClickEvent, _, cx| {
                    shell.tsp_click(session, &id, event.click_count() > 1, cx);
                }),
            );
        }
        if let Some(title) = title.filter(|_| p.tips()) {
            el = el.tooltip(tooltip(p.theme, title, ""));
        }
        el.into_any_element()
    }

    /// Alignment, justification, grow and wrap props of a flex node.
    fn tsp_flow(el: Div, node: &Node) -> Div {
        let el = match str_prop(node, "align") {
            Some("start") => el.items_start(),
            Some("center") => el.items_center(),
            Some("end") => el.items_end(),
            _ => el,
        };
        let el = match str_prop(node, "justify") {
            Some("end") => el.justify_end(),
            Some("between") => el.justify_between(),
            _ => el,
        };
        let el = if bool_prop(node, "wrap") {
            el.flex_wrap()
        } else {
            el
        };
        if tsp::grows(node) { el.flex_1() } else { el }
    }

    /// col, row, card, section, rule, spacer, overlay, toast.
    fn tsp_layout(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        match node.kind.as_str() {
            // Toasts go to Mkrew's toast layer when their frame arrives (`tsp_update`).
            "toast" => div().into_any_element(),
            "spacer" => Self::tsp_spacer(node),
            "rule" => Self::tsp_rule(p, node),
            "row" => self.tsp_row(p, node, cx),
            "card" | "section" => self.tsp_card(p, node, cx),
            // "col" and an `overlay` met outside `layer` (an autocomplete under the editor).
            _ => self.tsp_col(p, node, cx),
        }
    }

    #[inline(never)]
    fn tsp_spacer(node: &Node) -> AnyElement {
        let size = tsp::gap_px(str_prop(node, "size"));
        let el = if tsp::grows(node) {
            div().flex_1()
        } else {
            div().h(px(size)).w(px(size))
        };
        el.into_any_element()
    }
    #[inline(never)]
    fn tsp_rule(p: &Paint<'_>, node: &Node) -> AnyElement {
        let theme = p.theme;
        let label = (node.props.get("label"))
            .map(|label| tsp::spans_of(label, theme))
            .unwrap_or_default();
        let line = || {
            div()
                .flex_1()
                .h(px(theme::BORDER_WIDTH))
                .bg(rgb(theme.divider))
        };
        if label.is_empty() {
            return div()
                .w_full()
                .py(px(space::S))
                .child(line())
                .into_any_element();
        }
        div()
            .w_full()
            .py(px(space::S))
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(line())
            .child(styled_runs(&label, theme.text_muted, false, theme))
            .child(line())
            .into_any_element()
    }
    #[inline(never)]
    fn tsp_row(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        if tsp::role_of(node).and_then(tsp::dock_role) == Some(tsp::DockRole::QueueItem) {
            return self.tsp_queue_item(p, node, cx);
        }
        let kids = self.tsp_children(p, node, cx);
        self.tsp_row_el(p, node, kids, cx)
    }

    /// One queued text (`omp.queue.item`): a `TSP_QUEUE_ITEM_HEIGHT` pill with a `layer[0]` fill,
    /// its icon at 14 px in `text_subtle`, the text in META `text_muted` clipped at the row's end,
    /// then whatever omp put after it (the count, the edit chip) drawn generically.
    #[inline(never)]
    fn tsp_queue_item(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let kids: Vec<AnyElement> = (p.doc.children(&node.id))
            .filter(|child| !bool_prop(child, "hidden"))
            .map(|child| match child.kind.as_str() {
                "icon" => str_prop(child, "name")
                    .and_then(|name| self.tsp_icon(p, name, theme.text_subtle))
                    .map_or_else(
                        || div().into_any_element(),
                        |icon| div().flex_none().child(icon).into_any_element(),
                    ),
                "text" => div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.tsp_node(p, child, cx))
                    .into_any_element(),
                _ => self.tsp_node(p, child, cx),
            })
            .collect();
        let el = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .min_w_0()
            .h(theme::rems_px(theme::TSP_QUEUE_ITEM_HEIGHT))
            .pl(px(space::M))
            .pr(px(space::XS))
            .rounded(px(theme::radius::PILL))
            .bg(gpui::rgba(theme.layer[0]))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .children(kids);
        // `Look::Text`: the pill's own fill is `layer[0]`, which the `Chip` hover would not show on.
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }

    #[inline(never)]
    fn tsp_row_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        kids: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let gap = px(tsp::gap_px(str_prop(node, "gap")));
        let el = div().flex().flex_row().items_center().gap(gap).min_w_0();
        let el = Self::tsp_flow(el, node).children(kids);
        let el = if p.bounded { el.overflow_hidden() } else { el };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_col(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let kids = self.tsp_children(p, node, cx);
        self.tsp_col_el(p, node, kids, cx)
    }

    #[inline(never)]
    fn tsp_col_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        kids: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let gap = px(tsp::col_gap(tsp::role_of(node), str_prop(node, "gap")));
        let mut el = div().flex().flex_col().gap(gap).min_w_0();
        if tsp::role_of(node).and_then(tsp::dock_role) == Some(tsp::DockRole::Queue) {
            el = el.gap(px(space::XS)).pb(px(space::S));
        }
        if node.kind == "overlay" {
            el = el
                .p(px(space::M))
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border));
            if let Some(head) = node.props.get("head") {
                let head = tsp::spans_of(head, theme);
                el = el.child(styled_runs(&head, theme.text, false, theme));
            }
        }
        // In the overlay a column is bounded by its card: its growing list scrolls.
        let el = if p.bounded { el.overflow_hidden() } else { el };
        let el = Self::tsp_flow(el, node).children(kids);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }

    /// card (a raised box) and section (a titled group); `collapsible` ones toggle on their head.
    fn tsp_card(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let head_child = match tsp::head_of(p.doc, node) {
            tsp::Head::Child(child) => Some(child),
            _ => None,
        };
        let head_el = self.tsp_card_head(p, node, cx);
        let bounded = p.bounded && !p.in_layer && node.parent.as_deref() == Some("main");
        let split = bounded.then(|| tsp::bounded_split(p.doc, node)).flatten();
        let body: Vec<AnyElement> = if bool_prop(node, "collapsed") {
            Vec::new()
        } else if let Some(split) = split {
            self.tsp_scroll_body(p, node, split, cx)
        } else {
            let tool = tsp::is_tool(node);
            (p.doc.children(&node.id))
                .filter(|child| head_child.is_none_or(|head| head.id != child.id))
                .filter(|child| !bool_prop(child, "hidden"))
                .map(|child| {
                    let el = self.tsp_node(p, child, cx);
                    if tool && matches!(child.kind.as_str(), "ansi" | "rows") {
                        tool_well(el, p.theme)
                    } else {
                        el
                    }
                })
                .collect()
        };
        self.tsp_card_el(p, node, head_el, body, cx)
    }

    #[inline(never)]
    fn tsp_card_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        head_el: Option<AnyElement>,
        body: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let card = node.kind == "card";
        if card && !p.in_layer && tsp::is_user_prompt(node) {
            return self.tsp_user_prompt(p, node, head_el, body, cx);
        }
        let mut el = div().flex().flex_col().gap(px(space::S)).min_w_0();
        if p.bounded {
            // The pane (or the overlay container) bounds this card: its list or body scrolls.
            el = el.overflow_hidden();
        }
        // A tool row is plain rows on the page (Tern); other cards keep their raised frame.
        if !p.in_layer && card && !tsp::is_tool(node) {
            el = el
                .p(px(space::M))
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface_raised));
        }
        let el = Self::tsp_flow(el, node).children(head_el);
        let el = if tsp::is_thinking(node) && !body.is_empty() {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .min_w_0()
                    .pl(px(space::L))
                    .children(body),
            )
        } else {
            el.children(body)
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }

    /// The prompt card of the user: Tern's right-aligned bubble; its head row (copy, rewind,
    /// time) leaves the bubble and shows under it while the row is hovered.
    fn tsp_user_prompt(
        &self,
        p: &Paint<'_>,
        node: &Node,
        head_el: Option<AnyElement>,
        body: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("tsp-user-{}-{}", p.session, node.id));
        let body = div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .min_w_0()
            .children(body);
        let bubble = prompt_bubble(body, head_el.map(|head| (group, head)), p.theme);
        self.tsp_decorate(bubble, p, node, Look::Text, false, cx)
    }

    /// The head of a card or section: its `head` spans or its head child; `None` without one.
    #[inline(never)]
    fn tsp_card_head(
        &self,
        p: &Paint<'_>,
        node: &Node,
        cx: &mut Context<'_, Self>,
    ) -> Option<AnyElement> {
        let head_source = tsp::head_of(p.doc, node);
        let head_child = match head_source {
            tsp::Head::Child(child) => Some(child),
            _ => None,
        };
        let head = match head_source {
            tsp::Head::Spans(head) => tsp::spans_of(head, p.theme),
            _ => Vec::new(),
        };
        if head.is_empty() && head_child.is_none() {
            return None;
        }
        // A thinking head is its label only (omp's span colors are not used); a live one keeps
        // its spinner child.
        if tsp::is_thinking(node) {
            let label = match head_child {
                Some(child) if child.kind == "text" => Some(tsp::node_spans(&child.props, p.theme)),
                Some(_) => None,
                None => Some(head.clone()),
            };
            if let Some(runs) = label {
                let label: String = runs.into_iter().map(|(text, _)| text).collect();
                return Some(Self::tsp_thinking_head(p, node, label, cx));
            }
        }
        let child = head_child.map(|child| self.tsp_node(p, child, cx));
        Some(Self::tsp_card_head_row(p, node, &head, child, cx))
    }

    /// The head of an `omp.thinking*` section; a collapsible one toggles on a press.
    fn tsp_thinking_head(
        p: &Paint<'_>,
        node: &Node,
        label: String,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let collapsible = bool_prop(node, "collapsible");
        let collapsed = bool_prop(node, "collapsed");
        let key = format!("tsp-head-{}-{}", p.session, node.id);
        let head = thinking_head_el(p.theme, key, label, collapsible.then_some(collapsed));
        if !collapsible {
            return head.into_any_element();
        }
        let (session, id) = (p.session, node.id.clone());
        head.on_press(
            true,
            cx.listener(move |shell, _, _, cx| {
                if let Some(event) = tsp::toggle_event(&id, !collapsed) {
                    shell.tsp_emit(session, event, cx);
                }
            }),
        )
        .into_any_element()
    }

    /// The head row around its built child or spans; a collapsible one toggles on a press.
    #[inline(never)]
    fn tsp_card_head_row(
        p: &Paint<'_>,
        node: &Node,
        head: &[(String, SpanStyle)],
        child: Option<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let collapsible = bool_prop(node, "collapsible");
        let collapsed = bool_prop(node, "collapsed");
        let user = tsp::is_user_prompt(node);
        let tool = tsp::is_tool(node);
        let head_color = if user {
            theme.text_subtle
        } else {
            match str_prop(node, "tone") {
                // A tool title is `text` unless it failed; other heads keep omp's tone.
                Some(tone) if !tool || tone == "error" => tsp::tone_color(tone, theme),
                _ => theme.text,
            }
        };
        let group = SharedString::from(format!("tsp-hg-{}-{}", p.session, node.id));
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .group(group.clone());
        if user {
            row = row.justify_end();
        }
        if tool {
            row = row
                .min_h(theme::rems_px(theme::TSP_HEAD_HEIGHT))
                .mx(px(-space::XS))
                .px(px(space::XS))
                .rounded(px(theme::radius::SMALL));
        }
        if collapsible {
            let chevron = if collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            let color = if tool {
                theme.text_subtle
            } else {
                theme.text_muted
            };
            let mark = icon_svg(chevron, theme::ICON_SIZE_SMALL, color);
            // A tool card's chevron shows while its head is hovered.
            row = if tool {
                row.child(
                    mark.opacity(theme::opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(theme::opacity::FULL)),
                )
            } else {
                row.child(mark)
            };
        }
        let row = match child {
            // A tool title is MEDIUM (Tern); the child keeps its own color.
            Some(child) if tool => {
                row.child(div().font_weight(FontWeight(weight::MEDIUM)).child(child))
            }
            Some(child) => row.child(child),
            None if tool => row.child(styled_runs_w(
                head,
                head_color,
                false,
                weight::MEDIUM,
                theme,
            )),
            None => row.child(
                div()
                    .when(!user, |el| el.font_weight(FontWeight(weight::SEMIBOLD)))
                    .child(styled_runs(head, head_color, false, theme)),
            ),
        };
        if !collapsible {
            return row.into_any_element();
        }
        let (session, id) = (p.session, node.id.clone());
        row.id(SharedString::from(format!(
            "tsp-head-{}-{}",
            p.session, node.id
        )))
        .pressable(if tool { Look::Row } else { Look::Text }, true, theme)
        .on_press(
            true,
            cx.listener(move |shell, _, _, cx| {
                if let Some(event) = tsp::toggle_event(&id, !collapsed) {
                    shell.tsp_emit(session, event, cx);
                }
            }),
        )
        .into_any_element()
    }
}

impl Shell {
    /// text, md, code, ansi, rows, shimmer.
    fn tsp_text(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        match node.kind.as_str() {
            "md" | "code" => self.tsp_md(p, node, cx),
            "ansi" | "rows" => self.tsp_ansi(p, node, cx),
            _ => self.tsp_plain(p, node, cx),
        }
    }

    #[inline(never)]
    fn tsp_md(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        // A thinking body is `text_subtle`: paragraphs take `theme.text`, so give them that tier.
        // ponytail: one Theme clone per painted thinking body; a Flavor variant if it shows up in a profile.
        let thought = parent_is(p, node, tsp::is_thinking);
        let thought_theme;
        let theme = if thought {
            thought_theme = Theme {
                text: p.theme.text_subtle,
                ..p.theme.clone()
            };
            &thought_theme
        } else {
            p.theme
        };
        let text = str_prop(node, "text").unwrap_or("");
        // The cache keys on the node's revision; a fence is built on a miss only.
        let (kind, lang) = (node.kind.as_str(), str_prop(node, "lang"));
        let rev = p.mirror.rev(p.shown_id, &node.id);
        let shown = p.ui.md.borrow_mut().get(&node.id, rev, || match kind {
            "code" => docs::Shown::from_text(&tsp::code_fence(lang, text)),
            _ => docs::Shown::from_text(text),
        });
        // Selectable inside this node as in Chat: the painted texts go to `hits`.
        let hits =
            p.ui.hits
                .borrow_mut()
                .entry(node.id.clone())
                .or_default()
                .clone();
        let selection = (p.ui.selecting.as_ref())
            .filter(|(id, _)| *id == node.id)
            .map(|(_, selection)| *selection);
        let blocks = preview::message(
            &shown,
            selection,
            &hits,
            preview::Flavor::Chat { session: p.session },
            theme,
        );
        let (session, id) = (p.session, node.id.clone());
        // The prompt bubble keeps the UI's 1.5.
        let line_height = if parent_is(p, node, tsp::is_user_prompt) {
            theme::UI_LINE_HEIGHT
        } else {
            theme::doc::CHAT_LINE_HEIGHT
        };
        let el = div()
            .when(thought && kind == "md", |el| el.italic())
            .flex()
            .flex_col()
            .gap(px(space::M))
            .line_height(relative(line_height))
            .min_w_0()
            .children(blocks)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.tsp_press(session, id.clone(), event, window, cx);
                }),
            );
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_ansi(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let lines: Vec<&str> = if node.kind == "rows" {
            array_prop(node, "lines")
                .iter()
                .filter_map(Value::as_str)
                .collect()
        } else {
            str_prop(node, "text").unwrap_or("").split('\n').collect()
        };
        let rows = lines.iter().map(|line| {
            let mut runs = tsp::ansi_runs(line, theme);
            if runs.is_empty() {
                // An empty line keeps its height.
                runs.push((" ".to_owned(), SpanStyle::default()));
            }
            div()
                .whitespace_nowrap()
                .child(styled_runs(&runs, theme.text, true, theme))
        });
        let el = div()
            .flex()
            .flex_col()
            .min_w_0()
            .text_size(px(theme.terminal_font_size))
            .children(rows);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    /// text and shimmer (plain text in P1).
    #[inline(never)]
    fn tsp_plain(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let thought = parent_is(p, node, tsp::is_thinking);
        // A thinking body is `text_subtle` and italic (here and in `tsp_md`, never on its code cards).
        let base = str_prop(node, "tone").map_or(
            if thought {
                theme.text_subtle
            } else {
                theme.text
            },
            |tone| tsp::tone_color(tone, theme),
        );
        let runs = tsp::node_spans(&node.props, theme);
        let mut el = div()
            .min_w_0()
            .when(thought, |el| el.italic())
            .child(styled_runs(&runs, base, false, theme));
        if node.props.get("wrap").and_then(Value::as_str) == Some("none") {
            el = el.whitespace_nowrap();
            if p.bounded {
                // A line in a card shortens with an ellipsis, it never draws past the card.
                el = el.overflow_hidden().text_ellipsis();
            }
        }
        if str_prop(node, "truncate").is_some() {
            el = el.truncate();
        }
        if tsp::grows(node) {
            el = el.flex_1();
        }
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
}

impl Shell {
    /// A named icon (or nothing, logged once at `debug`).
    fn tsp_icon(&self, p: &Paint<'_>, name: &str, color: u32) -> Option<AnyElement> {
        match tsp::icon(name) {
            Some(icon) => Some(icon_svg(icon, theme::ICON_SIZE_SMALL, color).into_any_element()),
            None => {
                if p.ui.missing_icons.borrow_mut().insert(name.to_owned()) {
                    tracing::debug!(icon = name, "TSP icon without an entry");
                }
                None
            }
        }
    }

    /// kv, table, list, item, tabs, badge, kbd, icon, status, seg.
    fn tsp_data(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        match node.kind.as_str() {
            "kv" => self.tsp_kv(p, node, cx),
            "table" => self.tsp_table(p, node, cx),
            "list" => self.tsp_list(p, node, cx),
            "item" => self.tsp_item(p, node, cx),
            "tabs" => self.tsp_tabs(p, node, cx),
            "badge" => self.tsp_badge(p, node, cx),
            "kbd" => self.tsp_kbd(p, node, cx),
            "icon" => self.tsp_icon_node(p, node, cx),
            "status" => self.tsp_status(p, node, cx),
            _ => self.tsp_seg(p, node, cx),
        }
    }

    #[inline(never)]
    fn tsp_kv(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let items = array_prop(node, "items");
        let rows = items.iter().map(|item| {
            let spans = |key: &str| {
                item.get(key)
                    .map(|v| tsp::spans_of(v, theme))
                    .unwrap_or_default()
            };
            div()
                .flex()
                .gap(px(space::M))
                .child(
                    div()
                        .w(px(theme::TSP_KEY_WIDTH))
                        .flex_none()
                        .child(styled_runs(&spans("k"), theme.text_muted, false, theme)),
                )
                .child(div().flex_1().min_w_0().child(styled_runs(
                    &spans("v"),
                    theme.text,
                    false,
                    theme,
                )))
        });
        let el = div().flex().flex_col().gap(px(space::XS)).children(rows);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_table(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let cols = array_prop(node, "cols");
        let rows = array_prop(node, "rows");
        let cell = |col: &Value, runs: Vec<(String, SpanStyle)>, color: u32, head: bool| {
            let mut el = div().flex_1().min_w_0().truncate();
            if col.get("align").and_then(Value::as_str) == Some("end") {
                el = el.flex().justify_end();
            }
            if head {
                el = el.font_weight(FontWeight(weight::SEMIBOLD));
            }
            el.child(styled_runs(&runs, color, false, theme))
        };
        let header = div()
            .flex()
            .gap(px(space::M))
            .children(cols.iter().map(|col| {
                let head = col
                    .get("head")
                    .map(|h| tsp::spans_of(h, theme))
                    .unwrap_or_default();
                cell(col, head, theme.text_muted, true)
            }));
        let body = rows.iter().map(|row| {
            div()
                .flex()
                .gap(px(space::M))
                .children(cols.iter().map(|col| {
                    let id = col.get("id").and_then(Value::as_str).unwrap_or("");
                    let runs = (row.get("cells").and_then(|cells| cells.get(id)))
                        .map(|v| tsp::spans_of(v, theme))
                        .unwrap_or_default();
                    cell(col, runs, theme.text, false)
                }))
        });
        let el = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(header)
            .children(body);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_list(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let kids = self.tsp_children(p, node, cx);
        self.tsp_list_el(p, node, kids, cx)
    }

    #[inline(never)]
    fn tsp_list_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        kids: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let empty = (kids.is_empty())
            .then(|| node.props.get("empty").map(|e| tsp::spans_of(e, theme)))
            .flatten();
        let items = div()
            .flex()
            .min_w_0()
            .flex_col()
            .gap(px(space::XS))
            .children(kids)
            .children(empty.map(|runs| {
                div()
                    .p(px(space::S))
                    .child(styled_runs(&runs, theme.text_muted, false, theme))
            }));
        // A growing list takes the height left in its card and scrolls inside; its
        // selected item is kept in view.
        let el = if tsp::grows(node) {
            let handle = list_handle(&p.ui.lists, &node.id);
            let selected = str_prop(node, "selected");
            if let Some(ix) = tsp::selected_index(p.doc, node)
                && let Some(state) = p.ui.lists.borrow_mut().get_mut(&node.id)
                && state.selected.as_deref() != selected
            {
                state.selected = selected.map(str::to_owned);
                handle.scroll_to_item(ix);
            }
            let scroller = items
                .id(SharedString::from(format!(
                    "tsp-list-{}-{}",
                    p.session, node.id
                )))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&handle);
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(scroller)
        } else {
            items
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_item(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        let selected = (node.parent.as_deref())
            .and_then(|id| p.doc.get(id))
            .and_then(|list| str_prop(list, "selected"))
            == Some(node.id.as_str());
        let spans = |key: &str| {
            node.props
                .get(key)
                .map(|v| tsp::spans_of(v, theme))
                .unwrap_or_default()
        };
        let icon = str_prop(node, "icon")
            .and_then(|name| self.tsp_icon(p, name, tone.unwrap_or(theme.text_muted)));
        let detail = spans("detail");
        let value = spans("value");
        // The label shortens, never wraps; the value keeps its width at the right.
        let mut el = div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(theme::radius::SMALL))
            .children(icon)
            .child(div().flex_1().min_w_0().truncate().child(styled_runs(
                &spans("label"),
                theme.text,
                false,
                theme,
            )))
            .when(!detail.is_empty(), |el| {
                el.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text_muted))
                        .child(styled_runs(&detail, theme.text_muted, false, theme)),
                )
            })
            .when(!value.is_empty(), |el| {
                el.child(
                    div()
                        .min_w_0()
                        .max_w(relative(0.5))
                        .truncate()
                        .child(styled_runs(&value, theme.text_muted, false, theme)),
                )
            });
        // The picked row reads on any card: the selection fill and the accent bar (the
        // bar's width is reserved on the other rows, so nothing shifts).
        el = el
            .border_l(px(theme::TSP_ASK_BAR_WIDTH))
            .border_color(if selected {
                rgb(theme.accent).into()
            } else {
                gpui::transparent_black()
            })
            .when(selected, |el| el.bg(rgb(theme.selected)));
        if bool_prop(node, "disabled") {
            el = el.opacity(theme::opacity::SECONDARY);
        }
        self.tsp_decorate(el, p, node, Look::Row, selected, cx)
    }
    #[inline(never)]
    fn tsp_tabs(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let active = str_prop(node, "active").unwrap_or("");
        let items = array_prop(node, "items");
        let tabs = items.iter().filter_map(|tab| {
            let id = tab.get("id")?.as_str()?.to_owned();
            let label = tab
                .get("label")
                .map(|l| tsp::spans_of(l, theme))
                .unwrap_or_default();
            let (session, list) = (p.session, node.id.clone());
            let picked = id == active;
            let tab_id = id.clone();
            Some(
                div()
                    .id(SharedString::from(format!(
                        "tsp-{}-{}-{id}",
                        p.session, node.id
                    )))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(theme::radius::SMALL))
                    .pressable_unless(picked, Look::Chip, theme)
                    .on_press(
                        true,
                        cx.listener(move |shell, _, _, cx| {
                            if !tsp::root_owned(&list) {
                                shell.tsp_emit(
                                    session,
                                    tsp::select_event(&list, &tab_id, false),
                                    cx,
                                );
                            }
                        }),
                    )
                    .child(styled_runs(&label, theme.text, false, theme)),
            )
        });
        div()
            .flex()
            .gap(px(space::XS))
            .children(tabs)
            .into_any_element()
    }
    #[inline(never)]
    fn tsp_badge(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        let runs = tsp::node_spans(&node.props, theme);
        let el = div()
            .px(px(space::S))
            .rounded(px(theme::radius::TAG))
            .bg(rgb(theme.hover))
            .text_size(font_size::META)
            .child(styled_runs(
                &runs,
                tone.unwrap_or(theme.text_muted),
                false,
                theme,
            ));
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_kbd(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let keys = (node.props.get("keys").and_then(Value::as_array))
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let el = div()
            .flex()
            .gap(px(space::XS))
            .children(keys.into_iter().map(|key| {
                div()
                    .px(px(space::XS))
                    .rounded(px(theme::radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .font_family(theme.terminal_font_family.clone())
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(key)
            }));
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    #[inline(never)]
    fn tsp_icon_node(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        let icon = str_prop(node, "name")
            .and_then(|name| self.tsp_icon(p, name, tone.unwrap_or(theme.text_muted)));
        self.tsp_decorate(div().children(icon), p, node, Look::Icon, false, cx)
    }
    #[inline(never)]
    fn tsp_status(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        // Segments with `side: "right"` sit at the far end; low priority ones are not
        // dropped on overflow in P1 (the row clips).
        let (right, left): (Vec<_>, Vec<_>) = p
            .doc
            .children(&node.id)
            .partition(|seg| str_prop(seg, "side") == Some("right"));
        let draw = |segs: Vec<&Node>, cx: &mut Context<'_, Self>| -> Vec<AnyElement> {
            segs.into_iter()
                .map(|seg| self.tsp_node(p, seg, cx))
                .collect()
        };
        let (left, right) = (draw(left, cx), draw(right, cx));
        self.tsp_status_el(p, node, left, right, cx)
    }

    #[inline(never)]
    fn tsp_status_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        left: Vec<AnyElement>,
        right: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let mut el = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .overflow_hidden()
            .children(left);
        if !right.is_empty() {
            el = el.child(div().flex_1()).children(right);
        }
        if tsp::grows(node) {
            el = el.flex_1();
        }
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
    /// seg
    #[inline(never)]
    fn tsp_seg(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        let runs = tsp::node_spans(&node.props, theme);
        let color = tone.unwrap_or(theme.text_muted);
        let icon = str_prop(node, "icon").and_then(|name| self.tsp_icon(p, name, color));
        let el = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .text_size(font_size::META)
            .children(icon)
            .child(styled_runs(&runs, color, false, theme));
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
}

impl Shell {
    /// A track and its fill; `None` is an unknown amount (the loading mark).
    fn tsp_bar(p: &Paint<'_>, value: Option<f64>, color: u32) -> AnyElement {
        let theme = p.theme;
        let Some(value) = value else {
            return orbit_mark(p.now, color).into_any_element();
        };
        div()
            .w(px(theme::TSP_KEY_WIDTH))
            .flex_none()
            .h(px(space::XS))
            .rounded_full()
            .bg(rgb(theme.hover))
            .child(
                div()
                    .h_full()
                    .w(relative(value.clamp(0.0, 1.0) as f32))
                    .rounded_full()
                    .bg(rgb(color)),
            )
            .into_any_element()
    }

    /// spinner, elapsed, progress, rate, meter, effort. What moves (a spinner, a running elapsed,
    /// a bar without an amount) is redrawn on the status tick (ADR 0086); a `rate` is static.
    fn tsp_live(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let el = match node.kind.as_str() {
            "spinner" => Self::tsp_spinner(p, node),
            "elapsed" => Self::tsp_elapsed(p, node),
            "progress" => Self::tsp_progress(p, node),
            "rate" => Self::tsp_rate(p, node),
            "meter" if str_prop(node, "style") == Some("ring") => Self::tsp_ring(p, node),
            "meter" => Self::tsp_meter(p, node),
            _ => Self::tsp_effort(p, node),
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }

    /// The tone of a live node, when it has one.
    fn live_tone(p: &Paint<'_>, node: &Node) -> Option<u32> {
        str_prop(node, "tone").map(|tone| tsp::tone_color(tone, p.theme))
    }

    /// A live node's `label` prop as spans.
    fn live_label(p: &Paint<'_>, node: &Node) -> Vec<(String, SpanStyle)> {
        (node.props.get("label"))
            .map(|v| tsp::spans_of(v, p.theme))
            .unwrap_or_default()
    }

    #[inline(never)]
    fn tsp_spinner(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        let tone = Self::live_tone(p, node).unwrap_or(theme.status_working);
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(if str_prop(node, "style") == Some("dots") {
                loading_dots(p.now, rgb(tone)).into_any_element()
            } else {
                orbit_mark(p.now, tone).into_any_element()
            })
            .child(styled_runs(
                &Self::live_label(p, node),
                theme.text_muted,
                false,
                theme,
            ))
    }

    #[inline(never)]
    fn tsp_elapsed(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        let age = node.props.get("age").and_then(Value::as_u64).unwrap_or(0);
        let stopped = node.props.get("stopped").and_then(Value::as_u64);
        let since = {
            let mut seen = p.ui.elapsed_seen.borrow_mut();
            let entry = seen.entry(node.id.clone()).or_insert((age, p.now));
            if entry.0 != age {
                *entry = (age, p.now);
            }
            u64::try_from(p.now - entry.1).unwrap_or(0)
        };
        let text = tsp::elapsed_label(tsp::elapsed_ms(age, stopped, since));
        div()
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .child(text)
    }

    #[inline(never)]
    fn tsp_progress(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(Self::tsp_bar(
                p,
                num_prop(node, "value"),
                Self::live_tone(p, node).unwrap_or(theme.accent),
            ))
            .child(styled_runs(
                &Self::live_label(p, node),
                theme.text_muted,
                false,
                theme,
            ))
    }

    #[inline(never)]
    fn tsp_rate(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        let text = tsp::rate_label(
            num_prop(node, "value").unwrap_or(0.0),
            str_prop(node, "unit").unwrap_or(""),
        );
        div()
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .child(text)
    }

    /// A timeout ring (the ask card): a ring with the seconds left. The arc is not drawn.
    #[inline(never)]
    fn tsp_ring(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        div()
            .size(px(theme::TSP_RING_SIZE))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.accent))
            .text_size(font_size::META)
            .child(styled_runs(
                &Self::live_label(p, node),
                theme.text_muted,
                false,
                theme,
            ))
    }

    #[inline(never)]
    fn tsp_meter(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        // bar, blocks and segmented all draw as a bar in P1.
        let value = num_prop(node, "value");
        let color = tsp::tone_color(
            tsp::meter_tone(value.unwrap_or(0.0), node.props.get("thresholds")),
            theme,
        );
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(Self::tsp_bar(p, value, color))
            .child(div().text_size(font_size::META).child(styled_runs(
                &Self::live_label(p, node),
                theme.text_muted,
                false,
                theme,
            )))
    }

    #[inline(never)]
    fn tsp_effort(p: &Paint<'_>, node: &Node) -> Div {
        let theme = p.theme;
        let filled = tsp::effort_steps(str_prop(node, "level").unwrap_or(""));
        div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .children((0..4).map(move |i| {
                div()
                    .w(px(space::XS))
                    .h(px(space::M))
                    .rounded(px(theme::radius::SMALL))
                    .bg(rgb(if i < filled {
                        theme.accent
                    } else {
                        theme.divider
                    }))
            }))
    }
}

/// How [`Shell::tsp_editor`] draws an `editor`.
#[derive(Clone, Copy)]
enum EditorLook<'a> {
    /// Anywhere in the surface: the placeholder in `text_dim`, the ghost under the text.
    Plain,
    /// The dock's field (spec 7.8): the Chat placeholder row, the ghost (already checked by
    /// `DockModel`: caret at the end) right after the caret on the same line in `text_subtle`.
    Composer { ghost: Option<&'a str> },
}

impl Shell {
    /// editor, input, image.
    fn tsp_input(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        if node.kind == "image" {
            return self.tsp_image(p, node, cx);
        }
        self.tsp_editor(p, node, EditorLook::Plain, cx)
    }

    /// An `editor` or `input`: its text as one wrapping run of text (the `prompt` lead, the
    /// decorations, the selection, the caret, the ghost), or the placeholder. The composer field
    /// Mkrew edits itself (ADR 0113) shows that field's text, caret and selection; omp's
    /// decorations only while omp has the same text.
    fn tsp_editor(
        &self,
        p: &Paint<'_>,
        node: &Node,
        look: EditorLook<'_>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let session = p.session;
        let omp_text = str_prop(node, "text").unwrap_or("");
        let local = (matches!(look, EditorLook::Composer { .. }) && !self.is_exited(session))
            .then_some(p.ui.editor.as_ref())
            .flatten()
            .filter(|field| field.node == node.id);
        // omp says which editable node has the focus (`focus` op); until it does, the pane's
        // only editor is the one.
        let has_focus =
            p.focused && (local.is_some() || p.doc.focus().is_none_or(|id| id == node.id));
        let (text, cursor, selection) = match local {
            Some(field) => (
                field.input.as_str(),
                field.input.cursor(),
                field.input.selection(),
            ),
            None => {
                let cursor = (node.props.get("cursor").and_then(Value::as_u64))
                    .map_or(omp_text.len(), |units| {
                        tsp::byte_at_utf16(omp_text, units as usize)
                    });
                (omp_text, cursor, None)
            }
        };
        let lead = (node.props.get("prompt"))
            .map(|v| tsp::spans_of(v, theme))
            .unwrap_or_default();
        let lead_text: String = lead.iter().map(|(t, _)| t.as_str()).collect();
        let (shown, layout, map) = if text.is_empty() {
            let placeholder = (node.props.get("placeholder"))
                .map(|v| tsp::spans_of(v, theme))
                .unwrap_or_default();
            let placeholder = match look {
                EditorLook::Plain => {
                    div().child(styled_runs(&placeholder, theme.text_dim, false, theme))
                }
                EditorLook::Composer { .. } => {
                    placeholder_row(styled_runs(&placeholder, theme.subtle, false, theme), theme)
                }
            };
            let empty = div()
                .flex()
                .items_center()
                .when(!lead.is_empty(), |row| {
                    row.child(styled_runs(&lead, theme.text_muted, false, theme))
                })
                .child(mkrew_ui::field::empty_field(placeholder, has_focus, theme));
            (empty.into_any_element(), None, tsp::ShownMap::default())
        } else {
            // omp's decorations (skills, mentions, hidden markers, typos) are for omp's text.
            let decor = if text == omp_text {
                array_prop(node, "decor")
            } else {
                &[]
            };
            let pieces = tsp::decor_pieces(text, decor, theme);
            let view = tsp::EditorView::new(&lead_text, text, &pieces);
            let map = view.map().clone();
            let (mut drawn, mut runs) = (String::new(), Vec::new());
            push_runs(
                &mut drawn,
                &mut runs,
                &lead,
                theme.text_muted,
                false,
                weight::REGULAR,
                false,
                theme,
            );
            for (range, decor) in &view.pieces {
                let part = [(view.shown[range.clone()].to_owned(), decor.style)];
                push_runs(
                    &mut drawn,
                    &mut runs,
                    &part,
                    theme.text,
                    false,
                    weight::REGULAR,
                    decor.typo,
                    theme,
                );
            }
            if let Some(selection) = &selection {
                let range = map.to_shown(selection.start)..map.to_shown(selection.end);
                runs = mkrew_ui::field::highlight_runs(
                    runs,
                    range,
                    mkrew_ui::field::selection_bg(theme),
                );
            }
            // The composer's ghost follows the caret at the end, once omp has the same text.
            let ghost = match look {
                EditorLook::Composer { ghost } => {
                    ghost.filter(|_| text == omp_text && cursor == text.len())
                }
                EditorLook::Plain => None,
            };
            if let Some(ghost) = ghost {
                let part = [(ghost.to_owned(), SpanStyle::default())];
                push_runs(
                    &mut drawn,
                    &mut runs,
                    &part,
                    theme.text_subtle,
                    false,
                    weight::REGULAR,
                    false,
                    theme,
                );
            }
            let styled = StyledText::new(drawn).with_runs(runs);
            let caret = has_focus && selection.is_none();
            let (shown, layout) =
                mkrew_ui::field::caret_over(styled, map.to_shown(cursor), caret, theme);
            (shown, Some(layout), map)
        };
        // Plain editors: omp's suggestion under the text.
        let under = match look {
            EditorLook::Plain => {
                str_prop(node, "ghost").filter(|g| !g.is_empty() && !text.is_empty())
            }
            EditorLook::Composer { .. } => None,
        };
        let id = node.id.clone();
        // A text field: exempt from the press look, so a plain mouse-down sends the focus.
        let el = div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(shown)
            .children(under.map(|g| {
                styled_runs(
                    &[(g.to_owned(), SpanStyle::default())],
                    theme.text_dim,
                    false,
                    theme,
                )
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, _, cx| {
                    shell.tsp_click(session, &id, false, cx);
                }),
            );
        let el = if local.is_some() {
            mkrew_ui::field::caret_mouse(
                el,
                &cx.entity(),
                layout,
                move |shell: &mut Shell, caret| {
                    let at = map.to_text(caret.at);
                    shell.tsp_caret(session, mkrew_ui::field::Caret { at, ..caret });
                },
            )
        } else {
            el
        };
        let el = if tsp::grows(node) || matches!(look, EditorLook::Composer { .. }) {
            el.flex_1()
        } else {
            el
        };
        el.into_any_element()
    }

    /// An inline image from the blob bytes the daemon sent; nothing for a built-in or missing one.
    fn tsp_image(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        // The images of a sent prompt (`row role=omp.user.images`) are thumbnails, as in the
        // composer; their click still opens the file.
        let in_prompt = (node.parent.as_deref())
            .and_then(|id| p.doc.get(id))
            .and_then(tsp::role_of)
            == Some("omp.user.images");
        let el = match (self.tsp_image_data(p, node), str_prop(node, "alt")) {
            (Some(image), _) if in_prompt => div().child(thumbnail(image.data, theme)),
            // As wide as the image, at most the column (scaled down whole, never cropped).
            (Some(image), _) => match image.size {
                Some(natural) => {
                    let (w, h) = tsp::image_size(natural, p.column);
                    div().child(gpui::img(image.data).w(px(w)).h(px(h)).max_w_full())
                }
                None => div().max_w_full().child(gpui::img(image.data).max_w_full()),
            },
            (None, Some(alt)) if str_prop(node, "builtin").is_none() => div()
                .text_color(rgb(theme.text_muted))
                .child(alt.to_owned()),
            _ => div(),
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }

    /// The decoded image of an `image` node's blob, cached per blob.
    fn tsp_image_data(&self, p: &Paint<'_>, node: &Node) -> Option<TspImage> {
        let blob = str_prop(node, "blob").and_then(|id| p.mirror.blob(id))?;
        let mut cache = p.ui.images.borrow_mut();
        if let Some(image) = cache.get(&blob.id) {
            return Some(image.clone());
        }
        let size = image::ImageReader::new(std::io::Cursor::new(&blob.data[..]))
            .with_guessed_format()
            .ok()
            .and_then(|reader| reader.into_dimensions().ok());
        let image = TspImage {
            data: Arc::new(gpui::Image::from_bytes(
                image_format(&blob.mime)?,
                blob.data.clone(),
            )),
            size,
        };
        cache.insert(blob.id.clone(), image.clone());
        Some(image)
    }

    /// Any kind the pane does not draw (`agent chart diff math prefs`, unknown ones): its first
    /// text-like prop as a muted line, then its children (spec 7.1).
    fn tsp_generic(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let line = tsp::generic_line(&node.props, theme);
        let el = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .min_w_0()
            .children(line.map(|line| div().text_color(rgb(theme.text_muted)).child(line)))
            .children(self.tsp_children(p, node, cx));
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
}

/// An `image` blob decoded for drawing, and its natural size in px (`None`: an unread header).
#[derive(Clone)]
struct TspImage {
    data: Arc<gpui::Image>,
    size: Option<(u32, u32)>,
}

/// A pasted image as a small square tile (the composer's attachments, a sent prompt's images):
/// cropped to fill, rounded, with a faint border.
fn thumbnail(image: Arc<gpui::Image>, theme: &Theme) -> Div {
    div()
        .flex_none()
        .size(px(theme::TSP_THUMB_SIZE))
        .rounded(px(theme::radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .overflow_hidden()
        .child(
            gpui::img(image)
                .size_full()
                .rounded(px(theme::radius::MEDIUM))
                .object_fit(ObjectFit::Cover),
        )
}

/// Whether `node`'s parent satisfies `want` (a role test such as `tsp::is_thinking`).
fn parent_is(p: &Paint<'_>, node: &Node, want: fn(&Node) -> bool) -> bool {
    node.parent
        .as_deref()
        .and_then(|id| p.doc.get(id))
        .is_some_and(want)
}

/// An `ansi` or `rows` body of a tool row (command output, a diff): `code_bg` with a `layer[0]`
/// ring, `radius::MEDIUM`, 8 by 12 padding. A `code` node is a code card already.
fn tool_well(body: AnyElement, theme: &Theme) -> AnyElement {
    div()
        .min_w_0()
        .overflow_hidden()
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(theme::radius::MEDIUM))
        .bg(rgb(theme.code_bg))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[0]))
        .child(body)
        .into_any_element()
}

/// The head of an `omp.thinking*` section, or of a run of them: BODY italic `text_subtle`
/// (`text` on hover), 24 px at least, with the chevron after the label. `collapsed`: the
/// chevron's state, or `None` without one. The caller adds the press.
fn thinking_head_el(
    theme: &Theme,
    key: String,
    label: String,
    collapsed: Option<bool>,
) -> gpui::Stateful<Div> {
    let group = SharedString::from(key.clone());
    let chevron = collapsed.map(|collapsed| {
        let icon = if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        icon_svg(icon, theme::GROUP_CHEVRON_SIZE, theme.text_subtle)
            .group_hover(group.clone(), |style| style.text_color(rgb(theme.text)))
    });
    div()
        .id(SharedString::from(key))
        .group(group.clone())
        .self_start()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .min_h(theme::rems_px(theme::TSP_HEAD_HEIGHT))
        .text_size(font_size::BODY)
        .italic()
        .child(
            div()
                .text_color(rgb(theme.text_subtle))
                .group_hover(group, |style| style.text_color(rgb(theme.text)))
                .child(label),
        )
        .children(chevron)
        .pressable(Look::Text, true, theme)
}

/// The mark of a todo item or phase.
fn todo_mark(mark: tsp::ItemMark, color: u32) -> AnyElement {
    // Every mark has the icon box: a ring for the open states (a dot inside for the active one),
    // a check for a done one.
    let ring = || {
        div()
            .size(theme::rems_px(theme::ICON_SIZE_SMALL))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(color))
    };
    match mark {
        tsp::ItemMark::Active => ring().child(status_dot(color, false)).into_any_element(),
        tsp::ItemMark::Ring => ring().into_any_element(),
        tsp::ItemMark::Check => icon_svg(Icon::Check, theme::ICON_SIZE_SMALL, color)
            .flex_none()
            .into_any_element(),
    }
}

impl Shell {
    /// `checklist`: the hud chip with its popover, the `full` card, the `reminder` notice.
    fn tsp_checklist(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let list = tsp::Checklist::from_props(&node.props);
        match list.mode {
            tsp::ChecklistMode::Hud => self.tsp_todo_hud(p, node, &list, cx),
            tsp::ChecklistMode::Full => self.tsp_checklist_full(p, node, &list, cx),
            tsp::ChecklistMode::Reminder => self.tsp_checklist_reminder(p, node, &list, cx),
        }
    }

    #[inline(never)]
    fn tsp_checklist_full(
        &self,
        p: &Paint<'_>,
        node: &Node,
        list: &tsp::Checklist,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        div()
            .p(px(space::M))
            .rounded(px(theme::radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .child(self.tsp_phases(p, node, list, cx))
            .into_any_element()
    }

    #[inline(never)]
    fn tsp_checklist_reminder(
        &self,
        p: &Paint<'_>,
        node: &Node,
        list: &tsp::Checklist,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .children(
                list.note
                    .as_ref()
                    .map(|note| div().text_color(rgb(theme.text_muted)).child(note.clone())),
            )
            .child(self.tsp_phases(p, node, list, cx))
            .into_any_element()
    }

    /// The phases and their items (the `full` card, the `reminder` and the popover).
    fn tsp_phases(
        &self,
        p: &Paint<'_>,
        node: &Node,
        list: &tsp::Checklist,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let phases = list.phases.iter().enumerate().map(|(at, phase)| {
            let toggled =
                p.ui.todo_toggled
                    .contains(&(node.id.clone(), phase.id.clone()));
            let collapsed = tsp::collapsed_now(phase.default_collapsed(), toggled);
            let header = (!phase.headerless()).then(|| {
                let mark = match phase.mark() {
                    tsp::PhaseMark::Working => todo_mark(tsp::ItemMark::Active, theme.accent),
                    tsp::PhaseMark::Done => status_dot(theme.status_done, false).into_any_element(),
                    tsp::PhaseMark::Open => status_dot(theme.text_muted, true).into_any_element(),
                };
                let (done, count) = phase.counts();
                let title = format!("{}. {}", tsp::roman(at + 1), phase.title);
                let (session, node_id, phase_id) = (p.session, node.id.clone(), phase.id.clone());
                div()
                    .id(SharedString::from(format!(
                        "tsp-phase-{}-{}-{}",
                        p.session, node.id, phase.id
                    )))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(theme::radius::SMALL))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(if phase.mark() == tsp::PhaseMark::Done {
                                theme.text_muted
                            } else {
                                theme.text
                            }))
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme.terminal_font_family.clone())
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{done}/{count}")),
                    )
                    .pressable(Look::Row, true, theme)
                    .on_press(
                        true,
                        cx.listener(move |shell, _, _, cx| {
                            shell.tsp_todo_phase(session, &node_id, &phase_id, cx);
                        }),
                    )
            });
            let items: Vec<AnyElement> = if collapsed {
                Vec::new()
            } else {
                phase
                    .items
                    .iter()
                    .map(|item| Self::tsp_todo_item(p, item))
                    .collect()
            };
            div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .children(header)
                .children(items)
        });
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .children(phases)
            .into_any_element()
    }

    /// One item: its mark, its text (struck, colored or suffixed per status) and its note.
    fn tsp_todo_item(p: &Paint<'_>, item: &tsp::TodoItem) -> AnyElement {
        let theme = p.theme;
        let look = tsp::item_look(item.status, theme);
        let runs = vec![(
            format!("{}{}", item.text, look.suffix),
            SpanStyle {
                strike: look.strike,
                ..SpanStyle::default()
            },
        )];
        // The mark is centered on the first line of the text.
        let line = theme::rems_px(space::L + space::XS);
        div()
            .flex()
            .items_start()
            .gap(px(space::S))
            .px(px(space::S))
            .child(
                div()
                    .h(line)
                    .flex()
                    .items_center()
                    .child(todo_mark(look.mark, look.mark_color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().line_height(line).child(styled_runs(
                        &runs,
                        look.text_color,
                        false,
                        theme,
                    )))
                    .children(item.note.as_ref().map(|note| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(note.clone())
                    })),
            )
            .into_any_element()
    }

    /// The hud: one pressable line (mark, the item in progress, `done/total`) whose click opens
    /// the list in a glass panel above it. Esc or a click outside closes it.
    fn tsp_todo_hud(
        &self,
        p: &Paint<'_>,
        node: &Node,
        list: &tsp::Checklist,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let Some(line) = list.hud_line() else {
            return div().into_any_element();
        };
        let open = p.ui.todo_open.as_deref() == Some(node.id.as_str());
        let mark = match line.mark {
            tsp::HudMark::Active => todo_mark(tsp::ItemMark::Active, theme.accent),
            tsp::HudMark::Ring => status_dot(theme.text_muted, true).into_any_element(),
            tsp::HudMark::Done => status_dot(theme.status_done, false).into_any_element(),
        };
        let (session, id) = (p.session, node.id.clone());
        let chip = div()
            .id(SharedString::from(format!(
                "tsp-todo-{}-{}",
                p.session, node.id
            )))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(space::S))
            .px(px(space::S))
            .h(theme::rems_px(theme::PILL_HEIGHT))
            .rounded(px(theme::radius::PILL))
            .text_size(font_size::META)
            .child(div().flex_none().child(mark))
            .child(div().min_w_0().truncate().child(line.text.clone()))
            .child(
                div()
                    .flex_none()
                    .font_family(theme.terminal_font_family.clone())
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{}/{}", line.done, line.total)),
            )
            .pressable_unless(open, Look::Chip, theme)
            .when(p.tips(), |el| el.tooltip(tooltip(theme, "Todo list", "")))
            // The press outside the panel skips this chip (below), so the open state read here is
            // live: a press on the chip of an open panel closes it.
            .on_press(
                true,
                cx.listener(move |shell, _, _, cx| {
                    shell.tsp_todo_toggle(session, &id, cx);
                }),
            );
        // Where the chip is painted, for the outside press handler of the panel.
        let chip_at = p.ui.todo_chip.clone();
        let chip_bounds = canvas(
            {
                let chip_at = chip_at.clone();
                move |bounds, _, _| chip_at.set(bounds)
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .flex()
            .min_w_0()
            .max_w(relative(0.5))
            .child(chip)
            .child(chip_bounds)
            .when(open, |el| {
                let height = f32::from(p.ui.rows.viewport_bounds().size.height)
                    * theme::TSP_POPOVER_MAX_FRACTION;
                let panel = floating_panel(
                    SharedString::from(format!("tsp-todo-panel-{}", p.session)),
                    theme,
                )
                // An opaque fill: a popover with text must be readable over the page.
                .bg(rgb(theme.surface_raised))
                .shadow(theme::shadow_pop(theme))
                .w(px(theme::TSP_POPOVER_WIDTH))
                .max_h(px(height.max(theme::TSP_POPOVER_MIN_HEIGHT)))
                .p(px(space::M))
                .overflow_y_scroll()
                .on_mouse_down_out(cx.listener(move |shell, event: &MouseDownEvent, _, cx| {
                    if !chip_at.get().contains(&event.position) {
                        shell.tsp_todo_close(session, cx);
                    }
                }))
                .child(self.tsp_phases(p, node, list, cx));
                el.child(deferred(
                    anchored()
                        .anchor(Anchor::BottomLeft)
                        .snap_to_window_with_margin(px(space::S))
                        .child(panel),
                ))
            })
            .into_any_element()
    }

    /// `col{role: "omp.hud.status"}` holding a todo hud: the status text on the left, the hud on
    /// the right.
    fn tsp_status_hud(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let (hud, rest): (Vec<&Node>, Vec<&Node>) = p
            .doc
            .children(&node.id)
            .partition(|child| child.kind == "checklist");
        let left: Vec<AnyElement> = rest
            .into_iter()
            .map(|child| self.tsp_node(p, child, cx))
            .collect();
        let right: Vec<AnyElement> = hud
            .into_iter()
            .map(|child| self.tsp_node(p, child, cx))
            .collect();
        Self::tsp_status_hud_el(left, right)
    }

    #[inline(never)]
    fn tsp_status_hud_el(left: Vec<AnyElement>, right: Vec<AnyElement>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .min_w_0()
            .child(div().flex_1().min_w_0().flex().flex_col().children(left))
            .children(right)
            .into_any_element()
    }
}

impl Shell {
    /// The dock of an omp pane that has the composer roles (spec 7.8): the dock's own children in
    /// order, except that the node where the editor stands becomes the status row and the card
    /// (with the meter on it), and the nodes the status row took over are skipped. `column` is the
    /// text column in px (page padding off).
    fn tsp_dock(
        &self,
        p: &Paint<'_>,
        m: &tsp::DockModel,
        column: f32,
        cx: &mut Context<'_, Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        for node in p.doc.children("dock") {
            self.tsp_dock_node(p, m, node, column, &mut out, cx);
        }
        out
    }

    fn tsp_dock_node(
        &self,
        p: &Paint<'_>,
        m: &tsp::DockModel,
        node: &Node,
        column: f32,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) {
        if m.skip.contains(&node.id) {
            return;
        }
        if node.id == m.editor {
            out.push(self.tsp_composer(p, m, column, cx));
        } else if m.path.contains(&node.id) {
            for child in p.doc.children(&node.id) {
                self.tsp_dock_node(p, m, child, column, out, cx);
            }
        } else {
            out.push(self.tsp_node(p, node, cx));
        }
    }

    /// The dock of an omp pane whose ask card or note editor stands where the composer would
    /// (spec 7.6, 7.8): the status row first, then the dock's other nodes, the card among them.
    fn tsp_dock_card(
        &self,
        p: &Paint<'_>,
        d: &tsp::StatusDock,
        cx: &mut Context<'_, Self>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = (self.tsp_dock_status(p, &d.status, cx))
            .into_iter()
            .collect();
        for node in p.doc.children("dock") {
            self.tsp_dock_rest(p, d, node, &mut out, cx);
        }
        out
    }

    fn tsp_dock_rest(
        &self,
        p: &Paint<'_>,
        d: &tsp::StatusDock,
        node: &Node,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) {
        if d.skip.contains(&node.id) {
            return;
        }
        if d.path.contains(&node.id) {
            for child in p.doc.children(&node.id) {
                self.tsp_dock_rest(p, d, child, out, cx);
            }
        } else {
            out.push(self.tsp_node(p, node, cx));
        }
    }

    /// `node` as a chip: its `title` tooltip and, when it has an action, the press that sends the
    /// same `action` event as before (`tsp_decorate`). A node the document no longer has is drawn
    /// as it is.
    fn tsp_chip(
        &self,
        p: &Paint<'_>,
        id: &str,
        look: Look,
        el: Div,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        match p.doc.get(id) {
            Some(node) => self.tsp_decorate(el, p, node, look, false, cx),
            None => el.into_any_element(),
        }
    }

    /// The status row (when it has anything) and the card, with the context meter on its top
    /// edge: the Chat composer with omp's content. Compact (`Paint::compact`): the card with the
    /// field only.
    fn tsp_composer(
        &self,
        p: &Paint<'_>,
        m: &tsp::DockModel,
        column: f32,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let doc = p.doc;
        let line: Vec<AnyElement> = doc
            .children(&m.line)
            .map(|node| {
                if node.id == m.field.id {
                    let look = EditorLook::Composer {
                        ghost: m.field.ghost.as_deref(),
                    };
                    self.tsp_editor(p, node, look, cx)
                } else {
                    self.tsp_node(p, node, cx)
                }
            })
            .collect();
        let field = div()
            .id(SharedString::from(format!("tsp-field-{}", p.session)))
            .flex()
            .items_start()
            .gap(px(space::S))
            .cursor(CursorStyle::IBeam)
            .max_h(px(theme::COMPOSER_MAX_HEIGHT))
            .overflow_y_scroll()
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .children(line);
        if p.compact {
            let card = composer_card_frame(theme).child(field);
            return Self::composer_drops(card, p.session, theme::radius::PANEL, theme, cx)
                .into_any_element();
        }
        let status = self.tsp_dock_status(p, &m.status, cx);
        let meter = m.context.as_ref().map(|c| {
            let marks = c.marks.iter().map(|k| (k.at, k.title.clone())).collect();
            // Neutral fill; omp's warning and error tones past its thresholds keep theirs.
            let tone = (c.tone != "accent").then(|| tsp::tone_color(c.tone, theme));
            let bar = meter_bar(
                c.fraction.unwrap_or(0.0),
                tone,
                c.label.clone(),
                c.total.clone(),
                marks,
                theme,
            );
            self.tsp_chip(p, &c.id, Look::Text, bar, cx)
        });
        let focus = m.focus.as_ref().map(|f| self.tsp_dock_focus(p, f, cx));
        let chips: Vec<AnyElement> = (m.chips.iter())
            .filter_map(|id| doc.get(id))
            .map(|node| self.tsp_attachment(p, node, cx))
            .collect();
        let rise = meter.is_some();
        let card = composer_card_frame(theme)
            .children(meter)
            .when(rise, |el| {
                el.mt(theme::rems_px(theme::CONTEXT_METER_RISE - space::S))
            })
            .children(focus)
            .when(!chips.is_empty(), |el| {
                el.child(
                    (div().flex().flex_wrap().items_center())
                        .gap(px(space::S))
                        .children(chips),
                )
            })
            .child(field)
            .child(self.tsp_dock_bar(p, m, column, cx));
        // A Files row dropped on the card types its `@path` into the field (ADR 0081).
        let card = Self::composer_drops(card, p.session, theme::radius::PANEL, theme, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .children(status)
            .child(card)
            .into_any_element()
    }

    /// An attachment of the composer (`card role=omp.composer.chip`, head `#n`): a thumbnail of
    /// a pasted image, else a small accent chip like omp's own; its click shows what it holds
    /// (the image, the pasted text) in a panel above it. Esc or a click outside closes it.
    fn tsp_attachment(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let label: String = (node.props.get("head"))
            .map(|head| tsp::spans_of(head, theme))
            .unwrap_or_default()
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        let open = p.ui.chip_open.as_deref() == Some(node.id.as_str());
        let (session, id) = (p.session, node.id.clone());
        let key = SharedString::from(format!("tsp-attach-{}-{}", p.session, node.id));
        let image = (p.doc.children(&node.id))
            .find(|child| child.kind == "image")
            .and_then(|child| self.tsp_image_data(p, child))
            .map(|image| image.data);
        let filled = image.is_none();
        let chip = div().id(key.clone());
        let chip = match image {
            Some(image) => chip.group(key.clone()).child(
                thumbnail(image, theme)
                    .group_hover(key, |s| s.border_color(rgb(theme.text_subtle))),
            ),
            None => chip
                .flex()
                .items_center()
                .px(px(space::S))
                .h(theme::rems_px(theme::PILL_HEIGHT))
                .rounded(px(theme::radius::PILL))
                .bg(rgb(theme.accent_bg))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.accent_text))
                .child(if label.trim().is_empty() {
                    "#".to_owned()
                } else {
                    label
                }),
        };
        // The text chip has its own `accent_bg` fill: its hover must not erase it.
        let chip = (if filled && !open {
            chip.pressable_on(Look::Chip, theme.accent_bg, theme)
        } else {
            chip.pressable_unless(open, Look::Chip, theme)
        })
        .when(p.tips(), |el| {
            el.tooltip(tooltip(theme, "Preview the attachment", ""))
        })
        .on_press(
            true,
            cx.listener(move |shell, _, _, cx| {
                if let Some(ui) = shell.tsps.get_mut(&session) {
                    ui.chip_open = match ui.chip_open {
                        Some(_) => None,
                        None => Some(id.clone()),
                    };
                }
                cx.notify();
            }),
        );
        if !open {
            return chip.into_any_element();
        }
        // Where the open chip is painted: a press on it is not "outside" the panel.
        let chip_at = p.ui.chip_at.clone();
        let bounds = canvas(
            {
                let chip_at = chip_at.clone();
                move |bounds, _, _| chip_at.set(bounds)
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let panel = floating_panel(
            SharedString::from(format!("tsp-attach-panel-{session}")),
            theme,
        )
        .bg(rgb(theme.surface_raised))
        .shadow(theme::shadow_pop(theme))
        .max_w(px(theme::TSP_POPOVER_WIDTH))
        .p(px(space::M))
        .on_mouse_down_out(cx.listener(move |shell, event: &MouseDownEvent, _, cx| {
            if !chip_at.get().contains(&event.position)
                && let Some(ui) = shell.tsps.get_mut(&session)
            {
                ui.chip_open = None;
                cx.notify();
            }
        }))
        .children(self.tsp_children(p, node, cx));
        div()
            .relative()
            .child(chip)
            .child(bounds)
            .child(deferred(
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(space::S))
                    .child(panel),
            ))
            .into_any_element()
    }

    /// The chip row: omp's order, never wrapping, `min_w_0`; what does not fit is dropped by
    /// `fit_bar`; the send or stop button always ends it.
    fn tsp_dock_bar(
        &self,
        p: &Paint<'_>,
        m: &tsp::DockModel,
        column: f32,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let exited = self.is_exited(p.session);
        let fit = tsp::fit_bar(&m.bar, composer_room(column, cx));
        let mut cells: Vec<AnyElement> = Vec::new();
        for part in &m.bar.parts {
            match part {
                tsp::BarPart::Model => {
                    if let Some(model) = &m.bar.model {
                        cells.push(self.tsp_dock_model(
                            p,
                            model,
                            m.bar.effort.as_ref(),
                            fit.model_name,
                            exited,
                            cx,
                        ));
                    }
                }
                tsp::BarPart::Facts => {
                    let facts: Vec<AnyElement> = (m.bar.facts.iter().zip(&fit.facts))
                        .filter(|(_, shown)| **shown)
                        .map(|(fact, _)| self.tsp_dock_fact(p, fact, cx))
                        .collect();
                    cells.push(
                        div()
                            .flex()
                            .min_w_0()
                            .overflow_hidden()
                            .items_center()
                            .gap(px(space::XS))
                            .font_family(theme.terminal_font_family.clone())
                            .children(facts)
                            .into_any_element(),
                    );
                }
                tsp::BarPart::Usage => {
                    if let (true, Some(usage)) = (fit.usage, &m.bar.usage) {
                        let chip = quiet_chip(div(), theme)
                            .font_family(theme.terminal_font_family.clone())
                            .text_color(rgb(usage.color))
                            .child(usage.text.clone());
                        cells.push(self.tsp_chip(p, &usage.id, Look::Chip, chip, cx));
                    }
                }
            }
        }
        // The chips share what the send chip leaves; `fit_bar` decides what drops, and a low
        // estimate clips a chip (the model name truncates), never the button.
        div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(px(space::S))
                    .children(cells)
                    .child(div().flex_1()),
            )
            .child(self.tsp_dock_primary(p, m, exited, cx))
            .into_any_element()
    }

    /// The model pill, the name truncated to what `fit_bar` left it. omp's effort glyph leads
    /// the pill and the effort label joins its tooltip. A click is omp's `status.model`; a click
    /// on the glyph is the effort's own action.
    fn tsp_dock_model(
        &self,
        p: &Paint<'_>,
        model: &tsp::ModelPill,
        effort: Option<&tsp::Effort>,
        name_room: Option<f32>,
        exited: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let node = p.doc.get(&model.id);
        let click = !exited && node.is_some_and(|n| tsp::has_click(p.doc, n));
        let tip = [
            node.and_then(|n| str_prop(n, "title")).map(str::to_owned),
            effort
                .filter(|e| !e.label.is_empty())
                .map(|e| format!("Effort: {}", e.label)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let name = div()
            .min_w_0()
            .truncate()
            .when_some(name_room, |el, room| el.max_w(theme::rems_px(room)))
            .child(model.name.clone());
        let (session, id) = (p.session, model.id.clone());
        let lead = effort.and_then(|effort| {
            let glyph = self.tsp_node(p, effort.glyph.as_deref().and_then(|id| p.doc.get(id))?, cx);
            let press = !exited
                && p.doc
                    .get(&effort.id)
                    .is_some_and(|n| tsp::has_click(p.doc, n));
            if !press {
                return Some(glyph);
            }
            let effort_id = effort.id.clone();
            Some(
                div()
                    .id(SharedString::from(format!("tsp-effort-{session}")))
                    .flex()
                    .items_center()
                    .rounded(px(theme::radius::PILL))
                    .child(glyph)
                    .pressable(Look::Icon, true, theme)
                    .on_press(
                        true,
                        cx.listener(move |shell, event: &ClickEvent, _, cx| {
                            // The glyph is inside the pill: its click is not the pill's.
                            cx.stop_propagation();
                            shell.tsp_click(session, &effort_id, event.click_count() > 1, cx);
                        }),
                    )
                    .into_any_element(),
            )
        });
        model_pill(
            SharedString::from(format!("tsp-model-{session}")),
            lead,
            name,
            click,
            theme,
        )
        .when(p.tips() && !tip.is_empty(), |el| {
            el.tooltip(tooltip(theme, &tip, ""))
        })
        .on_press(
            click,
            cx.listener(move |shell, event: &ClickEvent, _, cx| {
                shell.tsp_click(session, &id, event.click_count() > 1, cx);
            }),
        )
        .into_any_element()
    }

    /// One fact: a quiet mono chip with its icon in `fact.color` (`text_subtle`); the `accent`
    /// one is the Chat permission chip; the colors omp put in the text's spans stay
    /// (`fact.runs`); an elapsed time is drawn by its own node (it ticks on the status tick).
    fn tsp_dock_fact(
        &self,
        p: &Paint<'_>,
        fact: &tsp::Fact,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let el = match &fact.body {
            tsp::FactBody::Text(text) if fact.permission => permission_chip(text.clone(), theme),
            body => {
                let mut el = quiet_chip(div(), theme).text_color(rgb(fact.color));
                if let Some(icon) = fact.icon {
                    el = el.child(icon_svg(
                        icon,
                        theme::ICON_SIZE_SMALL,
                        tsp::fact_icon_color(fact.color, theme),
                    ));
                }
                match body {
                    tsp::FactBody::Text(_) if !fact.runs.is_empty() => {
                        el.child(styled_runs(&fact.runs, fact.color, true, theme))
                    }
                    tsp::FactBody::Text(text) | tsp::FactBody::Rate(text) => el.child(text.clone()),
                    tsp::FactBody::Elapsed(id) => {
                        el.children(p.doc.get(id).map(|node| self.tsp_node(p, node, cx)))
                    }
                }
            }
        };
        self.tsp_chip(p, &fact.id, Look::Chip, el, cx)
    }

    /// The Send key chip (enabled with text, `submit`) or the Stop text button (`interrupt`),
    /// at the end of the bar.
    fn tsp_dock_primary(
        &self,
        p: &Paint<'_>,
        m: &tsp::DockModel,
        exited: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let session = p.session;
        let title = |id: &str, fallback: &'static str| {
            p.doc
                .get(id)
                .and_then(|node| str_prop(node, "title"))
                .unwrap_or(fallback)
                .to_owned()
        };
        match &m.bar.primary {
            tsp::Primary::Stop(id) => {
                let (tip, id) = (title(id, "Stop"), id.clone());
                stop_text_button("tsp-stop", !exited, theme)
                    .when(p.tips(), |el| el.tooltip(tooltip(theme, &tip, "")))
                    .on_press(
                        !exited,
                        cx.listener(move |shell, _, _, cx| {
                            shell.tsp_click(session, &id, false, cx)
                        }),
                    )
                    .into_any_element()
            }
            tsp::Primary::Send(id) => {
                let can = m.can_send() && !exited && id.is_some();
                let tip = id
                    .as_deref()
                    .map_or_else(|| "Send".to_owned(), |id| title(id, "Send"));
                let id = id.clone().unwrap_or_default();
                send_key_chip("tsp-send", can, theme)
                    .when(p.tips(), |el| el.tooltip(tooltip(theme, &tip, "")))
                    .on_press(
                        can,
                        cx.listener(move |shell, _, _, cx| {
                            shell.tsp_click(session, &id, false, cx)
                        }),
                    )
                    .into_any_element()
            }
        }
    }

    /// The row above the card and its meter, as Tern: on the left the working row (loading dots, elapsed time,
    /// a divider, the label) and any other status node; on the right the todo hud and the hud
    /// pill. `None` when there is nothing.
    fn tsp_dock_status(
        &self,
        p: &Paint<'_>,
        s: &tsp::StatusModel,
        cx: &mut Context<'_, Self>,
    ) -> Option<AnyElement> {
        if s.working.is_none() && s.todo.is_none() && s.pills.is_empty() && s.others.is_empty() {
            return None;
        }
        let doc = p.doc;
        let working = s.working.as_ref().map(|w| self.tsp_dock_working(p, w, cx));
        let others: Vec<AnyElement> = (s.others.iter())
            .filter_map(|id| doc.get(id))
            .map(|node| self.tsp_node(p, node, cx))
            .collect();
        // The checklist builder draws the hud chip and its popover (Task 10).
        let todo = (s.todo.as_deref())
            .and_then(|id| doc.get(id))
            .map(|node| self.tsp_node(p, node, cx));
        let pills = (!s.pills.is_empty()).then(|| self.tsp_dock_pills(p, &s.pills, cx));
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .min_w_0()
                .h(theme::rems_px(theme::TSP_STATUS_ROW_HEIGHT))
                .px(px(space::M))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .children(working)
                        .children(others),
                )
                .children(todo)
                .children(pills)
                .into_any_element(),
        )
    }

    /// The working row: loading dots, the elapsed time, a divider and the label. Its own Stop is
    /// not drawn: the card's Stop is the one.
    fn tsp_dock_working(
        &self,
        p: &Paint<'_>,
        w: &tsp::Working,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let elapsed = (w.elapsed.as_deref())
            .and_then(|id| p.doc.get(id))
            .map(|node| {
                div()
                    .flex_none()
                    .font_family(theme.terminal_font_family.clone())
                    .text_color(rgb(theme.text_subtle))
                    .child(self.tsp_node(p, node, cx))
            });
        div()
            .flex()
            .flex_1()
            .items_center()
            .gap(px(space::S))
            .min_w_0()
            .min_h(theme::rems_px(theme::TSP_HEAD_HEIGHT))
            .text_size(font_size::META)
            .child(
                div()
                    .flex_none()
                    .child(loading_dots(p.now, rgb(theme.status_working))),
            )
            .children(elapsed)
            .child(
                div()
                    .flex_none()
                    .w(px(theme::BORDER_WIDTH))
                    .h(theme::rems_px(space::M))
                    .bg(rgb(theme.subtle)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child(w.label.clone()),
            )
            .into_any_element()
    }

    /// One segment of the hud pill (subagents, jobs): the icon in its tone, then the
    /// count in mono; the node's `title` is the tooltip and a click is its action
    /// (`agents.open`, `jobs.open`).
    fn tsp_dock_pill(
        &self,
        p: &Paint<'_>,
        pill: &tsp::HudPill,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let tone = pill
            .tone
            .as_deref()
            .map_or(theme.text_muted, |tone| tsp::tone_color(tone, theme));
        let el = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(theme::TSP_HUD_SEGMENT_HEIGHT))
            .px(px(space::S))
            .rounded(px(theme::radius::PILL))
            .children(
                pill.icon
                    .map(|icon| icon_svg(icon, theme::ICON_SIZE_SMALL, tone)),
            )
            .child(pill.count.clone());
        self.tsp_chip(p, &pill.id, Look::Chip, el, cx)
    }

    /// The hud row as Tern draws it: ONE pill with a `layer[1]` ring and no fill, its segments
    /// split by a thin divider.
    fn tsp_dock_pills(
        &self,
        p: &Paint<'_>,
        pills: &[tsp::HudPill],
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let mut segs: Vec<AnyElement> = Vec::new();
        for (i, pill) in pills.iter().enumerate() {
            if i > 0 {
                segs.push(
                    div()
                        .flex_none()
                        .w(px(theme::BORDER_WIDTH))
                        .h(theme::rems_px(space::M))
                        .bg(gpui::rgba(theme.layer[1]))
                        .into_any_element(),
                );
            }
            segs.push(self.tsp_dock_pill(p, pill, cx));
        }
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(theme::rems_px(theme::COMPOSER_PILL_HEIGHT))
            .px(px(theme::TSP_HUD_PAD))
            .rounded(px(theme::radius::PILL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .children(segs)
            .into_any_element()
    }

    /// The strip above the field while omp's input goes to a subagent: an eye, omp's own
    /// children ("Viewing", the crumbs) and the exit chip.
    fn tsp_dock_focus(
        &self,
        p: &Paint<'_>,
        f: &tsp::FocusStrip,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let exit = f.exit.as_ref().map(|(id, label)| {
            let chip = div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .h(theme::rems_px(theme::PILL_HEIGHT))
                .px(px(space::S))
                .rounded(px(theme::radius::PILL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme::raised_border(theme)))
                .text_size(font_size::META)
                .text_color(rgb(theme.text))
                .child(label.clone());
            self.tsp_chip(p, id, Look::Chip, chip, cx)
        });
        let rest: Vec<AnyElement> = (f.rest.iter())
            .filter_map(|id| p.doc.get(id))
            .map(|node| self.tsp_node(p, node, cx))
            .collect();
        let strip = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .child(icon_svg(
                Icon::Eye,
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
            .children(rest)
            .child(div().flex_1())
            .children(exit);
        self.tsp_chip(p, &f.id, Look::Text, strip, cx)
    }
}

impl Shell {
    /// A node with an `omp.ask.*` role (spec 7.6). Kinds that already draw well (`md`, `kv`, the
    /// actions row) are handed to their own builder; no builder here calls `tsp_ask` for the same
    /// node again.
    fn tsp_ask(
        &self,
        p: &Paint<'_>,
        node: &Node,
        look: tsp::AskLook,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        use tsp::AskLook;
        match look {
            // The card is on `surface`, so the picked row (`surface_raised`) and the active tab
            // (`card`) show on it.
            // A hidden card is one row: "Question ..", Skip and Show.
            AskLook::Hidden => self.tsp_ask_card(p, node, true, cx),
            AskLook::Card => self.tsp_ask_card(p, node, false, cx),
            AskLook::Head => Self::tsp_ask_head(self.tsp_children(p, node, cx)),
            AskLook::Tabs => self.tsp_ask_tabs(p, node, cx),
            AskLook::Question => Self::tsp_ask_question(p, node),
            AskLook::Options => Self::tsp_ask_options(self.tsp_children(p, node, cx)),
            AskLook::Choice | AskLook::Check | AskLook::Other | AskLook::OtherCheck => {
                self.tsp_ask_choice(p, node, look, cx)
            }
            AskLook::Preview => self.tsp_text(p, node, cx),
            AskLook::Answers => self.tsp_data(p, node, cx),
            AskLook::Actions => self.tsp_layout(p, node, cx),
            AskLook::Notice => Self::tsp_ask_notice(p, node, false),
            AskLook::HookEyebrow => Self::tsp_ask_notice(p, node, true),
            AskLook::HookHead => Self::tsp_ask_options(self.tsp_children(p, node, cx)),
            AskLook::HookLine => self.tsp_hook_line(p, node, cx),
            AskLook::Summary => Self::tsp_ask_summary(p, node),
            AskLook::Button => self.tsp_ask_button(p, node, cx),
        }
    }

    #[inline(never)]
    fn tsp_ask_head(kids: Vec<AnyElement>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .children(kids)
            .into_any_element()
    }

    #[inline(never)]
    fn tsp_ask_options(kids: Vec<AnyElement>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .children(kids)
            .into_any_element()
    }

    #[inline(never)]
    fn tsp_ask_notice(p: &Paint<'_>, node: &Node, truncate: bool) -> AnyElement {
        let theme = p.theme;
        let runs = tsp::node_spans(&node.props, theme);
        div()
            .min_w_0()
            .when(truncate, |el| el.truncate())
            .text_size(font_size::META)
            .child(styled_runs(&runs, theme.text_muted, false, theme))
            .into_any_element()
    }

    /// `omp.hook-editor.line`: the note or custom answer field, framed like the composer's
    /// (placeholder as the Chat placeholder row, caret, text).
    #[inline(never)]
    fn tsp_hook_line(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let kids: Vec<AnyElement> = (p.doc.children(&node.id))
            .map(|child| match child.kind.as_str() {
                "editor" => {
                    let look = EditorLook::Composer { ghost: None };
                    self.tsp_editor(p, child, look, cx)
                }
                _ => self.tsp_node(p, child, cx),
            })
            .collect();
        Self::tsp_hook_field(p, kids)
    }

    #[inline(never)]
    fn tsp_hook_field(p: &Paint<'_>, kids: Vec<AnyElement>) -> AnyElement {
        let theme = p.theme;
        div()
            .id(SharedString::from(format!("tsp-hook-field-{}", p.session)))
            .flex()
            .items_start()
            .gap(px(space::S))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(theme::radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .cursor(CursorStyle::IBeam)
            .max_h(px(theme::COMPOSER_MAX_HEIGHT))
            .overflow_y_scroll()
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .children(kids)
            .into_any_element()
    }

    /// The ask card; a hidden one is a single row.
    #[inline(never)]
    fn tsp_ask_card(
        &self,
        p: &Paint<'_>,
        node: &Node,
        hidden: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let kids = self.tsp_children(p, node, cx);
        Self::tsp_ask_card_el(p, hidden, kids)
    }

    #[inline(never)]
    fn tsp_ask_card_el(p: &Paint<'_>, hidden: bool, kids: Vec<AnyElement>) -> AnyElement {
        let theme = p.theme;
        let el = div().flex().flex_col().min_w_0();
        let el = if hidden {
            el.px(px(space::M)).py(px(space::S))
        } else {
            el.gap(px(space::M)).p(px(space::M))
        };
        el.rounded(px(theme::radius::MEDIUM))
            .bg(rgb(theme.surface))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .children(kids)
            .into_any_element()
    }

    #[inline(never)]
    fn tsp_ask_question(p: &Paint<'_>, node: &Node) -> AnyElement {
        let theme = p.theme;
        let text = str_prop(node, "text").unwrap_or("");
        div()
            .child(styled_runs_w(
                &[(text.to_owned(), SpanStyle::default())],
                theme.text,
                false,
                weight::SEMIBOLD,
                theme,
            ))
            .into_any_element()
    }

    #[inline(never)]
    fn tsp_ask_summary(p: &Paint<'_>, node: &Node) -> AnyElement {
        let theme = p.theme;
        let runs = tsp::node_spans(&node.props, theme);
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(font_size::META)
            .child(styled_runs(&runs, theme.text_muted, false, theme))
            .into_any_element()
    }

    /// The question tabs: chips, the active one on the `card` fill. A click selects that tab
    /// (`select {id: tabs, item}`); the strip itself sends nothing.
    fn tsp_ask_tabs(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let active = str_prop(node, "active").unwrap_or("");
        let items = array_prop(node, "items");
        let tabs = items.iter().filter_map(|tab| {
            let id = tab.get("id")?.as_str()?.to_owned();
            let label = (tab.get("label"))
                .map(|l| tsp::spans_of(l, theme))
                .unwrap_or_default();
            let picked = id == active;
            let (session, list, tab_id) = (p.session, node.id.clone(), id.clone());
            Some(
                div()
                    .id(SharedString::from(format!(
                        "tsp-{}-{}-{id}",
                        p.session, node.id
                    )))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(theme::radius::SMALL))
                    .text_size(font_size::META)
                    .when(picked, |el| el.bg(rgb(theme.card)))
                    .pressable_unless(picked, Look::Chip, theme)
                    .on_press(
                        true,
                        cx.listener(move |shell, _, _, cx| {
                            if !tsp::root_owned(&list) {
                                shell.tsp_emit(
                                    session,
                                    tsp::select_event(&list, &tab_id, false),
                                    cx,
                                );
                            }
                        }),
                    )
                    .child(styled_runs(
                        &label,
                        if picked { theme.text } else { theme.text_muted },
                        false,
                        theme,
                    )),
            )
        });
        let strip = div().flex().flex_wrap().gap(px(space::XS)).children(tabs);
        match str_prop(node, "title").filter(|_| p.tips()) {
            Some(title) => strip
                .id(SharedString::from(format!(
                    "tsp-tabs-{}-{}",
                    p.session, node.id
                )))
                .tooltip(tooltip(theme, title, ""))
                .into_any_element(),
            None => strip.into_any_element(),
        }
    }

    /// One option: a radio (a check box for the `check` roles) filled with `accent` when picked,
    /// the label in `weight::MEDIUM`, the detail muted below, the value ("Recommended") as a
    /// badge on the right. The list's `selected` row is on `surface_raised` with an accent bar.
    fn tsp_ask_choice(
        &self,
        p: &Paint<'_>,
        node: &Node,
        look: tsp::AskLook,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let selected = (node.parent.as_deref())
            .and_then(|id| p.doc.get(id))
            .and_then(|list| str_prop(list, "selected"))
            == Some(node.id.as_str());
        let filled = tsp::choice_filled(node);
        let spans = |key: &str| {
            node.props
                .get(key)
                .map(|v| tsp::spans_of(v, theme))
                .unwrap_or_default()
        };
        let mark = Self::tsp_choice_mark(theme, look.boxed(), filled);
        let detail = spans("detail");
        let value = spans("value");
        let el = div()
            .flex()
            .items_start()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(theme::radius::SMALL))
            // A 2 px bar on the left of the picked row; the other rows keep it transparent, so
            // picking a row moves nothing.
            .border_l(px(theme::TSP_ASK_BAR_WIDTH))
            .border_color(if selected {
                rgb(theme.accent).into()
            } else {
                gpui::transparent_black()
            })
            .when(selected, |el| el.bg(rgb(theme.surface_raised)))
            .child(div().pt(px(space::XS)).child(mark))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(styled_runs_w(
                        &spans("label"),
                        theme.text,
                        false,
                        weight::MEDIUM,
                        theme,
                    ))
                    .when(!detail.is_empty(), |el| {
                        el.child(div().text_size(font_size::META).child(styled_runs(
                            &detail,
                            theme.text_muted,
                            false,
                            theme,
                        )))
                    }),
            )
            .when(!value.is_empty(), |el| {
                el.child(
                    div()
                        .flex_none()
                        .px(px(space::S))
                        .rounded(px(theme::radius::TAG))
                        .bg(rgb(theme.hover))
                        .text_size(font_size::META)
                        .child(styled_runs(&value, theme.text_muted, false, theme)),
                )
            });
        // Click selects, double click activates (`click_event` of an item in a list).
        self.tsp_decorate(el, p, node, Look::Row, selected, cx)
    }

    /// The radio (or check box) of an option.
    #[inline(never)]
    fn tsp_choice_mark(theme: &Theme, boxed: bool, filled: bool) -> Div {
        let size = theme::rems_px(theme::ICON_SIZE_SMALL);
        if boxed {
            div()
                .size(size)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(theme::radius::SMALL))
                .when(filled, |el| {
                    el.bg(rgb(theme.accent)).child(icon_svg(
                        Icon::Check,
                        theme::ICON_SIZE_SMALL,
                        theme.on_accent,
                    ))
                })
                .when(!filled, |el| {
                    el.border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.text_muted))
                })
        } else {
            div()
                .size(size)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(if filled {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .when(filled, |el| el.child(status_dot(theme.accent, false)))
        }
    }

    /// `omp.btn`: a pill in the card's bottom row. `tone: accent` is `accent` with `on_accent`
    /// text. The `kbd` child is not drawn; the node's `title` (with its keys) is the tooltip.
    fn tsp_ask_button(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let label: String = (p.doc.children(&node.id))
            .filter(|child| child.kind == "text")
            .flat_map(|child| tsp::node_spans(&child.props, theme))
            .map(|(text, _)| text)
            .collect();
        let accent = str_prop(node, "tone") == Some("accent");
        let (session, id) = (p.session, node.id.clone());
        let mut el = pill_button(
            SharedString::from(format!("tsp-btn-{}-{}", p.session, node.id)),
            label,
            accent,
            theme,
        )
        .on_press(
            true,
            cx.listener(move |shell, _, _, cx| {
                shell.tsp_click(session, &id, false, cx);
            }),
        );
        if let Some(title) = str_prop(node, "title").filter(|_| p.tips()) {
            el = el.tooltip(tooltip(theme, title, ""));
        }
        el.into_any_element()
    }
}
