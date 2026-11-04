//! The `/` commands an agent CLI shows (spec 2026-11-04 §4, §5): a built-in table per CLI plus
//! the skill, command and prompt files that CLI scans, read from the agent's own machine.
//!
//! The CLI is never started. Every scan reads small files from `roots`, stops at the repo root
//! and keeps what it cannot read as one line in [`SlashList::errors`].

pub mod front_matter;
mod tables;

use std::collections::BTreeMap;
use std::path::PathBuf;

use mkrew_protocol::{Cli, SlashItem, SlashSource};
use serde::Deserialize;

use crate::profile::SlashOverrides;

/// The operating system a table row can be limited to (`os = [...]` in `slash/*.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
}

/// Where a scan looks. Tests build it by hand; the daemon uses [`ScanRoots::for_machine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRoots {
    /// The folder the agent runs in: the project, or its task worktree.
    pub folder: PathBuf,
    /// The home folder whose `~/.claude` and friends are read.
    pub home: PathBuf,
    /// Folders holding `.claude/skills` and `managed-settings.json` for an organisation.
    pub managed: Vec<PathBuf>,
    pub os: Os,
    /// The environment the agent runs with (profile `env` over the daemon's): only
    /// `COPILOT_SKILLS_DIRS` is read.
    pub env: BTreeMap<String, String>,
}

impl ScanRoots {
    /// This machine: the Claude Code managed folder and the OS come from the build target.
    pub fn for_machine(folder: PathBuf, home: PathBuf, env: BTreeMap<String, String>) -> Self {
        let (os, managed) = if cfg!(windows) {
            (Os::Windows, r"C:\Program Files\ClaudeCode")
        } else if cfg!(target_os = "macos") {
            (Os::Macos, "/Library/Application Support/ClaudeCode")
        } else {
            (Os::Linux, "/etc/claude-code")
        };
        Self {
            folder,
            home,
            managed: vec![PathBuf::from(managed)],
            os,
            env,
        }
    }
}

/// The rows of one CLI plus the files that could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlashList {
    pub items: Vec<SlashItem>,
    pub errors: Vec<String>,
}

/// What typing `/` shows for `cli` in `roots.folder`, in the order the CLI shows it with an empty
/// filter, with the profile's `[slash]` changes applied.
pub fn list(cli: Cli, roots: &ScanRoots, overrides: &SlashOverrides) -> SlashList {
    let mut errors = Vec::new();
    let builtins = tables::builtins(cli, roots.os).unwrap_or_else(|e| {
        errors.push(e);
        Vec::new()
    });
    let mut items: Vec<SlashItem> = builtins.into_iter().map(|b| b.item).collect();
    apply(&mut items, overrides);
    if cli == Cli::Claude {
        items.sort_by_cached_key(|item| item.name.to_lowercase());
    }
    SlashList { items, errors }
}

/// `[[slash.add]]` rows replace every row of the same name (in place, else at the end), then
/// `[slash] hide` removes by name. Names compare case-insensitively.
fn apply(items: &mut Vec<SlashItem>, overrides: &SlashOverrides) {
    for add in &overrides.add {
        let row = SlashItem {
            name: add.name.clone(),
            description: add.description.clone(),
            argument_hint: add.argument_hint.clone(),
            source: SlashSource::Profile,
            group: None,
        };
        let first = items
            .iter()
            .position(|i| i.name.eq_ignore_ascii_case(&row.name));
        items.retain(|i| !i.name.eq_ignore_ascii_case(&row.name));
        items.insert(first.unwrap_or(items.len()), row);
    }
    items.retain(|i| {
        !overrides
            .hide
            .iter()
            .any(|h| i.name.eq_ignore_ascii_case(h))
    });
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
