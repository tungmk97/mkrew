//! The built-in rows of each CLI, embedded from `crates/mkrew-core/slash/*.toml` (spec 2026-11-04 §5.5).

use mkrew_protocol::{Cli, SlashItem, SlashSource};
use serde::Deserialize;

use super::Os;

pub(super) const CODEX: &str = include_str!("../../slash/codex.toml");
pub(super) const OMP: &str = include_str!("../../slash/omp.toml");
pub(super) const COPILOT: &str = include_str!("../../slash/copilot.toml");
pub(super) const CLAUDE: &str = include_str!("../../slash/claude.toml");

/// A built-in row and whether the table marks it `bundled` (a bundled skill in Claude Code,
/// the markdown fallback `init` in omp).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Builtin {
    pub item: SlashItem,
    pub bundled: bool,
}

/// The header keys (`cli_version` or `source_commit`, `source`) are for people and are checked
/// by the tests; only the rows are read here.
#[derive(Deserialize)]
struct Table {
    command: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    name: String,
    description: String,
    #[serde(default)]
    argument_hint: Option<String>,
    #[serde(default)]
    os: Vec<Os>,
    #[serde(default)]
    bundled: bool,
}

fn parse(src: &str, os: Option<Os>) -> Result<Vec<Builtin>, toml::de::Error> {
    let table: Table = toml::from_str(src)?;
    Ok(table
        .command
        .into_iter()
        .filter(|row| os.is_none_or(|os| row.os.is_empty() || row.os.contains(&os)))
        .map(|row| Builtin {
            item: SlashItem {
                name: row.name,
                description: row.description,
                argument_hint: row.argument_hint,
                source: SlashSource::BuiltIn,
                group: None,
            },
            bundled: row.bundled,
        })
        .collect())
}

/// The rows of `cli` that exist on `os`, in table order.
pub(super) fn builtins(cli: Cli, os: Os) -> Result<Vec<Builtin>, String> {
    let (name, src) = match cli {
        Cli::Claude => ("claude", CLAUDE),
        Cli::Codex => ("codex", CODEX),
        Cli::Omp => ("omp", OMP),
        Cli::Copilot => ("copilot", COPILOT),
    };
    parse(src, Some(os)).map_err(|e| format!("built-in table {name}.toml: {e}"))
}

#[cfg(test)]
mod tests;
