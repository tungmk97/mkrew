use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &crate::slash::testing::World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Claude, &w.roots(folder), &SlashOverrides::default());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    out.items
}

fn find<'a>(items: &'a [SlashItem], name: &str) -> &'a SlashItem {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

#[test]
fn claude_lists_rows_by_name_whatever_the_source() {
    let w = world();
    put(&w.home, ".claude/commands/zzz.md", &doc("z"));
    put(&w.repo, ".claude/commands/Aaa.md", &doc("a"));
    put(&w.repo, ".claude/skills/mid/SKILL.md", &skill("Mmm", "m"));
    let lower: Vec<_> = rows(&w, &w.repo)
        .iter()
        .map(|i| i.name.to_lowercase())
        .collect();
    let mut sorted = lower.clone();
    sorted.sort();
    assert_eq!(lower, sorted);
    for name in ["aaa", "mmm", "zzz"] {
        assert!(lower.contains(&name.to_owned()), "{name}");
    }
}

#[test]
fn skill_names_come_from_front_matter_else_the_folder_and_commands_join_paths() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/dir-name/SKILL.md",
        &skill("shown", "A skill"),
    );
    put(
        &w.home,
        ".claude/skills/folder-only/SKILL.md",
        "First body line\n",
    );
    put(&w.repo, ".claude/commands/a/b.md", &doc("Nested command"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "shown").description, "A skill");
    assert_eq!(find(&items, "folder-only").description, "First body line");
    assert_eq!(find(&items, "a:b").source, SlashSource::Project);
    assert!(items.iter().all(|i| i.name != "dir-name"));
}

#[test]
fn user_invocable_false_is_left_out() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/hidden/SKILL.md",
        "---\nuser-invocable: false\n---\nx\n",
    );
    put(
        &w.repo,
        ".claude/skills/shown/SKILL.md",
        "---\nuser-invocable: true\n---\nx\n",
    );
    let items = rows(&w, &w.repo);
    assert!(items.iter().all(|i| i.name != "hidden"));
    find(&items, "shown");
}

#[test]
fn managed_beats_user_beats_project_for_skills_of_one_name() {
    let w = world();
    put(
        &w.managed,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "managed"),
    );
    put(
        &w.home,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "user"),
    );
    put(
        &w.repo,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "project"),
    );
    put(
        &w.home,
        ".claude/skills/two/SKILL.md",
        &skill("two", "user"),
    );
    put(
        &w.repo,
        ".claude/skills/two/SKILL.md",
        &skill("two", "project"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "dup").source, SlashSource::Managed);
    assert_eq!(find(&items, "two").source, SlashSource::User);
    assert_eq!(items.iter().filter(|i| i.name == "dup").count(), 1);
}

#[test]
fn a_skill_beats_a_command_file_even_from_a_lower_scope() {
    let w = world();
    put(&w.home, ".claude/commands/ship.md", &doc("command"));
    put(
        &w.repo,
        ".claude/skills/ship/SKILL.md",
        &skill("ship", "skill"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "ship").description, "skill");
    assert_eq!(find(&items, "ship").source, SlashSource::Project);
}

#[test]
fn names_compare_case_insensitively() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/a/SKILL.md",
        &skill("Deploy", "user"),
    );
    put(&w.repo, ".claude/commands/deploy.md", &doc("project"));
    let items = rows(&w, &w.repo);
    assert_eq!(
        items
            .iter()
            .filter(|i| i.name.eq_ignore_ascii_case("deploy"))
            .count(),
        1
    );
    assert_eq!(find(&items, "Deploy").source, SlashSource::User);
}

#[test]
fn a_skill_replaces_the_built_in_row_but_a_command_file_does_not() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/compact/SKILL.md",
        &skill("compact", "my compact"),
    );
    put(&w.home, ".claude/commands/memory.md", &doc("my memory"));
    let items = rows(&w, &w.repo);
    assert_eq!(items.iter().filter(|i| i.name == "compact").count(), 1);
    assert_eq!(find(&items, "compact").source, SlashSource::User);
    assert_eq!(items.iter().filter(|i| i.name == "memory").count(), 1);
    assert_eq!(find(&items, "memory").source, SlashSource::BuiltIn);
}

