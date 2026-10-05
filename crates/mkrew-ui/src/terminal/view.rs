//! GPUI terminal pane: renders one attached daemon session (SPEC §10, §14).
//!
//! Performance rules: one custom element paints the whole grid; each row is batched into a few
//! shaped text runs (`layout::layout_row`), shaped rows are cached and rebuilt only when dirty
//! (`layout::RowCache`). The owner feeds output with [`TerminalPane::feed`] and notifies once per
//! batch, then waits for the `painted` signal, so bursts coalesce into one repaint per frame.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Element, ElementId, Entity, FocusHandle, Font, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, KeyDownEvent, LayoutId, MouseButton, Pixels,
    ShapedLine, SharedString, Style, TextAlign, TextRun, UnderlineStyle, Window, WindowAppearance,
    div, fill, font, point, px, relative, rgb, size,
};
use mkrew_protocol::{Outcome, Request, ScreenSnapshot, SessionInputParams, SessionResizeParams};
use ulid::Ulid;

use crate::client::Connection;
use crate::terminal::emulator::{Emulator, Grid};
use crate::terminal::input::{is_paste_shortcut, key_to_bytes, paste_bytes};
use crate::terminal::layout::{Background, CellStyle, RowCache, grid_size, layout_row};
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

    /// Keys go to the session, except the terminal's own bindings (SPEC §13).
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() {
            return;
        }
        let mode = self.emulator.input_mode();
        let bytes = if is_paste_shortcut(&event.keystroke) {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| paste_bytes(&text, mode))
        } else {
            key_to_bytes(&event.keystroke, mode)
        };
        if let Some(data) = bytes {
            cx.stop_propagation();
            let request = Request::SessionInput(SessionInputParams {
                session_id: self.session_id,
                data,
            });
            self.request(request, "session.input", cx);
        }
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
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|pane, _, window, cx| window.focus(&pane.focus_handle, cx)),
            )
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
