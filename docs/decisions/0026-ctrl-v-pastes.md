# 0026 — Plain Ctrl+V pastes in terminal panes

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user expects Ctrl+V to paste text and images into an agent pane, as in Windows Terminal.
Mkrew only pasted on Ctrl+Shift+V; plain Ctrl+V sent ^V, which Codex reads as "paste an image"
(and errors on text) and Claude Code ignores on Windows (its image key is Alt+V). Windows
bitmaps (PrintScreen) were dropped because GPUI reads them as BMP.

## Options

1. Plain Ctrl+V = the Ctrl+Shift+V paste; ^V only when the clipboard has nothing usable.
2. A per-profile key to forward for images (`image_paste_key`), so agents read the clipboard.
3. Keep Ctrl+Shift+V only; fix BMP.

## Decision

Option 1, decided in `terminal::input::key_action` like Ctrl+C (ADR 0022). Text is pasted
(bracketed when asked); an image (BMP re-encoded as PNG) is saved through `attachment.save` and
its path typed; Explorer-copied files type their paths. It works for every agent and shell
without per-agent code (§2.1). Cost: a program that wants a literal ^V gets it only with an
empty clipboard — the same trade Windows Terminal makes. Not rebindable (outside the keymap,
like Ctrl+C).

## Consequences

SPEC §13 gets a second plain-Ctrl exception; the Copy / paste row reads "Ctrl+V or
Ctrl+Shift+V". SPEC §20B.5 names both keys, BMP → PNG and copied files. The composer's Ctrl+V
also pastes an image as its saved path. New UI dependency: `image` (png + bmp only; already in
the tree through gpui).