#[test]
fn the_scan_reads_each_parent_up_to_the_repo_root_and_no_further() {
    let w = world();
    let folder = w.repo.join("apps/web");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".claude/skills/at-root/SKILL.md",
        &skill("at-root", "r"),
    );
    put(
        &w.repo,
        "apps/.claude/skills/in-middle/SKILL.md",
        &skill("in-middle", "m"),
    );
    put(
        &w.repo,
        "apps/web/.claude/skills/in-folder/SKILL.md",
        &skill("in-folder", "f"),
    );
    put(
        &w.repo,
        "apps/web/deeper/.claude/skills/below/SKILL.md",
        &skill("below", "b"),
    );
    put(
        w.repo.parent().unwrap(),
        ".claude/skills/above-repo/SKILL.md",
        &skill("above-repo", "a"),
    );
    let names: Vec<_> = rows(&w, &folder).into_iter().map(|i| i.name).collect();
    for name in ["at-root", "in-middle", "in-folder"] {
        assert!(names.contains(&name.to_owned()), "{name}");
    }
    for name in ["below", "above-repo"] {
        assert!(!names.contains(&name.to_owned()), "{name}");
    }
}

#[test]
fn the_nearer_folder_wins_between_two_project_skills() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "root"),
    );
    put(
        &folder,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "near"),
    );
    assert_eq!(find(&rows(&w, &folder), "dup").description, "near");
}

#[test]
fn a_folder_outside_any_repo_scans_only_itself() {
    let w = world();
    let loose = w.repo.parent().unwrap().join("loose/inner");
    put(&loose, ".claude/skills/here/SKILL.md", &skill("here", "h"));
    put(
        loose.parent().unwrap(),
        ".claude/skills/parent/SKILL.md",
        &skill("parent", "p"),
    );
    let names: Vec<_> = rows(&w, &loose).into_iter().map(|i| i.name).collect();
    assert!(names.contains(&"here".to_owned()));
    assert!(!names.contains(&"parent".to_owned()));
}

/// A linked worktree `wt` of `w.repo`; its `.git` file points into `repo/.git/worktrees/wt`.
fn worktree(w: &crate::slash::testing::World) -> std::path::PathBuf {
    let tree = w.repo.parent().unwrap().join("wt");
    put(&w.repo, ".git/worktrees/wt/commondir", "../..\n");
    put(
        &tree,
        ".git",
        &format!("gitdir: {}\n", w.repo.join(".git/worktrees/wt").display()),
    );
    tree
}

#[test]
fn a_linked_worktree_without_claude_skills_reads_the_main_checkout() {
    let w = world();
    let tree = worktree(&w);
    put(
        &w.repo,
        ".claude/skills/untracked/SKILL.md",
        &skill("untracked", "main only"),
    );
    put(&tree, ".claude/commands/local.md", &doc("worktree command"));
    let items = rows(&w, &tree);
    assert_eq!(find(&items, "untracked").description, "main only");
    find(&items, "local");
}

#[test]
fn a_linked_worktree_with_its_own_claude_skills_ignores_the_main_checkout() {
    let w = world();
    let tree = worktree(&w);
    put(
        &w.repo,
        ".claude/skills/untracked/SKILL.md",
        &skill("untracked", "main only"),
    );
    put(
        &tree,
        ".claude/skills/own/SKILL.md",
        &skill("own", "tracked"),
    );
    let names: Vec<_> = rows(&w, &tree).into_iter().map(|i| i.name).collect();
    assert!(names.contains(&"own".to_owned()));
    assert!(!names.contains(&"untracked".to_owned()));
}

#[test]
fn a_clone_never_reads_another_checkout() {
    let w = world();
    let other = w.repo.parent().unwrap().join("other");
    put(&other, ".git/HEAD", "ref: refs/heads/main\n");
    put(
        &other,
        ".claude/skills/elsewhere/SKILL.md",
        &skill("elsewhere", "e"),
    );
    let names: Vec<_> = rows(&w, &w.repo).into_iter().map(|i| i.name).collect();
    assert!(!names.contains(&"elsewhere".to_owned()));
}

