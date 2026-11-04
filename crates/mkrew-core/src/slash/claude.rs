//! Claude Code (spec 2026-11-04 §5.1): built-ins and bundled skills from the table, plus the
//! skill and command files of the managed, user and project scopes and of enabled plugins.

mod plugins;
mod settings;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;
use settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Skill,
    Command,
}

/// A skill or command file that may become a row.
struct Candidate {
    kind: Kind,
    /// 0 managed, 1 user, 2 project, 3 plugin.
    scope: u8,
    item: SlashItem,
    invocable: bool,
}

/// The rows of Claude Code in `roots.folder`, unordered (the caller sorts by name).
pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    let settings = settings::load(roots, errors);
    let mut found = Vec::new();
    for managed in &roots.managed {
        add_skills(
            &mut found,
            &managed.join(".claude/skills"),
            SlashSource::Managed,
            0,
            errors,
        );
    }
    let user = roots.home.join(".claude");
    add_skills(
        &mut found,
        &user.join("skills"),
        SlashSource::User,
        1,
        errors,
    );
    add_commands(
        &mut found,
        &user.join("commands"),
        SlashSource::User,
        1,
        errors,
    );
    for dir in project_dirs(&roots.folder) {
        let claude = dir.join(".claude");
        add_skills(
            &mut found,
            &claude.join("skills"),
            SlashSource::Project,
            2,
            errors,
        );
        add_commands(
            &mut found,
            &claude.join("commands"),
            SlashSource::Project,
            2,
            errors,
        );
    }
    plugins::scan(roots, &settings, &mut found, errors);
    resolve(found, builtins, &settings)
}

/// The folders to scan: `folder` up to the repo root. A linked worktree with no `.claude/skills`
/// on that way also reads the main checkout (Claude Code 2.1.277), after its own folders.
fn project_dirs(folder: &Path) -> Vec<PathBuf> {
    let mut dirs = scan::project_dirs(folder);
    if dirs.iter().any(|dir| dir.join(".claude/skills").is_dir()) {
        return dirs;
    }
    if let Some(main) = scan::main_checkout_folder(folder) {
        dirs.extend(scan::project_dirs(&main));
    }
    dirs
}

fn add_skills(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::skill_docs(dir, errors) {
        let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
        found.push(candidate(Kind::Skill, scope, name, &doc, source.clone()));
    }
}

fn add_commands(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::command_docs(dir, true, errors) {
        found.push(candidate(
            Kind::Command,
            scope,
            doc.key.clone(),
            &doc,
            source.clone(),
        ));
    }
}

fn candidate(kind: Kind, scope: u8, name: String, doc: &Doc, source: SlashSource) -> Candidate {
    Candidate {
        kind,
        scope,
        invocable: doc.fm.user_invocable != Some(false),
        item: SlashItem {
            name,
            description: doc.claude_description(),
            argument_hint: doc.fm.argument_hint.clone(),
            source,
            group: None,
        },
    }
}

/// Same name, case-insensitively: a skill beats a command file, then managed beats user beats
/// project, then the folder nearer to the working folder. A skill also replaces the built-in row
/// of its name; a command file never does. Left out: `user-invocable: false`, skills that
/// `skillOverrides` turns `off` (bundled ones too) and, with `disableBundledSkills`, the bundled
/// rows of the table.
fn resolve(
    mut found: Vec<Candidate>,
    builtins: Vec<Builtin>,
    settings: &Settings,
) -> Vec<SlashItem> {
    found.sort_by_key(|c| (c.kind, c.scope)); // stable: nearer folders stay first
    let mut seen = HashSet::new();
    found.retain(|c| seen.insert(c.item.name.to_lowercase()));
    let replaced: HashSet<String> = found
        .iter()
        .filter(|c| c.kind == Kind::Skill)
        .map(|c| c.item.name.to_lowercase())
        .collect();
    let builtin_names: HashSet<String> = builtins
        .iter()
        .map(|b| b.item.name.to_lowercase())
        .collect();
    let mut items: Vec<SlashItem> = builtins
        .into_iter()
        .filter(|b| !replaced.contains(&b.item.name.to_lowercase()))
        .filter(|b| {
            !(b.bundled && (settings.disable_bundled_skills || settings.skill_is_off(&b.item.name)))
        })
        .map(|b| b.item)
        .collect();
    items.extend(
        found
            .into_iter()
            .filter(|c| c.invocable)
            // `skillOverrides` does not reach plugin skills (scope 3).
            .filter(|c| {
                !(c.kind == Kind::Skill && c.scope < 3 && settings.skill_is_off(&c.item.name))
            })
            .filter(|c| {
                c.kind == Kind::Skill || !builtin_names.contains(&c.item.name.to_lowercase())
            })
            .map(|c| c.item),
    );
    items
}

#[cfg(test)]
mod tests;
