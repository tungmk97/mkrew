# Polish P1 - Foundation (glass panels, Pressable, Settings) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every floating panel (palette, Settings, every modal dialog, the confirm prompt) shares one translucent glass frame; every clickable shared widget gets hover, press and disabled looks from one `Pressable` rule; Settings becomes a larger panel with grouped cards, a global search, shared controls and a footer.

**Architecture:** New theme tokens (`glass`, `glass_border`, `glass_tint`, `scrim`, `shadow_float`) are derived in one place, `Theme::with_glass`, so the two Mkrew themes and every palette theme get them.
`Look`, `PressStyle`, `press_style`, `apply_press` and `Pressable` live in the lib crate (`crates/mkrew-ui/src/pressable.rs`) so lib code such as `preview.rs` can use them; `shell/widgets.rs` (bin crate) re-exports them and adds `floating_panel`, `appear`, `toggle`, `dropdown_trigger`, `list_row`.
The Settings model (`settings_dialog.rs`, lib) gets the navigation groups, the global search and its Esc rule as pure code with unit tests; `shell/settings.rs` only draws it.

**Tech Stack:** Rust, gpui-pre 0.3.7 (`linear_gradient`, `linear_color_stop`, `BoxShadow`, `AnimationExt::with_animation`, `ease_out_quint`, `App::reduce_motion`), `windows` 0.62 (`SystemParametersInfoW`), insta snapshots, `palette::fuzzy`.

**Spec:** `docs/superpowers/specs/2026-11-21-polish-palette-settings-design.md` (P1 row of section 9: sections 4, 5.1, 5.2, 6, and the P1 part of section 12).

## Global Constraints

- Work in a worktree `.worktrees/polish` on branch `feat/polish` (superpowers:using-git-worktrees), shared with Plans P2 and P3; commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, only the files of the task, never push.
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens.
  Spacing only 4 / 8 / 12 / 16 / 24 / 32.
  No hex colors or raw px numbers outside `theme.rs`.
  Dark and light themes both work.
- No `unwrap` / `expect` outside tests unless provably infallible (say why in a comment).
  `thiserror` in libraries, `anyhow` in binaries.
- Tests never call a real agent or the network; git tests use temporary repositories (`fn git(dir, &[..])` helpers).
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests).
  Fast loop: `cargo test -p mkrew-ui <filter>` (lib tests) and `cargo test -p mkrew-ui --bin mkrew <filter>` (tests of `shell/*`).
  `cargo test -p mkrew-core <filter>` for the settings table.
- ADR number 0097 (`docs/decisions/0097-glass-panels-and-pressable.md`); `PROTOCOL_VERSION` 54 belongs to Plan P2: P1 changes no protocol.
- Never the em dash character in code, docs or UI text; use `-`.
  In long Markdown put each sentence on its own line.
- Windows native is first-class: every `windows` call needs a `// SAFETY:` comment; non-Windows builds get a stub.
- gpui names in this plan were read from gpui-pre 0.3.7 on 2026-11-21: `color.rs:865 linear_gradient(angle, from, to)` (angle 0 = to the top, 180 = to the bottom), `color.rs:893 linear_color_stop(color, percentage 0..=1)`, `style.rs:349 BoxShadow` with `BoxShadow::new(x, y, color).blur_radius(..)`, `Styled::shadow(Vec<BoxShadow>)`, `elements/animation.rs:84 AnimationExt::with_animation(id, Animation, |el, delta| ..)`, `animation.rs:60 Animation::with_easing`, `animation.rs:526 ease_out_quint()`, `app.rs:1141 App::reduce_motion()` and `App::set_reduce_motion(bool)`.
  `with_animation` renders the END state with no frames when `reduce_motion()` is true (`animation.rs:407`).
  `FluentBuilder` (`when`, `when_some`) is implemented for `IntoElement` types only, not for `StyleRefinement`: inside `.hover(|style| ..)` use plain `if let`.
  A second `.hover(..)` on one element replaces the first.
  `voice_input.rs:247` is the existing `with_animation` example (`Animation::new(theme::motion::SLOW).with_easing(ease_out_quint())`).

## Spec notes

The spec was amended after the first draft of this plan (no element scale, `appear` in `modal_shade` and the confirm prompt, `settings_fields.rs` and `NAV`, `Look::Raised`, `Look` and `Pressable` in the lib crate, accent plus semibold matches, the Prompts footer hint).
The plan follows the amended spec.
Two choices the spec leaves open:

1. **Hover fills:** `Row`, `Chip` and `Icon` hover to `theme.hover` (never `surface_raised`, which equals `theme.selected` in Mkrew Dark and Light), `Raised` (menus, toasts, cards, the find bar) to `theme.border`.
   A selected element (`list_row(selected = true)`, the active Settings page) has no hover fill: it keeps the `selected` fill.
2. **Buttons keep signatures:** `quiet_button(.., hover: u32)` keeps its `hover` argument and `button_frame` stays hover-free, so widgets use `apply_press` with an overridden `hover_bg`; a filled control (primary button, switch) dims on hover instead of changing fill (`filled_press` in `widgets.rs`).

## Dialog panels moved to `floating_panel` (found with `modal_shade` and `.shadow_md()` + `radius::LARGE`)

Every row is one panel builder.
"Line" is the line of the `.shadow_md()` call today.

| File | Shade id / panel id | Line | Width token |
| --- | --- | --- | --- |
| `shell/ask_dialog.rs` | `ask-shade` / `ask-dialog` | 309 | `ASK_DIALOG_WIDTH` |
| `shell/composer.rs` | `prompt-picker-shade` / `prompt-picker` | 653 | `DIALOG_WIDTH` |
| `shell/diagram_dialog.rs` | `diagram-dialog-shade` / `diagram-dialog` | 50 | `HISTORY_DIALOG_FRACTION` (relative) |
| `shell/dialogs.rs` | `undo-dialog-shade` | 389 | `UNDO_DIALOG_WIDTH` |
| `shell/dialogs.rs` | `broadcast-shade` | 607 | `DIALOG_WIDTH` |
| `shell/git_dialogs.rs` | `git-discard-shade` | 334 | `UNDO_DIALOG_WIDTH` |
| `shell/git_dialogs.rs` | `git-base-shade` | 426 | `DIALOG_WIDTH` |
| `shell/history.rs` | `turns-dialog-shade` | 651 | `TURNS_DIALOG_WIDTH` |
| `shell/history.rs` | `history-shade` | 844 | `HISTORY_DIALOG_FRACTION` (relative) |
| `shell/navigation.rs` | `quick-open-shade` / `quick-open` | 1043 | `DIALOG_WIDTH` (Plan P2 deletes this builder) |
| `shell/navigation.rs` | `palette-shade` / `palette` | 1210 | `DIALOG_WIDTH` (Plan P2 widens it to `PALETTE_WIDTH`) |
| `shell/path_field.rs` | `plan-path-shade` | 146 | `DIALOG_WIDTH` |
| `shell/phone_access.rs` | `phone-access-shade` | 396 | `DIALOG_WIDTH` |
| `shell/ports.rs` | `ports-shade` | 325 | `PORTS_DIALOG_WIDTH` |
| `shell/run.rs` | `run-setup-shade` / `run-setup` | 977 | `RUN_SETUP_WIDTH` (+ `max_h`) |
| `shell/run.rs` | `run-setup-shade` / `run-picker` | 1200 | `RUN_SETUP_WIDTH` |
| `shell/send_dialog.rs` | `send-dialog-shade` and `changes-dialog-shade`, both through `doc_dialog_panel` | 287 | `width` argument |
| `shell/settings.rs` | `settings-shade` / `settings-dialog` | 352 | `SETTINGS_DIALOG_WIDTH` (rebuilt in Task 10) |
| `shell/slash.rs` | `slash-picker-shade` | 435 | `DIALOG_WIDTH` |
| `shell/task_dialog.rs` | `task-dialog-shade` | 429 | `DIALOG_WIDTH` |
| `shell/workspace_actions.rs` | `add-workspace-shade` | 1021 | `DIALOG_WIDTH` |
| `shell/worktrees.rs` | `worktree-dialog-shade` | 648 | `UNDO_DIALOG_WIDTH` |
| `shell/worktrees.rs` | `compare-dialog-shade` | 814 | `COMPARE_DIALOG_WIDTH` |
| `shell/confirm_prompt.rs` | `confirm-shade` / `confirm-dialog` (own scrim, no `modal_shade`) | 141 | `CONFIRM_DIALOG_WIDTH` |

Not moved, on purpose (popovers, not modal dialogs, `radius::MEDIUM`): `menu_panel` (`widgets.rs:741`), `peek.rs:580`, `reorder.rs:328` (drag chip), `voice_input.rs:245` (pill), `terminal/view.rs:869` (find bar), `cards.rs:888` (drag card), `widgets.rs:1070` (tooltip).

## Review Focus

Inputs and conditions the spec implies but no obvious test covers, most likely first.
Each line has its test in the task named after it.

1. A Settings query of only spaces behaves as an empty query (the page rows stay, no empty state).
   Test in Task 9: `a_blank_query_shows_the_page_rows`.
2. Esc inside a search result with a changed draft: the first Esc restores the draft, the second clears the query, the third closes.
   Test in Task 9: `esc_restores_a_draft_then_clears_the_search_then_closes`.
3. A query that matches nothing, or only hidden settings (`sidebar_width`, `md_split` ...), never lists a hidden row and an unmatched query gives no rows (the empty state).
   Test in Task 9: `hidden_settings_are_never_listed_and_no_match_is_empty`.
4. Long help lines match almost any short query: a label match must always rank above a help-only match.
   Test in Task 9: `a_label_match_ranks_above_a_help_only_match`, over several queries.
5. A theme whose `text` on `surface` is only just readable must not fall under 4.5:1 once the panel is translucent.
   Test in Task 2: `text_on_glass_reads_at_4_5_over_a_light_or_dark_page` (and `with_glass` raises the alpha until it holds).
6. A failed read of the OS animation setting must not disable motion for good.
   Test in Task 3: `a_failed_read_keeps_motion_on`.
7. A read-only Settings (broken `settings.toml`) keeps every control inert: toggle, segments and dropdown triggers take no press and show no hover or pointer.
   Covered by `on_press(false, ..)` attaching nothing, `segment_in(.., false, ..)` and `a_disabled_press_style_has_no_pointer_hover_or_active` (Task 4), and the manual check in Task 12.
8. A dialog taller or wider than the window: Settings caps its width and height at the viewport minus `space::XXL * 2`.
   Manual check in Task 12 (resize the window to its minimum).

## File Structure

- Create `docs/decisions/0097-glass-panels-and-pressable.md`.
- Create `crates/mkrew-ui/src/pressable.rs` (lib): `Look`, `PressStyle`, `press_style`, `apply_press`, `Pressable`; unit tests.
- Modify `crates/mkrew-ui/src/lib.rs`: `pub mod pressable;`.
- Modify `crates/mkrew-ui/src/theme.rs`: fields, `with_alpha`, `glass_over`, `with_glass`, `glass_gradient`, `shadow_float`, constants (`GLASS_TINT_ANGLE`, `opacity::DIMMED`), tests.
- Modify `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_{dark,light}.snap` (accepted with insta).
- Modify `crates/mkrew-ui/src/desktop.rs`, `desktop/win32.rs`, `desktop/none.rs`, `shell/connection.rs`, `main.rs`: reduced-motion detection.
- Modify `crates/mkrew-ui/src/shell/widgets.rs`: re-exports, rebuilt widgets, `segment_in`, `floating_panel`, `appear`, `toggle`, `dropdown_trigger`, `list_row`.
- Modify `crates/mkrew-ui/src/shell/files_render.rs:79` and `shell/pane_render.rs:804` (they keep the pointer once `button_frame` stops setting it), `shell/focus.rs` (`modal_shade`), `shell/confirm_prompt.rs`, and the dialog files of the table.
- Modify `crates/mkrew-core/src/settings_fields.rs`: `group` field, row order, tests.
- Modify `crates/mkrew-ui/src/settings_dialog.rs`: `NAV`, `Focus::Find`, `find`, `search`, `Hit`, `grouped`, key handling, tests.
- Modify `crates/mkrew-ui/src/shell/settings.rs`: frame, nav, search box, results, cards, controls, footer.
- Modify `docs/SPEC.md` (section 14 and the Settings bullet), `docs/HUMAN_CHECKS.md`.

---

### Task 1: ADR 0097

