//! GPUI elements of the Docs view (SPEC §12, §14): the [`markdown`](crate::markdown) blocks as
//! the rendered preview (a virtual list of blocks with a scroll bar, [`page`]), and the source
//! view with line numbers; and the Files viewer's
//! highlighted code (SPEC §12A). Only [`inline`] and [`token_color`] are pure (and tested);
//! the rest maps the model onto theme tokens.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, CursorStyle, Div, FontStyle, FontWeight, HighlightStyle, Hsla,
    ListAlignment, ListState, MouseButton, MouseDownEvent, Pixels, SharedString,
    StrikethroughStyle, StyledText, TextLayout, UnderlineStyle, UniformList, Window, canvas, div,
    img, list, point, px, relative, rgb, uniform_list,
};

use crate::comments::Selection;
use crate::diagram::Diagram;
use crate::docs::{Shown, gutter};
use crate::highlight::{Code, Token};
use crate::markdown::{Align, Block, BlockKind, Item, Row, Run, Style};
use crate::preview_select;
use crate::scrollbar::{self, Source};
use crate::theme::{self, ActiveTheme, Theme, font_size, radius, space, weight};

/// The text of `runs` joined, and the byte range, style and "is a link" of every run that is
/// not plain text.
pub fn inline(runs: &[Run]) -> (String, Vec<(Range<usize>, Style, bool)>) {
    let mut text = String::new();
    let mut styled = Vec::new();
    for run in runs {
        let start = text.len();
        text.push_str(&run.text);
        if run.style != Style::default() || run.link.is_some() {
            styled.push((start..text.len(), run.style, run.link.is_some()));
        }
    }
    (text, styled)
}

/// The rendered preview of `blocks`, top to bottom.
pub fn blocks(blocks: &[Block], shown: &Shown, theme: &Theme) -> Vec<AnyElement> {
    blocks_in(blocks, shown, None, theme)
}

fn blocks_in(
    blocks: &[Block],
    shown: &Shown,
    pick: Option<&Pick>,
    theme: &Theme,
) -> Vec<AnyElement> {
    blocks
        .iter()
        .filter_map(|block| block_in(block, shown, pick, theme))
        .collect()
}

/// The preview's virtual list and the page it shows; one per view (Docs, Files), kept across
/// frames.
#[derive(Debug)]
pub struct Page {
    state: ListState,
    drag: scrollbar::Drag,
    /// The opening (`docs::new_opening`) and the page last shown.
    shown: RefCell<Option<(u64, Rc<Shown>)>>,
    /// The fonts the blocks were measured with: their heights follow the rem size and the
    /// terminal font (code blocks).
    fonts: RefCell<Option<theme::Fonts>>,
    /// Where the selectable pieces were painted last frame.
    hits: Hits,
}

impl Default for Page {
    fn default() -> Self {
        Page {
            // Every block measured once per page: the scroll bar is exact, and only the blocks
            // on screen are laid out per frame.
            state: ListState::new(0, ListAlignment::Top, Pixels::ZERO).measure_all(),
            drag: scrollbar::Drag::default(),
            shown: RefCell::default(),
            fonts: RefCell::default(),
            hits: Hits::default(),
        }
    }
}

impl Page {
    /// Points the list at `shown`, the page of `opening` (one opening of a file in a view).
    /// The same page again changes nothing. A new page of the same opening (re-read, edited)
    /// keeps the scroll position, clamped to its blocks. Another opening starts at the top.
    /// Returns whether the list was reset.
    pub fn show(&self, opening: u64, shown: &Rc<Shown>) -> bool {
        let mut current = self.shown.borrow_mut();
        let same_opening = match current.as_ref() {
            Some((at, page)) if *at == opening && Rc::ptr_eq(page, shown) => return false,
            Some((at, _)) => *at == opening,
            None => false,
        };
        let at = self.state.logical_scroll_top();
        self.state.reset(shown.blocks.len());
        if same_opening {
            self.state.scroll_to(at);
        }
        *current = Some((opening, Rc::clone(shown)));
        true
    }

    /// Measures every block again (keeping the place) when `fonts` differ from the ones the
    /// page was measured with: a live `[ui] font_size` or terminal font change (ADR 0024).
    /// Returns whether it did.
    pub fn fit(&self, fonts: &theme::Fonts) -> bool {
        let mut last = self.fonts.borrow_mut();
        let changed = last.as_ref().is_some_and(|last| last != fonts);
        if last.as_ref() != Some(fonts) {
            *last = Some(fonts.clone());
        }
        if changed {
            self.state.remeasure();
        }
        changed
    }

    /// The text point under window position `at`, or the nearest painted one; `None` when no
    /// text is painted.
    pub fn hit(&self, at: gpui::Point<Pixels>) -> Option<preview_select::Point> {
        let hits = self.hits.0.borrow();
        let key = nearest(hits.iter().map(|(key, layout)| (*key, layout.bounds())), at)?;
        let byte = closest_byte(hits.get(&key)?, at);
        Some(preview_select::Point {
            block: key.0,
            piece: key.1,
            byte,
        })
    }

