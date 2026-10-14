# 0058 — Real logos for the shipped agents

- Date: 2026-10-15
- Status: accepted
- Supersedes: option 3 of ADR 0025 (rejected there for lack of an asset pipeline)

## Context

The agent chip (ADR 0025) drew the profile's `icon` glyph: `✻` Claude, `⌬` Codex, `⊙` Copilot,
`π` omp. Each glyph came from a different fallback font, so sizes and baselines did not match,
and none looked like the real product. The user asked for the real logos, sized to look even.
ADR 0051 has since added an SVG asset pipeline (`icons.rs`, Lucide).

## Options

1. Logos in the shipped profiles' TOML (a path) — profiles copied before would never get them,
   and SVG files in the user's config dir need loading and validating at run time.
2. Logos embedded in the UI, keyed by the shipped profile id (`claude`, `codex`, `copilot`,
   `omp`) — every user gets them at once; other profiles keep the glyph/letter.
3. Brand colors — faster to tell apart, but SPEC §14 keeps color for status only.

## Decision

Option 2, one color. `icon/brand/*.svg` goes through the same `Assets` source as Lucide
(`AgentLogo` in `icons.rs`) and paints in the chip's text color, so status tints still apply.
The logo wins over the profile's `icon`, because copied profiles still carry the old sample
glyph. A logo chip has no border at rest; it keeps the border width, so the sidebar's working
ring and the pane color still outline it. The logo is `ICON_SIZE_SMALL` (14 px) in the 18 px chip.

Sources (marks are their owners' trademarks; used only to identify the integration):

|Logo|Source|
|---|---|
|Claude|Simple Icons `claude` (CC0 path data)|
|Codex|`@lobehub/icons` `codex` (MIT); viewBox padded to 26, the solid mark read larger|
|Copilot|Primer Octicons `copilot-24` (MIT)|
|omp|`can1357/oh-my-pi` `assets/icon.svg` (MIT), one color: pins cut out, dots dropped|

## Consequences

- SPEC §2.1 (no per-agent code) gets one exception: the logo lookup by id, display only.
- A user profile that reuses a shipped id gets that agent's logo, whatever its `icon`.
- Visual check (dark/light) in HUMAN_CHECKS.
