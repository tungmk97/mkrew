//! `mk integration install|uninstall|status <agent>` (SPEC §6, §9): wires the agent's own hook
//! config to `mk hook`, so Mkrew learns the agent state from the agent instead of the screen.
//!
//! - Claude Code: `<CLAUDE_CONFIG_DIR or ~/.claude>/settings.json`, one command hook per event.
//! - Codex: `<CODEX_HOME or ~/.codex>/config.toml`, top-level `notify = ["<mk>", "hook", "codex"]`.
//! - Oh My Pi: `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`, a Mkrew-owned file.
//! - GitHub Copilot CLI: `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json`, a Mkrew-owned file.
//!
//! Edits merge into the existing file (other settings, hooks and TOML comments are kept), back
//! the original up to `<file>.<unix ms>.bak` before the first change, and are idempotent: a
//! file that needs no change is neither written nor backed up. The omp and Copilot owned files
//! are written whole, backed up only when their content differs, and deleted on uninstall.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use serde_json::{Map, Value, json};

/// Claude Code hook events Mkrew listens to (SPEC §6).
const CLAUDE_EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Notification", "Stop"];
const CODEX_ARGS: [&str; 2] = ["hook", "codex"];
const OMP_EXTENSION: &str = include_str!("../assets/omp-extension.ts");
const COPILOT_HOOKS: &str = include_str!("../assets/copilot-hooks.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    Omp,
    Copilot,
}

/// The agents whose hooks merge into a config file the user also edits.
#[derive(Clone, Copy, Debug)]
enum Merged {
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
    let mk = own_mk()?;
    let code = run_on(action, agent, &file, &mk)?;
    if action == Action::Install && code == ExitCode::SUCCESS && agent != Agent::Omp {
        println!("note: hooks run {mk}; reinstall after moving Mkrew");
    }
    if action == Action::Install {
        let profile = mkrew_core::Paths::from_env()?
            .agents_dir()
            .join(format!("{}.toml", name(agent)));
        let text = fs::read_to_string(&profile).ok();
        if let Some(hint) = resume_hint(agent, &profile, text.as_deref()) {
            println!("{hint}");
        }
    }
    Ok(code)
}

fn name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
        Agent::Omp => "omp",
        Agent::Copilot => "copilot",
    }
}

