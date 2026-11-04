use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{World, doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Omp, &w.roots(folder), &SlashOverrides::default());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    out.items
}

fn find<'a>(items: &'a [SlashItem], name: &str) -> &'a SlashItem {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

fn count(items: &[SlashItem], name: &str) -> usize {
    items.iter().filter(|i| i.name == name).count()
}

fn at(items: &[SlashItem], name: &str) -> usize {
    items
        .iter()
        .position(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

#[test]
fn the_order_is_built_ins_bundled_group_skills_commands_templates() {
    let w = world();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("alpha", "A"));
    put(&w.repo, ".omp/commands/mine.md", &doc("Mine"));
    put(&w.home, ".omp/agent/prompts/tpl.md", &doc("Template"));
    let items = rows(&w, &w.repo);
    let order = [
        "security",
        "quit",
        "green",
        "annotate",
        "init",
        "skill:",
        "skill:alpha",
        "mine",
        "tpl",
    ];
    let places: Vec<_> = order.iter().map(|n| at(&items, n)).collect();
    assert!(places.windows(2).all(|p| p[0] < p[1]), "{places:?}");
}

#[test]
fn skills_collapse_into_a_group_row_that_counts_them() {
    let w = world();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("alpha", "A"));
    put(&w.home, ".omp/agent/skills/b/SKILL.md", &skill("beta", "B"));
    let items = rows(&w, &w.repo);
    let group = find(&items, "skill:");
    assert_eq!(
        (group.group, group.description.as_str()),
        (Some(2), "2 skills")
    );
    assert_eq!(find(&items, "skill:alpha").source, SlashSource::Project);
    assert_eq!(find(&items, "skill:beta").source, SlashSource::User);
    assert_eq!(find(&items, "skill:alpha").group, None);
}

#[test]
fn without_skills_there_is_no_group_row() {
    let w = world();
    assert_eq!(count(&rows(&w, &w.repo), "skill:"), 0);
}

#[test]
fn skill_clashes_go_to_the_provider_priority_and_project_beats_user_inside_omp() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/c/SKILL.md",
        &skill("dup", "claude"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/p/SKILL.md",
        &skill("dup", "plugin"),
    );
    put(
        &w.home,
        ".omp/agent/skills/u/SKILL.md",
        &skill("dup", "user"),
    );
    put(&w.repo, ".omp/skills/n/SKILL.md", &skill("dup", "project"));
    put(
        &w.home,
        ".omp/agent/skills/x/SKILL.md",
        &skill("lower", "user"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/y/SKILL.md",
        &skill("lower", "plugin"),
    );
    put(
        &w.repo,
        ".claude/skills/z/SKILL.md",
        &skill("lowest", "claude"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/w/SKILL.md",
        &skill("lowest", "plugin"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "skill:dup"), 1);
    assert_eq!(find(&items, "skill:dup").description, "project");
    assert_eq!(find(&items, "skill:lower").description, "user");
    let lowest = find(&items, "skill:lowest");
    assert_eq!(lowest.description, "plugin");
    assert_eq!(lowest.source, SlashSource::Plugin("pkg".into()));
}

#[test]
fn a_skill_name_comes_from_front_matter_else_the_folder_and_slashes_drop_it() {
    let w = world();
    put(&w.repo, ".omp/skills/folder-name/SKILL.md", "Plain skill\n");
    put(&w.repo, ".omp/skills/odd/SKILL.md", &skill("a/b", "x"));
    put(&w.repo, ".omp/skills/odd2/SKILL.md", &skill("a\\b", "x"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "skill:folder-name").description, "Plain skill");
    assert!(
        items
            .iter()
            .all(|i| !i.name.contains('/') && !i.name.contains('\\'))
    );
}

#[test]
fn omp_skill_folders_climb_to_the_repo_root_but_claude_skills_do_not() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("from-root", "r"));
    put(
        &w.repo,
        ".claude/skills/b/SKILL.md",
        &skill("claude-root", "r"),
    );
    let items = rows(&w, &folder);
    find(&items, "skill:from-root");
    assert_eq!(count(&items, "skill:claude-root"), 0);
}

