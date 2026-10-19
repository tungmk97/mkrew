//! Jira Cloud REST v3 for the Board's Jira side (SPEC §20I, ADR 0062): pure helpers and two
//! blocking calls, run on the background executor. Nothing here touches gpui, so it can move
//! to the daemon unchanged.

use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use mkrew_core::settings::JiraSettings;
use serde_json::{Value, json};

/// Issues per search; one page only.
// ponytail: first 50 issues only; follow nextPageToken if someone has more open issues
const MAX_RESULTS: u32 = 50;

/// One row of the Jira list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub status: String,
    /// The issue type (Bug, Story, Task…).
    pub kind: String,
}

/// What the task dialog opens with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub prompt: String,
}

/// The API token from the variable `api_token_env` names (`var` reads it); also checks that an
/// email is set, since both go into the login.
pub fn api_token(settings: &JiraSettings, var: impl Fn(&str) -> Option<String>) -> Result<String> {
    if settings.email.trim().is_empty() {
        bail!("Set [jira] email in settings.toml");
    }
    let name = settings.api_token_env.trim();
    match (!name.is_empty())
        .then(|| var(name))
        .flatten()
        .map(|t| t.trim().to_owned())
    {
        Some(token) if !token.is_empty() => Ok(token),
        _ => bail!("Set the environment variable {name} to your Jira API token"),
    }
}

/// `https://<host>` for a site given as a bare host or a URL.
pub fn base_url(site: &str) -> String {
    let host = site.trim();
    let host = host
        .strip_prefix("https://")
        .or_else(|| host.strip_prefix("http://"))
        .unwrap_or(host);
    format!("https://{}", host.trim_end_matches('/'))
}

pub fn auth_header(email: &str, token: &str) -> String {
    let pair = format!("{}:{token}", email.trim());
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(pair)
    )
}

/// Keys are checked by the daemon (`mkrew_core::jira::normalize_keys`), so no quoting.
pub fn jql(keys: &[String]) -> String {
    format!(
        "project in ({}) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
        keys.join(", ")
    )
}

pub fn search_body(keys: &[String]) -> String {
    json!({
        "jql": jql(keys),
        "fields": ["summary", "status", "issuetype"],
        "maxResults": MAX_RESULTS,
    })
    .to_string()
}

pub fn parse_search(status: u16, body: &str) -> Result<Vec<Issue>> {
    let json = answer(status, body, None)?;
    let issues = json
        .get("issues")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("unexpected answer from Jira: {}", short(body)))?;
    Ok(issues
        .iter()
        .map(|issue| Issue {
            key: text(issue, "/key"),
            summary: text(issue, "/fields/summary"),
            status: text(issue, "/fields/status/name"),
            kind: text(issue, "/fields/issuetype/name"),
        })
        .collect())
}

pub fn parse_issue(site: &str, key: &str, status: u16, body: &str) -> Result<Draft> {
    let json = answer(status, body, Some(key))?;
    let summary = text(&json, "/fields/summary");
    let description = adf_to_text(json.pointer("/fields/description").unwrap_or(&Value::Null));
    Ok(draft(site, key, &summary, &description))
}

pub fn draft(site: &str, key: &str, summary: &str, description: &str) -> Draft {
    let mut prompt = format!("{key}: {summary}\n{}/browse/{key}", base_url(site));
    if !description.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(description);
    }
    Draft {
        title: format!("{key} {summary}"),
        prompt,
    }
}

/// The JSON of a 2xx answer, or the spec's message for any other status (`key` = the issue
/// asked for, for the 404 message).
fn answer(status: u16, body: &str, key: Option<&str>) -> Result<Value> {
    let json: Option<Value> = serde_json::from_str(body).ok();
    if (200..300).contains(&status) {
        return json.ok_or_else(|| anyhow!("unexpected answer from Jira: {}", short(body)));
    }
    match (status, key) {
        (401 | 403, _) => bail!("Jira rejected the login: check [jira] email and the API token"),
        (404, Some(key)) => bail!("{key} no longer exists or you cannot see it"),
        _ => {}
    }
    let messages: Vec<&str> = json
        .as_ref()
        .and_then(|json| json.get("errorMessages"))
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if status == 400 && !messages.is_empty() {
        bail!("{}", messages.join(" "));
    }
    bail!("Jira answered {status}: {}", short(body))
}

