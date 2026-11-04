//! `slash.list` (SPEC §20B.7, ADR 0094): the agent's command list, read on the daemon's machine.

use mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource};

use super::*;

fn slash_list(project: &Project, profile: &str, worktree_task_id: Option<Ulid>) -> Request {
    Request::SlashList(SlashListParams {
        profile_id: profile.into(),
        project_id: project.id,
        worktree_task_id,
    })
}

fn listed(client: &mut Client, request: Request) -> SlashListResult {
    client.call(request).into_result().unwrap()
}

fn find<'a>(list: &'a SlashListResult, name: &str) -> Option<&'a SlashItem> {
    list.items.iter().find(|item| item.name == name)
}

/// `<dir>/<name>.md` with a front matter `description`.
fn write_command(dir: &Path, name: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let text = format!("---\ndescription: {description}\n---\nDo it.\n");
    std::fs::write(dir.join(format!("{name}.md")), text).unwrap();
}

/// `<dir>/<name>/SKILL.md` with a front matter `name` and `description`.
fn write_skill(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir.join(name)).unwrap();
    let text = format!("---\nname: {name}\ndescription: A skill\n---\nDo it.\n");
    std::fs::write(dir.join(name).join("SKILL.md"), text).unwrap();
}

/// The folder is the project's, or with `worktree_task_id` the project's folder inside the
/// task's worktree (resolved like `fs.*`); the profile's `[[slash.add]]` rows come along.
#[test]
fn slash_list_scans_the_project_folder_or_its_task_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    env.write_profile(
        "cl",
        "command = \"claude\"\ncli = \"claude\"\n\n[[slash.add]]\nname = \"deploy\"\n\
         description = \"Deploy to staging\"\nargument_hint = \"<env>\"\n",
    );
    let repo = env.repo();
    let app = repo.join("app");
    write_command(&app.join(".claude/commands"), "ship", "Ship it");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Slash".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    // Only in the worktree: not tracked, written after the checkout.
    write_command(&root.join("app/.claude/commands"), "wt-only", "Only here");

    let in_project = listed(&mut client, slash_list(&project, "cl", None));
    assert_eq!(in_project.cli, Some(Cli::Claude));
    let ship = find(&in_project, "ship").unwrap();
    assert_eq!(
        (&ship.source, ship.description.as_str()),
        (&SlashSource::Project, "Ship it")
    );
    assert!(find(&in_project, "wt-only").is_none());
    let deploy = find(&in_project, "deploy").unwrap();
    assert_eq!(
        (&deploy.source, deploy.argument_hint.as_deref()),
        (&SlashSource::Profile, Some("<env>"))
    );

    let in_worktree = listed(&mut client, slash_list(&project, "cl", Some(task.id)));
    assert!(find(&in_worktree, "wt-only").is_some());
    assert!(
        find(&in_worktree, "ship").is_some(),
        "the tracked command is in the checkout too"
    );
    client.stop();
}

/// `~` is the daemon user's home and the profile's `env` wins over the daemon's own
/// (Copilot reads `COPILOT_SKILLS_DIRS`).
#[test]
fn slash_list_reads_the_home_and_the_profile_env_of_the_agents_machine() {
    let env = Env::new();
    let home = env.dir.path().join("home");
    let (daemon_skills, profile_skills) = (
        env.dir.path().join("daemon-skills"),
        env.dir.path().join("profile-skills"),
    );
    write_skill(&home.join(".copilot/skills"), "from-home");
    write_skill(&daemon_skills, "from-daemon-env");
    write_skill(&profile_skills, "from-profile-env");
    env.write_profile("plain", "command = \"copilot\"\ncli = \"copilot\"\n");
    env.write_profile(
        "own-env",
        &format!(
            "command = \"copilot\"\ncli = \"copilot\"\n\
             env = {{ COPILOT_SKILLS_DIRS = '{}' }}\n",
            profile_skills.display()
        ),
    );
    let repo = env.repo();
    let (_daemon, mut client) = env.start_with(&[
        ("HOME", home.as_path()),
        ("USERPROFILE", home.as_path()),
        ("COPILOT_SKILLS_DIRS", daemon_skills.as_path()),
    ]);
    let project = client.add_project(&repo);

    let plain = listed(&mut client, slash_list(&project, "plain", None));
    assert_eq!(plain.cli, Some(Cli::Copilot));
    assert!(find(&plain, "from-home").is_some());
    assert!(find(&plain, "from-daemon-env").is_some());
    assert!(find(&plain, "from-profile-env").is_none());

    let own = listed(&mut client, slash_list(&project, "own-env", None));
    assert!(find(&own, "from-home").is_some());
    assert!(find(&own, "from-profile-env").is_some());
    assert!(
        find(&own, "from-daemon-env").is_none(),
        "the profile's env replaces the daemon's value"
    );
    client.stop();
}

/// An unknown CLI is an empty answer, not an error; a missing profile, project or worktree is.
#[test]
fn slash_list_names_what_it_cannot_scan() {
    let env = Env::new();
    env.write_profile("custom", "command = \"aider\"\n");
    env.write_profile("cl", "command = \"claude\"\ncli = \"claude\"\n");
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    assert_eq!(
        listed(&mut client, slash_list(&project, "custom", None)),
        SlashListResult {
            cli: None,
            items: Vec::new(),
            errors: Vec::new()
        }
    );
    assert_eq!(
        error_code(client.call(slash_list(&project, "nope", None))),
        ErrorCode::NotFound
    );
    let other = Project {
        id: Ulid::new(),
        ..project.clone()
    };
    assert_eq!(
        error_code(client.call(slash_list(&other, "cl", None))),
        ErrorCode::NotFound
    );
    let plain: Task = client
        .call(create_task(&project, "No worktree", None))
        .into_result()
        .unwrap();
    assert_eq!(
        error_code(client.call(slash_list(&project, "cl", Some(plain.id)))),
        ErrorCode::InvalidState,
        "the task has no worktree"
    );
    client.stop();
}
