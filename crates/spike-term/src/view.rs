//! GPUI window that renders the emulator grid (SPEC §10, §14).
//!
//! Performance rules: one custom element paints the whole grid; each row is batched into a few
//! shaped text runs (`layout::layout_row`), shaped rows are cached and rebuilt only when dirty
//! (`layout::RowCache`), and PTY output is drained once per frame tick so bursts coalesce into a
//! single repaint.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Element, ElementId, FocusHandle, Font, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, KeyDownEvent, LayoutId, Pixels, ShapedLine,
    SharedString, Style, TextAlign, TextRun, TitlebarOptions, UnderlineStyle, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, fill, font, point, px, relative, rgb, size,
};

use crate::emulator::{Emulator, Grid};
use crate::input::{is_paste_shortcut, key_to_bytes, paste_bytes};
use crate::layout::{Background, CellStyle, RowCache, grid_size, layout_row};
use crate::pty::PtySession;
use crate::theme::Theme;

/// How often queued PTY output is drained; about one display refresh.
const FRAME_TICK: Duration = Duration::from_millis(16);
/// Cell width guess used only to size the initial window, before a font is resolved.
const CELL_WIDTH_ESTIMATE: f32 = 0.6;

/// Opens the window and runs the app until the window closes or the child exits.
pub fn run(session: PtySession, emulator: Emulator) {
    gpui_platform::application().run(move |cx: &mut App| {
        let theme = Theme::dark();
        let grid = emulator.grid();
        let window_size = size(
            px(
                grid.cols as f32 * theme.terminal_font_size * CELL_WIDTH_ESTIMATE
                    + 2.0 * theme.terminal_padding,
            ),
            px(
                grid.rows.len() as f32 * theme.terminal_font_size * theme.terminal_line_height
                    + 2.0 * theme.terminal_padding,
            ),
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                window_size,
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("spike-term".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            cx.new(|cx| TerminalView::new(session, emulator, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("spike-term: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

struct TerminalView {
    session: PtySession,
    emulator: Emulator,
    theme: Theme,
    rows: Rc<RefCell<RowCache<ShapedRow>>>,
    focus_handle: FocusHandle,
}

impl TerminalView {
    fn new(
        session: PtySession,
        emulator: Emulator,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME_TICK).await;
                let alive = this.update(cx, |view, cx| view.drain(cx));
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            session,
            emulator,
            theme: theme_for(window.appearance()),
            rows: Rc::default(),
            focus_handle,
        }
    }

    /// Keys go to the PTY, except the terminal's own bindings (SPEC §13).
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        let mode = self.emulator.input_mode();
        let bytes = if is_paste_shortcut(&event.keystroke) {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| paste_bytes(&text, mode))
        } else {
            key_to_bytes(&event.keystroke, mode)
        };
        if let Some(bytes) = bytes {
            cx.stop_propagation();
            if let Err(error) = self.session.write(&bytes) {
                eprintln!("spike-term: pty write failed: {error:#}");
            }
        }
    }

    /// Fits the grid to the window; a changed size goes to both the emulator and the PTY.
    /// Uses the viewport because the spike's terminal fills the window (panes need own bounds).
    fn fit_to_window(&mut self, window: &Window) {
        let (cell_width, line_height) = cell_size(window, &self.theme);
        let viewport = window.viewport_size();
        let (cols, rows) = grid_size(
            f32::from(viewport.width),
            f32::from(viewport.height),
            self.theme.terminal_padding,
            f32::from(cell_width),
            f32::from(line_height),
        );
        if (usize::from(cols), usize::from(rows)) == self.emulator.size() {
            return;
        }
        self.emulator.resize(cols.into(), rows.into());
        if let Err(error) = self.session.resize(cols, rows) {
            eprintln!("spike-term: pty resize failed: {error:#}");
        }
    }

    /// Feeds queued output; one `notify` (= at most one repaint) per tick. Returns false when
    /// the session is over.
    fn drain(&mut self, cx: &mut Context<'_, Self>) -> bool {
        match self.session.drain(&mut self.emulator) {
            Ok(drained) => {
                if drained.output {
                    cx.notify();
                }
                if drained.exited.is_some() {
                    cx.quit();
                    return false;
                }
                true
            }
            Err(error) => {
                eprintln!("spike-term: pty error: {error:#}");
                cx.quit();
                false
            }
        }
    }
}

fn theme_for(appearance: WindowAppearance) -> Theme {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
        WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::light(),
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = theme_for(window.appearance());
        if theme != self.theme {
            self.rows.borrow_mut().clear();
            self.theme = theme;
        }
        self.fit_to_window(window);
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .child(TerminalElement {
                grid: self.emulator.grid(),
                theme: self.theme.clone(),
                rows: self.rows.clone(),
            })
    }
}

/// A row ready to paint: shaped segments at their start column, plus background rects.
struct ShapedRow {
    segments: Vec<(usize, ShapedLine)>,
    backgrounds: Vec<Background>,
}

struct TerminalElement {
    grid: Grid,
    theme: Theme,
    rows: Rc<RefCell<RowCache<ShapedRow>>>,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.flex_grow = 1.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let theme = &self.theme;
        window.paint_quad(fill(bounds, hsla(theme.bg)));

        let text_system = window.text_system().clone();
        let base_font = terminal_font(theme);
        let font_size = px(theme.terminal_font_size);
        let (cell_width, line_height) = cell_size(window, theme);
        let padding = px(theme.terminal_padding);
        let origin = bounds.origin + point(padding, padding);

        let mut rows = self.rows.borrow_mut();
        rows.truncate(self.grid.rows.len());
        let cursor = self.grid.cursor;
        for (index, cells) in self.grid.rows.iter().enumerate() {
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