    /// Measures every block again: one block changed height (the block edit field).
    pub fn remeasure(&self) {
        self.state.remeasure();
    }
}

/// Wraps the element of top-level block `index` (the Docs view makes blocks pickable).
pub type Wrap = Rc<dyn Fn(usize, &Block, AnyElement) -> AnyElement>;

/// Makes `page`'s text selectable, with this selection drawn (Files preview).
#[derive(Debug, Clone, Copy)]
pub struct Selecting(pub Option<preview_select::Selection>);

/// The layouts of the pieces painted last frame, by (top-level block, piece); filled from
/// `canvas` paint callbacks, so every layout in it is prepainted (its methods panic before).
#[derive(Clone, Default)]
pub struct Hits(Rc<RefCell<HashMap<(usize, usize), TextLayout>>>);

impl std::fmt::Debug for Hits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hits({})", self.0.borrow().len())
    }
}

/// Drawing one top-level block of a selectable page: the next piece's number, the selection,
/// where painted pieces are recorded, the selection color.
struct Pick {
    block: usize,
    next: Cell<usize>,
    selection: Option<preview_select::Selection>,
    hits: Hits,
    bg: Hsla,
}

/// A piece of text: `build` gets its selected bytes (end may be `usize::MAX`) and the selection
/// color. On a selectable page it is numbered, shows an I-beam, and its layout is recorded once
/// painted (a size-0 canvas after it, so it takes no room).
fn selectable(
    pick: Option<&Pick>,
    build: impl FnOnce(Option<Range<usize>>, Hsla) -> StyledText,
) -> AnyElement {
    let Some(pick) = pick else {
        return build(None, Hsla::default()).into_any_element();
    };
    let piece = pick.next.get();
    pick.next.set(piece + 1);
    let selected = pick.selection.and_then(|s| s.range_in(pick.block, piece));
    let styled = build(selected, pick.bg);
    let layout = styled.layout().clone();
    let (hits, key) = (pick.hits.clone(), (pick.block, piece));
    div()
        .cursor(CursorStyle::IBeam)
        .child(styled)
        .child(
            canvas(
                |_, _, _| (),
                move |_, (), _, _| {
                    hits.0.borrow_mut().insert(key, layout.clone());
                },
            )
            .absolute()
            .size_0(),
        )
        .into_any_element()
}

/// `highlights` (sorted, not overlapping) with the selection `sel` drawn on `bg`, in `text`:
/// highlights are cut at the selection's ends, their selected parts get `bg` (over a code
/// span's own background), selected plain text gets `bg` alone. `sel` may end past the text
/// or inside a character (a stale page): both ends snap down to a character boundary.
/// Hand-made because `gpui::combine_highlights` folds overlaps in `HashSet` order.
pub fn with_selection(
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    sel: Option<Range<usize>>,
    text: &str,
    bg: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let snap = |mut at: usize| {
        at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let Some(sel) = sel
        .map(|sel| snap(sel.start)..snap(sel.end))
        .filter(|sel| !sel.is_empty())
    else {
        return highlights;
    };
    let lit = |style: HighlightStyle| HighlightStyle {
        background_color: Some(bg),
        ..style
    };
    let mut out = Vec::with_capacity(highlights.len() + 3);
    let mut at = 0;
    for (range, style) in highlights {
        let gap = at.max(sel.start)..range.start.min(sel.end);
        if gap.start < gap.end {
            out.push((gap, lit(HighlightStyle::default())));
        }
        let cuts = [
            range.start,
            sel.start.clamp(range.start, range.end),
            sel.end.clamp(range.start, range.end),
            range.end,
        ];
        for pair in cuts.windows(2).filter(|pair| pair[0] < pair[1]) {
            let inside = pair[0] >= sel.start && pair[1] <= sel.end;
            out.push((pair[0]..pair[1], if inside { lit(style) } else { style }));
        }
        at = range.end;
    }
    let tail = at.max(sel.start)..sel.end;
    if tail.start < tail.end {
        out.push((tail, lit(HighlightStyle::default())));
    }
    out
}

/// The key of the rectangle nearest to `at`: by vertical distance (0 inside its rows), then
/// horizontal, then key order.
pub fn nearest<K: Copy + Ord>(
    rects: impl IntoIterator<Item = (K, Bounds<Pixels>)>,
    at: gpui::Point<Pixels>,
) -> Option<K> {
    let gap = |from: Pixels, to: Pixels, x: Pixels| {
        let (from, to, x) = (f32::from(from), f32::from(to), f32::from(x));
        (from - x).max(x - to).max(0.0)
    };
    rects
        .into_iter()
        .map(|(key, b)| {
            (
                (
                    gap(b.top(), b.bottom(), at.y),
                    gap(b.left(), b.right(), at.x),
                ),
                key,
            )
        })
        .min_by(|(a, ka), (b, kb)| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(ka.cmp(kb))
        })
        .map(|(_, key)| key)
}

