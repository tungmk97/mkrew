//! GPUI terminal pane: renders one attached daemon session (SPEC §10, §14).
//!
//! Performance rules: one custom element paints the whole grid; each row is batched into a few
//! shaped text runs (`layout::layout_row`), shaped rows are cached and rebuilt only when dirty
//! (`layout::RowCache`). The owner feeds output with [`TerminalPane::feed`] and notifies once per
//! batch, then waits for the `painted` signal, so bursts coalesce into one repaint per frame.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use alacritty_terminal::index::Side;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, CursorStyle, Element, ElementId, Entity,
    EventEmitter, FocusHandle, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, KeyDownEvent, LayoutId, ModifiersChangedEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, ShapedLine, SharedString, Style,
    TextAlign, TextRun, UnderlineStyle, Window, canvas, div, fill, font, point, px, relative, rgb,
    size,
};
use mkrew_protocol::{
    AttachmentSaveParams, AttachmentSaveResult, Outcome, Request, ScreenSnapshot, Session,
    SessionInputParams, SessionResizeParams,
};
use ulid::Ulid;

use crate::client::Connection;
use crate::composer::{attachment, typed_paths};
use crate::keymap::{Command, Run, TERMINAL_CONTEXT};
use crate::terminal::emulator::{Emulator, Grid};
use crate::terminal::find::{FindBar, FindKey, FindStatus, find_key};
use crate::terminal::input::{KeyAction, is_copy_key, key_action, paste_bytes, wheel_bytes};
use crate::terminal::layout::{Background, CellStyle, RowCache, cell_at, grid_size, layout_row};
use crate::terminal::links::{self, Target};
use crate::theme::{self, ActiveTheme, Theme, font_size, radius, space};

/// Cell width guess used only until the terminal font is resolved.
const CELL_WIDTH_ESTIMATE: f32 = 0.6;

/// `attachment.save` refuses larger images (§20B.5); not worth sending.
const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// One attached session: the UI-side emulator plus its shaped-row cache.
pub struct TerminalPane {
    session_id: Ulid,
    connection: Rc<Connection>,
    emulator: Emulator,
    theme: Theme,
    rows: Rc<RefCell<RowCache<ShapedRow>>>,
    focus_handle: FocusHandle,
    /// Signalled after each paint; paces the owner's output drain loop.
    painted: async_channel::Sender<()>,
    /// `Some(exit_code)` once the session's process exited.
    exited: Option<Option<i32>>,
    /// Where the grid was last painted, to map mouse positions to cells.
    geometry: Option<Geometry>,
    /// A left-button drag is extending the selection.
    selecting: bool,
    /// Wheel movement below one line, carried to the next event.
    scroll_remainder: f32,
    /// The open find bar (Ctrl+Shift+F) and its keyboard focus.
    find: Option<FindBar>,
    find_focus: FocusHandle,
    /// The mouse-down being dispatched hit the find bar: the terminal must not take focus or
    /// start a selection (the event still bubbles so the shell focuses this pane).
    find_clicked: bool,
    /// Composing IME chars typed into the PTY or the find bar so far (see `crate::ime`).
    ime_marked: Rc<crate::ime::Composing<FocusHandle>>,
    /// `None` = no links (the history's replay pane). `Some(base)`: URLs, and with a `base`
    /// (the project folder, or the task worktree of the pane's agent) file paths.
    links: Option<Option<PathBuf>>,
    /// The link under the pointer while Ctrl / Cmd is held, and the cell it was looked up for
    /// (the row is read again only when the pointer moves to another cell).
    hover_link: Option<HoverLink>,
    probed: Option<(usize, usize)>,
}

/// Ctrl+Click (Cmd+Click on macOS) on a link of the terminal: the shell opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenLink {
    Url(String),
    /// A file of the pane's folder: its Files path (`/` separators) and the line to show.
    File {
        path: String,
        line: Option<u32>,
    },
}

impl EventEmitter<OpenLink> for TerminalPane {}

/// The link under the pointer while Ctrl / Cmd is held: its row and cells, for the underline.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HoverLink {
    row: usize,
    start: usize,
    end: usize,
    open: OpenLink,
}

/// Window position of the grid's top-left cell and the cell size.
#[derive(Clone, Copy)]
struct Geometry {
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
}