**Files:**
- Create: `docs/decisions/0097-glass-panels-and-pressable.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the ADR number every later task cites.

- [ ] **Step 1: Recheck the number is free**

Run: `git ls-tree -r --name-only HEAD docs/decisions | grep 0097; git branch -a --format='%(refname)' | while read b; do git ls-tree -r --name-only "$b" docs/decisions 2>/dev/null | grep 0097; done`
Expected: no output.
If a file named `0097*` exists on any branch, stop and tell the parent: the spec and all three plans name 0097.

- [ ] **Step 2: Write the ADR**

```markdown
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
```

- [ ] **Step 3: Commit**

```bash
git add docs/decisions/0097-glass-panels-and-pressable.md
git -c user.email=tungnv975@gmail.com commit -m "docs(adr): 0097 glass floating panels and Pressable"
```

---

### Task 2: Glass tokens, `shadow_float`, sizes and the contrast test

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct at ~720, `shared()` at ~1241, `dark()` closing at ~966, `light()` closing at ~1060, `from_palette` at ~1464, constants at ~305 and ~439, tests at ~1850)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`, `..._light.snap`

**Interfaces:**
- Consumes: `theme::mix`, `theme::contrast`, `Theme::{surface, accent, bg, text, dark}`.
- Produces: `Theme.glass`, `Theme.glass_border`, `Theme.glass_tint` (RGBA `u32`), changed `Theme.scrim`; `pub fn with_alpha(color: u32, alpha: f32) -> u32`; `pub fn glass_over(surface: u32, accent: u32, alpha: f32, tint: f32, backdrop: u32) -> u32`; `pub fn glass_gradient(theme: &Theme) -> (u32, u32)`; `pub fn shadow_float(theme: &Theme) -> Vec<gpui::BoxShadow>`; `opacity::DIMMED`; constants `GLASS_TINT_HEIGHT`, `GLASS_TINT_ANGLE`, `GLASS_TINT_FROM`, `GLASS_TINT_TO`, `PANEL_RISE`, `PALETTE_WIDTH`, `PALETTE_LIST_HEIGHT`, `PALETTE_SELECTED_BAR`, `PALETTE_GROUP_LIMIT`, changed `SETTINGS_DIALOG_WIDTH`, `SETTINGS_DIALOG_HEIGHT`, `SETTINGS_NAV_WIDTH` (the sizes are constants, so they have no test of their own).

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` of `theme.rs` (it already has `all_themes()`):

```rust
    #[test]
    fn glass_is_derived_from_surface_and_accent_in_every_theme() {
        for theme in all_themes() {
            assert_eq!(theme.glass >> 8, theme.surface, "{}", theme.name);
            assert_eq!(theme.glass_tint >> 8, theme.accent, "{}", theme.name);
            let edge = if theme.dark { 0xFFFFFF } else { 0x000000 };
            assert_eq!(theme.glass_border, with_alpha(edge, 0.08), "{}", theme.name);
            // The base alphas of the spec; the contrast rule may only raise the fill.
            let floor = if theme.dark { 0.88_f32 } else { 0.92_f32 };
            assert!(
                (theme.glass & 0xFF) >= (floor * 255.0).round() as u32,
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn the_scrim_is_lighter_than_before_and_lighter_in_light_themes() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_eq!(dark.scrim, with_alpha(0x000000, 0.24));
        assert_eq!(light.scrim, with_alpha(0x000000, 0.16));
        assert!(dark.scrim & 0xFF < 0x59 && light.scrim & 0xFF < dark.scrim & 0xFF);
    }

    #[test]
    fn text_on_glass_reads_at_4_5_over_a_light_or_dark_page() {
        for theme in all_themes() {
            // Decoded exactly as `with_glass` quantized them.
            let alpha = (theme.glass & 0xFF) as f32 / 255.0;
            let tint = (theme.glass_tint & 0xFF) as f32 / 255.0;
            for backdrop in [theme.bg, theme.text] {
                let fill = glass_over(theme.surface, theme.accent, alpha, tint, backdrop);
                let ratio = contrast(theme.text, fill);
                assert!(
                    ratio >= 4.5,
                    "{}: {ratio:.2}:1 over {backdrop:06X}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn the_tint_gradient_fades_to_the_same_color_with_no_alpha() {
        for theme in all_themes() {
            let (top, bottom) = glass_gradient(&theme);
            assert_eq!(top, theme.glass_tint);
            assert_eq!(bottom & 0xFF, 0);
            assert_eq!(bottom >> 8, top >> 8);
        }
    }

    #[test]
    fn the_float_shadow_has_two_layers_and_half_the_alpha_in_light_themes() {
        let (dark, light) = (shadow_float(&Theme::dark()), shadow_float(&Theme::light()));
        assert_eq!(dark.len(), 2);
        assert_eq!(light.len(), 2);
        assert_eq!(dark[0].offset.y, gpui::px(8.0));
        assert_eq!(dark[0].blur_radius, gpui::px(24.0));
        assert_eq!(dark[1].offset.y, gpui::px(2.0));
        assert_eq!(dark[1].blur_radius, gpui::px(6.0));
        for (d, l) in dark.iter().zip(&light) {
            assert!((d.color.a / 2.0 - l.color.a).abs() < 0.01);
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mkrew-ui theme::tests::glass`
Expected: FAIL to compile (`with_alpha`, `glass_over`, fields not defined).

- [ ] **Step 3: Add the fields, helpers, `with_glass` and constants**

In the `Theme` struct replace the `scrim` field (line ~720-721) with:

```rust
    /// The shade behind a modal dialog, RGBA: black at 24 % (dark) or 16 % (light), set by
    /// [`Theme::with_glass`].
    pub scrim: u32,
    /// Fill of every floating panel, RGBA: `surface` at 0.88 (dark) / 0.92 (light) or more, as
    /// much as body text needs to keep 4.5:1 over a light or dark page behind it.
    pub glass: u32,
    /// 1 px inner border of a floating panel, RGBA: white (dark) or black (light) at 8 %.
    pub glass_border: u32,
    /// Top of the vertical gradient over the first `GLASS_TINT_HEIGHT` px of a floating panel,
    /// RGBA: `accent` at 5 % (dark) / 4 % (light).
    pub glass_tint: u32,
```

In `shared()` replace the lines `// Black at ~35 %, in both appearances.` and `scrim: 0x0000_0059,` with:

```rust
            // Glass and scrim are derived from `surface` / `accent` by `with_glass`.
            scrim: 0,
            glass: 0,
            glass_border: 0,
            glass_tint: 0,
```

Next to `mix` (line ~1340) add the free functions:

```rust
/// `0xRRGGBB` as `0xRRGGBBAA` at `alpha` (0.0 to 1.0).
pub fn with_alpha(color: u32, alpha: f32) -> u32 {
    (color << 8) | (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// `alpha` as the 8-bit value an RGBA color stores (so tests and `with_glass` agree).
fn quantize(alpha: f32) -> f32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

/// The color a floating panel shows at its top over `backdrop`: `surface` at `alpha`, then the
/// accent tint at `tint`.
pub fn glass_over(surface: u32, accent: u32, alpha: f32, tint: f32, backdrop: u32) -> u32 {
    mix(mix(backdrop, surface, alpha), accent, tint)
}

/// The tint gradient of a floating panel: its top color and the same color with no alpha.
pub fn glass_gradient(theme: &Theme) -> (u32, u32) {
    (theme.glass_tint, theme.glass_tint & 0xFFFF_FF00)
}

/// The shadow of a floating panel (palette, Settings, dialogs): two layers, half as strong in
/// light themes.
pub fn shadow_float(theme: &Theme) -> Vec<gpui::BoxShadow> {
    let strength = if theme.dark { 1.0 } else { 0.5 };
    let black = |alpha: f32| gpui::Hsla::from(gpui::rgba(with_alpha(0x000000, alpha * strength)));
    vec![
        gpui::BoxShadow::new(gpui::px(0.0), gpui::px(8.0), black(0.35))
            .blur_radius(gpui::px(24.0)),
        gpui::BoxShadow::new(gpui::px(0.0), gpui::px(2.0), black(0.20))
            .blur_radius(gpui::px(6.0)),
    ]
}
```

In the `impl Theme` block that holds `from_palette` (line ~1365) add:

```rust
    /// Sets `scrim`, `glass`, `glass_border` and `glass_tint` from `surface` and `accent`.
    /// The fill alpha starts at the spec value and grows by 2 % until `text` keeps 4.5:1 on the
    /// panel over both `bg` and `text` behind it (the worst light and dark pages).
    fn with_glass(self) -> Self {
        let (base, edge, tint, scrim) = if self.dark {
            (0.88, 0xFFFFFF, 0.05, 0.24)
        } else {
            (0.92, 0x000000, 0.04, 0.16)
        };
        let tint = quantize(tint);
        let worst = |alpha: f32| {
            [self.bg, self.text]
                .into_iter()
                .map(|backdrop| {
                    let fill = glass_over(self.surface, self.accent, alpha, tint, backdrop);
                    contrast(self.text, fill)
                })
                .fold(f32::MAX, f32::min)
        };
        let mut alpha = quantize(base);
        // Ends: at alpha 1.0 the fill is `surface` plus the tint, which text reads on.
        while worst(alpha) < 4.5 && alpha < 1.0 {
            alpha = quantize(alpha + 0.02);
        }
        Self {
            scrim: with_alpha(0x000000, scrim),
            glass: with_alpha(self.surface, alpha),
            glass_border: with_alpha(edge, 0.08),
            glass_tint: with_alpha(self.accent, tint),
            ..self
        }
    }
```

Apply it in three places:

1. `dark()` (closing of `let theme = Self { .. ..Self::shared() };` at ~966): write the end as `}.with_glass();` (rustfmt formats it).
2. `light()` (closing of its `let theme = Self { .. };` at ~1060): same.
3. `from_palette` (the trailing `Self { .., ..base }` at ~1464): `Self { .., ..base }.with_glass()`.

Constants: replace the three Settings sizes at ~305-307 with

```rust
pub const SETTINGS_DIALOG_WIDTH: f32 = 880.0;
pub const SETTINGS_DIALOG_HEIGHT: f32 = 640.0;
pub const SETTINGS_NAV_WIDTH: f32 = 200.0;
```

and next to `QUICK_OPEN_LIST_HEIGHT` (~439; it stays until Plan P2 removes it) add:

```rust
/// Floating panels (ADR 0097): how far the accent tint gradient reaches down from the top,
/// its direction (180 = top to bottom in gpui) and color stops, and how far a panel rises
/// while it fades in (gpui has no element scale).
pub const GLASS_TINT_HEIGHT: f32 = 120.0;
pub const GLASS_TINT_ANGLE: f32 = 180.0;
pub const GLASS_TINT_FROM: f32 = 0.0;
pub const GLASS_TINT_TO: f32 = 1.0;
pub const PANEL_RISE: f32 = space::XS;
/// The scoped palette (Plan P2 uses these; Plan P1 only defines them).
pub const PALETTE_WIDTH: f32 = 640.0;
pub const PALETTE_LIST_HEIGHT: f32 = 480.0;
pub const PALETTE_SELECTED_BAR: f32 = 3.0;
pub const PALETTE_GROUP_LIMIT: usize = 5;
```

In `pub mod opacity` of `theme.rs` (next to `VIEWED`) add:

```rust
    /// A non-matching item that stays readable and clickable (Settings pages without a search
    /// match).
    pub const DIMMED: f32 = 0.6;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui theme::tests`
Expected: the six new tests PASS; `the_mkrew_themes_never_change` FAILS (snapshot differs: new fields and the new scrim).

- [ ] **Step 5: Accept the snapshots after reading the diff**

Run: `INSTA_UPDATE=always cargo test -p mkrew-ui theme::tests::the_mkrew_themes_never_change`
Then: `git diff crates/mkrew-ui/src/snapshots/`
Expected diff: only `scrim:` changed (89 to 61 dark, 89 to 41 light) and the three `glass*` lines added, in the top theme and in the nested `classic` theme.
If any other line changed, stop: a color moved by accident.

Run: `cargo test -p mkrew-ui theme::tests`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): glass tokens, shadow_float and polish sizes"
```

---

### Task 3: Reduced-motion detection (Windows)

**Files:**
- Modify: `crates/mkrew-ui/src/desktop/win32.rs` (imports at 20-24, function above `pub struct Os` at ~575, test in `mod tests` at ~737)
- Modify: `crates/mkrew-ui/src/desktop/none.rs`
- Modify: `crates/mkrew-ui/src/desktop.rs:18-19`
- Modify: `crates/mkrew-ui/src/shell/connection.rs:70-77`
- Modify: `crates/mkrew-ui/src/main.rs:221-225`

**Interfaces:**
- Consumes: `windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS}` (the `Win32_UI_WindowsAndMessaging` feature is already enabled in `crates/mkrew-ui/Cargo.toml`; `SPI_GETCLIENTAREAANIMATION` exists in windows 0.62.2), `gpui::App::set_reduce_motion`.
- Produces: `mkrew_ui::desktop::reduce_motion() -> bool`.

- [ ] **Step 1: Write the failing test**

In `win32.rs` `mod tests`:

```rust
    #[test]
    fn a_failed_read_keeps_motion_on() {
        assert!(!reduce_from(false, false), "the call failed: animate");
        assert!(!reduce_from(true, true), "animations are on: animate");
        assert!(reduce_from(true, false), "animations are off: reduce");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mkrew-ui desktop::win32::tests::a_failed_read_keeps_motion_on`
Expected: FAIL to compile (`reduce_from` not defined).

- [ ] **Step 3: Implement**

In the `use windows::Win32::UI::WindowsAndMessaging::{ .. }` list at the top of `win32.rs` (the first one, starting with `CreateIconIndirect`) add `SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW`.
Add above `pub struct Os`:

```rust
/// Whether the OS asks for reduced motion: the call worked and "Animation effects" is off
/// (Windows Settings, Accessibility, Visual effects). A failed call keeps motion on.
fn reduce_from(read_ok: bool, animations_on: bool) -> bool {
    read_ok && !animations_on
}

/// Windows' "Animation effects" setting, as `SPI_GETCLIENTAREAANIMATION` reports it.
pub fn reduce_motion() -> bool {
    let mut animations_on = windows::core::BOOL(1);
    // SAFETY: `SPI_GETCLIENTAREAANIMATION` writes one BOOL through `pvparam`; it points at
    // `animations_on`, which outlives the call, and `uiparam` is 0 as the action requires.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut animations_on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    reduce_from(read.is_ok(), animations_on.as_bool())
}
```

In `none.rs` add:

```rust
/// gpui-pre 0.3.7 exposes no reduced-motion setting here: motion stays on.
pub fn reduce_motion() -> bool {
    false
}
```

In `desktop.rs` after `pub use os::set_window_shown;` add `pub use os::reduce_motion;`.

In `main.rs` inside `.run(move |cx: &mut App| {`, right after `cx.set_global(MenuAnchors::default());` add:

```rust
            // gpui draws animations in their end state while this is set (ADR 0097).
            cx.set_reduce_motion(mkrew_ui::desktop::reduce_motion());
```

In `shell/connection.rs` inside the `observe_window_activation` closure, right after `if window.is_window_active() {` add (the user may change the OS setting while Mkrew runs):

```rust
                cx.set_reduce_motion(mkrew_ui::desktop::reduce_motion());
```

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-ui desktop::`
Expected: PASS.
Run: `cargo check -p mkrew-ui`
Expected: no errors (if `BOOL` is not at `windows::core::BOOL`, use `windows::Win32::Foundation::BOOL`; windows 0.62 moved it to `windows::core`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/desktop.rs crates/mkrew-ui/src/desktop/win32.rs crates/mkrew-ui/src/desktop/none.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): read the OS animation setting into reduce_motion"
```

---

### Task 4: `pressable.rs` (`Look`, `PressStyle`, `press_style`, `Pressable`)

**Files:**
- Create: `crates/mkrew-ui/src/pressable.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod pressable;` in sorted place)

**Interfaces:**
- Consumes: `crate::theme::{Theme, opacity}`.
- Produces (lib, `pub`; re-exported `pub(crate)` by `widgets.rs` in Task 5): `enum Look { Row, Chip, Icon, Raised, Text }`, `struct PressStyle`, `fn press_style(Look, bool, &Theme) -> PressStyle`, `fn apply_press(Stateful<Div>, PressStyle) -> Stateful<Div>`, `trait Pressable { fn pressable(self, Look, bool, &Theme) -> Self; fn on_press(self, bool, impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self }`.
  Plan P3 may add more helpers here: the `match` in `press_style` has no wildcard arm, and `PressStyle` has no field beyond the spec.
  `pressable(.., enabled = true, ..)` sets no base opacity.
  `on_press(false, ..)` attaches nothing and does not need `pressable` to have run.

- [ ] **Step 1: Write the file with its tests**

Create `crates/mkrew-ui/src/pressable.rs`:

```rust
//! One hover / press / disabled rule for every clickable element (ADR 0097). `press_style` is
//! pure, so it is tested without a window; `Pressable` applies it to a gpui element.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, Stateful, Window, rgb};

use crate::theme::{Theme, opacity};

/// How a pressable element looks at rest, hovered and pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A list or menu row: hover fill `theme.hover`, no border change.
    Row,
    /// A chip or pill: hover fill `theme.hover`, label to full text.
    Chip,
    /// An icon-only control: hover fill `theme.hover`, icon to full text color.
    Icon,
    /// An element that sits on `surface_raised` (menu rows, toasts, cards, the find bar): hover
    /// fill `theme.border`, one step above its rest fill.
    Raised,
    /// A text link: no fill, hover underline and full text color.
    Text,
}

/// The resolved style of a pressable element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStyle {
    pub hover_bg: Option<u32>,
    pub hover_fg: Option<u32>,
    pub underline_on_hover: bool,
    /// Opacity while pressed; 1.0 = unchanged (a disabled element does not react).
    pub active_opacity: f32,
    /// Opacity at rest: 1.0 enabled, `opacity::DISABLED` disabled.
    pub opacity: f32,
    /// The pointing-hand cursor (enabled elements only).
    pub pointer: bool,
}

pub fn press_style(look: Look, enabled: bool, theme: &Theme) -> PressStyle {
    if !enabled {
        return PressStyle {
            hover_bg: None,
            hover_fg: None,
            underline_on_hover: false,
            active_opacity: 1.0,
            opacity: opacity::DISABLED,
            pointer: false,
        };
    }
    let (hover_bg, hover_fg, underline_on_hover) = match look {
        Look::Row => (Some(theme.hover), None, false),
        Look::Chip | Look::Icon => (Some(theme.hover), Some(theme.text), false),
        Look::Raised => (Some(theme.border), None, false),
        Look::Text => (None, Some(theme.text), true),
    };
    PressStyle {
        hover_bg,
        hover_fg,
        underline_on_hover,
        active_opacity: opacity::ACTIVE,
        opacity: 1.0,
        pointer: true,
    }
}

/// Applies a resolved style: cursor, rest opacity, hover and active. A widget that needs its
/// own hover fill (a danger button, `quiet_button`'s `hover` argument) overrides `hover_bg` first.
pub fn apply_press(el: Stateful<Div>, style: PressStyle) -> Stateful<Div> {
    let PressStyle {
        hover_bg,
        hover_fg,
        underline_on_hover,
        active_opacity,
        opacity,
        pointer,
    } = style;
    el.when(pointer, |el| el.cursor_pointer())
        .when(opacity < 1.0, |el| el.opacity(opacity))
        .when(
            hover_bg.is_some() || hover_fg.is_some() || underline_on_hover,
            |el| {
                el.hover(move |mut style| {
                    if let Some(bg) = hover_bg {
                        style = style.bg(rgb(bg));
                    }
                    if let Some(fg) = hover_fg {
                        style = style.text_color(rgb(fg));
                    }
                    if underline_on_hover {
                        style = style.underline();
                    }
                    style
                })
            },
        )
        .when(pointer, |el| {
            el.active(move |style| style.opacity(active_opacity))
        })
}

pub trait Pressable: Sized {
    /// Applies `press_style(look, enabled, theme)`: cursor, hover, active, disabled opacity.
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self;
    /// Attaches `handler` to clicks only when `enabled`.
    fn on_press(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl Pressable for Stateful<Div> {
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self {
        apply_press(self, press_style(look, enabled, theme))
    }

    fn on_press(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.when(enabled, |el| el.on_click(handler))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> [Theme; 2] {
        [Theme::dark(), Theme::light()]
    }

    #[test]
    fn every_look_hovers_to_its_fill_and_dims_while_pressed() {
        for t in themes() {
            let row = press_style(Look::Row, true, &t);
            assert_eq!(row.hover_bg, Some(t.hover));
            assert_eq!(row.hover_fg, None);
            let chip = press_style(Look::Chip, true, &t);
            assert_eq!((chip.hover_bg, chip.hover_fg), (Some(t.hover), Some(t.text)));
            let icon = press_style(Look::Icon, true, &t);
            assert_eq!((icon.hover_bg, icon.hover_fg), (Some(t.hover), Some(t.text)));
            let raised = press_style(Look::Raised, true, &t);
            assert_eq!((raised.hover_bg, raised.hover_fg), (Some(t.border), None));
            let text = press_style(Look::Text, true, &t);
            assert_eq!((text.hover_bg, text.hover_fg), (None, Some(t.text)));
            assert!(text.underline_on_hover);
            for style in [row, chip, icon, raised, text] {
                assert!(style.pointer);
                assert_eq!(style.active_opacity, opacity::ACTIVE);
                assert_eq!(style.opacity, 1.0, "no base opacity while enabled");
            }
        }
    }

    #[test]
    fn a_disabled_press_style_has_no_pointer_hover_or_active() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised, Look::Text] {
                let style = press_style(look, false, &t);
                assert_eq!(style.opacity, opacity::DISABLED);
                assert!(!style.pointer);
                assert_eq!((style.hover_bg, style.hover_fg), (None, None));
                assert!(!style.underline_on_hover);
                assert_eq!(style.active_opacity, 1.0);
            }
        }
    }

    /// Over every built-in theme: a hover fill is never the rest fill of the element and never
    /// `theme.selected` (a hovered row must not look selected).
    #[test]
    fn every_hover_fill_differs_from_the_rest_fill_and_from_selected_in_every_theme() {
        for entry in crate::theme::THEMES {
            let t = entry.build();
            // Row, Chip and Icon rest on the page, a panel or a bar.
            for look in [Look::Row, Look::Chip, Look::Icon] {
                let hover = press_style(look, true, &t).hover_bg.unwrap();
                for (rest, name) in [(t.bg, "bg"), (t.surface, "surface"), (t.chrome, "chrome")] {
                    assert_ne!(hover, rest, "{} {look:?} hover equals {name}", t.name);
                }
                assert_ne!(hover, t.selected, "{} {look:?} hover equals selected", t.name);
            }
            // Raised rests on `surface_raised`.
            let raised = press_style(Look::Raised, true, &t).hover_bg.unwrap();
            assert_ne!(raised, t.surface_raised, "{} Raised hover equals its rest fill", t.name);
            assert_ne!(raised, t.selected, "{} Raised hover equals selected", t.name);
        }
    }
}
```

In `lib.rs` add `pub mod pressable;` between `pub mod ports;` and `pub mod preview;` (the list is sorted).

- [ ] **Step 2: Run to verify**

Run: `cargo test -p mkrew-ui pressable`
Expected: 3 tests PASS.
If it fails to compile on a gpui method (`underline`, `text_color` on `StyleRefinement`), the names come from `gpui::Styled`, which `use gpui::prelude::*` brings in (`settings.rs` already calls `.hover(|style| style.underline())`).

- [ ] **Step 3: Commit**

```bash
git add crates/mkrew-ui/src/pressable.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Pressable, Look and press_style"
```

---

### Task 5: Rebuild the existing widgets on `Pressable`; add `toggle`, `dropdown_trigger`, `list_row`

**Files:**
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (top, `button_frame` 83-100, `button` 103-119, `secondary_button` 142-154, `quiet_button` 158-169, `disabled_button` 172-199, `danger_button` 203-213, `icon_button_in` 352-372, `segment` 390-416, `menu_item` 719-735; new functions after `menu_panel`)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:79`, `crates/mkrew-ui/src/shell/pane_render.rs:804` (keep the pointer)

**Interfaces:**
- Consumes: Task 4 items; `theme::{PALETTE_SELECTED_BAR, SWITCH_*}`, `icon_svg`, `Icon::ChevronDown`.
- Produces: `pub(crate) use mkrew_ui::pressable::{Look, PressStyle, Pressable, apply_press, press_style};`, unchanged signatures of `button`, `secondary_button`, `quiet_button(id, label, theme, hover: u32)`, `danger_button`, `disabled_button`, `icon_button_in`, `segment`, `menu_item`, `button_frame` (hover-free and, from this task, cursor-free), and new `segment_in(id: impl Into<ElementId>, active: bool, enabled: bool, theme: &Theme) -> Stateful<Div>` (`segment` delegates with `enabled = true`), `toggle(id: impl Into<ElementId>, on: bool, enabled: bool, theme: &Theme) -> Stateful<Div>`, `dropdown_trigger(id: impl Into<ElementId>, label: impl IntoElement, open: bool, enabled: bool, theme: &Theme) -> Stateful<Div>`, `list_row(id: impl Into<ElementId>, selected: bool, theme: &Theme) -> Stateful<Div>` (a selected row has no hover fill).

Widget construction needs a window, so it has no unit test; the decision logic is tested in Task 4, and `cargo clippy` plus the Task 12 visual check cover the rest.

- [ ] **Step 1: Re-export and the shared filled-control helper**

Directly under `use crate::*;` at the top of `widgets.rs`:

```rust
pub(crate) use mkrew_ui::pressable::{Look, PressStyle, Pressable, apply_press, press_style};

/// A filled control (primary button, switch): the fill stays, it dims on hover and press.
fn filled_press(
    el: gpui::Stateful<gpui::Div>,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_bg: None,
        hover_fg: None,
        ..press_style(Look::Chip, enabled, theme)
    };
    apply_press(el, style).when(enabled, |el| el.hover(|style| style.opacity(opacity::HOVER)))
}
```

- [ ] **Step 2: Buttons**

`button_frame`: delete the line `.cursor_pointer()` (the pressable adds it) and keep it hover-free.
Its two callers outside this file build their own hover and click, so they would lose the pointer: add `.cursor_pointer()` to the chain at `shell/files_render.rs:79` (`let ignored = button_frame("files-ignored", "Ignored")` ... after `.flex_none()`) and at `shell/pane_render.rs:804` (`let share = button_frame("pane-share", ..)` ... after its `)` closing the call, as the first chained method).
`git_render.rs:375` already sets `.cursor_default()` and needs nothing.
Replace the bodies:

```rust
pub(crate) fn button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    if primary {
        filled_press(
            button_frame(id, label)
                .bg(rgb(theme.accent))
                .text_color(rgb(button_fg(theme, true)))
                .font_weight(FontWeight(weight::SEMIBOLD)),
            true,
            theme,
        )
    } else {
        quiet_button(id, label, theme, theme.hover)
    }
}

pub(crate) fn secondary_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.control_border))
        .text_color(rgb(theme.text_dim))
        .pressable(Look::Row, true, theme)
}

pub(crate) fn quiet_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
    hover: u32,
) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_bg: Some(hover),
        ..press_style(Look::Icon, true, theme)
    };
    apply_press(button_frame(id, label).text_color(rgb(theme.text_muted)), style)
}

pub(crate) fn danger_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_bg: Some(theme.danger_bg),
        hover_fg: None,
        ..press_style(Look::Row, true, theme)
    };
    apply_press(button_frame(id, label).text_color(rgb(theme.danger_text)), style)
}
```

`disabled_button`: the primary branch stays exactly as it is (a muted fill, no extra fade, nothing to press).
In the secondary branch replace `base.opacity(opacity::DISABLED).text_color(rgb(theme.text_muted))` with:

```rust
        base.text_color(rgb(theme.text_muted))
            .pressable(Look::Row, false, theme)
```

- [ ] **Step 3: Icon button, segment, menu item**

```rust
pub(crate) fn icon_button_in(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::CONTROL_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MEDIUM))
        .text_color(rgb(theme.text_muted))
        .pressable(Look::Icon, true, theme)
        .child(icon_svg(glyph, theme::ICON_SIZE, color))
}
```

Replace `segment` with these two functions (the old `.cursor_pointer()` and the `let (hover, text) = ..` line go; a disabled segment is faded, has no pointer and no hover, an active one keeps its `selected` fill and the pointer):

```rust
pub(crate) fn segment(
    id: impl Into<gpui::ElementId>,
    active: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    segment_in(id, active, true, theme)
}

/// [`segment`] that can be disabled (a read-only Settings page): faded, no pointer, no hover.
pub(crate) fn segment_in(
    id: impl Into<gpui::ElementId>,
    active: bool,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::SEGMENT_HEIGHT))
        .px(px(space::M))
        .rounded(px(radius::CHIP))
        .whitespace_nowrap()
        .text_size(font_size::BODY)
        .when(active, |el| {
            el.bg(rgb(theme.selected))
                .text_color(rgb(theme.text))
                .when(enabled, |el| el.cursor_pointer())
                .when(!enabled, |el| el.opacity(opacity::DISABLED))
        })
        .when(!active, |el| {
            el.text_color(rgb(theme.text_muted))
                .pressable(Look::Chip, enabled, theme)
        })
}
```

```rust
pub(crate) fn menu_item(id: SharedString, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // A menu sits on `surface_raised`: its rows use `Look::Raised` (hover `border`).
    let style = press_style(Look::Raised, true, theme);
    apply_press(
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text_muted)),
        style,
    )
}
```

- [ ] **Step 4: New widgets**

Add after `menu_panel`:

```rust
/// The on / off switch: `SWITCH_WIDTH` x `SWITCH_HEIGHT`, knob inset `SWITCH_INSET`, accent
/// track while `on`. A disabled switch is faded and takes no press.
pub(crate) fn toggle(
    id: impl Into<gpui::ElementId>,
    on: bool,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let knob = px(theme::SWITCH_HEIGHT - 2.0 * theme::SWITCH_INSET);
    filled_press(
        div()
            .id(id)
            .flex_none()
            .w(px(theme::SWITCH_WIDTH))
            .h(px(theme::SWITCH_HEIGHT))
            .p(px(theme::SWITCH_INSET))
            .rounded_full()
            .flex()
            .when(on, |el| el.justify_end())
            .bg(rgb(if on { theme.accent } else { theme.border }))
            .child(div().size(knob).rounded_full().bg(rgb(theme.surface))),
        enabled,
        theme,
    )
}

/// A field frame with `label` and a chevron; the frame is accent while `open`.
pub(crate) fn dropdown_trigger(
    id: impl Into<gpui::ElementId>,
    label: impl IntoElement,
    open: bool,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if open { theme.accent } else { theme.border }))
        .bg(rgb(theme.chrome))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .pressable(Look::Row, enabled, theme)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label),
        )
        .child(icon_svg(
            Icon::ChevronDown,
            theme::ICON_SIZE_SMALL,
            theme.text_muted,
        ))
}

/// A `Look::Row` list row; `selected` adds the `theme.selected` fill and the
/// `PALETTE_SELECTED_BAR` accent bar on the left. Callers add the content and the handler.
pub(crate) fn list_row(
    id: impl Into<gpui::ElementId>,
    selected: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .relative()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::M))
        .py(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .when(selected, |el| {
            // A selected row keeps its `selected` fill on hover (no hover fill) and the pointer.
            el.bg(rgb(theme.selected)).cursor_pointer().child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(space::XS))
                    .bottom(px(space::XS))
                    .w(px(theme::PALETTE_SELECTED_BAR))
                    .rounded_full()
                    .bg(rgb(theme.accent)),
            )
        })
        .when(!selected, |el| el.pressable(Look::Row, true, theme))
}
```

- [ ] **Step 5: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.
`toggle`, `dropdown_trigger` and `list_row` are unused until Task 11 and Plan P2: put `#[allow(dead_code)] // used by Task 11 / Plan P2` on each, and remove it from `toggle` and `dropdown_trigger` in Task 11 (it stays on `list_row` until Plan P2).
Run: `cargo test -p mkrew-ui --bin mkrew`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/widgets.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): rebuild shared widgets on Pressable, add toggle, dropdown_trigger, list_row"
```

---

### Task 6: `floating_panel`, `appear` and the shade

**Files:**
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (new functions after `list_row`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:393-417` (`modal_shade`)
- Modify: `crates/mkrew-ui/src/shell/confirm_prompt.rs:165-176`

**Interfaces:**
- Consumes: `theme::{glass_gradient, shadow_float, GLASS_TINT_HEIGHT, GLASS_TINT_ANGLE, GLASS_TINT_FROM, GLASS_TINT_TO, PANEL_RISE, motion::FAST}`, `Theme.{glass, glass_border}`.
- Produces: `floating_panel(id: impl Into<ElementId>, theme: &Theme) -> Stateful<Div>` (glass fill, tint gradient, 1 px `glass_border`, `shadow_float`, `radius::LARGE`, `.relative()`, `.occlude()`; callers add size, layout and content), `appear(id: &'static str, content: impl IntoElement) -> AnimationElement<Div>`.

The pure part (gradient colors, shadow) is tested in Task 2.
Task 7 ends with a one-off grep that no dialog frame is left (no source-text test: the repo keeps none).

- [ ] **Step 1: Add the functions**

```rust
/// The one frame of every floating panel (palette, Settings, dialogs; ADR 0097): the glass
/// fill, the accent tint gradient over the top `GLASS_TINT_HEIGHT` px, a 1 px glass border,
/// the float shadow, `radius::LARGE`, and it swallows the mouse. Callers set size, layout and
/// content; the gradient is the first child, so their children paint over it.
pub(crate) fn floating_panel(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let (top, bottom) = theme::glass_gradient(theme);
    div()
        .id(id)
        .relative()
        .rounded(px(radius::LARGE))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.glass_border))
        .bg(gpui::rgba(theme.glass))
        .shadow(theme::shadow_float(theme))
        .occlude()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(theme::GLASS_TINT_HEIGHT))
                .rounded_t(px(radius::LARGE))
                .bg(gpui::linear_gradient(
                    theme::GLASS_TINT_ANGLE,
                    gpui::linear_color_stop(gpui::rgba(top), theme::GLASS_TINT_FROM),
                    gpui::linear_color_stop(gpui::rgba(bottom), theme::GLASS_TINT_TO),
                )),
        )
}

/// A floating panel's entrance: it fades in and rises `PANEL_RISE` over `motion::FAST`,
/// ease-out. Under reduced motion gpui draws the end state, so then it appears at once. The
/// wrapper fills its parent and centers the panel, so a panel sized as a fraction of the
/// window still resolves.
pub(crate) fn appear(
    id: &'static str,
    content: impl IntoElement,
) -> gpui::AnimationElement<gpui::Div> {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(content)
        .with_animation(
            id,
            gpui::Animation::new(theme::motion::FAST).with_easing(gpui::ease_out_quint()),
            |el, delta| el.opacity(delta).mt(px(theme::PANEL_RISE * (1.0 - delta))),
        )
}
```

If the compiler does not find `with_animation`, add `use gpui::AnimationExt as _;` at the top of `widgets.rs` (`voice_input.rs` gets it through `crate::*`).
If `bg(linear_gradient(..))` does not type-check, wrap it as `.bg(gpui::Fill::from(..))`; gpui's gradient example passes the `Background` straight to `bg`.

- [ ] **Step 2: Use `appear` in the shade and the confirm prompt**

`focus.rs` `modal_shade`: replace `.child(panel)` with `.child(appear(id, panel))` (add `use super::widgets::appear;` at the top of `focus.rs` if `appear` is not in scope).
`confirm_prompt.rs`: replace the last `.child(panel)` of the render with `.child(appear("confirm-panel", panel))`, importing `appear` the way the file imports `button`.

- [ ] **Step 3: Build**

Run: `cargo check -p mkrew-ui`
Expected: no errors.
Run: `cargo test -p mkrew-ui --bin mkrew`
Expected: PASS (dialogs still build their own frames until Task 7).
Mark `floating_panel` with `#[allow(dead_code)] // moved into by Task 7` only if clippy reports it unused here.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/confirm_prompt.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): floating_panel glass frame and appear motion"
```

---

### Task 7: Move every dialog panel to `floating_panel`

**Files:**
- Modify: the dialog files of the table at the top (24 panel builders).
- Test: none permanent (a styling move; repo policy forbids source-text tests). Checked by a one-off grep, `cargo check`, clippy and the Task 12 screenshots.

**Interfaces:**
- Consumes: `floating_panel`.
- Produces: no dialog builds its own frame.

The transformation is the same at every site.
Before (the lines vary only in the id, the width and a few extra `.max_h` / `.h` / `.gap`):

```rust
        let panel = div()
            .id("ask-dialog")
            .w(px(theme::ASK_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
```

After:

```rust
        let panel = floating_panel("ask-dialog", theme)
            .w(px(theme::ASK_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .font_family(theme.ui_font_family.clone())
```

Rules: `div().id(X)` becomes `floating_panel(X, theme)` with the same `X` (pass `&theme` where the local is a `Theme` value, as in `confirm_prompt.rs`); delete exactly the five lines `.rounded(px(radius::LARGE))`, `.border(px(theme::BORDER_WIDTH))`, `.border_color(rgb(theme.border))`, `.bg(rgb(theme.surface))`, `.shadow_md()`; keep everything else (`.w`, `.h`, `.max_h`, `.flex`, `.gap`, `.p`, `.font_family`, `.dialog_root(..)`, `.track_focus(..)`, `.on_key_down(..)`).
`doc_dialog_panel` keeps its return type `gpui::Stateful<gpui::Div>`.
`floating_panel` already calls `.relative()` and `.occlude()`.

- [ ] **Step 1: List the sites left (one-off check, not a test)**

Run (Git Bash): `grep -rn -B8 '^\s*\.shadow_md()' crates/mkrew-ui/src/shell | grep 'radius::LARGE'`
Expected now: one line per site of the table (24). This command is only a progress check; do not add it to the repo.

- [ ] **Step 2: Move the dialogs in four groups (compile after each)**

Group A: `shell/confirm_prompt.rs` (~130-141, here `floating_panel("confirm-dialog", &theme)`), `shell/ask_dialog.rs` (~298-309), `shell/composer.rs` (~642-653), `shell/diagram_dialog.rs` (~39-50), `shell/path_field.rs` (~135-146), `shell/phone_access.rs` (~385-396), `shell/ports.rs` (~314-325).
Run: `cargo check -p mkrew-ui`.

Group B: `shell/dialogs.rs` (two panels, ~378-389 and ~596-607), `shell/git_dialogs.rs` (~323-334 and ~415-426), `shell/history.rs` (~640-651 and ~833-844), `shell/send_dialog.rs` (`doc_dialog_panel`, ~276-287), `shell/worktrees.rs` (~637-648 and ~803-814).
Run: `cargo check -p mkrew-ui`.

Group C: `shell/navigation.rs` (quick open ~1032-1043 and palette ~1199-1210: only the five frame lines, Plan P2 rebuilds these builders), `shell/run.rs` (`run-setup` ~960-977 keeps `.max_h(max_height)` and `.gap(px(space::L))`; `run-picker` ~1189-1200), `shell/slash.rs` (~424-435), `shell/task_dialog.rs` (~418-429), `shell/workspace_actions.rs` (~1010-1021).
Run: `cargo check -p mkrew-ui`.

Group D: `shell/settings.rs` (~341-352): only the frame lines now; Task 10 rebuilds the rest.
Run: `cargo check -p mkrew-ui`.

- [ ] **Step 3: Run**

Run: `grep -rn -B8 '^\s*\.shadow_md()' crates/mkrew-ui/src/shell | grep 'radius::LARGE'`
Expected: no output.
Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean (an unused `radius` or `rgb` import after the edits is the only likely warning; remove it).

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): every modal dialog uses floating_panel"
```

---

### Task 8: `Setting::group` in `mkrew-core`

**Files:**
- Modify: `crates/mkrew-core/src/settings_fields.rs` (struct 59-69, `row` 71-88, `hidden` 90-100, `SETTINGS` rows 106-393, tests from 441)

**Interfaces:**
- Consumes: nothing.
- Produces: `Setting.group: &'static str` (a UI group, independent of `section`), `row(page, group, section, key, kind, label, help)`, `hidden(..)` with group `""`.

- [ ] **Step 1: Failing tests**

Add to `mod tests`:

```rust
    #[test]
    fn every_setting_has_a_group_and_a_groups_rows_are_contiguous() {
        let shown: Vec<&Setting> = SETTINGS.iter().filter(|s| !s.hidden).collect();
        for s in &shown {
            assert!(!s.group.is_empty(), "{} has no group", s.id());
        }
        let mut closed: Vec<(Page, &str)> = Vec::new();
        let mut open: Option<(Page, &str)> = None;
        for s in shown {
            let key = (s.page, s.group);
            if open != Some(key) {
                assert!(
                    !closed.contains(&key),
                    "group {:?} of {:?} is split by another group",
                    s.group,
                    s.page
                );
                if let Some(previous) = open.replace(key) {
                    closed.push(previous);
                }
            }
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mkrew-core settings_fields`
Expected: FAIL to compile (`group` is not a field).

- [ ] **Step 3: Implement**

In `Setting` add after `page`:

```rust
    /// The card the row sits in on its page (a UI group; `section` stays the TOML table).
    /// Rows of one group are contiguous in [`SETTINGS`]; hidden rows have none.
    pub group: &'static str,
```

`row` and `hidden`:

```rust
const fn row(
    page: Page,
    group: &'static str,
    section: &'static str,
    key: &'static str,
    kind: Kind,
    label: &'static str,
    help: &'static str,
) -> Setting {
    Setting {
        page,
        group,
        section,
        key,
        kind,
        label,
        help,
        hidden: false,
    }
}

const fn hidden(section: &'static str, key: &'static str) -> Setting {
    Setting {
        page: Page::Appearance,
        group: "",
        section,
        key,
        kind: Kind::Number,
        label: "",
        help: "",
        hidden: true,
    }
}
```

In `SETTINGS`, add the group string as the second argument of every `row(` call (right after the page).
Keep each row's existing label, help, kind, section and key unchanged.
General becomes (close_to_tray moves up to follow keep_awake so the groups are contiguous):

```rust
    row(
        General,
        "System",
        "awake",
        "keep_awake",
        Bool,
        "Keep the PC awake",
        "While an agent works, the PC does not sleep.",
    ),
    row(
        General,
        "System",
        "ui",
        "close_to_tray",
        Bool,
        "Close to tray",
        "Windows: the window's ✕ hides Mkrew to the tray icon.",
    ),
    row(
        General,
        "Workspace",
        "composer",
        "file_ref",
        Text,
        "File reference",
        "How a picked file is typed in the composer; {path} is the path.",
    ),
    // then follow_debounce_ms, reading_width and board_all_workspaces, all "Workspace"
```

The group of every other row, by `section.key`:

| Page | Group | Rows |
| --- | --- | --- |
| Appearance | `Theme` | `ui.theme`, `ui.light_theme`, `ui.dark_theme` |
| Appearance | `Fonts` | `ui.font_family`, `ui.font_size`, `terminal.font_family`, `terminal.font_size` |
| Appearance | `Diffs` | `ui.diff_mode`, `ui.diff_wrap` |
| Appearance | `Markdown` | `ui.md_mode` |
| Notifications | `Alerts` | `notify.needs_input`, `notify.done` |
| Notifications | `Delivery` | `notify.sound`, `notify.flash_taskbar` |
| Voice | `Service` | `voice.base_url`, `voice.model`, `voice.api_key_env` |
| Voice | `Recording` | `voice.language`, `voice.prompt`, `voice.max_seconds` |
| Jira | `Connection` | `jira.site`, `jira.email`, `jira.api_token_env` |
| Jira | `Board` | `jira.mine`, `jira.layout` |
| Phone | `Access` | `web.enabled`, `web.push` |
| Features | `Panels` | `features.docs`, `features.session` |
| Features | `Sessions` | `features.turns` |

In Features move the `session` row above the `turns` row (so `Panels` rows are adjacent).
The 5 `hidden(..)` rows stay as they are.

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-core settings_fields`
Expected: all PASS, including the existing template test (it compares key sets, so the reorder is safe).
Run: `cargo check --workspace`
Expected: no error (nothing else constructs `Setting`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/settings_fields.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): group every setting for the Settings cards"
```

---

### Task 9: Settings model: navigation groups, global search, Esc

**Files:**
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (imports 5-18, `Tab` 20-45, `Focus` 92-103, `SettingsDialog` 169-206, `rows` 212-233, `set_tab` 283-295, `step_focus` 400-422, `chain_edge` 427-447, `enter` 456-466, `key` 502-614, `edit_text` 617-629, `typing` 633-652, tests from 862)

**Interfaces:**
- Consumes: `palette::fuzzy(query, text) -> Option<(i64, Vec<u32>)>`, `crate::quick_open::match_ranges(text, &[u32]) -> Vec<Range<usize>>` (Plan P2 moves it to `crate::palette::match_ranges` and edits this one import), `Setting.group` (Task 8).
- Produces: `pub const NAV: [(&str, &[Tab]); 3]`, `Tab::all()` in `NAV` order, `Focus::Find`, `SettingsDialog.find: Input` (plus a private cache of the last search keyed by its query text), `pub fn search(query: &str) -> Vec<Hit>` (a label match always ranks above a help-only match), `pub struct Hit { setting, score, label_ranges }`, `SettingsDialog::{searching, hits() -> Rc<Vec<Hit>>, heading, dimmed_tabs, clear_find, group_of}`, `pub struct Group { pub title: &'static str, pub rows: Vec<(usize, Row)> }`, `pub fn grouped(rows: &[Row], searching: bool) -> Vec<Group>`, Ctrl+F focuses `Focus::Find`, Esc clears a non-empty query before closing.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` of `settings_dialog.rs` (helpers `dialog()`, `key()`, `typed()`, `type_text()`, `focus_on()` already exist there):

```rust
    #[test]
    fn the_page_list_follows_the_navigation_groups() {
        let all = Tab::all();
        let flat: Vec<Tab> = NAV.iter().flat_map(|(_, tabs)| tabs.iter().copied()).collect();
        assert_eq!(all, flat);
        let titles: Vec<&str> = all.iter().map(|t| t.title()).collect();
        assert_eq!(
            titles,
            [
                "General", "Appearance", "Notifications", "Voice", "Features", "Jira", "Phone",
                "Machines", "Keyboard", "Prompts"
            ]
        );
        assert_eq!(NAV.map(|(title, _)| title), ["App", "Integrations", "Input"]);
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.key(&key("down"), None, &keymap);
        assert_eq!(d.tab, Tab::Prompts, "Down from Keyboard");
    }

    #[test]
    fn search_matches_label_help_key_and_page() {
        let ids = |q: &str| -> Vec<String> { search(q).iter().map(|h| h.setting.id()).collect() };
        assert!(ids("keep the pc").contains(&"awake.keep_awake".to_owned()), "label");
        assert!(ids("tailnet").contains(&"web.enabled".to_owned()), "help only");
        assert!(
            ids("follow_debounce").contains(&"zed.follow_debounce_ms".to_owned()),
            "key"
        );
        let jira = ids("jira");
        for s in SETTINGS.iter().filter(|s| s.page == Page::Jira) {
            assert!(jira.contains(&s.id()), "page name finds {}", s.id());
        }
    }

    #[test]
    fn a_label_match_ranks_above_a_help_only_match() {
        for query in ["theme", "font", "notif", "size", "wrap", "zed", "api"] {
            let hits = search(query);
            assert!(!hits.is_empty(), "{query}");
            let first_help_only = hits.iter().position(|h| h.label_ranges.is_empty());
            if let Some(at) = first_help_only {
                assert!(
                    hits[at..].iter().all(|h| h.label_ranges.is_empty()),
                    "{query}: a label match follows a help-only match"
                );
            }
            assert!(!hits[0].label_ranges.is_empty(), "{query}: the matched chars are marked");
        }
    }

    #[test]
    fn hidden_settings_are_never_listed_and_no_match_is_empty() {
        for query in ["sidebar", "inspector_width", "right_panel", "md_split"] {
            assert!(search(query).iter().all(|h| !h.setting.hidden), "{query}");
        }
        assert!(search("zzzzqq").is_empty());
    }

    #[test]
    fn a_blank_query_shows_the_page_rows() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        let page = d.rows(&keymap);
        d.find = Input::from("   ".to_owned());
        assert_eq!(d.rows(&keymap), page);
        assert!(d.hits().is_empty());
        assert!(d.dimmed_tabs().is_empty(), "nothing is dimmed without a query");
        assert_eq!(d.heading(), "General");
    }

    #[test]
    fn a_query_lists_matches_of_every_page_and_dims_the_pages_without_one() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.key(&key("ctrl-f"), None, &keymap);
        assert_eq!(d.focus, Focus::Find);
        type_text(&mut d, "keepthepc", &keymap);
        let rows = d.rows(&keymap);
        assert!(rows.iter().any(|r| r.id() == "awake.keep_awake"));
        assert_eq!(d.heading(), "Search results");
        let dimmed = d.dimmed_tabs();
        for never_searched in [Tab::Keyboard, Tab::Machines, Tab::Prompts] {
            assert!(dimmed.contains(&never_searched));
        }
        assert!(!dimmed.contains(&Tab::Settings(Page::General)));
    }

    #[test]
    fn esc_restores_a_draft_then_clears_the_search_then_closes() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.key(&key("ctrl-f"), None, &keymap);
        type_text(&mut d, "followinzed", &keymap);
        d.key(&key("down"), None, &keymap);
        assert!(matches!(d.focus, Focus::Row(0)));
        d.key(&typed("9"), None, &keymap);
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None, "draft restored");
        assert!(!d.find.is_empty());
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None, "search cleared");
        assert!(d.find.is_empty());
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::Close);
    }

    #[test]
    fn tab_visits_the_search_box_between_the_page_list_and_the_rows() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Find);
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Row(0));
        assert!(!d.chain_edge(true, &keymap));
        d.set_tab(Tab::Machines);
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Find);
        assert!(d.chain_edge(false, &keymap), "Machines has no rows after the box");
    }

    #[test]
    fn rows_group_into_cards_and_each_result_is_its_own_card() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Settings(Page::Appearance));
        let rows = d.rows(&keymap);
        let groups = grouped(&rows, false);
        let titles: Vec<&str> = groups.iter().map(|g| g.title).collect();
        assert_eq!(titles, ["Theme", "Fonts", "Diffs", "Markdown"]);
        let indices: Vec<usize> = groups
            .iter()
            .flat_map(|g| g.rows.iter().map(|(i, _)| *i))
            .collect();
        assert_eq!(indices, (0..rows.len()).collect::<Vec<_>>());
        assert_eq!(grouped(&rows, true).len(), rows.len());
        d.set_tab(Tab::Keyboard);
        let keys = d.rows(&keymap);
        assert_eq!(grouped(&keys, false).len(), keys.len(), "shortcuts stay flat");
        assert_eq!(d.group_of(0, &keymap), 0);
    }
