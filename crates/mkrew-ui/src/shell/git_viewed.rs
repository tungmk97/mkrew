//! Git view "Mark as viewed" (SPEC §12B, ADR 0065): asking the daemon which marks still stand,
//! and toggling one.

use crate::*;

impl Shell {
    /// `git.viewed` of the Git view of `project_id`: which listed files are still viewed. Asked
    /// after every status answer and after every toggle; a failure keeps the last set.
    pub(crate) fn load_git_viewed(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.wants_viewed() {
            return;
        }
        let at = view.at();
        let (params, serial) = view.begin_viewed();
        let asked = (params.mode, params.since.clone());
        let loaded = connection.call::<Vec<ViewedFile>>(Request::GitViewed(params));
        cx.spawn_in(window, async move |this, cx| {
            let files = match loaded.await {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(%project_id, "git.viewed failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                if let Some(view) = shell.git_at(project_id, &at) {
                    view.viewed_loaded(serial, asked, files);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Marks `target` viewed or not (the `v` key, a row's box, the toolbar button): shown at
    /// once, then sent; the daemon is asked again afterwards, so a failure just puts its truth
    /// back.
    pub(crate) fn toggle_git_viewed(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).toggle_viewed(&target) else {
            return;
        };
        cx.notify();
        let sent = connection.request(Request::GitSetViewed(params));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(error) = failure {
                    tracing::warn!(%project_id, path = target.path, "git.set_viewed failed: {error}");
                }
                shell.load_git_viewed(project_id, window, cx);
            });
        })
        .detach();
    }

    /// The `v` key and the palette command: the selected file of the shown Git view.
    pub(crate) fn toggle_selected_viewed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(target) = self
            .git
            .get(&project_id)
            .and_then(|view| view.selected.clone())
        else {
            return;
        };
        self.toggle_git_viewed(project_id, target, window, cx);
    }
}