impl TerminalPane {
    /// A pane showing `snapshot` (the `session.attach` result) of `session_id`. Not focused;
    /// the owner calls [`TerminalPane::focus`].
    pub fn new(
        session_id: Ulid,
        snapshot: &ScreenSnapshot,
        connection: Rc<Connection>,
        painted: async_channel::Sender<()>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let emulator = Emulator::from_snapshot(snapshot);
        Self::with_emulator(session_id, emulator, None, connection, painted, window, cx)
    }

    /// A read-only pane replaying `log` (output of the finished `session`) on the session's
    /// screen size: never attached, takes no input and never resizes the session.
    pub fn replay(
        session: &Session,
        log: &[u8],
        connection: Rc<Connection>,
        painted: async_channel::Sender<()>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut emulator = Emulator::new(session.cols.max(1).into(), session.rows.max(1).into());
        emulator.advance(log);
        let exited = Some(session.exit_code);
        Self::with_emulator(
            session.id, emulator, exited, connection, painted, window, cx,
        )
    }

    fn with_emulator(
        session_id: Ulid,
        emulator: Emulator,
        exited: Option<Option<i32>>,
        connection: Rc<Connection>,
        painted: async_channel::Sender<()>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let focus_handle = cx.focus_handle();
        Self {
            session_id,
            connection,
            emulator,
            theme: cx.theme().clone(),
            rows: Rc::default(),
            focus_handle,
            painted,
            exited,
            geometry: None,
            selecting: false,
            scroll_remainder: 0.0,
            find: None,
            find_focus: cx.focus_handle(),
            find_clicked: false,
            ime_marked: Rc::default(),
            links: None,
            hover_link: None,
            probed: None,
        }
    }

    pub fn session_id(&self) -> Ulid {
        self.session_id
    }

    /// Turns on Ctrl+Click links; `base` is where typed relative paths start (`None` = URLs only).
    pub fn set_links(&mut self, base: Option<PathBuf>) {
        self.links = Some(base);
    }