```

Replace the two existing tests this change breaks:

- Delete `the_page_list_has_prompts_before_machines_and_up_down_reaches_it` (line ~1512): `the_page_list_follows_the_navigation_groups` replaces it.
- In `tab_leaves_the_page_list_and_rows_at_their_ends` (line ~1151) the search box adds one Tab stop, so change the loop to:

```rust
        for _ in 0..=last + 1 {
            d.key(&key("tab"), None, &keymap);
        }
```

(the rest of that test is unchanged: `chain_edge(true)` at the page list, `Row(last)` after the presses, `enter(true)` to `Focus::Tabs`, `enter(false)` to `Focus::Row(last)`, the capture check).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mkrew-ui settings_dialog`
Expected: FAIL to compile (`NAV`, `search`, `Focus::Find`, ... not defined).

- [ ] **Step 3: Implement the model**

Imports: add `use std::cell::RefCell;`, `use std::collections::HashSet;`, `use std::ops::Range;`, `use std::rc::Rc;` and `use crate::quick_open::match_ranges;`.

Navigation (replace `Tab::all`; `title()` stays):

```rust
/// The page list, by overline (spec 6.1): the single order of the navigation.
pub const NAV: [(&str, &[Tab]); 3] = [
    (
        "App",
        &[
            Tab::Settings(Page::General),
            Tab::Settings(Page::Appearance),
            Tab::Settings(Page::Notifications),
            Tab::Settings(Page::Voice),
            Tab::Settings(Page::Features),
        ],
    ),
    (
        "Integrations",
        &[
            Tab::Settings(Page::Jira),
            Tab::Settings(Page::Phone),
            Tab::Machines,
        ],
    ),
    ("Input", &[Tab::Keyboard, Tab::Prompts]),
];

impl Tab {
    pub fn all() -> Vec<Tab> {
        NAV.iter().flat_map(|(_, tabs)| tabs.iter().copied()).collect()
    }
    // title() unchanged
}
```