fn text(json: &Value, pointer: &str) -> String {
    json.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The first 200 chars of `body`, trimmed.
fn short(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &body[..end]),
        None => body.to_owned(),
    }
}

/// Atlassian Document Format as plain text: blocks separated by a blank line, lists as
/// `- ` / `1. ` lines (nested ones indented), code blocks fenced, mentions and link cards as
/// their text and URL. Unknown nodes give their children's text.
pub fn adf_to_text(doc: &Value) -> String {
    let mut out = String::new();
    walk(doc, &mut out);
    out.trim().to_owned()
}

fn walk(node: &Value, out: &mut String) {
    let children = node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    let attr = |name: &str| {
        node.pointer(&format!("/attrs/{name}"))
            .and_then(Value::as_str)
    };
    match node.get("type").and_then(Value::as_str).unwrap_or_default() {
        "text" => out.push_str(node.get("text").and_then(Value::as_str).unwrap_or_default()),
        "hardBreak" => out.push('\n'),
        "mention" => out.push_str(attr("text").unwrap_or_default()),
        "inlineCard" | "blockCard" => out.push_str(attr("url").unwrap_or_default()),
        "paragraph" | "heading" => {
            children.for_each(|child| walk(child, out));
            out.push_str("\n\n");
        }
        "codeBlock" => {
            out.push_str("```\n");
            children.for_each(|child| walk(child, out));
            out.push_str("\n```\n\n");
        }
        kind @ ("bulletList" | "orderedList") => {
            for (index, item) in children.enumerate() {
                let marker = if kind == "orderedList" {
                    format!("{}. ", index + 1)
                } else {
                    "- ".to_owned()
                };
                let mut inner = String::new();
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .for_each(|child| walk(child, &mut inner));
                for (line_no, line) in inner.lines().filter(|l| !l.is_empty()).enumerate() {
                    if line_no == 0 {
                        out.push_str(&marker);
                    } else {
                        out.push_str(&" ".repeat(marker.len()));
                    }
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        _ => children.for_each(|child| walk(child, out)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> JiraSettings {
        JiraSettings {
            site: "acme.atlassian.net".into(),
            email: "an@acme.com".into(),
            ..JiraSettings::default()
        }
    }

    #[test]
    fn token_comes_from_the_named_variable() {
        let token = api_token(&settings(), |name| {
            (name == "JIRA_API_TOKEN").then(|| " tok \n".to_owned())
        });
        assert_eq!(token.unwrap(), "tok");
        for var in [None, Some("  ".to_owned())] {
            let error = api_token(&settings(), |_| var.clone())
                .unwrap_err()
                .to_string();
            assert_eq!(
                error,
                "Set the environment variable JIRA_API_TOKEN to your Jira API token"
            );
        }
        let no_email = JiraSettings {
            email: " ".into(),
            ..settings()
        };
        let error = api_token(&no_email, |_| Some("tok".into()))
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Set [jira] email in settings.toml");
    }

    #[test]
    fn site_becomes_an_https_base() {
        for site in [
            "acme.atlassian.net",
            "https://acme.atlassian.net",
            " https://acme.atlassian.net/ ",
            "http://acme.atlassian.net",
        ] {
            assert_eq!(base_url(site), "https://acme.atlassian.net", "{site}");
        }
    }

    #[test]
    fn basic_auth_encodes_email_and_token() {
        // base64("an@acme.com:tok")
        assert_eq!(
            auth_header("an@acme.com", "tok"),
            "Basic YW5AYWNtZS5jb206dG9r"
        );
    }

    #[test]
    fn jql_lists_open_issues_of_the_keys_assigned_to_me() {
        let keys = ["PROJ".to_owned(), "BILL".to_owned()];
        assert_eq!(
            jql(&keys),
            "project in (PROJ, BILL) AND assignee = currentUser() AND statusCategory != Done \
             ORDER BY updated DESC"
        );
        let body: Value = serde_json::from_str(&search_body(&keys[..1])).unwrap();
        assert_eq!(
            body,
            json!({
                "jql": "project in (PROJ) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
                "fields": ["summary", "status", "issuetype"],
                "maxResults": 50
            })
        );
    }

    #[test]
    fn search_answer_becomes_rows() {
        let body = r#"{"issues":[
            {"id":"1","key":"PROJ-7","fields":{"summary":"Fix login","status":{"name":"In Progress"},"issuetype":{"name":"Bug"}}},
            {"id":"2","key":"BILL-2","fields":{"summary":"Invoice PDF","status":{"name":"To Do"},"issuetype":{"name":"Story"}}}
        ],"isLast":true}"#;
        assert_eq!(
            parse_search(200, body).unwrap(),
            [
                Issue {
                    key: "PROJ-7".into(),
                    summary: "Fix login".into(),
                    status: "In Progress".into(),
                    kind: "Bug".into()
                },
                Issue {
                    key: "BILL-2".into(),
                    summary: "Invoice PDF".into(),
                    status: "To Do".into(),
                    kind: "Story".into()
                },
            ]
        );
        assert_eq!(parse_search(200, r#"{"issues":[]}"#).unwrap(), []);
        let odd = parse_search(200, "{}").unwrap_err().to_string();
        assert!(odd.starts_with("unexpected answer from Jira"), "{odd}");
    }

    #[test]
    fn error_answers_become_the_spec_messages() {
        let login = "Jira rejected the login: check [jira] email and the API token";
        assert_eq!(parse_search(401, "").unwrap_err().to_string(), login);
        assert_eq!(parse_search(403, "<html/>").unwrap_err().to_string(), login);
        let unknown = r#"{"errorMessages":["The value 'NOPE' does not exist for the field 'project'."],"errors":{}}"#;
        assert_eq!(
            parse_search(400, unknown).unwrap_err().to_string(),
            "The value 'NOPE' does not exist for the field 'project'."
        );
        assert_eq!(
            parse_issue("acme.atlassian.net", "PROJ-9", 404, "{}")
                .unwrap_err()
                .to_string(),
            "PROJ-9 no longer exists or you cannot see it"
        );
        let long = parse_search(500, &"x".repeat(500)).unwrap_err().to_string();
        assert_eq!(long, format!("Jira answered 500: {}…", "x".repeat(200)));
    }

    #[test]
    fn rich_descriptions_become_plain_text() {
        let doc = json!({"type": "doc", "version": 1, "content": [
            {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Steps"}]},
            {"type": "paragraph", "content": [
                {"type": "text", "text": "Login fails for "},
                {"type": "mention", "attrs": {"id": "1", "text": "@An"}},
                {"type": "text", "text": " see "},
                {"type": "inlineCard", "attrs": {"url": "https://x.test/y"}},
                {"type": "hardBreak"},
                {"type": "text", "text": "second line", "marks": [{"type": "strong"}]}
            ]},
            {"type": "bulletList", "content": [
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "a"}]},
                    {"type": "bulletList", "content": [
                        {"type": "listItem", "content": [
                            {"type": "paragraph", "content": [{"type": "text", "text": "b"}]}
                        ]}
                    ]}
                ]},
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "c"}]}
                ]}
            ]},
            {"type": "codeBlock", "attrs": {"language": "sh"}, "content": [{"type": "text", "text": "cargo test"}]},
            {"type": "orderedList", "content": [
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "one"}]}
                ]}
            ]},
            {"type": "rule"},
            {"type": "paragraph"}
        ]});
        assert_eq!(
            adf_to_text(&doc),
            "Steps\n\nLogin fails for @An see https://x.test/y\nsecond line\n\n- a\n  - b\n- c\n\n\
             ```\ncargo test\n```\n\n1. one"
        );
        assert_eq!(adf_to_text(&Value::Null), "");
    }

    #[test]
    fn issue_answer_becomes_the_task_draft() {
        let body = r#"{"key":"PROJ-7","fields":{"summary":"Fix login","description":
            {"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"It breaks."}]}]}}}"#;
        assert_eq!(
            parse_issue("https://acme.atlassian.net/", "PROJ-7", 200, body).unwrap(),
            Draft {
                title: "PROJ-7 Fix login".into(),
                prompt: "PROJ-7: Fix login\nhttps://acme.atlassian.net/browse/PROJ-7\n\nIt breaks."
                    .into(),
            }
        );
        let empty = r#"{"key":"PROJ-8","fields":{"summary":"No text","description":null}}"#;
        assert_eq!(
            parse_issue("acme.atlassian.net", "PROJ-8", 200, empty)
                .unwrap()
                .prompt,
            "PROJ-8: No text\nhttps://acme.atlassian.net/browse/PROJ-8"
        );
    }
}
