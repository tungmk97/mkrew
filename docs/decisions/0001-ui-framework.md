# 0001 — UI framework: GPUI vs Tauri

- Date: 2026-10-05
- Status: proposed
- Milestone: M0

Draft: only Context is written; Options/Decision/Consequences are filled in by T0.9.

## Context

SPEC §4 picks `gpui` + `gpui-component` for the UI, `portable-pty` for PTYs and
`alacritty_terminal` for emulation, with Tauri + React + xterm.js as the fallback (§19). M0 is
the gate. Findings below were checked on 2026-10-05 against crates.io, docs.rs and GitHub
(research spike T0.2). Items marked UNVERIFIED could not be confirmed from a primary source.

### gpui

- crates.io `gpui` latest is **0.2.2 (2025-10-22)** — stale relative to Zed `main`
  (<https://crates.io/api/v1/crates/gpui>). API: `Application::new().run(|cx: &mut App| …)`.
- Zed `main` has split gpui into `gpui` + `gpui_platform` + per-OS crates (`gpui_windows`,
  `gpui_macos`, `gpui_linux`, `gpui_wgpu`, …). Entry point is now
  `gpui_platform::application().run(…)`. `gpui_platform` is not on crates.io, so the newest
  API requires a git dependency on `zed-industries/zed`. Whether Zed recommends a rev/tag for
  external users: UNVERIFIED.
- Snapshot crates on crates.io:
  - `gpui-pre` **0.3.7 (2026-09-28)**, weekly snapshot of zed@1a28cff published by the
    gpui-component maintainers; imported as `gpui` via `package = "gpui-pre"`.
  - `gpui-ce` 0.2.2 (2026-08-28), community fork; its 0.3.x releases were yanked.
- Core API (current): `Render` trait (`fn render(&mut self, &mut Window, &mut Context<Self>)
  -> impl IntoElement`), `cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::
  Windowed(Bounds::centered(None, size(px(w), px(h)), cx))), .. }, |_, cx| cx.new(|_| View))`,
  `div()` builder, `cx.activate(true)`.
- Windows: Zed stable on Windows since 2025-10-15; renderer is DirectX 11, text via
  DirectWrite (<https://zed.dev/blog/zed-for-windows-is-here>). No cargo features needed.
  **Build prerequisite:** `gpui_windows/build.rs` compiles HLSL with `fxc.exe` in release
  builds (looks in `GPUI_FXC_PATH`, then `PATH`, then the newest Windows 10 SDK); panics if not
  found. Debug builds skip it. ⇒ MSVC toolchain + Windows SDK required for release builds.
- How Zed draws its terminal (`crates/terminal_view/src/terminal_element.rs`): per line, cells
  with equal style are merged into batched text runs (wide-char spacer cells skipped, their
  background kept); backgrounds are `paint_quad(fill(..))` rects; text is
  `window.text_system().shape_line(text, font_size, &[run], Some(cell_width))` then
  `.paint(..)` — the `Some(cell_width)` forces glyphs onto the cell grid. Block/shade glyphs are
  drawn as quads. This matches SPEC §10 ("styled text runs per row") and is the model for T0.5.

### gpui-component

- Latest **0.7.0 (2026-09-28)**; repo moved to `longbridge/gpui-kit`; umbrella crate
  `gpui-kit = "0.7"` with `gpui_kit::application()` + `gpui_kit::init(cx)`.
- Pins `gpui-pre = "=0.3.7"` exactly. It **cannot share types with a git `gpui`** in the same
  build: choosing gpui-component means choosing `gpui-pre` as the gpui source.
- Platforms: macOS, Windows, Linux, wasm. 75+ components (dock with splits/tabs, virtual
  list, code editor, tables, markdown). No terminal component — we draw the grid ourselves.

### portable-pty

- Latest **0.9.0 (2025-02-11)**, no release since; lives in `wezterm/wezterm`.
- API: `native_pty_system().openpty(PtySize { rows, cols, pixel_width, pixel_height })` →
  `PtyPair { master, slave }`; `slave.spawn_command(CommandBuilder::new(..))`;
  `master.try_clone_reader()`, `master.take_writer()` (once; drop = EOF), `master.resize(..)`.
  Errors are `anyhow::Error`.
- Windows gotchas (from wezterm/microsoft-terminal issues #463, #1396, discussion #4674,
  terminal#4564): the reader does **not** reach EOF when the child exits — wait on the child
  on its own thread, then drop the master (ClosePseudoConsole may block; keep it off the UI
  thread); dropping the pair too early yields 0xC0000142; exit code 259 = STILL_ACTIVE; no job
  object, so grandchildren survive. Bundled `conpty.dll`: UNVERIFIED.

### alacritty_terminal

- Latest **0.26.0 (2026-04-06)**; depends on and re-exports `vte 0.15`. 0.26:
  `ChildEvent::Exited`/`Event::ChildExit` carry `ExitStatus`; 0.25: `Options::hold` →
  `drain_on_exit`.
- Embedding: `Term::new(Config, &impl Dimensions, impl EventListener)`; `Dimensions` needs
  `columns()`, `screen_lines()`, `total_lines()`; feed bytes with
  `vte::ansi::Processor::<StdSyncHandler>::new().advance(&mut term, bytes)`;
  read with `term.renderable_content()` (`display_iter`, `cursor`, `display_offset`, `colors`,
  `mode`); `term.resize(dims)`.
- Cells: `Cell { c, fg, bg, flags, .. }`; `Color = Named | Spec(Rgb) | Indexed(u8)`; `Flags`
  include `INVERSE`, `BOLD`, `ITALIC`, `DIM`, `HIDDEN`, `WIDE_CHAR`, `WIDE_CHAR_SPACER`,
  `LEADING_WIDE_CHAR_SPACER`, underline variants. `RenderableCursor` fields and the
  `display_iter` item type (`Indexed<&Cell>`?) UNVERIFIED — check docs.rs in T0.3.
- Zed uses a fork (`zed-industries/alacritty`, pinned git rev) and alacritty's own `tty` +
  event loop instead of portable-pty. Alacritty's `tty` module (with ConPTY) is a possible
  replacement for portable-pty if the latter's Windows issues bite.

### Choices for the spike (assumed defaults, see HUMAN_CHECKS)

- gpui source: `gpui-pre = "=0.3.7"` (imported as `gpui`), so the spike runs on the exact gpui
  that `gpui-component` 0.7 needs in M2+. Upstream git `gpui` would be newer but would rule out
  `gpui-component`.
- PTY: keep `portable-pty` 0.9 per SPEC §4; mitigate the Windows EOF issue with a child-wait
  thread. Revisit (alacritty `tty`) only if T0.3/T0.6 hit blockers.
- Terminal: crates.io `alacritty_terminal` 0.26 (not Zed's fork).
- If `gpui-pre` / `gpui-kit` are kept, T0.9 must update the SPEC §4 table (crate names,
  gpui-component's new home `longbridge/gpui-kit`) in the same commit as the ADR decision.
- Measurements (T0.8) must use a release build, which needs `fxc.exe` (Windows SDK) or
  `GPUI_FXC_PATH`.

## Options

_To be written in T0.9 (GPUI vs Tauri + React + xterm.js, with measurements from T0.8)._

## Decision

_Pending (T0.9)._

## Consequences

_Pending (T0.9)._