Search and grouping (free items above `impl SettingsDialog`):

```rust
/// A setting that matches the global search.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub setting: &'static Setting,
    /// The best of the label, help, key and page scores (always above zero).
    pub score: i64,
    /// Byte ranges of the label the query matched (accented in the row).
    pub label_ranges: Vec<Range<usize>>,
}

/// Every visible setting whose label, help, `section.key` or page name `palette::fuzzy` scores
/// above zero for `query`, best first (ties keep the table order). Hidden settings never match.
pub fn search(query: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = SETTINGS
        .iter()
        .filter(|setting| !setting.hidden)
        .filter_map(|setting| {
            let scores = [
                palette::fuzzy(query, setting.label),
                palette::fuzzy(query, setting.help),
                palette::fuzzy(query, &setting.id()),
                palette::fuzzy(query, setting.page.title()),
            ];
            let label_ranges = scores[0]
                .as_ref()
                .map(|(_, at)| match_ranges(setting.label, at))
                .unwrap_or_default();
            let score = scores
                .iter()
                .flatten()
                .map(|(score, _)| *score)
                .max()
                .filter(|score| *score > 0)?;
            Some(Hit {
                setting,
                score,
                label_ranges,
            })
        })
        .collect();
    // A label match always ranks above a help-only match; the best score orders within each.
    hits.sort_by(|a, b| {
        let (a_label, b_label) = (!a.label_ranges.is_empty(), !b.label_ranges.is_empty());
        b_label.cmp(&a_label).then(b.score.cmp(&a.score))
    });
    hits
}

/// One card of a page: its overline title and its rows with their index in `rows()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub rows: Vec<(usize, Row)>,
}

