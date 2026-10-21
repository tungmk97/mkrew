//! The README (SPEC §18 M8) covers install, agent configuration and uninstall, and every `mk`
//! command line in its code blocks names real subcommands.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn readme() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md")).unwrap()
}

/// `mk …` lines inside fenced code blocks (without trailing `# comments`) and inline
/// `` `mk …` `` spans.
fn mk_lines(text: &str) -> Vec<String> {
    let mut in_fence = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            in_fence = !in_fence;
        } else if in_fence {
            if line.starts_with("mk ") {
                let command = line.split(" #").next().unwrap_or(line);
                lines.push(command.trim().to_owned());
            }
        } else {
            // Odd-numbered pieces of a line split on backticks are inline code.
            lines.extend(
                line.split('`')
                    .skip(1)
                    .step_by(2)
                    .filter(|span| span.starts_with("mk "))
                    .map(str::to_owned),
            );
        }
    }
    lines
}

#[test]
fn readme_has_the_required_sections() {
    let text = readme();
    for heading in [
        "## Install",
        "### Windows (setup .exe)",
        "### macOS (app)",
        "### Linux (tar.gz)",
        "### Updates",
        "### From source",
        "## Configure agents",
        "## Uninstall",
        "## Files and folders",
    ] {
        assert!(
            text.lines().any(|line| line == heading),
            "missing {heading}"
        );
    }
    for needle in [
        "mk integration install claude",
        "mk integration install codex",
        "mk integration uninstall claude",
        "mk integration uninstall codex",
        "Default agent for new panes",
        "--run-autostart",
    ] {
        assert!(text.contains(needle), "missing {needle}");
    }
}

#[test]
fn readme_mk_commands_exist() {
    let lines = mk_lines(&readme());
    assert!(lines.len() >= 15, "{lines:?}");
    for line in lines {
        // Subcommand words and plain positionals; stop at flags, placeholders, quotes and `…`.
        let words: Vec<&str> = line
            .split_whitespace()
            .skip(1)
            .take_while(|w| {
                !w.starts_with('-')
                    && w.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            })
            .collect();
        let output = Command::new(env!("CARGO_BIN_EXE_mk"))
            .args(&words)
            .arg("--help")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`{line}`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8_lossy(&output.stdout);
        for flag in line
            .split_whitespace()
            .map(|w| w.trim_matches(['[', ']']))
            .filter(|w| w.starts_with("--"))
        {
            assert!(help.contains(flag), "`{line}`: no {flag} in\n{help}");
        }
    }
}
