//! `git.*` handlers (SPEC §7, §12B).

use mkrew_protocol::{ErrorCode, Event, GitSetBaseParams, Outcome};

use super::{Daemon, store_outcome};

impl Daemon {
    /// Sets the Git view base of a project (`None` or blank = auto-detect) and broadcasts
    /// `project.updated`. The ref is not checked against the repo: it may only appear after
    /// a fetch.
    pub(super) fn git_set_base(&self, params: GitSetBaseParams) -> Outcome {
        let base = params.base_ref.as_deref().map(str::trim);
        let base = base.filter(|b| !b.is_empty());
        if let Some(base) = base
            && let Err(reason) = check_ref(base)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let updated = self.store().set_project_base(params.project_id, base);
        if let Ok(project) = &updated {
            let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        }
        store_outcome(updated.map(|_| ()))
    }
}

/// Rejects what can never be a ref and would change a git command line: a leading `-` (read
/// as an option), whitespace and control characters.
fn check_ref(base: &str) -> Result<(), String> {
    if base.starts_with('-') {
        return Err(format!("invalid base ref {base:?}: starts with '-'"));
    }
    if base.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "invalid base ref {base:?}: contains whitespace or control characters"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_ref;

    #[test]
    fn base_refs_that_git_could_read_as_options_are_rejected() {
        for ok in ["main", "origin/main", "release/1.2", "HEAD~3", "a1b2c3"] {
            assert_eq!(check_ref(ok), Ok(()), "{ok}");
        }
        for bad in ["-f", "--force", "a b", "a\tb", "a\nb"] {
            assert!(check_ref(bad).is_err(), "{bad:?}");
        }
    }
}