/// `rows` as cards: consecutive settings of one group share a card. A shortcut, and every
/// search result (`searching`), is a card of its own with an empty title.
pub fn grouped(rows: &[Row], searching: bool) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, row) in rows.iter().copied().enumerate() {
        match row {
            Row::Setting(setting) if !searching => match groups.last_mut() {
                Some(last) if last.title == setting.group => last.rows.push((index, row)),
                _ => groups.push(Group {
                    title: setting.group,
                    rows: vec![(index, row)],
                }),
            },
            _ => groups.push(Group {
                title: "",
                rows: vec![(index, row)],
            }),
        }
    }
    groups
}
```

`Focus`: add after `Tabs`:

```rust
    /// The search box at the top of the page list.
    Find,
```

`SettingsDialog`: add the field after `search`:

```rust
    /// The global search over every page (Ctrl+F).
    pub find: Input,
    /// The last search, keyed by its query text (see [`SettingsDialog::hits`]).
    hit_cache: RefCell<Option<(String, Rc<Vec<Hit>>)>>,
```

and `find: Input::default(), hit_cache: RefCell::new(None),` in `new()`.
`rows`: at the top of the body add:

```rust
        if self.searching() {
            return self
                .hits()
                .iter()
                .map(|hit| Row::Setting(hit.setting))
                .collect();
        }