/// The byte of `layout`'s text nearest to window point `position`: the line under it (the
/// first / last above / below the text), the char boundary closest along it.
pub fn closest_byte(layout: &TextLayout, position: gpui::Point<Pixels>) -> usize {
    let (bounds, line_height) = (layout.bounds(), layout.line_height());
    let lines = layout.line_layouts();
    let (mut top, mut start) = (bounds.top(), 0);
    for (i, line) in lines.iter().enumerate() {
        let height = line.size(line_height).height;
        if position.y < top + height || i + 1 == lines.len() {
            // Inside the line's rows: a y past its last row would find no row at all.
            let last_row = (height - px(theme::BORDER_WIDTH)).max(Pixels::ZERO);
            let y = (position.y - top).clamp(Pixels::ZERO, last_row);
            let within = point(position.x - bounds.left(), y);
            return start
                + line
                    .closest_index_for_position(within, line_height)
                    .unwrap_or_else(|i| i);
        }
        top += height;
        start += line.len() + 1;
    }
    0
}

/// The preview's page (ADR 0021): `shown`'s blocks in a centered column of at most
/// `DOC_PREVIEW_MAX_WIDTH`, in a virtual list (only the blocks on screen are laid out), with a
/// scroll bar on the right. `opening` names this opening of the file, so another one starts at
/// the top; `fonts` are the current ones (block heights follow them).
pub fn page(
    page: &Page,
    opening: u64,
    shown: &Rc<Shown>,
    wrap: Option<Wrap>,
    select: Option<Selecting>,
    fonts: &theme::Fonts,
    theme: &Theme,
) -> Div {
    page.show(opening, shown);
    page.fit(fonts);
    if select.is_some() {
        page.hits.0.borrow_mut().clear();
    }
    let (hits, bg) = (
        page.hits.clone(),
        Hsla::from(rgb(theme.terminal_selection_bg)),
    );
    let shown = Rc::clone(shown);
    let last = shown.blocks.len().saturating_sub(1);
    let blocks = list(page.state.clone(), move |index, _, cx| {
        let item_theme = cx.theme();
        let Some(block) = shown.blocks.get(index) else {
            return div().into_any_element();
        };
        let pick = select.map(|Selecting(selection)| Pick {
            block: index,
            next: Cell::new(0),
            selection,
            hits: hits.clone(),
            bg,
        });
        let element = block_in(block, &shown, pick.as_ref(), item_theme);
        if let Some(pick) = &pick {
            debug_assert_eq!(
                pick.next.get(),
                preview_select::piece_count(block, &shown),
                "preview_select::flat and preview::block disagree on block {index}'s pieces"
            );
        }
        let Some(element) = element else {
            return div().into_any_element();
        };
        let element = match &wrap {
            Some(wrap) => wrap(index, block, element),
            None => element,
        };
        div()
            .w_full()
            .px(px(space::XL))
            .when(index == 0, |el| el.pt(px(space::L)))
            .pb(px(if index == last { space::L } else { space::M }))
            .child(
                div()
                    .w_full()
                    .max_w(px(theme::DOC_PREVIEW_MAX_WIDTH))
                    .mx_auto()
                    .text_size(px(theme::doc::BODY))
                    .line_height(relative(theme::doc::LINE_HEIGHT))
                    .child(element),
            )
            .into_any_element()
    })
    .size_full();
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(blocks)
        .child(scrollbar::bar(
            Source::List(page.state.clone()),
            Rc::clone(&page.drag),
            theme,
        ))
}

/// `None` for blocks without anything to show (a text-less task item's paragraph).
pub fn block(block: &Block, shown: &Shown, theme: &Theme) -> Option<AnyElement> {
    block_in(block, shown, None, theme)
}

