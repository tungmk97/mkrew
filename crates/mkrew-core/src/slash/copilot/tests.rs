use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{World, doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Copilot, &w.roots(folder), &SlashOverrides::default());
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
fn built_ins_come_first_then_skills_then_commands() {
    let w = world();
    put(&w.repo, ".github/skills/s/SKILL.md", &skill("zeta", "Z"));
    put(&w.repo, ".claude/commands/alpha.md", &doc("A"));
    let items = rows(&w, &w.repo);
    let at = |n: &str| items.iter().position(|i| i.name == n).unwrap();
    assert!(at("add-dir") < at("move") && at("move") < at("zeta") && at("zeta") < at("alpha"));
    assert_eq!(find(&items, "zeta").source, SlashSource::Project);
}

#[test]
fn the_first_found_wins_in_the_documented_order() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/a/SKILL.md",
        &skill("dup", "claude"),
    );
    put(
        &w.repo,
        ".agents/skills/a/SKILL.md",
        &skill("dup", "agents"),
    );
    put(
        &w.home,
        ".copilot/skills/a/SKILL.md",
        &skill("dup", "copilot home"),
    );
    put(
        &w.repo,
        ".github/skills/a/SKILL.md",
        &skill("dup", "github"),
    );
    put(
        &w.home,
        ".agents/skills/b/SKILL.md",
        &skill("user-dup", "agents home"),
    );
    put(
        &w.home,
        ".copilot/skills/b/SKILL.md",
        &skill("user-dup", "copilot home"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(items.iter().filter(|i| i.name == "dup").count(), 1);
    assert_eq!(find(&items, "dup").description, "github");
    assert_eq!(find(&items, "user-dup").description, "copilot home");
    assert_eq!(find(&items, "user-dup").source, SlashSource::User);
}

#[test]
fn parents_contribute_only_their_github_skills_up_to_the_repo_root() {
    let w = world();
    let folder = w.repo.join("pkg/inner");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".github/skills/a/SKILL.md",
        &skill("from-root", "r"),
    );
    put(
        &w.repo,
        "pkg/.agents/skills/b/SKILL.md",
        &skill("parent-agents", "no"),
    );
    put(
        &folder,
        ".agents/skills/c/SKILL.md",
        &skill("own-agents", "yes"),
    );
    put(
        w.repo.parent().unwrap(),
        ".github/skills/d/SKILL.md",
        &skill("past-root", "no"),
    );
    let items = rows(&w, &folder);
    find(&items, "from-root");
    find(&items, "own-agents");
    assert!(
        items
            .iter()
            .all(|i| i.name != "parent-agents" && i.name != "past-root")
    );
}

#[test]
fn a_skill_beats_a_command_of_the_same_name_and_hidden_skills_are_left_out() {
    let w = world();
    put(&w.repo, ".claude/commands/ship.md", &doc("command"));
    put(
        &w.repo,
        ".github/skills/s/SKILL.md",
        &skill("ship", "skill"),
    );
    put(
        &w.repo,
        ".github/skills/h/SKILL.md",
        "---\nname: quiet\nuser-invocable: false\n---\nx\n",
    );
    put(
        &w.repo,
        ".claude/commands/quiet.md",
        &doc("shadowed by the hidden skill"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "ship").description, "skill");
    assert!(
        items.iter().all(|i| i.name != "quiet"),
        "the skill won the name, then was left out"
    );
}

#[test]
fn commands_are_read_from_the_folder_only_and_not_recursively() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".claude/commands/root.md", &doc("r"));
    put(&folder, ".claude/commands/own.md", &doc("o"));
    put(&folder, ".claude/commands/sub/deep.md", &doc("d"));
    let items = rows(&w, &folder);
    find(&items, "own");
    assert!(
        items
            .iter()
            .all(|i| i.name != "root" && i.name != "deep" && i.name != "sub:deep")
    );
}

#[test]
fn copilot_skills_dirs_come_from_the_given_environment() {
    let w = world();
    let (a, b) = (w.home.join("extra-a"), w.home.join("extra-b"));
    put(&a, "x/SKILL.md", &skill("from-a", "A"));
    put(&b, "y/SKILL.md", &skill("from-b", "B"));
    let mut roots = w.roots(&w.repo);
    let joined = std::env::join_paths([&a, &b]).unwrap();
    roots.env.insert(
        "COPILOT_SKILLS_DIRS".into(),
        joined.to_str().unwrap().into(),
    );
    let items = list(Cli::Copilot, &roots, &SlashOverrides::default()).items;
    assert_eq!(find(&items, "from-a").source, SlashSource::User);
    find(&items, "from-b");
    let none = list(Cli::Copilot, &w.roots(&w.repo), &SlashOverrides::default()).items;
    assert!(none.iter().all(|i| i.name != "from-a"));
}
