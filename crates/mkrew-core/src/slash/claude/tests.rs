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
