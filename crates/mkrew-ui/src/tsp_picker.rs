//! omp's `picker` sheet (TSP 19 `TspPickerProps`, ADR 0119): the chrome props as types and the
//! pure decisions the pane draws from. The items stay JSON in the node: a sheet of 1,600 models
//! is read row by row as the list shows them, never converted whole. omp filters, keeps the
//! selection and owns the keys; the pane draws what omp sent and sends clicks back.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::theme::Theme;
use crate::tsp::{self, SpanStyle};

/// A row of a picker's scope column (`TspPickerScope`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Scope {
    pub id: String,
    pub label: Value,
    pub icon: Option<String>,
    pub mark: Option<Mark>,
    pub count: Option<u64>,
    pub group: Option<String>,
    pub disabled: Option<Value>,
    pub dot: Option<String>,
}

/// An avatar: the `icon` when the pane has it, else up to two initials of `text` on a color
/// picked by `seed` (`TspPickerMark`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Mark {
    pub text: String,
    pub seed: Option<String>,
    pub icon: Option<String>,
}

/// A tab of the picker's tab bar.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Tab {
    pub id: String,
    pub label: Value,
    pub count: Option<u64>,
}

/// A right-aligned fact column (`TspPickerColumn`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Column {
    pub id: String,
    pub head: Option<String>,
    pub format: Option<String>,
    pub priority: Option<i64>,
    /// Minimum width in characters.
    pub min: Option<u32>,
}

/// A button of the action bar, or of the head when `head` (`TspPickerAction`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub keys: Vec<String>,
    pub primary: bool,
    pub danger: bool,
    pub end: bool,
    pub on: Option<bool>,
    pub head: bool,
    pub icon: Option<String>,
    pub disabled: Option<Value>,
}

/// The chip strip above the action bar.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Strip {
    pub label: Option<Value>,
    pub items: Vec<StripItem>,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct StripItem {
    pub id: String,
    pub label: Value,
    pub on: bool,
    pub dot: Option<String>,
}

/// The stepped slider above the action bar (`TspPickerLevel`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Level {
    pub label: String,
    pub icon: Option<String>,
    pub detail: Option<String>,
    pub steps: Vec<Step>,
    pub value: String,
    pub keys: Vec<String>,
    pub disabled: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Step {
    pub id: String,
    pub label: String,
}

/// A destructive action waiting for a second press.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Confirm {
    pub text: Value,
    pub act: String,
    pub label: Option<String>,
}