fn block_in(
    block: &Block,
    shown: &Shown,
    pick: Option<&Pick>,
    theme: &Theme,
) -> Option<AnyElement> {
    let element = match &block.kind {
        BlockKind::Heading { level, runs } => {
            let size = (theme::doc::HEADINGS.get(usize::from(*level).saturating_sub(1)))
                .copied()
                .unwrap_or(theme::doc::BODY);
            let major = *level <= 2;
            div()
                .pt(px(if major { space::L } else { space::S }))
                .text_size(px(size))
                .line_height(relative(theme::doc::HEADING_LINE_HEIGHT))
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .when(major, |el| {
                    el.pb(px(space::XS))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                })
                .child(selectable(pick, |sel, bg| text(runs, theme, sel, bg)))
                .into_any_element()
        }
        BlockKind::Paragraph(runs) if runs.is_empty() => return None,
        BlockKind::Paragraph(runs) => div()
            .text_color(rgb(theme.text))
            .child(selectable(pick, |sel, bg| text(runs, theme, sel, bg)))
            .into_any_element(),
        BlockKind::List { start, items } => div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .children(items.iter().enumerate().map(|(index, item)| {
                let number = start.map(|first| first + index as u64);
                list_item(item, number, shown, pick, theme)
            }))
            .into_any_element(),
        BlockKind::Quote(inner) => div()
            .flex()
            .gap(px(space::M))
            .child(
                div()
                    .flex_none()
                    .w(px(theme::DOC_QUOTE_BAR_WIDTH))
                    .rounded(px(radius::SMALL))
                    .bg(rgb(theme.pane_focus_border)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .text_color(rgb(theme.text))
                    .children(blocks_in(inner, shown, pick, theme)),
            )
            .into_any_element(),
        BlockKind::Code { lang, text } => {
            let start = block.lines.start;
            let source = || match shown.code.get(&start) {
                Some(highlighted) => code_block(lang.as_deref(), highlighted, pick, theme),
                None => code(text, theme.text, theme),
            };
            match shown.diagrams.get(&start) {
                Some(Ok(diagram)) => diagram_view(diagram, theme).into_any_element(),
                Some(Err(error)) => div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(format!("Cannot draw this diagram: {error}")),
                    )
                    .child(source())
                    .into_any_element(),
                None => source().into_any_element(),
            }
        }
        BlockKind::Html(html) => code(html, theme.text_muted, theme).into_any_element(),
        BlockKind::Table { aligns, head, rows } => div()
            .flex()
            .flex_col()
            .rounded(px(radius::MEDIUM))
            .border_1()
            .border_color(rgb(theme.border))
            .overflow_hidden()
            .child(
                table_row(head, aligns, pick, theme)
                    .bg(rgb(theme.surface_raised))
                    .font_weight(FontWeight(weight::SEMIBOLD)),
            )
            .children(rows.iter().map(|row| {
                table_row(row, aligns, pick, theme)
                    .border_t_1()
                    .border_color(rgb(theme.border))
            }))
            .into_any_element(),
        BlockKind::Rule => div()
            .my(px(space::S))
            .h(px(theme::BORDER_WIDTH))
            .bg(rgb(theme.border))
            .into_any_element(),
    };
    Some(element)
}

/// A bullet, number or check box, then the item's blocks.
fn list_item(
    item: &Item,
    number: Option<u64>,
    shown: &Shown,
    pick: Option<&Pick>,
    theme: &Theme,
) -> gpui::Div {
    let marker = match (item.checked, number) {
        (Some(checked), _) => div()
            .mt(px(space::XS))
            .size(px(theme::DOC_TASK_BOX_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::SMALL))
            .border_1()
            .border_color(rgb(if checked {
                theme.accent
            } else {
                theme.text_muted
            }))
            .when(checked, |el| el.bg(rgb(theme.accent)))
            .text_size(font_size::META)
            .text_color(rgb(theme.on_accent))
            .when(checked, |el| el.child("✓")),
        (None, Some(number)) => div()
            .text_color(rgb(theme.text_muted))
            .child(format!("{number}.")),
        (None, None) => div().text_color(rgb(theme.text_muted)).child("•"),
    };
    div()
        .flex()
        .gap(px(space::S))
        .child(
            div()
                .flex_none()
                .w(px(theme::doc::MARKER_WIDTH))
                .flex()
                .justify_end()
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .when(item.checked == Some(true), |el| {
                    el.text_color(rgb(theme.text_muted))
                })
                .children(blocks_in(&item.blocks, shown, pick, theme)),
        )
}

fn table_row(row: &Row, aligns: &[Align], pick: Option<&Pick>, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .children(row.cells.iter().enumerate().map(|(column, cell)| {
            let align = aligns.get(column).copied().unwrap_or(Align::Default);
            div()
                .flex_1()
                .min_w_0()
                .px(px(space::M))
                .py(px(space::S))
                .text_color(rgb(theme.text))
                .map(|el| match align {
                    Align::Center => el.text_center(),
                    Align::Right => el.text_right(),
                    Align::Default | Align::Left => el,
                })
                .child(selectable(pick, |sel, bg| text(cell, theme, sel, bg)))
        }))
}

/// A verbatim block in the monospaced font on a raised surface.
fn code(text: &str, color: u32, theme: &Theme) -> gpui::Div {
    div()
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(SharedString::from(text.to_owned()))
}