```

New methods in `impl SettingsDialog`:

```rust
    /// A non-blank query is typed in the search box: the right side lists matches of every page.
    pub fn searching(&self) -> bool {
        !self.find.trim().is_empty()
    }

    /// The matches of the query, in the order of [`Self::rows`] (empty without a query). The
    /// result is cached under the query text it was computed for, so a render or key press
    /// computes the search once, and an edit of `find` by any path just misses the cache.
    pub fn hits(&self) -> Rc<Vec<Hit>> {
        let query = self.find.trim();
        if query.is_empty() {
            return Rc::default();
        }
        let mut cache = self.hit_cache.borrow_mut();
        match cache.as_ref() {
            Some((cached, hits)) if cached == query => hits.clone(),
            _ => {
                let hits = Rc::new(search(query));
                *cache = Some((query.to_owned(), hits.clone()));
                hits
            }
        }
    }

    /// The title of the right side: "Search results", else the page.
    pub fn heading(&self) -> String {
        if self.searching() {
            "Search results".to_owned()
        } else {
            self.tab.title().to_owned()
        }
    }

    /// The pages to dim in the navigation: while a query is typed, those without a match (the
    /// shortcut, machine and prompt pages are never searched).
    pub fn dimmed_tabs(&self) -> Vec<Tab> {
        if !self.searching() {
            return Vec::new();
        }
        let pages: HashSet<Page> = self.hits().iter().map(|hit| hit.setting.page).collect();
        Tab::all()
            .into_iter()
            .filter(|tab| match tab {
                Tab::Settings(page) => !pages.contains(page),
                _ => true,
            })
            .collect()
    }

    /// Esc, or the empty state's "Clear search": the query goes, the keyboard stays in the box.
    pub fn clear_find(&mut self) -> Effect {
        let effect = self.commit_draft();
        self.find = Input::default();
        self.focus = Focus::Find;
        effect
    }

    /// The index of the card (see [`grouped`]) that holds row `index`: the scroll position.
    pub fn group_of(&self, index: usize, keymap: &Keymap) -> usize {
        let rows = self.rows(keymap);
        grouped(&rows, self.searching())
            .iter()
            .position(|group| group.rows.iter().any(|(i, _)| *i == index))
            .unwrap_or(0)
    }
```

`set_tab`: add `self.find = Input::default();` after `self.tab = tab;` (a click on a page leaves the search).
`step_focus`: replace its first lines so the page stops only exist without a query:

```rust
        let mut order = vec![Focus::Tabs, Focus::Find];
        if !self.searching() {
            if self.tab == Tab::Keyboard {
                order.push(Focus::Search);
            }
            if self.tab == Tab::Prompts {
                order.push(Focus::Prompts(Spot::List));
                if self.prompts.draft.is_some() {
                    order.extend([Spot::Name, Spot::Body].map(Focus::Prompts));
                }
            }
            if self.add.is_some() {
                order.extend([Field::Name, Field::Host, Field::Mk].map(Focus::Add));
            }
        }
```

`chain_edge` (the code after the `if back { .. }` early return):

```rust
        let rows = self.rows(keymap).len();
        let plain_page = !self.searching()
            && self.tab != Tab::Keyboard
            && self.tab != Tab::Prompts
            && self.add.is_none();
        match self.focus {
            Focus::Row(i) => i + 1 >= rows,
            Focus::Find => rows == 0 && (self.searching() || plain_page),
            Focus::Search => rows == 0,
            Focus::Add(field) => field == Field::Mk,
            Focus::Prompts(spot) => spot == self.prompts_last_spot(),
            // The search box always follows the page list.
            Focus::Tabs => false,
        }
```

(the `if back { return self.focus == Focus::Tabs; }` above it stays).
`enter`: the zero-rows arms become

```rust
        let last = match self.rows(keymap).len() {
            0 if self.searching() => Focus::Find,
            0 if self.tab == Tab::Keyboard => Focus::Search,
            0 if self.add.is_some() => Focus::Add(Field::Mk),
            0 if self.tab == Tab::Prompts => Focus::Prompts(self.prompts_last_spot()),
            0 => Focus::Find,
            count => Focus::Row(count - 1),
        };
```

`key`: after the Prompts shortcut block and before `let Some(key) = dialog_key(..)` add Ctrl+F:

```rust
        let m = keystroke.modifiers;
        if keystroke.key == "f" && m.control && !m.shift && !m.alt && !m.platform {
            return self.move_focus(Focus::Find, keymap);
        }
```

In `DialogKey::Cancel`, after the `self.add.is_some()` block and before the `Focus::Search` block:

```rust
                if !self.find.is_empty() {
                    return self.clear_find();
                }
```

In the Up/Down arm, the inner `match self.focus` gets (replacing the old `Focus::Search | Focus::Add(_) | Focus::Prompts(_) => Effect::None`):

```rust
                    Focus::Find if !up && !self.rows(keymap).is_empty() => {
                        self.move_focus(Focus::Row(0), keymap)
                    }
                    Focus::Find | Focus::Search | Focus::Add(_) | Focus::Prompts(_) => {
                        Effect::None
                    }
