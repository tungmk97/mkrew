use super::*;

fn names(rows: &[Builtin]) -> Vec<&str> {
    rows.iter().map(|b| b.item.name.as_str()).collect()
}

const ALL: [(Cli, &str, &str); 4] = [
    (Cli::Claude, CLAUDE, "cli_version"),
    (Cli::Codex, CODEX, "source_commit"),
    (Cli::Omp, OMP, "cli_version"),
    (Cli::Copilot, COPILOT, "cli_version"),
];

/// Spec §5.5: a unit test parses every table and checks unique names.
#[test]
fn every_embedded_table_parses_with_unique_names_and_a_header() {
    for (cli, src, version_key) in ALL {
        let rows = parse(src, None).unwrap_or_else(|e| panic!("{cli:?}: {e}"));
        assert!(!rows.is_empty(), "{cli:?}");
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            let name = &row.item.name;
            assert!(!name.is_empty() && !name.starts_with('/'), "{cli:?} {name}");
            assert!(!row.item.description.trim().is_empty(), "{cli:?} {name}");
            assert!(
                seen.insert(name.to_lowercase()),
                "{cli:?}: duplicate {name}"
            );
        }
        let header: toml::Table = toml::from_str(src).unwrap();
        let version = header.get(version_key).and_then(toml::Value::as_str);
        assert!(
            version.is_some_and(|v| !v.is_empty()),
            "{cli:?}: {version_key}"
        );
        let url = header.get("source").and_then(toml::Value::as_str);
        assert!(
            url.is_some_and(|u| u.starts_with("https://")),
            "{cli:?}: source"
        );
    }
}

#[test]
fn a_row_with_an_unknown_key_is_an_error() {
    let src = "[[command]]\nname = \"a\"\ndescription = \"d\"\nargument_hnt = \"x\"\n";
    assert!(
        parse(src, None)
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );
}

#[test]
fn os_filters_rows_and_an_empty_os_means_everywhere() {
    let src = "[[command]]\nname = \"all\"\ndescription = \"d\"\n\
               [[command]]\nname = \"win\"\ndescription = \"d\"\nos = [\"windows\", \"macos\"]\n";
    assert_eq!(
        names(&parse(src, Some(Os::Windows)).unwrap()),
        ["all", "win"]
    );
    assert_eq!(names(&parse(src, Some(Os::Linux)).unwrap()), ["all"]);
}

#[test]
fn codex_rows_follow_the_enum_order_and_leave_out_the_hidden_ones() {
    let rows = builtins(Cli::Codex, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(
        names[..3],
        ["model", "presets", "ide"],
        "daybreak is gated internally"
    );
    let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(at("permissions") < at("review") && at("review") < at("compact"));
    for left_out in [
        "debug-config",
        "debug-m-drop",
        "debug-m-update",
        "apps",
        "rollout",
        "test-approval",
        "quit",
        "btw",
        "daybreak",
        "setup-default-sandbox",
        "fast",
        "app",
    ] {
        assert!(!names.contains(&left_out), "{left_out}");
    }
    assert!(
        builtins(Cli::Codex, Os::Macos)
            .unwrap()
            .iter()
            .any(|b| b.item.name == "app")
    );
    let model = &rows[0].item;
    assert_eq!(
        model.description,
        "choose what model and reasoning effort to use"
    );
}

#[test]
fn omp_ends_with_the_bundled_commands_and_a_bundled_init() {
    let rows = builtins(Cli::Omp, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(names[..2], ["security", "settings"]);
    assert_eq!(
        names[names.len() - 4..],
        ["green", "review", "annotate", "init"]
    );
    let bundled: Vec<_> = rows
        .iter()
        .filter(|b| b.bundled)
        .map(|b| &b.item.name[..])
        .collect();
    assert_eq!(bundled, ["init"]);
}

#[test]
fn claude_rows_are_sorted_mark_bundled_skills_and_limit_desktop_to_two_platforms() {
    let rows = builtins(Cli::Claude, Os::Linux).unwrap();
    let names = names(&rows);
    assert!(names.windows(2).all(|p| p[0] < p[1]), "sorted and unique");
    assert!(!names.contains(&"desktop"), "windows and macos only");
    assert!(
        builtins(Cli::Claude, Os::Windows)
            .unwrap()
            .iter()
            .any(|b| b.item.name == "desktop")
    );
    for left_out in [
        "heapdump",
        "setup-bedrock",
        "setup-vertex",
        "new",
        "pr-comments",
    ] {
        assert!(!names.contains(&left_out), "{left_out}");
    }
    let bundled = |n: &str| rows.iter().find(|b| b.item.name == n).map(|b| b.bundled);
    assert_eq!(bundled("batch"), Some(true));
    assert_eq!(bundled("compact"), Some(false));
    let add_dir = rows.iter().find(|b| b.item.name == "add-dir").unwrap();
    assert_eq!(add_dir.item.argument_hint.as_deref(), Some("<path>"));
}

#[test]
fn copilot_keeps_the_reference_order_and_an_experimental_tag() {
    let rows = builtins(Cli::Copilot, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(names[..3], ["add-dir", "after", "agent"]);
    assert_eq!(names[names.len() - 3..], ["fork", "worktree", "move"]);
    let after = rows.iter().find(|b| b.item.name == "after").unwrap();
    assert!(after.item.description.ends_with("(experimental)"));
}
