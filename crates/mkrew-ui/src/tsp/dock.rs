//! The dock of omp 18.13.2 read as Tern's composer (spec 7.8): the pure model.

use mkrew_core::tsp::doc::{Document, Node};
use serde_json::Value;

use super::{
    SpanStyle, byte_at_utf16, icon, meter_tone, node_spans, rate_label, role_of, spans_of,
};
use crate::icons::Icon;
use crate::theme::{self, Theme, space};

// The dock as Tern's composer (ADR 0107, spec 7.8)

/// The roles of omp 18.13.2's dock that the composer look reads. Whole-string matches: the ask
/// card (`omp.editor.ask`, spec 7.6) is not the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockRole {
    Editor,
    Context,
    Line,
    Bar,
    Model,
    Effort,
    Extras,
    Fact,
    Usage,
    Rate,
    Send,
    Stop,
    Chips,
    Focus,
    Exit,
    HudStatus,
    Activity,
    Working,
    WorkingLabel,
    WorkingStop,
    Hud,
    HudSeg,
    HudTodo,
    /// `overlay role=omp.autocomplete`: the completion list under the editor's caret.
    Completion,
}

pub fn dock_role(role: &str) -> Option<DockRole> {
    Some(match role {
        "omp.editor" | "omp.editor.bash" | "omp.editor.python" => DockRole::Editor,
        "omp.composer.context" => DockRole::Context,
        "omp.composer.line" => DockRole::Line,
        "omp.composer.bar" => DockRole::Bar,
        "omp.composer.model" => DockRole::Model,
        "omp.composer.effort" => DockRole::Effort,
        "omp.composer.extras" => DockRole::Extras,
        "omp.composer.fact" => DockRole::Fact,
        "omp.composer.usage" => DockRole::Usage,
        "omp.composer.rate" => DockRole::Rate,
        "omp.composer.send" => DockRole::Send,
        "omp.composer.stop" => DockRole::Stop,
        "omp.composer.chips" => DockRole::Chips,
        "omp.composer.focus" => DockRole::Focus,
        "omp.composer.exit" => DockRole::Exit,
        "omp.hud.status" => DockRole::HudStatus,
        "omp.hud.activity" => DockRole::Activity,
        "omp.working" => DockRole::Working,
        "omp.working.label" => DockRole::WorkingLabel,
        "omp.working.stop" => DockRole::WorkingStop,
        "omp.hud" => DockRole::Hud,
        "omp.hud.seg" => DockRole::HudSeg,
        "omp.hud.todo" => DockRole::HudTodo,
        "omp.autocomplete" => DockRole::Completion,
        _ => return None,
    })
}

fn dock_role_of(node: &Node) -> Option<DockRole> {
    role_of(node).and_then(dock_role)
}

fn hidden(node: &Node) -> bool {
    node.props.get("hidden").and_then(Value::as_bool) == Some(true)
}

/// The first visible node of `want` under `from`, in document order; a hidden node hides its
/// subtree.
fn find<'a>(doc: &'a Document, from: &str, want: DockRole) -> Option<&'a Node> {
    doc.children(from)
        .filter(|node| !hidden(node))
        .find_map(|node| {
            if dock_role_of(node) == Some(want) {
                Some(node)
            } else {
                find(doc, &node.id, want)
            }
        })
}

/// The visible direct child of `parent` with the role `want`.
fn child<'a>(doc: &'a Document, parent: &str, want: DockRole) -> Option<&'a Node> {
    doc.children(parent)
        .find(|node| !hidden(node) && dock_role_of(node) == Some(want))
}

/// The composer's `editor` node (`omp.editor` > `omp.composer.line` > `editor`): the field Mkrew
/// edits itself (ADR 0113). `None` while omp shows another dock (the ask card).
pub fn composer_editor(doc: &Document) -> Option<&Node> {
    let editor = find(doc, "dock", DockRole::Editor)?;
    let line = child(doc, &editor.id, DockRole::Line)?;
    doc.children(&line.id)
        .find(|node| node.kind == "editor" && !hidden(node))
}

/// omp shows its completion list (slash commands, `@` files, skills): ↑ ↓ Tab Enter Esc are its.
pub fn completing(doc: &Document) -> bool {
    find(doc, "dock", DockRole::Completion).is_some()
}

/// The text of a text-like node, trimmed.
fn text_of(node: &Node, theme: &Theme) -> String {
    node_spans(&node.props, theme)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn str_of(node: &Node, key: &str) -> Option<String> {
    node.props
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// A tick on the context meter (omp's auto-compact point).
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// 0 to 1 along the track.
    pub at: f32,
    pub title: String,
}

/// `meter role=omp.composer.context`.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMeter {
    pub id: String,
    /// The used share, clamped to 0..=1; `None` before omp knows it (an empty track).
    pub fraction: Option<f32>,
    /// `accent`, `warning` (past `thresholds.warn`) or `error` (past `bad`), for `tone_color`.
    pub tone: &'static str,
    pub label: String,
    pub total: String,
    pub marks: Vec<Mark>,
    pub title: Option<String>,
}

