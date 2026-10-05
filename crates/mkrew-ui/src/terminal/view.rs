//! GPUI terminal pane: renders one attached daemon session (SPEC §10, §14).
//!
//! Performance rules: one custom element paints the whole grid; each row is batched into a few
//! shaped text runs (`layout::layout_row`), shaped rows are cached and rebuilt only when dirty
//! (`layout::RowCache`). The owner feeds output with [`TerminalPane::feed`] and notifies once per
//! batch, then waits for the `painted` signal, so bursts coalesce into one repaint per frame.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::index::Side;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, Entity, FocusHandle,
    Font, FontStyle, FontWeight, GlobalElementId, Hsla, InspectorElementId, KeyDownEvent, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent,
    ShapedLine, SharedString, Style, TextAlign, TextRun, UnderlineStyle, Window, WindowAppearance,
    div, fill, font, point, px, relative, rgb, size,
};
use mkrew_protocol::{Outcome, Request, ScreenSnapshot, SessionInputParams, SessionResizeParams};
use ulid::Ulid;

use crate::client::Connection;
use crate::keymap::{Command, Run, TERMINAL_CONTEXT};
use crate::terminal::emulator::{Emulator, Grid};
use crate::terminal::input::{key_to_bytes, paste_bytes, wheel_bytes};
use crate::terminal::layout::{Background, CellStyle, RowCache, cell_at, grid_size, layout_row};
use crate::theme::{self, Theme, font_size, space};

/// Cell width guess used only until the terminal font is resolved.
const CELL_WIDTH_ESTIMATE: f32 = 0.6;

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
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let focus_handle = cx.focus_handle();
        Self {
            session_id,
            connection,
            emulator: Emulator::from_snapshot(snapshot),
            theme: theme_for(window.appearance()),
            rows: Rc::default(),
            focus_handle,
            painted,
            exited: None,
            geometry: None,
            selecting: false,
            scroll_remainder: 0.0,
        }
    }

    pub fn session_id(&self) -> Ulid {
        self.session_id
    }

    /// Sends keyboard input to this pane.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    /// Feeds output bytes. The caller notifies once per batch (≤ one repaint per frame).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.emulator.advance(bytes);
    }

    /// Replaces the screen with a fresh daemon snapshot (`session.snapshot` after the output
    /// queue overflowed). The caller notifies.
    pub fn load_snapshot(&mut self, snapshot: &ScreenSnapshot) {
        self.emulator = Emulator::from_snapshot(snapshot);
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
    /// action handler stops them first.
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() {
            return;
        }
        if let Some(data) = key_to_bytes(&event.keystroke, self.emulator.input_mode()) {
            cx.stop_propagation();
            self.input(data, cx);
        }
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

    fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
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
            cx.notify();
        }
    }

    /// The terminal's own commands; all others go on to the window shell.
    fn run(&mut self, Run(command): &Run, _window: &mut Window, cx: &mut Context<'_, Self>) {
        match command {
            Command::Copy => {
                if let Some(text) = self.emulator.selection_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.emulator.clear_selection();
                    cx.notify();
                }
            }
            Command::PageUp | Command::PageDown => {
                self.emulator.scroll_page(*command == Command::PageUp);
                cx.notify();
            }
            Command::Paste => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                if let Some(text) = text
                    && self.exited.is_none()
                {
                    let data = paste_bytes(&text, self.emulator.input_mode());
                    self.input(data, cx);
                }
            }
            _ => cx.propagate(),
        }
    }

    /// Sends typed or pasted bytes; a scrolled-back view returns to the live screen first.
    fn input(&mut self, data: Vec<u8>, cx: &mut Context<'_, Self>) {
        if self.emulator.display_offset() > 0 {
            self.emulator.scroll_to_bottom();
            cx.notify();
        }
        let request = Request::SessionInput(SessionInputParams {
            session_id: self.session_id,
            data,
        });
        self.request(request, "session.input", cx);
    }
}

fn theme_for(appearance: WindowAppearance) -> Theme {
    Theme::for_appearance(matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    ))
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = theme_for(window.appearance());
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
                .font_family(theme::UI_FONT)
                .text_size(px(font_size::META))
                .text_color(rgb(self.theme.text_muted))
                .child(text)
        });
        div()
            .size_full()
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
            .cursor(CursorStyle::IBeam)
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .child(TerminalElement {
                pane: cx.entity(),
                theme: self.theme.clone(),
                rows: self.rows.clone(),
                painted: self.painted.clone(),
            })
            .children(exited)
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
    grid: Grid,
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
            grid: self.pane.read(cx).emulator.grid(),
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
        rows.truncate(grid.rows.len());
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
        // Full = a signal is already pending, which is just as good.
        let _ = self.painted.try_send(());
    }
}

/// Cell width and line height. Monospace: one precomputed cell size, no per-cell measurement
/// (SPEC §10).
fn cell_size(window: &Window, theme: &Theme) -> (Pixels, Pixels) {
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

fn terminal_font(theme: &Theme) -> Font {
    let mut base = font(theme.terminal_font_family);
    base.fallbacks = Some(gpui::FontFallbacks::from_fonts(vec![
        theme.terminal_font_fallback.to_string(),
    ]));
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