#[test]
fn command_clashes_go_to_the_provider_priority() {
    let w = world();
    put(&w.repo, ".opencode/commands/dup.md", &doc("opencode"));
    put(&w.repo, ".codex/commands/dup.md", &doc("codex"));
    put(&w.repo, ".agents/commands/dup.md", &doc("agents"));
    put(&w.repo, ".claude/commands/dup.md", &doc("claude"));
    put(&w.repo, ".omp/commands/dup.md", &doc("omp"));
    put(&w.repo, ".opencode/commands/low.md", &doc("opencode"));
    put(&w.repo, ".codex/commands/low.md", &doc("codex"));
    put(&w.repo, ".codex/commands/lower.md", &doc("codex"));
    put(&w.repo, ".agents/commands/lower.md", &doc("agents"));
    put(&w.repo, ".claude/commands/lowest.md", &doc("claude"));
    put(&w.repo, ".agent/commands/lowest.md", &doc("agent"));
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "dup"), 1);
    assert_eq!(find(&items, "dup").description, "omp");
    assert_eq!(find(&items, "low").description, "codex");
    assert_eq!(find(&items, "lower").description, "agents");
    assert_eq!(find(&items, "lowest").description, "claude");
}

#[test]
fn inside_omp_the_project_beats_the_user_and_inside_agents_the_user_beats_the_project() {
    let w = world();
    put(&w.home, ".omp/agent/commands/a.md", &doc("user"));
    put(&w.repo, ".omp/commands/a.md", &doc("project"));
    put(&w.home, ".agents/commands/b.md", &doc("user"));
    put(&w.repo, ".agents/commands/b.md", &doc("project"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "a").description, "project");
    assert_eq!(find(&items, "a").source, SlashSource::Project);
    assert_eq!(find(&items, "b").description, "user");
    assert_eq!(find(&items, "b").source, SlashSource::User);
}

#[test]
fn agents_commands_climb_to_the_repo_root() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".agent/commands/up.md", &doc("up"));
    put(
        w.repo.parent().unwrap(),
        ".agent/commands/past-root.md",
        &doc("no"),
    );
    let items = rows(&w, &folder);
    find(&items, "up");
    assert_eq!(count(&items, "past-root"), 0);
}

#[test]
fn a_claude_command_in_a_folder_gives_the_bare_name_and_the_colon_name() {
    let w = world();
    put(&w.repo, ".claude/commands/foo/bar.md", &doc("Nested"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "bar").description, "Nested");
    assert_eq!(find(&items, "foo:bar").description, "Nested");
}

#[test]
fn descriptions_use_camel_case_hints_and_cut_the_first_line_at_sixty_characters() {
    let w = world();
    put(
        &w.repo,
        ".omp/commands/hinted.md",
        "---\nargumentHint: <file>\n---\nOpen a file\n",
    );
    let long = "x".repeat(70);
    put(&w.repo, ".omp/commands/long.md", &format!("{long}\n"));
    let items = rows(&w, &w.repo);
    assert_eq!(
        find(&items, "hinted").argument_hint.as_deref(),
        Some("<file>")
    );
    assert_eq!(
        find(&items, "long").description,
        format!("{}...", "x".repeat(60))
    );
}

#[test]
fn a_template_hides_behind_any_row_with_its_name_and_keeps_its_subfolder_out_of_the_name() {
    let w = world();
    put(
        &w.home,
        ".omp/agent/prompts/model.md",
        &doc("hidden by the built-in"),
    );
    put(
        &w.home,
        ".omp/agent/prompts/deep/er/note.md",
        &doc("User note"),
    );
    put(&w.repo, ".omp/prompts/note.md", &doc("Project note"));
    put(&w.repo, ".omp/commands/taken.md", &doc("command"));
    put(
        &w.repo,
        ".omp/prompts/taken.md",
        &doc("hidden by the command"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "model"), 1);
    assert_eq!(find(&items, "model").source, SlashSource::BuiltIn);
    let notes: Vec<_> = items.iter().filter(|i| i.name == "note").collect();
    assert_eq!(notes.len(), 2, "templates do not hide each other");
    assert_eq!(
        (notes[0].source.clone(), notes[1].source.clone()),
        (SlashSource::User, SlashSource::Project)
    );
    assert_eq!(find(&items, "taken").description, "command");
}

#[test]
fn the_bundled_init_shows_only_when_no_file_command_is_named_init() {
    let w = world();
    assert_eq!(count(&rows(&w, &w.repo), "init"), 1);
    put(&w.repo, ".omp/commands/init.md", &doc("My init"));
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "init"), 1);
    assert_eq!(find(&items, "init").description, "My init");
}