/// The `editor` of the line: text, caret (a byte index), placeholder and ghost.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorModel {
    pub id: String,
    pub text: String,
    pub cursor: usize,
    pub placeholder: Vec<(String, SpanStyle)>,
    /// omp's inline suggestion; only with a non-empty text and the caret at its end.
    pub ghost: Option<String>,
    pub sendable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Effort {
    pub id: String,
    /// The `effort` glyph child (the view draws it through `tsp_node`).
    pub glyph: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPill {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FactBody {
    Text(String),
    /// `rate_label` of a `rate` child.
    Rate(String),
    /// The id of an `elapsed` child (it ticks on the status tick).
    Elapsed(String),
}

/// One `seg role=omp.composer.fact` of the extras.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub id: String,
    pub icon: Option<Icon>,
    pub body: FactBody,
    /// omp's drop priority: the highest number goes first. A loose `rate` has 0.
    pub priority: u32,
    /// A span styled `accent` (omp's `⚡ yolo`): the Chat permission chip look.
    pub permission: bool,
    /// The text color: Chat's own for the stats Chat shows too, else `text_muted`.
    pub color: u32,
}

/// The color of a fact's text. omp styles facts with its own `statusLine*` palette names, which
/// are not Mkrew tokens; the stats Chat's composer also shows take Chat's colors (context
/// `syntax_keyword`, branch `syntax_string`, rate `text`, clock and session `text_subtle`), any
/// other fact stays `text_muted`. `token` is the first `statusLine*` token of the fact's spans.
pub fn fact_color(id: &str, token: Option<&str>, theme: &Theme) -> u32 {
    match (id.rsplit('/').next().unwrap_or(id), token) {
        (_, Some("statusLineContext")) => theme.syntax_keyword,
        (
            _,
            Some(
                "statusLineGitClean" | "statusLineGitDirty" | "statusLineStaged"
                | "statusLineDirty",
            ),
        ) => theme.syntax_string,
        ("token_rate", _) => theme.text,
        ("time" | "session", _) => theme.text_subtle,
        _ => theme.text_muted,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub id: String,
    pub text: String,
}

/// The button at the end of the bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Primary {
    /// `None`: omp sent neither Send nor Stop (disabled Send).
    Send(Option<String>),
    Stop(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarPart {
    Effort,
    Model,
    Facts,
    Usage,
}

/// `row role=omp.composer.bar`, children in omp's order.
#[derive(Debug, Clone, PartialEq)]
pub struct BarModel {
    pub parts: Vec<BarPart>,
    pub effort: Option<Effort>,
    pub model: Option<ModelPill>,
    pub facts: Vec<Fact>,
    pub usage: Option<Usage>,
    pub primary: Primary,
    /// The session works (Stop shows or the working row exists): loading dots in the model pill.
    pub working: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Working {
    pub id: String,
    pub elapsed: Option<String>,
    pub label: String,
    pub stop: Option<String>,
}

/// A `row role=omp.hud.seg` of the hud pills.
#[derive(Debug, Clone, PartialEq)]
pub struct HudPill {
    pub id: String,
    pub icon: Option<Icon>,
    pub tone: Option<String>,
    pub count: String,
}

/// What the status row above the meter shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusModel {
    pub working: Option<Working>,
    /// The id of the `checklist` hud (drawn by `tsp_node`).
    pub todo: Option<String>,
    pub pills: Vec<HudPill>,
    /// Any other visible child of the activity row or of `omp.hud.status` (omp's retry hint):
    /// drawn generically on the left.
    pub others: Vec<String>,
}

/// `row role=omp.composer.focus`.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusStrip {
    pub id: String,
    /// Children drawn generically after the eye icon (the "Viewing" text, the crumbs).
    pub rest: Vec<String>,
    /// `(id, label)` of the `omp.composer.exit` chip.
    pub exit: Option<(String, String)>,
}

/// The dock of omp 18.13.2 read as a composer (spec 7.8). `None` for a dock without the roles
/// (another omp version, or the ask card): the dock is then drawn generically.
#[derive(Debug, Clone, PartialEq)]
pub struct DockModel {
    /// The id of `col role=omp.editor`: the card is drawn where this node stands.
    pub editor: String,
    pub line: String,
    /// Ids from the dock's child down to the editor, inclusive: wrappers to walk through.
    pub path: Vec<String>,
    /// Nodes drawn in the status row instead of where they stand (activity row, status col, hud).
    pub skip: Vec<String>,
    pub context: Option<ContextMeter>,
    pub field: EditorModel,
    pub focus: Option<FocusStrip>,
    /// Ids of the attachment chips (the row is hidden while there are none).
    pub chips: Vec<String>,
    pub bar: BarModel,
    pub status: StatusModel,
}

impl DockModel {
    /// Send is enabled when omp says `sendable` and there is text or an attachment, and the
    /// button is not Stop. (omp says `sendable: true` for an empty editor.)
    pub fn can_send(&self) -> bool {
        self.field.sendable
            && (!self.field.text.trim().is_empty() || !self.chips.is_empty())
            && !matches!(self.bar.primary, Primary::Stop(_))
    }
}

/// `⚡ yolo` is `yolo`: the Chat permission chip draws its own icon, so the leading symbol (or
/// private-use glyph) and the space after it go. Nothing left: what omp sent.
pub fn permission_text(text: &str) -> String {
    let rest = text
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim();
    if rest.is_empty() {
        text.trim().to_owned()
    } else {
        rest.to_owned()
    }
}

pub fn dock_model(doc: &Document, theme: &Theme) -> Option<DockModel> {
    let editor = find(doc, "dock", DockRole::Editor)?;
    let line = child(doc, &editor.id, DockRole::Line)?;
    let bar = child(doc, &editor.id, DockRole::Bar)?;
    let field = composer_editor(doc)?;

    let mut path = vec![editor.id.clone()];
    let mut at = editor;
    while let Some(parent) = at
        .parent
        .as_deref()
        .filter(|id| *id != "dock")
        .and_then(|id| doc.get(id))
    {
        path.push(parent.id.clone());
        at = parent;
    }
    path.reverse();

    let activity = find(doc, "dock", DockRole::Activity);
    let status_node = find(doc, "dock", DockRole::HudStatus);
    let hud = find(doc, "dock", DockRole::Hud);
    let status = status_model(doc, activity, status_node, hud, theme);
    let skip = [activity, status_node, hud]
        .into_iter()
        .flatten()
        .map(|node| node.id.clone())
        .collect();
    let mut bar = bar_model(doc, bar, theme);
    bar.working = matches!(bar.primary, Primary::Stop(_)) || status.working.is_some();

    Some(DockModel {
        editor: editor.id.clone(),
        line: line.id.clone(),
        path,
        skip,
        context: child(doc, &editor.id, DockRole::Context).map(context_model),
        field: editor_model(field, theme),
        focus: child(doc, &editor.id, DockRole::Focus).map(|row| focus_model(doc, row, theme)),
        chips: child(doc, &editor.id, DockRole::Chips)
            .map(|row| {
                doc.children(&row.id)
                    .filter(|node| !hidden(node))
                    .map(|node| node.id.clone())
                    .collect()
            })
            .unwrap_or_default(),
        bar,
        status,
    })
}

/// The dock while omp shows its ask card or a note editor where the composer stood (spec 7.6):
/// no `omp.editor`, so no composer card, but the status row (working row, todo hud, pills) still
/// reads. The other dock nodes are drawn one by one, the card among them.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusDock {
    /// Ancestors of the nodes in `skip`, below the dock: wrappers to walk through.
    pub path: Vec<String>,
    /// Nodes drawn in the status row instead of where they stand.
    pub skip: Vec<String>,
    pub status: StatusModel,
}

/// `None` when the dock holds no status at all (the generic dock draws it).
pub fn status_dock(doc: &Document, theme: &Theme) -> Option<StatusDock> {
    let activity = find(doc, "dock", DockRole::Activity);
    let status_node = find(doc, "dock", DockRole::HudStatus);
    let hud = find(doc, "dock", DockRole::Hud);
    let status = status_model(doc, activity, status_node, hud, theme);
    if status.working.is_none()
        && status.todo.is_none()
        && status.pills.is_empty()
        && status.others.is_empty()
    {
        return None;
    }
    let skip: Vec<String> = [activity, status_node, hud]
        .into_iter()
        .flatten()
        .map(|node| node.id.clone())
        .collect();
    let mut path: Vec<String> = Vec::new();
    for id in &skip {
        let mut at = doc.get(id);
        while let Some(parent) = (at.and_then(|node| node.parent.as_deref()))
            .filter(|id| *id != "dock")
            .and_then(|id| doc.get(id))
        {
            if !path.contains(&parent.id) && !skip.contains(&parent.id) {
                path.push(parent.id.clone());
            }
            at = Some(parent);
        }
    }
    Some(StatusDock { path, skip, status })
}

fn context_model(node: &Node) -> ContextMeter {
    let value = node.props.get("value").and_then(Value::as_f64);
    let marks = node
        .props
        .get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|mark| {
            Some(Mark {
                at: mark.get("at")?.as_f64()?.clamp(0.0, 1.0) as f32,
                title: mark
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect();
    ContextMeter {
        id: node.id.clone(),
        fraction: value.map(|v| v.clamp(0.0, 1.0) as f32),
        tone: value.map_or("accent", |v| meter_tone(v, node.props.get("thresholds"))),
        label: str_of(node, "label").unwrap_or_default(),
        total: str_of(node, "total").unwrap_or_default(),
        marks,
        title: str_of(node, "title"),
    }
}

fn editor_model(node: &Node, theme: &Theme) -> EditorModel {
    let text = str_of(node, "text").unwrap_or_default();
    let cursor = node
        .props
        .get("cursor")
        .and_then(Value::as_u64)
        .map_or(text.len(), |units| byte_at_utf16(&text, units as usize));
    let ghost =
        str_of(node, "ghost").filter(|g| !g.is_empty() && !text.is_empty() && cursor == text.len());
    EditorModel {
        id: node.id.clone(),
        cursor,
        placeholder: node
            .props
            .get("placeholder")
            .map(|value| spans_of(value, theme))
            .unwrap_or_default(),
        ghost,
        sendable: node.props.get("sendable").and_then(Value::as_bool) == Some(true),
        text,
    }
}

fn focus_model(doc: &Document, row: &Node, theme: &Theme) -> FocusStrip {
    let exit = child(doc, &row.id, DockRole::Exit);
    FocusStrip {
        id: row.id.clone(),
        rest: doc
            .children(&row.id)
            .filter(|node| {
                !hidden(node)
                    && Some(node.id.as_str()) != exit.map(|e| e.id.as_str())
                    && !(node.kind == "icon"
                        && node.props.get("name").and_then(Value::as_str) == Some("eye"))
            })
            .map(|node| node.id.clone())
            .collect(),
        exit: exit.map(|node| {
            let label = doc
                .children(&node.id)
                .filter(|n| n.kind == "text")
                .map(|n| text_of(n, theme))
                .collect::<Vec<_>>()
                .join(" ");
            (node.id.clone(), label)
        }),
    }
}

fn rate_of(node: &Node) -> String {
    rate_label(
        node.props
            .get("value")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        node.props.get("unit").and_then(Value::as_str).unwrap_or(""),
    )
}

/// The first `statusLine*` token among a fact's span styles.
fn status_line_token(node: &Node) -> Option<&str> {
    node.props
        .get("spans")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|span| span.get("s").and_then(Value::as_str))
        .flat_map(str::split_whitespace)
        .find(|token| token.starts_with("statusLine"))
}

fn fact_of(doc: &Document, node: &Node, theme: &Theme) -> Option<Fact> {
    if dock_role_of(node) != Some(DockRole::Fact) {
        return None;
    }
    let runs = node_spans(&node.props, theme);
    let accent = runs
        .iter()
        .any(|(text, style)| style.color == Some(theme.accent) && !text.trim().is_empty());
    let text: String = runs.iter().map(|(text, _)| text.as_str()).collect();
    let body = if let Some(rate) = doc.children(&node.id).find(|n| n.kind == "rate") {
        FactBody::Rate(rate_of(rate))
    } else if let Some(timer) = doc.children(&node.id).find(|n| n.kind == "elapsed") {
        FactBody::Elapsed(timer.id.clone())
    } else if text.trim().is_empty() {
        return None;
    } else if accent {
        FactBody::Text(permission_text(&text))
    } else {
        FactBody::Text(text.trim().to_owned())
    };
    Some(Fact {
        id: node.id.clone(),
        icon: node
            .props
            .get("icon")
            .and_then(Value::as_str)
            .and_then(icon),
        permission: accent && matches!(body, FactBody::Text(_)),
        color: if matches!(body, FactBody::Rate(_)) {
            theme.text
        } else {
            fact_color(&node.id, status_line_token(node), theme)
        },
        body,
        priority: node
            .props
            .get("priority")
            .and_then(Value::as_u64)
            .map_or(0, |p| u32::try_from(p).unwrap_or(u32::MAX)),
    })
}

fn bar_model(doc: &Document, bar: &Node, theme: &Theme) -> BarModel {
    let mut m = BarModel {
        parts: Vec::new(),
        effort: None,
        model: None,
        facts: Vec::new(),
        usage: None,
        primary: Primary::Send(None),
        working: false,
    };
    let facts_part = |m: &mut BarModel| {
        if !m.parts.contains(&BarPart::Facts) {
            m.parts.push(BarPart::Facts);
        }
    };
    for node in doc.children(&bar.id).filter(|node| !hidden(node)) {
        match (dock_role_of(node), node.kind.as_str()) {
            (Some(DockRole::Effort), _) => {
                let glyph = doc.children(&node.id).find(|n| n.kind == "effort");
                let label = doc
                    .children(&node.id)
                    .find(|n| n.kind == "text")
                    .map(|n| text_of(n, theme))
                    .filter(|text| !text.is_empty())
                    .or_else(|| glyph.and_then(|g| str_of(g, "level")))
                    .unwrap_or_default();
                m.effort = Some(Effort {
                    id: node.id.clone(),
                    glyph: glyph.map(|g| g.id.clone()),
                    label,
                });
                m.parts.push(BarPart::Effort);
            }
            (Some(DockRole::Model), _) => {
                let name = doc
                    .children(&node.id)
                    .find(|n| n.kind == "text")
                    .map(|n| text_of(n, theme))
                    .unwrap_or_default();
                if !name.is_empty() {
                    m.model = Some(ModelPill {
                        id: node.id.clone(),
                        name,
                    });
                    m.parts.push(BarPart::Model);
                }
            }
            (Some(DockRole::Extras), _) => {
                m.facts.extend(
                    doc.children(&node.id)
                        .filter(|n| !hidden(n))
                        .filter_map(|n| fact_of(doc, n, theme)),
                );
                facts_part(&mut m);
            }
            // Reference 8.2 sends the rate as a bar child; the recording sends it inside a fact.
            (Some(DockRole::Rate), _) | (None, "rate") => {
                m.facts.push(Fact {
                    id: node.id.clone(),
                    icon: None,
                    body: FactBody::Rate(rate_of(node)),
                    priority: 0,
                    permission: false,
                    color: theme.text,
                });
                facts_part(&mut m);
            }
            (Some(DockRole::Usage), _) => {
                let text = text_of(node, theme);
                if !text.is_empty() {
                    m.usage = Some(Usage {
                        id: node.id.clone(),
                        text,
                    });
                    m.parts.push(BarPart::Usage);
                }
            }
            (Some(DockRole::Stop), _) => m.primary = Primary::Stop(node.id.clone()),
            (Some(DockRole::Send), _) if !matches!(m.primary, Primary::Stop(_)) => {
                m.primary = Primary::Send(Some(node.id.clone()));
            }
            _ => {}
        }
    }
    m
}

fn collect_pills(doc: &Document, from: &str, theme: &Theme, out: &mut Vec<HudPill>) {
    for node in doc.children(from).filter(|node| !hidden(node)) {
        if dock_role_of(node) != Some(DockRole::HudSeg) {
            collect_pills(doc, &node.id, theme, out);
            continue;
        }
        let glyph = doc.children(&node.id).find(|n| n.kind == "icon");
        out.push(HudPill {
            id: node.id.clone(),
            icon: glyph
                .and_then(|g| g.props.get("name"))
                .and_then(Value::as_str)
                .and_then(icon),
            tone: glyph.and_then(|g| str_of(g, "tone")),
            // The count is the text without a role: `omp.hud.glance` is a second text.
            count: doc
                .children(&node.id)
                .filter(|n| n.kind == "text" && role_of(n).is_none())
                .map(|n| text_of(n, theme))
                .collect::<Vec<_>>()
                .join(" "),
        });
    }
}

fn status_model(
    doc: &Document,
    activity: Option<&Node>,
    status: Option<&Node>,
    hud: Option<&Node>,
    theme: &Theme,
) -> StatusModel {
    let row = status.and_then(|s| find(doc, &s.id, DockRole::Working));
    let working = row.map(|row| Working {
        id: row.id.clone(),
        elapsed: doc
            .children(&row.id)
            .find(|n| n.kind == "elapsed")
            .map(|n| n.id.clone()),
        label: child(doc, &row.id, DockRole::WorkingLabel)
            .map(|n| text_of(n, theme))
            .unwrap_or_default(),
        stop: child(doc, &row.id, DockRole::WorkingStop).map(|n| n.id.clone()),
    });
    let mut others: Vec<String> = Vec::new();
    if let Some(status) = status {
        others.extend(
            doc.children(&status.id)
                .filter(|n| {
                    !hidden(n)
                        && n.kind != "checklist"
                        && Some(n.id.as_str()) != row.map(|r| r.id.as_str())
                })
                .map(|n| n.id.clone()),
        );
    }
    if let Some(activity) = activity {
        others.extend(
            doc.children(&activity.id)
                .filter(|n| !hidden(n) && Some(n.id.as_str()) != status.map(|s| s.id.as_str()))
                .map(|n| n.id.clone()),
        );
    }
    let mut pills = Vec::new();
    if let Some(hud) = hud {
        collect_pills(doc, &hud.id, theme, &mut pills);
    }
    StatusModel {
        working,
        todo: status
            .and_then(|s| {
                doc.children(&s.id)
                    .find(|n| n.kind == "checklist" && !hidden(n))
            })
            .map(|n| n.id.clone()),
        pills,
        others,
    }
}

/// What of the bar fits: which facts show (indexes of `BarModel.facts`), whether the effort chip
/// shows, and, when even that is too wide, the room (px at 13 px UI size) the model name gets.
#[derive(Debug, Clone, PartialEq)]
pub struct BarFit {
    pub facts: Vec<bool>,
    pub effort: bool,
    pub usage: bool,
    pub model_name: Option<f32>,
}

impl BarFit {
    pub fn all(bar: &BarModel) -> Self {
        Self {
            facts: vec![true; bar.facts.len()],
            effort: bar.effort.is_some(),
            usage: bar.usage.is_some(),
            model_name: None,
        }
    }
}

fn text_room(text: &str) -> f32 {
    text.chars().count() as f32 * theme::COMPOSER_MONO_CHAR_WIDTH
}

/// The room of UI-font text (the effort label, the model name).
fn ui_room(text: &str) -> f32 {
    text.chars().count() as f32 * theme::TSP_UI_CHAR_WIDTH
}

fn fact_width(fact: &Fact) -> f32 {
    let icon = if fact.icon.is_some() || fact.permission {
        theme::ICON_SIZE_SMALL + space::XS
    } else {
        0.0
    };
    let body = match &fact.body {
        FactBody::Text(text) | FactBody::Rate(text) => text_room(text),
        FactBody::Elapsed(_) => theme::TSP_ELAPSED_CHARS as f32 * theme::COMPOSER_MONO_CHAR_WIDTH,
    };
    icon + body + space::S
}

/// The room the bar takes (px at 13 px UI size), every chip `flex_none` and the stats cell
/// shrinking last, like `chat::chips_width`.
pub fn bar_width(bar: &BarModel, fit: &BarFit) -> f32 {
    let effort = bar.effort.as_ref().filter(|_| fit.effort).map_or(0.0, |e| {
        theme::TSP_EFFORT_GLYPH_WIDTH + space::XS + ui_room(&e.label) + space::S
    });
    let model = bar.model.as_ref().map_or(0.0, |m| {
        theme::TSP_MODEL_CHROME_WIDTH
            + fit.model_name.unwrap_or_else(|| ui_room(&m.name))
            + if bar.working {
                theme::COMPOSER_CHIP_LOADING_WIDTH
            } else {
                0.0
            }
    });
    let facts: f32 = bar
        .facts
        .iter()
        .zip(&fit.facts)
        .filter(|(_, shown)| **shown)
        .map(|(fact, _)| fact_width(fact))
        .sum();
    let usage = bar
        .usage
        .as_ref()
        .filter(|_| fit.usage)
        .map_or(0.0, |u| text_room(&u.text) + space::S);
    theme::TSP_BAR_FIXED_WIDTH + effort + model + facts + usage
}

/// What of `bar` fits in `room` px (13 px UI size): facts drop highest priority number first
/// (of equal priorities the later one first), then the effort chip, then the cost, then the model name
/// truncates down to `TSP_MODEL_MIN_CHARS`. The send or stop button always stays. Mirrors
/// `chat::fit_chips`.
pub fn fit_bar(bar: &BarModel, room: f32) -> BarFit {
    let mut fit = BarFit::all(bar);
    let mut order: Vec<usize> = (0..bar.facts.len()).collect();
    order.sort_by_key(|&i| {
        (
            std::cmp::Reverse(bar.facts[i].priority),
            std::cmp::Reverse(i),
        )
    });
    for i in order {
        if bar_width(bar, &fit) <= room {
            return fit;
        }
        fit.facts[i] = false;
    }
    if bar_width(bar, &fit) <= room {
        return fit;
    }
    fit.effort = false;
    if bar_width(bar, &fit) <= room {
        return fit;
    }
    fit.usage = false;
    if bar_width(bar, &fit) <= room {
        return fit;
    }
    if let Some(model) = &bar.model {
        let over = bar_width(bar, &fit) - room;
        let natural = ui_room(&model.name);
        let floor = theme::TSP_MODEL_MIN_CHARS as f32 * theme::TSP_UI_CHAR_WIDTH;
        fit.model_name = Some((natural - over).max(floor).min(natural));
    }
    fit
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::theme::Theme;
    use crate::tsp::tone_color;

    fn doc(ops: Vec<Value>) -> Document {
        let mut d = Document::new("s:1");
        let applied = d.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        d
    }

    // ---- The dock as Tern's composer (Task 10b, spec 7.8) ----

    fn fact(id: &str, priority: u32, icon: Option<&str>, spans: Value, children: Value) -> Value {
        let mut props = json!({"role": "omp.composer.fact", "priority": priority, "spans": spans});
        if let Some(icon) = icon {
            props["icon"] = json!(icon);
        }
        json!({"id": format!("3.bar/extras/{id}"), "k": "seg", "p": props, "c": children})
    }

    /// The dock children of the recorded omp 18.13.2 session: `[e, 6, d, h]`. Idle is frame 4
    /// (3 facts, Send, empty usage), working is frame 25 (9 facts with the rate and the elapsed
    /// time, Stop, `$0.10`, the `omp.working` row). Paths used by the tests below:
    /// `kids[3]` is `h`, `/c/0` the editor col, `/c/0/c/0` the hidden chips row, `/c/0/c/1` the
    /// context meter, `/c/0/c/2` the line, `/c/0/c/2/c/0` the editor, `/c/0/c/3` the bar.
    fn real_children(working: bool) -> Vec<Value> {
        let facts = if working {
            vec![
                fact(
                    "token_rate",
                    6,
                    Some("throughput"),
                    json!([]),
                    json!([{"id": "3.bar/extras/token_rate/rate", "k": "rate", "p": {"value": 45.6, "unit": "tok/s"}}]),
                ),
                fact(
                    "cache_write",
                    5,
                    Some("cache"),
                    json!([{"t": "12K", "s": "statusLineOutput"}]),
                    json!([]),
                ),
                fact(
                    "cache_read",
                    4,
                    Some("cache"),
                    json!([{"t": "8.9K", "s": "statusLineSpend"}]),
                    json!([]),
                ),
                fact(
                    "token_out",
                    3,
                    Some("output"),
                    json!([{"t": "325", "s": "statusLineOutput"}]),
                    json!([]),
                ),
                fact(
                    "token_in",
                    2,
                    Some("input"),
                    json!([{"t": "1", "s": "statusLineSpend"}]),
                    json!([]),
                ),
                fact(
                    "time_spent",
                    10,
                    Some("time"),
                    json!([]),
                    json!([{"id": "3.bar/extras/time_spent/timer", "k": "elapsed", "p": {"age": 12164, "format": "short"}}]),
                ),
                fact(
                    "status",
                    6,
                    None,
                    json!([{"t": "⚡ yolo", "s": "accent"}]),
                    json!([]),
                ),
                fact(
                    "session",
                    2,
                    Some("session"),
                    json!([{"t": "01a20b2a"}]),
                    json!([]),
                ),
                fact(
                    "time",
                    11,
                    Some("time"),
                    json!([{"t": "8:28:43"}]),
                    json!([]),
                ),
            ]
        } else {
            vec![
                fact(
                    "status",
                    6,
                    None,
                    json!([{"t": "⚡ yolo", "s": "accent"}]),
                    json!([]),
                ),
                fact(
                    "session",
                    2,
                    Some("session"),
                    json!([{"t": "01a20b2a"}]),
                    json!([]),
                ),
                fact(
                    "time",
                    11,
                    Some("time"),
                    json!([{"t": "8:27:18"}]),
                    json!([]),
                ),
            ]
        };
        let (cost, primary) = if working {
            (
                "$0.10",
                json!({"id": "3.bar/stop", "k": "text", "p": {"role": "omp.composer.stop", "text": "Stop", "tone": "error", "title": "Stop  esc", "actions": {"click": "interrupt"}}}),
            )
        } else {
            (
                "",
                json!({"id": "3.bar/send", "k": "kbd", "p": {"role": "omp.composer.send", "keys": ["enter"], "title": "Send  ⏎", "actions": {"click": "submit"}}}),
            )
        };
        let effort = json!({"id": "3.bar/effort", "k": "row", "p": {"role": "omp.composer.effort", "gap": "xs", "align": "center", "title": "Thinking effort  ⇧⇥", "actions": {"click": "thinking.cycle"}}, "c": [
            {"id": "3.bar/effort/glyph", "k": "effort", "p": {"level": "high"}},
            {"id": "3.bar/effort/level", "k": "text", "p": {"text": "high", "wrap": "none"}}]});
        // omp restyles the name while working (`statusLineModel` is one of its palette names).
        let name_style = if working { "statusLineModel" } else { "accent" };
        let model = json!({"id": "3.bar/model", "k": "row", "p": {"role": "omp.composer.model", "gap": "xs", "align": "center", "title": "Switch model  Alt+P", "actions": {"click": "status.model"}}, "c": [
            {"id": "3.bar/model/icon", "k": "icon", "p": {"name": "model"}},
            {"id": "3.bar/model/name", "k": "text", "p": {"spans": [{"t": "Opus 5.5", "s": name_style}], "wrap": "none"}},
            {"id": "3.bar/model/chev", "k": "icon", "p": {"name": "chev"}}]});
        let extras = json!({"id": "3.bar/extras", "k": "status", "p": {"role": "omp.composer.extras", "transparent": true, "grow": 1}, "c": facts});
        let usage = json!({"id": "3.bar/usage", "k": "text", "p": {"role": "omp.composer.usage", "text": cost, "wrap": "none", "actions": {"click": "status.cost"}}});
        let editor = json!({"id": "3", "k": "col", "p": {"role": "omp.editor"}, "c": [
            {"id": "m", "k": "row", "p": {"gap": "sm", "wrap": true, "role": "omp.composer.chips", "hidden": true}},
            {"id": "3.context", "k": "meter", "p": {"role": "omp.composer.context", "style": "bar", "thresholds": {"warn": 0.15, "bad": 0.5}, "marks": [{"at": 0.5, "icon": "context.compaction", "title": "Auto-compact at 50%"}], "total": "1M", "title": "Context 3% used\n27K of 1M tokens\nAuto-compact at 50%", "actions": {"click": "status.context"}, "value": 0.027201, "label": "3%"}},
            {"id": "3.line", "k": "row", "p": {"role": "omp.composer.line", "align": "start", "gap": "sm"}, "c": [
                {"id": "3.line/input", "k": "editor", "p": {"text": "", "cursor": 0, "sendable": true, "maxLines": 18, "placeholder": [{"t": "“\u{f14f7} README: List files and explain README”", "s": "em"}]}}]},
            {"id": "3.bar", "k": "row", "p": {"role": "omp.composer.bar", "gap": "sm", "align": "center"}, "c": [effort, model, extras, usage, primary]}]});
        let activity = if working {
            json!({"id": "e", "k": "row", "p": {"role": "omp.hud.activity", "align": "center", "gap": "sm"}, "c": [
                {"id": "e.status", "k": "col", "p": {"role": "omp.hud.status"}, "c": [
                    {"id": "x", "k": "row", "p": {"role": "omp.working", "align": "center", "gap": "sm"}, "c": [
                        {"id": "x.spinner", "k": "spinner", "p": {"style": "starburst", "tone": "accent"}},
                        {"id": "x.elapsed", "k": "elapsed", "p": {"age": 12261, "format": "short"}},
                        {"id": "x.sep", "k": "text", "p": {"text": "·", "role": "omp.working.sep"}},
                        {"id": "x.label", "k": "shimmer", "p": {"spans": [{"t": "Reading simple-english skill", "s": "accent"}], "mode": "classic", "role": "omp.working.label"}},
                        {"id": "x.fill", "k": "row", "p": {"grow": 1}},
                        {"id": "x.stop", "k": "row", "p": {"role": "omp.working.stop", "gap": "xs", "align": "center", "title": "Stop  esc", "actions": {"click": "interrupt"}}, "c": [
                            {"id": "x.stop/0", "k": "kbd", "p": {"keys": ["escape"]}},
                            {"id": "x.stop/1", "k": "text", "p": {"text": "Stop"}}]}]}]}]})
        } else {
            json!({"id": "e", "k": "col"})
        };
        vec![
            activity,
            json!({"id": "6", "k": "row", "p": {"role": "omp.hud", "justify": "end", "hidden": true}}),
            json!({"id": "d", "k": "col", "c": [{"id": "j", "k": "col", "p": {"role": "omp.hud.judge"}}]}),
            json!({"id": "h", "k": "col", "c": [editor]}),
        ]
    }

    fn dock_with(kids: Vec<Value>) -> Document {
        let mut ops = vec![json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col"}])];
        ops.extend(
            kids.into_iter()
                .map(|kid| json!(["add", kid["id"].clone(), "dock", null, kid])),
        );
        doc(ops)
    }

    fn model_of(kids: Vec<Value>) -> Option<DockModel> {
        dock_model(&dock_with(kids), &Theme::dark())
    }

    #[test]
    fn dock_roles_are_matched_whole_and_the_ask_card_is_not_the_composer() {
        assert_eq!(dock_role("omp.editor"), Some(DockRole::Editor));
        assert_eq!(dock_role("omp.editor.bash"), Some(DockRole::Editor));
        assert_eq!(dock_role("omp.editor.python"), Some(DockRole::Editor));
        assert_eq!(dock_role("omp.editor.ask"), None);
        assert_eq!(dock_role("omp.editor.ask.hidden"), None);
        assert_eq!(dock_role("omp.composer.model"), Some(DockRole::Model));
        assert_eq!(dock_role("omp.composer.model.effort"), None);
        assert_eq!(dock_role("omp.composer.effort"), Some(DockRole::Effort));
        assert_eq!(dock_role("omp.composer.fact"), Some(DockRole::Fact));
        assert_eq!(dock_role("omp.hud"), Some(DockRole::Hud));
        assert_eq!(dock_role("omp.hud.seg"), Some(DockRole::HudSeg));
        assert_eq!(dock_role("omp.hud.todo"), Some(DockRole::HudTodo));
        assert_eq!(dock_role("omp.hud.pill"), None);
        assert_eq!(dock_role("omp.working.sep"), None);
        assert_eq!(dock_role("omp.composer"), None);
        assert_eq!(dock_role(""), None);
    }

    /// The dock of `real_children(true)` with the ask card where the editor col stood.
    fn ask_children() -> Vec<Value> {
        let mut kids = real_children(true);
        kids[3] = json!({"id": "h", "k": "col", "c": [
            {"id": "1l", "k": "col", "p": {"role": "omp.editor.ask", "gap": "md"}, "c": [
                {"id": "1l.question", "k": "md", "p": {"text": "Pick a color", "role": "omp.ask.question"}}
            ]}
        ]});
        kids
    }

    #[test]
    fn an_ask_card_in_place_of_the_editor_keeps_the_status_row() {
        let doc = dock_with(ask_children());
        assert!(
            dock_model(&doc, &Theme::dark()).is_none(),
            "no editor, no composer card"
        );
        let d = status_dock(&doc, &Theme::dark()).expect("the working row is still there");
        let working = d.status.working.as_ref().expect("working row");
        assert_eq!(working.label, "Reading simple-english skill");
        assert_eq!(working.elapsed.as_deref(), Some("x.elapsed"));
        assert_eq!(d.skip, ["e", "e.status"], "the hud row is hidden");
        assert!(d.path.is_empty(), "the wrapper is in `skip` already");
        let calm = dock_with(vec![
            json!({"id": "e", "k": "col"}),
            ask_children().remove(3),
        ]);
        assert!(
            status_dock(&calm, &Theme::dark()).is_none(),
            "nothing to show"
        );
    }

    #[test]
    fn the_recorded_idle_dock_gives_the_composer_model() {
        let m = model_of(real_children(false)).expect("composer");
        assert_eq!((m.editor.as_str(), m.line.as_str()), ("3", "3.line"));
        assert_eq!(m.path, ["h", "3"], "the wrapper col the editor sits in");
        assert!(
            m.skip.is_empty(),
            "idle: no activity row, the hud row is hidden"
        );

        let c = m.context.as_ref().expect("context meter");
        assert_eq!(c.id, "3.context");
        assert!((c.fraction.expect("value") - 0.027201).abs() < 1e-6);
        assert_eq!(
            (c.tone, c.label.as_str(), c.total.as_str()),
            ("accent", "3%", "1M")
        );
        assert_eq!(
            c.marks,
            [Mark {
                at: 0.5,
                title: "Auto-compact at 50%".into()
            }]
        );
        assert!(c.title.as_deref().is_some_and(|t| t.contains("27K of 1M")));

        assert_eq!(m.field.id, "3.line/input");
        assert_eq!(m.field.text, "");
        let placeholder: String = m
            .field
            .placeholder
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
        assert!(
            placeholder.contains('\u{f14f7}') && placeholder.ends_with("README”"),
            "{placeholder}"
        );
        assert!(m.field.placeholder[0].1.italic, "the em span");
        assert!(m.field.ghost.is_none());
        assert!(
            m.field.sendable && !m.can_send(),
            "sendable is true when empty; the text decides"
        );

        let bar = &m.bar;
        assert_eq!(
            bar.parts,
            [BarPart::Effort, BarPart::Model, BarPart::Facts],
            "an empty usage is no part"
        );
        let effort = bar.effort.as_ref().expect("effort");
        assert_eq!(
            (effort.id.as_str(), effort.label.as_str()),
            ("3.bar/effort", "high")
        );
        assert_eq!(effort.glyph.as_deref(), Some("3.bar/effort/glyph"));
        let model = bar.model.as_ref().expect("model");
        assert_eq!(
            (model.id.as_str(), model.name.as_str()),
            ("3.bar/model", "Opus 5.5")
        );
        let texts: Vec<_> = bar.facts.iter().map(|f| &f.body).collect();
        assert_eq!(
            texts,
            [
                &FactBody::Text("yolo".into()),
                &FactBody::Text("01a20b2a".into()),
                &FactBody::Text("8:27:18".into())
            ]
        );
        assert_eq!(
            bar.facts.iter().map(|f| f.priority).collect::<Vec<_>>(),
            [6, 2, 11]
        );
        assert_eq!(
            bar.facts.iter().map(|f| f.permission).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(bar.facts[1].icon, Some(Icon::SquareTerminal));
        assert_eq!(bar.facts[2].icon, Some(Icon::Clock));
        assert!(bar.usage.is_none(), "an empty usage text is not a chip");
        assert_eq!(bar.primary, Primary::Send(Some("3.bar/send".into())));
        assert!(!bar.working);

        assert!(m.status.working.is_none() && m.status.todo.is_none());
        assert!(m.status.pills.is_empty() && m.status.others.is_empty());
        assert!(m.chips.is_empty() && m.focus.is_none());
    }

    #[test]
    fn the_recorded_working_dock_swaps_send_for_stop_and_reads_rate_elapsed_and_the_working_row() {
        let m = model_of(real_children(true)).expect("composer");
        assert_eq!(m.skip, ["e", "e.status"], "drawn in the status row instead");
        let bar = &m.bar;
        assert_eq!(bar.primary, Primary::Stop("3.bar/stop".into()));
        assert!(bar.working, "the model pill shows the loading dots");
        assert_eq!(
            bar.facts.iter().map(|f| f.priority).collect::<Vec<_>>(),
            [6, 5, 4, 3, 2, 10, 6, 2, 11],
            "omp's order, not sorted"
        );
        assert_eq!(bar.facts[0].body, FactBody::Rate("46 tok/s".into()));
        assert_eq!(bar.facts[0].icon, None, "`throughput` has no Mkrew icon");
        assert_eq!(bar.facts[1].icon, Some(Icon::Database), "cache");
        assert_eq!(bar.facts[3].icon, Some(Icon::ArrowUp), "output");
        assert_eq!(bar.facts[4].icon, Some(Icon::ArrowDown), "input");
        assert_eq!(
            bar.facts[5].body,
            FactBody::Elapsed("3.bar/extras/time_spent/timer".into())
        );
        assert_eq!(bar.usage.as_ref().map(|u| u.text.as_str()), Some("$0.10"));
        assert!(!m.can_send(), "Stop shows, there is nothing to send");

        let w = m.status.working.as_ref().expect("working row");
        assert_eq!(w.id, "x");
        assert_eq!(w.elapsed.as_deref(), Some("x.elapsed"));
        assert_eq!(w.label, "Reading simple-english skill");
        assert_eq!(w.stop.as_deref(), Some("x.stop"));
        assert!(
            m.status.others.is_empty(),
            "the working row is not drawn twice"
        );
        assert!(m.status.todo.is_none() && m.status.pills.is_empty());
    }

    fn with_editor(text: &str, cursor: u64, ghost: Option<&str>) -> Option<DockModel> {
        let mut kids = real_children(false);
        let props = kids[3].pointer_mut("/c/0/c/2/c/0/p").expect("editor props");
        props["text"] = json!(text);
        props["cursor"] = json!(cursor);
        if let Some(ghost) = ghost {
            props["ghost"] = json!(ghost);
        }
        model_of(kids)
    }

    #[test]
    fn typing_gives_text_caret_ghost_and_enables_send() {
        let m = with_editor("abc", 3, Some("d")).expect("composer");
        assert_eq!((m.field.text.as_str(), m.field.cursor), ("abc", 3));
        assert_eq!(m.field.ghost.as_deref(), Some("d"), "recorded at frame 12");
        assert!(m.can_send());
        // The cursor counts UTF-16 units; the model holds a byte index: `a😀b`, caret after 😀.
        let m = with_editor("a😀b", 3, None).expect("composer");
        assert_eq!(m.field.cursor, 5);
        // A ghost belongs after the caret at the end of the text only.
        assert!(
            with_editor("abc", 1, Some("d"))
                .expect("m")
                .field
                .ghost
                .is_none()
        );
        assert!(
            with_editor("", 0, Some("d"))
                .expect("m")
                .field
                .ghost
                .is_none()
        );
        assert!(
            with_editor("abc", 3, Some(""))
                .expect("m")
                .field
                .ghost
                .is_none()
        );
        // Blank text sends nothing.
        assert!(!with_editor("  \n ", 4, None).expect("m").can_send());
    }

    #[test]
    fn send_needs_omp_to_say_sendable_and_text_or_an_attachment() {
        let mut kids = real_children(false);
        kids[3].pointer_mut("/c/0/c/0/p").expect("chips")["hidden"] = json!(false);
        kids[3].pointer_mut("/c/0/c/0").expect("chips")["c"] = json!([{"id": "m/1", "k": "card", "p": {"role": "omp.composer.chip", "tone": "accent", "head": "#1"}}]);
        let m = model_of(kids).expect("composer");
        assert_eq!(
            m.chips,
            ["m/1"],
            "reference 8.2: attachment chips above the field"
        );
        assert!(m.can_send(), "an attachment alone can be sent");
        let mut kids = real_children(false);
        kids[3].pointer_mut("/c/0/c/2/c/0/p").expect("editor")["sendable"] = json!(false);
        kids[3].pointer_mut("/c/0/c/2/c/0/p").expect("editor")["text"] = json!("abc");
        assert!(!model_of(kids).expect("composer").can_send());
        // A bar with neither Send nor Stop still gets a (disabled) Send.
        let mut kids = real_children(false);
        kids[3]
            .pointer_mut("/c/0/c/3/c")
            .expect("bar")
            .as_array_mut()
            .expect("array")
            .pop();
        assert_eq!(
            model_of(kids).expect("composer").bar.primary,
            Primary::Send(None)
        );
    }

    #[test]
    fn the_context_meter_reads_value_thresholds_marks_and_labels() {
        let at = |value: Option<f64>| {
            let mut kids = real_children(false);
            let props = kids[3].pointer_mut("/c/0/c/1/p").expect("meter props");
            match value {
                Some(v) => props["value"] = json!(v),
                None => {
                    props.as_object_mut().expect("object").remove("value");
                }
            }
            model_of(kids).expect("composer").context.expect("meter")
        };
        assert_eq!(at(Some(0.014)).tone, "accent");
        assert_eq!(at(Some(0.15)).tone, "warning", "warn = status_needs_input");
        assert_eq!(at(Some(0.49)).tone, "warning");
        assert_eq!(at(Some(0.5)).tone, "error", "bad = danger_text");
        let full = at(Some(1.7));
        assert_eq!(
            (full.fraction, full.tone),
            (Some(1.0), "error"),
            "clamped to the track"
        );
        let unknown = at(None);
        assert_eq!(
            unknown.fraction, None,
            "the first frame has no value: an empty track"
        );
        assert_eq!(
            (unknown.tone, unknown.label.as_str(), unknown.total.as_str()),
            ("accent", "3%", "1M")
        );
        let t = Theme::dark();
        assert_eq!(tone_color(at(Some(0.15)).tone, &t), t.status_needs_input);
        assert_eq!(tone_color(at(Some(0.6)).tone, &t), t.danger_text);
        assert_eq!(tone_color(at(Some(0.01)).tone, &t), t.accent);
    }

    #[test]
    fn a_narrow_bar_drops_facts_by_priority_then_effort_then_truncates_the_model() {
        let m = model_of(real_children(true)).expect("composer");
        let bar = &m.bar;
        let all = BarFit::all(bar);
        let natural = bar_width(bar, &all);
        assert_eq!(fit_bar(bar, natural), all, "fits as it is");
        assert_eq!(fit_bar(bar, 10_000.0), all);
        // Shrink the room 1 px at a time: facts leave highest priority number first; of two
        // equal priorities the later one leaves first; then the effort chip; then the model.
        let mut dropped: Vec<usize> = Vec::new();
        let mut effort_gone_after = None;
        let mut usage_gone_with_effort = None;
        let mut room = natural;
        while room > 0.0 {
            room -= 1.0;
            let fit = fit_bar(bar, room);
            for (i, shown) in fit.facts.iter().enumerate() {
                if !shown && !dropped.contains(&i) {
                    dropped.push(i);
                }
            }
            if !fit.effort && effort_gone_after.is_none() {
                effort_gone_after = Some(dropped.len());
            }
            if fit.model_name.is_some() {
                assert!(
                    !fit.effort && !fit.usage && fit.facts.iter().all(|shown| !shown),
                    "the model truncates last"
                );
            }
            if !fit.usage && usage_gone_with_effort.is_none() {
                usage_gone_with_effort = Some(fit.effort);
            }
        }
        // time 11, time_spent 10, status 6 (later), token_rate 6, cache_write 5, cache_read 4,
        // token_out 3, session 2 (later), token_in 2.
        assert_eq!(dropped, [8, 5, 6, 0, 1, 2, 3, 7, 4]);
        assert_eq!(
            effort_gone_after,
            Some(9),
            "the effort chip goes after the last fact"
        );
        assert_eq!(
            usage_gone_with_effort,
            Some(false),
            "the cost goes after the effort chip, before the model name truncates"
        );

        let bare = BarFit {
            facts: vec![false; bar.facts.len()],
            effort: false,
            usage: false,
            model_name: None,
        };
        let room = bar_width(bar, &bare) - 5.0;
        let fit = fit_bar(bar, room);
        assert!(fit.model_name.is_some());
        assert!(
            (bar_width(bar, &fit) - room).abs() < 0.01,
            "the name gives back exactly the overflow"
        );
        let floor = theme::TSP_MODEL_MIN_CHARS as f32 * theme::TSP_UI_CHAR_WIDTH;
        assert_eq!(
            fit_bar(bar, 0.0).model_name,
            Some(floor),
            "never below the floor"
        );
    }

    #[test]
    fn working_costs_the_model_pill_the_loading_dots() {
        let idle = model_of(real_children(false)).expect("composer");
        let mut working = idle.bar.clone();
        working.working = true;
        let fit = BarFit::all(&idle.bar);
        assert!(
            (bar_width(&working, &fit)
                - bar_width(&idle.bar, &fit)
                - theme::COMPOSER_CHIP_LOADING_WIDTH)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn a_dock_without_the_composer_roles_is_not_a_composer() {
        // Another omp version: an editor with no roles at all.
        let plain =
            json!({"id": "a", "k": "col", "c": [{"id": "b", "k": "editor", "p": {"text": "x"}}]});
        assert!(model_of(vec![plain]).is_none());
        // The ask card replaces the editor: Task 10's `tsp_ask` draws it.
        let mut kids = real_children(false);
        kids[3].pointer_mut("/c/0/p").expect("editor col")["role"] = json!("omp.editor.ask");
        assert!(model_of(kids).is_none());
        // A hidden editor, no line, no bar, no editor node in the line.
        let mut kids = real_children(false);
        kids[3].pointer_mut("/c/0/p").expect("editor col")["hidden"] = json!(true);
        assert!(model_of(kids).is_none());
        for (index, what) in [(2, "line"), (3, "bar")] {
            let mut kids = real_children(false);
            kids[3]
                .pointer_mut("/c/0/c")
                .expect("children")
                .as_array_mut()
                .expect("array")
                .remove(index);
            assert!(model_of(kids).is_none(), "no {what}");
        }
        let mut kids = real_children(false);
        kids[3]
            .pointer_mut("/c/0/c/2/c")
            .expect("line children")
            .as_array_mut()
            .expect("array")
            .clear();
        assert!(model_of(kids).is_none(), "no editor in the line");
        // No dock children at all.
        assert!(model_of(Vec::new()).is_none());
    }

    #[test]
    fn hud_pills_come_from_the_visible_hud_row_per_the_reference() {
        // Reference 8.3 (the recording never shows the pills): row omp.hud > row omp.hud.pill >
        // row omp.hud.seg [icon, text N].
        let mut kids = real_children(false);
        kids[1] = json!({"id": "6", "k": "row", "p": {"role": "omp.hud", "justify": "end"}, "c": [
            {"id": "6.pill", "k": "row", "p": {"role": "omp.hud.pill"}, "c": [
                {"id": "6.agents", "k": "row", "p": {"role": "omp.hud.seg", "gap": "xs", "align": "center", "title": "2 agents running  Ctrl+B", "actions": {"click": "agents.open"}}, "c": [
                    {"id": "6.agents/i", "k": "icon", "p": {"name": "users", "tone": "accent"}},
                    {"id": "6.agents/n", "k": "text", "p": {"spans": [{"t": "2", "s": "num"}], "wrap": "none"}}]},
                {"id": "6.jobs", "k": "row", "p": {"role": "omp.hud.seg", "title": "1 job running  /jobs", "actions": {"click": "jobs.open"}}, "c": [
                    {"id": "6.jobs/i", "k": "icon", "p": {"name": "job", "tone": "warning"}},
                    {"id": "6.jobs/n", "k": "text", "p": {"text": "1"}},
                    {"id": "6.jobs/g", "k": "text", "p": {"role": "omp.hud.glance", "text": "build"}}]}]}]});
        let m = model_of(kids).expect("composer");
        assert_eq!(m.skip, ["6"]);
        let pills: Vec<_> = m
            .status
            .pills
            .iter()
            .map(|p| (p.id.as_str(), p.icon, p.tone.as_deref(), p.count.as_str()))
            .collect();
        assert_eq!(
            pills,
            [
                ("6.agents", Some(Icon::Users), Some("accent"), "2"),
                ("6.jobs", Some(Icon::Zap), Some("warning"), "1"),
            ],
            "the glance text is not the count"
        );
    }

    #[test]
    fn a_status_node_with_a_todo_and_other_rows_keeps_them_for_the_status_row() {
        let mut kids = real_children(true);
        kids[0].pointer_mut("/c/0/c").expect("status children").as_array_mut().expect("array").extend([
            json!({"id": "1k", "k": "row", "p": {"role": "omp.hint.retry"}}),
            json!({"id": "tt", "k": "checklist", "p": {"mode": "hud", "role": "omp.hud.todo", "phases": []}}),
        ]);
        let m = model_of(kids).expect("composer");
        assert_eq!(m.status.todo.as_deref(), Some("tt"));
        assert_eq!(m.status.others, ["1k"]);
        assert!(m.status.working.is_some());
    }

    #[test]
    fn the_focus_strip_follows_the_reference() {
        // Reference 8.2: row omp.composer.focus [icon eye, "Viewing", crumbs, row omp.composer.exit [kbd, "main"]].
        let mut kids = real_children(false);
        kids[3].pointer_mut("/c/0/c").expect("children").as_array_mut().expect("array").insert(
            1,
            json!({"id": "f", "k": "row", "p": {"role": "omp.composer.focus", "title": "Viewing subagent 0-Foo: what you send goes to it"}, "c": [
                {"id": "f/eye", "k": "icon", "p": {"name": "eye"}},
                {"id": "f/t", "k": "text", "p": {"text": "Viewing"}},
                {"id": "f/crumb", "k": "text", "p": {"text": "0-Foo", "actions": {"click": "focus:0-Foo"}}},
                {"id": "f/exit", "k": "row", "p": {"role": "omp.composer.exit", "title": "Back to the main session  Ctrl+X", "actions": {"click": "focus:main"}}, "c": [
                    {"id": "f/exit/k", "k": "kbd", "p": {"keys": ["ctrl+x"]}},
                    {"id": "f/exit/t", "k": "text", "p": {"text": "main"}}]}]}),
        );
        let f = model_of(kids)
            .expect("composer")
            .focus
            .expect("focus strip");
        assert_eq!(f.id, "f");
        assert_eq!(
            f.rest,
            ["f/t", "f/crumb"],
            "the eye icon and the exit chip are drawn by the view"
        );
        assert_eq!(f.exit, Some(("f/exit".to_owned(), "main".to_owned())));
    }

    #[test]
    fn a_leading_symbol_is_not_part_of_the_permission_text() {
        assert_eq!(permission_text("⚡ yolo"), "yolo");
        assert_eq!(permission_text("\u{f0e7} on-request"), "on-request");
        assert_eq!(permission_text("yolo"), "yolo");
        assert_eq!(
            permission_text("⚡"),
            "⚡",
            "nothing left: keep what omp sent"
        );
    }

    #[test]
    fn dock_fact_icons_resolve() {
        assert_eq!(icon("cache"), Some(Icon::Database));
        assert_eq!(icon("input"), Some(Icon::ArrowDown));
        assert_eq!(icon("output"), Some(Icon::ArrowUp));
        assert_eq!(icon("throughput"), None);
    }

    #[test]
    fn facts_the_chat_composer_shows_take_chats_colors() {
        let t = Theme::dark();
        // The pure table: context, branch (omp's git tokens), rate, clock and session.
        assert_eq!(
            fact_color("3.bar/extras/ctx", Some("statusLineContext"), &t),
            t.syntax_keyword
        );
        for token in [
            "statusLineGitClean",
            "statusLineGitDirty",
            "statusLineStaged",
            "statusLineDirty",
        ] {
            assert_eq!(
                fact_color("3.bar/extras/git", Some(token), &t),
                t.syntax_string,
                "{token}"
            );
        }
        assert_eq!(fact_color("3.bar/extras/token_rate", None, &t), t.text);
        assert_eq!(fact_color("3.bar/extras/time", None, &t), t.text_subtle);
        assert_eq!(fact_color("3.bar/extras/session", None, &t), t.text_subtle);
        // Any other omp palette name, and plain text, stays quiet.
        assert_eq!(
            fact_color("3.bar/extras/token_out", Some("statusLineOutput"), &t),
            t.text_muted
        );
        assert_eq!(
            fact_color("3.bar/extras/token_in", Some("statusLineSpend"), &t),
            t.text_muted
        );
        assert_eq!(fact_color("3.bar/extras/other", None, &t), t.text_muted);
        // On the recorded facts.
        let m = model_of(real_children(true)).expect("composer");
        let colors: Vec<u32> = m.bar.facts.iter().map(|f| f.color).collect();
        assert_eq!(
            colors,
            [
                t.text,
                t.text_muted,
                t.text_muted,
                t.text_muted,
                t.text_muted,
                t.text_muted,
                t.text_muted,
                t.text_subtle,
                t.text_subtle
            ]
        );
    }

    #[test]
    fn the_model_name_and_effort_label_are_reckoned_in_the_ui_font_the_facts_in_mono() {
        let m = model_of(real_children(false)).expect("composer");
        let none = BarFit {
            facts: vec![false; m.bar.facts.len()],
            effort: false,
            usage: false,
            model_name: None,
        };
        let name = "Opus 5.5".chars().count() as f32;
        assert!(
            (bar_width(&m.bar, &none)
                - (theme::TSP_BAR_FIXED_WIDTH
                    + theme::TSP_MODEL_CHROME_WIDTH
                    + name * theme::TSP_UI_CHAR_WIDTH))
                .abs()
                < 0.01
        );
        let with_effort = BarFit {
            effort: true,
            ..none.clone()
        };
        let label = "high".chars().count() as f32;
        assert!(
            (bar_width(&m.bar, &with_effort)
                - bar_width(&m.bar, &none)
                - (theme::TSP_EFFORT_GLYPH_WIDTH
                    + space::XS
                    + label * theme::TSP_UI_CHAR_WIDTH
                    + space::S))
                .abs()
                < 0.01
        );
        let yolo = Fact {
            id: String::new(),
            icon: None,
            body: FactBody::Text("abcd".into()),
            priority: 0,
            permission: false,
            color: 0,
        };
        assert!(
            (fact_width(&m.bar.facts[0])
                - fact_width(&yolo)
                - (theme::ICON_SIZE_SMALL + space::XS))
                .abs()
                < 0.01
        );
    }
}
