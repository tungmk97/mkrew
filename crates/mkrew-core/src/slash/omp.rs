//! Oh My Pi (spec 2026-11-04 §5.3), in the order of its popup with an empty filter: built-ins
//! and bundled commands, the `skill:` group row and its skills, markdown commands, prompt
//! templates.

use std::collections::HashSet;
use std::path::PathBuf;

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;

/// The first-line description of a command or template is cut to this many characters.
const CUT: usize = 60;

pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    let commands = commands(roots, errors);
    // The bundled markdown `init` is only a fallback for a file command of the same name.
    let has_init = commands.iter().any(|c| c.name == "init");
    let mut items: Vec<SlashItem> = builtins
        .into_iter()
        .filter(|b| !(b.bundled && has_init))
        .map(|b| b.item)
        .collect();

    let skills = skills(roots, errors);
    if !skills.is_empty() {
        let count = skills.len();
        items.push(SlashItem {
            name: "skill:".into(),
            description: if count == 1 {
                "1 skill".into()
            } else {
                format!("{count} skills")
            },
            argument_hint: None,
            source: SlashSource::BuiltIn,
            group: Some(u32::try_from(count).unwrap_or(u32::MAX)),
        });
        items.extend(skills);
    }
    items.extend(commands);

    // A template whose name another row already has is hidden; two templates may share one.
    let taken: HashSet<String> = items.iter().map(|i| i.name.clone()).collect();
    items.extend(
        templates(roots, errors)
            .into_iter()
            .filter(|t| !taken.contains(&t.name)),
    );
    items
}

fn item(name: String, doc: &Doc, source: &SlashSource) -> SlashItem {
    SlashItem {
        name,
        description: doc.description(Some(CUT)),
        argument_hint: doc.fm.argument_hint.clone(),
        source: source.clone(),
        group: None,
    }
}

/// Markdown commands by provider priority: omp 100, Claude 80, `.agent(s)` 70, Codex 70,
/// opencode 55; inside omp the project beats the user, inside `.agent(s)` the user beats the
/// project. The first row of a name wins. `foo/bar.md` under `.claude/commands` is `bar` and
/// `foo:bar`.
fn commands(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let (folder, home) = (&roots.folder, &roots.home);
    let mut sources: Vec<(PathBuf, bool, SlashSource)> = vec![
        (folder.join(".omp/commands"), false, SlashSource::Project),
        (home.join(".omp/agent/commands"), false, SlashSource::User),
        (folder.join(".claude/commands"), true, SlashSource::Project),
        (home.join(".agent/commands"), false, SlashSource::User),
        (home.join(".agents/commands"), false, SlashSource::User),
    ];
    for dir in scan::project_dirs(folder) {
        sources.push((dir.join(".agent/commands"), false, SlashSource::Project));
        sources.push((dir.join(".agents/commands"), false, SlashSource::Project));
    }
    sources.push((folder.join(".codex/commands"), false, SlashSource::Project));
    sources.push((
        folder.join(".opencode/commands"),
        false,
        SlashSource::Project,
    ));

    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (dir, recursive, source) in sources {
        for doc in scan::command_docs(&dir, recursive, errors) {
            let last = doc.key.rsplit(':').next().unwrap_or(&doc.key).to_owned();
            let mut names = vec![last];
            if doc.key.contains(':') {
                names.push(doc.key.clone());
            }
            for name in names {
                if seen.insert(name.clone()) {
                    items.push(item(name, &doc, &source));
                }
            }
        }
    }
    items
}

/// `skill:<name>` rows by provider priority: native 100 (project folders up to the repo root,
/// then the user's `~/.omp/agent/skills`), omp plugins 90, Claude's project folder 80. A name
/// with `/` or `\` is dropped.
fn skills(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let mut sources: Vec<(PathBuf, SlashSource)> = scan::project_dirs(&roots.folder)
        .into_iter()
        .map(|dir| (dir.join(".omp/skills"), SlashSource::Project))
        .collect();
    sources.push((roots.home.join(".omp/agent/skills"), SlashSource::User));
    let modules = roots.home.join(".omp/plugins/node_modules");
    for (package, path) in scan::entries(&modules, errors) {
        sources.push((path.join("skills"), SlashSource::Plugin(package)));
    }
    sources.push((roots.folder.join(".claude/skills"), SlashSource::Project));

    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (dir, source) in sources {
        for doc in scan::skill_docs(&dir, errors) {
            let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
            if name.contains(['/', '\\']) || !seen.insert(name.clone()) {
                continue;
            }
            items.push(item(format!("skill:{name}"), &doc, &source));
        }
    }
    items
}

/// Prompt templates, user then project; the name is the file name without `.md`, whatever
/// folder it is in.
fn templates(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let sources = [
        (roots.home.join(".omp/agent/prompts"), SlashSource::User),
        (roots.folder.join(".omp/prompts"), SlashSource::Project),
    ];
    let mut items = Vec::new();
    for (dir, source) in sources {
        for doc in scan::command_docs(&dir, true, errors) {
            let name = doc.key.rsplit(':').next().unwrap_or(&doc.key).to_owned();
            items.push(item(name, &doc, &source));
        }
    }
    items
}

#[cfg(test)]
mod tests;
