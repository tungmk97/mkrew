use std::collections::BTreeMap;

use mkrew_protocol::{Cli, SlashItem, SlashSource};

use super::testing::{doc, put, skill, world};
use super::*;
use crate::profile::{SlashAdd, SlashOverrides};

fn names(out: &SlashList) -> Vec<&str> {
    out.items.iter().map(|i| i.name.as_str()).collect()
}

fn add(name: &str, description: &str) -> SlashAdd {
    SlashAdd {
        name: name.into(),
        description: description.into(),
        argument_hint: None,
    }
}

#[test]
fn codex_lists_only_its_built_ins_whatever_the_folder_holds() {
    let w = world();
    put(&w.repo, ".claude/commands/mine.md", &doc("x"));
    put(&w.repo, ".agents/skills/s/SKILL.md", &skill("skilled", "x"));
    let out = list(Cli::Codex, &w.roots(&w.repo), &SlashOverrides::default());
    assert!(out.errors.is_empty());
    assert_eq!(names(&out)[..2], ["model", "presets"]);
    assert!(out.items.iter().all(|i| i.source == SlashSource::BuiltIn));
    assert!(!names(&out).contains(&"mine") && !names(&out).contains(&"skilled"));
}

#[test]
fn hide_removes_rows_by_name_case_insensitively() {
    let w = world();
    let hide = |names: &[&str]| SlashOverrides {
        hide: names.iter().map(|n| (*n).to_owned()).collect(),
        add: vec![],
    };
    let out = list(
        Cli::Codex,
        &w.roots(&w.repo),
        &hide(&["MODEL", "review", "nope"]),
    );
    assert!(!names(&out).contains(&"model") && !names(&out).contains(&"review"));
    assert!(names(&out).contains(&"presets"));
}

#[test]
fn an_added_row_replaces_every_source_of_its_name_in_place() {
    let w = world();
    put(&w.repo, ".claude/commands/deploy.md", &doc("file"));
    let overrides = SlashOverrides {
        hide: vec![],
        add: vec![SlashAdd {
            name: "compact".into(),
            description: "Mine".into(),
            argument_hint: Some("<x>".into()),
        }],
    };
    let plain = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    let out = list(Cli::Claude, &w.roots(&w.repo), &overrides);
    assert_eq!(out.items.iter().filter(|i| i.name == "compact").count(), 1);
    let row = out.items.iter().find(|i| i.name == "compact").unwrap();
    assert_eq!(row.source, SlashSource::Profile);
    assert_eq!(
        (row.description.as_str(), row.argument_hint.as_deref()),
        ("Mine", Some("<x>"))
    );
    assert_eq!(names(&out), names(&plain), "same place in the order");
}

#[test]
fn an_added_name_that_no_source_has_goes_last_but_claude_stays_sorted_by_name() {
    let w = world();
    let overrides = SlashOverrides {
        hide: vec![],
        add: vec![add("deploy", "Staging")],
    };
    let codex = list(Cli::Codex, &w.roots(&w.repo), &overrides);
    assert_eq!(names(&codex).last(), Some(&"deploy"));
    let claude = list(Cli::Claude, &w.roots(&w.repo), &overrides);
    let sorted: Vec<_> = {
        let mut v: Vec<_> = names(&claude).iter().map(|n| n.to_lowercase()).collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&claude)
            .iter()
            .map(|n| n.to_lowercase())
            .collect::<Vec<_>>(),
        sorted
    );
    assert!(names(&claude).contains(&"deploy"));
}

#[test]
fn hide_applies_after_the_added_rows_are_merged() {
    let w = world();
    let overrides = SlashOverrides {
        hide: vec!["deploy".into()],
        add: vec![add("deploy", "Staging"), add("keep", "Kept")],
    };
    let out = list(Cli::Copilot, &w.roots(&w.repo), &overrides);
    assert!(!names(&out).contains(&"deploy"));
    assert!(names(&out).contains(&"keep"));
}

#[test]
fn a_cli_with_no_files_still_lists_its_built_ins_in_an_empty_folder() {
    let w = world();
    for cli in [Cli::Claude, Cli::Codex, Cli::Omp, Cli::Copilot] {
        let out = list(cli, &w.roots(&w.repo), &SlashOverrides::default());
        assert!(out.errors.is_empty(), "{cli:?}: {:?}", out.errors);
        assert!(out.items.len() > 40, "{cli:?}");
    }
}

#[test]
fn for_machine_picks_the_managed_folder_and_the_os_of_this_build() {
    let roots = ScanRoots::for_machine("/p".into(), "/h".into(), BTreeMap::new());
    assert_eq!(roots.managed.len(), 1);
    let expected = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    };
    assert_eq!(roots.os, expected);
}

#[test]
fn desktop_shows_only_where_the_docs_say() {
    let w = world();
    let on = |os| {
        let mut roots = w.roots(&w.repo);
        roots.os = os;
        list(Cli::Claude, &roots, &SlashOverrides::default())
            .items
            .iter()
            .any(|i: &SlashItem| i.name == "desktop")
    };
    assert!(on(Os::Windows) && on(Os::Macos) && !on(Os::Linux));
}

#[test]
fn the_omp_group_row_counts_the_skills_left_after_hide() {
    let w = world();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("alpha", "A"));
    put(&w.repo, ".omp/skills/b/SKILL.md", &skill("beta", "B"));
    let hide = |names: &[&str]| SlashOverrides {
        hide: names.iter().map(|n| (*n).to_owned()).collect(),
        add: vec![],
    };
    let group = |hidden: &[&str]| {
        let out = list(Cli::Omp, &w.roots(&w.repo), &hide(hidden));
        out.items
            .into_iter()
            .find(|i| i.name == "skill:")
            .map(|i| (i.group, i.description))
    };
    assert_eq!(group(&[]), Some((Some(2), "2 skills".into())));
    assert_eq!(group(&["skill:alpha"]), Some((Some(1), "1 skill".into())));
    assert_eq!(group(&["skill:alpha", "skill:beta"]), None);
}