#[test]
fn a_file_that_cannot_be_read_is_an_error_line_and_the_rest_still_lists() {
    let w = world();
    put(&w.repo, ".claude/commands/ok.md", &doc("fine"));
    std::fs::write(w.repo.join(".claude/commands/bad.md"), [0xff, 0xfe, 0xfd]).unwrap();
    let out = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(out.errors[0].contains("bad.md"));
    find(&out.items, "ok");
}

#[test]
fn a_file_with_broken_front_matter_is_listed_with_no_fields_set() {
    let w = world();
    put(
        &w.repo,
        ".claude/commands/odd.md",
        "---\nthis is not yaml\n---\nFirst line wins\n",
    );
    let item = find(&rows(&w, &w.repo), "odd").clone();
    assert_eq!(item.description, "First line wins");
    assert_eq!(item.argument_hint, None);
}

const HIDDEN: &str = "---\nuser-invocable: false\n---\nx\n";

#[test]
fn a_hidden_skill_still_owns_its_name_over_a_visible_lower_scope_skill() {
    let w = world();
    put(&w.home, ".claude/skills/ghost/SKILL.md", HIDDEN);
    put(
        &w.repo,
        ".claude/skills/ghost/SKILL.md",
        &skill("ghost", "visible project"),
    );
    put(&w.managed, ".claude/skills/spook/SKILL.md", HIDDEN);
    put(
        &w.home,
        ".claude/skills/spook/SKILL.md",
        &skill("spook", "u"),
    );
    let items = rows(&w, &w.repo);
    assert!(items.iter().all(|i| i.name != "ghost"));
    assert!(items.iter().all(|i| i.name != "spook"));
}

#[test]
fn a_hidden_skill_named_like_a_built_in_removes_the_built_in_row_too() {
    let w = world();
    put(&w.home, ".claude/skills/compact/SKILL.md", HIDDEN);
    assert!(rows(&w, &w.repo).iter().all(|i| i.name != "compact"));
}

#[test]
fn worktree_folders_come_before_the_main_checkout_for_one_kind() {
    let w = world();
    let tree = worktree(&w);
    put(&tree, ".claude/commands/same.md", &doc("worktree"));
    put(&w.repo, ".claude/commands/same.md", &doc("main"));
    put(
        &w.repo,
        ".claude/skills/x/SKILL.md",
        &skill("x", "main skill"),
    );
    put(&tree, ".claude/commands/x.md", &doc("worktree command"));
    let items = rows(&w, &tree);
    assert_eq!(find(&items, "same").description, "worktree");
    assert_eq!(find(&items, "x").description, "main skill");
}

// --- settings and plugins ---

use crate::slash::testing::World;

fn names(items: &[SlashItem]) -> Vec<String> {
    items.iter().map(|i| i.name.clone()).collect()
}

/// Installs plugin `key` at `home/store/<dir>` and lists it in `installed_plugins.json`.
fn install(w: &World, key: &str, dir: &str, manifest: &str) -> std::path::PathBuf {
    let path = w.home.join("store").join(dir);
    put(&path, ".claude-plugin/plugin.json", manifest);
    let index = w.home.join(".claude/plugins/installed_plugins.json");
    let mut plugins: serde_json::Value = std::fs::read_to_string(&index)
        .map(|t| serde_json::from_str(&t).unwrap())
        .unwrap_or_else(|_| serde_json::json!({"version": 2, "plugins": {}}));
    plugins["plugins"][key] =
        serde_json::json!([{ "scope": "user", "installPath": path.to_str().unwrap() }]);
    put(
        &w.home,
        ".claude/plugins/installed_plugins.json",
        &plugins.to_string(),
    );
    path
}

fn enable(w: &World, switches: &str) {
    put(
        &w.home,
        ".claude/settings.json",
        &format!("{{\"enabledPlugins\": {{{switches}}}}}"),
    );
}

