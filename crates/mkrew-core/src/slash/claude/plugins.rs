//! Claude Code plugins on disk (spec 2026-11-04 §5.1): the enabled entries of
//! `~/.claude/plugins/installed_plugins.json` and their skills and commands.

use std::fs;
use std::io;
use std::path::Path;

use mkrew_protocol::{SlashItem, SlashSource};
use serde_json::Value;

use super::settings::Settings;
use super::{Candidate, Kind};
use crate::slash::ScanRoots;
use crate::slash::scan::{self, Doc};

/// Adds the rows of every enabled plugin to `found`. A plugin is enabled by `enabledPlugins`
/// (`"<name>@<marketplace>"`), else by `defaultEnabled` in its manifest. Names are
/// `<manifest name>:<name>` and the scope is 3, below every other.
pub(super) fn scan(
    roots: &ScanRoots,
    settings: &Settings,
    found: &mut Vec<Candidate>,
    errors: &mut Vec<String>,
) {
    let index = roots.home.join(".claude/plugins/installed_plugins.json");
    let text = match fs::read_to_string(&index) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(e) => {
            errors.push(format!("{}: {e}", index.display()));
            return;
        }
    };
    let installed: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            errors.push(format!("{}: {e}", index.display()));
            return;
        }
    };
    let Some(plugins) = installed.get("plugins").and_then(Value::as_object) else {
        return;
    };
    for (key, entries) in plugins {
        let entries = match entries {
            Value::Array(list) => list.iter().collect(),
            single => vec![single],
        };
        for entry in entries {
            let Some(install) = entry.get("installPath").and_then(Value::as_str) else {
                continue;
            };
            plugin(Path::new(install), key, settings, found, errors);
        }
    }
}

fn plugin(
    install: &Path,
    key: &str,
    settings: &Settings,
    found: &mut Vec<Candidate>,
    errors: &mut Vec<String>,
) {
    let manifest_file = install.join(".claude-plugin/plugin.json");
    let manifest = match fs::read_to_string(&manifest_file) {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap_or_else(|e| {
            errors.push(format!("{}: {e}", manifest_file.display()));
            Value::Null
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Value::Null,
        Err(e) => {
            errors.push(format!("{}: {e}", manifest_file.display()));
            Value::Null
        }
    };
    let default_on = manifest
        .get("defaultEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !settings
        .enabled_plugins
        .get(key)
        .copied()
        .unwrap_or(default_on)
    {
        return;
    }
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| key.split('@').next())
        .unwrap_or(key)
        .to_owned();
    let add = |found: &mut Vec<Candidate>, kind: Kind, local: &str, doc: &Doc| {
        let local = match kind {
            Kind::Skill => doc.fm.name.clone().unwrap_or_else(|| local.to_owned()),
            Kind::Command => local.to_owned(),
        };
        found.push(row(
            kind,
            &name,
            &local,
            doc.description(None),
            doc.fm.argument_hint.clone(),
            doc,
        ));
    };

    // Skills: the default folder, plus every `skills` path of the manifest.
    let mut skill_dirs = vec![install.join("skills")];
    skill_dirs.extend(paths(manifest.get("skills")).map(|p| install.join(p)));
    for dir in skill_dirs {
        for doc in scan::skill_docs(&dir, errors) {
            add(found, Kind::Skill, &doc.key, &doc);
        }
        if dir.join("SKILL.md").is_file() {
            let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned());
            let doc = scan::read_doc(&dir.join("SKILL.md"), folder.unwrap_or_default(), errors);
            if let Some(doc) = doc {
                add(found, Kind::Skill, &doc.key, &doc);
            }
        }
    }

    // Commands: `commands` of the manifest replaces the default folder.
    match manifest.get("commands") {
        Some(Value::Object(map)) => {
            for (command, spec) in map {
                let doc = command_spec(install, spec, errors);
                let description = spec
                    .get("description")
                    .and_then(Value::as_str)
                    .map_or_else(|| doc.description(None), str::to_owned);
                let hint = spec
                    .get("argumentHint")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| doc.fm.argument_hint.clone());
                found.push(row(Kind::Command, &name, command, description, hint, &doc));
            }
        }
        Some(paths_value) => {
            for path in paths(Some(paths_value)) {
                let path = install.join(path);
                if path.is_dir() {
                    for doc in scan::command_docs(&path, false, errors) {
                        add(found, Kind::Command, &doc.key, &doc);
                    }
                } else if let Some(stem) =
                    path.file_stem().map(|s| s.to_string_lossy().into_owned())
                    && let Some(doc) = scan::read_doc(&path, stem, errors)
                {
                    add(found, Kind::Command, &doc.key, &doc);
                }
            }
        }
        None => {
            for doc in scan::command_docs(&install.join("commands"), false, errors) {
                add(found, Kind::Command, &doc.key, &doc);
            }
        }
    }
}

/// A manifest `commands` entry `{source | content, ...}` as a document: the file at `source`
/// below `install`, else the inline `content`, else an empty one.
fn command_spec(install: &Path, spec: &Value, errors: &mut Vec<String>) -> Doc {
    let from_file = spec
        .get("source")
        .and_then(Value::as_str)
        .and_then(|source| scan::read_doc(&install.join(source), String::new(), errors));
    from_file.unwrap_or_else(|| {
        let content = spec
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        scan::doc_from_text(String::new(), content)
    })
}

/// A manifest path value: one string or an array of strings, `./` removed.
fn paths(value: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    let list: Vec<&Value> = match value {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    list.into_iter()
        .filter_map(Value::as_str)
        .map(|p| p.trim_start_matches("./").to_owned())
}

fn row(
    kind: Kind,
    plugin: &str,
    local: &str,
    description: String,
    argument_hint: Option<String>,
    doc: &Doc,
) -> Candidate {
    Candidate {
        kind,
        scope: 3,
        invocable: doc.fm.user_invocable != Some(false),
        item: SlashItem {
            name: format!("{plugin}:{local}"),
            description,
            argument_hint,
            source: SlashSource::Plugin(plugin.to_owned()),
            group: None,
        },
    }
}
