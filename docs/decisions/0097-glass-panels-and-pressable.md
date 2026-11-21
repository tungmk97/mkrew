# 0097 - Glass floating panels, and hover and active only in widgets.rs and pressable.rs

- Date: 2026-11-21
- Status: accepted (amends SPEC section 14)
- Milestone: none (human request 2026-11-21, spec `docs/superpowers/specs/2026-11-21-polish-palette-settings-design.md`)

## Context

The palette, Settings and about twenty dialogs each built their own panel frame (`radius::LARGE`, 1 px border, `surface`, `shadow_md()`).
Clickable elements got their hover and active look from 76 hand-written `.hover(` calls against 327 click handlers, so many controls gave no feedback.
The user liked the crisp feel of Tern's palette, Settings window and hover feedback.

## Decision

- Fake glass, no backdrop blur (gpui-pre 0.3.7 cannot blur behind one element): a translucent `glass` fill (surface at alpha 0.96 in dark themes, 0.97 in light ones; first drafted at 0.88 / 0.92, raised after the first smoke run because without blur the lower alpha showed sharp ghost text of the UI behind), a 1 px `glass_border`, a `glass_tint` gradient over the top `GLASS_TINT_HEIGHT` px, `shadow_float` (two layers) and a lighter `scrim`.
- The three glass colors are derived from each theme's `surface` and `accent` in one function, `Theme::with_glass`, so every built-in theme gets them; the function raises the fill alpha until body text keeps 4.5:1 over a light or a dark page behind the panel.
- `widgets::floating_panel` is the only panel frame; no dialog builds its own.
  Its entrance (fade and a 4 px rise over `motion::FAST`) is `widgets::appear`, applied by `modal_shade` and the confirm prompt.
  gpui has no element scale, and under reduced motion gpui draws the end state, so the panel then appears at once.
- Reduced motion is read from Windows (`SPI_GETCLIENTAREAANIMATION`) at start and on window activation and stored with `App::set_reduce_motion`.
- `mkrew_ui::pressable` (`Look`, `PressStyle`, `press_style`, `Pressable`) and `shell/widgets.rs` are the only files that call `.hover(` and `.active(` or set `.cursor_pointer()` for a clickable element; `group_hover` is allowed anywhere.
  Every other element is a widget or takes its look from `.pressable(look, enabled, theme)`.
  A disabled element is faded with `opacity::DISABLED`, has the default cursor and no hover, no active and no click.
- Settings: groups are a UI concept (`Setting::group`), independent of the TOML `section`.

## Alternatives

- Real blur or a Mica window: not possible per element in gpui 0.3.7.
- A `with_animation` inside `floating_panel`: it changes the return type, so every caller chain breaks.
- Keeping per-widget `.hover(` calls: the reason many handlers had no feedback.

## Consequences

- Every theme added later gets glass for free and is checked by the contrast test.
- A new clickable element uses a widget or `.pressable(..)`; Plan P3 of this work enforces it across `crates/mkrew-ui/src`.
- macOS and Linux do not detect reduced motion until gpui exposes it.

## Audit (Plan P3, 2026-11-21)

At 1b7abb5, before P1, `crates/mkrew-ui/src` had 327 click or mouse-down handlers in 48 files and 76 `.hover(` calls.
Of those handlers, 208 were already on a shared widget, 71 were hand-built and moved to a `Look` or a helper, and 48 are exempt.
At the start of P3 (after P1 and P2) the counts were 334 handlers (including 10 `.on_press(`) in 49 files and 66 `.hover(` calls.
After the audit only `shell/widgets.rs` and `pressable.rs` call `.hover(` and `.active(` or set `.cursor_pointer()`.

The audit added around the P1 contract:

- `Pressable::pressable_unless` for a selected element that stays clickable.
- `row_action` (moved into `widgets.rs`), `grip_hover`, `pick_surface`, `card_press` and `disabled_icon_button`.

`Look::Row`, `Look::Chip` and `Look::Icon` hover with `theme.hover` and `Look::Raised` with `theme.border` (P1 Task 4, with tests over every built-in theme), because `surface_raised` equals `selected` in Mkrew Dark and Mkrew Light.
An icon inside a row that hovers `theme.hover` uses `Look::Raised`, so its own hover shows.

Exempt, with no hover: resize grips (they tint through `grip_hover`), terminal surfaces, drop zones and drag sources, the modal scrim, text fields (they keep their I-beam), editor and text-selection surfaces, window-drag areas, the pane title (it renames on double-click only and shows no pointer), links drawn by `InteractiveText`, and the context-menu mouse down of an element that is already pressable.