/// A fenced code block: its language muted above, then its lines with syntax colors in the
/// monospaced font on a raised surface (long lines wrap).
fn code_block(lang: Option<&str>, code: &Code, pick: Option<&Pick>, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        .children(lang.map(|lang| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(lang.to_owned())
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .font_family(theme.terminal_font_family.clone())
                .text_size(px(theme::doc::CODE))
                .line_height(relative(theme::doc::CODE_LINE_HEIGHT))
                .text_color(rgb(theme.text))
                .children(code.lines.iter().zip(&code.spans).map(|(line, spans)| {
                    // An empty line still takes a line's height.
                    let line = if line.is_empty() {
                        " ".to_owned()
                    } else {
                        line.clone()
                    };
                    div().child(selectable(pick, |sel, bg| {
                        let highlights =
                            with_selection(code_highlights(spans, &[], theme), sel, &line, bg);
                        StyledText::new(line).with_highlights(highlights)
                    }))
                })),
        )
}

/// A drawn Mermaid diagram on a bordered card, at its natural size, at most as wide as the
/// preview.
fn diagram_view(diagram: &Diagram, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .justify_center()
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        // The image decodes after its first layout: the known ratio gives it its height at
        // once, so measured page heights (and the scroll bar) do not jump when it appears.
        .child(
            img(diagram.image(theme))
                .w(px(diagram.width))
                .max_w_full()
                .when(diagram.height > 0.0, |img| {
                    img.aspect_ratio(diagram.width / diagram.height)
                }),
        )
}

/// Inline runs as one text element: bold, italic, struck through, code spans on a raised
/// background, links in the accent color and underlined.
fn text(runs: &[Run], theme: &Theme, selected: Option<Range<usize>>, bg: Hsla) -> StyledText {
    let (text, styled) = inline(runs);
    let mono: Vec<(Range<usize>, SharedString)> = styled
        .iter()
        .filter(|(_, style, _)| style.code)
        .map(|(range, _, _)| (range.clone(), theme.terminal_font_family.clone()))
        .collect();
    let accent = Hsla::from(rgb(theme.accent));
    let code_bg = Hsla::from(rgb(theme.surface_raised));
    let highlights = styled.into_iter().map(move |(range, style, link)| {
        let highlight = HighlightStyle {
            color: link.then_some(accent),
            font_weight: style.bold.then_some(FontWeight(weight::SEMIBOLD)),
            font_style: style.italic.then_some(FontStyle::Italic),
            background_color: style.code.then_some(code_bg),
            underline: link.then_some(UnderlineStyle {
                thickness: px(theme::BORDER_WIDTH),
                color: Some(accent),
                wavy: false,
            }),
            strikethrough: style.strike.then_some(StrikethroughStyle {
                thickness: px(theme::BORDER_WIDTH),
                color: None,
            }),
            fade_out: None,
        };
        (range, highlight)
    });
    let highlights = with_selection(highlights.collect(), selected, &text, bg);
    StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono)
}

/// Called with a clicked source line (1-based) and whether Shift was held.
pub type OnLine = Rc<dyn Fn(u32, bool, &mut Window, &mut App)>;

/// The file's lines with right-aligned line numbers in the monospaced font; only the rows on
/// screen are laid out. The `selected` lines are highlighted; a click picks a line.
pub fn source(
    id: &'static str,
    shown: Rc<Shown>,
    selected: Option<Selection>,
    on_line: OnLine,
    theme: &Theme,
) -> UniformList {
    let total = shown.lines.len();
    let (text, muted, selection_bg) = (theme.text, theme.text_muted, theme.terminal_selection_bg);
    let line_height = theme.terminal_font_size * theme.terminal_line_height;
    uniform_list(id, total, move |range, _, _| {
        range
            .map(|index| {
                let line = shown.lines.get(index).map(String::as_str).unwrap_or("");
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let on_line = Rc::clone(&on_line);
                div()
                    .w_full()
                    .h(px(line_height))
                    .flex()
                    .gap(px(space::L))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(selected.is_some_and(|s| s.contains(number, number)), |el| {
                        el.bg(rgb(selection_bg))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event: &MouseDownEvent, window, cx| {
                            on_line(number, event.modifiers.shift, window, cx);
                        },
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(muted))
                            .child(gutter(index + 1, total)),
                    )
                    .child(
                        div()
                            .text_color(rgb(text))
                            .child(SharedString::from(line.to_owned())),
                    )
            })
            .collect()
    })
    .font_family(theme.terminal_font_family.clone())
    .text_size(px(theme.terminal_font_size))
}

/// The theme color of a highlighted token (SPEC §14).
pub fn token_color(token: Token, theme: &Theme) -> u32 {
    match token {
        Token::Comment => theme.text_muted,
        Token::String => theme.syntax_string,
        Token::Number => theme.syntax_number,
        Token::Keyword => theme.syntax_keyword,
        Token::Type => theme.syntax_type,
        Token::Function => theme.syntax_function,
    }
}

/// How a piece of a code line is drawn: in a token color, or as a find match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Token(Token),
    Match { current: bool },
}

