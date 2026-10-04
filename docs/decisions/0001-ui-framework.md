# 0001 — UI framework: GPUI vs Tauri

- Date: 2026-10-05
- Status: proposed
- Milestone: M0

Recommendation written (T0.9); waiting for the human gate (HUMAN_CHECKS "Waiting (blocking)").

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

## Measurements (T0.8, 2026-10-05)

Harness: `bash scripts/measure-spike.sh [flood-MiB]` builds release and runs three windowed
runs with `--stats` (own RSS/CPU via `sysinfo`, frames, output rate, key latency printed to
stderr once a second; logs in `target/measure/`). Headless PTY baseline:
`cargo test --release -p spike-term --test flood_pty -- --nocapture`.
Host: native Windows 11 (26100), 16 logical CPUs, release build, one terminal running
`cmd.exe` at the default 80x24 window size. CPU % is of **one core** (÷16 for whole machine).

| What (SPEC §15 target) | Result | vs target |
| --- | --- | --- |
| Idle RAM, one terminal (UI < 150 MB) | 66–101 MB RSS (working set; varies run to run) | OK |
| Idle CPU (UI + daemon < 1 %) | 1.5–6 %, mean 3.5–4 % of one core (~0.25 % of the machine) | over, if "%" means one core |
| Key → painted echo (< 16 ms) | p50 22–23 ms, p95 36–37 ms (33–38 synthetic keys/run; one 527 ms outlier) | over |
| Flood 50 MiB (`--flood 50`) | 5.8 MB/s, 32–35 fps, frame gap max 35–53 ms, UI CPU ~26 % of one core, RSS flat at 61–70 MB, took 8.7 s | RAM/CPU OK; frame gap over 16 ms |
| PTY flood baseline, no UI (1 MiB) | ~7.7 MB/s | — |

Interpretation:

- Key latency is measured from the PTY write to the end of the first paint after the echo
  arrived; it excludes OS input dispatch and GPU present (so the real number is higher), and
  includes the ConPTY + `cmd.exe` echo round trip and up to one 16 ms poll tick. The poll timer
  is the first suspect for both latency and idle CPU (it wakes the UI 60×/s with nothing to do).
  Fix: wake the UI on PTY output instead of polling (T0.8a), then re-measure.
- Flood: RAM stays flat and the UI uses about a quarter of a core while consuming ~75 % of
  the PTY's own headless rate. But the frame rate is ~34 fps, not 60: the gap between frames
  (35–53 ms) is the stand-in for input latency during a flood (the flood child does not read
  stdin, so there is no echo to time), and it misses §15's 16 ms. Suspects: the poll tick
  stacked on frame time, and the 1 MiB per-frame drain budget (`pty.rs`). Same follow-up.
  ConPTY itself delivers ~7.7 MB/s here (a 50 MB flood takes ~7 s at best); that ceiling
  applies to any UI (GPUI or Tauri).
- Only the UI process is sampled; conhost/OpenConsole and the child are not included.
- Not measured: real keyboard input (needs a human; HUMAN_CHECKS), several panes (M2+).

### After wake-on-output (T0.8a, 2026-10-05)

Change: the 16 ms poll is gone. The PTY reader thread appends every read to one shared buffer
(capped at 4 MiB, then it blocks the child) and signals a capacity-1 wake channel; the UI awaits
it, drains (≤ 1 MiB), and after output waits for that frame's paint (50 ms fallback) before the
next drain — i.e. at most one drain per display refresh (§10). Same host and harness; this
host's display refreshes at **165 Hz** (GPUI paints up to ~166 fps).

| What (SPEC §15 target) | T0.8 (poll) | T0.8a (wake) | vs target |
| --- | --- | --- | --- |
| Idle RAM (UI < 150 MB) | 66–101 MB | 65 MB | OK |
| Idle CPU (< 1 %) | mean 3.5–4 % of one core | mean 3.3–3.8 %, 0 frames painted | over (GPUI floor, see below) |
| Key → painted echo (< 16 ms) | p50 22 / p95 37 ms | p50 3.6–4.4 / p95 6.1–6.6 ms (33 keys/run; one ~508 ms startup outlier) | **OK** |
| Flood 50 MiB | 5.8 MB/s, ~34 fps, gap max 35–53 ms, CPU ~26 % | 8.9–9.5 MB/s, ~165 fps, gap max 18–24 ms (mean ~6 ms), CPU ~79 %, RSS flat 62–70 MB, 5.7 s | throughput OK; gap max slightly over 16 ms |
| PTY flood baseline, no UI (1 MiB) | ~7.7 MB/s | ~6.6 MB/s (short run, noisy) | — |

Interpretation (two T0.8a targets are **missed**: idle CPU < 1 % of a core, and flood frame
gap < 16 ms; both are open inputs for T0.9):

- Key latency is fixed: the echo is drained the moment the reader thread sees it and painted
  on the next refresh.
- Idle CPU did not move with the poll gone (the UI now does zero work and paints zero frames
  while idle). The floor is GPUI's own: on Windows its `VSyncProvider` thread calls
  `RedrawWindow(RDW_INVALIDATE)` on every window each vsync and checks for device loss
  (`gpui-pre-windows` 0.3.7 `platform.rs` `begin_vsync_thread`), so an idle window still
  handles ~165 `WM_PAINT`s/s. Not fixable from the app without patching gpui; §15's < 1 % is
  met only if read as % of the machine (~0.25 % of 16 cores). Input for T0.9.
