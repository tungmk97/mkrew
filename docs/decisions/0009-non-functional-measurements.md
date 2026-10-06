# 0009 — Non-functional measurements (SPEC §15)

- Date: 2026-10-06
- Status: accepted
- Milestone: M8

## Context

M8 asks for the §15 non-functional checks to be measured and recorded. Several already had
automated tests (flood stress `crates/mkrew-ui/tests/stress.rs`, git status
`git_view::status_of_a_10000_file_repo_meets_the_budget`); UI startup and idle CPU/memory with
open panes needed the real window, which `verify` cannot open.

## Options

1. One script per number, scraping process stats from the shell — duplicates the stress test's
   sampling, differs per OS.
2. An `#[ignore]`d test that opens the real window (`crates/mkrew-ui/tests/measure.rs`) plus
   `scripts/measure.sh` / `measure.ps1` running it and the existing perf tests in release —
   one sampler (`sysinfo`), same code on every OS.

## Decision

Option 2. The UI logs `first frame` and `first connected frame` (time since process start) on
its first renders; the measure test times launch → those log lines from the outside, so the
numbers include process start. It runs six fake agents (built-in `ask` scenario, waiting on
their question) in one tab of six panes, waits 5 s, then averages CPU over 10 s and reads
resident memory (Windows: working set) of the UI and the daemon (agents excluded). The scripts
only report; budgets stay asserted where they already were (release `stress`, release git test).

## Results

Windows 11 Pro 10.0.26100, developer desktop, release builds, 2026-10-06.

| §15 row | Budget | Measured | Verdict |
| --- | --- | --- | --- |
| UI startup (daemon running) | first frame < 500 ms | 370 ms first frame, 402 ms first connected frame (warm); 3.9 s on the first run after a build (cold exe, antivirus scan) | ok (warm) |
| Idle CPU, 6 sessions, no output | UI + daemon < 1% | UI 1.9–2.5%, daemon 0.0% of a core | **over** (UI) → T8.9a |
| Memory, 6 sessions | UI < 150 MB, daemon < 50 MB | UI 60–109 MiB, daemon 19–24 MiB | ok |
| Throughput, 6 panes flooding | UI < 16 ms, RAM flat, daemon < 1 core | ~13 MiB/pane: round trip p50 0.19 ms / p95 0.48 ms, batch feed p50 4.0 ms / p95 8.5 ms, daemon 78% of a core, daemon RSS 35→35 MiB, client 130→131 MiB | ok |
| Git status, 10,000 files | < 150 ms on Windows | median 98 ms (94–112 ms) | ok |

Not measured here (→ `docs/HUMAN_CHECKS.md`): key press → glyph on screen (the stress test's
round trip and batch times are proxies, not a painted glyph), the 50 MB-per-pane flood (the test
uses ~13 MiB per pane), and Linux/macOS numbers. Security, data, errors and history rows are
behaviours covered by their own tests, not measurements.

## Consequences

- New: `scripts/measure.sh`, `scripts/measure.ps1`, `crates/mkrew-ui/tests/measure.rs`, the UI's
  two startup log lines. README links this table.
- Idle UI CPU is over budget: follow-up task T8.9a (profile the idle window with six panes and
  bring it under 1%). No SPEC change.
