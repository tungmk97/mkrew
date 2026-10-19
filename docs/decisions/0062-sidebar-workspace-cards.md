# 0062 — Sidebar workspaces as cards

- Date: 2026-10-19
- Status: accepted (extends ADR 0020, ADR 0054; SPEC §10, §11, §14)
- Milestone: none (user request 2026-10-19)

## Context

With five workspaces open the full sidebar read as one long list: a workspace row had the
height and weight of a session row, nothing separated one workspace's rows from the next
header, the workspace color showed only on a 20 px avatar, every mixed workspace added
`AGENTS` / `TERMINAL` / `RUN` label lines, and the green tint of finished-unseen rows was the
loudest thing on screen.

## Decision

- Each workspace in the full sidebar is one card: `card` fill, 1 px `border` border, radius 8,
  4 px padding, 8 px apart. Its rows sit 16 px in; the 1 px guide line is gone.
- A colored workspace has a 3 px stripe of its color on the card's left edge
  (`WORKSPACE_STRIPE_WIDTH`). No automatic color for uncolored workspaces.
- The workspace name is `CARD_TITLE` size, semibold.
- No `AGENTS` label; `TERMINAL` is labeled only beside agents or a RUN group; `RUN` keeps its
  label and, while folded, shows its ports (`:8082 :5180`).
- Finished-unseen rows in the full sidebar lose the green fill; the semibold title says
  "not yet viewed" (a `done` agent's ✓ stays while it is done). The compact rail keeps the
  fill: it has no title to make bold.
- The compact rail layout and the `N empty workspaces` line are unchanged.

## Consequences

- Workspaces are told apart by shape, gap and color stripe without reading names.
- Cards cost about 16 px of height per workspace (padding + gap); each mixed workspace saves
  the `AGENTS` line.
- Not done: sticky headers, automatic colors, one-open-at-a-time accordion; each can be added
  if the cards are not enough.