- Flood: throughput now exceeds the old headless figure (the 64-chunk channel was a
  bottleneck), and frames track the display refresh. The cost is CPU: a fully dirty 80x24 grid
  is re-shaped ~165×/s (~0.8 core). Tried and rejected: a 16 ms timer between drains (Windows
  timer granularity made it ~31 ms: 50 fps, gap 35–40 ms, 34 % CPU, 10 MB/s); no pacing at all
  (~85 % CPU, no better gaps). If CPU matters more than refresh rate, a 60 Hz cap (skip paints
  closer than ~16 ms) is a small change; the remaining gap spikes (18–24 ms, per-second max) are single slow frames.

## Options

Evidence for GPUI comes from the `spike-term` build (T0.3–T0.8a). Tauri was **not built**; its
column is from general knowledge of WebView2 + xterm.js and is marked as such.

| Concern | A. GPUI (`gpui-pre` 0.3.7 + `gpui-component` 0.7) | B. Tauri + React + xterm.js |
| --- | --- | --- |
| Terminal rendering | Own cell-grid renderer (Zed's approach): styled runs per row, dirty rows, one paint per frame. Snapshot tests on the emulator grid (colors, cursor, alt screen, wide chars, a hand-made TUI frame) pass. Real agent TUIs not yet seen by a human. | xterm.js: mature, used by VS Code; agent TUIs known to render well. Nothing to build. |
| Key → glyph (< 16 ms) | p95 6.1–6.6 ms (synthetic, PTY write → paint end) | Not measured; adds a Rust → webview IPC hop per output chunk and per key. |
| Idle RAM (< 150 MB) | 65 MB RSS, one terminal | Not measured; a WebView2 app typically starts around 100–200 MB across its processes. |
| Idle CPU (< 1 %) | ~3.5 % of one core (~0.25 % of the machine), floor set by GPUI's Windows vsync thread; app does zero work | Not measured; an idle webview is usually near 0 %. |
| Output flood | ~9 MB/s (above the 6.6–7.7 MB/s headless PTY rate), frames at display refresh (165 Hz), gap max 18–24 ms, ~0.8 core, RAM flat | Not measured; xterm.js WebGL handles floods well, but every byte crosses IPC (base64/JSON or channel). |
| Windows native | Builds and runs on Windows 11 (DirectX 11, DirectWrite); release needs `fxc.exe` (Windows SDK) | WebView2 is preinstalled on Windows 11; mature. |
| IME / input | Spike has no `InputHandler`; IME and dead keys need work in M2 | Browser handles IME. |
| Rest of the app (§10–§12B: dock, lists, markdown, diffs, code view) | `gpui-component` has dock/tabs, virtual list, editor, markdown, tables | Huge React ecosystem. |
| Architecture fit (§2, §3) | One language, one native binary, shares types with daemon/protocol crates | Two languages + a JS build toolchain; types must be mirrored in TS. |
| Supply chain / churn | `gpui-pre` is a third-party weekly snapshot of Zed (gpui-component maintainers); API churns; gpui-component pins it exactly | Stable, widely used release process. |

Both options keep the daemon, CLI, protocol and store unchanged (§19): the choice only affects
`mkrew-ui`.

## Decision

**Recommendation: A — GPUI** (`gpui-pre = "=0.3.7"` as `gpui`, `gpui-component` 0.7 via
`gpui-kit`), **pending the human checks** in `docs/HUMAN_CHECKS.md` → "Waiting (blocking)".
Status stays *proposed* until the human confirms.

Why: every automated M0 target that GPUI controls is met or nearly met (latency 6.6 ms p95, RAM
65 MB, flood throughput at the ConPTY ceiling), the renderer is the same design Zed ships, and
staying in Rust keeps the single-binary, shared-types architecture of §3. The two misses are
bounded: idle CPU is ~0.25 % of the machine and caused by GPUI itself; flood CPU can be cut with
a 60 Hz paint cap, while the 18–24 ms flood gap spikes (single slow frames, mean ~6 ms) remain an
open miss to profile in M2. Neither is a reason to take on a second language and a webview.

Switch to B (Tauri) if any of these turn out true in the human checks:

- a real `claude` / `codex` TUI renders with broken frames, wrong colors or misaligned glyphs
  that are not a small renderer bug;
- typing, paste or resize misbehave in ways that point at GPUI rather than spike code;
- the idle CPU floor (~3.5 % of a core per window) or IME support is judged unacceptable.

## Consequences

If GPUI is confirmed:

- SPEC §4 UI row names the actual crates: `gpui-pre` (imported as `gpui`; the spike also uses
  `gpui-pre-platform` for the entry point until `gpui-kit`'s `application()` replaces it) +
  `gpui-kit` (gpui-component, now `longbridge/gpui-kit`), pinned. Updated now with the
  recommendation; reverted if Tauri is chosen.
- `mkrew-ui` (M2) inherits from the spike: the row renderer and dirty-row cache, wake-on-output
  draining paced by paint, and the key mapping. It must add: an `InputHandler` for IME/dead
  keys, an optional 60 Hz paint cap during floods, keeping a finished session's last screen,
  and PTY writes off the UI thread (the daemon owns them from M1).
- Release builds on Windows need the Windows SDK (`fxc.exe`) or `GPUI_FXC_PATH`; CI must have it.
- gpui upgrades follow `gpui-component` releases (exact pin); budget time for API churn.
- Idle CPU (~3.5 % of a core) stays until upstream gpui stops invalidating idle windows each
  vsync; §15's "< 1 %" is read as % of the machine for now.

If Tauri is chosen instead: write ADR 0002, replace the §4 UI row, and rebuild only the UI on
top of the M1 daemon protocol; the spike's measurements above become the baseline to beat.