    /// Sends keyboard input to this pane; keeps it where it is if the pane (terminal or its
    /// find bar) already has it.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if !self.focus_handle.contains_focused(window, cx) {
            window.focus(&self.focus_handle, cx);
        }
    }

    /// Whether the pane (or its find bar) has the keyboard.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle.contains_focused(window, cx)
    }

    /// The selected text, for "Send selection" (§20B.4).
    pub fn selection_text(&self) -> Option<String> {
        self.emulator.selection_text()
    }

    /// Feeds output bytes. The caller notifies once per batch (≤ one repaint per frame).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.emulator.advance(bytes);
    }

    /// Replaces the screen with a fresh daemon snapshot (`session.snapshot` after the output
    /// queue overflowed). The scrollback restarts from the snapshot's history (≤ 200 lines,
    /// ADR 0030). The caller notifies.
    pub fn load_snapshot(&mut self, snapshot: &ScreenSnapshot) {
        self.emulator = Emulator::from_snapshot(snapshot);
        if let Some(find) = &self.find {
            find.reapply(&mut self.emulator);
        }
    }

    /// The session's process exited: keep the last screen, stop sending input, say so.
    /// The caller notifies.
    pub fn set_exited(&mut self, exit_code: Option<i32>) {
        self.exited = Some(exit_code);
    }

    /// Fits the session to `cols` x `rows`; a changed size goes to the emulator and the daemon.
    fn fit(&mut self, cols: u16, rows: u16, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() || (usize::from(cols), usize::from(rows)) == self.emulator.size() {
            return;
        }
        self.emulator.resize(cols.into(), rows.into());
        let request = Request::SessionResize(SessionResizeParams {
            session_id: self.session_id,
            cols,
            rows,
        });
        self.request(request, "session.resize", cx);
    }

    /// Sends `request` without waiting; a failure is logged (the pane keeps working).
    fn request(&self, request: Request, method: &'static str, cx: &mut Context<'_, Self>) {
        let reply = self.connection.request(request);
        let session_id = self.session_id;
        cx.background_executor()
            .spawn(async move {
                match reply.await {
                    Ok(Outcome::Result(_)) => {}
                    Ok(Outcome::Error(body)) => {
                        tracing::warn!(%session_id, "{method} failed: {}", body.message);
                    }
                    Err(error) => tracing::warn!(%session_id, "{method} failed: {error:#}"),
                }
            })
            .detach();
    }

    /// Keys go to the session. Keystrokes bound in the keymap (SPEC §13) never get here: their
    /// action handler stops them first. Plain Ctrl+C copies a selection instead, also in an
    /// exited pane (ADR 0022). Plain Ctrl+V pastes (ADR 0026).
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        // Only plain Ctrl+C reads the selection: building its text per keystroke costs O(selection).
        let selected = is_copy_key(&event.keystroke) && self.emulator.selection_text().is_some();
        let mode = self.emulator.input_mode();
        match key_action(&event.keystroke, selected, self.exited.is_some(), mode) {
            KeyAction::Copy => {
                cx.stop_propagation();
                self.copy_selection(cx);
            }
            KeyAction::Send(data) => {
                cx.stop_propagation();
                self.input(data, cx);
            }
            KeyAction::Paste => {
                cx.stop_propagation();
                if !self.paste(cx) {
                    self.input(vec![0x16], cx);
                }
            }
            KeyAction::Ignore => {}
        }
    }

    /// Pastes the clipboard (§20B.5): text (bracketed when asked), else an image saved through
    /// `attachment.save` and typed as its path, else copied files typed as paths. `false` when
    /// the clipboard holds none of these (Ctrl+V then sends ^V, ADR 0026).
    fn paste(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        if self.exited.is_some() {
            return true;
        }
        if let Some(text) = item.text() {
            let data = paste_bytes(&text, self.emulator.input_mode());
            self.input(data, cx);
            return true;
        }
        for entry in item.into_entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    if let Some((ext, bytes)) = attachment(image) {
                        self.paste_image(ext, bytes, cx);
                        return true;
                    }
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    let paths: Vec<String> = paths
                        .paths()
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    self.type_text(&typed_paths(paths.iter().map(String::as_str)), cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Copies the selection to the clipboard and clears it; `false` when nothing is selected.
    fn copy_selection(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(text) = self.emulator.selection_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.emulator.clear_selection();
        cx.notify();
        true
    }

    /// The visible cell under a window position, once the grid has been painted.
    fn cell_at(&self, position: Point<Pixels>) -> Option<(usize, usize, Side)> {
        let geometry = self.geometry?;
        let at = position - geometry.origin;
        Some(cell_at(
            f32::from(at.x),
            f32::from(at.y),
            f32::from(geometry.cell_width),
            f32::from(geometry.line_height),
            self.emulator.size(),
        ))
    }

    /// The cell under `position` when it is inside the painted grid.
    fn cell_inside(&self, position: Point<Pixels>) -> Option<(usize, usize)> {
        let geometry = self.geometry?;
        let at = position - geometry.origin;
        let (cols, rows) = self.emulator.size();
        let inside = at.x >= px(0.)
            && at.y >= px(0.)
            && at.x < geometry.cell_width * cols as f32
            && at.y < geometry.line_height * rows as f32;
        let (row, col, _) = self.cell_at(position).filter(|_| inside)?;
        Some((row, col))
    }

    /// The link at a cell, read from that row of the cached grid only.
    fn link_in(&self, row: usize, col: usize) -> Option<HoverLink> {
        let base = self.links.as_ref()?;
        let cells = self.emulator.shared_grid().row_chars(row)?;
        let link = links::link_at(&cells, col)?;
        let open = match link.target {
            Target::Url(url) => OpenLink::Url(url),
            Target::Path { path, line } => OpenLink::File {
                path: links::resolve(base.as_deref()?, &path)?,
                line,
            },
        };
        Some(HoverLink {
            row,
            start: link.start,
            end: link.end,
            open,
        })
    }

    /// Ctrl / Cmd held (or not) with the pointer at `position`: finds the link under it, or
    /// drops the one shown. One row is read, and only when the pointer moved to another cell,
    /// so holding the key costs nothing while output streams; without the key nothing runs.
    fn update_link(&mut self, held: bool, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        if self.links.is_none() {
            return;
        }
        let cell = if held {
            self.cell_inside(position)
        } else {
            None
        };
        if cell == self.probed {
            return;
        }
        self.probed = cell;
        let found = cell.and_then(|(row, col)| self.link_in(row, col));
        if found != self.hover_link {
            self.hover_link = found;
            cx.notify();
        }
    }

    /// Drops the link under the pointer and forgets the probed cell.
    fn clear_link(&mut self, cx: &mut Context<'_, Self>) {
        self.probed = None;
        if self.hover_link.take().is_some() {
            cx.notify();
        }
    }

    fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if std::mem::take(&mut self.find_clicked) {
            return;
        }
        // Ctrl+Click (Cmd+Click) on a link opens it: no selection starts, and the pane does
        // not take the keyboard from the view the link opens.
        if event.modifiers.secondary() && self.links.is_some() {
            // Looked up again here: output or the wheel may have moved the text since the pointer
            // last changed cell. One row is read per click.
            let link =
                (self.cell_inside(event.position)).and_then(|(row, col)| self.link_in(row, col));
            if let Some(link) = link {
                cx.emit(link.open);
                return;
            }
        }
        window.focus(&self.focus_handle, cx);
        if let Some((row, col, side)) = self.cell_at(event.position) {
            self.emulator.start_selection(row, col, side);
            self.selecting = true;
            cx.notify();
        }
    }

    fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.update_link(event.modifiers.secondary(), event.position, cx);
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let Some((row, col, side)) = self.cell_at(event.position) {
            self.emulator.update_selection(row, col, side);
            cx.notify();
        }
    }

    fn mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) {
        self.selecting = false;
    }

    /// The wheel scrolls the UI-side history (wheel up = back in time); full-screen programs
    /// that asked for alternate scroll get arrow keys instead.
    fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(geometry) = self.geometry else {
            return;
        };
        let lines = f32::from(event.delta.pixel_delta(geometry.line_height).y)
            / f32::from(geometry.line_height)
            + self.scroll_remainder;
        self.scroll_remainder = lines.fract();
        // Whole lines of one wheel event: far inside i32.
        let whole = lines.trunc() as i32;
        if whole == 0 {
            return;
        }
        if self.emulator.wheel_sends_arrows() {
            if self.exited.is_none() {
                let data = wheel_bytes(whole, self.emulator.input_mode());
                self.input(data, cx);
            }
        } else {
            self.emulator.scroll_lines(whole);
            // The text moved under the pointer: the link shown (and the cell probed) is stale.
            self.clear_link(cx);
            cx.notify();
        }
    }

    /// The terminal's own commands; all others go on to the window shell.
    fn run(&mut self, Run(command): &Run, window: &mut Window, cx: &mut Context<'_, Self>) {
        match command {
            Command::Find => {
                self.find.get_or_insert_default();
                window.focus(&self.find_focus, cx);
                cx.notify();
            }
            Command::Paste if self.find.is_some() && self.find_focus.is_focused(window) => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                if let (Some(find), Some(text)) = (self.find.as_mut(), text) {
                    find.insert(&text, &mut self.emulator);
                    cx.notify();
                }
            }
            Command::Copy => {
                self.copy_selection(cx);
            }
            Command::PageUp | Command::PageDown => {
                self.emulator.scroll_page(*command == Command::PageUp);
                cx.notify();
            }
            Command::Paste => {
                self.paste(cx);
            }
            _ => cx.propagate(),
        }
    }

    /// Keys typed into the find bar edit the query or move between matches; none reach the
    /// session.
    fn find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        if key == FindKey::Paste {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            if let (Some(find), Some(text)) = (self.find.as_mut(), text) {
                find.insert(&text, &mut self.emulator);
                cx.notify();
            }
            return;
        }
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if find.key(key, &mut self.emulator) {
            cx.notify();
        } else {
            self.close_find(window, cx);
        }
    }

    /// Closes the find bar, clears the highlights and gives the terminal the keyboard.
    fn close_find(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.find = None;
        self.emulator.set_search("");
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// A click on the find bar focuses it instead of starting a terminal selection.
    fn find_mouse_down(
        &mut self,
        _event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.find_clicked = true;
        window.focus(&self.find_focus, cx);
        cx.notify();
    }

    fn render_find_bar(&self, window: &Window, cx: &mut Context<'_, Self>) -> Option<gpui::Div> {
        let find = self.find.as_ref()?;
        let theme = &self.theme;
        let focused = self.find_focus.is_focused(window);
        let (text, color) = if find.query().is_empty() {
            ("Find (regex)".to_owned(), theme.text_muted)
        } else {
            (find.query().to_owned(), theme.text)
        };
        let caret = div()
            .flex_none()
            .w(px(theme::BORDER_WIDTH))
            .h(font_size::BODY)
            .bg(rgb(theme.text));
        // While typing, the caret sits after the query; the placeholder comes after the caret.
        // Ctrl+A highlights the query instead of showing the caret.
        let selected = find.selected();
        let mut shown = div().text_color(rgb(color));
        if selected {
            shown = shown.bg(rgb(theme.terminal_selection_bg));
        }
        let query = div().flex().items_center().child(shown.child(text));
        let query = if !focused {
            query
        } else if find.query().is_empty() {
            div().flex().items_center().child(caret).child(query)
        } else if selected {
            query
        } else {
            query.child(caret)
        };
        let status = match find.status() {
            FindStatus::Invalid => format!("✕ {}", find.status().label()),
            status => status.label().to_owned(),
        };
        let hover = theme.border;
        let close = div()
            .id("find-close")
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(theme::opacity::ACTIVE))
            .on_click(cx.listener(|pane, _, window, cx| pane.close_find(window, cx)))
            .child("✕");
        Some(
            div()
                .absolute()
                .top(px(space::S))
                .right(px(space::L))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(if focused {
                    theme.pane_focus_border
                } else {
                    theme.border
                }))
                .bg(rgb(theme.surface_raised))
                .shadow_sm()
                .font_family(theme.ui_font_family.clone())
                .text_size(font_size::BODY)
                .cursor(CursorStyle::IBeam)
                .track_focus(&self.find_focus)
                .on_key_down(cx.listener(Self::find_key_down))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::find_mouse_down))
                .child(
                    div()
                        .w(px(theme::FIND_INPUT_WIDTH))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(query),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(status),
                )
                .child(close)
                .child({
                    let (focus, view) = (self.find_focus.clone(), cx.entity());
                    let marked = self.ime_marked.clone();
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, cx| {
                            let typing = crate::ime::Typing {
                                view,
                                on_key: TerminalPane::find_key_down,
                                marked,
                                focus: focus.clone(),
                                caret: bounds,
                            };
                            window.handle_input(&focus, typing, cx);
                        },
                    )
                    .absolute()
                    .size_full()
                }),
        )
    }

    /// Stores a clipboard image through `attachment.save`, then types its path (§20B.5).
    fn paste_image(&mut self, ext: &str, data: Vec<u8>, cx: &mut Context<'_, Self>) {
        let session_id = self.session_id;
        if data.len() > MAX_ATTACHMENT_BYTES {
            tracing::warn!(%session_id, size = data.len(), "pasted image over 10 MiB, not saved");
            return;
        }
        let saved = self
            .connection
            .call::<AttachmentSaveResult>(Request::AttachmentSave(AttachmentSaveParams {
                session_id,
                ext: ext.to_owned(),
                data,
            }));
        cx.spawn(async move |this, cx| match saved.await {
            Ok(saved) => {
                let _ = this.update(cx, |pane, cx| {
                    pane.type_text(&typed_paths([saved.path.as_str()]), cx)
                });
            }
            Err(error) => tracing::warn!(%session_id, "attachment.save failed: {error:#}"),
        })
        .detach();
    }

    /// Types `text` as one paste into a live session (dropped or pasted paths, §20B.5).
    pub fn type_text(&mut self, text: &str, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() || text.is_empty() {
            return;
        }
        let data = paste_bytes(text, self.emulator.input_mode());
        self.input(data, cx);
    }

    /// Sends typed or pasted bytes; a scrolled-back view returns to the live screen first.
    /// Sends `data` to the session. Anything sent (keys, paste, dropped paths, wheel arrows)
    /// clears the selection, so a later Ctrl+C interrupts again (ADR 0022).
    fn input(&mut self, data: Vec<u8>, cx: &mut Context<'_, Self>) {
        self.emulator.clear_selection();
        if self.emulator.display_offset() > 0 {
            self.emulator.scroll_to_bottom();
        }
        cx.notify();
        let request = Request::SessionInput(SessionInputParams {
            session_id: self.session_id,
            data,
        });
        self.request(request, "session.input", cx);
    }
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        if theme != self.theme {
            self.rows.borrow_mut().clear();
            self.theme = theme;
        }
        let exited = self.exited.map(|code| {
            let text = match code {
                Some(code) => format!("✕ Session exited (code {code})."),
                None => "✕ Session exited.".to_owned(),
            };
            div()
                .px(px(space::M))
                .py(px(space::XS))
                .bg(rgb(self.theme.surface))
                .font_family(self.theme.ui_font_family.clone())
                .text_size(font_size::META)
                .text_color(rgb(self.theme.text_muted))
                .child(text)
        });
        let find_bar = self.render_find_bar(window, cx);
        div()
            .size_full()
            .relative()
            .id(SharedString::from(self.session_id.to_string()))
            .flex()
            .flex_col()
            .key_context(TERMINAL_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::run))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .cursor(if self.hover_link.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            })
            // The pointer leaving the pane drops the link (and its hand pointer).
            .on_hover(cx.listener(|pane, hovered: &bool, _, cx| {
                if !*hovered {
                    pane.clear_link(cx);
                }
            }))
            // Pressing or releasing Ctrl / Cmd with the pointer at rest: only the focused pane
            // gets this; the others update when the pointer moves.
            .on_modifiers_changed(
                cx.listener(|pane, event: &ModifiersChangedEvent, window, cx| {
                    pane.update_link(event.modifiers.secondary(), window.mouse_position(), cx);
                }),
            )
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .child(TerminalElement {
                pane: cx.entity(),
                theme: self.theme.clone(),
                rows: self.rows.clone(),
                painted: self.painted.clone(),
            })
            .children(exited)
            .children(find_bar)
    }
}

