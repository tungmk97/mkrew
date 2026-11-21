# 0097 - Glass floating panels and Pressable as the only place for hover and active

- Date: 2026-11-21
- Status: accepted (amends SPEC section 14)
- Milestone: none (human request 2026-11-21, spec `docs/superpowers/specs/2026-11-21-polish-palette-settings-design.md`)

## Context

The palette, Settings and about twenty dialogs each built their own panel frame (`radius::LARGE`, 1 px border, `surface`, `shadow_md()`).
Clickable elements got their hover and active look from 76 hand-written `.hover(` calls against 327 click handlers, so many controls gave no feedback.
The user liked the crisp feel of Tern's palette, Settings window and hover feedback.

## Decision

- Fake glass, no backdrop blur (gpui-pre 0.3.7 cannot blur behind one element): a translucent `glass` fill, a 1 px `glass_border`, a `glass_tint` gradient over the top `GLASS_TINT_HEIGHT` px, `shadow_float` (two layers) and a lighter `scrim`.
- The three glass colors are derived from each theme's `surface` and `accent` in one function, `Theme::with_glass`, so every built-in theme gets them; the function raises the fill alpha until body text keeps 4.5:1 over a light or a dark page behind the panel.
- `widgets::floating_panel` is the only panel frame; no dialog builds its own.
  Its entrance (fade and a 4 px rise over `motion::FAST`) is `widgets::appear`, applied by `modal_shade` and the confirm prompt.
  gpui has no element scale, and under reduced motion gpui draws the end state, so the panel then appears at once.
- Reduced motion is read from Windows (`SPI_GETCLIENTAREAANIMATION`) at start and on window activation and stored with `App::set_reduce_motion`.
- `mkrew_ui::pressable` (`Look`, `PressStyle`, `press_style`, `Pressable`) is the only place that calls `.hover(` and `.active(` for a clickable element; `widgets.rs` and the lib code both use it.
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