```

The space rule `!matches!(self.focus, Focus::Search | Focus::Add(_))` becomes `!matches!(self.focus, Focus::Search | Focus::Find | Focus::Add(_))`.
`edit_text`: start with `if self.focus == Focus::Find { edit(&mut self.find); } else if self.focus == Focus::Search { .. }` (keep the rest chained after it).
`typing`: add `} else if self.focus == Focus::Find { Some(&self.find)` before the `Focus::Search` branch.

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-ui settings_dialog`
Expected: all PASS.
Run: `cargo check -p mkrew-ui`
Expected: no errors (`shell/settings.rs` has no exhaustive `match` on `Focus`, so it is not touched in this task).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/settings_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Settings navigation groups and global search model"
```

---

### Task 10: Settings frame, navigation, search box, results, empty state and footer

**Files:**
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`render_settings_dialog` 195-376, `settings_link` 413-429, `settings_row` 431-445 (signature only; Task 11 writes the body), `settings_key_down` scroll lines 101-113)

**Interfaces:**
- Consumes: Tasks 4-7 widgets, Task 9 model (`NAV`, `dimmed_tabs`, `heading`, `hits`, `searching`, `clear_find`, `group_of`, `grouped`), `search_box`, `caret_field`, `overline`, `empty_state`, `secondary_button`, `kbd`, `icon_button`.
- Produces: the Settings frame (`floating_panel` of `SETTINGS_DIALOG_WIDTH` x `SETTINGS_DIALOG_HEIGHT`), a navigation with three overlines, a header with the page title and a close button, a footer.
  Task 11 fills `settings_row` and `settings_control`.

There is no unit test for drawing; the model tests of Task 9 cover the logic.
To keep each commit compiling, add the `settings_row` signature of Task 11 Step 1 (with the new parameters unused, `_divider`, `_hit`) in this task and complete its body in Task 11.

- [ ] **Step 1: Navigation with overlines, dimming and the search box**

Replace the `tabs` / `nav` block (lines 206-237) with:

```rust
        let dimmed = dialog.dimmed_tabs();
        let tab_item = |tab: Tab| {
            let active = tab == dialog.tab && !dialog.searching();
            let focused = tab == dialog.tab && dialog.focus == Focus::Tabs;
            div()
                .id(SharedString::from(format!("settings-tab-{}", tab.title())))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .text_size(font_size::BODY)
                .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                // The active page keeps its `selected` fill on hover; the others hover.
                .when(active, |el| el.bg(rgb(theme.selected)).cursor_pointer())
                .when(!active, |el| el.pressable(Look::Row, true, theme))
                .when(focused, |el| {
                    el.border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.selected_ring))
                })
                // Dimmed, still readable and still clickable.
                .when(dimmed.contains(&tab), |el| el.opacity(opacity::DIMMED))
                .on_press(
                    true,
                    cx.listener(move |shell, _, window, cx| {
                        shell.settings_do(|d, _| d.set_tab(tab), window, cx);
                    }),
                )
                .child(tab.title())
                .into_any_element()
        };
        let nav_items: Vec<gpui::AnyElement> = settings_dialog::NAV
            .iter()
            .flat_map(|(title, tabs)| {
                std::iter::once(
                    overline(title, theme)
                        .px(px(space::S))
                        .pt(px(space::S))
                        .into_any_element(),
                )
                .chain(tabs.iter().map(|&tab| tab_item(tab)))
            })
            .collect();
        let find = search_box(
            caret_field(
                "settings-find",
                &dialog.find,
                "Search settings",
                dialog.focus == Focus::Find && typing,
                theme,
                cx,
                |shell, caret| {
                    if let Some(d) = shell.settings_dialog.as_mut() {
                        d.find.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Find, keymap), window, cx);
            })),
            Some(kbd("Ctrl+F".to_owned(), theme, false).into_any_element()),
            dialog.focus == Focus::Find,
            theme,
        );
        let nav = div()
            .w(px(theme::SETTINGS_NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(find)
            .children(nav_items);
```

Make `settings_link` a text pressable (replacing `.cursor_pointer()`, `.hover(..underline)` and `.on_click`):

```rust
        div()
            .id(id)
            .text_color(rgb(theme.accent_text))
            .pressable(Look::Text, true, theme)
            .on_press(true, cx.listener(move |shell, _, _, cx| on_click(shell, cx)))
            .focus_stop(theme)
            .child(label)
```

- [ ] **Step 2: Cards, results, empty state, header, footer, panel**

In `render_settings_dialog` the body's last child (the `.children(rows.iter().enumerate().map(..))` of `body`) becomes:

```rust
            .children(self.settings_cards(dialog, &rows, typing, theme, cx))
```

and the body's `.gap(px(space::XS))` becomes `.gap(px(space::M))` so cards breathe.
The Keyboard page's own search box is shown only without a global query; the render and the scroll index of Step 3 both ask one helper, a free function in `shell/settings.rs`:

```rust
/// Whether the Keyboard page's own search box is drawn (a global query hides it).
fn keyboard_search_shown(dialog: &SettingsDialog) -> bool {
    dialog.tab == Tab::Keyboard && !dialog.searching()
}
```

`render_settings_dialog` uses it as `let search = keyboard_search_shown(dialog).then(|| { .. })`; the Machines and Prompts pages likewise draw only while `!dialog.searching()`.

Add the method:

```rust
    /// The right side's rows: the empty state for a query without a match, else the cards.
    fn settings_cards(
        &self,
        dialog: &SettingsDialog,
        rows: &[Row],
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let searching = dialog.searching();
        if searching && rows.is_empty() {
            return vec![
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(space::M))
                    .py(px(space::XL))
                    .child(empty_state(
                        theme,
                        None,
                        None,
                        format!("No setting matches “{}”.", dialog.find.trim()).into(),
                        None,
                    ))
                    .child(
                        secondary_button("settings-clear-search", "Clear search (Esc)", theme)
                            .on_press(
                                true,
                                cx.listener(|shell, _, window, cx| {
                                    shell.settings_do(|d, _| d.clear_find(), window, cx);
                                }),
                            ),
                    )
                    .into_any_element(),
            ];
        }
        let hits = dialog.hits();
        settings_dialog::grouped(rows, searching)
            .into_iter()
            .map(|group| {
                let flat = group.title.is_empty();
                let single_shortcut = flat && !searching;
                let title = group.title;
                let items = group
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(at, (index, row))| {
                        self.settings_row(dialog, index, row, at > 0, hits.get(index), typing, theme, cx)
                    });
                let card = div()
                    .flex()
                    .flex_col()
                    .rounded(px(radius::LARGE))
                    .bg(rgb(theme.surface))
                    .children(items);
                if single_shortcut {
                    // A shortcut row: no title (the Keyboard page stays a flat list).
                    card.into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children((!flat).then(|| overline(title, theme).px(px(space::S))))
                        .child(card)
                        .into_any_element()
                }
            })
            .collect()
    }
```

Replace the panel header and footer, and the panel itself:

```rust
        let header = div()
            .flex()
            .items_center()
            .px(px(space::L))
            .py(px(space::M))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(
                div()
                    .flex_1()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                icon_button("settings-close", Icon::X, theme)
                    .tooltip(tooltip(theme, "Close", "Esc"))
                    .on_press(
                        true,
                        cx.listener(|shell, _, window, cx| {
                            shell.close_settings_dialog(window, cx);
                        }),
                    ),
            );
        let footer = div()
            .flex()
            .items_center()
            .gap(px(space::L))
            .px(px(space::L))
            .py(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .text_size(font_size::META)
            .child(
                div()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                self.settings_link("settings-edit-toml", "settings.toml", theme, cx, |shell, cx| {
                    shell.edit_settings_file(cx);
                }),
            )
            .child(
                self.settings_link("settings-edit-keymap", "keymap.toml", theme, cx, |shell, cx| {
                    shell.edit_keymap_file(cx);
                }),
            )
            .child(div().flex_1())
            .child(div().text_color(rgb(theme.text_muted)).child(
                // The Prompts page saves by itself, so its own keys stay in the hint.
                if dialog.tab == Tab::Prompts && !dialog.searching() {
                    "Ctrl+N new · Ctrl+S saves · Esc closes"
                } else {
                    "Changes apply at once."
                },
            ));
        let max_height = (viewport_height - px(space::XXL * 2.0)).max(px(0.0));
        let max_width = (window.viewport_size().width - px(space::XXL * 2.0)).max(px(0.0));
        let panel = floating_panel("settings-dialog", theme)
            .w(px(theme::SETTINGS_DIALOG_WIDTH))
            .h(px(theme::SETTINGS_DIALOG_HEIGHT))
            .max_w(max_width)
            .max_h(max_height)
            .flex()
            .flex_col()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::settings_key_down),
            )
            .child(self.typing(&self.dialog_focus, Self::settings_key_down, cx))
            .child(header)
            .child(div().flex().flex_1().min_h_0().child(nav).child(body))
            .child(footer);
        Self::modal_shade("settings-shade", panel, cx)
```

The close button deliberately has no `focus_stop`: Esc closes and the Tab chain keeps its current dialog buttons.

- [ ] **Step 3: Scroll to the card, not the row**

In `settings_key_down` (lines 101-113) replace the scroll block with:

```rust
        // Keep the focused row on screen: the banner and the Keyboard search box come before
        // the cards, and a card holds several rows.
        if let Some(dialog) = &self.settings_dialog
            && let Focus::Row(index) = dialog.focus
        {
            let banner = match (&dialog.settings, dialog.tab) {
                (Err(_), Tab::Settings(_)) => true,
                (_, Tab::Keyboard) => keymap::notice(&self.keymap_errors).is_some(),
                _ => false,
            };
            let search = keyboard_search_shown(dialog);
            let card = dialog.group_of(index, &self.keymap);
            self.settings_scroll
                .scroll_to_item(card + usize::from(banner) + usize::from(search));
        }
```

- [ ] **Step 4: Build**

Run: `cargo check -p mkrew-ui`
Expected: no errors once the `settings_row` signature of Task 11 Step 1 exists.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Settings frame, grouped navigation, search box and footer"
```

---

### Task 11: Settings cards, rows and shared controls

**Files:**
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`settings_row` 431-519, `settings_control` 521-640, `list_pick_view` 696-765, `binding_control` 643-693)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (remove the `#[allow(dead_code)]` of `toggle` and `dropdown_trigger`)

**Interfaces:**
- Consumes: `toggle`, `segmented`, `segment_in`, `dropdown_trigger`, `Pressable`, `Look`, `Hit` (Task 9), `gpui::StyledText` highlights.
- Produces: `settings_row(&self, dialog, index, row, divider: bool, hit: Option<&Hit>, typing, theme, cx) -> AnyElement`, `settings_control(&self, dialog, index, setting, focused, typing, theme, cx)`, `Shell::press_row(index, cx)` (the one row-click handler).

- [ ] **Step 1: The row**

Replace `settings_row` (it keeps the `title` / `help` / `control` tuple, the `is_default` tag, the error line and the list pick):

```rust
    #[allow(clippy::too_many_arguments)]
    fn settings_row(
        &self,
        dialog: &SettingsDialog,
        index: usize,
        row: Row,
        divider: bool,
        hit: Option<&settings_dialog::Hit>,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = dialog.focus == Focus::Row(index);
        let error = dialog.errors.get(&row.id()).cloned();
        let (title, help, control): (SharedString, Option<String>, gpui::AnyElement) = match row {
            Row::Setting(setting) => (
                setting.label.into(),
                (!setting.help.is_empty()).then(|| setting.help.to_owned()),
                self.settings_control(dialog, index, setting, focused, typing, theme, cx),
            ),
            Row::Binding(command) => (
                palette::title(command).into(),
                command
                    .context()
                    .map(|context| format!("{} only", context_name(context))),
                self.binding_control(dialog, command, theme, cx),
            ),
        };
        let is_default = match row {
            Row::Setting(setting) => dialog.settings.as_ref().is_ok_and(|settings| {
                setting.current(settings) == setting.current(&Default::default())
            }),
            Row::Binding(_) => false,
        };
        // A search result names its place: "Appearance › Fonts".
        let crumb = hit.map(|hit| format!("{} › {}", hit.setting.page.title(), hit.setting.group));
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let label: gpui::AnyElement = match hit {
            Some(hit) => gpui::StyledText::new(title.clone())
                .with_highlights(hit.label_ranges.iter().cloned().map(|range| {
                    (
                        range,
                        gpui::HighlightStyle {
                            color: Some(accent),
                            font_weight: Some(FontWeight(weight::SEMIBOLD)),
                            ..gpui::HighlightStyle::default()
                        },
                    )
                }))
                .into_any_element(),
            None => title.into_any_element(),
        };
        div()
            .id(SharedString::from(format!("settings-row-{}", row.id())))
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .when(divider, |el| {
                el.border_t(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.divider))
            })
            .when(focused, |el| el.bg(rgb(theme.selected)).cursor_pointer())
            .when(!focused, |el| el.pressable(Look::Row, true, theme))
            .on_press(true, Self::press_row(index, cx))
            .children(crumb.map(|crumb| meta_line(theme, crumb.into())))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(space::S))
                                    .text_size(font_size::BODY)
                                    .text_color(rgb(theme.text))
                                    .child(label)
                                    .when(is_default, |el| {
                                        el.child(
                                            div()
                                                .text_size(font_size::META)
                                                .text_color(rgb(theme.text_muted))
                                                .child("default"),
                                        )
                                    }),
                            )
                            .children(help.map(|help| meta_line(theme, help.into()))),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(theme::SETTINGS_CONTROL_WIDTH))
                            .flex()
                            .justify_end()
                            .child(control),
                    ),
            )
            .children(error.map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(error)
            }))
            .children(self.list_pick_view(dialog, row, typing, theme, cx))
            .into_any_element()
    }
```

Add the shared click handler next to `settings_row` (the row, the switch and the dropdown trigger all use it, so the closure is written once):

```rust
    /// A click on a row, or on its switch or dropdown trigger: the keyboard moves to the row
    /// and the click acts like Enter (`SettingsDialog::click`).
    fn press_row(
        index: usize,
        cx: &Context<'_, Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static + use<> {
        cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.settings_do(|d, keymap| d.click(Focus::Row(index), keymap), window, cx);
        })
    }
```

The row is a click target that moves the keyboard to it and acts like Enter (as today), through `.pressable(Look::Row, ..)` and `.on_press(..)`; a focused row keeps its `selected` fill and gets no hover fill.
The controls inside take their own press and stop it from reaching the row.

- [ ] **Step 2: The controls**

