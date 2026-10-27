# Sidebar workspace cards — design

- Date: 2026-10-19
- Source: user screenshot (5 workspaces open, sidebar reads as one long list); option **A** chosen
  in brainstorming.

## Problem

With several workspaces open, the full sidebar is one continuous list:

1. A workspace row is the same height and weight as a session row (only bold text differs).
2. Nothing separates one workspace's rows from the next workspace's header.
3. The workspace color shows only on a 20 px avatar.
4. Every workspace with two kinds of rows adds `AGENTS` / `TERMINAL` / `RUN` label lines.
5. The green tint of finished-unseen rows is the loudest thing on screen (6 blocks in the
   screenshot) and outshouts the workspace headers.

## Goal

At a glance: where each workspace starts and ends, which workspace a session belongs to, which
workspace has work going on. Keep seeing several workspaces at once (no accordion, no rail).

## Design

1. **Workspace card.** Each workspace (header row + its session, terminal and RUN rows) is one
   block: `theme.card` fill, 1 px `theme.border` outline (in dark `card` is one step off
   `sidebar`, so the outline carries the shape), `radius::MEDIUM`, inner padding
   `space::XS`. Cards are `space::S` apart; the list has `space::S` padding. Session rows sit
   `space::L` in from the card edge (the old 1 px guide line goes: the card already groups them).
2. **Color stripe.** A workspace with a color gets a `WORKSPACE_STRIPE_WIDTH` (3 px) bar in that
   color on the card's left edge, inset `space::S` top and bottom, `radius::SMALL`. No color → no
   stripe (the card alone separates it). No automatic color: the user sets one in the workspace
   menu (ADR 0020 unchanged).
3. **Header weight.** The workspace name uses `font_size::CARD_TITLE` (15/13 rem), semibold.
4. **No `AGENTS` label.** Agents lead the card unlabeled (their chips say what they are). The
   `TERMINAL` label stays only while the card also shows agents or RUN rows (as today). `RUN`
   keeps its label (it carries Restart all / Stop all and the fold).
5. **Folded RUN shows its ports.** A folded RUN label shows `Summary::ports_text()`
   (`:8082 :5180`) in green monospace, so the group can stay folded and still say what runs.
6. **No tint for finished-unseen rows in the full sidebar.** The semibold title marks them
   until viewed (a `done` agent keeps its ✓ while done); they still count in `Needs you`. The
   compact rail keeps its tint (no title there to make bold).

Unchanged: the compact rail layout, the `N empty workspaces ▸` line (outside cards), filter,
chips, drag to reorder (inside a card), peek, menus, pins, footer.

## Not doing (YAGNI)

- Sticky card headers while scrolling: add if long cards scroll their header away in practice.
- Automatic workspace colors: add if anh leaves workspaces uncolored and still mixes them up.
- Accordion (only one workspace open): add as a setting if the cards are still too long.

## Docs

ADR 0062 (extends ADR 0020, ADR 0054; SPEC §10, §11, §14). HUMAN_CHECKS "To verify" entry.
