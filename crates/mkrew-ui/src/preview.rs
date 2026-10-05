//! GPUI elements of the Docs view (SPEC §12, §14): the [`markdown`](crate::markdown) blocks as
//! the rendered preview, and the source view with line numbers. Only [`inline`] is pure (and
//! tested); the rest maps the model onto theme tokens.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, FontStyle, FontWeight, HighlightStyle, Hsla, MouseButton, MouseDownEvent,
    SharedString, StrikethroughStyle, StyledText, UnderlineStyle, UniformList, Window, div, px,
    rgb, uniform_list,
};

use crate::comments::Selection;
use crate::docs::{Shown, gutter};
use crate::markdown::{Align, Block, BlockKind, Item, Row, Run, Style};
use crate::theme::{self, Theme, font_size, radius, space, weight};

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
pub fn blocks(blocks: &[Block], theme: &Theme) -> Vec<AnyElement> {
    blocks
        .iter()
        .filter_map(|block| self::block(block, theme))
        .collect()
}

/// `None` for blocks without anything to show (a text-less task item's paragraph).
pub fn block(block: &Block, theme: &Theme) -> Option<AnyElement> {
    let element = match &block.kind {
        BlockKind::Heading { level, runs } => {
            let (size, weight) = match level {
                1 => (font_size::VIEW_TITLE, weight::SEMIBOLD),
                2 => (font_size::CARD_TITLE, weight::SEMIBOLD),
                _ => (font_size::BODY, weight::SEMIBOLD),
            };
            div()
                .pt(px(space::S))
                .text_size(px(size))
                .font_weight(FontWeight(weight))
                .text_color(rgb(theme.text))
                .child(text(runs, theme))
                .into_any_element()
        }
        BlockKind::Paragraph(runs) if runs.is_empty() => return None,
        BlockKind::Paragraph(runs) => div()
            .text_color(rgb(theme.text))
            .child(text(runs, theme))
            .into_any_element(),
        BlockKind::List { start, items } => div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .children(items.iter().enumerate().map(|(index, item)| {
                let number = start.map(|first| first + index as u64);
                list_item(item, number, theme)
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
                    .bg(rgb(theme.border)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .text_color(rgb(theme.text_muted))
                    .children(blocks(inner, theme)),
            )
            .into_any_element(),
        BlockKind::Code { text, .. } => code(text, theme.text, theme).into_any_element(),
        BlockKind::Html(html) => code(html, theme.text_muted, theme).into_any_element(),
        BlockKind::Table { aligns, head, rows } => div()
            .flex()
            .flex_col()
            .rounded(px(radius::MEDIUM))
            .border_1()
            .border_color(rgb(theme.border))
            .overflow_hidden()
            .child(
                table_row(head, aligns, theme)
                    .bg(rgb(theme.surface_raised))
                    .font_weight(FontWeight(weight::SEMIBOLD)),
            )
            .children(rows.iter().map(|row| {
                table_row(row, aligns, theme)
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
fn list_item(item: &Item, number: Option<u64>, theme: &Theme) -> gpui::Div {
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
            .text_size(px(font_size::META))
            .text_color(rgb(theme.surface))
            .when(checked, |el| el.child("✓")),
        (None, Some(number)) => div()
            .text_color(rgb(theme.text_muted))
            .child(format!("{number}.")),
        (None, None) => div().text_color(rgb(theme.text_muted)).child("•"),
    };
    div()
        .flex()
        .gap(px(space::S))
        .child(marker.flex_none())
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
                .children(blocks(&item.blocks, theme)),
        )
}

fn table_row(row: &Row, aligns: &[Align], theme: &Theme) -> gpui::Div {
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
                .child(text(cell, theme))
        }))
}

/// A verbatim block in the monospaced font on a raised surface.
fn code(text: &str, color: u32, theme: &Theme) -> gpui::Div {
    div()
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        .font_family(theme.terminal_font_family)
        .text_size(px(font_size::META))
        .text_color(rgb(color))
        .child(SharedString::from(text.to_owned()))
}

/// Inline runs as one text element: bold, italic, struck through, code spans on a raised
/// background, links in the accent color and underlined.
fn text(runs: &[Run], theme: &Theme) -> StyledText {
    let (text, styled) = inline(runs);
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
    StyledText::new(text).with_highlights(highlights)
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
    .font_family(theme.terminal_font_family)
    .text_size(px(theme.terminal_font_size))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
