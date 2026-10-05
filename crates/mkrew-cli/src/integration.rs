//! `mk integration install|uninstall|status <agent>` (SPEC §6, §9): wires the agent's own hook
//! config to `mk hook`, so Mkrew learns the agent state from the agent instead of the screen.
//!
//! - Claude Code: `<CLAUDE_CONFIG_DIR or ~/.claude>/settings.json`, one command hook per event.
//! - Codex: `<CODEX_HOME or ~/.codex>/config.toml`, top-level `notify = ["mk", "hook", "codex"]`.
//!
//! Edits merge into the existing file (other settings, hooks and TOML comments are kept), back
//! the original up to `<file>.<unix ms>.bak` before the first change, and are idempotent: a
//! file that needs no change is neither written nor backed up.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use serde_json::{Map, Value, json};

/// Claude Code hook events Mkrew listens to (SPEC §6).
const CLAUDE_EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Notification", "Stop"];
const CODEX_NOTIFY: [&str; 3] = ["mk", "hook", "codex"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
    Status,
}

#[derive(Debug, PartialEq, Eq)]
enum Installed {
    Yes,
    Partly,
    No,
}

pub fn run(action: Action, agent: Agent) -> anyhow::Result<ExitCode> {
    let file = config_file(agent, |key| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    })?;
    run_on(action, agent, &file)
}

/// The agent's config file; `var` is the environment (Claude's `CLAUDE_CONFIG_DIR`, Codex's
/// `CODEX_HOME`, which also let tests point at a temp dir).
fn config_file(agent: Agent, var: impl Fn(&str) -> Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let (env, dir, name) = match agent {
        Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude", "settings.json"),
        Agent::Codex => ("CODEX_HOME", ".codex", "config.toml"),
    };
    let dir = match var(env) {
        Some(dir) => dir,
        None => directories::BaseDirs::new()
            .context("cannot determine the home directory")?
            .home_dir()
            .join(dir),
    };
    Ok(dir.join(name))
}

fn run_on(action: Action, agent: Agent, file: &Path) -> anyhow::Result<ExitCode> {
    let name = match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    };
    let text = match fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("read {}", file.display())),
    };
    let original = text.as_deref().unwrap_or("");
    if action == Action::Status {
        let installed = match agent {
            Agent::Claude => claude_status(original),
            Agent::Codex => codex_status(original),
        }
        .with_context(|| format!("parse {}", file.display()))?;
        let label = match installed {
            Installed::Yes => "installed",
            Installed::Partly => "partly installed (run `mk integration install` again)",
            Installed::No => "not installed",
        };
        println!("{name}: {label} ({})", file.display());
        if mkrew_core::lookup::find("mk".as_ref()).is_none() {
            println!(
                "warning: `mk` is not on PATH ({}); hooks will fail",
                mkrew_core::lookup::SEARCHED
            );
        }
        return Ok(if installed == Installed::Yes {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    let install = action == Action::Install;
    let updated = match agent {
        Agent::Claude => claude_edit(original, install),
        Agent::Codex => codex_edit(original, install),
    }
    .with_context(|| format!("update {}", file.display()))?;
    let verb = if install { "installed" } else { "uninstalled" };
    let Some(updated) = updated else {
        println!("{name}: already {verb} ({})", file.display());
        return Ok(ExitCode::SUCCESS);
    };
    let backup = match text {
        Some(_) => Some(back_up(file)?),
        None => None,
    };
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    fs::write(file, updated).with_context(|| format!("write {}", file.display()))?;
    match backup {
        Some(backup) => println!(
            "{name}: {verb} ({}; backup: {})",
            file.display(),
            backup.display()
        ),
        None => println!("{name}: {verb} ({})", file.display()),
    }
    Ok(ExitCode::SUCCESS)
}

/// Copies `file` to `<file>.<unix ms>.bak` (a new name, never overwriting an older backup).
fn back_up(file: &Path) -> anyhow::Result<PathBuf> {
    let mut ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    loop {
        let mut name = file.as_os_str().to_owned();
        name.push(format!(".{ms}.bak"));
        let backup = PathBuf::from(name);
        if !backup.exists() {
            fs::copy(file, &backup).with_context(|| format!("back up to {}", backup.display()))?;
            return Ok(backup);
        }
        ms += 1;
    }
}

fn claude_command(event: &str) -> String {
    format!("mk hook claude {event}")
}

fn parse_settings(text: &str) -> anyhow::Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text).context("not valid JSON")? {
        Value::Object(settings) => Ok(settings),
        _ => bail!("not a JSON object"),
    }
}

/// Whether a matcher group (`{"matcher"?, "hooks": [...]}`) runs `command`.
fn group_runs(group: &Value, command: &str) -> bool {
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == command))
}