/// Everything of a picker but its items: small, read once per paint.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chrome {
    pub title: Option<String>,
    pub subtitle: Option<Value>,
    pub icon: Option<String>,
    pub noun: Option<String>,
    pub size: Size,
    /// Rises from the composer (the model switcher).
    pub at_composer: bool,
    /// The search leads the column heads' row; there is no head row.
    pub search_in_list: bool,
    pub snug: bool,
    pub layout: Layout,
    /// `None`: no search field (`query: null`).
    pub query: Option<String>,
    /// The caret as a byte offset into `query`.
    pub caret: usize,
    pub placeholder: Option<String>,
    pub scopes: Vec<Scope>,
    pub scope: Option<String>,
    pub rail: bool,
    pub tabs: Vec<Tab>,
    pub tab: Option<String>,
    pub columns: Vec<Column>,
    /// The characters each of `columns` needs ([`column_chars`]).
    pub column_chars: Vec<usize>,
    /// Which of `columns` fit the sheet ([`shown_columns`]); all of them until the pane says.
    pub column_shown: Vec<bool>,
    pub current: HashSet<String>,
    pub total: Option<u64>,
    pub preview: Preview,
    pub actions: Vec<Action>,
    pub strip: Option<Strip>,
    pub level: Option<Level>,
    pub state: State,
    pub message: Option<Value>,
    pub empty: Option<Value>,
    pub confirm: Option<Confirm>,
    pub focus: Option<String>,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Size {
    Md,
    #[default]
    Lg,
    Screen,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Rows,
    Cards,
    Timeline,
    Tree,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Preview {
    #[default]
    Side,
    Below,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Ready,
    Loading,
    Error,
}

/// A prop as `T`; absent or of another shape is `None` (omp's sheet still draws).
fn prop<T: DeserializeOwned>(props: &Map<String, Value>, key: &str) -> Option<T> {
    let value = props.get(key)?;
    T::deserialize(value).ok()
}

fn text_prop(props: &Map<String, Value>, key: &str) -> Option<String> {
    props.get(key).and_then(Value::as_str).map(str::to_owned)
}

impl Chrome {
    /// The columns drawn, with the characters each needs.
    pub fn drawn_columns(&self) -> impl Iterator<Item = (&Column, usize)> {
        (self.columns.iter().zip(&self.column_chars))
            .zip(&self.column_shown)
            .filter(|(_, shown)| **shown)
            .map(|((col, chars), _)| (col, *chars))
    }

    pub fn of(props: &Map<String, Value>) -> Self {
        let query = match props.get("query") {
            Some(Value::Null) => None,
            Some(Value::String(query)) => Some(query.clone()),
            _ => Some(String::new()),
        };
        let caret = match (&query, props.get("cursor").and_then(Value::as_u64)) {
            (Some(query), Some(units)) => {
                tsp::byte_at_utf16(query, usize::try_from(units).unwrap_or(usize::MAX))
            }
            (Some(query), None) => query.len(),
            (None, _) => 0,
        };
        let is = |key: &str, value: &str| props.get(key).and_then(Value::as_str) == Some(value);
        let columns: Vec<Column> = prop(props, "columns").unwrap_or_default();
        Self {
            title: text_prop(props, "title").filter(|t| !t.is_empty()),
            subtitle: props.get("subtitle").filter(|v| !v.is_null()).cloned(),
            icon: text_prop(props, "icon"),
            noun: text_prop(props, "noun"),
            size: match props.get("size").and_then(Value::as_str) {
                Some("md") => Size::Md,
                Some("screen") => Size::Screen,
                _ => Size::Lg,
            },
            at_composer: is("anchor", "composer"),
            search_in_list: is("search", "list"),
            snug: is("density", "snug"),
            layout: match props.get("layout").and_then(Value::as_str) {
                Some("cards") => Layout::Cards,
                Some("timeline") => Layout::Timeline,
                Some("tree") => Layout::Tree,
                _ => Layout::Rows,
            },
            query,
            caret,
            placeholder: text_prop(props, "placeholder"),
            scopes: prop(props, "scopes").unwrap_or_default(),
            scope: text_prop(props, "scope"),
            rail: is("scopeStyle", "rail"),
            tabs: prop(props, "tabs").unwrap_or_default(),
            tab: text_prop(props, "tab"),
            column_chars: column_chars(props, &columns),
            column_shown: vec![true; columns.len()],
            columns,
            current: prop(props, "current").unwrap_or_default(),
            total: props.get("total").and_then(Value::as_u64),
            preview: match props.get("preview").and_then(Value::as_str) {
                Some("below") => Preview::Below,
                Some("none") => Preview::None,
                _ => Preview::Side,
            },
            actions: prop(props, "actions").unwrap_or_default(),
            strip: prop(props, "strip"),
            level: prop(props, "level"),
            state: match props.get("state").and_then(Value::as_str) {
                Some("loading") => State::Loading,
                Some("error") => State::Error,
                _ => State::Ready,
            },
            message: props.get("message").filter(|v| !v.is_null()).cloned(),
            empty: props.get("empty").filter(|v| !v.is_null()).cloned(),
            confirm: prop(props, "confirm"),
            focus: text_prop(props, "focus"),
            selected: text_prop(props, "selected"),
        }
    }
}

/// One entry of the list: a group header of `order`, or the item at this index of `items`.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Group { label: Value, count: Option<u64> },
    Item(usize),
}

/// What the list shows, in order: `order` (omp's filtering and groups), else every item. An id
/// `items` does not hold is skipped.
pub fn rows(props: &Map<String, Value>) -> Vec<Row> {
    let items = items(props);
    let Some(order) = props.get("order").and_then(Value::as_array) else {
        return (0..items.len()).map(Row::Item).collect();
    };
    let index: HashMap<&str, usize> = (items.iter().enumerate())
        .filter_map(|(ix, item)| Some((item.get("id")?.as_str()?, ix)))
        .collect();
    order
        .iter()
        .filter_map(|entry| match entry {
            Value::String(id) => index.get(id.as_str()).copied().map(Row::Item),
            Value::Object(group) => Some(Row::Group {
                label: group.get("label").cloned().unwrap_or(Value::Null),
                count: group.get("count").and_then(Value::as_u64),
            }),
            _ => None,
        })
        .collect()
}

