# 0099 - macOS terminal input, selection, scroll and IME

- Date: 2026-11-22
- Status: accepted
- Milestone: macOS audit 2026-11-22

## Context

An audit of the terminal pane on macOS found these problems.
It was done by reading the code and GPUI's macOS backend, not by running Mkrew on a Mac.

Cmd+C and Cmd+V did nothing in a terminal.
Plain Ctrl+C copied and plain Ctrl+V pasted (ADR 0022, ADR 0026).
That is a Windows Terminal habit: on a Mac Ctrl+C is the interrupt key and Ctrl+V is "quoted insert".

GPUI fills `key_char` with the Option-composed character (Option+B is "∫").
Option+key therefore sent `ESC ∫`, which is neither Meta nor the character.

GPUI gives a key that has a `key_char` to the view first, and to the input method only when the view leaves it alone.
The terminal sent the accent of a dead key at once, so Option+E then E never composed "é".

A double click or a triple click selected no more than a single click.

A trackpad sends pixel deltas with momentum.
One swipe sent dozens of arrow keys to `less` or an agent TUI.
The part of a line left over from the old direction delayed the first line after a change of direction.

The input handler ignored the range of `replace_text_in_range` and returned nothing from `text_for_range`.
With an IME input source active the first printable key reached the program as ASCII, because the handler never asked for printable keys.

The cell size is 13 px x 1.75 = 22.75 px.
GPUI snaps every quad to device pixels on its own (`Window::paint_quad`), so at 2x a row is 45 or 46 device pixels high depending on its index.

## Options

Option key:

1. Option always composes (the Terminal.app and iTerm2 default), no setting.
2. Option is always Meta, no setting.
3. `[terminal] option_as_meta`, default off.

Copy and paste:

1. On macOS Cmd+C / Cmd+V are the keymap's Copy and Paste commands, and plain Ctrl+C / Ctrl+V are never copy and paste.
2. Keep plain Ctrl+C / Ctrl+V everywhere.

Dead keys:

1. Let Option keys that compose go to the input method (what Zed's terminal does).
2. Detect dead keys in the terminal. GPUI gives no signal for them.

Trackpad:

1. Scale pixel deltas that go to a program, and drop the carried fraction when the direction changes.
2. Cap the lines per event. This needs a second constant and still lets a slow swipe run away.

## Decision

Option key 3, copy and paste 1, dead keys 1, trackpad 1.

With `option_as_meta` off (the default), a macOS Option+key press sends nothing from the key handler.
GPUI then hands the key to the input method, which types the composed character through `ime::Typing`.
With it on, Option+key sends `ESC` plus the key without the composition (Shift on a letter gives the capital).
Keys that are not text (arrows, Backspace, Enter, function keys) keep their xterm encoding with Option as the Alt modifier.
The setting has no effect on Windows and Linux.

`terminal::input::KeyEnv { os, option_as_meta }` carries both facts.
It is a GPUI global set at start and by `apply_settings_file`, so a change in `settings.toml` or the Settings dialog applies to open panes at once.

On macOS `is_copy_key` and `is_paste_key` are false: Ctrl+C is always ^C and Ctrl+V is always ^V.
Cmd+C is `Command::Copy`: it copies a selection and does nothing without one, and the keystroke is spent, so it never becomes ^C.
Cmd+V is `Command::Paste`.
Other Cmd keys are not sent to the program.

A double click selects the word under the pointer (alacritty's semantic selection, so a bracket selects through its partner), a triple click selects the line.
A drag after either extends by words or lines.
`Emulator::selection_text` never ends with a line break, so a copied line pasted into a shell does not run.
This applies on every OS.

A pixel (precise) scroll delta counts `PIXEL_WHEEL_SCALE` = 0.5 lines per line of travel when it goes to a program (wheel reports or arrow keys), and 1.0 when it scrolls the history.
The carried fraction is dropped when the direction changes.
This applies on every OS.

`ime::Typing` keeps the composing text (`ime::Marked`) as its document.
`text_for_range` returns a slice of it, and `replace_text_in_range` with a range erases back to the start of the range and types the replacement and what followed it.
Ranges count UTF-16 units.
Ranges are honored on macOS only: Windows Telex was written and tested with every range meaning "all composing text", so there a range stays that.
`prefers_ime_for_printable_keys` is true, so with an input mode that is not ASCII-capable (Japanese kana, Chinese, Korean) printable keys reach the input method first (only GPUI's macOS backend asks for it).
GPUI does not do this for ASCII-capable modes (Telex, VNI, Romaji): there plain letters reach the view first and the terminal sends them as typed.

`terminal::view::grid_cell_size` rounds the cell width and the line height to whole device pixels with the window's scale factor (`layout::snap_to_device`).
The Files editor keeps `cell_size`, because its text is laid out by the font's own advance.
This applies on every OS.

## Consequences

SPEC §13 says that the ADR 0022 and ADR 0026 exceptions are for Windows and Linux, and adds the macOS rules and the Option setting.
SPEC §10 gets the selection, scroll and cell rules.
SPEC §20.0 lists `[terminal] option_as_meta`.
The Settings dialog gets a "Option as Meta" switch on the General page (shown on every OS, like "Close to tray").
The settings type `FontSettings` is renamed `TerminalSettings`.

The Mac-only parts are written as runtime branches on `Os`, so Windows CI tests them.
Nothing in this ADR could be run on a Mac: `docs/HUMAN_CHECKS.md` lists what to try.

Ctrl+Space (^@) is taken by macOS's input source switch when two or more input sources are enabled.
That is a macOS setting and is not changed here.

## Not changed

- **"SF Mono" in `TERMINAL_FONTS`.**
  `all_font_names` returns the CoreText family names of all fonts registered with the system (`create_for_all_families`), and `pick_family` skips a name that is not in it.
  So "SF Mono" is used when it is installed and skipped otherwise, and Menlo is always there.
  The entry is not dead.
- **`DIAGRAM_FONT`.**
  On macOS the list resolves to Helvetica Neue, a system font.
  `mermaid-rs-renderer` maps `-apple-system` and `system-ui` to the generic `sans-serif`, which `fontdb` resolves to Arial, not to San Francisco, so adding them would make diagrams worse.
- **Option+Left / Option+Right in a shell.**
  They keep the xterm encoding `CSI 1;3D` / `CSI 1;3C` that agent TUIs read.
  Terminal.app sends `ESC b` / `ESC f` instead, and zsh's defaults bind only those.
  Changing it is a policy choice for the user.
- **Cmd+K (clear), Cmd+A (select all), Cmd+Left / Cmd+Backspace line editing in a terminal.**
  They need new commands in the keymap (Plan A's file), palette entries and SPEC rows on every OS.
- **Unmodified dead keys on non-US layouts (a French "^", a Spanish "´").**
  GPUI gives no dead key signal, and sending every printable key through the input method would make held keys show the accent picker instead of repeating.
- **Horizontal trackpad scrolling in a terminal.**
  No program the terminal hosts uses it, and `ScrollWheelEvent.delta.x` stays ignored.
- **Middle click closes a file tab.**
  It stays for mouse users.
  A trackpad has the visible x on each tab (always on the active tab, on hover for the others), the right-click or two-finger-click menu with Close / Close others / Close to the right, and the Close file tab key.
  A Cmd+click or Option+click alias would be a second hidden gesture next to three visible ways.
- **Row heights of code and diff lists (`DIFF_FONT_SIZE` 12.5, `code_line_height`).**
  GPUI's list layout owns them, and nothing shows they alternate in height the way the terminal grid does.
  Look again after the Mac visual check.
- **The default font size 13.**
  A larger default for macOS would change SPEC §14 for one OS without a failing behavior.
- **The IME candidate window position.**
  It already opens at the terminal's cursor cell (`TerminalElement::paint` passes the cursor cell as `Typing.caret`).
  It opens a few cells right of the start of the composing text, because the text is typed in place and the cursor sits after it.
  Changing that needs the echo's timing; check on a Mac first.
