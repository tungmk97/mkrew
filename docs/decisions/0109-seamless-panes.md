# 0109 - Seamless panes: no gap, a 1 px divider, a full-width top stripe, a stronger dim

- Date: 2026-11-24
- Status: accepted (amends the "Pane look" of ADR 0108 and SPEC section 14)
- Milestone: none (human request 2026-11-24, with a Tern screenshot as the reference)

## Context

ADR 0108 put the panes on a frame of `pane_gap`, 8 px apart and 8 px from the sidebar, the right panel and the title bar, with rounded corners.
The user compared it with Tern and asked for Tern's layout instead.
In Tern the sidebar, the panes and the right side touch: one 1 px line is the only thing between two regions.
The focused pane shows a thin line along its top edge, and the other panes are visibly dimmer.
The user asked to keep every Mkrew component and change only how the parts sit next to each other.

## Decision

- Panes fill the tab area edge to edge: no padding around the area, no inset per pane, square corners.
  `PANE_HALF_GAP` and the corner masks are deleted.
- Two panes meet at a 1 px line in `divider`, the same line that sets the sidebar and the right panel apart.
  The grab strip stays centred on it and still shows a grip only on hover.
- The top stripe (`PANE_ACCENT_WIDTH`, same colors and precedence as ADR 0108) runs the full width of the pane.
- `unfocused_pane_dim` goes from 0.05 to 0.2: at 5 % the dim could not be seen.
- The file pane loses its rounded corners; the listening ring of voice input is square like the pane.
- `pane_gap` stays: it is the title bar fill and shows behind the panes only before they paint; the "pane gap" contrast rule stays, so the bar reads apart from the panes.

## Alternatives

- **Keep the gap and make it 1 px.**
  The frame color would still show as a line, but in `pane_gap`, which in dark themes is darker than `divider`, so it would not match the sidebar and panel lines.
  Rejected.
- **A border around the focused pane.**
  The user asked for a single line at the top, like Tern.
  Rejected.

## Consequences

- The sidebar, the panes and the right panel read as one surface with 1 px lines between them.
- Each pane gains the space the frame took: 8 px on a side at the edge of the area, 4 px on a side next to another pane.
- Detached windows draw the same layout (they share `render_tab`).