/// The non-overlapping pieces of a line to draw: find matches (`found`, with "is current")
/// cover the syntax `spans`; text in neither keeps the parent's color. Both inputs are sorted
/// and non-overlapping, so one pass over the cuts is enough (minified lines hold thousands).
pub fn paint(
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
) -> Vec<(Range<usize>, Paint)> {
    if found.is_empty() {
        return spans
            .iter()
            .map(|(range, token)| (range.clone(), Paint::Token(*token)))
            .collect();
    }
    let mut cuts: Vec<usize> = spans
        .iter()
        .map(|(range, _)| range)
        .chain(found.iter().map(|(range, _)| range))
        .flat_map(|range| [range.start, range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    // The first range of `ranges`, from `*at` on, that covers `piece`; `*at` only moves
    // forward because the pieces come left to right.
    fn covering<'a, T>(
        ranges: &'a [(Range<usize>, T)],
        at: &mut usize,
        piece: &Range<usize>,
    ) -> Option<&'a T> {
        while ranges
            .get(*at)
            .is_some_and(|(range, _)| range.end <= piece.start)
        {
            *at += 1;
        }
        ranges
            .get(*at)
            .filter(|(range, _)| range.start <= piece.start && piece.end <= range.end)
            .map(|(_, value)| value)
    }
    let (mut next_found, mut next_span) = (0, 0);
    cuts.windows(2)
        .filter_map(|pair| {
            let piece = pair[0]..pair[1];
            let matched = covering(found, &mut next_found, &piece)
                .map(|current| Paint::Match { current: *current });
            let token = covering(spans, &mut next_span, &piece).map(|token| Paint::Token(*token));
            matched.or(token).map(|paint| (piece, paint))
        })
        .collect()
}

/// The highlights of one code line: syntax colors, with find matches (`found`, with "is
/// current") on the match backgrounds in the text color.
fn code_highlights(
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
    theme: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    paint(spans, found)
        .into_iter()
        .map(|(range, paint)| {
            let style = match paint {
                Paint::Token(token) => HighlightStyle {
                    color: Some(Hsla::from(rgb(token_color(token, theme)))),
                    ..HighlightStyle::default()
                },
                Paint::Match { current } => HighlightStyle {
                    color: Some(Hsla::from(rgb(theme.text))),
                    background_color: Some(Hsla::from(rgb(if current {
                        theme.terminal_match_current_bg
                    } else {
                        theme.terminal_match_bg
                    }))),
                    ..HighlightStyle::default()
                },
            };
            (range, style)
        })
        .collect()
}

/// One line of code with its syntax `spans` colored and its find matches (`found`, with "is
/// current") on the match backgrounds in the text color; other text keeps the parent's color.
pub fn code_line(
    text: String,
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
    theme: &Theme,
) -> StyledText {
    StyledText::new(text).with_highlights(code_highlights(spans, found, theme))
}

/// What one row of the Files editor shows besides its text. Caret and selection are in screen
/// cells (tabs count as `TAB`); `found` is in bytes of the drawn text.
#[derive(Debug, Default)]
pub struct RowMarks {
    pub caret: Option<usize>,
    pub selected: Option<Range<usize>>,
    pub found: Vec<(Range<usize>, bool)>,
    /// The line's change against the git index (SPEC §12A gutter).
    pub change: Option<crate::gutter::Change>,
}

