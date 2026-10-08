# 0024 — Font settings for the app and the terminal

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user asked to configure the font family and size of the app and of the terminal. UI sizes
were `f32` constants in `theme::font_size`, the UI family was the `UI_FONT` const, and the
terminal family was picked once at startup into a `OnceLock`.

## Options

1. `[ui]` / `[terminal]` in `settings.toml`; UI sizes become `Rems` relative to body and the
   window's rem size is `[ui] font_size` — GPUI's own mechanism; the call-site edits are
   mechanical (`px(font_size::X)` → `font_size::X`).
2. `Theme` size fields multiplied by a scale — every site has to read the theme, a larger diff.
3. Terminal only — leaves half of the request undone.

## Decision

Option 1. A `theme::Fonts` GPUI global is built from `Settings` and the installed font list;
`Theme::with_fonts` copies it into every theme (Shell and terminal panes). A terminal family that
is not installed falls back to the platform list, so the grid never becomes proportional.
`settings.toml` is re-read when its mtime changes, checked on the existing 500 ms status tick —
no watcher crate, no protocol event. A broken file keeps the last good fonts; the UI lists the
error (`file:line:col`) at the top of the command palette as `Settings error: …`, which opens the
file (§20.0). Palette `Global: Open settings` writes a commented template if the file is missing
and opens it in Zed.

## Consequences

- SPEC §14 and §20.0 describe the keys, the 8–32 px range and the live reload.
- The terminal size also drives the Files and Git code views (they read `Theme`).
- Docs preview sizes (ADR 0021) stay fixed px; spacing stays px.
- Text that sets no size follows GPUI's default `rems(1.)`, so it moves from 16 px to the UI
  body size (13 px by default).
- A terminal font change goes through the existing "theme changed → clear row cache → re-fit
  PTY" path; no new resize code.
