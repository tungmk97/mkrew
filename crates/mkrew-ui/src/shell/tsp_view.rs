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
    Anchor, AnyElement, Bounds, ClickEvent, Div, FontStyle, FontWeight, StrikethroughStyle,
    StyledText, TextRun, anchored, deferred,
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

use super::chat_view::chat_column;
use crate::*;

/// What a pane in TSP mode remembers: the mirror of the daemon's documents and the UI-only state.
pub(crate) struct TspUi {
    /// `None` until the first `tsp.snapshot` reply.
    pub(crate) mirror: Option<Mirror>,
    scroll: ScrollHandle,
    /// Markdown and code parsed once per text (the preview parse is slow).
    md: RefCell<TextCache<docs::Shown>>,
    images: RefCell<HashMap<String, Arc<gpui::Image>>>,
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
    /// A `reveal` just moved the list: the next frame must not follow the end over it.
    reveal_pending: Cell<bool>,
    /// The scroll of each growing `list` (a picker's items scroll inside their card).
    lists: RefCell<HashMap<String, ListState>>,
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

impl TspUi {
    pub(crate) fn new() -> Self {
        Self {
            mirror: None,
            scroll: ScrollHandle::new(),
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
            reveal_pending: Cell::new(false),
            lists: RefCell::new(HashMap::new()),
        }
    }