/// The picker's `items` (empty when absent).
pub fn items(props: &Map<String, Value>) -> &[Value] {
    props
        .get("items")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The id of an item.
pub fn item_id(item: &Value) -> &str {
    item.get("id").and_then(Value::as_str).unwrap_or("")
}

/// The list index of the selected item.
pub fn selected_row(rows: &[Row], items: &[Value], selected: Option<&str>) -> Option<usize> {
    let selected = selected?;
    rows.iter().position(
        |row| matches!(row, Row::Item(ix) if items.get(*ix).map(item_id) == Some(selected)),
    )
}

/// How many items the list shows (group headers do not count).
pub fn shown_count(rows: &[Row]) -> usize {
    rows.iter()
        .filter(|row| matches!(row, Row::Item(_)))
        .count()
}

/// The text of a `TspText` (a string, or spans).
pub fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(spans) => spans
            .iter()
            .filter_map(|span| span.get("t").and_then(Value::as_str))
            .collect(),
        _ => String::new(),
    }
}

/// The search hits of `item` in its label as UTF-16 `[from, to)` pairs: the picker's `hits` for
/// the query, else the item's own.
pub fn hits_of<'a>(props: &'a Map<String, Value>, item: &'a Value) -> &'a [Value] {
    props
        .get("hits")
        .and_then(|hits| hits.get(item_id(item)))
        .or_else(|| item.get("hits"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// An item's label as runs: search hits in bold `text`; a `mono` label (a model id, a path) has
/// its prefix up to the last `/` in `muted` and the rest in `text`. A label given as spans keeps
/// its own styles.
pub fn label_runs(
    label: &Value,
    mono: bool,
    hits: &[Value],
    theme: &Theme,
) -> Vec<(String, SpanStyle)> {
    let Value::String(text) = label else {
        return tsp::spans_of(label, theme);
    };
    let ranges: Vec<(usize, usize)> = hits
        .iter()
        .filter_map(|hit| {
            let from = usize::try_from(hit.get(0)?.as_u64()?).ok()?;
            let to = usize::try_from(hit.get(1)?.as_u64()?).ok()?;
            let (from, to) = (tsp::byte_at_utf16(text, from), tsp::byte_at_utf16(text, to));
            (from < to).then_some((from, to))
        })
        .collect();
    let dim_end = if mono {
        text.rfind('/').map_or(0, |slash| slash + 1)
    } else {
        0
    };
    let mut cuts: Vec<usize> = vec![0, dim_end, text.len()];
    cuts.extend(ranges.iter().flat_map(|&(from, to)| [from, to]));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter(|w| w[0] < w[1])
        .map(|w| {
            let (from, to) = (w[0], w[1]);
            let hit = ranges.iter().any(|&(a, b)| a <= from && to <= b);
            let style = SpanStyle {
                color: Some(if from < dim_end && !hit {
                    theme.text_muted
                } else {
                    theme.text
                }),
                bold: hit,
                mono,
                ..SpanStyle::default()
            };
            (text[from..to].to_owned(), style)
        })
        .collect()
}

/// A fact cell's text: a `TspText` as is; a number by the column's `format` (`bar` is drawn as a
/// meter and has no text).
pub fn fact_text(value: &Value, format: Option<&str>) -> Option<String> {
    match value {
        Value::Number(number) => {
            let n = number.as_f64()?;
            Some(match format {
                Some("bar") => return None,
                Some("elapsed") | Some("time") => {
                    tsp::elapsed_label(u64::try_from(number.as_i64()?.max(0)).ok()?)
                }
                Some("price") => format!("${n:.2}"),
                _ if n.fract() == 0.0 && n.abs() < 1e15 => group_thousands(n as i64),
                _ => format!("{n:.2}"),
            })
        }
        other => Some(plain(other)).filter(|text| !text.is_empty()),
    }
}

/// The widest a fact column grows, in characters; a longer fact is cut.
pub const MAX_FACT_CHARS: usize = 16;

/// How many characters each column needs: its head, its `min`, and its widest fact (at most
/// [`MAX_FACT_CHARS`]), so the columns line up down the list and nothing is cut.
pub fn column_chars(props: &Map<String, Value>, columns: &[Column]) -> Vec<usize> {
    let items = items(props);
    columns
        .iter()
        .map(|col| {
            let format = col.format.as_deref();
            let widest = (items.iter())
                .filter_map(|item| item.get("facts")?.get(&col.id))
                .filter_map(|value| fact_text(value, format))
                .map(|text| text.chars().count())
                .max()
                .unwrap_or(0);
            let head = col.head.as_deref().map_or(0, |h| h.chars().count());
            let min = col.min.map_or(0, |min| min as usize);
            widest.max(head).max(min).min(MAX_FACT_CHARS)
        })
        .collect()
}

/// `1604` as `1,604`.
pub fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (ix, c) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The head's count: `12 of 1,604` while filtered, else the total (a sheet may list rows that
/// are not counted, such as `/agents`' "New agent"), else the number shown.
pub fn count_label(shown: usize, total: Option<u64>) -> String {
    let text = |n: u64| group_thousands(i64::try_from(n).unwrap_or(i64::MAX));
    let shown = shown as u64;
    match total {
        Some(total) if shown < total => format!("{} of {}", text(shown), text(total)),
        Some(total) => text(total),
        None => text(shown),
    }
}

/// Which of `columns` are shown when `room` px are left for them, `widths` being theirs and
/// `gap` the space before each: the lowest `priority` hides first (a column without one last),
/// until the others fit.
pub fn shown_columns(columns: &[Column], widths: &[f32], room: f32, gap: f32) -> Vec<bool> {
    let mut shown = vec![true; columns.len()];
    let mut used: f32 = widths.iter().map(|w| w + gap).sum();
    let mut order: Vec<usize> = (0..columns.len()).collect();
    order.sort_by_key(|&ix| columns[ix].priority.unwrap_or(i64::MAX));
    for ix in order {
        if used <= room {
            break;
        }
        shown[ix] = false;
        used -= widths.get(ix).copied().unwrap_or(0.0) + gap;
    }
    shown
}

/// A key name omp binds (`enter`, `alt`, `left`) as a keycap label.
pub fn key_label(key: &str) -> String {
    match key {
        "enter" | "return" => "↵".to_owned(),
        "esc" | "escape" => "Esc".to_owned(),
        "tab" => "Tab".to_owned(),
        "space" => "Space".to_owned(),
        "backspace" => "⌫".to_owned(),
        "delete" | "del" => "Del".to_owned(),
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "alt" | "option" => "Alt".to_owned(),
        "ctrl" | "control" => "Ctrl".to_owned(),
        "shift" => "Shift".to_owned(),
        "meta" | "cmd" | "super" => "Win".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// Up to two initials of a mark's text, upper case.
pub fn initials(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// The fill of a mark: one of `palette`, the same for the same seed.
pub fn mark_color(seed: &str, palette: &[u32]) -> u32 {
    // FNV-1a: stable across runs and platforms, unlike the std hasher.
    let hash = seed.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    palette
        .get(hash as usize % palette.len().max(1))
        .copied()
        .unwrap_or_default()
}

/// Whether a `disabled` prop (`true` or the reason) disables.
pub fn is_disabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(_)) => true,
        _ => false,
    }
}

/// The reason of a `disabled` prop, when it gives one.
pub fn disabled_reason(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty())
}

/// A press on an item: `select` it, or `activate` it when it already is selected (a second
/// press opens it, as in omp's own terminal) or on a double press.
pub fn item_event(picker: &str, item: &str, selected: Option<&str>, double: bool) -> Value {
    tsp::select_event(picker, item, double || selected == Some(item))
}

/// A press on a scope, tab, chip, level step or button: `action` with its `act` (and `value`).
pub fn action_event(picker: &str, act: &str, value: Option<&str>) -> Value {
    match value {
        Some(value) => json!({"ev": "action", "id": picker, "act": act, "value": value}),
        None => json!({"ev": "action", "id": picker, "act": act}),
    }
}

/// The index of the level's current step and the number of steps.
pub fn level_position(level: &Level) -> (usize, usize) {
    let at = (level.steps.iter())
        .position(|step| step.id == level.value)
        .unwrap_or(0);
    (at, level.steps.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => unreachable!("an object"),
        }
    }

    #[test]
    fn rows_follow_order_with_groups_and_skip_unknown_ids() {
        let p = props(json!({
            "items": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}, {"id": "c", "label": "C"}],
            "order": [{"group": "g1", "label": "Recent", "count": 2}, "c", "nope", "a"],
        }));
        assert_eq!(
            rows(&p),
            vec![
                Row::Group {
                    label: json!("Recent"),
                    count: Some(2)
                },
                Row::Item(2),
                Row::Item(0)
            ]
        );
        let r = rows(&p);
        assert_eq!(selected_row(&r, items(&p), Some("a")), Some(2));
        assert_eq!(selected_row(&r, items(&p), Some("b")), None, "filtered out");
        assert_eq!(shown_count(&r), 2);
    }

    #[test]
    fn rows_without_order_are_the_items() {
        let p = props(json!({"items": [{"id": "a"}, {"id": "b"}]}));
        assert_eq!(rows(&p), vec![Row::Item(0), Row::Item(1)]);
        assert!(rows(&props(json!({}))).is_empty());
    }

    #[test]
    fn chrome_reads_the_model_switcher() {
        let c = Chrome::of(&props(json!({
            "icon": "cpu", "size": "md", "anchor": "composer", "search": "list",
            "density": "snug", "layout": "rows", "query": "só", "cursor": 1,
            "scopes": [{"id": "provider:anthropic", "label": "anthropic", "count": 213,
                        "mark": {"text": "An", "seed": "anthropic"}}],
            "scopeStyle": "rail", "current": ["m1"],
            "level": {"label": "Thinking", "icon": "brain",
                      "steps": [{"id": "low", "label": "low"}, {"id": "high", "label": "high"}],
                      "value": "high", "keys": ["left", "right"]},
            "actions": [{"id": "task", "label": "Task model", "head": true, "on": true}],
        })));
        assert_eq!(c.size, Size::Md);
        assert!(c.at_composer && c.search_in_list && c.snug && c.rail);
        assert_eq!(c.query.as_deref(), Some("só"));
        assert_eq!(c.caret, 1);
        assert_eq!(c.scopes[0].count, Some(213));
        assert_eq!(
            c.scopes[0].mark.as_ref().map(|m| m.text.as_str()),
            Some("An")
        );
        assert!(c.current.contains("m1"));
        assert_eq!(level_position(c.level.as_ref().unwrap()), (1, 2));
        assert!(c.actions[0].head && c.actions[0].on == Some(true));
    }

    #[test]
    fn a_null_query_hides_the_search_and_a_missing_one_is_empty() {
        assert_eq!(Chrome::of(&props(json!({"query": null}))).query, None);
        let c = Chrome::of(&props(json!({})));
        assert_eq!((c.query.as_deref(), c.caret), (Some(""), 0));
    }

    #[test]
    fn a_bad_prop_falls_back_without_losing_the_others() {
        let c = Chrome::of(&props(json!({"scopes": "oops", "title": "Models"})));
        assert!(c.scopes.is_empty());
        assert_eq!(c.title.as_deref(), Some("Models"));
    }

    #[test]
    fn mono_labels_dim_the_prefix_and_hits_are_bold() {
        let theme = Theme::dark();
        // "anthropic/claude" with a hit on "cla" (UTF-16 10..13).
        let runs = label_runs(&json!("anthropic/claude"), true, &[json!([10, 13])], &theme);
        let parts: Vec<(&str, bool, Option<u32>)> = runs
            .iter()
            .map(|(t, s)| (t.as_str(), s.bold, s.color))
            .collect();
        assert_eq!(
            parts,
            vec![
                ("anthropic/", false, Some(theme.text_muted)),
                ("cla", true, Some(theme.text)),
                ("ude", false, Some(theme.text)),
            ]
        );
        assert!(runs.iter().all(|(_, s)| s.mono));
    }

    #[test]
    fn hits_count_utf16_units() {
        let theme = Theme::dark();
        // "😀ab": the emoji is two UTF-16 units, so [2, 3) is "a".
        let runs = label_runs(&json!("😀ab"), false, &[json!([2, 3])], &theme);
        let bold: Vec<&str> = (runs.iter())
            .filter(|(_, s)| s.bold)
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(bold, vec!["a"]);
    }

    #[test]
    fn the_query_hits_win_over_the_item_hits() {
        let p = props(json!({"hits": {"a": [[0, 1]]}}));
        let item = json!({"id": "a", "hits": [[1, 2]]});
        assert_eq!(hits_of(&p, &item), &[json!([0, 1])]);
        let other = json!({"id": "b", "hits": [[1, 2]]});
        assert_eq!(hits_of(&p, &other), &[json!([1, 2])]);
    }

    #[test]
    fn facts_format_numbers_by_column() {
        assert_eq!(
            fact_text(&json!(1604), Some("num")).as_deref(),
            Some("1,604")
        );
        assert_eq!(
            fact_text(&json!(0.5), Some("price")).as_deref(),
            Some("$0.50")
        );
        assert_eq!(fact_text(&json!(0.5), Some("bar")), None);
        assert_eq!(fact_text(&json!("200k"), None).as_deref(), Some("200k"));
        assert_eq!(fact_text(&json!(""), None), None);
        assert_eq!(group_thousands(-1_234_567), "-1,234,567");
    }

    #[test]
    fn the_count_says_of_total_only_when_filtered() {
        assert_eq!(count_label(12, Some(1604)), "12 of 1,604");
        assert_eq!(count_label(8, Some(8)), "8");
        assert_eq!(count_label(9, Some(8)), "8", "an uncounted row (New agent)");
        assert_eq!(count_label(3, None), "3");
    }

    #[test]
    fn the_lowest_priority_column_hides_first_until_the_rest_fit() {
        let col = |priority: Option<i64>| Column {
            priority,
            ..Column::default()
        };
        // /agents: Model 43, Prewalk 23, Advisor 33, Source 13.
        let columns = [col(Some(43)), col(Some(23)), col(Some(33)), col(Some(13))];
        let widths = [50.0; 4];
        assert_eq!(
            shown_columns(&columns, &widths, 240.0, 10.0),
            [true; 4],
            "room for all"
        );
        assert_eq!(
            shown_columns(&columns, &widths, 125.0, 10.0),
            [true, false, true, false]
        );
        assert_eq!(shown_columns(&columns, &widths, 0.0, 10.0), [false; 4]);
        let unranked = [col(None), col(Some(1))];
        assert_eq!(
            shown_columns(&unranked, &[50.0, 50.0], 60.0, 10.0),
            [true, false],
            "a column without a priority hides last"
        );
    }

    #[test]
    fn a_column_is_as_wide_as_its_widest_fact() {
        let p = props(json!({"items": [
            {"id": "a", "facts": {"ctx": 1000000, "price": "$0.1·0.5"}},
            {"id": "b", "facts": {"ctx": 200000}},
            {"id": "c", "facts": {"note": "x".repeat(40)}},
        ]}));
        let col = |id: &str, head: Option<&str>, format: Option<&str>| Column {
            id: id.into(),
            head: head.map(str::to_owned),
            format: format.map(str::to_owned),
            ..Column::default()
        };
        let columns = [
            col("ctx", Some("Ctx"), Some("num")),
            col("price", Some("$/M"), Some("price")),
            col("int", Some("Intelligence"), Some("num")),
            col("note", None, None),
        ];
        // "1,000,000", "$0.1·0.5", the head alone, then capped.
        assert_eq!(column_chars(&p, &columns), vec![9, 8, 12, MAX_FACT_CHARS]);
    }

    #[test]
    fn a_press_on_the_selected_item_activates_it() {
        assert_eq!(
            item_event("p", "a", Some("b"), false),
            json!({"ev": "select", "id": "p", "item": "a"})
        );
        assert_eq!(item_event("p", "a", Some("a"), false)["ev"], "activate");
        assert_eq!(item_event("p", "a", None, true)["ev"], "activate");
        assert_eq!(
            action_event("p", "scope", Some("all")),
            json!({"ev": "action", "id": "p", "act": "scope", "value": "all"})
        );
        assert_eq!(
            action_event("p", "close", None),
            json!({"ev": "action", "id": "p", "act": "close"})
        );
    }

    #[test]
    fn keys_marks_and_disabled() {
        assert_eq!(key_label("enter"), "↵");
        assert_eq!(key_label("alt"), "Alt");
        assert_eq!(key_label("p"), "P");
        assert_eq!(initials("anthropic"), "AN");
        assert_eq!(initials("x"), "X");
        let palette = [1, 2, 3];
        assert_eq!(
            mark_color("openai", &palette),
            mark_color("openai", &palette)
        );
        assert!(is_disabled(Some(&json!("signed out"))));
        assert!(!is_disabled(Some(&json!(false))));
        assert_eq!(
            disabled_reason(Some(&json!("signed out"))),
            Some("signed out")
        );
    }
}
