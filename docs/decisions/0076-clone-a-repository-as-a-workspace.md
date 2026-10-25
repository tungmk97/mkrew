# 0076 — Add a workspace by cloning a Git repository

- Date: 2026-10-25
- Status: accepted (SPEC §7, §10; protocol 48)
- Milestone: none (user request 2026-10-25)

## Context

"Add workspace" only opened the OS folder picker. To work on a repository from GitHub, GitLab or
any other host, the user had to clone it in a terminal first and then add the folder.

## Decision

- "Add workspace" (sidebar `+`, empty state, workspace switcher, Ctrl+Shift+K, palette) opens a
  dialog: **Open a folder…** (the old picker), or a repository URL, the folder the clone goes in
  (default: the folder holding the active workspace, else the home folder; **Browse…** changes
  it) and **Clone** (Enter). The clone is `<folder>/<repository name>`, previewed under the URL.
- The daemon clones: `project.clone { clone_id, url, parent }` checks the URL names a folder,
  `parent` is an existing absolute folder and `<parent>/<name>` is absent or empty, answers `null`
  at once, runs `git clone --quiet -- <url> <name>` in the background and ends with
  `project.cloned { clone_id, project?, error? }` (plus `project.updated` when added). A clone can
  take minutes; one connection handles requests in order, so it must not block the reply.
- No login of our own: git uses the machine's setup (Git Credential Manager for HTTPS opens its
  own sign-in window; SSH keys). `GIT_TERMINAL_PROMPT=0` and no console, so missing credentials
  fail instead of hanging.

## Consequences

- Esc closes the dialog while a clone runs; the clone goes on, shows up in the sidebar when done,
  or a toast says why it failed.
- Not done: browsing a host's repositories through its API, picking a branch, shallow clones,
  progress percent. A credential helper that waits forever for input keeps "Cloning…" shown.
