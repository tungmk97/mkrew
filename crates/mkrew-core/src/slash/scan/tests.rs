use super::*;
use crate::slash::testing::{put, world};

#[test]
fn the_scan_climbs_to_the_repo_root_and_stops_there() {
    let w = world();
    let deep = w.repo.join("a/b");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(
        project_dirs(&deep),
        [deep.clone(), w.repo.join("a"), w.repo.clone()]
    );
    assert_eq!(project_dirs(&w.repo), std::slice::from_ref(&w.repo));
}

#[test]
fn without_a_repo_the_scan_is_the_folder_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("loose/inner");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(project_dirs(&folder), [folder]);
}

#[test]
fn a_git_file_marks_a_repo_root_like_a_git_folder() {
    let w = world();
    let tree = w.repo.join("tree");
    put(&tree, ".git", "gitdir: elsewhere\n");
    let inner = tree.join("pkg");
    std::fs::create_dir_all(&inner).unwrap();
    assert_eq!(project_dirs(&inner), [inner.clone(), tree.clone()]);
}

/// A linked worktree of `repo` named `wt`, as `git worktree add` writes it.
fn linked(w: &crate::slash::testing::World, link: &str) -> PathBuf {
    let tree = w.repo.parent().unwrap().join("wt");
    put(&w.repo, ".git/worktrees/wt/commondir", "../..\n");
    put(&tree, ".git", link);
    tree
}

#[test]
fn a_linked_worktree_maps_its_folder_into_the_main_checkout() {
    let w = world();
    let link = format!("gitdir: {}\n", w.repo.join(".git/worktrees/wt").display());
    let tree = linked(&w, &link);
    let folder = tree.join("apps/web");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(main_checkout_folder(&folder), Some(w.repo.join("apps/web")));
    assert_eq!(main_checkout_folder(&tree), Some(w.repo.clone()));
}

#[test]
fn a_relative_gitdir_is_read_from_the_worktree_root() {
    let w = world();
    let tree = linked(&w, "gitdir: ../repo/.git/worktrees/wt\n");
    assert_eq!(main_checkout_folder(&tree), Some(w.repo.clone()));
}

#[test]
fn a_clone_or_a_broken_link_has_no_main_checkout() {
    let w = world();
    assert_eq!(main_checkout_folder(&w.repo), None, "a clone");
    let tree = w.repo.parent().unwrap().join("wt");
    put(&tree, ".git", "gitdir: nowhere/at/all\n");
    assert_eq!(main_checkout_folder(&tree), None, "no commondir");
    put(&tree, ".git", "not a link\n");
    assert_eq!(main_checkout_folder(&tree), None, "not a gitdir line");
}

#[test]
fn a_skill_is_its_folder_and_a_command_is_its_path_joined_by_colons() {
    let w = world();
    put(
        &w.repo,
        "s/alpha/SKILL.md",
        "---\nname: renamed\n---\nUse it\n",
    );
    put(&w.repo, "s/not-a-skill/README.md", "x");
    put(&w.repo, "c/top.md", "Top line\nmore\n");
    put(
        &w.repo,
        "c/frontend/component.md",
        "---\ndescription: Make one\n---\n",
    );
    put(&w.repo, "c/frontend/notes.txt", "x");
    put(&w.repo, "c/UPPER.MD", "x");
    let mut errors = Vec::new();
    let skills = skill_docs(&w.repo.join("s"), &mut errors);
    assert_eq!(skills.len(), 1);
    assert_eq!(
        (skills[0].key.as_str(), skills[0].fm.name.as_deref()),
        ("alpha", Some("renamed"))
    );
    assert_eq!(skills[0].description(None), "Use it");

    let mut keys = |recursive| {
        command_docs(&w.repo.join("c"), recursive, &mut errors)
            .into_iter()
            .map(|d| d.key)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(true), ["UPPER", "frontend:component", "top"]);
    assert_eq!(keys(false), ["UPPER", "top"]);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn descriptions_fall_back_and_omp_cuts_the_first_line() {
    let w = world();
    put(
        &w.repo,
        "c/a.md",
        "\n\n  A very long first line that goes on and on and on past sixty characters\n",
    );
    put(
        &w.repo,
        "c/b.md",
        "---\nwhen_to_use: After a merge\n---\nBody\n",
    );
    put(
        &w.repo,
        "c/c.md",
        "---\ndescription: >\n  Two\n  lines\n---\n",
    );
    let docs = command_docs(&w.repo.join("c"), false, &mut Vec::new());
    assert_eq!(
        docs[0].description(Some(60)),
        "A very long first line that goes on and on and on past sixty..."
    );
    assert!(docs[0].description(None).ends_with("sixty characters"));
    assert_eq!(docs[1].description(None), "After a merge");
    assert_eq!(docs[2].description(None), "Two lines");
}

#[test]
fn an_unreadable_file_is_one_error_line_and_a_missing_folder_is_silent() {
    let w = world();
    put(&w.repo, "c/ok.md", "fine\n");
    std::fs::write(w.repo.join("c/binary.md"), [0xff, 0xfe, 0x00, 0xc3]).unwrap();
    let mut errors = Vec::new();
    let docs = command_docs(&w.repo.join("c"), false, &mut errors);
    assert_eq!(docs.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("binary.md"), "{errors:?}");
    assert!(command_docs(&w.repo.join("missing"), true, &mut errors).is_empty());
    assert_eq!(errors.len(), 1);
}