/// A row ready to paint: shaped segments at their start column, plus background rects.
struct ShapedRow {
    segments: Vec<(usize, ShapedLine)>,
    backgrounds: Vec<Background>,
}

struct TerminalElement {
    pane: Entity<TerminalPane>,
    theme: Theme,
    rows: Rc<RefCell<RowCache<ShapedRow>>>,
    painted: async_channel::Sender<()>,
}

/// The grid at the pane's final size (read after fitting) and the cell size measured once per
/// frame.
struct Prepainted {
    grid: std::sync::Arc<Grid>,
    cell_width: Pixels,
    line_height: Pixels,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Fills the pane's column; a footer below it (exit line) keeps its own height.
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.flex_grow = 1.;
        style.flex_shrink = 1.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepainted {
        let (cell_width, line_height) = cell_size(window, &self.theme);
        let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let geometry = Geometry {
            origin: bounds.origin
                + point(
                    px(self.theme.terminal_padding),
                    px(self.theme.terminal_padding),
                ),
            cell_width,
            line_height,
        };
        self.pane
            .update(cx, |pane, _| pane.geometry = Some(geometry));
        let padding = 2.0 * self.theme.terminal_padding;
        // No room for even one cell (minimized window, collapsed layout): keep the session's
        // size instead of shrinking a live agent to the minimum grid.
        if width >= padding + f32::from(cell_width) && height >= padding + f32::from(line_height) {
            let (cols, rows) = grid_size(
                width,
                height,
                self.theme.terminal_padding,
                f32::from(cell_width),
                f32::from(line_height),
            );
            self.pane.update(cx, |pane, cx| pane.fit(cols, rows, cx));
        }
        Prepainted {
            grid: self.pane.read(cx).emulator.shared_grid(),
            cell_width,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        prepainted: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let theme = &self.theme;
        window.paint_quad(fill(bounds, hsla(theme.bg)));

        let text_system = window.text_system().clone();
        let base_font = terminal_font(theme);
        let font_size = px(theme.terminal_font_size);
        let Prepainted {
            grid,
            cell_width,
            line_height,
        } = prepainted;
        let (cell_width, line_height) = (*cell_width, *line_height);
        let padding = px(theme.terminal_padding);
        let origin = bounds.origin + point(padding, padding);

        let mut rows = self.rows.borrow_mut();
        rows.start_frame(grid.rows.len());
        let cursor = grid.cursor;
        for (index, cells) in grid.rows.iter().enumerate() {
            let cursor_col = (cursor.visible && cursor.row == index).then_some(cursor.col);
            let row = rows.row(index, cells, cursor_col, || {
                let layout = layout_row(cells, cursor_col, theme);
                let segments = layout
                    .segments
                    .into_iter()
                    .map(|segment| {
                        let runs: Vec<TextRun> = segment
                            .runs
                            .iter()
                            .map(|run| {
                                text_run(
                                    run.len,
                                    run.style,
                                    &base_font,
                                    theme.terminal_underline_thickness,
                                )
                            })
                            .collect();
                        let force_width = segment.grid_aligned.then_some(cell_width);
                        let line = text_system.shape_line(
                            SharedString::from(segment.text),
                            font_size,
                            &runs,
                            force_width,
                        );
                        (segment.start_col, line)
                    })
                    .collect();
                ShapedRow {
                    segments,
                    backgrounds: layout.backgrounds,
                }
            });

            let y = origin.y + line_height * index as f32;
            for bg in &row.backgrounds {
                let rect = Bounds::new(
                    point(origin.x + cell_width * bg.start_col as f32, y),
                    size(cell_width * bg.cols as f32, line_height),
                );
                window.paint_quad(fill(rect, hsla(bg.color)));
            }
            for (start_col, line) in &row.segments {
                let at = point(origin.x + cell_width * *start_col as f32, y);
                // A failed glyph paint only loses that glyph; keep painting the frame.
                let _ = line.paint(at, line_height, TextAlign::Left, None, window, cx);
            }
        }
        // The link under the pointer: one underline in the accent color, outside the row cache.
        if let Some(link) = &self.pane.read(cx).hover_link
            && link.row < grid.rows.len()
        {
            let thickness = px(theme.terminal_underline_thickness);
            let underline = Bounds::new(
                point(
                    origin.x + cell_width * link.start as f32,
                    origin.y + line_height * (link.row + 1) as f32 - thickness,
                ),
                size(cell_width * (link.end - link.start) as f32, thickness),
            );
            window.paint_quad(fill(underline, hsla(theme.accent)));
        }
        let pane = self.pane.read(cx);
        let (focus, marked) = (pane.focus_handle.clone(), pane.ime_marked.clone());
        let caret = Bounds::new(
            point(
                origin.x + cell_width * cursor.col as f32,
                origin.y + line_height * cursor.row as f32,
            ),
            size(cell_width, line_height),
        );
        let typing = crate::ime::Typing {
            view: self.pane.clone(),
            on_key: TerminalPane::key_down,
            marked,
            focus: focus.clone(),
            caret,
        };
        window.handle_input(&focus, typing, cx);
        // Full = a signal is already pending, which is just as good.
        let _ = self.painted.try_send(());
    }
}

/// Cell width and line height. Monospace: one precomputed cell size, no per-cell measurement
/// (SPEC §10).
pub fn cell_size(window: &Window, theme: &Theme) -> (Pixels, Pixels) {
    let text_system = window.text_system();
    let font_size = px(theme.terminal_font_size);
    let cell_width = text_system
        .advance(
            text_system.resolve_font(&terminal_font(theme)),
            font_size,
            'm',
        )
        .map(|advance| advance.width)
        .unwrap_or(font_size * CELL_WIDTH_ESTIMATE);
    (cell_width, font_size * theme.terminal_line_height)
}

/// Glyph fallbacks built once: the terminal paints every frame.
static GLYPH_FALLBACKS: std::sync::LazyLock<gpui::FontFallbacks> = std::sync::LazyLock::new(|| {
    gpui::FontFallbacks::from_fonts(
        theme::TERMINAL_GLYPH_FALLBACKS
            .iter()
            .map(|family| (*family).to_owned())
            .collect(),
    )
});

fn terminal_font(theme: &Theme) -> Font {
    let mut base = font(theme.terminal_font_family.clone());
    base.fallbacks = Some(GLYPH_FALLBACKS.clone());
    base
}

fn text_run(len: usize, style: CellStyle, base_font: &Font, underline_thickness: f32) -> TextRun {
    let mut font = base_font.clone();
    if style.bold {
        font.weight = FontWeight::BOLD;
    }
    if style.italic {
        font.style = FontStyle::Italic;
    }
    let color = hsla(style.fg);
    TextRun {
        len,
        font,
        color,
        background_color: None,
        underline: style.underline.then_some(UnderlineStyle {
            thickness: px(underline_thickness),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: None,
    }
}

fn hsla(color: u32) -> Hsla {
    rgb(color).into()
}