/// One row of the Files editor (SPEC §12A): its number, then `text` (tabs already spaces) in
/// the syntax colors of `spans`, with the find matches, over the selected cells, with the
/// caret. The caret's row is raised and its number accented. `text_left` learns where the text
/// starts on screen, so a click can be turned into a column.
#[allow(clippy::too_many_arguments)]
pub fn code_row(
    index: usize,
    total: usize,
    text: String,
    spans: &[(Range<usize>, Token)],
    marks: &RowMarks,
    cell_width: Pixels,
    text_left: &Rc<Cell<Pixels>>,
    theme: &Theme,
) -> Div {
    let line_height = theme.terminal_font_size * theme.terminal_line_height;
    let cells = move |n: usize| cell_width * n as f32;
    let current = marks.caret.is_some();
    let text_left = Rc::clone(text_left);
    div()
        .w_full()
        .h(px(line_height))
        .flex()
        .gap(px(space::L))
        .whitespace_nowrap()
        .cursor(CursorStyle::IBeam)
        // A raised band plus an accent number: syntax colors and muted comments stay
        // ≥ 4.5:1 on it.
        .when(current, |el| el.bg(rgb(theme.surface_raised)))
        .child(
            div()
                .flex_none()
                .flex()
                .gap(px(space::XS))
                .text_color(rgb(if current {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(gutter(index + 1, total))
                .child(change_bar(marks.change, theme)),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .text_color(rgb(theme.text))
                .child(
                    canvas(
                        move |bounds, _, _| text_left.set(bounds.left()),
                        |_, (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .children(marks.selected.clone().map(|selected| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(cells(selected.start))
                        .w(cells(selected.end - selected.start))
                        .bg(rgb(theme.terminal_selection_bg))
                }))
                .child(code_line(text, spans, &marks.found, theme))
                .children(marks.caret.map(|at| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(cells(at))
                        .w(px(theme::CARET_WIDTH))
                        .bg(rgb(theme.text))
                })),
        )
}

/// The change bar right of a line number (SPEC §12A): the whole row for an added or changed
/// line, a short mark at the top for lines removed above it. Always as wide, so the text
/// never moves.
fn change_bar(change: Option<crate::gutter::Change>, theme: &Theme) -> Div {
    use crate::gutter::Change;
    let bar = div().flex_none().w(px(theme::GIT_GUTTER_BAR_WIDTH));
    match change {
        None => bar,
        Some(Change::Added) => bar.h_full().bg(rgb(theme.diff_add_text)),
        Some(Change::Modified) => bar.h_full().bg(rgb(theme.diff_mod_text)),
        Some(Change::Deleted) => bar
            .h(px(theme::GIT_GUTTER_DELETED_HEIGHT))
            .bg(rgb(theme.diff_del_text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::ListOffset;

    fn page_of(text: &str) -> Rc<Shown> {
        Rc::new(Shown::from_text(text))
    }

    fn run(text: &str, style: Style, link: Option<&str>) -> Run {
        Run {
            text: text.into(),
            style,
            link: link.map(Into::into),
        }
    }

    #[test]
    fn inline_joins_runs_and_ranges_only_the_styled_ones() {
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let runs = [
            run("Use ", Style::default(), None),
            run("Rédis", bold, None),
            run(" — see ", Style::default(), None),
            run("docs", Style::default(), Some("https://x")),
        ];
        let (text, styled) = inline(&runs);
        assert_eq!(text, "Use Rédis — see docs");
        assert_eq!(
            styled,
            [
                (4..10, bold, false),
                (text.len() - 4..text.len(), Style::default(), true)
            ]
        );
        assert!(
            styled
                .iter()
                .all(|(range, _, _)| text.get(range.clone()).is_some())
        );
        assert_eq!(inline(&[]), (String::new(), vec![]));
    }

    #[test]
    fn tokens_take_theme_colors_and_comments_are_muted() {
        let theme = Theme::dark();
        assert_eq!(token_color(Token::Comment, &theme), theme.text_muted);
        assert_eq!(token_color(Token::Keyword, &theme), theme.syntax_keyword);
        assert_eq!(token_color(Token::Function, &theme), theme.syntax_function);
    }

    #[test]
    fn matches_paint_over_syntax_colors() {
        let spans = [(0..2, Token::Keyword), (3..8, Token::String)];
        let found = [(1..4, false), (6..7, true)];
        assert_eq!(
            paint(&spans, &found),
            [
                (0..1, Paint::Token(Token::Keyword)),
                (1..2, Paint::Match { current: false }),
                (2..3, Paint::Match { current: false }),
                (3..4, Paint::Match { current: false }),
                (4..6, Paint::Token(Token::String)),
                (6..7, Paint::Match { current: true }),
                (7..8, Paint::Token(Token::String)),
            ]
        );
        assert_eq!(
            paint(&spans, &[]),
            [
                (0..2, Paint::Token(Token::Keyword)),
                (3..8, Paint::Token(Token::String))
            ]
        );
    }

    /// Drawing the same page again (every frame) changes nothing. A new page of the same
    /// opening (re-read after an agent wrote it, or edited) keeps the reader's place, clamped
    /// when it got shorter. Another opening (another file, the same file opened again, the same
    /// path in another workspace) starts at the top.
    #[test]
    fn a_new_page_of_the_same_opening_keeps_its_place_and_another_opening_starts_at_the_top() {
        let page = Page::default();
        let long: String = (0..50).map(|i| format!("Paragraph {i}\n\n")).collect();
        let first = page_of(&long);
        assert!(page.show(1, &first));
        assert_eq!(page.state.item_count(), 50);
        page.state.scroll_to(ListOffset {
            item_ix: 30,
            offset_in_item: px(5.),
        });
        assert!(!page.show(1, &first), "the same page is not laid out again");
        assert!(page.show(1, &page_of(&long)));
        let top = page.state.logical_scroll_top();
        assert_eq!((top.item_ix, top.offset_in_item), (30, px(5.)));
        assert!(page.show(1, &page_of("# One\n\nTwo\n")));
        assert_eq!(page.state.item_count(), 2);
        assert_eq!(
            page.state.logical_scroll_top().item_ix,
            2,
            "clamped to the end"
        );
        page.state.scroll_to(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.),
        });
        // The same file opened again: a new opening, a new page.
        assert!(page.show(2, &page_of(&long)));
        assert_eq!(page.state.logical_scroll_top().item_ix, 0);
    }

    /// A live font change (`[ui] font_size`, the terminal font of code blocks) measures every
    /// block again and keeps the place; the same fonts again change nothing.
    #[test]
    fn other_fonts_measure_the_page_again_and_keep_its_place() {
        let page = Page::default();
        let long: String = (0..50).map(|i| format!("Paragraph {i}\n\n")).collect();
        page.show(1, &page_of(&long));
        let fonts = theme::Fonts {
            ui_family: "Inter".into(),
            ui_size: 13.0,
            terminal_family: "JetBrains Mono".into(),
            terminal_size: 13.0,
        };
        assert!(
            !page.fit(&fonts),
            "the first fonts are the ones it is measured with"
        );
        assert!(!page.fit(&fonts));
        page.state.scroll_to(ListOffset {
            item_ix: 20,
            offset_in_item: px(0.),
        });
        let larger = theme::Fonts {
            ui_size: 16.0,
            ..fonts.clone()
        };
        assert!(page.fit(&larger));
        assert_eq!(page.state.item_count(), 50);
        assert_eq!(page.state.logical_scroll_top().item_ix, 20);
        assert!(!page.fit(&larger));
        let code = theme::Fonts {
            terminal_size: 15.0,
            ..larger
        };
        assert!(page.fit(&code));
    }

    fn bold() -> HighlightStyle {
        HighlightStyle {
            font_weight: Some(FontWeight(weight::SEMIBOLD)),
            ..HighlightStyle::default()
        }
    }

    fn lit() -> Hsla {
        Hsla::from(rgb(0x334455))
    }

    #[test]
    fn selection_cuts_highlights_and_fills_plain_text() {
        let merged = with_selection(vec![(0..5, bold())], Some(3..8), "0123456789", lit());
        let on = |style: HighlightStyle| HighlightStyle {
            background_color: Some(lit()),
            ..style
        };
        assert_eq!(
            merged,
            vec![
                (0..3, bold()),
                (3..5, on(bold())),
                (5..8, on(HighlightStyle::default()))
            ]
        );
    }

    #[test]
    fn selection_covers_a_code_span_background_and_clamps_to_the_text() {
        let code = HighlightStyle {
            background_color: Some(Hsla::from(rgb(0x111111))),
            ..HighlightStyle::default()
        };
        let merged = with_selection(vec![(2..4, code)], Some(0..usize::MAX), "abcdef", lit());
        let backgrounds: Vec<_> = merged
            .iter()
            .map(|(r, s)| (r.clone(), s.background_color))
            .collect();
        assert_eq!(
            backgrounds,
            vec![
                (0..2, Some(lit())),
                (2..4, Some(lit())),
                (4..6, Some(lit()))
            ]
        );
    }

    #[test]
    fn no_selection_keeps_the_highlights() {
        assert_eq!(
            with_selection(vec![(0..5, bold())], None, "hello", lit()),
            vec![(0..5, bold())]
        );
        assert_eq!(
            with_selection(vec![(0..5, bold())], Some(9..12), "hello", lit()),
            vec![(0..5, bold())]
        );
    }

    #[test]
    fn selection_ends_inside_a_character_snap_down_to_a_boundary() {
        // "Việt": ệ is bytes 2..5.
        let ranges = |sel| {
            with_selection(vec![], Some(sel), "Việt", lit())
                .into_iter()
                .map(|(range, _)| range)
                .collect::<Vec<_>>()
        };
        assert_eq!(ranges(0..4), vec![0..2], "end inside ệ");
        assert_eq!(ranges(3..6), vec![2..6], "start inside ệ");
        assert_eq!(
            ranges(3..4),
            Vec::<Range<usize>>::new(),
            "empty once snapped"
        );
    }

    #[test]
    fn nearest_prefers_the_text_under_then_beside_the_pointer() {
        let rect =
            |y: f32| gpui::bounds(gpui::point(px(10.), px(y)), gpui::size(px(100.), px(20.)));
        let rects = [((0, 0), rect(0.)), ((1, 0), rect(40.)), ((2, 0), rect(80.))];
        assert_eq!(
            nearest(rects, gpui::point(px(50.), px(45.))),
            Some((1, 0)),
            "inside"
        );
        assert_eq!(
            nearest(rects, gpui::point(px(500.), px(85.))),
            Some((2, 0)),
            "right of it"
        );
        assert_eq!(
            nearest(rects, gpui::point(px(50.), px(28.))),
            Some((0, 0)),
            "gap: closer above"
        );
        assert_eq!(
            nearest(rects, gpui::point(px(50.), px(-30.))),
            Some((0, 0)),
            "above the page"
        );
        assert_eq!(
            nearest(
                Vec::<((usize, usize), _)>::new(),
                gpui::point(px(0.), px(0.))
            ),
            None
        );
    }
}
