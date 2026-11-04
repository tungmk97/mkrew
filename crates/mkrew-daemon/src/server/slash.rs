//! `slash.list` (SPEC §20B.7, ADR 0094): the commands that typing `/` shows in the agent's own
//! terminal, read on the machine that runs the agent.

use std::collections::BTreeMap;
use std::ffi::OsString;

use mkrew_core::slash::{self, ScanRoots};
use mkrew_protocol::{ErrorCode, Outcome, SlashListParams, SlashListResult};

use super::{Daemon, ok};

impl Daemon {
    /// The commands of profile `params.profile_id`'s CLI for the project folder, or for the
    /// project's folder inside the task worktree `worktree_task_id` (resolved like `fs.*`, but
    /// no watcher is kept: a few small files are read once). A profile whose CLI is unknown
    /// answers `cli: None` and no items.
    pub(super) async fn slash_list(&self, params: SlashListParams) -> Outcome {
        let profile = match self.load_profile(&params.profile_id) {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let target = match self
            .target(params.project_id, params.worktree_task_id, None)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let Some(cli) = profile.cli else {
            return ok(&SlashListResult {
                cli: None,
                items: Vec::new(),
                errors: Vec::new(),
            });
        };
        // `~` means the home of the user the agent runs as: the daemon's.
        let Some(home) = std::env::home_dir() else {
            return Outcome::error(ErrorCode::Internal, "cannot determine the home directory");
        };
        let roots = ScanRoots::for_machine(
            target.dir,
            home,
            merged_env(std::env::vars_os(), &profile.env),
        );
        let overrides = profile.slash;
        let listed =
            tokio::task::spawn_blocking(move || slash::list(cli, &roots, &overrides)).await;
        match listed {
            Ok(list) => ok(&SlashListResult {
                cli: Some(cli),
                items: list.items,
                errors: list.errors,
            }),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("slash.list failed: {e}")),
        }
    }
}

/// The daemon's environment with the profile's `env` on top: what the agent would see. A name
/// or value that is not Unicode names no setting the scan reads, so it is left out.
fn merged_env(
    daemon: impl IntoIterator<Item = (OsString, OsString)>,
    profile: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = daemon
        .into_iter()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    env.extend(
        profile
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    env
}