    /// Scrolls the list a page (the height it shows) up or down.
    fn page(&self, up: bool) {
        let offset = self.scroll.offset();
        let page = f32::from(self.scroll.bounds().size.height);
        let max = f32::from(self.scroll.max_offset().y);
        let y = tsp::paged_offset(f32::from(offset.y), page, max, up);
        self.scroll.set_offset(point(offset.x, px(y)));
    }
}

impl Shell {
    /// What `info`'s pane shows (spec 7.3 table).
    pub(crate) fn tsp_pane_view(&self, info: &Session) -> PaneView {
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
        let TspChange::Frame { ops, .. } = &update.change else {
            return;
        };
        let Some((_, doc)) = mirror.surface(&update.surface) else {
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
        } else if let Some(ix) = reveal {
            // Index 0 of the list is the spacer under the pane's drag strip.
            ui.scroll.scroll_to_item(ix + 1);
            // The list may be at its end: the next frame must not follow it back down.
            ui.reveal_pending.set(true);
        }
        match tsp::scroll_in(ops) {
            Some(ListScroll::Top) => ui.scroll.set_offset(Point::default()),
            Some(ListScroll::Bottom) => ui.scroll.scroll_to_bottom(),
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

    /// Something on screen changes with time (the starting dots, a spinner, a running elapsed,
    /// a rate, a bar without an amount): the status tick redraws (ADR 0086). Only panes on screen
    /// count.
    pub(crate) fn tsp_animating(&self) -> bool {
        let visible = self.visible_sessions();
        (self.sidebar.sessions().iter())
            .filter(|s| visible.contains(&s.id))
            .any(|s| match self.tsp_pane_view(s) {
                PaneView::Starting => true,
                PaneView::Surface => (self.tsps.get(&s.id))
                    .and_then(|ui| ui.mirror.as_ref())
                    .and_then(Mirror::shown)
                    .is_some_and(|(_, doc)| tsp::animating(doc)),
                _ => false,
            })
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
    /// except Ctrl+C with a selection (copies) and Esc with a selection (clears it).
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
        let popover = (self.tsps.get(&session)).is_some_and(|ui| ui.todo_open.is_some());
        if tsp::escape_clears(popover, escape) {
            cx.stop_propagation();
            if let Some(ui) = self.tsps.get_mut(&session) {
                ui.todo_open = None;
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
        let Some(pane) = self.panes.get(&session).cloned() else {
            return;
        };
        pane.update(cx, |pane, cx| pane.key_down(event, window, cx));
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

/// `grow` > 0: the node takes the free space of its row or column.
fn grows(node: &Node) -> bool {
    num_prop(node, "grow").is_some_and(|grow| grow > 0.0)
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
    styled_runs_wavy(runs, base, all_mono, regular, false, theme)
}

/// [`styled_runs_w`] with a wavy `danger_text` underline under every run when `typo`.
fn styled_runs_wavy(
    runs: &[(String, SpanStyle)],
    base: u32,
    all_mono: bool,
    regular: f32,
    typo: bool,
    theme: &Theme,
) -> StyledText {
    // Private-use glyphs (omp's icons) are drawn in the terminal font, which carries them.
    let runs = tsp::with_icon_glyphs(runs);
    let ui_font = gpui::font(theme.ui_font_family.clone());
    let mono_font = terminal_font(theme);
    let mut text = String::new();
    let mut text_runs = Vec::new();
    for (part, style) in runs.iter().filter(|(part, _)| !part.is_empty()) {
        let mut font = if style.mono || all_mono {
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
    StyledText::new(text).with_runs(text_runs)
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
    /// last document read-only.
    pub(crate) fn render_tsp(
        &self,
        session: Ulid,
        is_focused: bool,
        width: f32,
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
        if let Some(cols) = tsp::fit_width(width, f32::from(cell_width)) {
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
        };
        let layer_paint = Paint {
            in_layer: true,
            ..paint
        };
        // Follow the end while the user is at it, except in the frame after a `reveal` moved the
        // list: that scroll is applied while this frame paints.
        let at_end = super::thread_panel::at_bottom(&ui.scroll);
        if !ui.reveal_pending.replace(false) && at_end {
            ui.scroll.scroll_to_bottom();
        }
        // Each row of `main` is a direct child of the scroll box (after the spacer), so `reveal`
        // can scroll to it. The spacer keeps the first row clear of the pane's 8 px drag strip
        // (`PANE_GRIP_HEIGHT`), the 2 px color line and the badges, like the Chat list's
        // `py(space::L)`.
        let rows: Vec<AnyElement> = std::iter::once(div().h(px(space::L)).into_any_element())
            .chain(
                (doc.children("main"))
                    .map(|node| chat_column(self.tsp_node(&paint, node, cx)).into_any_element()),
            )
            .collect();
        let dock: Vec<AnyElement> = (doc.children("dock"))
            .map(|node| self.tsp_node(&paint, node, cx))
            .collect();
        let layer: Vec<AnyElement> = (doc.children("layer"))
            .filter(|node| !bool_prop(node, "hidden"))
            .map(|node| self.tsp_layer_child(&layer_paint, node, cx))
            .collect();
        root.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id(SharedString::from(format!("tsp-main-{session}")))
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&ui.scroll)
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
                        )
                        .children(rows),
                )
                .when(!at_end, |el| {
                    el.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(space::S))
                            .child(chat_column(
                                div().flex().justify_end().child(
                                    float_button("tsp-scroll-end", Icon::ArrowDown, theme)
                                        .tooltip(tooltip(
                                            theme,
                                            "Scroll to the latest message",
                                            "End",
                                        ))
                                        .on_press(
                                            true,
                                            cx.listener(move |shell, _, _, cx| {
                                                if let Some(ui) = shell.tsps.get(&session) {
                                                    ui.scroll.scroll_to_bottom();
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                ),
                            )),
                    )
                }),
        )
        .child(chat_column(
            div().flex().flex_col().pb(px(space::S)).children(dock),
        ))
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

    /// A child of `layer`: an `overlay` is a modal card placed by its `anchor` and `size`; any other
    /// node (a sheet omp was not asked for) is drawn as is.
    fn tsp_layer_child(
        &self,
        p: &Paint<'_>,
        node: &Node,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        if node.kind != "overlay" {
            return self.tsp_node(p, node, cx);
        }
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
                    .children(self.tsp_children(p, node, cx)),
            )
            .into_any_element()
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
        if grows(node) { el.flex_1() } else { el }
    }

    /// col, row, card, section, rule, spacer, overlay, toast.
    fn tsp_layout(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let gap = px(tsp::gap_px(str_prop(node, "gap")));
        match node.kind.as_str() {
            // Toasts go to Mkrew's toast layer when their frame arrives (`tsp_update`).
            "toast" => div().into_any_element(),
            "spacer" => {
                let size = tsp::gap_px(str_prop(node, "size"));
                let el = if grows(node) {
                    div().flex_1()
                } else {
                    div().h(px(size)).w(px(size))
                };
                el.into_any_element()
            }
            "rule" => {
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
            "row" => {
                let el = div().flex().flex_row().items_center().gap(gap).min_w_0();
                let el = Self::tsp_flow(el, node).children(self.tsp_children(p, node, cx));
                self.tsp_decorate(el, p, node, Look::Text, false, cx)
            }
            "card" | "section" => self.tsp_card(p, node, cx),
            // "col" and an `overlay` met outside `layer` (an autocomplete under the editor).
            _ => {
                let mut el = div().flex().flex_col().gap(gap).min_w_0();
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
                let el = if p.in_layer { el.overflow_hidden() } else { el };
                let el = Self::tsp_flow(el, node).children(self.tsp_children(p, node, cx));
                self.tsp_decorate(el, p, node, Look::Text, false, cx)
            }
        }
    }

    /// card (a raised box) and section (a titled group); `collapsible` ones toggle on their head.
    fn tsp_card(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let card = node.kind == "card";
        let collapsible = bool_prop(node, "collapsible");
        let collapsed = bool_prop(node, "collapsed");
        let head_source = tsp::head_of(p.doc, node);
        let head_child = match head_source {
            tsp::Head::Child(child) => Some(child),
            _ => None,
        };
        let head = match head_source {
            tsp::Head::Spans(head) => tsp::spans_of(head, theme),
            _ => Vec::new(),
        };
        let head_color =
            str_prop(node, "tone").map_or(theme.text, |tone| tsp::tone_color(tone, theme));
        let head_el = (!head.is_empty() || head_child.is_some()).then(|| {
            let mut row = div().flex().items_center().gap(px(space::S));
            if collapsible {
                let chevron = if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                row = row.child(icon_svg(chevron, theme::ICON_SIZE_SMALL, theme.text_muted));
            }
            let row = match head_child {
                Some(child) => row.child(self.tsp_node(p, child, cx)),
                None => row.child(
                    div()
                        .font_weight(FontWeight(weight::SEMIBOLD))
                        .child(styled_runs(&head, head_color, false, theme)),
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
            .pressable(Look::Text, true, theme)
            .on_press(
                true,
                cx.listener(move |shell, _, _, cx| {
                    if let Some(event) = tsp::toggle_event(&id, !collapsed) {
                        shell.tsp_emit(session, event, cx);
                    }
                }),
            )
            .into_any_element()
        });
        let body = if collapsed {
            Vec::new()
        } else {
            (p.doc.children(&node.id))
                .filter(|child| head_child.is_none_or(|head| head.id != child.id))
                .map(|child| self.tsp_node(p, child, cx))
                .collect()
        };
        let mut el = div().flex().flex_col().gap(px(space::S)).min_w_0();
        if p.in_layer {
            // The overlay container is the card; it bounds this one.
            el = el.overflow_hidden();
        } else if card {
            el = el
                .p(px(space::M))
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface_raised));
        }
        let el = Self::tsp_flow(el, node).children(head_el).children(body);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
}

impl Shell {
    /// text, md, code, ansi, rows, shimmer.
    fn tsp_text(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let base = str_prop(node, "tone").map_or(theme.text, |tone| tsp::tone_color(tone, theme));
        match node.kind.as_str() {
            "md" | "code" => {
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
                let el = div()
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    .line_height(relative(theme::doc::LINE_HEIGHT))
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
            "ansi" | "rows" => {
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
            // text and shimmer (plain text in P1).
            _ => {
                let runs = tsp::node_spans(&node.props, theme);
                let mut el = div()
                    .min_w_0()
                    .child(styled_runs(&runs, base, false, theme));
                if node.props.get("wrap").and_then(Value::as_str) == Some("none") {
                    el = el.whitespace_nowrap();
                }
                if str_prop(node, "truncate").is_some() {
                    el = el.truncate();
                }
                if grows(node) {
                    el = el.flex_1();
                }
                self.tsp_decorate(el, p, node, Look::Text, false, cx)
            }
        }
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
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        match node.kind.as_str() {
            "kv" => {
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
            "table" => {
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
            "list" => {
                let kids = self.tsp_children(p, node, cx);
                let empty = (kids.is_empty())
                    .then(|| node.props.get("empty").map(|e| tsp::spans_of(e, theme)))
                    .flatten();
                let items = div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .children(kids)
                    .children(empty.map(|runs| {
                        div().p(px(space::S)).child(styled_runs(
                            &runs,
                            theme.text_muted,
                            false,
                            theme,
                        ))
                    }));
                // A growing list takes the height left in its card and scrolls inside; its
                // selected item is kept in view.
                let el = if grows(node) {
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
                    div().flex().flex_col().flex_1().min_h_0().child(scroller)
                } else {
                    items
                };
                self.tsp_decorate(el, p, node, Look::Text, false, cx)
            }
            "item" => {
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
                        el.child(div().flex_none().whitespace_nowrap().child(styled_runs(
                            &value,
                            theme.text_muted,
                            false,
                            theme,
                        )))
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
            "tabs" => {
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
            "badge" => {
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
            "kbd" => {
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
            "icon" => {
                let icon = str_prop(node, "name")
                    .and_then(|name| self.tsp_icon(p, name, tone.unwrap_or(theme.text_muted)));
                self.tsp_decorate(div().children(icon), p, node, Look::Icon, false, cx)
            }
            "status" => {
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
                let mut el = div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .overflow_hidden()
                    .children(left);
                if !right.is_empty() {
                    el = el.child(div().flex_1()).children(right);
                }
                if grows(node) {
                    el = el.flex_1();
                }
                self.tsp_decorate(el, p, node, Look::Text, false, cx)
            }
            // seg
            _ => {
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
    }
}

impl Shell {
    /// A track and its fill; `None` is an unknown amount (the loading mark).
    fn tsp_bar(p: &Paint<'_>, value: Option<f64>, color: u32) -> AnyElement {
        let theme = p.theme;
        let Some(value) = value else {
            return loading_dots(p.now, rgb(color)).into_any_element();
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

    /// spinner, elapsed, progress, rate, meter, effort: redrawn on the status tick (ADR 0086).
    fn tsp_live(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let tone = str_prop(node, "tone").map(|tone| tsp::tone_color(tone, theme));
        let label = |key: &str| {
            node.props
                .get(key)
                .map(|v| tsp::spans_of(v, theme))
                .unwrap_or_default()
        };
        let el = match node.kind.as_str() {
            "spinner" => div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(loading_dots(
                    p.now,
                    rgb(tone.unwrap_or(theme.status_working)),
                ))
                .child(styled_runs(&label("label"), theme.text_muted, false, theme)),
            "elapsed" => {
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
            "progress" => div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(Self::tsp_bar(
                    p,
                    num_prop(node, "value"),
                    tone.unwrap_or(theme.accent),
                ))
                .child(styled_runs(&label("label"), theme.text_muted, false, theme)),
            "rate" => {
                let text = tsp::rate_label(
                    num_prop(node, "value").unwrap_or(0.0),
                    str_prop(node, "unit").unwrap_or(""),
                );
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(text)
            }
            // A timeout ring (the ask card): a ring with the seconds left. The arc is not drawn.
            "meter" if str_prop(node, "style") == Some("ring") => div()
                .size(px(theme::TSP_RING_SIZE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.accent))
                .text_size(font_size::META)
                .child(styled_runs(&label("label"), theme.text_muted, false, theme)),
            "meter" => {
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
                        &label("label"),
                        theme.text_muted,
                        false,
                        theme,
                    )))
            }
            // effort
            _ => {
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
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
}

impl Shell {
    /// editor, input, image.
    fn tsp_input(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        if node.kind == "image" {
            return self.tsp_image(p, node, cx);
        }
        let theme = p.theme;
        let text = str_prop(node, "text").unwrap_or("");
        let caret_at = (node.props.get("cursor").and_then(Value::as_u64))
            .map(|units| tsp::byte_at_utf16(text, units as usize));
        // omp says which editable node has the focus (`focus` op); until it does, the pane's
        // only editor is the one.
        let has_focus = p.focused && p.doc.focus().is_none_or(|id| id == node.id);
        let caret = || div().w(px(theme::BORDER_WIDTH)).bg(rgb(theme.text));
        let part = |s: &str, color: u32| {
            div().min_w_0().child(styled_runs(
                &[(s.to_owned(), SpanStyle::default())],
                color,
                false,
                theme,
            ))
        };
        let piece = |s: &str, decor: tsp::Decor| {
            div().min_w_0().child(styled_runs_wavy(
                &[(s.to_owned(), decor.style)],
                theme.text,
                false,
                weight::REGULAR,
                decor.typo,
                theme,
            ))
        };
        let line_height = theme::rems_px(space::L + space::XS);
        // The prompt starts the first line, before the caret and the text.
        let mut lead = (node.props.get("prompt"))
            .map(|v| tsp::spans_of(v, theme))
            .filter(|runs| !runs.is_empty())
            .map(|runs| {
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .child(styled_runs(&runs, theme.text_muted, false, theme))
                    .into_any_element()
            });
        let mut lines: Vec<AnyElement> = Vec::new();
        if text.is_empty() {
            let placeholder = (node.props.get("placeholder"))
                .map(|v| tsp::spans_of(v, theme))
                .unwrap_or_default();
            let mut row = div().flex().items_center().min_h(line_height);
            row = row.children(lead.take());
            if has_focus {
                row = row.child(caret());
            }
            lines.push(
                row.child(styled_runs(&placeholder, theme.text_dim, false, theme))
                    .into_any_element(),
            );
        } else {
            // Decorations (mention colors, hidden markers, typos) cut the text into pieces.
            let pieces = tsp::decor_pieces(text, array_prop(node, "decor"), theme);
            let caret_at = caret_at.filter(|_| has_focus);
            let mut start = 0;
            for line in text.split('\n') {
                let end = start + line.len();
                let mut row = div().flex().items_center().min_h(line_height);
                row = row.children(lead.take());
                for atom in tsp::line_atoms(&pieces, start..end, caret_at) {
                    row = match atom {
                        tsp::Atom::Caret => row.child(caret()),
                        tsp::Atom::Text(range, decor) => row.child(piece(&text[range], decor)),
                    };
                }
                lines.push(row.into_any_element());
                start = end + 1;
            }
        }
        // Inline suggestion after the text.
        let ghost = str_prop(node, "ghost").filter(|g| !g.is_empty() && !text.is_empty());
        let session = p.session;
        let id = node.id.clone();
        // A text field: exempt from the press look, so a plain mouse-down sends the focus.
        let el = div()
            .flex()
            .flex_col()
            .min_w_0()
            .children(lines)
            .children(ghost.map(|g| part(g, theme.text_dim)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, _, cx| {
                    shell.tsp_click(session, &id, false, cx);
                }),
            );
        let el = if grows(node) { el.flex_1() } else { el };
        el.into_any_element()
    }

    /// An inline image from the blob bytes the daemon sent; nothing for a built-in or missing one.
    fn tsp_image(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let blob = str_prop(node, "blob").and_then(|id| p.mirror.blob(id));
        let image = blob.and_then(|blob| {
            let mut cache = p.ui.images.borrow_mut();
            if let Some(image) = cache.get(&blob.id) {
                return Some(image.clone());
            }
            let image = Arc::new(gpui::Image::from_bytes(
                image_format(&blob.mime)?,
                blob.data.clone(),
            ));
            cache.insert(blob.id.clone(), image.clone());
            Some(image)
        });
        let el = match (image, str_prop(node, "alt")) {
            (Some(image), _) => div()
                .max_w(px(theme::TSP_IMAGE_MAX_WIDTH))
                .child(gpui::img(image)),
            (None, Some(alt)) if str_prop(node, "builtin").is_none() => div()
                .text_color(rgb(theme.text_muted))
                .child(alt.to_owned()),
            _ => div(),
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
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

/// The mark of a todo item or phase.
fn todo_mark(mark: tsp::ItemMark, color: u32) -> AnyElement {
    match mark {
        tsp::ItemMark::Active => div()
            .size(theme::rems_px(theme::ICON_SIZE_SMALL))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(color))
            .child(status_dot(color, false))
            .into_any_element(),
        tsp::ItemMark::Ring => status_dot(color, true).flex_none().into_any_element(),
        tsp::ItemMark::Check => icon_svg(Icon::Check, theme::ICON_SIZE_SMALL, color)
            .flex_none()
            .into_any_element(),
    }
}

impl Shell {
    /// `checklist`: the hud chip with its popover, the `full` card, the `reminder` notice.
    fn tsp_checklist(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let list = tsp::Checklist::from_props(&node.props);
        match list.mode {
            tsp::ChecklistMode::Hud => self.tsp_todo_hud(p, node, &list, cx),
            tsp::ChecklistMode::Full => div()
                .p(px(space::M))
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .child(self.tsp_phases(p, node, &list, cx))
                .into_any_element(),
            tsp::ChecklistMode::Reminder => div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .children(
                    list.note
                        .as_ref()
                        .map(|note| div().text_color(rgb(theme.text_muted)).child(note.clone())),
                )
                .child(self.tsp_phases(p, node, &list, cx))
                .into_any_element(),
        }
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
                    tsp::PhaseMark::Working => {
                        loading_dots(p.now, rgb(theme.status_working)).into_any_element()
                    }
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
        div()
            .flex()
            .items_start()
            .gap(px(space::S))
            .px(px(space::S))
            .child(
                div()
                    .pt(px(space::XS))
                    .child(todo_mark(look.mark, look.mark_color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(styled_runs(&runs, look.text_color, false, theme))
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
            tsp::HudMark::Dots => loading_dots(p.now, rgb(theme.status_working)).into_any_element(),
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
            .gap(px(space::S))
            .px(px(space::S))
            .h(theme::rems_px(theme::PILL_HEIGHT))
            .rounded(px(theme::radius::PILL))
            .text_size(font_size::META)
            .child(mark)
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
            .flex_none()
            .child(chip)
            .child(chip_bounds)
            .when(open, |el| {
                let height =
                    f32::from(p.ui.scroll.bounds().size.height) * theme::TSP_POPOVER_MAX_FRACTION;
                let panel = floating_panel(
                    SharedString::from(format!("tsp-todo-panel-{}", p.session)),
                    theme,
                )
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
        let theme = p.theme;
        match look {
            // The card is on `surface`, so the picked row (`surface_raised`) and the active tab
            // (`card`) show on it.
            // A hidden card is one row: "Question ..", Skip and Show.
            AskLook::Hidden => div()
                .flex()
                .flex_col()
                .px(px(space::M))
                .py(px(space::S))
                .min_w_0()
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .children(self.tsp_children(p, node, cx))
                .into_any_element(),
            AskLook::Card => div()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .p(px(space::M))
                .min_w_0()
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .children(self.tsp_children(p, node, cx))
                .into_any_element(),
            AskLook::Head => div()
                .flex()
                .items_center()
                .children(self.tsp_children(p, node, cx))
                .into_any_element(),
            AskLook::Tabs => self.tsp_ask_tabs(p, node, cx),
            AskLook::Question => {
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
            AskLook::Options => div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .children(self.tsp_children(p, node, cx))
                .into_any_element(),
            AskLook::Choice | AskLook::Check | AskLook::Other | AskLook::OtherCheck => {
                self.tsp_ask_choice(p, node, look, cx)
            }
            AskLook::Preview => self.tsp_text(p, node, cx),
            AskLook::Answers => self.tsp_data(p, node, cx),
            AskLook::Actions => self.tsp_layout(p, node, cx),
            AskLook::Notice => {
                let runs = tsp::node_spans(&node.props, theme);
                div()
                    .text_size(font_size::META)
                    .child(styled_runs(&runs, theme.text_muted, false, theme))
                    .into_any_element()
            }
            AskLook::Summary => {
                let runs = tsp::node_spans(&node.props, theme);
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::META)
                    .child(styled_runs(&runs, theme.text_muted, false, theme))
                    .into_any_element()
            }
            AskLook::Button => self.tsp_ask_button(p, node, cx),
        }
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
        let size = theme::rems_px(theme::ICON_SIZE_SMALL);
        let mark = if look.boxed() {
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
        };
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