In `settings_control` add the parameter `index: usize` after `dialog` and replace the `Kind::Bool`, `Kind::Choice` and `Kind::Font | Kind::List(_)` arms (the `Kind::Text | Kind::Number` arm is unchanged):

```rust
            Kind::Bool => toggle(
                SharedString::from(format!("switch-{}", setting.id())),
                shown == "true",
                !read_only,
                theme,
            )
            // Today's focus ring (`shell/settings.rs:552-554`): a 1 px ring while the row has the keyboard.
            .when(focused, |el| {
                el.border(px(theme::BORDER_WIDTH)).border_color(rgb(ring))
            })
            .on_press(!read_only, Self::press_row(index, cx))
            .into_any_element(),
            Kind::Choice(choices) => segmented(theme)
                .when(focused, |el| el.border_color(rgb(ring)))
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    // Read-only: each segment is faded, with no pointer and no hover.
                    segment_in(
                        SharedString::from(format!("choice-{}-{choice}", setting.id())),
                        active,
                        !read_only,
                        theme,
                    )
                    .text_size(font_size::META)
                    .on_press(
                        !active && !read_only,
                        cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                        }),
                    )
                    .child(choice)
                }))
                .into_any_element(),
            Kind::Font | Kind::List(_) => {
                let open = dialog
                    .list_pick
                    .as_ref()
                    .is_some_and(|pick| pick.setting.id() == setting.id());
                dropdown_trigger(
                    SharedString::from(format!("dropdown-{}", setting.id())),
                    if shown.is_empty() {
                        "Default".to_owned()
                    } else {
                        shown
                    },
                    open,
                    !read_only,
                    theme,
                )
                // Today's focus ring (`shell/settings.rs:628`); an open pick keeps the accent frame.
                .when(focused && !open, |el| el.border_color(rgb(ring)))
                .on_press(!read_only, Self::press_row(index, cx))
                .into_any_element()
            }
```

A click on a Font or List row's control calls `d.click(Focus::Row(index))`, which focuses the row and activates it, opening the pick (`Row::click_focuses_only` is false for these kinds, as today).
A click on the switch does the same and toggles it.

In `list_pick_view` replace `.on_click(cx.listener(..))` on the `menu_item(..)` rows with `.on_press(true, cx.listener(..))` (same closure).
In `binding_control` replace the Reset chain (`.cursor_pointer()`, `.hover(..)`, `.on_click(..)`) with:

```rust
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .pressable(Look::Icon, true, theme)
                        .tooltip(tooltip(
                            theme,
                            &format!("Back to {}", keymap::keys_label(&command.default_keys())),
                            "",
                        ))
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.settings_do(|d, _| d.reset(command), window, cx);
                            }),
                        )
```

(keep its `.id(..)`, `.focus_stop(theme)` and `.child("Reset")` lines).

- [ ] **Step 3: Build, lint, run**

Remove the `#[allow(dead_code)]` of `toggle` and `dropdown_trigger` in `widgets.rs`.
Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.
Run: `cargo test -p mkrew-ui`
Expected: PASS.
Run: `cargo test -p mkrew-ui --bin mkrew`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/widgets.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Settings cards, result rows and shared controls"
```

---

### Task 12: SPEC section 14, HUMAN_CHECKS, screenshots and the full verify

**Files:**
- Modify: `docs/SPEC.md` (a new paragraph pair at the end of section 14, before the `---` that precedes `## §15`; the Settings bullet at lines 1695-1703)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify (non-blocking)" section)

**Interfaces:**
- Consumes: everything above.
- Produces: the docs of spec section 12 that belong to P1 (ADR 0097 was Task 1; SPEC section 13 keys and the protocol section belong to Plan P2; the `mkrew-ui-design` skill and the `ui-reviewer` checklist belong to Plan P3).

- [ ] **Step 1: SPEC section 14, new paragraphs**

Insert after the "**Design check (every milestone with UI):**" paragraph and before the `---` that precedes `## §15 Non-functional requirements`:

```markdown
**Floating panels (glass), ADR 0097.**
The palette, Settings, every modal dialog and the confirm prompt share one frame, `widgets::floating_panel`.
It is a translucent fill (`glass`: the surface at 0.88 alpha in dark themes and 0.92 in light ones, raised until body text keeps 4.5:1 over a light or a dark page behind it), a 1 px `glass_border`, an accent `glass_tint` gradient over the top `GLASS_TINT_HEIGHT` px, `shadow_float` and `radius::LARGE`.
There is no real backdrop blur (gpui-pre 0.3.7 cannot blur behind one element).
The scrim behind a modal is black at 24 % (dark) or 16 % (light).
A panel fades in and rises 4 px over `motion::FAST` and disappears at once; with the OS "Animation effects" setting off (Windows) it appears at once.
No dialog builds its own panel frame.

**Pressable states, ADR 0097.**
Every element with a click handler gets its hover, press and disabled look from `mkrew_ui::pressable` (`Look`, `press_style`, `Pressable`) or from a widget in `widgets.rs` built on it.
A disabled element is faded (`opacity::DISABLED`), has the default cursor and no hover, no press and no click.
Focus rings stay with `DialogControl`.
```

- [ ] **Step 2: SPEC Settings bullet**

Replace lines 1695-1703 (from "- Settings (Ctrl+," to "opens `settings.toml` and `keymap.toml` in Zed.") with:

```markdown
- Settings (Ctrl+,, palette "Global: Open settings", ADR 0073, ADR 0097): a floating glass panel (`SETTINGS_DIALOG_WIDTH` ×
  `SETTINGS_DIALOG_HEIGHT`, never larger than the window minus a margin). The left navigation (`SETTINGS_NAV_WIDTH`) starts with a
  search box (Ctrl+F focuses it) and lists the pages under three overlines: App (General, Appearance, Notifications, Voice, Features),
  Integrations (Jira, Phone, Machines) and Input (Keyboard, Prompts). The right side has the page title and a close button.
  With a query, the right side lists the matching settings of every page (a muted `Page › Group` breadcrumb, the label with the
  matched characters in accent and semibold, the help line and the real control); a setting matches when `palette::fuzzy` scores its label,
  help, TOML key or page name above zero; a label match always ranks above a help-only match, then by score; pages with no match are dimmed; shortcuts are not searched.
  Esc clears a non-empty query first and closes Settings on an empty one; no match shows "No setting matches “<query>”." with
  "Clear search (Esc)". Each page shows its settings in cards, one per group (`Setting::group`: an overline title and a card
  of rows with faint dividers; label and muted help on the left, the control on the right). Controls: switch (`toggle`), segmented
  choice, `dropdown_trigger` (fonts, themes) and text fields for Text and Number; Number and Text rows are
  written on Enter or when the row loses the keyboard, Esc drops the draft. Keyboard rows show a
  chip, an accent dot when changed and Reset; a click or Enter captures the next keystroke, Esc
  cancels. A refused value or duplicate keystroke shows under its row, never as a toast. A file that
  does not parse shows a banner with "Fix in Zed" and the settings controls do nothing (switches, segments and dropdowns are faded, with no hover and no pointer). The footer
  shows `v<version>`, the links `settings.toml` and `keymap.toml` (they open the files in Zed) and "Changes apply at once."; on the Prompts page the hint stays "Ctrl+N new · Ctrl+S saves · Esc closes".
```

- [ ] **Step 3: HUMAN_CHECKS**

Under "## To verify (non-blocking)" add (newest first):

```markdown
### To verify - 2026-11-21 Polish P1: glass panels, pressable widgets, Settings

- [ ] **Look (dark + light, then Mkrew Light and one palette theme):** open Settings (Ctrl+,), the Ask AI dialog (palette), a Git discard confirm and the in-app confirm prompt; each is a translucent panel with a thin light border, a faint accent tint at the top and a soft shadow; the text stays easy to read over a bright pane and over a dark pane behind it.
- [ ] **Settings:** the navigation lists App, Integrations and Input; typing in the search box lists settings of every page with `Page › Group`; Esc clears the query, Esc again closes; a query with no match shows "No setting matches" and "Clear search (Esc)"; switches, segmented choices and the font and theme dropdowns react to hover and press; the footer shows the version and both file links.
- [ ] **Narrow window:** make the window as small as it goes; Settings stays inside it (width and height are capped).
- [ ] **Reduced motion (Windows):** turn "Animation effects" off in Windows Settings and come back to Mkrew (activating the window rereads it); panels appear at once with no fade. Turn it on again; panels fade in.
- [ ] **Broken settings.toml:** with a file that does not parse, the switches and dropdowns look faded and do nothing.
```

- [ ] **Step 4: Screenshots**

Use the method of the earlier UI runs (see `git log --stat -3 -- docs/screenshots` for the files and the commit that made them).
If it works here, save `docs/screenshots/2026-11-21-polish-settings-dark.png` and `docs/screenshots/2026-11-21-polish-settings-light.png` (Settings on Appearance) and `docs/screenshots/2026-11-21-polish-dialog-{dark,light}.png` (the Ask AI dialog), and name them in the HUMAN_CHECKS item.
If automatic capture is not possible, the HUMAN_CHECKS lines above are the deliverable; do not block.

- [ ] **Step 5: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and tests all green.
If the run exceeds 600 s, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -j 4` separately.

- [ ] **Step 6: Commit**

```bash
git add docs/SPEC.md docs/HUMAN_CHECKS.md docs/screenshots
git -c user.email=tungnv975@gmail.com commit -m "docs: SPEC 14 glass and pressable, Settings layout, polish P1 checks"
```

---

## Self-Review

**Spec coverage (sections 4, 5.1, 5.2, 6 and the P1 row of 9 and 12):**

| Spec item | Task |
| --- | --- |
| 4.1 tokens `glass`, `glass_border`, `glass_tint`, `scrim`, `shadow_float`, sizes | 2 |
| 4.1 per-theme derivation (dark and light differ) | 2 (`with_glass` in `dark()`, `light()`, `from_palette`) |
| 4.2 `floating_panel`, every dialog moved, none builds its own frame | 6, 7 (table of 24 panel builders, one-off grep) |
| 4.3 motion, reduced motion | 3 (detection), 6 (`appear`); spec notes above |
| 4.4 contrast test over `bg` and `text` for every built-in theme, failing theme gets higher alpha | 2 |
| 5.1 rule | 4, 5 (rule), 12 (SPEC); enforcement across the app is P3 |
| 5.2 `Look`, `PressStyle`, `press_style`, `Pressable`, `toggle`, `dropdown_trigger`, `list_row`, existing widgets rebuilt keeping signatures | 4, 5 |
| 6.1 frame, navigation groups, close button | 9 (`NAV`), 10 |
| 6.2 global search, breadcrumb, accented match, dimmed pages, Esc, Ctrl+F, empty state | 9 (model), 10, 11 (rendering) |
| 6.3 `group` field, invariants | 8, 9 (`grouped`), 11 (cards) |
| 6.4 controls through shared widgets | 11 |
| 6.5 footer | 10 |
| 12 ADR 0097, SPEC 14, HUMAN_CHECKS | 1, 12 |

Not in P1 by the spec: SPEC section 13 keys and the protocol section (Plan P2), the skill and `ui-reviewer` checklist (Plan P3).

**Placeholder scan:** no TBD or "similar to" steps; the mechanical edits (the group argument on the `row(` calls, the 5-line frame replacement on 24 panels) are given as an exact rule plus a before and after, a full table, and a one-off grep that lists the sites left.

**Type consistency:** `floating_panel(id, &Theme) -> Stateful<Div>` (Task 6) is the call in Tasks 7 and 10; `appear(&'static str, impl IntoElement)` (Task 6) is called from `modal_shade` and the confirm prompt; `Look` / `PressStyle` / `press_style` / `apply_press` / `Pressable` (Task 4) are the names re-exported in Task 5 and used in Tasks 10 and 11; `segment_in(id, active, enabled, theme)` (Task 5) is the call in Task 11; `Hit.label_ranges`, `search`, `grouped`, `Group`, `NAV`, `Focus::Find`, `find`, `hits() -> Rc<Vec<Hit>>`, `searching`, `heading`, `dimmed_tabs`, `clear_find`, `group_of` (Task 9) are the names Tasks 10 and 11 call; `keyboard_search_shown` (Task 10) is used by the render and the scroll index; `Shell::press_row(index, cx)` (Task 11) is the one row-click handler; `settings_row` takes `(dialog, index, row, divider, hit, typing, theme, cx)` in Tasks 10 and 11 alike; `settings_control` gains `index` in Task 11 and has one caller.

**Review Focus:** items 1-6 each have a named test (Tasks 9, 2, 3); item 7 is covered by Task 4's disabled test plus the manual check; item 8 by the manual check.
