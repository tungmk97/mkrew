# 0025 — Agent badges from the profile's `icon`

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user asked for an icon per agent in the workspace. Rows and pane headers showed only the
profile id. SPEC §2.1 forbids per-agent code; the UI has no SVG asset pipeline.

## Options

1. Optional `icon` key in the profile TOML (≤ 3 characters), sent on `agent.profiles`, cached by
   the UI on refresh; missing = the id's first letter.
2. UI-only letter badge from the id — no config, but `claude` and `copilot` both read `C`.
3. Brand SVG logos — needs an asset pipeline and logo licensing.

## Decision

Option 1. The agent difference stays in TOML (§2.1), glyphs match the existing Unicode icons, and
old profiles keep working through the letter fallback (seeding never overwrites them). A longer
`icon` is a profile error so rows cannot stretch. Shown in the sidebar session row and the pane
header, muted, between the status icon and the agent name; status keeps its own icon and text
(§11).
