# 0063 — Closing the pane when a run command ends is optional

- Date: 2026-10-19
- Status: accepted
- Amends: 0018

## Context

ADR 0018 typed an exit after every run command, so its pane left the run tab as soon as the
command ended. A command that fails or prints something short vanished with its output.

## Decision

- `RunCommand` gets `exit_after` (default `false`). `run::typed_line(shell, command, exit)` types
  only the command when it is off; when on it adds the per-shell exit as before (`& exit` for
  cmd, `; exit $LASTEXITCODE` for pwsh/powershell, `; exit` otherwise).
- Off: the shell stays open after the command, showing its output; stopping it still closes the
  pane. On: the pane leaves the run tab when the command ends.
- Run setup has a `Close the pane when it ends` checkbox per command, off by default.
- Store migration 0020 adds `run_command.exit_after INTEGER NOT NULL DEFAULT 0`: saved commands
  become off. `PROTOCOL_VERSION` 44, since an older daemon would ignore the flag and keep exiting.
- Run setup folds every card but the one holding the keyboard to one line (number, name,
  command, folder; Duplicate / Remove on hover); a click on it opens it at Command. Eight full
  cards were a wall of fields.
- The fields draw their text as one `StyledText` with the caret painted at its layout position
  (`caret_field`), so a click puts the caret under the pointer (`Input::click`), Shift+click or a
  drag selects, a double / triple click selects the word / line. Before, the text was split into
  pieces around the caret and a click only focused the field. The task and ask dialogs share the
  widget but keep editing at the end, so a click there only focuses.

## Consequences

Existing commands no longer close their pane on their own until the box is ticked. A command that
ended with the shell left open counts as running until the shell is stopped.
