//! The Claude Code settings that decide which rows show (spec 2026-11-04 §5.1).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde_json::Value;

use crate::slash::ScanRoots;

/// `enabledPlugins`, `skillOverrides` and `disableBundledSkills` merged over the settings files.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Settings {
    /// `"<name>@<marketplace>"` to its switch.
    pub enabled_plugins: BTreeMap<String, bool>,
    /// Skill name (lowercase) to its `skillOverrides` value; only `"off"` hides.
    pub skill_overrides: BTreeMap<String, String>,
    pub disable_bundled_skills: bool,
}

impl Settings {
    pub fn skill_is_off(&self, name: &str) -> bool {
        self.skill_overrides
            .get(&name.to_lowercase())
            .is_some_and(|v| v == "off")
    }
}

/// Lowest to highest precedence: user, project, project local, then each managed folder's
/// `managed-settings.json`. `enabledPlugins` and `skillOverrides` merge by key, so a later file
/// wins per key; `disableBundledSkills` is the last one set. A missing file is skipped, one that
/// does not parse is a line in `errors`.
pub(super) fn load(roots: &ScanRoots, errors: &mut Vec<String>) -> Settings {
    let project = roots.folder.join(".claude");
    let mut files: Vec<PathBuf> = vec![
        roots.home.join(".claude/settings.json"),
        project.join("settings.json"),
        project.join("settings.local.json"),
    ];
    files.extend(
        roots
            .managed
            .iter()
            .map(|m| m.join("managed-settings.json")),
    );

    let mut settings = Settings::default();
    for file in files {
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                errors.push(format!("{}: {e}", file.display()));
                continue;
            }
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => merge(&mut settings, &map),
            Ok(_) => errors.push(format!("{}: not a JSON object", file.display())),
            Err(e) => errors.push(format!("{}: {e}", file.display())),
        }
    }
    settings
}

fn merge(settings: &mut Settings, map: &serde_json::Map<String, Value>) {
    if let Some(Value::Object(plugins)) = map.get("enabledPlugins") {
        for (key, value) in plugins {
            if let Some(on) = value.as_bool() {
                settings.enabled_plugins.insert(key.clone(), on);
            }
        }
    }
    if let Some(Value::Object(overrides)) = map.get("skillOverrides") {
        for (key, value) in overrides {
            if let Some(mode) = value.as_str() {
                settings
                    .skill_overrides
                    .insert(key.to_lowercase(), mode.to_owned());
            }
        }
    }
    if let Some(off) = map.get("disableBundledSkills").and_then(Value::as_bool) {
        settings.disable_bundled_skills = off;
    }
}