fn claude_status(text: &str) -> anyhow::Result<Installed> {
    let settings = parse_settings(text)?;
    let found = CLAUDE_EVENTS
        .iter()
        .filter(|event| {
            settings
                .get("hooks")
                .and_then(|hooks| hooks[**event].as_array())
                .is_some_and(|groups| {
                    groups
                        .iter()
                        .any(|group| group_runs(group, &claude_command(event)))
                })
        })
        .count();
    Ok(match found {
        0 => Installed::No,
        n if n == CLAUDE_EVENTS.len() => Installed::Yes,
        _ => Installed::Partly,
    })
}

/// The new `settings.json` text, or `None` if nothing changes.
fn claude_edit(text: &str, install: bool) -> anyhow::Result<Option<String>> {
    let mut settings = parse_settings(text)?;
    let before = Value::Object(settings.clone());
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(hooks) = hooks else {
        bail!("`hooks` is not an object");
    };
    for event in CLAUDE_EVENTS {
        let command = claude_command(event);
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(groups) = groups else {
            bail!("`hooks.{event}` is not an array");
        };
        if install {
            if !groups.iter().any(|group| group_runs(group, &command)) {
                groups.push(json!({"hooks": [{"type": "command", "command": command}]}));
            }
            continue;
        }
        // Drop our hooks, then the groups and events that only held them.
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let len = list.len();
            list.retain(|hook| hook["command"] != command.as_str());
            !(list.is_empty() && list.len() < len)
        });
        if groups.is_empty() && before["hooks"][event] != json!([]) {
            // `shift_remove`: `remove` swaps the last key into the gap.
            hooks.shift_remove(event);
        }
    }
    if hooks.is_empty() && before["hooks"] != json!({}) {
        settings.shift_remove("hooks");
    }
    let after = Value::Object(settings);
    if after == before {
        return Ok(None);
    }
    let mut out = serde_json::to_string_pretty(&after).context("serialize settings")?;
    out.push('\n');
    Ok(Some(out))
}

fn parse_config(text: &str) -> anyhow::Result<toml_edit::DocumentMut> {
    text.parse().context("not valid TOML")
}

/// Whether `notify` is exactly Mkrew's command.
fn notify_is_ours(doc: &toml_edit::DocumentMut) -> bool {
    doc.get("notify")
        .and_then(toml_edit::Item::as_array)
        .is_some_and(|notify| {
            notify.len() == CODEX_NOTIFY.len()
                && notify
                    .iter()
                    .zip(CODEX_NOTIFY)
                    .all(|(arg, ours)| arg.as_str() == Some(ours))
        })
}

fn codex_status(text: &str) -> anyhow::Result<Installed> {
    Ok(if notify_is_ours(&parse_config(text)?) {
        Installed::Yes
    } else {
        Installed::No
    })
}

