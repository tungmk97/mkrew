# 0098 - macOS keyboard: Cmd defaults, symbol labels, Mac text keys

- Date: 2026-11-22
- Status: accepted (amends SPEC §13)
- Milestone: none (human request 2026-11-22, macOS audit 2026-11-21)

## Context

SPEC §13 said "On macOS, `Ctrl` becomes `Cmd`", but the code had one set of defaults, all Ctrl and Alt.
On a Mac, Option types characters, so `alt-1..9` and `alt-arrows` took `[ ] { } | ~` and the word jump away from every field and terminal.
macOS reports Shift + digit or symbol as the shifted character and gpui-pre-macos does not fold it back, so 15 defaults such as `ctrl-shift-2` never matched.
A MacBook needs Fn for F-keys and PgUp / PgDn, F11 is Show Desktop, and Ctrl + arrows switch Spaces.
Labels said "Ctrl", "Alt" and "Win", and text fields used Windows caret keys.

## Decision

- `keymap::Os` (Windows, Mac, Linux) is passed explicitly to the pure functions that choose defaults, labels and text keys, so the macOS rules are unit-tested on Windows.
- macOS defaults: Cmd takes Ctrl's place; no default uses Option + a character key, Shift + a digit or symbol, an F-key or PgUp / PgDn / Home / End; Ctrl+Option keys stay because they type nothing and are not system shortcuts. The table is in SPEC §13 "macOS defaults". Commands added later get a row there too: Toggle chat view is ⇧⌘T and Toggle widget bar ⇧⌘B (their PC keys, Ctrl+Alt+T and Ctrl+Alt+Shift+J, would otherwise stay on the Mac).
- macOS shortcut text uses ⌃ ⌥ ⇧ ⌘ in Apple order with no separator, and ↩ ⌫ ⌦ ⇥ for those keys.
- macOS text fields and the Editor: Option moves and deletes by word, Cmd goes to the line's or text's ends and Cmd+Backspace / Cmd+Delete delete to them, Cmd+Shift+Z redoes, and the Emacs keys Ctrl+A E B F P N D H K work.
- On macOS an Option + character key types the character macOS composed (Option+B is "∫", German Option+5 is "["), as today.
- CI compiles and lints every target on macOS; it does not run the tests there, because they pin the PC defaults.

## Alternatives

- One set of defaults with `secondary-` (Ctrl on PC, Cmd on Mac): fixes Ctrl but keeps the Option, Shift + digit and Fn problems.
- Fold Shift + digit into the shifted char on macOS like gpui-pre-windows does: depends on the layout and shows "⌘@" instead of a digit.
- `#[cfg(target_os = "macos")]` defaults: cannot be compiled or tested on the Windows dev machine.

## Consequences

A `keymap.toml` written on Windows keeps its keys on a Mac; an entry that clashes with a macOS default is reported and that entry keeps its default.
Existing macOS users get new defaults at the next start; their uncommented entries still win.
