//! omp's `picker` sheet (TSP 19 `TspPickerProps`, ADR 0119) drawn like Tern's: the head with
//! the search omp filters, the scope column or rail, tabs, the list (rows, cards, timeline or
//! tree) with facts, the preview of the selected item (the picker's children), the chip strip,
//! the level slider and the action bar. Keys stay omp's (they reach it through the PTY); a press
//! sends `select`, `activate` or `action` to the picker node. The model is
//! `mkrew_ui::tsp_picker`.

use mkrew_ui::theme::picker as size;
use mkrew_ui::tsp_picker::{self as pk, Chrome, Layout, Preview, Row, Size, State};
use serde::Deserialize;
use serde_json::Map;

use super::*;

/// One picker's list: its rows and chrome, read again only when omp writes the node, and the
/// list's scroll (only the rows on screen are laid out: a model sheet holds 1,600 items).
pub(crate) struct PickerList {
    rev: Option<u64>,
    chrome: Rc<Chrome>,
    rows: Rc<Vec<Row>>,
    list: gpui::ListState,
}

impl PickerList {
    fn new() -> Self {
        Self {
            rev: None,
            chrome: Rc::default(),
            rows: Rc::default(),
            list: gpui::ListState::new(0, gpui::ListAlignment::Top, px(theme::TSP_LIST_OVERDRAW)),
        }
    }

    /// Reads the node again after omp wrote it (`rev`): another row count starts the list over
    /// at the selected row (the row above it, often its group head, on top); the same count in
    /// another order measures the rows again in place, and a new selection is scrolled into view.
    fn sync(&mut self, rev: u64, props: &Map<String, Value>) {
        if self.rev == Some(rev) {
            return;
        }
        self.rev = Some(rev);
        let chrome = Chrome::of(props);
        let rows = pk::rows(props);
        let reset = rows.len() != self.rows.len();
        if reset {
            self.list
                .reset_with_uniform_height(rows.len(), px(row_height(&chrome)));
        } else if rows != *self.rows {
            self.list.remeasure();
        }
        let selected = pk::selected_row(&rows, pk::items(props), chrome.selected.as_deref());
        match selected {
            // Unmeasured rows have no height yet: place the selection by index, not by pixels.
            Some(ix) if reset => self.list.scroll_to(gpui::ListOffset {
                item_ix: ix.saturating_sub(1),
                offset_in_item: px(0.),
            }),
            Some(ix) if chrome.selected != self.chrome.selected => {
                self.list.scroll_to_reveal_item(ix);
            }
            _ => {}
        }
        self.chrome = Rc::new(chrome);
        self.rows = Rc::new(rows);
    }
}

fn row_height(c: &Chrome) -> f32 {
    match c.layout {
        Layout::Cards => size::CARD_HEIGHT,
        _ if c.snug => size::ROW_HEIGHT_SNUG,
        _ => size::ROW_HEIGHT,
    }
}

fn group_height(c: &Chrome) -> f32 {
    if c.snug {
        size::GROUP_HEIGHT_SNUG
    } else {
        size::GROUP_HEIGHT
    }
}

/// The height of a sheet that fits its content (an `md` sheet and one rising from the composer
/// are no taller than that): the chrome plus the rows, at most `max`.
fn fit_height(c: &Chrome, rows: &[Row], preview_below: bool, max: f32) -> f32 {
    let mut height = if c.search_in_list {
        if c.snug {
            size::SEARCH_ROW_HEIGHT_SNUG
        } else {
            size::SEARCH_ROW_HEIGHT
        }
    } else {
        let head = if c.snug {
            size::HEAD_HEIGHT_SNUG
        } else {
            size::HEAD_HEIGHT
        };
        let heads = if has_column_heads(c) {
            size::COLUMN_HEADS_HEIGHT
        } else {
            0.0
        };
        head + heads
    };
    if !c.tabs.is_empty() {
        height += size::TABS_HEIGHT;
    }
    if bar_actions(c).next().is_some() {
        height += size::BAR_HEIGHT;
    }
    if c.level.is_some() {
        height += if c.snug {
            size::LEVEL_HEIGHT_SNUG
        } else {
            size::LEVEL_HEIGHT
        };
    }
    if c.strip.is_some() {
        height += size::STRIP_HEIGHT;
    }
    if preview_below {
        height += size::PREVIEW_BELOW_MAX_HEIGHT;
    }
    // The list's padding, and room for the note of an empty, loading or failed list.
    height += space::S + 2.0 * row_height(c);
    for row in rows {
        if height >= max {
            break;
        }
        height += match row {
            Row::Group { .. } => group_height(c),
            Row::Item(_) => row_height(c),
        };
    }
    height.min(max)
}

fn has_column_heads(c: &Chrome) -> bool {
    c.layout == Layout::Rows && c.drawn_columns().any(|(col, _)| col.head.is_some())
}

fn bar_actions(c: &Chrome) -> impl Iterator<Item = &pk::Action> {
    c.actions.iter().filter(|a| !a.head)
}

/// A press target's id: unique per session, picker and part.
fn part_id(session: Ulid, picker: &str, part: &str) -> SharedString {
    SharedString::from(format!("tsp-pk-{session}-{picker}-{part}"))
}

/// The colors a mark's initials sit on, picked by its seed.
fn mark_palette(theme: &Theme) -> [u32; 8] {
    [
        theme.accent,
        theme.brand_claude,
        theme.brand_codex,
        theme.brand_copilot,
        theme.brand_omp,
        theme.syntax_keyword,
        theme.syntax_function,
        theme.syntax_type,
    ]
}

/// The keys of a control as tooltip text (`Alt+←`).
fn keys_text(keys: &[String]) -> String {
    keys.iter()
        .map(|key| pk::key_label(key))
        .collect::<Vec<_>>()
        .join("+")
}

/// The widest a sheet is, and its tallest: by `size`, or rising from the composer.
fn sheet_max(c: &Chrome) -> (f32, f32) {
    match (c.at_composer, c.size) {
        (true, _) => (size::COMPOSER_MAX_WIDTH, size::COMPOSER_MAX_HEIGHT),
        (false, Size::Md) => (size::MD_MAX_WIDTH, size::MD_MAX_HEIGHT),
        _ => (size::LG_MAX_WIDTH, size::LG_MAX_HEIGHT),
    }
}