/// The new `config.toml` text, or `None` if nothing changes. Codex runs a single `notify`
/// program, so install refuses to replace one that is not Mkrew's.
fn codex_edit(text: &str, install: bool) -> anyhow::Result<Option<String>> {
    let mut doc = parse_config(text)?;
    let other = doc
        .get("notify")
        .map(|item| item.to_string().trim().to_owned());
    match (install, notify_is_ours(&doc), other) {
        (true, true, _) | (false, false, _) => return Ok(None),
        (true, false, Some(other)) => bail!(
            "`notify` is already set to {other}; Codex runs only one notify program, remove it first"
        ),
        (true, false, None) => {
            let notify = toml_edit::Array::from_iter(CODEX_NOTIFY);
            doc.insert("notify", toml_edit::value(notify));
        }
        (false, true, _) => {
            doc.remove("notify");
        }
    }
    Ok(Some(doc.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn config_files_honour_the_agents_env_overrides() {
        let var = |key: &str| (key == "CODEX_HOME").then(|| PathBuf::from("/c"));
        assert_eq!(
            config_file(Agent::Codex, var).unwrap(),
            Path::new("/c/config.toml")
        );
        let var = |key: &str| (key == "CLAUDE_CONFIG_DIR").then(|| PathBuf::from("/a"));
        assert_eq!(
            config_file(Agent::Claude, var).unwrap(),
            Path::new("/a/settings.json")
        );
    }

    #[test]
    fn claude_install_merges_and_is_idempotent() {
        let original = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "say done"}]}]
  },
  "env": {"A": "1"}
}"#;
        assert_eq!(claude_status(original).unwrap(), Installed::No);
        let installed = claude_edit(original, true).unwrap().unwrap();
        assert_eq!(claude_status(&installed).unwrap(), Installed::Yes);
        assert_eq!(claude_edit(&installed, true).unwrap(), None);

        let settings = json(&installed);
        let keys: Vec<_> = settings.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "hooks", "env"], "key order is kept");
        assert_eq!(settings["env"], json(r#"{"A": "1"}"#));
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "the user's own Stop hook stays");
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        for event in CLAUDE_EVENTS {
            let groups = settings["hooks"][event].as_array().unwrap();
            let ours = groups.last().unwrap();
            assert_eq!(
                ours,
                &json(&format!(
                    r#"{{"hooks": [{{"type": "command", "command": "mk hook claude {event}"}}]}}"#
                ))
            );
        }
    }

    #[test]
    fn claude_uninstall_removes_only_mkrew_hooks() {
        let original = r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}], "PreToolUse": []}}"#;
        let installed = claude_edit(original, true).unwrap().unwrap();
        let removed = claude_edit(&installed, false).unwrap().unwrap();
        assert_eq!(json(&removed), json(original));
        assert_eq!(claude_status(&removed).unwrap(), Installed::No);
        assert_eq!(claude_edit(&removed, false).unwrap(), None);

        // A hand-added Mkrew hook sharing a group with the user's hook leaves that hook alone.
        let shared = r#"{"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": "say done"},
            {"type": "command", "command": "mk hook claude Stop"}
        ]}]}}"#;
        assert_eq!(claude_status(shared).unwrap(), Installed::Partly);
        let removed = claude_edit(shared, false).unwrap().unwrap();
        assert_eq!(
            json(&removed),
            json(
                r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}}"#
            )
        );

        // Removing `hooks` from the middle keeps the other keys in place.
        let middle = r#"{"a": 1, "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mk hook claude Stop"}]}]}, "b": 2, "c": 3}"#;
        let removed = claude_edit(middle, false).unwrap().unwrap();
        let removed = json(&removed);
        let keys: Vec<_> = removed.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["a", "b", "c"]);

        // Nothing installed in an empty or missing file: nothing to write.
        assert_eq!(claude_edit("", false).unwrap(), None);
        assert_eq!(claude_edit("{}", false).unwrap(), None);
    }

    #[test]
    fn claude_rejects_settings_it_cannot_merge() {
        assert!(claude_edit("{", true).is_err());
        assert!(claude_edit("[]", true).is_err());
        assert!(claude_edit(r#"{"hooks": []}"#, true).is_err());
        assert!(claude_edit(r#"{"hooks": {"Stop": {}}}"#, true).is_err());
        assert!(claude_status("{").is_err());
    }

    #[test]
    fn claude_install_into_a_missing_file() {
        let installed = claude_edit("", true).unwrap().unwrap();
        assert!(installed.ends_with('\n'));
        assert_eq!(claude_status(&installed).unwrap(), Installed::Yes);
        assert_eq!(
            json(&claude_edit(&installed, false).unwrap().unwrap()),
            json("{}")
        );
    }

    #[test]
    fn codex_install_keeps_comments_and_stays_top_level() {
        let original =
            "# my config\nmodel = \"o3\" # fast\n\n[profiles.slow]\nmodel = \"o3-pro\"\n";
        assert_eq!(codex_status(original).unwrap(), Installed::No);
        let installed = codex_edit(original, true).unwrap().unwrap();
        assert_eq!(
            installed,
            "# my config\nmodel = \"o3\" # fast\nnotify = [\"mk\", \"hook\", \"codex\"]\n\n\
             [profiles.slow]\nmodel = \"o3-pro\"\n"
        );
        let parsed: toml_edit::DocumentMut = installed.parse().unwrap();
        assert!(parsed["profiles"]["slow"].get("notify").is_none());
        assert_eq!(codex_status(&installed).unwrap(), Installed::Yes);
        assert_eq!(codex_edit(&installed, true).unwrap(), None);

        let removed = codex_edit(&installed, false).unwrap().unwrap();
        assert_eq!(removed, original);
        assert_eq!(codex_edit(&removed, false).unwrap(), None);
    }

    #[test]
    fn codex_never_replaces_another_notify_program() {
        let other = "notify = [\"python3\", \"notify.py\"]\n";
        let error = codex_edit(other, true).unwrap_err().to_string();
        assert!(error.contains("python3"), "{error}");
        assert_eq!(codex_status(other).unwrap(), Installed::No);
        assert_eq!(
            codex_edit(other, false).unwrap(),
            None,
            "uninstall leaves it alone"
        );
        assert!(codex_edit("notify = [", true).is_err());
        assert_eq!(
            codex_edit("", true).unwrap().unwrap(),
            "notify = [\"mk\", \"hook\", \"codex\"]\n"
        );
    }
}
