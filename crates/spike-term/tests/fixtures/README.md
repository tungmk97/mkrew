# Terminal fixtures

Each `*.ans` file is terminal output written in an escaped text form, so fixtures stay
readable, diffable and immune to git line-ending conversion:

- Literal line breaks in the file are ignored (use them for readability only). Never end a
  line with significant whitespace; editors strip it.
- `\e` = ESC, `\r` = CR, `\n` = LF, `\\` = backslash, `\xNN` = one raw byte (hex).
- Any other character is emitted as its UTF-8 bytes.

`tests/grid_snapshots.rs` decodes a fixture, feeds it into `Emulator::advance` and snapshots the
grid dump with `insta` (`tests/snapshots/`).

`tui_frame.ans` is hand-written, modelled on a Claude Code screen (no real agent may run in
tests). Replacing it with a real recording is tracked in `docs/HUMAN_CHECKS.md`.