/// A fact column's width: `chars` code-font characters, at least `FACT_MIN_WIDTH` (a `bar`
/// column at least its meter).
fn fact_width(col: &pk::Column, chars: usize) -> f32 {
    let text = chars as f32 * size::FACT_CHAR_WIDTH;
    if col.format.as_deref() == Some("bar") {
        size::METER_WIDTH.max(text)
    } else {
        text.max(size::FACT_MIN_WIDTH)
    }
}

/// Which fact columns fit beside the labels in this pane (Tern hides the lowest `priority`
/// first): the sheet's width less the scopes, the side preview, the row padding and
/// `LABEL_MIN_WIDTH`.
fn fitting_columns(p: &Paint<'_>, node: &Node, c: &Chrome) -> Vec<bool> {
    let sheet = if c.size == Size::Screen {
        p.width
    } else {
        (p.width - 2.0 * space::L).min(sheet_max(c).0)
    };
    let scopes = match (c.scopes.is_empty(), c.rail) {
        (true, _) => 0.0,
        (false, true) => size::RAIL_WIDTH,
        (false, false) => size::SCOPES_WIDTH,
    };
    let side = c.preview == Preview::Side
        && p.doc
            .children(&node.id)
            .any(|kid| !bool_prop(kid, "hidden"));
    let preview = if side { size::PREVIEW_WIDTH } else { 0.0 };
    let room = sheet - scopes - preview - 2.0 * (space::S + space::M) - size::LABEL_MIN_WIDTH;
    let widths: Vec<f32> = (c.columns.iter().zip(&c.column_chars))
        .map(|(col, &chars)| fact_width(col, chars))
        .collect();
    pk::shown_columns(&c.columns, &widths, room, space::S)
}

impl Shell {
    /// A `picker` node: the whole sheet, placed by its `size` and `anchor`.
    #[inline(never)]
    pub(super) fn tsp_picker(
        &self,
        p: &Paint<'_>,
        node: &Node,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let rev = p.mirror.rev(p.shown_id, &node.id);
        let (chrome, rows, list) = {
            let mut pickers = p.ui.pickers.borrow_mut();
            let state = (pickers.entry(node.id.clone())).or_insert_with(PickerList::new);
            state.sync(rev, &node.props);
            let shown = fitting_columns(p, node, &state.chrome);
            if shown != state.chrome.column_shown {
                Rc::make_mut(&mut state.chrome).column_shown = shown;
            }
            (state.chrome.clone(), state.rows.clone(), state.list.clone())
        };
        let c = &*chrome;
        let theme = p.theme;
        let preview = (c.preview != Preview::None)
            .then(|| self.tsp_children(p, node, cx))
            .filter(|kids| !kids.is_empty());
        let below = preview.is_some() && c.preview == Preview::Below;
        let shown = pk::shown_count(&rows);
        let center = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .when(c.search_in_list, |el| {
                el.child(self.pk_search_row(p, node, c, cx))
            })
            .when(!c.search_in_list && has_column_heads(c), |el| {
                el.child(Self::pk_column_heads(c, theme))
            })
            .child(self.pk_list(p, node, c, &rows, list, cx));
        let (side, under) = match preview {
            Some(kids) if below => (None, Some(Self::pk_preview(p, node, kids, false))),
            Some(kids) => (Some(Self::pk_preview(p, node, kids, true)), None),
            None => (None, None),
        };
        let body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .when(!c.scopes.is_empty(), |el| {
                el.child(self.pk_scopes(p, node, c, cx))
            })
            .child(center.children(under))
            .children(side);
        let content = div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .when(!c.search_in_list, |el| {
                el.child(self.pk_head(p, node, c, shown, cx))
            })
            .when(!c.tabs.is_empty(), |el| {
                el.child(self.pk_tabs(p, node, c, cx))
            })
            .child(body)
            .children(c.strip.as_ref().map(|s| self.pk_strip(p, node, s, cx)))
            .children(c.level.as_ref().map(|l| self.pk_level(p, node, c, l, cx)))
            .when(bar_actions(c).next().is_some(), |el| {
                el.child(self.pk_bar(p, node, c, cx))
            });
        if c.size == Size::Screen {
            return div()
                .size_full()
                .bg(rgb(theme.surface))
                .child(content)
                .into_any_element();
        }
        let (max_w, max_h) = sheet_max(c);
        let height = if !c.at_composer && c.size == Size::Lg {
            max_h
        } else {
            fit_height(c, &rows, below, max_h)
        };
        let sheet = floating_panel(part_id(p.session, &node.id, "sheet"), theme)
            .w_full()
            .max_w(px(max_w))
            .h(theme::rems_px(height))
            .max_h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(content);
        // A sheet acting on the draft rises from the composer at the bottom; the others sit in
        // the middle of the pane.
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .when(c.at_composer, |el| el.justify_end())
            .when(!c.at_composer, |el| el.justify_center())
            .child(sheet)
            .into_any_element()
    }