#[test]
fn an_enabled_plugin_lists_its_skills_and_commands_under_its_manifest_name() {
    let w = world();
    let dir = install(&w, "kit@market", "kit", r#"{"name": "toolkit"}"#);
    put(&dir, "skills/deploy/SKILL.md", &skill("deploy", "Ship it"));
    put(
        &dir,
        "commands/lint.md",
        "---\nargument-hint: <path>\n---\nLint files\n",
    );
    put(&dir, "commands/sub/ignored.md", "x");
    enable(&w, r#""kit@market": true"#);
    let items = rows(&w, &w.repo);
    let deploy = find(&items, "toolkit:deploy");
    assert_eq!(deploy.source, SlashSource::Plugin("toolkit".into()));
    assert_eq!(deploy.description, "Ship it");
    let lint = find(&items, "toolkit:lint");
    assert_eq!(
        (lint.description.as_str(), lint.argument_hint.as_deref()),
        ("Lint files", Some("<path>"))
    );
    assert!(
        !names(&items).contains(&"toolkit:sub:ignored".to_owned()),
        "commands/*.md is not recursive"
    );
}

#[test]
fn a_plugin_is_on_unless_a_setting_or_its_manifest_turns_it_off() {
    let w = world();
    for (key, dir, manifest) in [
        ("plain@m", "plain", r#"{"name": "plain"}"#),
        (
            "off@m",
            "off",
            r#"{"name": "off", "defaultEnabled": false}"#,
        ),
        (
            "forced@m",
            "forced",
            r#"{"name": "forced", "defaultEnabled": false}"#,
        ),
        (
            "vetoed@m",
            "vetoed",
            r#"{"name": "vetoed", "defaultEnabled": true}"#,
        ),
    ] {
        let path = install(&w, key, dir, manifest);
        put(&path, "commands/c.md", "x\n");
    }
    enable(&w, r#""vetoed@m": false, "forced@m": true"#);
    let names = names(&rows(&w, &w.repo));
    assert!(
        names.contains(&"plain:c".to_owned()),
        "defaultEnabled is true when absent"
    );
    assert!(
        names.contains(&"forced:c".to_owned()),
        "a setting beats the manifest"
    );
    assert!(!names.contains(&"off:c".to_owned()));
    assert!(!names.contains(&"vetoed:c".to_owned()));
}

#[test]
fn skill_overrides_do_not_reach_plugin_skills() {
    let w = world();
    let dir = install(&w, "kit@m", "kit", r#"{"name": "toolkit"}"#);
    put(&dir, "skills/deploy/SKILL.md", &skill("deploy", "d"));
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"skillOverrides": {"toolkit:deploy": "off", "deploy": "off"}}"#,
    );
    find(&rows(&w, &w.repo), "toolkit:deploy");
}

#[test]
fn a_project_install_shows_only_inside_its_project() {
    let w = world();
    let here = install(&w, "here@m", "here", r#"{"name": "here"}"#);
    let there = install(&w, "there@m", "there", r#"{"name": "there"}"#);
    put(&here, "commands/c.md", "x\n");
    put(&there, "commands/c.md", "x\n");
    let index = w.home.join(".claude/plugins/installed_plugins.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index).unwrap()).unwrap();
    json["plugins"]["here@m"][0]["projectPath"] = w.repo.to_str().unwrap().into();
    json["plugins"]["there@m"][0]["projectPath"] = w.home.join("other").to_str().unwrap().into();
    put(
        &w.home,
        ".claude/plugins/installed_plugins.json",
        &json.to_string(),
    );
    let sub = w.repo.join("src");
    std::fs::create_dir_all(&sub).unwrap();
    let names = names(&rows(&w, &sub));
    assert!(
        names.contains(&"here:c".to_owned()),
        "at or below projectPath"
    );
    assert!(!names.contains(&"there:c".to_owned()), "another project");
}

#[test]
fn a_manifest_path_that_escapes_the_plugin_is_an_error_line_and_is_skipped() {
    let w = world();
    let dir = install(
        &w,
        "kit@m",
        "kit",
        r#"{"name": "kit", "commands": ["../escape.md", "ok.md"]}"#,
    );
    put(&dir, "ok.md", "x\n");
    put(&dir, "../escape.md", "x\n");
    let out = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(out.errors[0].contains("../escape.md"));
    let names = names(&out.items);
    assert!(names.contains(&"kit:ok".to_owned()));
    assert!(!names.contains(&"kit:escape".to_owned()));
}

#[test]
fn settings_merge_by_key_from_user_to_project_to_local_to_managed() {
    let w = world();
    for key in ["a", "b", "c"] {
        let path = install(
            &w,
            &format!("{key}@m"),
            key,
            &format!(r#"{{"name": "{key}"}}"#),
        );
        put(&path, "commands/c.md", "x\n");
    }
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"enabledPlugins": {"a@m": true, "b@m": true, "c@m": false}}"#,
    );
    put(
        &w.repo,
        ".claude/settings.json",
        r#"{"enabledPlugins": {"b@m": false, "c@m": true}}"#,
    );
    put(
        &w.repo,
        ".claude/settings.local.json",
        r#"{"enabledPlugins": {"b@m": true}}"#,
    );
    put(
        &w.managed,
        "managed-settings.json",
        r#"{"enabledPlugins": {"a@m": false}}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(!names.contains(&"a:c".to_owned()), "managed wins");
    assert!(
        names.contains(&"b:c".to_owned()),
        "local beats project beats user"
    );
    assert!(names.contains(&"c:c".to_owned()), "project beats user");
}

#[test]
fn a_manifest_commands_path_replaces_the_default_folder_and_skills_adds_to_it() {
    let w = world();
    let dir = install(
        &w,
        "kit@m",
        "kit",
        r#"{"name": "kit", "commands": ["./extra", "./single.md"], "skills": "./more"}"#,
    );
    put(&dir, "commands/default.md", "x\n");
    put(&dir, "extra/one.md", "x\n");
    put(&dir, "single.md", "Single\n");
    put(&dir, "skills/base/SKILL.md", &skill("base", "b"));
    put(&dir, "more/added/SKILL.md", &skill("added", "a"));
    enable(&w, r#""kit@m": true"#);
    let names = names(&rows(&w, &w.repo));
    for name in ["kit:one", "kit:single", "kit:base", "kit:added"] {
        assert!(names.contains(&name.to_owned()), "{name}");
    }
    assert!(
        !names.contains(&"kit:default".to_owned()),
        "commands replaces the default folder"
    );
}

#[test]
fn a_manifest_commands_map_gives_names_descriptions_and_hints() {
    let w = world();
    let dir = install(
        &w,
        "kit@m",
        "kit",
        r#"{"name": "kit", "commands": {
            "inline": {"content": "---\ndescription: From content\n---\nbody", "argumentHint": "<x>"},
            "filed": {"source": "./cmds/filed.md"},
            "told": {"source": "./cmds/filed.md", "description": "Declared"}
        }}"#,
    );
    put(&dir, "cmds/filed.md", "From the file\n");
    enable(&w, r#""kit@m": true"#);
    let items = rows(&w, &w.repo);
    let inline = find(&items, "kit:inline");
    assert_eq!(
        (inline.description.as_str(), inline.argument_hint.as_deref()),
        ("From content", Some("<x>"))
    );
    assert_eq!(find(&items, "kit:filed").description, "From the file");
    assert_eq!(find(&items, "kit:told").description, "Declared");
}

#[test]
fn disable_bundled_skills_hides_only_the_bundled_rows() {
    let w = world();
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"disableBundledSkills": true}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(!names.contains(&"simplify".to_owned()), "bundled skill");
    assert!(names.contains(&"compact".to_owned()), "built-in command");
}

#[test]
fn skill_overrides_off_hides_that_skill_and_other_values_keep_it() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/noisy/SKILL.md",
        &skill("noisy", "n"),
    );
    put(
        &w.repo,
        ".claude/skills/quiet/SKILL.md",
        &skill("quiet", "q"),
    );
    put(
        &w.repo,
        ".claude/skills/terse/SKILL.md",
        &skill("terse", "t"),
    );
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"skillOverrides": {"Noisy": "off", "terse": "name-only", "simplify": "off"}}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(
        !names.contains(&"noisy".to_owned()),
        "key matches case-insensitively"
    );
    assert!(names.contains(&"quiet".to_owned()));
    assert!(names.contains(&"terse".to_owned()));
    assert!(
        !names.contains(&"simplify".to_owned()),
        "a bundled skill too"
    );
}

#[test]
fn a_settings_file_that_does_not_parse_is_an_error_line_and_the_scan_goes_on() {
    let w = world();
    put(&w.home, ".claude/settings.json", "{ not json");
    put(
        &w.repo,
        ".claude/skills/still/SKILL.md",
        &skill("still", "s"),
    );
    let out = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(out.errors[0].contains("settings.json"));
    find(&out.items, "still");
}
