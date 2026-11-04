//! GitHub Copilot CLI (spec 2026-11-04 §5.4): built-ins in the reference order, then skills,
//! then `.claude/commands` files. The first found wins in the order of the rows below; a skill
//! beats a command of the same name; `user-invocable: false` is left out.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;

pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    // The working folder holds `.github`, `.agents` and `.claude` skills; its parents up to the
    // repo root only `.github/skills`.
    let mut skill_dirs: Vec<(PathBuf, SlashSource)> = Vec::new();
    for (at, dir) in scan::project_dirs(&roots.folder).into_iter().enumerate() {
        skill_dirs.push((dir.join(".github/skills"), SlashSource::Project));
        if at == 0 {
            skill_dirs.push((dir.join(".agents/skills"), SlashSource::Project));
            skill_dirs.push((dir.join(".claude/skills"), SlashSource::Project));
        }
    }
    skill_dirs.push((roots.home.join(".copilot/skills"), SlashSource::User));
    skill_dirs.push((roots.home.join(".agents/skills"), SlashSource::User));
    if let Some(list) = roots.env.get("COPILOT_SKILLS_DIRS") {
        for dir in std::env::split_paths(OsStr::new(list)).filter(|d| !d.as_os_str().is_empty()) {
            skill_dirs.push((dir, SlashSource::User));
        }
    }

    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for (dir, source) in skill_dirs {
        for doc in scan::skill_docs(&dir, errors) {
            let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
            push(&mut rows, &mut seen, name, &doc, &source);
        }
    }
    let commands = roots.folder.join(".claude/commands");
    for doc in scan::command_docs(&commands, false, errors) {
        push(
            &mut rows,
            &mut seen,
            doc.key.clone(),
            &doc,
            &SlashSource::Project,
        );
    }

    let mut items: Vec<SlashItem> = builtins.into_iter().map(|b| b.item).collect();
    items.extend(
        rows.into_iter()
            .filter(|(invocable, _)| *invocable)
            .map(|(_, item)| item),
    );
    items
}

/// Adds a row unless an earlier one has the name (case-insensitively).
fn push(
    rows: &mut Vec<(bool, SlashItem)>,
    seen: &mut HashSet<String>,
    name: String,
    doc: &Doc,
    source: &SlashSource,
) {
    if !seen.insert(name.to_lowercase()) {
        return;
    }
    let item = SlashItem {
        name,
        description: doc.description(None),
        argument_hint: doc.fm.argument_hint.clone(),
        source: source.clone(),
        group: None,
    };
    rows.push((doc.fm.user_invocable != Some(false), item));
}

#[cfg(test)]
mod tests;
