---
name: mkrew-ui-design
description: Visual design system and UI quality checklist for the Mkrew GPUI app — design tokens, spacing, radius, typography, status display, empty states, motion. Use for ANY change under crates/mkrew-ui (or the M0 spike UI), any new view, component, panel, dialog, or when the user mentions UI, design, theme, colors, layout, or "make it look good".
---

# Mkrew UI design

Full spec: `docs/SPEC.md` §14 (design system), §11 (status display), §13 (shortcuts).
Feel: simple, calm, soft, never rigid — Linear / Zed / Raycast. The terminal is the hero.

## Hard rules

1. Every color, spacing, radius, font size and duration comes from `theme.rs` tokens. No literals in components.
2. Spacing only from the scale 4 · 8 · 12 · 16 · 24 · 32. Radius 4 / 8 / 12.
3. One accent color. Status colors only for agent states, always paired with an icon + text.
4. Separate regions with spacing and subtle surface shifts; borders 1 px, faint, only if needed.
5. Secondary actions appear on hover/focus; every interactive element has hover, focus (visible ring), active, disabled states.
6. Every list/view has an empty state: one short sentence + one primary action showing its shortcut.
7. Motion 120–180 ms ease-out; respect reduce-motion.
8. Both themes (dark/light) must work; follow the OS setting.
9. Text contrast ≥ 4.5:1 for body text.
10. Tooltips show keyboard shortcuts; the command palette lists every action.

## Before committing UI work

- Capture screenshots of the changed view in dark and light into `docs/screenshots/<milestone>-<view>-<theme>.png`
  if you can do it automatically; otherwise add "check <view> visually (dark/light)" to
  `docs/HUMAN_CHECKS.md` → "To verify".
- Ask the `ui-reviewer` subagent (if available) to check the diff against this list.
- grep your diff for hex colors or raw numeric pixel values outside `theme.rs`; fix any hit.
