# 0095 - The New task dialog takes images and dropped files

- Date: 2026-11-04
- Status: accepted (amends SPEC §20B.1 and §20B.5, ADR 0060 for attachments)
- Milestone: none (human request 2026-11-04, spec `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` section 6)

## Context

The task dialog's Prompt box shares `composer::Prompt` with the composer, but it ignored pasted images and dropped files.
SPEC §20B.1 and a HUMAN_CHECKS decision said why: attachments were stored per session (`attachment.save` took a `session_id`), and a new task has no session until it runs.
A new task has no task id either: the daemon makes it at `task.create`.

## Decision

- `attachment.save` takes `owner`: `{"session": id}` or `{"project": id}` instead of `session_id` (protocol 53, no compatibility shim: the version check rejects an old client).
- `Session` keeps `<data>/attachments/<session_id>/<ulid>.<ext>`.
- `Project` writes `<data>/attachments/projects/<project_id>/<ulid>.<ext>`.
- An unknown session or project is `not_found`.
- The composer and the panes send `Session`; the task dialog sends `Project(dialog.project_id())`, for a new task and for an edit.
- Retention (ADR 0060): the hourly check keeps the session rule and deletes a file under `projects/<id>/` only when it is older than 7 days, no task that is not done names it, and no task done within the last 7 days (its `updated_at`) names it.
  The age is the time in the ULID of the file name, which `attachment.save` wrote, so a copy of the file does not reset it.
  When the tasks cannot be read, nothing is deleted.
- Ctrl+V with an image in the dialog's Prompt box saves it and types its path, through the code the composer uses (`prompt_key_down`, ADR 0094).
  The Title field, and a dialog that is saving, take no image.
- The Prompt box takes OS file drops.
  Paths are typed with the pane's rules (`composer::dropped_files`).
  While a drag is over the box it shows the accent ring.
  The box uses the pane's `drop_hover` slot and `drop_catcher` (the gpui-pre 0.3.7 OLE workaround) with a new `DropTarget::TaskPrompt`.
- File tree row drags onto the dialog are out of scope: the dialog's modal shade covers the tree, so a row drag cannot start while the dialog is open.
  No handler for tree rows is built.
  They can be added if the modal changes.
- `type_into_pane` keeps ignoring drops while a dialog is open; the dialog's box handles its own.
- A machine window refuses OS files for the dialog as for a pane (ADR 0089).

## Alternatives

- A folder per task id: a new task has no id before save, so the file would have to move after `task.create`, and the prompt text would change after the user saw it.
- A client generated task id: changes `task.create` and the store for one feature.
- Keeping the image bytes in the task row: prompts are text sent to the agent as a path.
- Deleting project images with the project or the task only: a crashed daemon or an abandoned dialog would leave files forever; the age rule bounds them.

## Consequences

- A task in the backlog keeps its screenshot as long as it is not done; a done task's screenshot goes 7 days after it was done (and never before the file is 7 days old).
- An image pasted and then abandoned (dialog cancelled) goes after 7 days, like a session's.
- Editing a task so its prompt no longer names an image releases the file for the next check after 7 days.
- OS file drops onto the dialog's Prompt box work and Ctrl+V image paste works.
  File tree row drags cannot start while the modal shade covers the tree, so they stay out of scope until the modal changes.
- Every client of `attachment.save` migrates in this change: the composer, the pane's paste and the daemon tests.
- SPEC §20B.1, §20B.5 and the protocol section say so.
- Tests: `attachments_are_saved_per_project_and_unknown_projects_rejected` (daemon), `retention_keeps_the_screenshot_of_a_task_until_7_days_after_it_is_done` (daemon unit), `an_image_pasted_in_the_prompt_belongs_to_the_project` (UI).