    /// The head: icon, title and subtitle, the search, the count, head actions and Esc.
    fn pk_head(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        shown: usize,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let icon = (c.icon.as_deref()).and_then(|name| self.tsp_icon(p, name, theme.text_subtle));
        let title = c.title.as_ref().map(|title| {
            let sub = (c.subtitle.as_ref())
                .filter(|_| c.size != Size::Md)
                .map(|sub| tsp::spans_of(sub, theme));
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(space::S))
                .min_w_0()
                .max_w(relative(0.42))
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight(weight::SEMIBOLD))
                        .child(title.clone()),
                )
                .children(sub.map(|sub| {
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(font_size::META)
                        .child(styled_runs(&sub, theme.text_subtle, false, theme))
                }))
        });
        let search = c.query.as_ref().map(|_| {
            div()
                .flex_1()
                .min_w(px(size::FACT_MIN_WIDTH * 2.0))
                .when(c.title.is_some(), |el| {
                    el.ml(px(space::XS))
                        .pl(px(space::M))
                        .border_l(px(theme::BORDER_WIDTH))
                        .border_color(gpui::rgba(theme.layer[1]))
                })
                .child(Self::pk_search(c, theme))
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(if c.snug {
                size::HEAD_HEIGHT_SNUG
            } else {
                size::HEAD_HEIGHT
            }))
            .pl(px(space::L))
            .pr(px(space::M))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(icon)
            .children(title)
            .children(search)
            .when(c.query.is_none(), |el| el.child(div().flex_1()))
            .child(
                div()
                    .flex_none()
                    .font_family(theme.terminal_font_family.clone())
                    .text_size(font_size::OVERLINE)
                    .text_color(rgb(theme.text_subtle))
                    .child(pk::count_label(shown, c.total)),
            )
            .children(self.pk_head_actions(p, node, c, true, cx))
            .into_any_element()
    }

    /// The search text omp keeps, with its caret (drawn while the list has the keys), or the
    /// placeholder.
    fn pk_search(c: &Chrome, theme: &Theme) -> Div {
        let query = c.query.as_deref().unwrap_or("");
        let caret_on = c.focus.as_deref().is_none_or(|focus| focus == "list");
        let caret = caret_on.then(|| {
            div()
                .flex_none()
                .w(px(size::CARET_WIDTH))
                .h(theme::rems_px(size::CARET_HEIGHT))
                .rounded(px(theme::BORDER_WIDTH))
                .bg(rgb(theme.accent))
        });
        let el = div().flex().items_center().min_w_0().overflow_hidden();
        if query.is_empty() {
            let placeholder = c.placeholder.clone().unwrap_or_else(|| match &c.noun {
                Some(noun) => format!("Search {noun}…"),
                None => "Search…".to_owned(),
            });
            return el.children(caret).child(
                div()
                    .truncate()
                    .text_color(rgb(theme.text_subtle))
                    .child(placeholder),
            );
        }
        let at = c.caret.min(query.len());
        el.child(div().whitespace_nowrap().child(query[..at].to_owned()))
            .children(caret)
            .child(div().min_w_0().truncate().child(query[at..].to_owned()))
    }

    /// The head's icon buttons (`head` actions) and, with `esc`, its Esc keycap (`close`).
    fn pk_head_actions(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        esc: bool,
        cx: &mut Context<'_, Self>,
    ) -> Vec<AnyElement> {
        let theme = p.theme;
        let mut out: Vec<AnyElement> = (c.actions.iter().filter(|a| a.head))
            .map(|action| {
                let disabled = pk::is_disabled(action.disabled.as_ref());
                let on = action.on == Some(true);
                let color = if on {
                    theme.accent_text
                } else {
                    theme.text_subtle
                };
                let glyph = (action.icon.as_deref())
                    .and_then(tsp::icon)
                    .map(|icon| icon_svg(icon, theme::ICON_SIZE_SMALL, color).into_any_element())
                    .unwrap_or_else(|| {
                        let first: String = pk::initials(&action.label).chars().take(1).collect();
                        div()
                            .text_size(font_size::OVERLINE)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(color))
                            .child(first)
                            .into_any_element()
                    });
                let tip = match pk::disabled_reason(action.disabled.as_ref()) {
                    Some(reason) => format!("{} ({reason})", action.label),
                    None => action.label.clone(),
                };
                let (session, picker, act) = (p.session, node.id.clone(), action.id.clone());
                div()
                    .id(part_id(p.session, &node.id, &format!("head-{}", action.id)))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(theme::rems_px(theme::ICON_BUTTON_SIZE))
                    .rounded(px(radius::MEDIUM))
                    .when(on, |el| el.bg(rgb(theme.accent_bg)))
                    .when(disabled, |el| el.opacity(opacity::DISABLED))
                    .pressable_unless(on || disabled, Look::Icon, theme)
                    .on_press(
                        !disabled,
                        cx.listener(move |shell, _, _, cx| {
                            shell.tsp_emit(session, pk::action_event(&picker, &act, None), cx);
                        }),
                    )
                    .tooltip(tooltip(theme, &tip, keys_text(&action.keys)))
                    .child(glyph)
                    .into_any_element()
            })
            .collect();
        if !esc {
            return out;
        }
        let (session, picker) = (p.session, node.id.clone());
        out.push(
            div()
                .id(part_id(p.session, &node.id, "esc"))
                .flex_none()
                .flex()
                .items_center()
                .h(theme::rems_px(theme::ICON_BUTTON_SIZE))
                .px(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .pressable(Look::Icon, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| {
                        shell.tsp_emit(session, pk::action_event(&picker, "close", None), cx);
                    }),
                )
                .tooltip(tooltip(theme, "Close", "Esc"))
                .child(kbd(pk::key_label("esc"), theme, false))
                .into_any_element(),
        );
        out
    }

    /// The search row of a sheet whose search leads the list (the model switcher): the icon, the
    /// search and the column heads, lined up with the facts of the rows. As in Tern there is no
    /// Esc, and the head actions sit atop the scopes (here when there are none).
    fn pk_search_row(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let icon = (c.icon.as_deref()).and_then(|name| self.tsp_icon(p, name, theme.text_subtle));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(if c.snug {
                size::SEARCH_ROW_HEIGHT_SNUG
            } else {
                size::SEARCH_ROW_HEIGHT
            }))
            .pl(px(space::M))
            .pr(px(space::S + space::M))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(icon)
            .child(div().flex_1().min_w_0().child(Self::pk_search(c, theme)))
            .when(c.layout == Layout::Rows, |el| {
                el.children(Self::pk_head_cells(c, theme))
            })
            .when(c.scopes.is_empty(), |el| {
                el.children(self.pk_head_actions(p, node, c, false, cx))
            })
            .into_any_element()
    }

    /// The row of column heads above the list, lined up with the facts of the rows.
    fn pk_column_heads(c: &Chrome, theme: &Theme) -> AnyElement {
        div()
            .flex()
            .flex_none()
            .items_end()
            .gap(px(space::S))
            .h(theme::rems_px(size::COLUMN_HEADS_HEIGHT))
            .pb(px(space::XS))
            .px(px(space::S + space::M))
            .child(div().flex_1())
            .children(Self::pk_head_cells(c, theme))
            .into_any_element()
    }

    fn pk_head_cells(c: &Chrome, theme: &Theme) -> Vec<Div> {
        c.drawn_columns()
            .map(|(col, chars)| {
                Self::pk_fact_cell(col, chars)
                    .text_size(font_size::OVERLINE)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.subtle))
                    .children(col.head.clone())
            })
            .collect()
    }

    /// A fact column's box: right-aligned, [`fact_width`] wide.
    fn pk_fact_cell(col: &pk::Column, chars: usize) -> Div {
        div()
            .flex_none()
            .w(theme::rems_px(fact_width(col, chars)))
            .flex()
            .justify_end()
            .overflow_hidden()
            .whitespace_nowrap()
    }

    /// The list, or its state: loading, the error, or the empty note.
    fn pk_list(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        rows: &[Row],
        list: gpui::ListState,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let note = |text: String| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(space::L))
                .text_color(rgb(theme.text_subtle))
                .child(text)
                .into_any_element()
        };
        let noun = c.noun.as_deref().unwrap_or("items");
        let body = match c.state {
            State::Loading => note(format!("Loading {noun}…")),
            State::Error => div()
                .p(px(space::L))
                .child(error_note(
                    (c.message.as_ref().map(pk::plain))
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| format!("Could not load the {noun}")),
                    theme,
                ))
                .into_any_element(),
            State::Ready if rows.is_empty() => note(
                (c.empty.as_ref().map(pk::plain))
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| {
                        if c.query.as_deref().is_some_and(|q| !q.is_empty()) {
                            format!("No {noun} match")
                        } else {
                            format!("No {noun}")
                        }
                    }),
            ),
            State::Ready => {
                let shell = cx.entity().downgrade();
                let (session, picker) = (p.session, node.id.clone());
                let row_theme = theme.clone();
                gpui::list(list, move |ix, _, cx| {
                    shell
                        .update(cx, |shell, cx| {
                            shell.pk_row(session, &picker, ix, &row_theme, cx)
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                })
                .size_full()
                .py(px(space::XS))
                .into_any_element()
            }
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(body)
            .children((c.confirm.as_ref()).map(|confirm| self.pk_confirm(p, node, confirm, cx)))
            .into_any_element()
    }

    /// Row `ix` of picker `picker`'s list, built when the list shows it.
    fn pk_row(
        &self,
        session: Ulid,
        picker: &str,
        ix: usize,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let empty = || div().into_any_element();
        let Some(ui) = self.tsps.get(&session) else {
            return empty();
        };
        let Some((_, doc)) = ui.mirror.as_ref().and_then(Mirror::shown) else {
            return empty();
        };
        let Some(node) = doc.get(picker) else {
            return empty();
        };
        let (c, row) = {
            let pickers = ui.pickers.borrow();
            let Some(state) = pickers.get(picker) else {
                return empty();
            };
            (state.chrome.clone(), state.rows.get(ix).cloned())
        };
        let el = match row {
            Some(Row::Group { label, count }) => Self::pk_group(&c, &label, count, theme),
            Some(Row::Item(at)) => match pk::items(&node.props).get(at) {
                Some(item) => self.pk_item(session, picker, &node.props, item, &c, theme, cx),
                None => return empty(),
            },
            None => return empty(),
        };
        div().px(px(space::S)).child(el).into_any_element()
    }

    /// A group header inside the list: its label and count, at the bottom of its row.
    fn pk_group(c: &Chrome, label: &Value, count: Option<u64>, theme: &Theme) -> AnyElement {
        let runs = tsp::spans_of(label, theme);
        div()
            .flex()
            .items_end()
            .gap(px(space::S))
            .h(theme::rems_px(group_height(c)))
            .px(px(space::M))
            .pb(px(space::XS))
            .text_size(font_size::META)
            .child(styled_runs_w(
                &runs,
                theme.text_subtle,
                false,
                weight::MEDIUM,
                theme,
            ))
            .children(count.map(|count| {
                div()
                    .font_family(theme.terminal_font_family.clone())
                    .text_size(font_size::OVERLINE)
                    .text_color(rgb(theme.subtle))
                    .child(pk::group_thousands(
                        i64::try_from(count).unwrap_or(i64::MAX),
                    ))
            }))
            .into_any_element()
    }

    /// One item: the current mark, the lead (mark, icon, dot, tree or timeline node), the label
    /// with its hits, badges and chips, the detail, and the facts.
    #[allow(clippy::too_many_arguments)]
    fn pk_item(
        &self,
        session: Ulid,
        picker: &str,
        props: &Map<String, Value>,
        item: &Value,
        c: &Chrome,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let id = pk::item_id(item);
        let selected = c.selected.as_deref() == Some(id);
        let disabled = pk::is_disabled(item.get("disabled"));
        let mono = item.get("mono").and_then(Value::as_bool) == Some(true);
        let tone = (item.get("tone").and_then(Value::as_str)).map(|t| tsp::tone_color(t, theme));
        let label = pk::label_runs(
            item.get("label").unwrap_or(&Value::Null),
            mono,
            pk::hits_of(props, item),
            theme,
        );
        let detail = (item.get("detail"))
            .map(|d| tsp::spans_of(d, theme))
            .filter(|d| !d.is_empty());
        let cards = c.layout == Layout::Cards;
        // A timeline's answers and tool calls sit under the prompts, smaller and muted.
        let quiet = c.layout == Layout::Timeline
            && matches!(
                item.get("node").and_then(Value::as_str),
                Some("assistant" | "tool")
            );
        let detail_el = |d: &Vec<(String, SpanStyle)>| {
            div()
                .min_w_0()
                .truncate()
                .text_size(font_size::META)
                .child(styled_runs(d, theme.text_subtle, false, theme))
        };
        let line = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .min_w_0()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .when(cards, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                    .when(quiet, |el| el.text_size(font_size::META))
                    .child(styled_runs(
                        &label,
                        tone.unwrap_or(if quiet { theme.text_muted } else { theme.text }),
                        mono,
                        theme,
                    )),
            )
            .children(Self::pk_badges(item, theme))
            .children(Self::pk_chips(item, theme))
            // The detail takes what the label leaves (a zero basis): the label is cut only when
            // it alone does not fit.
            .when(!cards, |el| {
                el.children(
                    detail
                        .as_ref()
                        .map(|d| detail_el(d).flex_1().ml(px(space::XS))),
                )
            });
        let text = div()
            .flex()
            .flex_col()
            .justify_center()
            .flex_1()
            .min_w_0()
            .gap(px(theme::BORDER_WIDTH * 2.0))
            .child(line)
            .when(cards, |el| el.children(detail.as_ref().map(detail_el)));
        // The current item's check; the other rows keep its width, so labels line up.
        let current = (!c.current.is_empty()).then(|| {
            let slot = div().flex_none().w(theme::rems_px(theme::ICON_SIZE_SMALL));
            if c.current.contains(id) {
                slot.child(icon_svg(
                    Icon::Check,
                    theme::ICON_SIZE_SMALL,
                    theme.accent_text,
                ))
            } else {
                slot
            }
        });
        let depth = (item.get("depth").and_then(Value::as_u64)).unwrap_or(0) as f32;
        let row = list_row(
            part_id(session, picker, &format!("item-{id}")),
            selected,
            theme,
        )
        .h(theme::rems_px(row_height(c)))
        .when(c.layout == Layout::Tree, |el| {
            el.pl(px(space::M + depth * size::TREE_INDENT))
        })
        .when(quiet, |el| el.pl(px(space::M + space::XL)))
        .children(current)
        .children(self.pk_lead(item, c, selected, theme))
        .child(text)
        .children(
            c.drawn_columns()
                .map(|(col, chars)| Self::pk_fact(item, col, chars, cards, selected, theme)),
        )
        .when(disabled, |el| el.opacity(opacity::DISABLED));
        let tip = (pk::disabled_reason(item.get("disabled")))
            .or_else(|| item.get("title").and_then(Value::as_str))
            .map(str::to_owned);
        let (picker, id) = (picker.to_owned(), id.to_owned());
        row.on_press(
            !disabled,
            cx.listener(move |shell, event: &ClickEvent, _, cx| {
                shell.pk_press_item(session, &picker, &id, event.click_count() > 1, cx);
            }),
        )
        .when_some(tip, |el, tip| el.tooltip(tooltip(theme, &tip, "")))
        .into_any_element()
    }

    /// A press on an item: `select`, or `activate` when it is the selected one already.
    fn pk_press_item(
        &self,
        session: Ulid,
        picker: &str,
        item: &str,
        double: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let selected = (self.tsps.get(&session)).and_then(|ui| {
            (ui.pickers.borrow().get(picker)).and_then(|state| state.chrome.selected.clone())
        });
        self.tsp_emit(
            session,
            pk::item_event(picker, item, selected.as_deref(), double),
            cx,
        );
    }

    /// The lead of an item: a timeline node, a tree chevron, a mark, an icon or a dot.
    fn pk_lead(
        &self,
        item: &Value,
        c: &Chrome,
        selected: bool,
        theme: &Theme,
    ) -> Option<AnyElement> {
        if c.layout == Layout::Timeline {
            return Some(Self::pk_timeline_node(item, theme));
        }
        let chevron = (c.layout == Layout::Tree)
            .then(|| item.get("open").and_then(Value::as_bool))
            .flatten()
            .map(|open| {
                let icon = if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                };
                icon_svg(icon, theme::ICON_SIZE_SMALL, theme.text_subtle).into_any_element()
            });
        let mark = (item.get("mark"))
            .and_then(|mark| pk::Mark::deserialize(mark).ok())
            .map(|mark| {
                let side = if c.layout == Layout::Cards {
                    size::CARD_MARK_SIZE
                } else {
                    size::MARK_SIZE
                };
                Self::pk_mark(&mark, side, theme)
            });
        let icon = (item.get("icon").and_then(Value::as_str))
            .and_then(tsp::icon)
            .map(|icon| {
                let color = if selected {
                    theme.text
                } else {
                    theme.text_subtle
                };
                icon_svg(icon, theme::ICON_SIZE_SMALL, color).into_any_element()
            });
        let dot = (item.get("dot").and_then(Value::as_str)).map(|tone| {
            status_dot(tsp::tone_color(tone, theme), tone == "muted").into_any_element()
        });
        let lead = mark.or(icon).or(dot);
        if chevron.is_none() && lead.is_none() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .children(chevron)
                .children(lead)
                .into_any_element(),
        )
    }

    /// A timeline node: the prompt an accent dot, an answer a ring, a tool its icon, a marker a
    /// small square.
    fn pk_timeline_node(item: &Value, theme: &Theme) -> AnyElement {
        let node = match item.get("node").and_then(Value::as_str) {
            Some("user") => status_dot(theme.accent, false).into_any_element(),
            Some("tool") => {
                let icon = (item.get("role").and_then(Value::as_str))
                    .and_then(|role| tsp::icon(role.rsplit('.').next().unwrap_or(role)))
                    .unwrap_or(Icon::Settings);
                icon_svg(icon, theme::ICON_SIZE_SMALL, theme.text_subtle).into_any_element()
            }
            Some("marker") => div()
                .size(px(space::XS + theme::BORDER_WIDTH))
                .rounded(px(theme::BORDER_WIDTH))
                .bg(rgb(theme.subtle))
                .into_any_element(),
            _ => status_dot(theme.text_subtle, true).into_any_element(),
        };
        div()
            .flex_none()
            .w(theme::rems_px(theme::ICON_SIZE))
            .flex()
            .justify_center()
            .child(node)
            .into_any_element()
    }

    /// A mark: the icon when Mkrew has it, else the initials on a color picked by the seed.
    fn pk_mark(mark: &pk::Mark, side: f32, theme: &Theme) -> AnyElement {
        if let Some(icon) = mark.icon.as_deref().and_then(tsp::icon) {
            return icon_svg(icon, side - space::XS, theme.text).into_any_element();
        }
        let seed = mark.seed.as_deref().unwrap_or(&mark.text);
        div()
            .flex_none()
            .size(theme::rems_px(side))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::TAG))
            .bg(rgb(pk::mark_color(seed, &mark_palette(theme))))
            .text_size(font_size::OVERLINE)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .text_color(rgb(theme.on_accent))
            .child(pk::initials(&mark.text))
            .into_any_element()
    }

    /// An item's badges: tinted by their tone, neutral ones muted.
    fn pk_badges(item: &Value, theme: &Theme) -> Vec<Div> {
        let Some(badges) = item.get("badges").and_then(Value::as_array) else {
            return Vec::new();
        };
        badges
            .iter()
            .filter_map(|badge| {
                let text = badge.get("text")?.as_str()?.to_owned();
                let color = match badge.get("tone").and_then(Value::as_str) {
                    None | Some("neutral" | "muted") => theme.text_muted,
                    Some(tone) => tsp::tone_color(tone, theme),
                };
                Some(tinted_chip(text, color, theme))
            })
            .collect()
    }

    /// An item's chips: outlined, the `on` ones filled.
    fn pk_chips(item: &Value, theme: &Theme) -> Vec<Div> {
        let Some(chips) = item.get("chips").and_then(Value::as_array) else {
            return Vec::new();
        };
        chips
            .iter()
            .filter_map(|chip| {
                let text = chip.get("text")?.as_str()?.to_owned();
                let on = chip.get("on").and_then(Value::as_bool) == Some(true);
                let dot = (chip.get("dot").and_then(Value::as_str))
                    .and_then(|token| tsp::color(token, theme));
                Some(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .h(theme::rems_px(theme::CHIP_HEIGHT))
                        .px(px(space::XS))
                        .rounded(px(radius::TAG))
                        .text_size(font_size::OVERLINE)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .when(on, |el| {
                            el.bg(gpui::rgba(theme.layer[0]))
                                .text_color(rgb(theme.text_muted))
                        })
                        .when(!on, |el| {
                            el.border(px(theme::BORDER_WIDTH))
                                .border_color(gpui::rgba(theme.layer[1]))
                                .text_color(rgb(theme.text_subtle))
                        })
                        .children(dot.map(|color| status_dot(color, false)))
                        .child(text),
                )
            })
            .collect()
    }

    /// One fact of an item under `col`: its text, or a meter for a `bar` column.
    fn pk_fact(
        item: &Value,
        col: &pk::Column,
        chars: usize,
        cards: bool,
        selected: bool,
        theme: &Theme,
    ) -> Div {
        let cell = Self::pk_fact_cell(col, chars)
            .font_family(theme.terminal_font_family.clone())
            .text_size(if cards {
                font_size::OVERLINE
            } else {
                font_size::META
            });
        let Some(value) = item.get("facts").and_then(|facts| facts.get(&col.id)) else {
            return cell;
        };
        let format = col.format.as_deref();
        if format == Some("bar") {
            let fraction = value.as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            let fill = if selected {
                theme.accent
            } else {
                theme.text_subtle
            };
            return cell.items_center().child(
                div()
                    .w(px(size::METER_WIDTH))
                    .h(px(size::METER_HEIGHT))
                    .rounded_full()
                    .bg(gpui::rgba(theme.layer[1]))
                    .child(
                        div()
                            .h_full()
                            .w(relative(fraction))
                            .rounded_full()
                            .bg(rgb(fill)),
                    ),
            );
        }
        let color = match format {
            Some("dim" | "time" | "elapsed") => theme.text_subtle,
            _ => theme.text_muted,
        };
        match value {
            Value::Array(_) => cell.child(styled_runs(
                &tsp::spans_of(value, theme),
                color,
                true,
                theme,
            )),
            _ => cell
                .text_color(rgb(color))
                .children(pk::fact_text(value, format)),
        }
    }

    /// The scope column (labels, counts, group heads) or, for `rail`, a column of icons.
    fn pk_scopes(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let mut kids: Vec<AnyElement> = Vec::new();
        let mut group: Option<&str> = None;
        for (ix, scope) in c.scopes.iter().enumerate() {
            let next = scope.group.as_deref();
            let new_group = if ix == 0 {
                next.is_some()
            } else {
                next != group
            };
            if new_group && c.rail {
                kids.push(
                    div()
                        .flex_none()
                        .w(px(space::L + space::XS))
                        .h(px(theme::BORDER_WIDTH))
                        .my(px(space::XS))
                        .bg(gpui::rgba(theme.layer[0]))
                        .into_any_element(),
                );
            } else if new_group && let Some(head) = next {
                kids.push(
                    div()
                        .flex()
                        .flex_none()
                        .items_end()
                        .h(theme::rems_px(size::GROUP_HEIGHT_SNUG))
                        .when(ix > 0, |el| el.mt(px(space::S)))
                        .px(px(space::S))
                        .pb(px(space::XS))
                        .text_size(font_size::META)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text_subtle))
                        .child(head.to_owned())
                        .into_any_element(),
                );
            }
            group = next;
            kids.push(self.pk_scope(p, node, c, scope, cx));
        }
        // A sheet whose search leads the list keeps its head actions atop the scopes, in a box
        // as tall as the search row.
        let head = (c.search_in_list).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .gap(px(space::XS))
                .w_full()
                .h(theme::rems_px(if c.snug {
                    size::SEARCH_ROW_HEIGHT_SNUG
                } else {
                    size::SEARCH_ROW_HEIGHT
                }))
                .border_b(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(theme.layer[0]))
                .children(self.pk_head_actions(p, node, c, false, cx))
        });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .w(px(if c.rail {
                size::RAIL_WIDTH
            } else {
                size::SCOPES_WIDTH
            }))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(head)
            .child(
                div()
                    .id(part_id(p.session, &node.id, "scopes"))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(theme::BORDER_WIDTH))
                    .py(px(space::S))
                    .when(c.rail, |el| el.items_center())
                    .when(!c.rail, |el| el.px(px(space::S)))
                    .children(kids),
            )
            .into_any_element()
    }

    fn pk_scope(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        scope: &pk::Scope,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let on = c.scope.as_deref() == Some(scope.id.as_str());
        let disabled = pk::is_disabled(scope.disabled.as_ref());
        let icon_color = if on && c.rail {
            theme.accent_text
        } else {
            theme.text_subtle
        };
        let lead = match (&scope.mark, scope.icon.as_deref().and_then(tsp::icon)) {
            (Some(mark), _) => Some(Self::pk_mark(mark, size::MARK_SIZE, theme)),
            (None, Some(icon)) => {
                Some(icon_svg(icon, theme::ICON_SIZE_SMALL, icon_color).into_any_element())
            }
            (None, None) => None,
        };
        let label = tsp::spans_of(&scope.label, theme);
        let (session, picker, value) = (p.session, node.id.clone(), scope.id.clone());
        let el = div()
            .id(part_id(p.session, &node.id, &format!("scope-{}", scope.id)))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .rounded(px(radius::MEDIUM))
            .when(disabled, |el| el.opacity(opacity::DISABLED))
            .pressable_unless(on || disabled, Look::Row, theme)
            .on_press(
                !disabled,
                cx.listener(move |shell, _, _, cx| {
                    let event = pk::action_event(&picker, "scope", Some(&value));
                    shell.tsp_emit(session, event, cx);
                }),
            );
        if c.rail {
            let side = if c.snug {
                size::RAIL_BUTTON_SNUG
            } else {
                size::RAIL_BUTTON
            };
            let name: String = label.iter().map(|(t, _)| t.as_str()).collect();
            let tip = match scope.count {
                Some(count) => format!("{name} ({count})"),
                None => name,
            };
            return el
                .size(theme::rems_px(side))
                .justify_center()
                .when(on, |el| {
                    el.bg(rgb(theme.accent_bg)).child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(space::S))
                            .bottom(px(space::S))
                            .w(px(theme::PALETTE_SELECTED_BAR))
                            .rounded_full()
                            .bg(rgb(theme.accent)),
                    )
                })
                .children(lead)
                .tooltip(tooltip(theme, &tip, ""))
                .into_any_element();
        }
        let text = if on { theme.text } else { theme.text_muted };
        el.gap(px(space::S))
            .h(theme::rems_px(size::SCOPE_ROW_HEIGHT))
            .px(px(space::S))
            .text_color(rgb(text))
            .when(on, |el| {
                el.bg(rgb(theme.surface_raised))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::rgba(theme.layer[1]))
            })
            .children(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(styled_runs(&label, text, false, theme)),
            )
            .children(
                (scope.dot.as_deref())
                    .map(|tone| status_dot(tsp::tone_color(tone, theme), tone == "muted")),
            )
            .children(scope.count.map(|count| {
                div()
                    .flex_none()
                    .font_family(theme.terminal_font_family.clone())
                    .text_size(font_size::OVERLINE)
                    .text_color(rgb(if on { theme.text_subtle } else { theme.subtle }))
                    .child(pk::group_thousands(
                        i64::try_from(count).unwrap_or(i64::MAX),
                    ))
            }))
            .when_some(
                pk::disabled_reason(scope.disabled.as_ref()),
                |el, reason| el.tooltip(tooltip(theme, reason, "")),
            )
            .into_any_element()
    }

    /// The tab bar: the active tab tinted with the accent.
    fn pk_tabs(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let tabs = c.tabs.iter().map(|tab| {
            let on = c.tab.as_deref() == Some(tab.id.as_str());
            let text = if on {
                theme.accent_text
            } else {
                theme.text_subtle
            };
            let (session, picker, value) = (p.session, node.id.clone(), tab.id.clone());
            div()
                .id(part_id(p.session, &node.id, &format!("tab-{}", tab.id)))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .h(theme::rems_px(theme::SEGMENT_HEIGHT))
                .px(px(space::S))
                .rounded(px(radius::MEDIUM))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(text))
                .when(on, |el| el.bg(rgb(theme.accent_bg)))
                .pressable_unless(on, Look::Chip, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| {
                        let event = pk::action_event(&picker, "tab", Some(&value));
                        shell.tsp_emit(session, event, cx);
                    }),
                )
                .child(styled_runs(
                    &tsp::spans_of(&tab.label, theme),
                    text,
                    false,
                    theme,
                ))
                .children(tab.count.map(|count| {
                    div()
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::OVERLINE)
                        .opacity(opacity::DIMMED)
                        .child(pk::group_thousands(
                            i64::try_from(count).unwrap_or(i64::MAX),
                        ))
                }))
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(theme::BORDER_WIDTH * 2.0))
            .h(theme::rems_px(size::TABS_HEIGHT))
            .px(px(space::M))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(tabs)
            .into_any_element()
    }

    /// The preview of the selected item (the picker's children): beside the list, or below it.
    fn pk_preview(p: &Paint<'_>, node: &Node, kids: Vec<AnyElement>, side: bool) -> AnyElement {
        let theme = p.theme;
        div()
            .id(part_id(p.session, &node.id, "preview"))
            .flex_none()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .px(px(space::L))
            .text_size(font_size::META)
            .when(side, |el| {
                el.w(px(size::PREVIEW_WIDTH))
                    .h_full()
                    .py(px(space::L))
                    .border_l(px(theme::BORDER_WIDTH))
            })
            .when(!side, |el| {
                el.max_h(px(size::PREVIEW_BELOW_MAX_HEIGHT))
                    .py(px(space::M))
                    .border_t(px(theme::BORDER_WIDTH))
            })
            .border_color(gpui::rgba(theme.layer[0]))
            .children(kids)
            .into_any_element()
    }

    /// The chip strip above the action bar (the model hub's role assignment).
    fn pk_strip(
        &self,
        p: &Paint<'_>,
        node: &Node,
        strip: &pk::Strip,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let chips = strip.items.iter().map(|chip| {
            let picked = chip.on || strip.selected.as_deref() == Some(chip.id.as_str());
            let text = if picked {
                theme.accent_text
            } else {
                theme.text_subtle
            };
            let dot = (chip.dot.as_deref()).and_then(|token| tsp::color(token, theme));
            let (session, picker, value) = (p.session, node.id.clone(), chip.id.clone());
            div()
                .id(part_id(p.session, &node.id, &format!("strip-{}", chip.id)))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .h(theme::rems_px(theme::TAB_CHIP_HEIGHT))
                .px(px(space::S))
                .rounded(px(radius::MEDIUM))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(text))
                .border(px(theme::BORDER_WIDTH))
                .when(picked, |el| {
                    el.bg(rgb(theme.accent_bg))
                        .border_color(gpui::rgba(theme::with_alpha(
                            theme.accent,
                            theme::CHIP_RING_ALPHA,
                        )))
                })
                .when(!picked, |el| el.border_color(gpui::rgba(theme.layer[1])))
                .pressable_unless(picked, Look::Chip, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| {
                        let event = pk::action_event(&picker, "strip", Some(&value));
                        shell.tsp_emit(session, event, cx);
                    }),
                )
                .children(dot.map(|color| status_dot(color, false)))
                .child(styled_runs(
                    &tsp::spans_of(&chip.label, theme),
                    text,
                    false,
                    theme,
                ))
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::M))
            .min_h(theme::rems_px(size::STRIP_HEIGHT))
            .px(px(space::L))
            .py(px(space::S))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(strip.label.as_ref().map(|label| {
                div()
                    .flex_none()
                    .max_w(relative(0.4))
                    .truncate()
                    .text_size(font_size::META)
                    .child(styled_runs(
                        &tsp::spans_of(label, theme),
                        theme.text_subtle,
                        false,
                        theme,
                    ))
            }))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_wrap()
                    .gap(px(space::XS))
                    .children(chips),
            )
            .into_any_element()
    }

    /// The level slider (the model switcher's thinking effort): the track filled up to the
    /// current step, named in its middle; a press on a step sends it.
    fn pk_level(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        level: &pk::Level,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let disabled = pk::is_disabled(level.disabled.as_ref());
        let (at, count) = pk::level_position(level);
        let fill = if count == 0 {
            0.0
        } else {
            (at + 1) as f32 / count as f32
        };
        let label = match level.icon.as_deref().and_then(tsp::icon) {
            Some(icon) => icon_svg(icon, theme::ICON_SIZE, theme.text_subtle).into_any_element(),
            None => div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(level.label.clone())
                .into_any_element(),
        };
        let name = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(space::XS))
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::OVERLINE)
            .text_color(rgb(theme.text))
            .children(level.steps.get(at).map(|step| step.label.clone()))
            .children(
                level
                    .detail
                    .clone()
                    .map(|detail| div().text_color(rgb(theme.text_subtle)).child(detail)),
            );
        let steps = level.steps.iter().map(|step| {
            let (session, picker, value) = (p.session, node.id.clone(), step.id.clone());
            div()
                .id(part_id(p.session, &node.id, &format!("level-{}", step.id)))
                .flex_1()
                .h_full()
                .rounded_full()
                .when(!disabled, |el| el.cursor_pointer())
                .on_press(
                    !disabled,
                    cx.listener(move |shell, _, _, cx| {
                        let event = pk::action_event(&picker, "level", Some(&value));
                        shell.tsp_emit(session, event, cx);
                    }),
                )
                .tooltip(tooltip(theme, &step.label, ""))
        });
        let track = div()
            .relative()
            .flex_1()
            .h(theme::rems_px(size::LEVEL_TRACK_HEIGHT))
            .rounded_full()
            .bg(gpui::rgba(theme.layer[0]))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(relative(fill))
                    .rounded_full()
                    .bg(theme.tint(theme.accent)),
            )
            .child(name)
            .child(div().absolute().inset_0().flex().children(steps));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::M))
            .h(theme::rems_px(if c.snug {
                size::LEVEL_HEIGHT_SNUG
            } else {
                size::LEVEL_HEIGHT
            }))
            .pl(px(space::L))
            .pr(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .when(disabled, |el| el.opacity(opacity::DISABLED))
            .child(
                div()
                    .id(part_id(p.session, &node.id, "level-label"))
                    .flex_none()
                    .child(label)
                    .tooltip(tooltip(theme, &level.label, keys_text(&level.keys))),
            )
            .child(track)
            .into_any_element()
    }

    /// The action bar: buttons with their keycaps, `end` ones at the right.
    fn pk_bar(
        &self,
        p: &Paint<'_>,
        node: &Node,
        c: &Chrome,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let (mut start, mut end) = (Vec::new(), Vec::new());
        for action in bar_actions(c) {
            let el = self.pk_button(p, node, action, cx);
            if action.end {
                end.push(el);
            } else {
                start.push(el);
            }
        }
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(size::BAR_HEIGHT))
            .pl(px(space::M))
            .pr(px(space::S))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[0]))
            .children(start)
            .child(div().flex_1())
            .children(end)
            .into_any_element()
    }

    fn pk_button(
        &self,
        p: &Paint<'_>,
        node: &Node,
        action: &pk::Action,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let disabled = pk::is_disabled(action.disabled.as_ref());
        let on = action.on == Some(true);
        let color = if action.danger {
            theme.danger_text
        } else if on {
            theme.accent_text
        } else if action.primary {
            theme.text
        } else {
            theme.text_muted
        };
        let (session, picker, act) = (p.session, node.id.clone(), action.id.clone());
        div()
            .id(part_id(p.session, &node.id, &format!("act-{}", action.id)))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(size::BUTTON_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::MEDIUM))
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(color))
            .when(action.primary, |el| {
                el.ml(px(space::XS))
                    .bg(rgb(theme.surface_raised))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::rgba(theme.layer[1]))
                    .child(top_highlight(theme, radius::MEDIUM))
            })
            .when(on, |el| el.bg(rgb(theme.accent_bg)))
            .when(disabled, |el| el.opacity(opacity::DISABLED))
            .pressable(Look::Row, !disabled, theme)
            .on_press(
                !disabled,
                cx.listener(move |shell, _, _, cx| {
                    shell.tsp_emit(session, pk::action_event(&picker, &act, None), cx);
                }),
            )
            .when_some(
                pk::disabled_reason(action.disabled.as_ref()),
                |el, reason| el.tooltip(tooltip(theme, reason, "")),
            )
            .child(action.label.clone())
            .children((action.keys.iter()).map(|key| kbd(pk::key_label(key), theme, false)))
            .into_any_element()
    }

    /// A destructive action waiting for a second press, over the bottom of the list.
    fn pk_confirm(
        &self,
        p: &Paint<'_>,
        node: &Node,
        confirm: &pk::Confirm,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let button = |part: &str, label: String, act: String, danger: bool| {
            let (session, picker) = (p.session, node.id.clone());
            div()
                .id(part_id(p.session, &node.id, part))
                .flex_none()
                .flex()
                .items_center()
                .h(theme::rems_px(theme::CHIP_HEIGHT + space::XS))
                .px(px(space::S))
                .rounded(px(radius::MEDIUM))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(if danger {
                    theme.danger_text
                } else {
                    theme.text_muted
                }))
                .pressable(Look::Raised, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| {
                        shell.tsp_emit(session, pk::action_event(&picker, &act, None), cx);
                    }),
                )
                .child(label)
        };
        let cancel = button("cancel", "Cancel".to_owned(), "cancel".to_owned(), false);
        let label = (confirm.label.clone()).unwrap_or_else(|| "Confirm".to_owned());
        let go = button("confirm", label, confirm.act.clone(), true);
        div()
            .absolute()
            .left(px(space::S))
            .right(px(space::S))
            .bottom(px(space::S))
            .occlude()
            .flex()
            .items_center()
            .gap(px(space::S))
            .py(px(space::XS))
            .pl(px(space::M))
            .pr(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.danger_bg))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(danger_ring(theme, theme::DANGER_RING_MIX)))
            .child(icon_svg(
                Icon::TriangleAlert,
                theme::ICON_SIZE_SMALL,
                theme.danger_text,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight(weight::MEDIUM))
                    .child(styled_runs(
                        &tsp::spans_of(&confirm.text, theme),
                        theme.text,
                        false,
                        theme,
                    )),
            )
            .child(cancel)
            .child(go)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome(value: Value) -> (Chrome, Vec<Row>) {
        let Value::Object(props) = value else {
            unreachable!("an object")
        };
        (Chrome::of(&props), pk::rows(&props))
    }

    #[test]
    fn a_short_sheet_fits_its_rows_and_a_long_one_stops_at_its_max() {
        let (c, rows) = chrome(json!({"size": "md", "items": [{"id": "a"}, {"id": "b"}]}));
        let short = fit_height(&c, &rows, false, size::MD_MAX_HEIGHT);
        assert!(short < size::MD_MAX_HEIGHT, "{short}");
        let many: Vec<Value> = (0..500).map(|i| json!({"id": i.to_string()})).collect();
        let (c, rows) = chrome(json!({"size": "md", "items": many}));
        assert_eq!(
            fit_height(&c, &rows, false, size::MD_MAX_HEIGHT),
            size::MD_MAX_HEIGHT
        );
    }
}