/// The agent's config file; `var` is the environment (Claude's `CLAUDE_CONFIG_DIR`, Codex's
/// `CODEX_HOME`, which also let tests point at a temp dir).
fn config_file(agent: Agent, var: impl Fn(&str) -> Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let (env, dir, name) = match agent {
        Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude", "settings.json"),
        Agent::Codex => ("CODEX_HOME", ".codex", "config.toml"),
        Agent::Omp => ("PI_CODING_AGENT_DIR", ".omp/agent", "extensions/mkrew.ts"),
        Agent::Copilot => ("COPILOT_HOME", ".copilot", "hooks/mkrew.json"),
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

fn run_on(action: Action, agent: Agent, file: &Path, mk: &str) -> anyhow::Result<ExitCode> {
    let name = name(agent);
    let text = match fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("read {}", file.display())),
    };
    let merged = match owned_contents(agent, mk) {
        Ok(ours) => return run_owned(action, name, file, text.as_deref(), &ours),
        Err(merged) => merged,
    };
    let original = text.as_deref().unwrap_or("");
    if action == Action::Status {
        let installed = match merged {
            Merged::Claude => claude_status(original, mk),
            Merged::Codex => codex_status(original, mk),
        }
        .with_context(|| format!("parse {}", file.display()))?;
        return Ok(print_status(name, file, &installed));
    }
    let install = action == Action::Install;
    let updated = match merged {
        Merged::Claude => claude_edit(original, install, mk),
        Merged::Codex => codex_edit(original, install, mk),
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

fn print_status(name: &str, file: &Path, installed: &Installed) -> ExitCode {
    let label = match installed {
        Installed::Yes => "installed",
        Installed::Partly => "partly installed (run `mk integration install` again)",
        Installed::No => "not installed",
    };
    println!("{name}: {label} ({})", file.display());
    if *installed == Installed::Yes {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// omp and Copilot load every file in one directory, so Mkrew owns one file there; the
/// others merge into the user's config.
fn owned_contents(agent: Agent, mk: &str) -> Result<String, Merged> {
    match agent {
        Agent::Omp => Ok(OMP_EXTENSION.to_owned()),
        Agent::Copilot => Ok(copilot_hooks(mk)),
        Agent::Claude => Err(Merged::Claude),
        Agent::Codex => Err(Merged::Codex),
    }
}

/// Another text = an older Mkrew version or the user's edit: install replaces it.
fn owned_status(text: Option<&str>, ours: &str) -> Installed {
    match text {
        None => Installed::No,
        Some(text) if text == ours => Installed::Yes,
        Some(_) => Installed::Partly,
    }
}

/// Install writes the whole file, uninstall deletes it; a different file is backed up first.
fn run_owned(
    action: Action,
    name: &str,
    file: &Path,
    text: Option<&str>,
    ours: &str,
) -> anyhow::Result<ExitCode> {
    let installed = owned_status(text, ours);
    let verb = if action == Action::Install {
        "installed"
    } else {
        "uninstalled"
    };
    let backup = match (action, &installed) {
        (Action::Status, _) => return Ok(print_status(name, file, &installed)),
        (Action::Install, Installed::Yes) | (Action::Uninstall, Installed::No) => {
            println!("{name}: already {verb} ({})", file.display());
            return Ok(ExitCode::SUCCESS);
        }
        (_, Installed::Partly) => Some(back_up(file)?),
        _ => None,
    };
    if action == Action::Install {
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        fs::write(file, ours).with_context(|| format!("write {}", file.display()))?;
    } else {
        fs::remove_file(file).with_context(|| format!("delete {}", file.display()))?;
    }
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

/// The shipped `resume` line of `agent`'s profile (SPEC §6).
fn resume_example(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude --resume {session_id}",
        Agent::Codex => "codex resume {session_id}",
        Agent::Omp => "omp --resume {session_id}",
        Agent::Copilot => "copilot --resume={session_id}",
    }
}

/// A note when the user's profile cannot resume: seeding never overwrites an existing
/// profile, so one written before `resume` shipped stays without it. `None` when the profile
/// resumes, is missing (seeded on the next daemon start) or does not parse (the UI says so).
fn resume_hint(agent: Agent, file: &Path, text: Option<&str>) -> Option<String> {
    let profile = mkrew_core::profile::parse_profile(name(agent), file, text?).ok()?;
    profile.resume.is_none().then(|| {
        format!(
            "note: {} has no `resume`; add `resume = \"{}\"` so its conversations come back after a restart",
            file.display(),
            resume_example(agent)
        )
    })
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

/// The absolute path of this `mk`: hook configs name it, never a bare `mk` that `PATH` may
/// resolve to another version (ADR 0059).
fn own_mk() -> anyhow::Result<String> {
    let exe = std::env::current_exe().context("locate mk")?;
    let exe = dunce::canonicalize(&exe).with_context(|| format!("resolve {}", exe.display()))?;
    exe.into_os_string()
        .into_string()
        .map_err(|exe| anyhow::anyhow!("mk's path is not UTF-8: {exe:?}"))
}

/// Whether `program` is some `mk` (any folder, or bare): older Mkrew versions wrote `mk`.
fn is_mk(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("mk"))
}

/// Claude Code exec form: `command` runs directly with `args`, no shell (ADR 0059).
fn claude_hook(mk: &str, event: &str) -> Value {
    json!({"type": "command", "command": mk, "args": ["hook", "claude", event]})
}

/// Whether a hook entry is Mkrew's for `event`: the exec form with any `mk`, or the string
/// `mk hook claude <event>` older versions wrote.
fn claude_hook_is_ours(hook: &Value, event: &str) -> bool {
    let legacy = hook["command"] == format!("mk hook claude {event}").as_str();
    let exec = hook["args"] == json!(["hook", "claude", event])
        && hook["command"].as_str().is_some_and(is_mk);
    legacy || exec
}

/// Copilot's hooks file running `mk`; the asset is a template with `"exec": "mk"`.
fn copilot_hooks(mk: &str) -> String {
    COPILOT_HOOKS.replace(r#""exec": "mk""#, &format!(r#""exec": {}"#, json!(mk)))
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

/// Whether a matcher group (`{"matcher"?, "hooks": [...]}`) runs a hook for which `pred` holds.
fn group_runs(group: &Value, pred: impl Fn(&Value) -> bool) -> bool {
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(pred))
}

/// `Yes` when every event runs `mk`; `Partly` when some run an `mk` (older path or form).
fn claude_status(text: &str, mk: &str) -> anyhow::Result<Installed> {
    let settings = parse_settings(text)?;
    let count = |pred: &dyn Fn(&Value, &str) -> bool| {
        CLAUDE_EVENTS
            .iter()
            .filter(|event| {
                settings
                    .get("hooks")
                    .and_then(|hooks| hooks[**event].as_array())
                    .is_some_and(|groups| {
                        groups
                            .iter()
                            .any(|group| group_runs(group, |hook| pred(hook, event)))
                    })
            })
            .count()
    };
    let current = count(&|hook, event| *hook == claude_hook(mk, event));
    let ours = count(&|hook, event| claude_hook_is_ours(hook, event));
    Ok(match (current, ours) {
        (n, _) if n == CLAUDE_EVENTS.len() => Installed::Yes,
        (_, 0) => Installed::No,
        _ => Installed::Partly,
    })
}

/// The new `settings.json` text, or `None` if nothing changes.
fn claude_edit(text: &str, install: bool, mk: &str) -> anyhow::Result<Option<String>> {
    let mut settings = parse_settings(text)?;
    let before = Value::Object(settings.clone());
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(hooks) = hooks else {
        bail!("`hooks` is not an object");
    };
    for event in CLAUDE_EVENTS {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(groups) = groups else {
            bail!("`hooks.{event}` is not an array");
        };
        if install
            && groups
                .iter()
                .any(|group| group_runs(group, |hook| *hook == claude_hook(mk, event)))
        {
            continue;
        }
        // Drop our hooks (any `mk`, either form), then the groups and events that only held them.
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let len = list.len();
            list.retain(|hook| !claude_hook_is_ours(hook, event));
            !(list.is_empty() && list.len() < len)
        });
        if install {
            groups.push(json!({"hooks": [claude_hook(mk, event)]}));
            continue;
        }
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

/// Whether `notify` is an `mk hook codex` command: any `mk` path, or the bare `mk` of older versions.
fn notify_is_ours(doc: &toml_edit::DocumentMut) -> bool {
    notify_args(doc).is_some_and(|args| {
        args.len() == 3 && is_mk(&args[0]) && args[1..] == CODEX_ARGS.map(str::to_owned)
    })
}

/// Whether `notify` is exactly `[mk, "hook", "codex"]`.
fn notify_is_current(doc: &toml_edit::DocumentMut, mk: &str) -> bool {
    notify_args(doc).is_some_and(|args| args == [mk, CODEX_ARGS[0], CODEX_ARGS[1]])
}

/// `notify` as strings, when it is an array of strings.
fn notify_args(doc: &toml_edit::DocumentMut) -> Option<Vec<String>> {
    doc.get("notify")?
        .as_array()?
        .iter()
        .map(|arg| arg.as_str().map(str::to_owned))
        .collect()
}

fn codex_status(text: &str, mk: &str) -> anyhow::Result<Installed> {
    let doc = parse_config(text)?;
    Ok(if notify_is_current(&doc, mk) {
        Installed::Yes
    } else if notify_is_ours(&doc) {
        Installed::Partly
    } else {
        Installed::No
    })
}

/// The new `config.toml` text, or `None` if nothing changes. Codex runs a single `notify`
/// program, so install refuses to replace one that is not Mkrew's.
fn codex_edit(text: &str, install: bool, mk: &str) -> anyhow::Result<Option<String>> {
    let mut doc = parse_config(text)?;
    let other = doc
        .get("notify")
        .map(|item| item.to_string().trim().to_owned());
    match (install, notify_is_ours(&doc), other) {
        (true, _, _) if notify_is_current(&doc, mk) => return Ok(None),
        (false, false, _) => return Ok(None),
        (true, false, Some(other)) => bail!(
            "`notify` is already set to {other}; Codex runs only one notify program, remove it first"
        ),
        (true, _, _) => {
            let notify = toml_edit::Array::from_iter([mk, CODEX_ARGS[0], CODEX_ARGS[1]]);
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

    // `\` separates folders on Windows only, so `is_mk` needs a native path.
    #[cfg(windows)]
    const MK: &str = r"C:\Program Files\Mkrew\bin\mk.exe";
    #[cfg(not(windows))]
    const MK: &str = "/opt/Mk rew/bin/mk";
    #[cfg(windows)]
    const OLD_MK: &str = r"D:\old\mk.exe";
    #[cfg(not(windows))]
    const OLD_MK: &str = "/old/mk";

    #[test]
    fn omp_extension_runs_the_daemons_mk() {
        assert!(
            OMP_EXTENSION.contains("process.env.MKREW_MK"),
            "{OMP_EXTENSION}"
        );
        assert!(!OMP_EXTENSION.contains("execFile(\"mk\""), "no bare mk");
    }

    #[test]
    fn claude_install_replaces_an_old_bare_mk_hook() {
        let old = r#"{"hooks": {"Stop": [
  {"hooks": [{"type": "command", "command": "mk hook claude Stop"}]},
  {"hooks": [{"type": "command", "command": "mk-other"}]}
]}}"#;
        assert_eq!(claude_status(old, MK).unwrap(), Installed::Partly);
        let new = claude_edit(old, true, MK).unwrap().unwrap();
        let stop = json(&new)["hooks"]["Stop"].clone();
        assert_eq!(
            stop[0]["hooks"][0],
            json!({"type": "command", "command": "mk-other"}),
            "{new}"
        );
        assert_eq!(
            stop[1]["hooks"][0],
            json!({"type": "command", "command": MK, "args": ["hook", "claude", "Stop"]}),
            "{new}"
        );
        assert!(!new.contains("mk hook claude"), "{new}");
        assert_eq!(claude_status(&new, MK).unwrap(), Installed::Yes);
        assert_eq!(claude_edit(&new, true, MK).unwrap(), None, "idempotent");
        // Uninstall drops ours in both forms and keeps the user's hook.
        let mixed = json!({"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "mk hook claude Stop"}]},
            {"hooks": [claude_hook(OLD_MK, "Stop")]},
            {"hooks": [{"type": "command", "command": "mk-other"}]}
        ]}});
        let left = json(&claude_edit(&mixed.to_string(), false, MK).unwrap().unwrap());
        assert_eq!(
            left,
            json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mk-other"}]}]}})
        );
    }

    #[test]
    fn codex_install_upgrades_an_older_mkrew_notify() {
        let old = "notify = [\"mk\", \"hook\", \"codex\"]\n";
        assert_eq!(codex_status(old, MK).unwrap(), Installed::Partly);
        let new = codex_edit(old, true, MK).unwrap().unwrap();
        assert_eq!(notify_of(&new), [MK, "hook", "codex"]);
        assert_eq!(codex_status(&new, MK).unwrap(), Installed::Yes);
        assert_eq!(codex_edit(&new, true, MK).unwrap(), None);
        assert_eq!(
            codex_edit(old, false, MK).unwrap().unwrap().trim(),
            "",
            "uninstall drops the old one"
        );
    }

    fn notify_of(text: &str) -> Vec<String> {
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        doc["notify"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }

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
        assert_eq!(claude_status(original, MK).unwrap(), Installed::No);
        let installed = claude_edit(original, true, MK).unwrap().unwrap();
        assert_eq!(claude_status(&installed, MK).unwrap(), Installed::Yes);
        assert_eq!(claude_edit(&installed, true, MK).unwrap(), None);

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
            assert_eq!(ours, &json!({"hooks": [claude_hook(MK, event)]}));
        }
    }

    #[test]
    fn claude_uninstall_removes_only_mkrew_hooks() {
        let original = r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}], "PreToolUse": []}}"#;
        let installed = claude_edit(original, true, MK).unwrap().unwrap();
        let removed = claude_edit(&installed, false, MK).unwrap().unwrap();
        assert_eq!(json(&removed), json(original));
        assert_eq!(claude_status(&removed, MK).unwrap(), Installed::No);
        assert_eq!(claude_edit(&removed, false, MK).unwrap(), None);

        // A hand-added Mkrew hook sharing a group with the user's hook leaves that hook alone.
        let shared = r#"{"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": "say done"},
            {"type": "command", "command": "mk hook claude Stop"}
        ]}]}}"#;
        assert_eq!(claude_status(shared, MK).unwrap(), Installed::Partly);
        let removed = claude_edit(shared, false, MK).unwrap().unwrap();
        assert_eq!(
            json(&removed),
            json(
                r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}}"#
            )
        );

        // Removing `hooks` from the middle keeps the other keys in place.
        let middle = r#"{"a": 1, "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mk hook claude Stop"}]}]}, "b": 2, "c": 3}"#;
        let removed = claude_edit(middle, false, MK).unwrap().unwrap();
        let removed = json(&removed);
        let keys: Vec<_> = removed.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["a", "b", "c"]);

        // Nothing installed in an empty or missing file: nothing to write.
        assert_eq!(claude_edit("", false, MK).unwrap(), None);
        assert_eq!(claude_edit("{}", false, MK).unwrap(), None);
    }

    #[test]
    fn claude_rejects_settings_it_cannot_merge() {
        assert!(claude_edit("{", true, MK).is_err());
        assert!(claude_edit("[]", true, MK).is_err());
        assert!(claude_edit(r#"{"hooks": []}"#, true, MK).is_err());
        assert!(claude_edit(r#"{"hooks": {"Stop": {}}}"#, true, MK).is_err());
        assert!(claude_status("{", MK).is_err());
    }

    #[test]
    fn claude_install_into_a_missing_file() {
        let installed = claude_edit("", true, MK).unwrap().unwrap();
        assert!(installed.ends_with('\n'));
        assert_eq!(claude_status(&installed, MK).unwrap(), Installed::Yes);
        assert_eq!(
            json(&claude_edit(&installed, false, MK).unwrap().unwrap()),
            json("{}")
        );
    }

    #[test]
    fn codex_install_keeps_comments_and_stays_top_level() {
        let original =
            "# my config\nmodel = \"o3\" # fast\n\n[profiles.slow]\nmodel = \"o3-pro\"\n";
        assert_eq!(codex_status(original, MK).unwrap(), Installed::No);
        let installed = codex_edit(original, true, MK).unwrap().unwrap();
        assert_eq!(notify_of(&installed), [MK, "hook", "codex"]);
        assert!(installed.starts_with("# my config\nmodel = \"o3\" # fast\nnotify = "));
        assert!(installed.ends_with("\n\n[profiles.slow]\nmodel = \"o3-pro\"\n"));
        let parsed: toml_edit::DocumentMut = installed.parse().unwrap();
        assert!(parsed["profiles"]["slow"].get("notify").is_none());
        assert_eq!(codex_status(&installed, MK).unwrap(), Installed::Yes);
        assert_eq!(codex_edit(&installed, true, MK).unwrap(), None);

        let removed = codex_edit(&installed, false, MK).unwrap().unwrap();
        assert_eq!(removed, original);
        assert_eq!(codex_edit(&removed, false, MK).unwrap(), None);
    }

    #[test]
    fn codex_never_replaces_another_notify_program() {
        let other = "notify = [\"python3\", \"notify.py\"]\n";
        let error = codex_edit(other, true, MK).unwrap_err().to_string();
        assert!(error.contains("python3"), "{error}");
        assert_eq!(codex_status(other, MK).unwrap(), Installed::No);
        assert_eq!(
            codex_edit(other, false, MK).unwrap(),
            None,
            "uninstall leaves it alone"
        );
        assert!(codex_edit("notify = [", true, MK).is_err());
        assert_eq!(
            notify_of(&codex_edit("", true, MK).unwrap().unwrap()),
            [MK, "hook", "codex"]
        );
    }

    #[test]
    fn config_files_of_owned_integrations() {
        let none = |_: &str| None;
        let home = directories::BaseDirs::new().unwrap().home_dir().to_owned();
        assert_eq!(
            config_file(Agent::Omp, none).unwrap(),
            home.join(".omp/agent/extensions/mkrew.ts")
        );
        let var = |key: &str| (key == "PI_CODING_AGENT_DIR").then(|| PathBuf::from("/o"));
        assert_eq!(
            config_file(Agent::Omp, var).unwrap(),
            Path::new("/o/extensions/mkrew.ts")
        );
        let var = |key: &str| (key == "COPILOT_HOME").then(|| PathBuf::from("/c"));
        assert_eq!(
            config_file(Agent::Copilot, var).unwrap(),
            Path::new("/c/hooks/mkrew.json")
        );
    }

    #[test]
    fn owned_files_install_replace_and_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        for agent in [Agent::Omp, Agent::Copilot] {
            let file = dir.path().join(format!("{agent:?}")).join("mkrew.x");
            let ours = owned_contents(agent, MK).unwrap();
            let ours = ours.as_str();
            assert_eq!(owned_status(None, ours), Installed::No);
            run_on(Action::Install, agent, &file, MK).unwrap();
            let text = fs::read_to_string(&file).unwrap();
            assert_eq!(owned_status(Some(&text), ours), Installed::Yes);
            run_on(Action::Install, agent, &file, MK).unwrap();
            let backups = || {
                fs::read_dir(file.parent().unwrap())
                    .unwrap()
                    .filter(|e| {
                        e.as_ref()
                            .unwrap()
                            .file_name()
                            .to_string_lossy()
                            .ends_with(".bak")
                    })
                    .count()
            };
            assert_eq!(
                backups(),
                0,
                "an unchanged file is neither written nor backed up"
            );
            // An older version is outdated, replaced, and kept as a backup.
            fs::write(&file, "old").unwrap();
            assert_eq!(owned_status(Some("old"), ours), Installed::Partly);
            run_on(Action::Install, agent, &file, MK).unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), ours);
            assert_eq!(backups(), 1);
            run_on(Action::Uninstall, agent, &file, MK).unwrap();
            assert!(!file.exists());
            run_on(Action::Uninstall, agent, &file, MK).unwrap();
        }

        // The template (`"exec": "mk"`) is an older install: outdated, replaced, backed up.
        let file = dir.path().join("template").join("mkrew.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, COPILOT_HOOKS).unwrap();
        let ours = copilot_hooks(MK);
        assert_eq!(owned_status(Some(COPILOT_HOOKS), &ours), Installed::Partly);
        run_on(Action::Install, Agent::Copilot, &file, MK).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), ours);
        assert_eq!(fs::read_dir(file.parent().unwrap()).unwrap().count(), 2);
    }

    #[test]
    fn copilot_hooks_call_mk_for_the_events_mkrew_maps() {
        let hooks: Value = serde_json::from_str(&copilot_hooks(MK)).unwrap();
        assert_eq!(hooks["version"], 1);
        for (event, entries) in hooks["hooks"].as_object().unwrap() {
            assert_eq!(entries[0]["exec"], MK);
            assert_eq!(entries[0]["args"], json!(["hook", "copilot", event]));
            let known = mkrew_core::HookAgent::Copilot
                .event(event, &json!({"notification_type": "permission_prompt"}));
            assert!(
                known.is_some() || event == "sessionStart",
                "{event} maps to nothing"
            );
        }
    }

    #[test]
    fn resume_hint_names_the_missing_line() {
        let file = Path::new("C:/cfg/agents/omp.toml");
        let old = "name = \"Oh My Pi\"\ncommand = \"omp\"\n[state]\nmode = \"screen\"\n";
        let hint = resume_hint(Agent::Omp, file, Some(old)).unwrap();
        assert!(
            hint.contains("resume = \"omp --resume {session_id}\""),
            "{hint}"
        );
        assert!(hint.contains("omp.toml"), "{hint}");
        let new = include_str!("../../../agents/omp.toml");
        assert_eq!(resume_hint(Agent::Omp, file, Some(new)), None);
        assert_eq!(
            resume_hint(Agent::Omp, file, None),
            None,
            "seeded on the next daemon start"
        );
    }
}
