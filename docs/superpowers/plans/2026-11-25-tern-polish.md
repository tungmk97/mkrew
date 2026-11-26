# Tern polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mkrew reads as calmly as Tern across the whole app: neutral ordered grounds, four text tiers, alpha layers, tint-plus-bar selection, a transcript with air, one quiet composer card, and built-in Tern Dark / Tern Light themes.

**Architecture:** Part A changes the token layer in `crates/mkrew-ui/src/theme.rs` (new `layer`, `code_bg`, `composer_fill`, `glass_hi` fields, retuned Mkrew values, Tern constructors, new contrast rules, new shadows) and every later part consumes those tokens.
Part B rebuilds the shared widgets (`pressable.rs`, `shell/widgets.rs`) around layer hovers and the `with_pick` selection helper, then dialogs and Settings.
Parts C-G restyle the transcript, composer, files, chrome and Git/board views on top of A and B; each task is one reviewable commit with its SPEC and ADR 0117 edits.

**Tech Stack:** Rust, gpui (vendored `gpui-pre`), insta snapshots, `scripts/smoke.ps1` screenshots on Windows.

**Spec:** `docs/superpowers/specs/2026-11-25-tern-polish-design.md`

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs`; new values are added there with a doc comment.
- Spacing scale 4 / 8 / 12 / 16 / 24 / 32; fixed component sizes (24, 26, 28, 32, 540, 720, 960) are named constants.
- Both appearances (dark and light) and every built-in theme pass `theme::problems()`; body text ≥ 4.5:1.
- No continuous animation (ADR 0086); hover and press changes are instant.
- `.hover(`, `.active(` and `.cursor_pointer()` appear only in `shell/widgets.rs` and `pressable.rs`; gpui panics when `.hover()` is set twice on one element.
- Sizes that hold text are drawn with `theme::rems_px` so `[ui] font_size` scales them.
- Defaults stay `Mkrew Dark` / `Mkrew Light`; `THEME_NAMES` lists `Tern Dark` after `Mkrew Dark` and `Tern Light` after `Mkrew Light`.
- UI text size stays 13 px at line height 1.5; chat Markdown 14 px at 1.6; Docs flavor 15 px at 1.65; code blocks 13 px at 1.6; Files editor code line height 1.5.
- Composer Send is a key chip (24 high, min 28 wide); Stop is a text button "Stop" in the danger tint; toasts stay bottom right; panes keep their 1 px divider (no tiles).
- ADR 0117 is created in Task 1; later tasks append their bullets to it and edit SPEC §14 / §12A in the same commit as their code.
- Commits: Conventional Commits, explicit `git add <paths>`, `git -c user.email=tungnv975@gmail.com commit`, no co-author, no push. Tests run with `-j 2`.
- No em dash in any text.

## Review Focus

1. A user theme file (palette only, e.g. a very low-contrast or very light palette) must still load: `from_palette` fills every new field and the derived tiers stay readable, or the file shows the "Low contrast" badge instead of panicking. Owner: Task 1 (`from_palette` tier test over all palettes plus an edge palette).
2. Rows that draw nothing (hidden TSP nodes, folded thinking rows) must not add gaps, or the transcript shows stacked blank bands. Owner: Task 10 (`row_gap` / `col_gap` tests).
3. A picked row must be visible in every menu, list and theme, including light themes where `selected` equals `border`. Owner: Task 3 ("menu pick" rule) and Task 5 (`with_pick` uses `accent_bg`, which the rule checks against the glass fill).
4. Narrow panes: the composer bar must drop chips in the same order as today after the size changes, the prompt bubble must shrink with the pane, and the breadcrumb must cut the folder part before the file name. Owners: Task 15 (fit tests), Task 11 (min of 80 % and 540 px), Task 18 (`crumbs` test).
5. A larger `[ui] font_size` (16-20 px) must not clip text in the new fixed-height controls (24 px pills, 26 px rows, 28 px menu rows, 18 px kbd). Owner: every task that adds a height uses `theme::rems_px`; Task 26 takes one screenshot at `font_size = 18`.

---
## Execution order and cross-part notes

Run the parts in order A → B → C → D → E → F → G → H on branch `feat/tern-polish` (worktree `.worktrees/tern-polish`).
Each part was written against master `c775413`; a later part must re-read the functions it edits, because earlier parts change them:

- Part B Task 5 removes the `hover` argument of `quiet_button` (and `composer_tools`); Parts D and E call the new signature.
- Part B makes `PressStyle` hover fills RGBA and adds `pressable_on` for elements with their own fill; Part D's `lift_press` and every pressable with a fill use it.
- Part B owns `with_pick` (added in Task 6 with its first caller); Parts E, F and G use it.
- Part A Task 3 owns `top_highlight(theme, radius)` in `shell/widgets.rs`; the terminal find bar (Part G Task 25) lives in the library crate and draws its own 1 px `glass_hi` line.
- Part A replaces the spec §3.6 rule "`selected` differs from `surface_raised`" with "menu pick" (`accent_bg` differs from the flattened glass fill), because the spec's own values make `selected == surface_raised` in dark; the selection look is `with_pick` (accent tint plus bar), which the rule protects.
- Part A Task 1 changes Mkrew Light `git_modified` / `git_untracked` to `0x8F5410` (4.5:1 on the new `selected`).
- Part C owns `doc::CODE_LINE_HEIGHT` (1.6), `doc::CHAT_LINE_HEIGHT`, `Ctx::new`; Part E adds `Ctx.quoted` through `Ctx::new` and the Docs heading arm.
- Part D keeps the status row left padding at `space::M` (lines up with the card text, which starts 12 px in).
- Part G uses alpha 0.75 for light-theme diff numbers (0.6 reads 2.1:1).

Tasks whose checks need a running omp pane end with a `docs/HUMAN_CHECKS.md` line instead of a screenshot; Task 26 collects them.

---

# Part A: Theme system (Tasks 1-4)

Spec: `docs/superpowers/specs/2026-11-25-tern-polish-design.md` §2, §3, §12, §13 (theme tests). Planned against master c775413.

## Decisions (read first)

1. **`top_highlight` and `floating_panel` belong to Part A (Task 3).** Task 3 adds `pub(crate) fn top_highlight(theme: &Theme, radius: f32) -> gpui::Div` to `shell/widgets.rs` and rewrites `floating_panel` to use it. Part B only calls it (it must not define it). The contract's `pub fn` is `pub(crate)` here, the visibility every other widget in that file has. The caller's parent must be `.relative()`; `floating_panel` already is.
2. **Spec §3.6 "`selected` differs from `surface_raised` and the glass fill" cannot hold.** §3.2 sets Mkrew Dark `selected = surface_raised = 0x232327`, and every derived dark theme has `selected = surface_raised` and glass = `surface_raised` (measured: max channel step 0-2). Menu picks use `with_pick` (Part B: `accent_bg` fill plus bar), so the rule is written on what is drawn: rule `"menu pick"`: `accent_bg` differs from the flattened glass fill by at least 3 in some channel. Measured over all 24 themes: min 4 (Nord, Rosé Pine), Mkrew Dark 12, Tern Dark 13. `selected` itself gets no rule.
3. **`text_subtle` derivation narrowed.** §3.4 says `readable(mix(muted, bg, 0.25), fg, grounds, 4.5)`. With the full `grounds` list (diff tints at 30 %) text_subtle collapses onto text_muted for 15 of 20 palettes (measured). Task 1 uses the rule's own grounds `[bg, surface, surface_raised, composer_fill]`; measured: darker (dark) / lighter (light) than `text_muted` for all 20 palettes, min contrast 4.50.
4. **Mkrew Light `git_modified` / `git_untracked` change `0x975A16 -> 0x8F5410`.** The new light `selected` `0xE7E7E7` gives the old value 4.48:1 on the "diff lines" rule (needs 4.5). New value 4.93:1 on `selected`, 5.89:1 on `bg`. `syntax_number` stays `0x975A16`.
5. **Neutrality test uses blue <= red/green + 4**, not +3: the spec's own `0x232327` is +4.
6. **Derived themes: `accent_bg` and `pane_gap`/`composer_fill` join the `from_palette` grounds** so `accent_text` reads 4.5:1 on `accent_bg` (PlanF asked for that rule; without it 7 light palettes read 4.33-4.48).
7. **Snapshots are accepted in Task 1, 2 (classic field leaves the dump) and 4 (two new snapshots).** One ADR (0117) covers all; the final diff of the two `.snap` files is the review artifact.
8. **Coordination.** `.worktrees/composer-polish` has uncommitted edits to `theme.rs`. Tasks 1, 3, 4 edit `theme.rs`; Task 3 also edits `composer.rs::composer_card_frame` (radius, shadow only; Part D owns the rest of that function) and `widgets.rs::{float_button, floating_panel}`. No task touches `push_runs`, `tsp_image`, `tsp_attachment`, `thumbnail`, `space_render.rs`, `window_mode.rs`.

Derived values (computed with `mix` = per-channel linear blend, rounded half up, exactly what `theme::mix` does):

| Token | Mkrew Dark | Mkrew Light | formula |
|---|---|---|---|
| `accent_bg` | `0x212132` | `0xE4E3F4` | `mix(bg, accent, 0.14)` |
| `selected_ring`, `pane_focus_stripe` | `0x4F4E84` | `0xA8A7E5` | `mix(accent, bg, 0.5)` |
| `terminal_selection_bg` | `0x353556` | `0xC9C8ED` | `fill(bg, accent, 0.30, text)`: the full 30 % passes 4.5:1 (9.99 / 10.51) |
| `warning_bg` / `success_bg` / `danger_bg` | `0x2D241B` / `0x192923` / `0x2E1E1F` | `0xEBE4D9` / `0xDDEAE2` / `0xF3E1DE` | `mix(bg, status hue, 0.14)`; hues dark `E4A452 57C78A EF7B6C`, light `8B570D 23854F C2412F` |
| `layer[0..4]` | `0xFFFFFF12 1A 26 3D` | `0x181A2C0F 17 24 38` | alphas 7/10/15/24 % and 6/9/14/22 % (`0x12 0x1A 0x26 0x3D`; `0x0F 0x17 0x24 0x38`) |
| `glass_hi` | `0xFFFFFF0D` | `0xFFFFFFFF` | white 5 % / 100 % |
| `scrim` (Task 3) | `0x0000008C` | `0xEFEFEC80` | black 55 %; `pane_gap` 50 % |
| `raised_border` (Task 3) | `0x39393D` | `0xE0E0E0` | `layer[1]` over `surface_raised` (step 22 / 20 from the fill) |
| flattened glass over `bg` | `0x222226` | `0xFFFFFF`... | `mix(bg, surface_raised or surface, alpha)` |

Tern overrides (`accent_bg 0x1A2133 / 0xDEE6F9`, `selected_ring 0x354E89 / 0x95ADF5`) are the spec's literals; they differ by 1-2 steps from the formula (`0x95AFF5`), the spec wins.

Callers found by grep (every one is migrated in Task 3, none in theme_file.rs / theme_catalog.rs, which only use `Palette`, `problems`, `ThemeEntry::palette()`):

| Symbol | Sites |
|---|---|
| `Theme::classic()` / `.classic` | `shell/sidebar.rs:567`, `:810`; `theme.rs` (`with_fonts` 1121, `classic_dark/light`, `problems` 2664, tests 2917, 3124) |
| `glass_tint`, `GLASS_TINT_*`, `glass_gradient` | `theme.rs` (fields, consts 628-631, `with_glass`, `problems` 2603, tests 2996/3018), `shell/widgets.rs:1203-1225` (`floating_panel`) |
| `shadow_float` | `shell/widgets.rs:277` (`float_button`), `:1211` (`floating_panel`), `shell/composer.rs:1463` (`composer_card_frame`), `theme.rs` test 3029 |
| `radius::COMPOSER` | `shell/composer.rs:1423`, `:1459`; `theme.rs:45`; SPEC §7.8 line 1312 and the `omp.editor` row 2132 |
| `floating_panel(` callers (inherit the new frame; two are popovers) | 31 sites, listed in Task 3 |
| `theme.scrim` readers (no edit, value changes) | `confirm_prompt.rs:165`, `focus.rs:596`, `tsp_view.rs:1383`, `window_mode.rs:633` |

---

### Task 1: New tokens, Mkrew retune, `from_palette` tiers, new rules, ADR 0117

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`Theme` struct, `Theme::dark`, `Theme::light`, `Theme::shared`, new `layers`, `with_glass`, `from_palette`, `problems`, `PANE_GAP_MIN_CONTRAST` doc, tests)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`, `..._mkrew_light.snap` (re-accept)
- Create: `docs/decisions/0117-tern-polish.md`
- Modify: `docs/SPEC.md` (§14 token table rows around 1837-1880)
- Test: `crates/mkrew-ui/src/theme.rs` (`mod tests`)

**Interfaces:**
- Consumes: existing `mix`, `with_alpha`, `readable`, `fill`, `contrast`, `Palette`, `THEMES`.
- Produces: `Theme.layer: [u32; 4]` (RGBA), `Theme.code_bg: u32`, `Theme.composer_fill: u32`, `Theme.glass_hi: u32` (RGBA); `fn layers(ink: u32, dark: bool) -> [u32; 4]`; `const LAYER_INK_LIGHT: u32 = 0x181A2C`; rules `"meta text"` and `"accent text"` in `problems`; derived themes have `sidebar == pane_gap`. Mkrew tokens as in the §3.2 table. `glass_tint` stays until Task 3.

- [ ] **Step 0: Check the ADR number.** `ls docs/decisions | tail -3; git branch -a; git log --all --oneline -- docs/decisions | head`. If 0117 is taken on any branch use the next free number everywhere in this part.

- [ ] **Step 1: Write the failing tests** (in `mod tests` of `theme.rs`)

Add a helper that replaces the two index-based lookups `THEMES[1].palette()` (lines ~2800 and ~2822), which Task 4 would otherwise break:

```rust
    /// The palette of the built-in theme `name` (a palette-based one).
    fn palette_of(name: &str) -> Palette {
        THEMES
            .iter()
            .find(|e| e.name == name)
            .and_then(ThemeEntry::palette)
            .copied()
            .unwrap_or_else(|| panic!("{name} has a palette"))
    }
```

Replace `let base = *THEMES[1].palette().expect("One Dark has a palette");` with `let base = palette_of("One Dark");` and `let mut palette = *THEMES[1].palette().expect("One Dark has a palette");` with `let mut palette = palette_of("One Dark");`.

New tests:

```rust
    #[test]
    fn a_derived_palette_splits_the_text_tiers_and_keeps_meta_readable() {
        for name in ["One Dark", "Nord", "Gruvbox Dark", "One Light", "Gruvbox Light"] {
            let t = Theme::from_palette(name, &palette_of(name));
            // t1 > t2 > t3 > t4 in prominence: toward the page goes darker (dark) / lighter (light).
            let (a, b, c) = (t.text_muted, t.text_subtle, t.subtle);
            let steps = if t.dark { darker(b, a) && darker(c, b) } else { darker(a, b) && darker(b, c) };
            assert!(steps, "{name}: tiers {a:06X} {b:06X} {c:06X}");
            for ground in [t.bg, t.surface, t.surface_raised, t.composer_fill] {
                assert!(contrast(t.text_subtle, ground) >= 4.5, "{name} on {ground:06X}");
            }
            let ink = if t.dark { palette_of(name).fg } else { LAYER_INK_LIGHT };
            assert!(t.layer.iter().all(|l| l >> 8 == ink), "{name}: layers use the ink");
            if t.dark {
                assert!(darker(t.code_bg, t.bg), "{name}: code is sunken");
                assert!(darker(t.bg, t.composer_fill), "{name}: the composer is lifted");
            }
        }
    }

    #[test]
    fn the_sidebar_is_the_page_in_every_built_in_theme() {
        let mut themes = all_themes();
        let mut edge = palette_of("One Dark");
        edge.bg = 0x000000; // the lighter-gap branch of `from_palette`
        themes.push(Theme::from_palette("Edge", &edge));
        for t in themes {
            assert_eq!(t.sidebar, t.pane_gap, "{}", t.name);
        }
    }

    #[test]
    fn the_mkrew_grounds_are_ordered_and_neutral() {
        let (dark, light) = (Theme::dark(), Theme::light());
        // Dark: the page frame < the panel < the card < the raised fill.
        for pair in [
            [dark.pane_gap, dark.bg],
            [dark.bg, dark.surface],
            [dark.surface, dark.surface_raised],
        ] {
            assert!(darker(pair[0], pair[1]), "{:06X} < {:06X}", pair[0], pair[1]);
        }
        // Light: the frame is darker than the panel, which is darker than the card.
        assert!(darker(light.pane_gap, light.bg) && darker(light.bg, light.surface));
        // Blue is at most 4 over red and green on every ground (Tern's grounds are +3, `232327` +4).
        for t in [&dark, &light] {
            for g in [t.pane_gap, t.bg, t.surface, t.surface_raised, t.chrome, t.well, t.tab_active, t.hover, t.divider, t.border, t.control_border, t.selected, t.code_bg, t.composer_fill] {
                let (r, gr, b) = ((g >> 16) & 0xFF, (g >> 8) & 0xFF, g & 0xFF);
                assert!(b <= r.max(gr) + 4, "{}: {g:06X} leans blue", t.name);
            }
        }
    }
```

In `each_contrast_rule_fires_when_its_token_is_broken`, add to `broken` (before the `workspace colors` entry):

```rust
            (
                "meta text",
                Theme {
                    text_subtle: t.bg,
                    ..t.clone()
                },
            ),
            (
                "accent text",
                Theme {
                    accent_text: t.accent_bg,
                    ..t.clone()
                },
            ),
```

Update `the_raised_border_shows_on_the_raised_surface_in_both_themes` for the new grounds: it asserts `old < 32` for `control_border` (now 16 in dark) and `gap >= 32` (raised_border changes in Task 3). Replace the last assertion block now so Task 3 only has to change the threshold: keep the `gap >= 32` assertion for this task (the value `mix(0x232327, 0xA1A1A1, 0.3)` = 37.8 passes, light `mix(0xF4F4F2, 0x55575F, 0.3)` = 47.7 passes) and the `old < 32` assertion (16 / 25 pass). No edit needed in this task.

- [ ] **Step 2: Run, expect FAIL** (compile error: no field `layer`, `code_bg`, `composer_fill`; no `LAYER_INK_LIGHT`)

`cargo test -p mkrew-ui theme:: -j 2`

- [ ] **Step 3: Implement** (all in `crates/mkrew-ui/src/theme.rs`)

(a) `Theme` struct: add after `glass_tint` (Task 3 deletes that one):

```rust
    /// Highlight line, RGBA: a 1 px line just inside the top edge of raised and floating
    /// panels (`widgets::top_highlight`): white at 5 % (dark) / 100 % (light); set by
    /// [`Theme::with_glass`]. Replaces the accent wash of `glass_tint` (ADR 0117).
    pub glass_hi: u32,
    /// Tern's l1..l4 alpha ladder, RGBA: the ink (white in dark themes, `0x181A2C` in light
    /// ones; the foreground in a derived dark theme) at 7/10/15/24 % (dark) or 6/9/14/22 %
    /// (light). l1 hover, l2 pressed and rings, l3 strong rings and scrollbar thumbs, l4 check
    /// box borders (ADR 0117).
    pub layer: [u32; 4],
    /// The sunken ground of code blocks, darker than the page (dark) or a shade off it (light).
    pub code_bg: u32,
    /// The composer card.
    pub composer_fill: u32,
```

(b) `shared()`: add `glass_hi: 0,`, `layer: [0; 4],`, `code_bg: 0,`, `composer_fill: 0,` (with the other zeros; `layer` is set by every constructor, `glass_hi` by `with_glass`).

(c) After `with_alpha`, add:

```rust
/// Alphas of the l1..l4 layers (see [`Theme::layer`]).
const LAYER_ALPHA_DARK: [f32; 4] = [0.07, 0.10, 0.15, 0.24];
const LAYER_ALPHA_LIGHT: [f32; 4] = [0.06, 0.09, 0.14, 0.22];
/// Ink of the light themes' layers (Tern's `24 26 44`).
const LAYER_INK_LIGHT: u32 = 0x181A2C;

/// The l1..l4 layers of `ink` for a dark or a light appearance.
fn layers(ink: u32, dark: bool) -> [u32; 4] {
    let alphas = if dark { LAYER_ALPHA_DARK } else { LAYER_ALPHA_LIGHT };
    alphas.map(|alpha| with_alpha(ink, alpha))
}
```

(d) `with_glass`: add `glass_hi: with_alpha(0xFFFFFF, if self.dark { 0.05 } else { 1.0 }),` to the returned struct and update the doc line to "Sets `scrim`, `glass`, `glass_border`, `glass_tint` and `glass_hi`".

(e) `Theme::dark()`: replace these lines in the struct literal (same order as the file; fields not listed stay):

```rust
            bg: 0x0F0F12,
            surface: 0x161619,
            surface_raised: 0x232327,
            sidebar: 0x050506,
            chrome: 0x19191B,
            card: 0x161619,
            // Tern's l2 / ink 12 % over the bar.
            tab_active: 0x1E1E1F,
            // l1 over `bg`.
            hover: 0x202023,
            selected: 0x232327,
            // accent halfway to `bg`.
            selected_ring: 0x4F4E84,
            // l2 / l1 over `bg`.
            border: 0x27272A,
            divider: 0x202023,
            text: 0xEDEDED,
            text_dim: 0xC7C7C7,
            text_muted: 0xA1A1A1,
            subtle: 0x55555C,
            // `mix(bg, accent, 0.14)`.
            accent_bg: 0x212132,
            warning_bg: 0x2D241B,
            success_bg: 0x192923,
            danger_bg: 0x2E1E1F,
            // Tern's `--page` darkened to clear `PANE_GAP_MIN_CONTRAST` (1.065:1 under `bg`).
            pane_gap: 0x050506,
            pane_focus_stripe: 0x4F4E84,
            focus_ring: 0xEDEDED,
            terminal_selection_bg: 0x353556,
            status_idle: 0x8A8A92,
            diff_hunk_header: 0x17171A,
            text_subtle: 0x8A8A92,
            terminal_bg: 0x0F0F12,
            control_border: 0x333336,
            row_hover: 0x202023,
            well: 0x17171A,
            folder: 0x8A8A92,
            text_disabled: 0x707077,
            layer: layers(0xFFFFFF, true),
            code_bg: 0x0C0C0D,
            composer_fill: 0x1D1D21,
```

`warning_bg` ... `danger_bg` and `tab_active`, `hover` etc. replace the existing lines of the same name (delete the old comments `// Tern's 10 % white over the bar`, `// Black: 1.07:1 ...`, and the old `// Removed / added lines` comment stays). `on_accent`, `accent`, status hues other than `status_idle`, brand, diff_*, syntax_*, ANSI, `run_pill_*`, `terminal_match_*` keep their values.

(f) `Theme::light()`:

```rust
            bg: 0xFBFBFA,
            surface: 0xFFFFFF,
            surface_raised: 0xF4F4F2,
            sidebar: 0xEFEFEC,
            chrome: 0xE3E3E2,
            card: 0xFFFFFF,
            tab_active: 0xD6D6D4,
            hover: 0xEDEEEE,
            selected: 0xE7E7E7,
            selected_ring: 0xA8A7E5,
            border: 0xE7E7E7,
            divider: 0xEDEEEE,
            text: 0x1B1C20,
            text_dim: 0x383A40,
            text_muted: 0x55575F,
            subtle: 0xB3B5BB,
            accent_bg: 0xE4E3F4,
            warning_bg: 0xEBE4D9,
            success_bg: 0xDDEAE2,
            danger_bg: 0xF3E1DE,
            // Tern's `--page`; 1.11:1 under `terminal_bg`, 1.26:1 from `tab_active`.
            pane_gap: 0xEFEFEC,
            pane_focus_stripe: 0xA8A7E5,
            focus_ring: 0x1B1C20,
            terminal_selection_bg: 0xC9C8ED,
            status_idle: 0x6A6C74,
            // The old 0x975A16 read 4.48:1 on the new `selected` (rule "diff lines").
            git_modified: 0x8F5410,
            git_untracked: 0x8F5410,
            diff_hunk_header: 0xF4F4F2,
            text_subtle: 0x6A6C74,
            terminal_bg: 0xFBFBFA,
            control_border: 0xDBDCDD,
            row_hover: 0xEDEEEE,
            well: 0xF4F4F2,
            folder: 0x6A6C74,
            text_disabled: 0x8F9198,
            layer: layers(LAYER_INK_LIGHT, false),
            code_bg: 0xF8F8F6,
            composer_fill: 0xFFFFFF,
```

(g) `from_palette`: replace the block from `let surface_raised = ...` through `let brand_grounds = ...;` with:

```rust
        let surface_raised = mix(p.surface, p.fg, 0.10);
        let hover = mix(p.bg, p.fg, 0.06);
        // The composer card: lifted a step in dark themes, the panel in light ones.
        let composer_fill = if p.dark { mix(p.bg, p.fg, 0.06) } else { p.surface };
        let accent_bg = tint(p.accent, 0.14);
        // The frame is a step off `bg` (the terminal ground): toward black, so panes look
        // raised; toward `fg` when `bg` is already so dark that black is under the floor.
        let pane_gap = {
            let toward_black = mix(p.bg, 0x000000, if p.dark { 0.70 } else { 0.065 });
            if contrast(toward_black, p.bg) >= PANE_GAP_MIN_CONTRAST {
                toward_black
            } else {
                mix(p.bg, p.fg, 0.10)
            }
        };
        let diff = |hue| (tint(hue, 0.14), tint(hue, 0.24), tint(hue, 0.30));
        let (add_bg, add_gutter, add_word) = diff(p.green);
        let (del_bg, del_gutter, del_word) = diff(p.red);
        let (mod_bg, mod_gutter, _) = diff(p.yellow);
        let grounds = [
            p.bg,
            p.chrome,
            p.surface,
            surface_raised,
            hover,
            // The sidebar is the page (`pane_gap`); the composer card; the pick fill under
            // `accent_text`.
            pane_gap,
            composer_fill,
            accent_bg,
            add_bg,
            del_bg,
            mod_bg,
            add_gutter,
            del_gutter,
            mod_gutter,
            add_word,
            del_word,
        ];
        // Meta text (t3) reads on the grounds of the "meta text" rule only; the diff tints
        // would pull it up onto `text_muted`.
        let meta_grounds = [p.bg, p.surface, surface_raised, composer_fill];
        // The shown tab chip sits on the title bar, which is `pane_gap`.
        let tab_active = mix(pane_gap, p.fg, 0.12);
        // A brand logo sits on the bar and, in the shown tab, on `tab_active`.
        let brand_grounds = [pane_gap, tab_active];
```

then in the returned struct replace/add these fields:

```rust
            sidebar: pane_gap,
            composer_fill,
            code_bg: if p.dark { mix(p.bg, 0x000000, 0.25) } else { mix(p.bg, p.fg, 0.02) },
            layer: layers(if p.dark { p.fg } else { LAYER_INK_LIGHT }, p.dark),
            text_subtle: readable(mix(p.muted, p.bg, 0.25), p.fg, &meta_grounds, 4.5),
            subtle: mix(p.muted, p.bg, 0.45),
            accent_bg,
```

(delete the old `sidebar: p.chrome,`, `text_subtle: text(p.muted),`, `subtle: mix(p.muted, p.bg, 0.35),` and `accent_bg: tint(p.accent, 0.14),` lines; `chrome`, everything else unchanged). `from_palette`'s doc: "the classic sidebar palette" stays until Task 2.

(h) `problems`: after the `body text` loop add

```rust
    // Meta text (timestamps, placeholders, counts, tree rows) reads on every ground it sits on.
    for bg in [t.bg, t.surface, t.card, t.terminal_bg, t.surface_raised, t.composer_fill] {
        check("meta text", t.text_subtle, bg, 4.5);
    }
    // The selected panel icon and the picked row's label sit on `accent_bg`.
    check("accent text", t.accent_text, t.accent_bg, 4.5);
```

(i) `PANE_GAP_MIN_CONTRAST` doc: replace "about 1.07" wording with: "Tern's default tokens measure 1.02 to 1.05 (`--page` vs `--panel`); the 1.07 once quoted came from a themed screenshot. 1.06 keeps the frame visible: Mkrew Dark `050506` under `0F0F12` reads 1.065:1."

(j) `Theme::chrome` doc: "Inputs and segmented controls on the title bar. The sidebar is `pane_gap`." `sidebar` doc: "The workspace sidebar: the page frame (`pane_gap`) in every theme." `subtle` doc: "Graphics and placeholders (t4: chevrons, counts, line numbers); never body text." `text_subtle` doc: "t3: meta text (timestamps, counts, tree rows, tool details) that is still read: at least 4.5:1 on every ground in `problems`." `text_muted`: "t2: names, labels, inactive tabs."

(k) ADR `docs/decisions/0117-tern-polish.md`:

```markdown
# 0117 - Tern polish: neutral grounds, four text tiers, layers and Tern themes

- Date: 2026-11-25
- Status: accepted (amends ADR 0116: composer pill ring -> fill, round Send/Stop -> key chip and Stop text button, Markdown line height 1.5 -> chat 1.6 / docs 1.65; ADR 0105: accent bullet -> `text_subtle`, accent headings -> `text`; ADR 0097: glass tint wash -> `glass_hi` line; ADR 0085: focus ring -> accent plus halo; ADR 0078: built-in list gains Tern Dark / Tern Light, `ThemeEntry` takes a recipe)
- Milestone: none (human request 2026-11-25; spec `docs/superpowers/specs/2026-11-25-tern-polish-design.md`)

## Context

Next to Tern, Mkrew's chat now reads as calmly (ADR 0116) but the rest of the app does not.
Measured from Tern's CSS (six gap reports):

- Mkrew's greys lean blue and are not ordered: `surface #121418` is darker than `bg #15171B`; Tern's `page < panel < card < raise` are neutral and strictly ordered.
- `text_dim #D5D8DD` is almost `text`, and `text_subtle == status_idle`; Tern has four tiers with fixed jobs (t1 `#EDEDED`, t2 `#A1A1A1`, t3 `#8A8A92`, t4 `#55555C`).
- Hover, pressed, divider and ring colors are hand-picked hexes; Tern derives all of them from one alpha ladder (7/10/15/24 % white, 6/9/14/22 % ink).
- Floating panels carry an accent wash (ADR 0097) and one two-layer shadow; Tern has a 1 px top highlight and separate pop / modal / composer shadows.
- The sidebar session list keeps a second, pre-restyle palette (`Theme::classic`) that clashes with any retune.

## Decision

- **Retune Mkrew Dark / Light** (grounds, lines, text tiers; accent family, status, brand, diff hues, syntax, ANSI, workspace colors kept). Values in SPEC §14. `pane_gap` stays at 1.06:1 minimum; its comment is corrected.
- **New tokens:** `layer[4]`, `code_bg`, `composer_fill`, `glass_hi`. `glass_tint`, `GLASS_TINT_*`, `glass_gradient`, `Theme::classic`, `classic_dark/light`, the "sidebar text" rule and `shadow_float` are removed. Shadows are `shadow_pop`, `shadow_modal`, `shadow_composer`. `radius::COMPOSER` is renamed `radius::PANEL`.
- **Floating panels:** fill = `surface_raised` (dark) / `surface` (light) at the contrast-loop alpha, `glass_border = layer[1]`, a 1 px `glass_hi` line inside the top edge (`widgets::top_highlight`), scrim black 55 % (dark) / `pane_gap` 50 % (light).
- **New contrast rules:** "meta text" (`text_subtle` 4.5:1 on bg, surface, card, terminal_bg, surface_raised, composer_fill), "accent text" (`accent_text` on `accent_bg`), "glass lift" (dark glass is lighter than `bg`), "menu pick" (`accent_bg` differs from the glass fill; replaces the unsatisfiable spec wording "`selected` differs from `surface_raised`").
- **Derived themes** (`from_palette`, theme files) get the layers, `code_bg`, `composer_fill`, a real t2/t3/t4 split and `sidebar = pane_gap`.
- **Tern Dark / Tern Light** are built-ins: the retuned Mkrew theme with Tern's blue accent, status, selection and ANSI colors. `ThemeEntry` takes a `Recipe` (`Mkrew`, `Tern`, `Palette(Palette)`); defaults stay Mkrew Dark / Light.
- Components (shared controls, chat, composer, files, chrome, settings, git) follow in the same change; see the spec.

## Consequences

- `the_mkrew_themes_never_change` snapshots are re-accepted under this ADR; two new snapshots pin the Tern themes.
- Theme files keep loading (palette-only); derived themes change look because their tiers and layers are now derived.
- Mkrew Light `git_modified` / `git_untracked` move to `#8F5410` (4.5:1 on the new `selected`).
- No setting migration: `THEME_NAMES` only grows.

## Alternatives

- Only retune Mkrew, no Tern themes: rejected; the user chose both (Q1 = B).
- Keep `classic` for the sidebar: rejected; a second palette defeats the retune.
- Copy Tern's backdrop blur, gradient buttons, glows and animations: GPUI cannot blur, ADR 0086 bans continuous animation; see spec §11.
- Pane tiles with gaps and radius: rejected (Q2).
```

(l) `docs/SPEC.md` §14 token table (lines ~1837-1880): replace these rows exactly:

| Token | Dark | Light |
| --- | --- | --- |
| `bg` (content) | #0F0F12 | #FBFBFA |
| `surface` (panel) | #161619 | #FFFFFF |
| `surface-raised` | #232327 | #F4F4F2 |
| `chrome` | #19191B | #E3E3E2 |
| `sidebar` | `pane-gap` | `pane-gap` |
| `card` (raised) | #161619 | #FFFFFF |
| `hover` / `row-hover` (l1 over `bg`) | #202023 | #EDEEEE |
| `well` | #17171A | #F4F4F2 |
| `selected` | #232327 | #E7E7E7 |
| `border` (l2) | #27272A | #E7E7E7 |
| `control-border` (l3) | #333336 | #DBDCDD |
| `divider` (l1) | #202023 | #EDEEEE |
| `terminal-bg` | #0F0F12 | #FBFBFA |
| `pane-gap` (the frame behind the panes, the sidebar and the title bar) | #050506 | #EFEFEC |
| `tab-active` | #1E1E1F | #D6D6D4 |
| `text` (t1) | #EDEDED | #1B1C20 |
| `text-dim` | #C7C7C7 | #383A40 |
| `text-muted` (t2: names, labels, inactive tabs) | #A1A1A1 | #55575F |
| `text-subtle` (t3: meta) | #8A8A92 | #6A6C74 |
| `text-disabled` | #707077 | #8F9198 |
| `subtle` (t4: graphics, placeholders, chevrons) | #55555C | #B3B5BB |
| `accent-bg` | #212132 | #E4E3F4 |
| `selected-ring` / `pane-focus-stripe` | #4F4E84 | #A8A7E5 |
| `folder` | #8A8A92 | #6A6C74 |
| `warning-bg` / `success-bg` | #2D241B / #192923 | #EBE4D9 / #DDEAE2 |
| `danger-bg` | #2E1E1F | #F3E1DE |
| `status-idle` | #8A8A92 | #6A6C74 |
| `git-modified` / `git-untracked` (light only) | unchanged | #8F5410 |
| `diff-hunk-header` | #17171A | #F4F4F2 |
| `code-bg` (sunken code) | #0C0C0D | #F8F8F6 |
| `composer-fill` | #1D1D21 | #FFFFFF |
| `layer` l1 / l2 / l3 / l4 (ink at 7 / 10 / 15 / 24 % dark, 6 / 9 / 14 / 22 % light) | white | #181A2C |
| `glass-hi` | white 5 % | white 100 % |

and add under the table: "Hover is l1, pressed l2, rings l2, strong rings and scrollbar thumbs l3, check box borders l4 (ADR 0117). Text tiers: t1 content, t2 names and labels, t3 meta, t4 placeholders, line numbers and chevrons (`theme::problems` rule \"meta text\" keeps t3 at 4.5:1)." Also replace the sentence under the table beginning "`pane-gap` is darker than `terminal-bg` ..." keeping its rule text but adding "Tern's own default tokens measure 1.02 to 1.05; Mkrew keeps 1.06".

- [ ] **Step 4: Run, expect PASS** after re-accepting the snapshots:

`cargo test -p mkrew-ui theme:: -j 2` (the three new tests, the two new rule rows and `every_built_in_theme_meets_every_contrast_rule` pass; `the_mkrew_themes_never_change` fails) then `$env:INSTA_UPDATE="always"; cargo test -p mkrew-ui the_mkrew_themes_never_change -j 2` (or `cargo insta accept`), `git diff crates/mkrew-ui/src/snapshots` to confirm only color fields, the four new fields and the removed-nothing; re-run `cargo test -p mkrew-ui theme:: -j 2` green. Then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap docs/decisions/0117-tern-polish.md docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): retune Mkrew themes to Tern grounds and text tiers, add layer/code/composer tokens (ADR 0117)"
```

Visual note: after this task the whole UI is already neutral grey (grounds, text). No screenshot gate here; Task 3 takes the first pair.

---

### Task 2: Remove the `classic` palette

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct field `classic` and doc, `with_fonts`, `Theme::classic`, `dark`, `light`, `classic_dark`, `classic_light`, `shared`, `from_palette` doc, `problems`, tests)
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` (lines ~566-567 and ~809-810)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`, `..._mkrew_light.snap`
- Modify: `docs/SPEC.md` (the two lines before the §14 token table)
- Test: `crates/mkrew-ui/src/theme.rs`

**Interfaces:**
- Consumes: Task 1 tokens.
- Produces: `Theme` without `classic`; `Theme::dark()` / `light()` return `with_glass()` directly; sidebar session list and compact rail use the main tokens.

- [ ] **Step 1: Write the failing test.** Removal is the change; the guard that fails first is the assertion that the sidebar rows read on the tokens they now use. In `each_contrast_rule_fires_when_its_token_is_broken`, delete the `("sidebar text", ...)` entry and its comment `// A built-in Mkrew theme is its own classic palette.`; in `derived_themes_keep_the_fixed_tokens` delete the line `assert_eq!(theme.classic(), base.classic(), "{}", entry.name);`. The first deletion compiles, so write the failing test below, which pins the replacement rule (sidebar text now comes from the main tokens and is already covered by "body text" because `sidebar` is in its grounds):

```rust
    #[test]
    fn body_text_is_checked_on_the_sidebar_itself() {
        let t = Theme::dark();
        let broken = Theme { text_muted: t.sidebar, ..t.clone() };
        let rules: Vec<&str> = problems(&broken).iter().map(|p| p.rule).collect();
        assert!(rules.contains(&"body text"), "{rules:?}");
    }
```

- [ ] **Step 2: Run** `cargo test -p mkrew-ui theme:: -j 2`: passes already (the "sidebar text" rule is only redundant); the real gate for this task is the compile after removal in Step 3. Expect PASS here and a compile FAIL after deleting the field if any caller was missed.

- [ ] **Step 3: Implement.**

`theme.rs`:
- Delete the `classic: Option<std::sync::Arc<Theme>>` field with its 3-line doc (struct, ~1070-1072) and `classic: None,` in `shared()`.
- `with_fonts`: delete the `if let Some(classic) = self.classic.take() { ... }` block; the body is the three assignments then `self`.
- Delete `pub fn classic(&self)` with its doc, and the whole `fn classic_dark(base: &Self)` and `fn classic_light(base: &Self)` (lines ~1359-1504).
- `dark()` and `light()`: the `let theme = Self { ... }.with_glass();` followed by `Self { classic: Some(...), ..theme }` becomes `Self { ... }.with_glass()` returned directly:

```rust
    pub fn dark() -> Self {
        Self {
            bg: 0x0F0F12,
            // ... every field as after Task 1 ...
            ..Self::shared()
        }
        .with_glass()
    }
```
(same for `light`).
- `from_palette` doc: "Status, workspace and QR colors, fonts and sizes stay the Mkrew base's."
- `problems`: delete the 5 lines `// The sidebar's session list keeps Mkrew's classic palette ...` through the `for fg in [classic.text, classic.text_muted] { check("sidebar text", ...) }` loop.

`shell/sidebar.rs` (the sidebar session list and the compact rail now use the passed `theme`):
- Delete `// The session list keeps its pre-restyle look (out of the restyle's scope).` and `let theme = theme.classic();` (~566-567).
- Delete `// The rail is the session list folded: it keeps the pre-restyle look too.` and `let theme = theme.classic();` (~809-810).
(Part F's Task 21 also edits `sidebar.rs`; these are four-line deletions, apply them first.)

`docs/SPEC.md`: delete the two lines "Restyle of 2026-10-24 (ADR 0074). The sidebar session list and compact rail keep the palette from before it (`Theme::classic`)." and replace with "Restyle of 2026-10-24 (ADR 0074), retuned to Tern's grounds and tiers by ADR 0117; the sidebar session list and rail use these tokens too."

- [ ] **Step 4: Run, expect PASS.** `cargo build -p mkrew-ui` then `$env:INSTA_UPDATE="always"; cargo test -p mkrew-ui the_mkrew_themes_never_change -j 2`, `git diff` of the two snapshots shows only the removal of the `classic: Some(...)` block (about 125 lines each; `classic: None` becomes absent), then `cargo test -p mkrew-ui theme:: -j 2` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

Screenshots (sidebar list changes look): `cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins`; `powershell -File scripts/smoke.ps1 start -Theme dark`; `powershell -File scripts/smoke.ps1 mk project list` to confirm the workspace `api`; `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-sidebar-dark.png`; `... stop`; same with `-Theme light` and `tern-polish-sidebar-light.png`. Check session rows hover/selected read, labels are `text_muted`/`text`.

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap docs/SPEC.md docs/screenshots/tern-polish-sidebar-dark.png docs/screenshots/tern-polish-sidebar-light.png
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): remove the classic sidebar palette; the session list uses the main tokens (ADR 0117)"
```

---

### Task 3: Glass, scrim, shadows, `radius::PANEL`, top highlight

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (consts `GLASS_TINT_*`, `radius::COMPOSER`, `Theme.glass_tint` + docs of `scrim`/`glass`/`glass_border`, `shared`, `raised_border`, `glass_over`, `glass_gradient` (delete), `shadow_float` (delete), new `shadow_pop`/`shadow_modal`/`shadow_composer`, `with_glass`, `problems`, new `floating_problems`, tests)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (new `top_highlight`, `floating_panel`, `float_button`)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (`composer_card_frame` radius+shadow, line ~1423 radius; the two `.rounded`/`composer_drops` radius uses only)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (the two popover panels, lines ~3197 and ~3489: `.shadow(theme::shadow_pop(theme))`)
- Modify: `docs/SPEC.md` (lines 1312, 2061-2063, 2132, radius list), `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/theme.rs`

**Interfaces:**
- Consumes: Task 1 `layer`, `glass_hi`; Task 2 (no classic).
- Produces: `theme::shadow_pop(&Theme)`, `theme::shadow_modal(&Theme)`, `theme::shadow_composer(&Theme)` all `-> Vec<gpui::BoxShadow>`; `theme::glass_over(fill: u32, alpha: f32, backdrop: u32) -> u32`; `radius::PANEL = 16.0`; `widgets::top_highlight(theme: &Theme, radius: f32) -> gpui::Div`; `floating_panel` unchanged signature (now modal shadow, top highlight, no gradient); problems rules `"glass lift"`, `"menu pick"`; `Theme.scrim` = black 55 % (dark) / `pane_gap` 50 % (light); `glass_border = layer[1]`.

- [ ] **Step 1: Write the failing tests** in `theme.rs` `mod tests`:

Replace `glass_is_derived_from_surface_and_accent_in_every_theme`, `the_scrim_is_lighter_than_before_and_lighter_in_light_themes`, delete `the_tint_gradient_fades_to_the_same_color_with_no_alpha`, replace `the_float_shadow_has_two_layers_and_half_the_alpha_in_light_themes`:

```rust
    #[test]
    fn glass_is_the_raised_fill_in_dark_and_the_surface_in_light() {
        for theme in all_themes() {
            let fill = if theme.dark { theme.surface_raised } else { theme.surface };
            assert_eq!(theme.glass >> 8, fill, "{}", theme.name);
            assert_eq!(theme.glass_border, theme.layer[1], "{}", theme.name);
            // The base alphas of the spec; the contrast rule may only raise the fill.
            let floor = if theme.dark { 0.96_f32 } else { 0.97_f32 };
            assert!(
                (theme.glass & 0xFF) >= (floor * 255.0).round() as u32,
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn the_scrim_is_black_in_dark_and_a_milky_page_veil_in_light() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_eq!(dark.scrim, with_alpha(0x000000, 0.55));
        assert_eq!(light.scrim, with_alpha(light.pane_gap, 0.50));
    }

    #[test]
    fn the_shadows_differ_by_role_and_a_light_modal_has_a_contact_layer() {
        let (dark, light) = (Theme::dark(), Theme::light());
        let (d, l) = (shadow_modal(&dark), shadow_modal(&light));
        assert_eq!((d.len(), l.len()), (1, 2));
        assert!(d[0].color.a > l[0].color.a, "a dark modal is the deeper one");
        assert_eq!(shadow_pop(&dark), shadow_pop(&light));
        assert_ne!(shadow_composer(&dark), shadow_composer(&light));
        assert_eq!(shadow_pop(&dark).len(), 1);
    }

    #[test]
    fn the_flattened_glass_fill_is_lighter_than_the_page_in_dark_themes() {
        for theme in all_themes().into_iter().filter(|t| t.dark) {
            let alpha = (theme.glass & 0xFF) as f32 / 255.0;
            let flat = glass_over(theme.glass >> 8, alpha, theme.bg);
            assert!(darker(theme.bg, flat), "{}", theme.name);
        }
    }
```

Add to `broken` in `each_contrast_rule_fires_when_its_token_is_broken`:

```rust
            (
                "glass lift",
                Theme {
                    glass: with_alpha(t.bg, 1.0),
                    ..t.clone()
                },
            ),
            (
                "menu pick",
                Theme {
                    accent_bg: t.surface_raised,
                    ..t.clone()
                },
            ),
```

Update `the_raised_border_shows_on_the_raised_surface_in_both_themes`: change `gap >= 32` to `gap >= 16` with message "too faint", and delete the `old` block (`control_border` vs the raised fill is 16 in dark now, the guard no longer holds) plus its comment.

Update `spacing_and_radius_follow_the_scale`: add `assert_eq!(radius::PANEL, 16.0);`.

- [ ] **Step 2: Run, expect FAIL** (`glass_over` arity, `shadow_modal`, `radius::PANEL` missing): `cargo test -p mkrew-ui theme:: -j 2`

- [ ] **Step 3: Implement.**

`theme.rs`:
- `radius`: `/// The composer card and other panels... (ADR 0117)` `pub const PANEL: f32 = 16.0;` replacing `COMPOSER` (keep its existing doc, rename).
- Delete the four `GLASS_TINT_*` consts (lines ~627-631: doc comment too); keep `PANEL_RISE`.
- `Theme`: delete `glass_tint` and its 3 doc lines; `scrim` doc: "The shade behind a modal dialog, RGBA: black at 55 % (dark) or the page frame at 50 % (light: a milky veil), set by [`Theme::with_glass`]."; `glass` doc: "Fill of every floating panel, RGBA: `surface_raised` (dark) / `surface` (light) at 0.96 / 0.97 or more, as much as body text needs to keep 4.5:1 over a light or dark page behind it."; `glass_border` doc: "1 px inner border of a floating panel, RGBA: `layer[1]`." `shared()`: delete `glass_tint: 0,` and fix the comment to "Glass and scrim are derived from the fills by `with_glass`."
- Replace `raised_border`, `glass_over`, delete `glass_gradient` and `shadow_float`:

```rust
/// The 1 px border of a control that sits on `surface_raised` (the Chat composer's chips):
/// `layer[1]` flattened over the raised fill, so it is the same step on every theme.
pub fn raised_border(theme: &Theme) -> u32 {
    mix(
        theme.surface_raised,
        theme.layer[1] >> 8,
        (theme.layer[1] & 0xFF) as f32 / 255.0,
    )
}

/// The color a floating panel shows over `backdrop`: its `fill` at `alpha`.
pub fn glass_over(fill: u32, alpha: f32, backdrop: u32) -> u32 {
    mix(backdrop, fill, alpha)
}

fn black(alpha: f32) -> gpui::Hsla {
    gpui::Hsla::from(gpui::rgba(with_alpha(0x000000, alpha)))
}

/// One shadow layer: `0 y blur spread` black at `alpha` (CSS order).
fn shadow(y: f32, blur: f32, spread: f32, alpha: f32) -> gpui::BoxShadow {
    gpui::BoxShadow::new(gpui::px(0.0), gpui::px(y), black(alpha))
        .blur_radius(gpui::px(blur))
        .spread_radius(gpui::px(spread))
}

/// Menus, tooltips, the peek, toasts and the find bar (ADR 0117): `0 18 50 -12` black 30 %.
pub fn shadow_pop(_theme: &Theme) -> Vec<gpui::BoxShadow> {
    vec![shadow(18.0, 50.0, -12.0, 0.30)]
}

/// Dialogs, the palette and Settings: dark `0 28 72 -16` black 78 %; light `0 24 64 -18`
/// black 32 % plus a `0 2 6 -2` black 6 % contact layer.
pub fn shadow_modal(theme: &Theme) -> Vec<gpui::BoxShadow> {
    if theme.dark {
        vec![shadow(28.0, 72.0, -16.0, 0.78)]
    } else {
        vec![shadow(24.0, 64.0, -18.0, 0.32), shadow(2.0, 6.0, -2.0, 0.06)]
    }
}

/// The composer card: dark `0 12 30 -16` black 60 %; light `0 18 50 -14` black 28 %.
pub fn shadow_composer(theme: &Theme) -> Vec<gpui::BoxShadow> {
    if theme.dark {
        vec![shadow(12.0, 30.0, -16.0, 0.60)]
    } else {
        vec![shadow(18.0, 50.0, -14.0, 0.28)]
    }
}
```
(`spread_radius` is the builder next to `blur_radius` in gpui-pre 0.3.7 `style.rs`; if the builder is named differently, set the `spread_radius` field on the value instead.)
- `with_glass`:

```rust
    /// Sets `scrim`, `glass`, `glass_border` and `glass_hi` from the fills and `layer`. The
    /// fill alpha starts at the spec value and grows by 2 % until `text` keeps 4.5:1 on the
    /// panel over both `bg` and `text` behind it (the worst light and dark pages). Needs
    /// `layer`, `pane_gap`, `surface` and `surface_raised` set.
    fn with_glass(self) -> Self {
        let (fill, base, scrim, hi) = if self.dark {
            (self.surface_raised, 0.96, with_alpha(0x000000, 0.55), 0.05)
        } else {
            (self.surface, 0.97, with_alpha(self.pane_gap, 0.50), 1.0)
        };
        let worst = |alpha: f32| {
            [self.bg, self.text]
                .into_iter()
                .map(|backdrop| contrast(self.text, glass_over(fill, alpha, backdrop)))
                .fold(f32::MAX, f32::min)
        };
        let mut alpha = quantize(base);
        // Ends: at alpha 1.0 the fill is the panel color itself, which text reads on.
        while worst(alpha) < 4.5 && alpha < 1.0 {
            alpha = quantize(alpha + 0.02);
        }
        Self {
            scrim,
            glass: with_alpha(fill, alpha),
            glass_border: self.layer[1],
            glass_hi: with_alpha(0xFFFFFF, hi),
            ..self
        }
    }
```
- `problems`: replace the glass block (`let alpha = ...; let tint = ...; for backdrop ...`) with:

```rust
    // Decoded exactly as `with_glass` quantized it; the page behind is a light or a dark one.
    let alpha = (t.glass & 0xFF) as f32 / 255.0;
    for backdrop in [t.bg, t.text] {
        check("text on glass", t.text, glass_over(t.glass >> 8, alpha, backdrop), 4.5);
    }
```
and change the tail `found` to `found.extend(floating_problems(t));\n    found` (after the brand-logo loop, where `check` is no longer used). Add:

```rust
/// Rules on the lift of a floating panel and of a picked row (ADR 0117).
fn floating_problems(t: &Theme) -> Vec<Problem> {
    let alpha = (t.glass & 0xFF) as f32 / 255.0;
    let flat = glass_over(t.glass >> 8, alpha, t.bg);
    let mut found = Vec::new();
    // A floating panel is lighter than the page it covers in dark themes.
    if t.dark && contrast(flat, 0x000000) <= contrast(t.bg, 0x000000) {
        found.push(Problem {
            rule: "glass lift",
            detail: format!("the panel {flat:06X} is not lighter than the page {:06X}", t.bg),
        });
    }
    // A picked menu row (`accent_bg` fill plus bar) must show on the panel: some channel
    // differs by at least 3. (`selected` equals `surface_raised` in dark themes, so it
    // cannot carry the pick.)
    let step = [16, 8, 0]
        .into_iter()
        .map(|s| (((t.accent_bg >> s) & 0xFF) as i32 - ((flat >> s) & 0xFF) as i32).abs())
        .max()
        .unwrap_or(0);
    if step < 3 {
        found.push(Problem {
            rule: "menu pick",
            detail: format!("accent_bg {:06X} is not apart from the panel {flat:06X}", t.accent_bg),
        });
    }
    found
}
```

`widgets.rs`:

```rust
/// A 1 px line of `glass_hi` just inside the top edge of a raised or floating panel (ADR
/// 0117), inset by the panel's corner `radius` so it does not touch the curve. The parent
/// must be `.relative()`; add it as the first child so the panel's content paints over it.
pub(crate) fn top_highlight(theme: &Theme, radius: f32) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left(px(radius))
        .right(px(radius))
        .h(px(theme::BORDER_WIDTH))
        .bg(gpui::rgba(theme.glass_hi))
}
```
`floating_panel` (doc: "...the glass fill, a 1 px glass border, the `glass_hi` top line, the modal shadow, `radius::LARGE`, and it swallows the mouse. Popovers (`tsp_view`) override the shadow with `shadow_pop`."):

```rust
pub(crate) fn floating_panel(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .relative()
        .rounded(px(radius::LARGE))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.glass_border))
        .bg(gpui::rgba(theme.glass))
        .shadow(theme::shadow_modal(theme))
        .occlude()
        .child(top_highlight(theme, radius::LARGE))
}
```
`float_button`: `.shadow(theme::shadow_pop(theme))`.

`composer.rs`: `radius::COMPOSER` -> `radius::PANEL` (lines ~1423 and ~1459); `.shadow(theme::shadow_float(theme))` -> `.shadow(theme::shadow_composer(theme))` (~1463). (Part D rewrites the rest of `composer_card_frame`; it must keep these two names.)

`tsp_view.rs`: after `.bg(rgb(theme.surface_raised))` in the two popover panels (`tsp-todo-panel-..` ~3201, `tsp-attach-panel-..` ~3492) add `.shadow(theme::shadow_pop(theme))` (a later `.shadow` replaces the modal one; popovers are not modals). All other 29 `floating_panel(` callers (ask-dialog, prompt-picker, confirm-dialog, diagram-dialog, undo, broadcast, git-discard, git-base, turns, history, palette, plan-path, phone-access, ports, run-setup, run-picker, send dialog, settings, slash-picker, task, open-ask, split-picker, add-workspace, worktree, compare) are modal-class and need no edit.

Docs. `docs/SPEC.md`:
- line 2061: replace the sentence beginning "It is a translucent fill (...)" with: "It is a translucent fill (`glass`: `surface_raised` in dark themes and `surface` in light ones at alpha 0.96 / 0.97, raised until body text keeps 4.5:1 over a light or a dark page behind it, so a dark panel is always lighter than the page), a 1 px `glass_border` (`layer[1]`), a 1 px `glass_hi` line inside its top edge (`widgets::top_highlight`; this replaces the accent wash of ADR 0097), `shadow_modal` (dialogs, palette, Settings) or `shadow_pop` (menus, tooltips, peek, toasts, the find bar) and `radius::LARGE`. Shadows: pop `0 18 50 -12` black 30 %; modal dark `0 28 72 -16` black 78 %, light `0 24 64 -18` black 32 % plus `0 2 6 -2` black 6 %; composer dark `0 12 30 -16` black 60 %, light `0 18 50 -14` black 28 %."
- line 2063: "The scrim behind a modal is black at 55 % (dark) or the page frame (`pane-gap`) at 50 % (light), a milky veil."
- lines 1312 and 2132: `radius::COMPOSER` -> `radius::PANEL`, "float shadow" -> "`shadow_composer`".
- radius list (~1876): add "16 (the composer card, `radius::PANEL`)".
`docs/decisions/0117-tern-polish.md`: no edit needed (Decision already lists the shadows and rename).

- [ ] **Step 4: Run, expect PASS.**

`cargo test -p mkrew-ui theme:: -j 2`; accept the snapshots (`glass`, `scrim`, `glass_border` values, `glass_tint` field gone): `$env:INSTA_UPDATE="always"; cargo test -p mkrew-ui the_mkrew_themes_never_change -j 2`, `git diff` shows only those three plus the removal; `cargo build -p mkrew-ui`; `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

Screenshots: build bins as in Task 2; `powershell -File scripts/smoke.ps1 start -Theme dark`; `powershell -File scripts/smoke.ps1 keys -Keys "^+p"` (palette), `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-palette-dark.png`; `keys -Keys "{ESC}"`, `keys -Keys "^,"`, `shot -Out docs/screenshots/tern-polish-settings-dark.png`; `stop`; repeat with `-Theme light` and `-light.png`. Check: panel lighter than the page in dark, a faint top line, no blue wash, milky scrim in light.

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap docs/SPEC.md docs/screenshots/tern-polish-palette-dark.png docs/screenshots/tern-polish-palette-light.png docs/screenshots/tern-polish-settings-dark.png docs/screenshots/tern-polish-settings-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): glass top highlight, pop/modal/composer shadows, milky light scrim, radius::PANEL (ADR 0117)"
```

---

### Task 4: Tern Dark / Tern Light, `Recipe`, `THEME_NAMES`

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (new `tern_dark`, `tern_light`; `Recipe`; `ThemeEntry`, `dark_entry`, `light_entry`, `THEMES`; tests)
- Modify: `crates/mkrew-core/src/settings.rs` (`THEME_NAMES`, test `theme_names_default_to_mkrew_and_accept_any_string` ~881)
- Create: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__tern_dark.snap`, `..._tern_light.snap`
- Modify: `docs/SPEC.md` (§14 theme list ~2038-2040, token table note)
- Test: `crates/mkrew-ui/src/theme.rs`, `crates/mkrew-core/src/settings.rs`

**Interfaces:**
- Consumes: Task 1-3 (`Theme::dark/light`, `with_glass`).
- Produces: `Theme::tern_dark() -> Theme`, `Theme::tern_light() -> Theme`; `pub enum Recipe { Mkrew, Tern, Palette(Palette) }`; `ThemeEntry { name, dark, recipe }` with `build()` and `palette()` (Some only for `Recipe::Palette`); `THEMES` / `THEME_NAMES` of 24 names: "Tern Dark" at index 1, "Tern Light" at index 14 (right after Mkrew Light at 13).

- [ ] **Step 1: Write the failing tests.**

`settings.rs` (`theme_names_default_to_mkrew_and_accept_any_string`): replace the last assertion with

```rust
        assert_eq!(THEME_NAMES[0], DEFAULT_DARK_THEME);
        assert_eq!(THEME_NAMES[1], "Tern Dark");
        assert_eq!(
            THEME_NAMES[13], DEFAULT_LIGHT_THEME,
            "the light ones follow the 13 dark ones"
        );
        assert_eq!(THEME_NAMES[14], "Tern Light");
```

`theme.rs` tests: update `there_are_22_built_in_themes_grouped_dark_then_light` (rename `there_are_24_built_in_themes_grouped_dark_then_light`): `THEMES.len()` 24, dark count 13, `take(13)` all dark, `skip(13)` all light, `names.len()` 24. Update `derived_themes_keep_the_fixed_tokens`: `.filter(|e| e.palette().is_some())` (field `palette` becomes `recipe`). Add:

```rust
    #[test]
    fn the_tern_entries_build_the_tern_constructors() {
        let build = |name: &str| THEMES.iter().find(|e| e.name == name).map(ThemeEntry::build);
        assert_eq!(build("Tern Dark"), Some(Theme::tern_dark()));
        assert_eq!(build("Tern Light"), Some(Theme::tern_light()));
        assert_eq!(build("Mkrew Dark"), Some(Theme::dark()));
        for name in ["Mkrew Dark", "Tern Dark", "Mkrew Light", "Tern Light"] {
            let entry = THEMES.iter().find(|e| e.name == name).unwrap();
            assert!(entry.palette().is_none(), "{name} is written token by token");
        }
        assert!(palette_of("One Dark").dark);
        assert_eq!(THEMES[1].name, "Tern Dark");
        assert_eq!(THEMES[14].name, "Tern Light");
    }

    #[test]
    fn tern_changes_only_the_accent_family_and_status_colors() {
        for (tern, mkrew) in [
            (Theme::tern_dark(), Theme::dark()),
            (Theme::tern_light(), Theme::light()),
        ] {
            let grounds = |t: &Theme| {
                (t.bg, t.surface, t.surface_raised, t.pane_gap, t.text, t.text_muted, t.text_subtle, t.layer, t.code_bg)
            };
            assert_eq!(grounds(&tern), grounds(&mkrew), "{}", tern.name);
            assert_ne!(tern.accent, mkrew.accent, "{}", tern.name);
            assert_ne!(tern.ansi, mkrew.ansi, "{}", tern.name);
            assert_eq!(tern.dark, mkrew.dark);
        }
    }
```

Extend `the_mkrew_themes_never_change` with `insta::assert_snapshot!("tern_dark", golden(&Theme::tern_dark()));` and `insta::assert_snapshot!("tern_light", golden(&Theme::tern_light()));`. Extend `the_mkrew_grounds_are_ordered_and_neutral` (Task 1) loops from `[&dark, &light]` to `[&dark, &light, &Theme::tern_dark(), &Theme::tern_light()]` by binding the two Tern themes to locals first. `all_themes()` already feeds every contrast rule (`every_built_in_theme_meets_every_contrast_rule`, `the_sidebar_is_the_page_in_every_built_in_theme`, glass tests) with the Tern themes.

- [ ] **Step 2: Run, expect FAIL**: `cargo test -p mkrew-core theme_names -j 2` (index mismatch) and `cargo test -p mkrew-ui theme:: -j 2` (no `tern_dark`).

- [ ] **Step 3: Implement.**

`settings.rs`: `THEME_NAMES` becomes (Tern right after each Mkrew):

```rust
pub const THEME_NAMES: &[&str] = &[
    "Mkrew Dark",
    "Tern Dark",
    "One Dark",
    "Dracula",
    "Catppuccin Mocha",
    "Tokyo Night",
    "Gruvbox Dark",
    "Nord",
    "Solarized Dark",
    "GitHub Dark",
    "Rosé Pine",
    "Kanagawa",
    "Everforest Dark",
    "Mkrew Light",
    "Tern Light",
    "One Light",
    "GitHub Light",
    "Catppuccin Latte",
    "Solarized Light",
    "Gruvbox Light",
    "Rosé Pine Dawn",
    "Everforest Light",
    "Ayu Light",
    "Tokyo Night Day",
];
```
(24 names; the doc line above stays.)

`theme.rs`, in `impl Theme` after `light()`:

```rust
    /// Tern Dark (ADR 0117): the retuned Mkrew Dark with Tern's blue accent, status, selection
    /// and ANSI colors.
    pub fn tern_dark() -> Self {
        Self {
            accent: 0x5A8CFF,
            accent_text: 0x9DBCFF,
            accent_bg: 0x1A2133,
            on_accent: 0x08080A,
            selected_ring: 0x354E89,
            pane_focus_stripe: 0x3B6CFF,
            status_working: 0x5A8CFF,
            status_needs_input: 0xF5A524,
            status_done: 0x3ECF8E,
            status_exited: 0xFF6166,
            warning_bg: 0x2F2415,
            success_bg: 0x162A23,
            danger_bg: 0x311A1E,
            run_pill_bg: 0x152621,
            run_pill_border: 0x1F523D,
            run_pill_text: 0x3ECF8E,
            success_text: 0x3ECF8E,
            danger_text: 0xFF6166,
            terminal_selection_bg: 0x27375E,
            terminal_match_bg: 0x5D4621,
            ansi: [
                0x50525A, 0xFF6166, 0x3ECF8E, 0xF5B13D, 0x5B8DFF, 0xB98CFF, 0x35C7E6, 0xC9CAD0,
                0x7A7C85, 0xFF8A8E, 0x6FE3B0, 0xFFD27A, 0x8FB6FF, 0xD2B3FF, 0x7FDCF0, 0xF7F7F8,
            ],
            name: "Tern Dark".into(),
            ..Self::dark()
        }
        .with_glass()
    }

    /// Tern Light (ADR 0117). Tern's `#d98a0b` warn fails 4.5:1, so needs-input keeps Mkrew
    /// Light's `8B570D`; success / danger text keep Mkrew Light's.
    pub fn tern_light() -> Self {
        Self {
            accent: 0x2F63F0,
            accent_text: 0x2448C4,
            accent_bg: 0xDEE6F9,
            on_accent: 0xFFFFFF,
            selected_ring: 0x95ADF5,
            pane_focus_stripe: 0x3B6CFF,
            status_working: 0x2F63F0,
            status_needs_input: 0x8B570D,
            status_done: 0x167A4D,
            status_exited: 0xBF3038,
            warning_bg: 0xF6EBD9,
            success_bg: 0xDEF0E7,
            danger_bg: 0xF8E2E2,
            run_pill_bg: 0xE3F2EA,
            run_pill_border: 0xB4E0CB,
            run_pill_text: 0x167A4D,
            terminal_selection_bg: 0xD6E0FC,
            terminal_match_bg: 0xF8DCAB,
            ansi: [
                0x1F2126, 0xCF3A3F, 0x1D8A5A, 0xA86A00, 0x2F5FE0, 0x8A4FD8, 0x0F7F93, 0x6F727B,
                0x80838C, 0xE5484D, 0x2FAE74, 0xC98A0B, 0x3D6FFF, 0xA86AF4, 0x16A3C4, 0x9EA1A8,
            ],
            name: "Tern Light".into(),
            ..Self::light()
        }
        .with_glass()
    }
```

`ThemeEntry`:

```rust
/// How a built-in theme is made (ADR 0078, ADR 0117).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    /// `Theme::dark()` / `Theme::light()`: written token by token.
    Mkrew,
    /// `Theme::tern_dark()` / `Theme::tern_light()`: Mkrew with Tern's accent family.
    Tern,
    /// Derived from a palette by [`Theme::from_palette`].
    Palette(Palette),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeEntry {
    pub name: &'static str,
    pub dark: bool,
    recipe: Recipe,
}

impl ThemeEntry {
    pub fn build(&self) -> Theme {
        match (&self.recipe, self.dark) {
            (Recipe::Palette(palette), _) => Theme::from_palette(self.name, palette),
            (Recipe::Mkrew, true) => Theme::dark(),
            (Recipe::Mkrew, false) => Theme::light(),
            (Recipe::Tern, true) => Theme::tern_dark(),
            (Recipe::Tern, false) => Theme::tern_light(),
        }
    }

    /// The palette the theme is derived from; `None` for the Mkrew and Tern themes, which
    /// are written token by token.
    pub fn palette(&self) -> Option<&Palette> {
        match &self.recipe {
            Recipe::Palette(palette) => Some(palette),
            Recipe::Mkrew | Recipe::Tern => None,
        }
    }
}

const fn dark_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry { name, dark: true, recipe: Recipe::Palette(palette) }
}

const fn light_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry { name, dark: false, recipe: Recipe::Palette(palette) }
}
```
(`dark_entry` / `light_entry` existing bodies change `palette: Some(palette)` to `recipe: Recipe::Palette(palette)`.)

`THEMES`: the first entry (`name: "Mkrew Dark", dark: true, palette: None`) becomes `recipe: Recipe::Mkrew`, followed by a new entry

```rust
    ThemeEntry {
        name: "Tern Dark",
        dark: true,
        recipe: Recipe::Tern,
    },
```
before "One Dark"; the entry at ~2205 (`Mkrew Light`) becomes `recipe: Recipe::Mkrew` and is followed by

```rust
    ThemeEntry {
        name: "Tern Light",
        dark: false,
        recipe: Recipe::Tern,
    },
```
before "One Light". The `THEMES` doc line stays (it names `THEME_NAMES`). `theme_catalog.rs` needs no edit: it calls `entry.palette()` and `Palette::from_theme(&theme)` for entries without one, which re-derives tiers for Tern as for Mkrew. `settings_dialog.rs` and `palette/tests.rs` counts follow `THEMES.len()` / `THEME_NAMES.len()` (verified by grep, no literal 22 outside `theme.rs` and SPEC).

`docs/SPEC.md` (~2038-2040): "There are 24 built-in themes." `Dark: Mkrew Dark, Tern Dark, One Dark, ... Everforest Dark.` `Light: Mkrew Light, Tern Light, One Light, ... Tokyo Night Day.` Add one line after the list: "Tern Dark / Tern Light (ADR 0117) are Mkrew's grounds and text with Tern's blue accent (`#5A8CFF` / `#2F63F0`), status and ANSI colors." and, under the §14 token table, a short table of the Tern overrides (accent, accent-text, accent-bg, on-accent, selected-ring, pane-focus-stripe, status-working/needs-input/done/exited, warning/success/danger-bg, run-pill, terminal-selection/match-bg) with the values in `tern_dark()` / `tern_light()` above.

- [ ] **Step 4: Run, expect PASS.**

`cargo test -p mkrew-core -j 2 theme_names`; `cargo test -p mkrew-ui theme:: -j 2`: `the_mkrew_themes_never_change` writes two new snapshots (`.snap.new`): `$env:INSTA_UPDATE="always"; cargo test -p mkrew-ui the_mkrew_themes_never_change -j 2` (or `cargo insta accept`); `git status` shows the two new `.snap`. Then `cargo test -p mkrew-ui -j 2 theme_catalog settings_dialog palette::` (counts follow `THEMES.len()`), `cargo clippy -p mkrew-ui -p mkrew-core --all-targets -- -D warnings`.

Screenshots: build bins; `powershell -File scripts/smoke.ps1 start -Theme dark`, then set the slot: append `[ui]` / `dark_theme = "Tern Dark"` to `$env:TEMP\mkrew-sandbox\config\settings.toml` (the file the sandbox's `MKREW_CONFIG_DIR` points at; settings reload within a second; if the key sits in an existing `[ui]` table edit it in place) and `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-tern-dark.png`; `stop`; for light write `light_theme = "Tern Light"`, `start -Theme light`, `shot -Out docs/screenshots/tern-polish-tern-light.png`; also one Settings → Appearance shot (`keys -Keys "^,"`) per theme to see the gallery with 24 cards: `tern-polish-gallery-dark.png`, `tern-polish-gallery-light.png`.

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-core/src/settings.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__tern_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__tern_light.snap docs/SPEC.md docs/screenshots/tern-polish-tern-dark.png docs/screenshots/tern-polish-tern-light.png docs/screenshots/tern-polish-gallery-dark.png docs/screenshots/tern-polish-gallery-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Tern Dark and Tern Light built-in themes, ThemeEntry recipe (ADR 0117)"
```

---

## Notes for other parts

- Part B: `top_highlight` and `floating_panel` exist after Task 3 (this part). Do not define them. `theme.layer[i]` (RGBA) is available from Task 1; `shadow_pop/modal/composer` from Task 3; `radius::PANEL` from Task 3.
- Part D: `composer_card_frame` keeps `radius::PANEL` and `theme::shadow_composer(theme)` as set by Task 3; its card needs `.relative()` before adding `top_highlight(theme, radius::PANEL)`. `composer_fill` replaces `surface_raised` there (Task 1 token).
- Part F: after Task 2 the `sidebar.rs` `classic()` lines are gone; `theme.sidebar == theme.pane_gap` in every theme (tested in Task 1).
- Part G: this part does not add `diff_add_num`, `diff_del_num`, `hunk_bg`, `hunk_current_bg`; Task 22 adds them to the struct, `shared()`, `dark()/light()` (and Tern inherits via `..dark()/..light()`), `from_palette`, and the `golden` snapshots (all four re-accepted there).
- Part C: `CHAT_*`/`CODE_HEAD_HEIGHT` consts are not touched here.
- Every part adding a Theme field must also set it in `shared()` and `from_palette` (or inherit through `..base`) and re-accept the four snapshots (`mkrew_dark`, `mkrew_light`, `tern_dark`, `tern_light`).


---

# Plan part B: shared controls, floating UI, dialogs, Settings (Tasks 5-9)

Spec: `docs/superpowers/specs/2026-11-25-tern-polish-design.md` §4 and §9. Master at c775413.

## Read this first (decisions that apply to every task below)

1. **Order and ownership.** Part A (Tasks 1-4) lands first and gives `Theme.layer: [u32; 4]` (RGBA), `Theme.glass_hi`, `theme::shadow_pop`, `theme::shadow_modal`, `radius::PANEL` (16) and `top_highlight(theme, radius) -> Div` in `shell/widgets.rs`. Part B only calls `top_highlight`. Where Part A already changed `floating_panel` and `float_button` (they used `shadow_float`), Part B keeps that work and only adds what is stated here.
2. **Where `with_pick` lives.** The assignment suggests Task 5, but a `pub(crate)` helper nobody calls fails `clippy -D warnings` (dead code in the bin crate). So `with_pick` is added in **Task 6**, in the same commit as its first callers (menu rows). Later parts (Tasks 10+) can rely on it.
3. **Hover rule, one API.** `PressStyle` gets RGBA fills (`gpui::Rgba`). A look's hover fill is `theme.layer[0]`, pressed is `stack(layer[1], layer[0])`. gpui's `.hover()` style **replaces** the element's own `bg`, so an element that paints a fill at rest must use the new `pressable_on(look, rest_fill, theme)` (the layers are flattened over `rest_fill`, so the fill does not vanish on hover). Elements without a fill use `pressable`.
4. **`quiet_button` loses its 4th argument** (`hover: u32`): every caller passed "one step above the parent". With layer overlays the step is always right, so the argument goes and all 24 callers are edited in Task 5 (clean cutover, no shim). `composer_tools` loses its `hover` parameter for the same reason. Part D plans against the post-B `composer.rs`.
5. **Dialog API (every other part that builds a dialog uses it as written here):**
   - `dialog_frame(panel, head: DialogHead, close: Option<Stateful<Div>>, body: Option<Div>, foot: Option<Div>, theme) -> Stateful<Div>`
   - `DialogHead { icon: Icon, tone: DialogTone, title: SharedString, sub: Option<SharedString> }`, `enum DialogTone { Normal, Danger }`
   - `dialog_body() -> Div`, `dialog_foot(theme) -> Div`, `dialog_close(id, theme) -> Stateful<Div>`, `dialog_text(text, theme) -> Div`, `dialog_hint(text, theme) -> Div`
   - `primary_button(id, label, key: Option<String>, theme) -> Stateful<Div>` (min width 84, optional `kbd`)
   - `error_note(text: impl Into<SharedString>, theme: &Theme) -> Div`
6. **Shared conflict notes.** `composer-polish` (worktree `.worktrees/composer-polish`) has uncommitted edits to `theme.rs`, `shell/tsp_view.rs` (`push_runs`, `tsp_image`, `tsp_attachment`, `thumbnail`), `shell/space_render.rs`, `shell/window_mode.rs`. Part B touches `theme.rs` (token lines only, added next to related ones) and `window_mode.rs` (`render_need_you_chip` in Task 5; `render_open_ask`, `open_split_picker` render in Task 8). None of those are in the functions listed above.
7. **Style rules for every edit:** values from `theme.rs` tokens (new ones are added there with a doc comment), spacing 4/8/12/16/24/32, `.hover(` / `.active(` / `.cursor_pointer()` only in `shell/widgets.rs` and `pressable.rs`, no second `.hover()` on one element, no em dash in any text.
8. **Verification commands** (Windows, low RAM): `cargo test -p mkrew-ui <filter> -j 2`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. Screenshots: `cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins`, then `powershell -File scripts/smoke.ps1 start -Theme dark`, drive with `click -FromX .. -FromY ..` / `keys -Keys ".."`, `shot -Out docs/screenshots/tern-polish-<view>-dark.png`, `stop`; repeat with `-Theme light`.

---

### Task 5: Layer hover and press for every pressable look, scrollbar colors

**Files:**
- Modify: `crates/mkrew-ui/src/pressable.rs` (`PressStyle`, `press_style`, new `press_style_on`, `apply_press`, `Pressable::pressable_on`, new `flatten`, `stack`, `mod tests`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`filled_press`, `pill_button`, `round_button`, `round_stop_button`, `quiet_button`, `danger_button`, `button`)
- Modify (drop the `hover` argument of `quiet_button`): `shell/cards.rs`, `shell/composer.rs`, `shell/git_diff.rs`, `shell/git_menus.rs`, `shell/git_render.rs`, `shell/inspector.rs`, `shell/run.rs`, `shell/theme_gallery.rs`, `shell/workspace_actions.rs`, `shell/worktrees.rs`
- Modify (fills that must flatten): `shell/chat_view.rs` (~1052), `shell/composer.rs` (~1151), `preview.rs` (~1274), `shell/window_mode.rs` (~1008, `render_need_you_chip`)
- Modify: `crates/mkrew-ui/src/scrollbar.rs` (`bar`)
- Test: `crates/mkrew-ui/src/pressable.rs` (`mod tests`)

**Interfaces:**
- Consumes (Part A): `Theme.layer: [u32; 4]`.
- Produces:
  - `pressable::PressStyle { hover_bg: Option<gpui::Rgba>, pressed_bg: Option<gpui::Rgba>, hover_border: Option<gpui::Rgba>, hover_fg: Option<u32>, underline_on_hover: bool, active_opacity: f32, opacity: f32, pointer: bool }`
  - `pressable::press_style(look, enabled, theme) -> PressStyle` (unchanged signature)
  - `pressable::press_style_on(look: Look, enabled: bool, theme: &Theme, rest: Option<u32>) -> PressStyle`
  - `pressable::flatten(layer: u32, base: u32) -> u32` (RGBA over RGB, gives RGB)
  - `pressable::stack(top: u32, below: u32) -> u32` (RGBA over RGBA)
  - `Pressable::pressable_on(self, look: Look, rest: u32, theme: &Theme) -> Self`
  - `widgets::quiet_button(id, label, theme)` (3 arguments)

- [ ] **Step 1: Write the failing tests.** In `pressable.rs`, replace the three tests in `mod tests` with these (keep `fn themes()`):

```rust
    fn alpha(color: u32) -> f32 {
        (color & 0xFF) as f32 / 255.0
    }

    #[test]
    fn fill_looks_hover_to_layer_one_and_press_stacks_layer_two() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised] {
                let s = press_style(look, true, &t);
                assert_eq!(s.hover_bg, Some(gpui::rgba(t.layer[0])), "{look:?}");
                assert_eq!(
                    s.pressed_bg,
                    Some(gpui::rgba(stack(t.layer[1], t.layer[0]))),
                    "{look:?}"
                );
                assert_eq!(s.hover_border, None, "{look:?}");
                assert_eq!(s.active_opacity, 1.0, "a fill look presses with a fill");
                assert_eq!(s.opacity, 1.0);
                assert!(s.pointer);
            }
            assert_eq!(press_style(Look::Row, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Chip, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Icon, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Raised, true, &t).hover_fg, None);
            let text = press_style(Look::Text, true, &t);
            assert_eq!(
                (text.hover_bg, text.pressed_bg, text.hover_fg),
                (None, None, Some(t.text))
            );
            assert!(text.underline_on_hover && text.pointer);
            assert_eq!(text.active_opacity, opacity::ACTIVE, "a link has no fill to press");
        }
    }

    #[test]
    fn a_disabled_press_style_has_no_pointer_hover_or_active() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised, Look::Text] {
                for rest in [None, Some(t.surface_raised)] {
                    let s = press_style_on(look, false, &t, rest);
                    assert_eq!(s.opacity, opacity::DISABLED);
                    assert!(!s.pointer);
                    assert_eq!((s.hover_bg, s.pressed_bg, s.hover_border), (None, None, None));
                    assert_eq!(s.hover_fg, None);
                    assert!(!s.underline_on_hover);
                    assert_eq!(s.active_opacity, 1.0);
                }
            }
        }
    }

    #[test]
    fn on_a_fill_the_layers_are_flattened_so_hover_keeps_the_fill() {
        for t in themes() {
            let s = press_style_on(Look::Row, true, &t, Some(t.surface_raised));
            let hover = flatten(t.layer[0], t.surface_raised);
            assert_eq!(s.hover_bg, Some(gpui::rgb(hover)));
            assert_eq!(s.pressed_bg, Some(gpui::rgb(flatten(t.layer[1], hover))));
            assert_ne!(hover, t.surface_raised, "the step shows on the fill");
        }
    }

    #[test]
    fn stacked_layers_are_denser_than_either_and_keep_their_hue() {
        let white = |a: u8| 0xFFFF_FF00 | u32::from(a);
        let both = stack(white(26), white(18));
        assert_eq!(both >> 8, 0xFFFFFF);
        assert!(alpha(both) > alpha(white(26)) && alpha(both) > alpha(white(18)));
        assert_eq!(stack(white(26), 0), white(26), "over nothing it is unchanged");
        assert_eq!(flatten(white(255), 0x101010), 0xFFFFFF);
        assert_eq!(flatten(white(0), 0x101010), 0x101010);
    }

    /// A hover or press step that equals the ground would be invisible: checked on every ground a
    /// pressable sits on, in every built-in theme.
    #[test]
    fn every_hover_and_press_step_shows_on_every_ground_of_every_theme() {
        for entry in crate::theme::THEMES {
            let t = entry.build();
            for (ground, name) in [
                (t.bg, "bg"),
                (t.surface, "surface"),
                (t.chrome, "chrome"),
                (t.pane_gap, "pane_gap"),
                (t.surface_raised, "surface_raised"),
                (t.well, "well"),
            ] {
                let hover = flatten(t.layer[0], ground);
                let pressed = flatten(t.layer[1], hover);
                assert_ne!(hover, ground, "{} hover is invisible on {name}", t.name);
                assert_ne!(pressed, hover, "{} press is invisible on {name}", t.name);
            }
        }
    }
```

- [ ] **Step 2: Run it, expect FAIL** (compile errors: `press_style_on`, `flatten`, `stack`, `pressed_bg` do not exist): `cargo test -p mkrew-ui pressable -j 2`

- [ ] **Step 3: Implement.**

`pressable.rs`, imports and the `Look` docs and `PressStyle`:

```rust
use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, Rgba, Stateful, Window, rgb};

use crate::theme::{self, Theme, opacity};

/// How a pressable element looks at rest, hovered and pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A list or menu row: hover fill `layer[0]`, label to `text`.
    Row,
    /// A chip or pill: hover fill `layer[0]`, label to `text`.
    Chip,
    /// An icon-only control: hover fill `layer[0]`, icon to `text`.
    Icon,
    /// An element on a raised surface or inside a hovered row (menu rows, toasts, row actions, the
    /// find bar): hover fill `layer[0]` stacked over what is behind it, the label unchanged.
    Raised,
    /// A text link: no fill, hover underline and full text color.
    Text,
}

/// The resolved style of a pressable element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStyle {
    /// Fill while hovered (translucent `layer[0]`, or opaque when the element has its own fill).
    pub hover_bg: Option<Rgba>,
    /// Fill while pressed (`layer[1]` stacked on the hover fill).
    pub pressed_bg: Option<Rgba>,
    /// Border color while hovered (a secondary button's ring); `None` keeps the rest color.
    pub hover_border: Option<Rgba>,
    pub hover_fg: Option<u32>,
    pub underline_on_hover: bool,
    /// Opacity while pressed; 1.0 = unchanged (a fill look presses with `pressed_bg`, a link and a
    /// filled control dim).
    pub active_opacity: f32,
    /// Opacity at rest: 1.0 enabled, `opacity::DISABLED` disabled.
    pub opacity: f32,
    /// The pointing-hand cursor (enabled elements only).
    pub pointer: bool,
}

/// `layer` (`0xRRGGBBAA`) drawn over the opaque `base`, as `0xRRGGBB`.
pub fn flatten(layer: u32, base: u32) -> u32 {
    theme::mix(base, layer >> 8, (layer & 0xFF) as f32 / 255.0)
}

/// `top` drawn over `below` (both `0xRRGGBBAA`): what two translucent layers show together over
/// any ground.
pub fn stack(top: u32, below: u32) -> u32 {
    let alpha = |color: u32| (color & 0xFF) as f32 / 255.0;
    let (at, ab) = (alpha(top), alpha(below));
    let out = at + ab * (1.0 - at);
    if out <= 0.0 {
        return 0;
    }
    let rgb = [24, 16, 8].into_iter().fold(0u32, |acc, shift| {
        let (ct, cb) = (((top >> shift) & 0xFF) as f32, ((below >> shift) & 0xFF) as f32);
        let channel = (ct * at + cb * ab * (1.0 - at)) / out;
        acc | ((channel.round() as u32) << shift)
    });
    rgb | (out * 255.0).round() as u32
}

/// What `look` gives an element in this state, resolved from `theme`: hover and pressed fills
/// (translucent layers), the label color, the underline, the opacities and the pointer. Pure, so
/// it is tested without a window.
pub fn press_style(look: Look, enabled: bool, theme: &Theme) -> PressStyle {
    press_style_on(look, enabled, theme, None)
}

/// [`press_style`] for an element that paints its own fill `rest` (a raised pill, a field): gpui's
/// hover style replaces the element's `bg`, so the layers are flattened over `rest` and the fill
/// keeps showing, one step lighter.
pub fn press_style_on(look: Look, enabled: bool, theme: &Theme, rest: Option<u32>) -> PressStyle {
    if !enabled {
        return PressStyle {
            hover_bg: None,
            pressed_bg: None,
            hover_border: None,
            hover_fg: None,
            underline_on_hover: false,
            active_opacity: 1.0,
            opacity: opacity::DISABLED,
            pointer: false,
        };
    }
    let [l1, l2, ..] = theme.layer;
    let (hover_bg, pressed_bg) = match (look, rest) {
        (Look::Text, _) => (None, None),
        (_, None) => (Some(gpui::rgba(l1)), Some(gpui::rgba(stack(l2, l1)))),
        (_, Some(rest)) => {
            let hover = flatten(l1, rest);
            (Some(rgb(hover)), Some(rgb(flatten(l2, hover))))
        }
    };
    let hover_fg = match look {
        Look::Raised => None,
        Look::Row | Look::Chip | Look::Icon | Look::Text => Some(theme.text),
    };
    PressStyle {
        hover_bg,
        pressed_bg,
        hover_border: None,
        hover_fg,
        underline_on_hover: look == Look::Text,
        active_opacity: if look == Look::Text { opacity::ACTIVE } else { 1.0 },
        opacity: 1.0,
        pointer: true,
    }
}
```

`apply_press` and the trait:

```rust
/// Applies a resolved style: cursor, rest opacity, hover and active. A widget that needs its
/// own hover fill (a round button, a danger button) overrides `hover_bg` first.
pub fn apply_press(el: Stateful<Div>, style: PressStyle) -> Stateful<Div> {
    let PressStyle {
        hover_bg,
        pressed_bg,
        hover_border,
        hover_fg,
        underline_on_hover,
        active_opacity,
        opacity,
        pointer,
    } = style;
    el.when(pointer, |el| el.cursor_pointer())
        .when(opacity < 1.0, |el| el.opacity(opacity))
        .when(
            hover_bg.is_some()
                || hover_border.is_some()
                || hover_fg.is_some()
                || underline_on_hover,
            |el| {
                el.hover(move |mut style| {
                    if let Some(bg) = hover_bg {
                        style = style.bg(bg);
                    }
                    if let Some(border) = hover_border {
                        style = style.border_color(border);
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
        .when(
            pointer && (pressed_bg.is_some() || active_opacity < 1.0),
            |el| {
                el.active(move |mut style| {
                    if let Some(bg) = pressed_bg {
                        style = style.bg(bg);
                    }
                    if active_opacity < 1.0 {
                        style = style.opacity(active_opacity);
                    }
                    style
                })
            },
        )
}
```

In `trait Pressable` add, and in `impl Pressable for Stateful<Div>`:

```rust
    /// [`Pressable::pressable`] for an enabled element that paints its own fill `rest` (the
    /// layers are flattened over it, see [`press_style_on`]).
    fn pressable_on(self, look: Look, rest: u32, theme: &Theme) -> Self;
```
```rust
    fn pressable_on(self, look: Look, rest: u32, theme: &Theme) -> Self {
        apply_press(self, press_style_on(look, true, theme, Some(rest)))
    }
```

`shell/widgets.rs` (types follow the new `PressStyle`):

```rust
fn filled_press(el: gpui::Stateful<gpui::Div>, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_bg: None,
        pressed_bg: None,
        hover_fg: None,
        active_opacity: if enabled { opacity::ACTIVE } else { 1.0 },
        ..press_style(Look::Chip, enabled, theme)
    };
    apply_press(el, style).when(enabled, |el| el.hover(|style| style.opacity(opacity::HOVER)))
}
```
In `pill_button`, the neutral arm becomes
`pill.bg(rgb(theme.surface_raised)).text_color(rgb(theme.text)).pressable_on(Look::Raised, theme.surface_raised, theme)`.
In `round_button` and `round_stop_button` the style literals become
`PressStyle { hover_bg: Some(gpui::rgb(theme.text)), pressed_bg: None, hover_fg: None, active_opacity: opacity::ACTIVE, ..press_style(Look::Chip, enabled, theme) }` (stop button: `gpui::rgb(theme.text_muted)`, `true`).
`quiet_button`:

```rust
/// A ghost [`button`]: muted label, the `Look::Icon` hover (a layer step, the label to `text`).
pub(crate) fn quiet_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .text_color(rgb(theme.text_muted))
        .pressable(Look::Icon, true, theme)
}
```
`button` non-primary arm: `quiet_button(id, label, theme)`. `danger_button` (reworked again in Task 7):

```rust
    let style = PressStyle {
        hover_bg: Some(gpui::rgb(theme.danger_bg)),
        pressed_bg: None,
        hover_fg: None,
        active_opacity: opacity::ACTIVE,
        ..press_style(Look::Row, true, theme)
    };
```

`quiet_button` callers (delete the 4th argument; the compiler lists any missed one):
- `cards.rs`: delete `let edit_hover = theme.border;` (~191) and the argument at ~417, 440, 471, 508, 556-560 (`theme.border,`), 614-618 (`theme.border,`), 781.
- `composer.rs`: remove the `hover: u32` parameter of `composer_tools` (~818), the argument at ~830 and ~857, and the call arguments `theme.surface_raised,` (~1033) and `theme.border,` (~1411).
- `git_diff.rs` ~362, ~417-426, ~744 (also delete `let hover = theme.hover;` at ~706 if it has no other use); `git_menus.rs` ~435; `git_render.rs` ~876-880; `inspector.rs` ~578, ~583-587; `run.rs` ~629, ~728-732, ~1179-1183; `theme_gallery.rs` ~212-216, ~232-236; `workspace_actions.rs` ~1022; `worktrees.rs` ~705, ~726.

Flatten the four fills (hover would otherwise replace them):
- `chat_view.rs` ~1050-1052: `.bg(rgb(theme.surface_raised))` stays, `.pressable(Look::Raised, true, theme)` becomes `.pressable_on(Look::Raised, theme.surface_raised, theme)`.
- `composer.rs` ~1145-1151 (the model pill): same change.
- `preview.rs` ~1269-1274 (the maximize chip): same change.
- `window_mode.rs` ~1005-1008 (`render_need_you_chip`, `bg(warning_bg)`): `.pressable_on(Look::Chip, theme.warning_bg, theme)`.

`scrollbar.rs` `bar`: idle thumb `layer[2]`, held `text_subtle`, update the doc comment:

```rust
/// ... a thumb in `layer[2]` (a step stronger than the hover fill), or `text_subtle` while held. ...
pub fn bar(source: Source, drag: Drag, theme: &Theme) -> Div {
    let (idle, held) = (gpui::rgba(theme.layer[2]), rgb(theme.text_subtle));
```
and `window.paint_quad(fill(shape, color).corner_radii(px(radius::SMALL)))` (drop the `rgb(color)` wrapper; `color` is already an `Rgba`).

- [ ] **Step 4: Run, expect PASS.** `cargo test -p mkrew-ui pressable -j 2`, then `cargo check -p mkrew-ui` (every `quiet_button` caller compiles) and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshot.** Dark and light: hover a sidebar row, a tab, a pill, and press an icon button; the hover step must show on `pane_gap`, `bg` and `surface_raised`. Save `tern-polish-hover-dark.png` and `-light.png`.

- [ ] **Step 6: Commit.** Append to `docs/decisions/0117-tern-polish.md` (section Decision): "Pressables hover to `layer[0]` and press to `layer[0]+layer[1]`; an element with its own fill flattens the layers over it (`pressable_on`); `quiet_button` has no hover argument." In `docs/SPEC.md` replace the sentence that says hover fills are `theme.hover` / `theme.border` (`grep -n "Look::\|hover fill" docs/SPEC.md`).
```
git add crates/mkrew-ui/src/pressable.rs crates/mkrew-ui/src/scrollbar.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/chat_view.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/worktrees.rs crates/mkrew-ui/src/shell/window_mode.rs docs/decisions/0117-tern-polish.md docs/SPEC.md docs/screenshots/tern-polish-hover-dark.png docs/screenshots/tern-polish-hover-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): layer hover and press steps for every pressable look"
```

**Notes for other parts:** `shell/tsp_view.rs` `tsp_attachment` chip (~3450-3459, Part C/D) paints `accent_bg` and uses `pressable_unless(open, Look::Chip, theme)`: after Task 5 its hover replaces the tint with a bare layer. The owner of that function should use `.when(open, |el| el.cursor_pointer()).when(!open, |el| el.pressable_on(Look::Chip, theme.accent_bg, theme))`.

---

### Task 6: Menus, tooltips, toasts, peek, palette row (glass popovers, tint-plus-bar selection)

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (tokens: `MENU_ROW_HEIGHT`, `TOOLTIP_MAX_WIDTH`, `TOOLTIP_MIN_HEIGHT`, `TOAST_MIN_HEIGHT`, `TOAST_ERROR_RING_ALPHA`, `PICK_BAR_WIDTH`, `PICK_BAR_HEIGHT`; `PALETTE_SELECTED_BAR` 3 → 2)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (new `with_pick`, `pop_surface`, `menu_row_frame`; `menu_item`, `menu_panel`, `list_row`, `Tooltip`, `tooltip`, `status_tooltip`)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`item_row`, `swatch_row`, `render_menu`)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (the `menu_item` call at ~1015 only, new signature)
- Modify: `crates/mkrew-ui/src/shell/toasts.rs` (`render_toasts`)
- Modify: `crates/mkrew-ui/src/shell/peek.rs` (`render_peek`)
- No change: `menu.rs`, `context_menu.rs` (pure models: rows, tones, positions; no drawing), `shell/navigation.rs` (its two `list_row` calls pick up the new look)

**Interfaces:**
- Consumes: Task 5 `pressable_on`; Part A `top_highlight`, `theme::shadow_pop`, `Theme.layer`, `Theme.glass`, `Theme.glass_hi`.
- Produces:
  - `widgets::with_pick<E: ParentElement + Styled>(el: E, picked: bool, theme: &Theme) -> E`
  - `widgets::pop_surface(theme: &Theme, radius: f32) -> Div` (glass fill, `layer[1]` ring, `shadow_pop`, `glass_hi` line; the terminal find bar and any other popover use it)
  - `widgets::menu_item(id: SharedString, picked: bool, theme: &Theme) -> Stateful<Div>` (new `picked` argument)
  - `widgets::menu_row_frame(id: SharedString, theme: &Theme) -> Stateful<Div>` (the 28 px row box with no press)

- [ ] **Step 1: No unit test.** All of this is drawing; the rules it relies on (`layer` steps, `selected` differing from glass) are tested in Tasks 2 and 5. Gate: clippy and screenshots below.

- [ ] **Step 2: Tokens** (`theme.rs`, next to `MENU_WIDTH` and the toast constants):

```rust
/// A menu row's minimum height (Tern `.sf-menu-item`); a row with a second line grows.
pub const MENU_ROW_HEIGHT: f32 = 28.0;
/// A tooltip's widest and shortest box.
pub const TOOLTIP_MAX_WIDTH: f32 = 280.0;
pub const TOOLTIP_MIN_HEIGHT: f32 = 24.0;
/// A toast's shortest box, and the alpha of an error toast's `status_exited` ring.
pub const TOAST_MIN_HEIGHT: f32 = 40.0;
pub const TOAST_ERROR_RING_ALPHA: f32 = 0.4;
/// The bar of a picked row (`with_pick`): 2 px wide, 14 px tall, centred at the left edge.
pub const PICK_BAR_WIDTH: f32 = 2.0;
pub const PICK_BAR_HEIGHT: f32 = 14.0;
```
and change `PALETTE_SELECTED_BAR` to `2.0` (update its doc: "the palette row's bar, inset 8 px top and bottom").

- [ ] **Step 3: `widgets.rs` helpers.** Add above `menu_item`:

```rust
/// A picked row (the menu row under the keyboard, a Settings page, a list pick): the `accent_bg`
/// fill and a short accent bar centred on the left edge inside the row. The label color is the
/// caller's (`text`); the keyboard focus ring is separate. A picked row should not also hover:
/// pair it with `pressable_unless(picked, ..)`.
pub(crate) fn with_pick<E: ParentElement + Styled>(el: E, picked: bool, theme: &Theme) -> E {
    if !picked {
        return el;
    }
    el.relative().bg(rgb(theme.accent_bg)).child(
        div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(theme::PICK_BAR_WIDTH))
                    .h(px(theme::PICK_BAR_HEIGHT))
                    .rounded_full()
                    .bg(rgb(theme.accent)),
            ),
    )
}

/// The surface every popover shares (menu, tooltip, toast, peek, find bar): the glass fill, a 1 px
/// `layer[1]` ring, `shadow_pop` and the `glass_hi` line along the top edge. The caller sets
/// size, layout and content; the highlight is the first child, so content paints over it.
pub(crate) fn pop_surface(theme: &Theme, radius: f32) -> gpui::Div {
    div()
        .relative()
        .rounded(px(radius))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[1]))
        .bg(gpui::rgba(theme.glass))
        .shadow(theme::shadow_pop(theme))
        .font_family(theme.ui_font_family.clone())
        .child(top_highlight(theme, radius))
}

/// The box of a menu row: at least `MENU_ROW_HEIGHT` high, `radius::CHIP`, the `space::S` gap.
/// No hover, so a row that cannot act (faded) shares it with [`menu_item`].
pub(crate) fn menu_row_frame(id: SharedString, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .min_h(theme::rems_px(theme::MENU_ROW_HEIGHT))
        .px(px(space::S))
        .rounded(px(radius::CHIP))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
}

/// A full-width row of a popover menu; callers add the label (and a shortcut on the right).
/// `picked` is the keyboard cursor: [`with_pick`] by the caller, no hover fill here.
pub(crate) fn menu_item(id: SharedString, picked: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    with_pick(
        menu_row_frame(id, theme).pressable_unless(picked, Look::Row, theme),
        picked,
        theme,
    )
}

/// The floating panel of a menu drawn in a `deferred(anchored(..))` (...keep the existing doc
/// about `.occlude()`...): `pop_surface` at `radius::LARGE`, padding `space::S`.
pub(crate) fn menu_panel(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    pop_surface(theme, radius::LARGE)
        .id(id)
        .flex()
        .flex_col()
        .p(px(space::S))
        .occlude()
}
```
(keep the long doc comment of the old `menu_panel`; only the first sentence changes.)

`list_row` (palette and the split picker): selected = `layer[0]` fill, bar inset 8 px top and bottom, 2 px wide, `text` label:

```rust
        .when(selected, |el| {
            el.bg(gpui::rgba(theme.layer[0])).child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(space::S))
                    .bottom(px(space::S))
                    .w(px(theme::PALETTE_SELECTED_BAR))
                    .rounded_full()
                    .bg(rgb(theme.accent)),
            )
        })
```
and fix its doc ("`selected` adds the `layer[0]` fill and the 2 px `PALETTE_SELECTED_BAR` accent bar").

Tooltip: it now reads the theme when it opens (`cx.theme()`), so the per-frame closure carries only text and keys (no `PopupStyle` copy, no allocation per hoverable element). Replace the end of the file (`status_tooltip`, `tooltip`, `Tooltip`; keep `PopupStyle` and `PopupStyle::of`, `cards.rs` and `reorder.rs` still use them):

```rust
pub(crate) fn status_tooltip(
    _theme: &Theme,
    state: SessionState,
    source: StateSource,
    changed_at: i64,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| {
        let text = status::hover(state, source, changed_at, status::now_ms()).into();
        cx.new(|_| Tooltip { text, keys: SharedString::default() }).into()
    }
}

/// Builds a tooltip showing `text` and, if any, its keyboard shortcut.
pub(crate) fn tooltip(
    _theme: &Theme,
    text: &str,
    keys: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = SharedString::from(text.to_owned());
    let keys = keys.into();
    move |_, cx| {
        let (text, keys) = (text.clone(), keys.clone());
        cx.new(|_| Tooltip { text, keys }).into()
    }
}

pub(crate) struct Tooltip {
    pub(crate) text: SharedString,
    pub(crate) keys: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        pop_surface(theme, radius::MEDIUM)
            .flex()
            .items_center()
            .gap(px(space::S))
            .max_w(px(theme::TOOLTIP_MAX_WIDTH))
            .min_h(theme::rems_px(theme::TOOLTIP_MIN_HEIGHT))
            .px(px(space::S))
            .py(px(space::XS))
            .text_size(font_size::META)
            .text_color(rgb(theme.text))
            .child(self.text.clone())
            .when(!self.keys.is_empty(), |el| {
                el.child(kbd(self.keys.to_string(), theme, false))
            })
    }
}
```
The `theme` parameter of `tooltip` and `status_tooltip` stays in the signature (about 400 call sites): it is `_theme` because the tooltip reads the live theme when it opens, so a theme switch while a tooltip is open cannot leave it stale. Add that sentence to the doc.

- [ ] **Step 4: Menu rendering** (`menu_render.rs`):
  - `item_row`: replace the `let frame = if enabled {..} else {..}` block with
    ```rust
    let frame = if enabled {
        menu_item(row_id.clone(), selected, theme).group(row_id)
    } else {
        menu_row_frame(row_id, theme).opacity(opacity::DISABLED)
    };
    frame
        .when(slot, |el| { /* unchanged ✓ column */ })
    ```
    i.e. delete the line `.when(selected, |el| el.bg(rgb(theme.selected)))`. For a disabled row that is selected, `with_pick` is not applied (a row that cannot act shows no pick); the keyboard skips disabled rows already.
  - `item.keys` div: add `.text_color(rgb(theme.text_subtle))`; `item.detail` div: same.
  - `swatch_row`: replace `div().flex().flex_wrap().gap(..).p(..).rounded(px(radius::SMALL)).when(selected, |el| el.bg(rgb(theme.selected)))` with `with_pick(div().flex().flex_wrap().gap(px(space::S)).p(px(space::S)).rounded(px(radius::CHIP)), selected, theme)` and keep the `.when(!enabled, ..)` and `.children(..)` after it.
  - `render_menu`: separator `.my(px(space::XS)).mx(px(space::XS)).bg(rgb(theme.divider))`; heading `.px(px(space::S)).pt(px(space::S)).pb(px(space::XS)).text_color(rgb(theme.text_subtle))` (keep META and MEDIUM); note `.text_color(rgb(theme.text_subtle))`.
  - `settings.rs` ~1015: `menu_item(SharedString::from(format!("list-choice-{i}")), i == selected, theme)` and delete the following `.when(i == selected, |el| el.bg(rgb(theme.surface_raised)))`.

- [ ] **Step 5: Toasts** (`toasts.rs`, replace the `menu_panel(("toast", id), theme)` chain, keep the stack wrapper bottom right):

```rust
            let group = SharedString::from(format!("toast-{id}"));
            pop_surface(theme, radius::LARGE)
                .id(("toast", id))
                .group(group.clone())
                .occlude()
                .w(theme::rems_px(theme::TOAST_WIDTH))
                .min_h(theme::rems_px(theme::TOAST_MIN_HEIGHT))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(space::S))
                .pl(px(space::M))
                .pr(px(space::S))
                .py(px(space::S))
                // An error is ringed in its own color, not only a red icon.
                .when(is_error, |el| {
                    el.border_color(gpui::rgba(theme::with_alpha(
                        theme.status_exited,
                        theme::TOAST_ERROR_RING_ALPHA,
                    )))
                })
                // Hovering pauses the countdown; leaving restarts it (toast.rs).
                .on_hover(cx.listener(move |shell, hovered: &bool, _, cx| {
                    shell.toasts.set_hovered(id, *hovered, Instant::now());
                    cx.notify();
                }))
                .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
                .child(body)
                .child(
                    div()
                        .id(("toast-close", id))
                        .flex_none()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        // Shown while the toast is hovered.
                        .opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                        .pressable(Look::Raised, true, theme)
                        .tooltip(tooltip(theme, "Dismiss", ""))
                        .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_subtle))
                        .on_press(true, cx.listener(move |shell, _, _, cx| {
                            shell.toasts.dismiss(id);
                            cx.notify();
                        })),
                )
```
Info toasts keep `Icon::Info` in `accent_text`. Update the module doc line ("restyled, still bottom right").

- [ ] **Step 6: Peek** (`peek.rs`). The panel becomes `pop_surface` and the hint moves to a foot band. Replace the `let panel = div().id("peek")...` header (lines ~590-602) and its children structure with:

```rust
        let body = div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .child(/* header row: status mark, name, state text: the existing `.child(div().flex().items_center()...)` moved here unchanged */)
            .child(/* the screen box: the existing `.child(div().p(px(space::S))...child(screen))` moved here unchanged */)
            .child(reply)
            .children(peek.error.clone().map(|error| {
                div().text_size(font_size::META).text_color(rgb(theme.status_exited)).child(error)
            }));
        let foot = div()
            .px(px(space::M))
            .py(px(space::S))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_subtle))
            .child(hint);
        let panel = pop_surface(theme, radius::LARGE)
            .id("peek")
            .w(px(theme::PEEK_WIDTH))
            .flex()
            .flex_col()
            // Only once it has the keyboard: a tracked element takes focus on any mouse down,
            // so a click on a hover peek would steal the keyboard before `take_peek_focus`.
            .when(peek.focused, |el| el.track_focus(&self.peek_focus))
            .on_key_down(cx.listener(Self::peek_key_down))
            .when(peek.focused, |el| {
                el.child(self.typing(&self.peek_focus, Self::peek_key_down, cx))
            })
            .on_mouse_down_out(cx.listener(|shell, _, window, cx| {
                shell.close_peek(window, cx);
            }))
            .child(body)
            .child(foot);
```
(`pop_surface` is already `relative()`, so the `.relative()` before the typing child is gone; `peek-area` and the `deferred(anchored())` tail are unchanged. Move the three existing blocks by cut and paste, keeping their content.)

- [ ] **Step 7: Check `shadow_float` is gone.** `rg "shadow_float|\.shadow_md\(\)" crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/peek.rs` must print nothing. If `float_button` still calls `theme::shadow_float(theme)` (Part A did not reach it), change it to `.shadow(theme::shadow_pop(theme))`. (`shell/reorder.rs` and `shell/voice_input.rs` keep `shadow_md()`: drag preview and the voice pill, not in this part.)

- [ ] **Step 8: Verify.** `cargo check -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. Screenshots dark and light: the Run menu or a tab right-click menu (hover and keyboard-selected row show tint plus bar), a tooltip with keys (hover the sidebar +), the palette (`keys -Keys "^+p"`, then `{DOWN}`), a toast (use the palette "Global: Copy last error" or any copy action), `tern-polish-menu-*.png`, `tern-polish-palette-*.png`, `tern-polish-toast-*.png`. The peek popover needs a session that needs input, which the sandbox cannot make: Task 26 adds it to `docs/HUMAN_CHECKS.md` (dark and light).

- [ ] **Step 9: Commit.** Append to ADR 0117: "Menus, tooltips, toasts and peek share `pop_surface` (glass, `layer[1]` ring, `shadow_pop`, `glass_hi`); picked rows are `accent_bg` plus a 2x14 bar (`with_pick`); the palette row is a `layer[0]` fill with an inset 2 px bar." Update `docs/SPEC.md` line about the palette row (`grep -n "PALETTE_SELECTED_BAR" docs/SPEC.md`, ~1973) and the menu and toast sentences (`grep -n "menu_panel\|toast" docs/SPEC.md`).
```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/peek.rs docs/decisions/0117-tern-polish.md docs/SPEC.md docs/screenshots/tern-polish-menu-dark.png docs/screenshots/tern-polish-menu-light.png docs/screenshots/tern-polish-palette-dark.png docs/screenshots/tern-polish-palette-light.png docs/screenshots/tern-polish-toast-dark.png docs/screenshots/tern-polish-toast-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): glass menus, tooltips, toasts and peek with tint-plus-bar selection"
```

---

### Task 7: Buttons, focus halo, inputs, switch, segmented, kbd, chips, empty state, `CONTROL_HEIGHT` 32

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (tokens below; `CONTROL_HEIGHT` 30 → 32, `SWITCH_WIDTH` 36, `SWITCH_HEIGHT` 22, `SEGMENT_PADDING` 2, `KBD_BOTTOM_BORDER` 1)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`button`, `button_frame`, `secondary_button`, `danger_button`, `DialogControl` + impl, `kbd`, `icon_frame`, `icon_button_in`, `segmented`, `segment_in`, `tinted_chip`, `status_pill`, `search_box`, `field_frame`, `dropdown_trigger`, `toggle`, `empty_state`; new `focus_halo`, `danger_ring`, `small_icon_button`)
- Modify: `crates/mkrew-ui/src/field.rs` (`empty_field` placeholder color)
- Modify: `crates/mkrew-ui/src/shell/ask_dialog.rs` (~225) and `shell/task_dialog.rs` (~543): multi-line field keeps the old vertical padding
- Mechanical rewrite over `crates/mkrew-ui/src` (`sg`, below): `focus_stop_filled` on every accent-filled button, `focusable_bordered` on every danger button
- Test: none new (pure styling; the layer rules are tested in Task 5, contrast in Part A). Gate is clippy plus screenshots.

**Interfaces:**
- Consumes: Task 5 `press_style_on`, `PressStyle.hover_border`, `pressable_on`; Part A `top_highlight`.
- Produces:
  - `widgets::focus_halo(accent: u32) -> Vec<gpui::BoxShadow>`
  - `widgets::danger_ring(theme: &Theme, mix: f32) -> u32` (`theme::mix(danger_bg, status_exited, mix)`)
  - `DialogControl::focus_stop_filled(self, theme) -> Self` (1 px `focus_ring` border, no halo: for an accent-filled button)
  - `widgets::small_icon_button(id, glyph, theme) -> Stateful<Div>` (`ICON_BUTTON_SIZE` square, icon 14, `text_subtle`)
  - `widgets::empty_state(theme, glyph, title, sentence, detail) -> Div` (same signature, new look)

- [ ] **Step 1: Tokens** (`theme.rs`):

```rust
pub const CONTROL_HEIGHT: f32 = 32.0;      // was 30
pub const SWITCH_WIDTH: f32 = 36.0;        // was 32
pub const SWITCH_HEIGHT: f32 = 22.0;       // was 18; the knob is height - 2 * inset = 18
pub const SEGMENT_PADDING: f32 = 2.0;      // was 3
pub const KBD_BOTTOM_BORDER: f32 = 1.0;    // was 2
/// A key hint's fixed height and least width.
pub const KBD_HEIGHT: f32 = 18.0;
pub const KBD_MIN_WIDTH: f32 = 18.0;
/// The keyboard focus halo: a ring this wide, in the accent at this alpha, outside the 1 px border.
pub const FOCUS_HALO_SPREAD: f32 = 3.0;
pub const FOCUS_HALO_ALPHA: f32 = 0.18;
/// The knob of a switch is white in every theme (a shadow lifts it).
pub const SWITCH_KNOB: u32 = 0xFFFFFF;
/// The danger ring: `danger_bg` mixed this far toward `status_exited`, at rest and hovered.
pub const DANGER_RING_MIX: f32 = 0.3;
pub const DANGER_RING_HOVER_MIX: f32 = 0.6;
/// A badge's ring is its tone at this alpha.
pub const CHIP_RING_ALPHA: f32 = 0.22;
```
and in `font_size`: `/// An empty state's title (Tern 14 px).  pub const EMPTY_TITLE: Rems = rems(14.0 / 13.0);`. Update the doc comments of the changed constants (the "redesign's controls" line says 30). `ICON_BUTTON_SIZE` stays 26 (title bar and dialog close).

- [ ] **Step 2: Buttons and focus** (`widgets.rs`).

```rust
/// The keyboard focus halo (ADR 0085, amended by ADR 0117): a ring of the accent at
/// `FOCUS_HALO_ALPHA` just outside the box, drawn with the 1 px accent border.
// ponytail: one small Vec per focus stop per frame, because gpui's `shadow` takes a Vec; cache it
// on the theme if a profile ever shows it.
pub(crate) fn focus_halo(accent: u32) -> Vec<gpui::BoxShadow> {
    vec![
        gpui::BoxShadow::new(
            px(0.0),
            px(0.0),
            gpui::Hsla::from(gpui::rgba(theme::with_alpha(accent, theme::FOCUS_HALO_ALPHA))),
        )
        .spread_radius(px(theme::FOCUS_HALO_SPREAD)),
    ]
}

/// The ring of a danger button, an error note and a destructive dialog's icon tile.
pub(crate) fn danger_ring(theme: &Theme, mix: f32) -> u32 {
    theme::mix(theme.danger_bg, theme.status_exited, mix)
}
```
`DialogControl`: add `fn focus_stop_filled(self, theme: &Theme) -> Self;` with the doc "[`DialogControl::focus_stop`] for a button filled with the accent: the ring is `focus_ring` (the text color), which reads on the fill; no halo". Impl:

```rust
    fn focusable_bordered(self, theme: &Theme) -> Self {
        let accent = theme.accent;
        self.tab_index(0).focus_visible(move |style| {
            style.border_color(rgb(accent)).shadow(focus_halo(accent))
        })
    }

    fn focus_stop_filled(self, theme: &Theme) -> Self {
        let ring = rgb(theme.focus_ring);
        self.border(px(theme::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .tab_index(0)
            .focus_visible(move |style| style.border_color(ring))
    }
```
(`focus_stop` is unchanged: transparent border, then `focusable_bordered`.) Update the trait docs ("shows the accent border and halo").

Buttons:

```rust
/// A small text button: primary (accent fill, `on_accent` text, MEDIUM, `glass_hi` line) or ghost.
pub(crate) fn button(id: &'static str, label: impl IntoElement, theme: &Theme, primary: bool) -> gpui::Stateful<gpui::Div> {
    if primary {
        filled_press(
            button_frame(id, label)
                .relative()
                .bg(rgb(theme.accent))
                .text_color(rgb(button_fg(theme, true)))
                .font_weight(FontWeight(weight::MEDIUM))
                .child(top_highlight(theme, radius::MEDIUM)),
            true,
            theme,
        )
    } else {
        quiet_button(id, label, theme)
    }
}

/// A secondary button: `surface_raised` fill, 1 px `layer[1]` ring, `glass_hi` line, `text` label
/// MEDIUM; hover steps the fill up and the ring to `layer[2]`.
pub(crate) fn secondary_button(id: &'static str, label: impl IntoElement, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_border: Some(gpui::rgba(theme.layer[2])),
        ..press_style_on(Look::Row, true, theme, Some(theme.surface_raised))
    };
    apply_press(
        button_frame(id, label)
            .relative()
            .bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .text_color(rgb(theme.text))
            .font_weight(FontWeight(weight::MEDIUM))
            .child(top_highlight(theme, radius::MEDIUM)),
        style,
    )
}

/// A ghost [`button`] that discards work (Discard, Undo's confirm): `danger_bg` fill with a 1 px
/// ring and `danger_text`; hover lifts the fill and strengthens the ring. Give it
/// `focusable_bordered`, not `focus_stop` (which would clear its ring at rest).
pub(crate) fn danger_button(id: &'static str, label: impl IntoElement, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let style = PressStyle {
        hover_border: Some(rgb(danger_ring(theme, theme::DANGER_RING_HOVER_MIX))),
        hover_fg: None,
        ..press_style_on(Look::Row, true, theme, Some(theme.danger_bg))
    };
    apply_press(
        button_frame(id, label)
            .bg(rgb(theme.danger_bg))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(danger_ring(theme, theme::DANGER_RING_MIX)))
            .text_color(rgb(theme.danger_text)),
        style,
    )
}
```
`disabled_button` primary arm: `.font_weight(FontWeight(weight::MEDIUM))` (was SEMIBOLD). `button_frame` height is already `CONTROL_HEIGHT` (now 32).

Mechanical rewrites (run from the repo root, then `git diff --stat` and `cargo check -p mkrew-ui`):

```
sg --lang rust -p 'button($A, $B, $C, true).focus_stop($D)' -r 'button($A, $B, $C, true).focus_stop_filled($D)' -U crates/mkrew-ui/src
sg --lang rust -p 'danger_button($A, $B, $C).focus_stop($D)' -r 'danger_button($A, $B, $C).focusable_bordered($D)' -U crates/mkrew-ui/src
```
Expected: the 23 primary sites (`rg -n "true\)\s*\.focus_stop\(" crates/mkrew-ui/src` must print nothing afterwards, ignoring `focus_stop_filled`) and every `danger_button(..)` call that had `focus_stop`. Sites where the button is bound to a variable first are not matched: grep `focus_stop` once more and fix those by hand (`prompts_render.rs` ~69 and `machines.rs` ~290 are `if empty { button(.., true).focus_stop(theme) }` and ARE matched).

Icon buttons (`icon_frame` takes the size and icon size; `icon_button_in` keeps `CONTROL_HEIGHT` and `ICON_SIZE`):

```rust
fn icon_frame(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    (box_size, icon_size): (f32, f32),
    color: u32,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(box_size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MEDIUM))
        .text_color(rgb(theme.text_muted))
        .pressable(Look::Icon, enabled, theme)
        .child(icon_svg(glyph, icon_size, color))
}
```
`icon_button_in` and `disabled_icon_button` pass `(theme::CONTROL_HEIGHT, theme::ICON_SIZE)`. New:

```rust
/// A 26 px icon button with a 14 px icon in `text_subtle` (a dialog's close).
pub(crate) fn small_icon_button(id: impl Into<gpui::ElementId>, glyph: Icon, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    icon_frame(id, glyph, (theme::ICON_BUTTON_SIZE, theme::ICON_SIZE_SMALL), theme.text_subtle, true, theme)
}
```
(`small_icon_button` is used in Task 8; to keep this task warning-free the first caller must exist: use it now for the Settings header close button in `shell/settings.rs` (`icon_button("settings-close", Icon::X, theme)` becomes `small_icon_button("settings-close", Icon::X, theme)`); Task 9 removes that header.)

- [ ] **Step 3: Inputs.**

```rust
/// A field's frame: `layer[0]` fill, 1 px `layer[1]` ring, `INPUT_HEIGHT` at least; while `focused`
/// the accent border and the focus halo. An I-beam pointer. A multi-line field adds its own
/// vertical padding and height.
fn field_frame(id: &'static str, focused: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .relative()
        .px(px(space::S))
        .py(px(space::XS))
        .min_h(theme::rems_px(theme::INPUT_HEIGHT))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(if focused { rgb(theme.accent) } else { gpui::rgba(theme.layer[1]) })
        .bg(gpui::rgba(theme.layer[0]))
        .when(focused, |el| el.shadow(focus_halo(theme.accent)))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .cursor(CursorStyle::IBeam)
}
```
`search_box`: `.px(px(space::M))`, `.border_color(if focused {rgb(accent)} else {rgba(layer[1])})`, `.bg(gpui::rgba(theme.layer[0]))`, `.when(focused, |el| el.shadow(focus_halo(theme.accent)))` (drop the `chrome` fill and `border`). `dropdown_trigger`: fill `gpui::rgba(theme.layer[0])`, border `if open { accent } else { layer[1] }`, `.when(open, |el| el.shadow(focus_halo(theme.accent)))`, and replace `.pressable(Look::Row, enabled, theme)` with

```rust
        .map(|el| {
            let style = PressStyle {
                hover_bg: None,
                pressed_bg: None,
                hover_border: (!open).then(|| gpui::rgba(theme.layer[2])),
                ..press_style(Look::Row, enabled, theme)
            };
            apply_press(el, style)
        })
```
`field.rs` `empty_field`: placeholder `theme.text_subtle` becomes `theme.subtle` (t4), doc "the faint `placeholder`". `ask_dialog.rs` ~225 and `task_dialog.rs` ~543: after `.h(px(theme::PROMPT_FIELD_HEIGHT))` add `.py(px(space::S))`.

- [ ] **Step 4: Switch, segmented, kbd, chips.**

```rust
pub(crate) fn toggle(id: impl Into<gpui::ElementId>, on: bool, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // 22 px = 1 px ring + 1 px gap + 18 px knob + 1 px + 1 px.
    let knob = px(theme::SWITCH_HEIGHT - 2.0 * theme::SWITCH_INSET);
    filled_press(
        div()
            .id(id)
            .flex_none()
            .w(px(theme::SWITCH_WIDTH))
            .h(px(theme::SWITCH_HEIGHT))
            .p(px(theme::SWITCH_INSET - theme::BORDER_WIDTH))
            .rounded_full()
            .flex()
            .when(on, |el| el.justify_end())
            .border(px(theme::BORDER_WIDTH))
            .border_color(if on { gpui::transparent_black() } else { gpui::rgba(theme.layer[0]) })
            .bg(if on { rgb(theme.accent) } else { gpui::rgba(theme.layer[2]) })
            .child(
                div()
                    .size(knob)
                    .rounded_full()
                    .bg(rgb(theme::SWITCH_KNOB))
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(1.0), gpui::hsla(0.0, 0.0, 0.0, 0.25))
                            .blur_radius(px(3.0)),
                    ]),
            ),
        enabled,
        theme,
    )
}
```
(`border_color` of a Rgba and a `transparent_black()` Hsla differ in type: write both arms as `gpui::Hsla::from(..)`.) `segmented`:

```rust
/// A segmented control's container (...): a `layer[0]` track, padding `SEGMENT_PADDING`.
pub(crate) fn segmented(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .p(px(theme::SEGMENT_PADDING))
        .rounded(px(radius::SEGMENTED))
        .bg(gpui::rgba(theme.layer[0]))
}
```
`segment_in`: add `.relative()`, a 1 px transparent border on every segment (`.border(px(BORDER_WIDTH)).border_color(gpui::transparent_black())`), `.font_weight(FontWeight(weight::MEDIUM))`; active arm becomes `el.bg(rgb(theme.surface_raised)).border_color(gpui::rgba(theme.layer[1])).text_color(rgb(theme.text)).child(top_highlight(theme, radius::CHIP)).when(enabled, |el| el.cursor_pointer()).when(!enabled, |el| el.opacity(opacity::DISABLED))`. Update the docs ("selected = a lifted `surface_raised` chip with a ring").

`kbd`: both arms start from

```rust
    let base = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .h(theme::rems_px(theme::KBD_HEIGHT))
        .min_w(theme::rems_px(theme::KBD_MIN_WIDTH))
        .px(px(space::XS))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::MEDIUM));
```
`on_accent` arm unchanged below that (`text_color(on_accent)`, `opacity(SECONDARY)`); the neutral arm:
`.rounded(px(radius::TAG)).border(px(theme::BORDER_WIDTH)).border_color(gpui::rgba(theme.layer[1])).bg(gpui::rgba(theme.layer[0])).font_family(theme.terminal_font_family.clone()).text_size(font_size::OVERLINE).text_color(rgb(theme.text_muted)).child(text)` (delete `.border_b(KBD_BOTTOM_BORDER)`, `KBD_BOTTOM_BORDER` is 1 and equals the border).

`tinted_chip`: add `.border(px(theme::BORDER_WIDTH)).border_color(gpui::rgba(theme::with_alpha(fg, theme::CHIP_RING_ALPHA)))`, weight `MEDIUM`. `status_pill`: add the same ring from `color`.

- [ ] **Step 5: Empty state.** The glyph stays text (`>_`, `✓`) so the other parts' call sites do not change.

```rust
pub(crate) fn empty_state(
    theme: &Theme,
    glyph: Option<&'static str>,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(space::M))
        .p(px(space::XL))
        .font_family(theme.ui_font_family.clone())
        .when_some(glyph, |el, glyph| {
            el.child(
                div()
                    .size(theme::rems_px(theme::EMPTY_STATE_TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::PILL))
                    .bg(gpui::rgba(theme.layer[0]))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::rgba(theme.layer[1]))
                    .text_size(font_size::VIEW_TITLE)
                    .text_color(rgb(theme.text_subtle))
                    .child(glyph),
            )
        })
        .children(title.map(|title| {
            div()
                .text_size(font_size::EMPTY_TITLE)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(title)
        }))
        .child(
            div()
                .max_w(px(theme::EMPTY_STATE_MAX_WIDTH))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_subtle))
                .child(sentence),
        )
        .children(detail.map(|detail| {
            div()
                .max_w(px(theme::READABLE_WIDTH))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_subtle))
                .child(detail)
        }))
}
```
Update the doc ("a circle holding the glyph (VIEW_TITLE size, the 20 px of the spec)"). An action under it adds `.mt(px(space::XS))` itself (12 gap + 4 = 16, the spec's `mt(L)`); Task 9 does this for the Settings call, and the sidebar's call is Part F's.

- [ ] **Step 6: Verify.** `cargo check -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, `cargo test -p mkrew-ui -j 2` (no snapshot should change; if an insta snapshot of a control changed, inspect before accepting). Check the title bar still fits `CONTROL_HEIGHT` 32 in `TITLE_BAR_HEIGHT` 42 and at `[ui] font_size` 20 (`rems_px`). Screenshots dark and light: a "New task" dialog (`keys -Keys "^+n"`; fields, Tab to focus a button, primary and secondary buttons), Settings Appearance (switch off and on, segmented, search box with kbd), an empty state (a workspace without panes). Files `tern-polish-controls-dark.png` / `-light.png`, `tern-polish-settings-appearance-*.png`.

- [ ] **Step 7: Commit.** ADR 0117 bullet: "Focus is a 1 px accent border plus a 3 px halo at 18 % (amends 0085); `CONTROL_HEIGHT` 32; secondary buttons are raised fills with a ring; the switch is 36x22 with a white knob." Update `docs/SPEC.md` where it names the focus ring, `SWITCH_*` or button heights (`grep -n "focus_ring\|SWITCH\|CONTROL_HEIGHT\|segmented" docs/SPEC.md`).
```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/field.rs crates/mkrew-ui/src/shell docs/decisions/0117-tern-polish.md docs/SPEC.md docs/screenshots/tern-polish-controls-dark.png docs/screenshots/tern-polish-controls-light.png docs/screenshots/tern-polish-settings-appearance-dark.png docs/screenshots/tern-polish-settings-appearance-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Tern buttons, focus halo, inputs, switch, segmented, kbd, chips and empty state"
```
(`git add crates/mkrew-ui/src/shell` stages only files this task edited because the working tree of this branch holds only this plan's commits; if other uncommitted edits exist, list the files from `git diff --name-only` instead.)

---

### Task 8: `dialog_frame`, `error_note`, and every dialog on them

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`DIALOG_WIDTH` 520 → 560; tokens `DIALOG_ICON_TILE`, `DIALOG_BUTTON_MIN_WIDTH`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`floating_panel` radius; new `DialogTone`, `DialogHead`, `dialog_frame`, `dialog_body`, `dialog_foot`, `dialog_close`, `dialog_text`, `dialog_hint`, `primary_button`, `error_note`)
- Modify (one commit per group below): `shell/confirm_prompt.rs`, `shell/task_dialog.rs`, `shell/dialogs.rs` (undo, broadcast), `shell/git_dialogs.rs` (discard, base), `shell/worktrees.rs` (worktree, compare), `shell/send_dialog.rs` (`doc_dialog_panel`, send, changes), `shell/ports.rs`, `shell/phone_access.rs`, `shell/ask_dialog.rs`, `shell/history.rs` (turns, history), `shell/path_field.rs`, `shell/workspace_actions.rs`, `shell/window_mode.rs` (open-ask, split picker), `shell/run.rs` (run setup, run picker), `shell/diagram_dialog.rs`, `shell/machines.rs` (`error_line`)
- `crates/mkrew-ui/src/base_dialog.rs`, `confirm.rs`, `task_dialog.rs`, `undo_dialog.rs`, `turns_dialog.rs`, `phone_dialog.rs`, `ports.rs`, `ask.rs`, `worktree_dialog.rs` (root, models): **no change**, they are pure state and key maps with no drawing
- Test: none new. The frame is layout and color only; its tokens are covered by Task 5 and Part A. Gate: clippy and screenshots.

**Interfaces:**
- Consumes: Task 7 (`danger_ring`, `DANGER_RING_MIX`, `small_icon_button`, `focus_stop_filled`, `focus_halo`), Part A (`top_highlight`, `radius::PANEL`).
- Produces (names every other part uses for a dialog):
  - `enum DialogTone { Normal, Danger }`
  - `struct DialogHead { icon: Icon, tone: DialogTone, title: SharedString, sub: Option<SharedString> }`
  - `fn dialog_frame(panel: Stateful<Div>, head: DialogHead, close: Option<Stateful<Div>>, body: Option<Div>, foot: Option<Div>, theme: &Theme) -> Stateful<Div>`
  - `fn dialog_body() -> Div`, `fn dialog_foot(theme: &Theme) -> Div`, `fn dialog_close(id: &'static str, theme: &Theme) -> Stateful<Div>`
  - `fn dialog_text(text: impl IntoElement, theme: &Theme) -> Div` (body sentence, `text_muted`), `fn dialog_hint(text: impl IntoElement, theme: &Theme) -> Div` (foot hint, `text_subtle`, grows)
  - `fn primary_button(id: &'static str, label: impl IntoElement, key: Option<String>, theme: &Theme) -> Stateful<Div>`
  - `fn error_note(text: impl Into<SharedString>, theme: &Theme) -> Div`

- [ ] **Step 1: Tokens** (`theme.rs`): `DIALOG_WIDTH` = 560.0 (doc "a dialog's width", was 520); add

```rust
/// A dialog head's icon tile (Tern `.mdl-ic`, 34 px rounded to the 4 px grid).
pub const DIALOG_ICON_TILE: f32 = 32.0;
/// The primary button of a dialog is at least this wide (room for its key hint).
pub const DIALOG_BUTTON_MIN_WIDTH: f32 = 84.0;
```

- [ ] **Step 2: Helpers** (`widgets.rs`, after `floating_panel`). First, in `floating_panel` set the radius to the modal radius: `.rounded(px(radius::PANEL))` and `top_highlight(theme, radius::PANEL)` (both are `radius::LARGE` today, or whatever Part A left; every floating panel, the palette included, is 16 like Tern's `.cmdk`). Then:

```rust
/// Whether a dialog is about something destructive: its icon tile takes the danger tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogTone {
    Normal,
    Danger,
}

/// The head of a dialog: a 32 px icon tile, the title, an optional one-line sub.
pub(crate) struct DialogHead {
    pub(crate) icon: Icon,
    pub(crate) tone: DialogTone,
    pub(crate) title: SharedString,
    pub(crate) sub: Option<SharedString>,
}

/// The close button of a dialog head: 26 px, a 14 px X in `text_subtle`. Not a Tab stop (Esc
/// closes); the caller adds `.on_click(..)`.
pub(crate) fn dialog_close(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    small_icon_button(id, Icon::X, theme).tooltip(tooltip(theme, "Close", "Esc"))
}

/// The body column of a dialog: gap `space::L`, `px(XL) pb(L)`, it takes the height the head and
/// foot leave (a fixed-height dialog scrolls inside it).
pub(crate) fn dialog_body() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(space::L))
        .px(px(space::XL))
        .pb(px(space::L))
}

/// The foot band: a `divider` line on top, `py(M) px(L)`, buttons right-aligned with gap `S`.
/// A hint goes first as [`dialog_hint`] (it grows and pushes the buttons right).
pub(crate) fn dialog_foot(theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .gap(px(space::S))
        .px(px(space::L))
        .py(px(space::M))
        .border_t(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
}

/// A sentence of a dialog body (`text_muted`).
pub(crate) fn dialog_text(text: impl IntoElement, theme: &Theme) -> gpui::Div {
    div()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .child(text)
}

/// The hint on the left of a dialog foot (why a button is off, the keys): META `text_subtle`.
pub(crate) fn dialog_hint(text: impl IntoElement, theme: &Theme) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .text_size(font_size::META)
        .text_color(rgb(theme.text_subtle))
        .child(text)
}

/// The primary button of a dialog: at least `DIALOG_BUTTON_MIN_WIDTH` wide, with the key that
/// presses it drawn as a [`kbd`] on the accent (`Some("↵")` when Enter does it).
pub(crate) fn primary_button(
    id: &'static str,
    label: impl IntoElement,
    key: Option<String>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button(
        id,
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(label)
            .children(key.map(|key| kbd(key, theme, true))),
        theme,
        true,
    )
    .min_w(px(theme::DIALOG_BUTTON_MIN_WIDTH))
    .justify_center()
}

/// One error line of a dialog or page: `danger_bg` fill, a 1 px ring of the danger tone, a
/// CircleAlert and META text.
pub(crate) fn error_note(text: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_start()
        .gap(px(space::S))
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.danger_bg))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(danger_ring(theme, theme::DANGER_RING_MIX)))
        .text_size(font_size::META)
        .text_color(rgb(theme.text))
        .child(icon_svg(Icon::CircleAlert, theme::ICON_SIZE_SMALL, theme.danger_text))
        .child(div().flex_1().min_w_0().child(text.into()))
}

/// The frame of every dialog: `panel` (a `floating_panel` the caller sized and wired with
/// `dialog_root`), then the head (icon tile, title, sub, close), the `body` and the `foot` band.
pub(crate) fn dialog_frame(
    panel: gpui::Stateful<gpui::Div>,
    head: DialogHead,
    close: Option<gpui::Stateful<gpui::Div>>,
    body: Option<gpui::Div>,
    foot: Option<gpui::Div>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let danger = head.tone == DialogTone::Danger;
    let (fill, ring, glyph) = if danger {
        (
            rgb(theme.danger_bg),
            rgb(danger_ring(theme, theme::DANGER_RING_MIX)),
            theme.danger_text,
        )
    } else {
        (rgb(theme.accent_bg), gpui::rgba(theme.layer[1]), theme.accent_text)
    };
    let head_row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(space::M))
        .px(px(space::XL))
        .pt(px(space::L))
        .pb(px(space::M))
        .child(
            div()
                .flex_none()
                .size(theme::rems_px(theme::DIALOG_ICON_TILE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::MEDIUM))
                .bg(fill)
                .border(px(theme::BORDER_WIDTH))
                .border_color(ring)
                .child(icon_svg(head.icon, theme::ICON_SIZE, glyph)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(font_size::CARD_TITLE)
                        .font_weight(FontWeight(weight::SEMIBOLD))
                        .text_color(rgb(theme.text))
                        .child(head.title),
                )
                .children(head.sub.map(|sub| {
                    div()
                        .truncate()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_subtle))
                        .child(sub)
                })),
        )
        .children(close);
    panel
        .flex()
        .flex_col()
        .font_family(theme.ui_font_family.clone())
        .child(head_row)
        .children(body)
        .children(foot)
}
```
(`FontWeight`, `SharedString`, `icon_svg`, `Icon` come from `crate::*`.)

- [ ] **Step 3: Convert every dialog.** One recipe for all of them. In each `render_*` fn:

  1. Keep the `floating_panel(id, theme)`, its `.w(..)` / `.h(..)` and the `.dialog_root(..)` (and, for text dialogs, `.relative().child(self.typing(..))`). Delete `.flex().flex_col().gap(px(space::M)).p(px(space::XL)).font_family(..)` (the frame sets them).
  2. Delete the CARD_TITLE title child. Its text becomes `DialogHead.title`; a sentence that described the dialog under it either becomes `sub` (one short line) or the first `dialog_text(..)` of the body.
  3. Move every remaining child, unchanged, from the panel chain into `let body = dialog_body().child(..)...`. Replace each hand-rolled `error_row` closure by `let error_row = |error: String| error_note(error, theme);` (callers stay `.children(x.map(error_row))`).
  4. Replace the footer `div().flex().items_center()...gap(px(space::S)).pt(px(space::S))` by `dialog_foot(theme)`, keep its children, and turn a `text_muted` hint div into `dialog_hint(.., theme)`.
  5. Assemble: `let panel = dialog_frame(panel, DialogHead { .. }, Some(dialog_close("<id>-close", theme).on_click(cx.listener(|shell, _, window, cx| shell.close_<x>(window, cx)))), Some(body), Some(foot), theme);` and pass `panel` to `Self::modal_shade` as before.
  6. A primary button whose Enter really presses it becomes `primary_button(id, label, Some("↵".to_owned()), theme).focus_stop_filled(theme)`; one pressed by Ctrl+Enter shows `Some(mkrew_ui::os_text!("Ctrl+Enter", "⌘↩").to_owned())`; one Enter does not press (undo, turns Close) uses `None`.

  **8a: frame, confirm, task, undo, discard.**

  `confirm_prompt.rs` (`Render::render`), full replacement of the button map and panel:

```rust
        let critical = self.level == PromptLevel::Critical;
        let primary_key = (self.selected == 0 && !critical).then(|| "↵".to_owned());
        let buttons: Vec<gpui::AnyElement> = self
            .order
            .iter()
            .map(|&ix| {
                // `order` only holds indices of `buttons`.
                let answer = &self.buttons[ix];
                let id = BUTTON_IDS[ix];
                let label = answer.label().clone();
                let secondary = ix != 0 && !answer.is_cancel();
                let filled = ix == 0 && !critical;
                let frame = if ix == 0 && critical {
                    danger_button(id, label, &theme)
                } else if ix == 0 {
                    primary_button(id, label, primary_key.clone(), &theme)
                } else if secondary {
                    secondary_button(id, label, &theme)
                } else {
                    // No tooltip: gpui draws none while a prompt is open.
                    button(id, label, &theme, false)
                };
                // Every button keeps a 1 px border, so the keyboard ring never shifts the row:
                // the ring is the accent plus its halo, or `focus_ring` on the accent fill.
                let frame = if ix == self.selected {
                    let ringed = frame.border(px(theme::BORDER_WIDTH));
                    if filled {
                        ringed.border_color(rgb(theme.focus_ring))
                    } else {
                        ringed
                            .border_color(rgb(theme.accent))
                            .shadow(focus_halo(theme.accent))
                    }
                } else if secondary || (ix == 0 && critical) {
                    frame
                } else {
                    frame
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(gpui::transparent_black())
                };
                frame
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(ix))))
                    .into_any_element()
            })
            .collect();
        let body = self.detail.clone().map(|detail| {
            dialog_body().child(dialog_text(detail, &theme))
        });
        let panel = dialog_frame(
            floating_panel("confirm-dialog", &theme).w(px(theme::CONFIRM_DIALOG_WIDTH)),
            DialogHead {
                icon: if critical { Icon::TriangleAlert } else { Icon::CircleHelp },
                tone: if critical { DialogTone::Danger } else { DialogTone::Normal },
                title: self.message.clone(),
                sub: None,
            },
            None,
            body,
            Some(dialog_foot(&theme).children(buttons)),
            &theme,
        );
```
The shade `div()` below is unchanged. (`dialog_frame` is `pub(crate)` in `shell::widgets`; `confirm_prompt.rs` already reaches it as `super::widgets::appear`, and `use crate::*` brings the rest.)

  `task_dialog.rs` `render_task_dialog` (the panel starts at ~417): head `DialogHead { icon: if editing { Icon::Pencil } else { Icon::Plus }, tone: DialogTone::Normal, title: (if editing { "Edit task" } else { "New task" }).into(), sub: Some("What the agent should do, and which agent does it.".into()) }`, close `dialog_close("task-close", theme)` with `shell.close_task_dialog(window, cx)`. Body: `dialog_body()` holding, unchanged and in order, the Title block, the Prompt block, the Agent block, `worktree`, and `.children(dialog.error.clone().map(|error| error_note(error, theme)))` (this replaces the inline `✕` row at ~579-586). Foot, replacing ~587-629:

```rust
        let mut foot = dialog_foot(theme);
        if editing {
            foot = foot.child(if dialog.busy {
                disabled_button("task-delete", "Delete", theme, false).into_any_element()
            } else {
                button("task-delete", "Delete", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Delete this task", ""))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.delete_task(window, cx);
                    }))
                    .into_any_element()
            });
        }
        let foot = foot
            .child(div().flex_1())
            .child(
                button("task-cancel", "Cancel", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Discard changes", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.close_task_dialog(window, cx);
                    })),
            )
            .child(if dialog.busy {
                disabled_button("task-save", save_label, theme, true).into_any_element()
            } else {
                primary_button(
                    "task-save",
                    save_label,
                    Some(mkrew_ui::os_text!("Ctrl+Enter", "⌘↩").to_owned()),
                    theme,
                )
                .focus_stop_filled(theme)
                .tooltip(tooltip(theme, save_label, mkrew_ui::os_text!("Ctrl+Enter", "⌘↩")))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.save_task_dialog(window, cx);
                }))
                .into_any_element()
            });
```
(keep the existing save listener body, ~625-627, in place of `save_task_dialog` if its name differs.) The panel keeps the existing `.when(self.prompt_picker.is_none() && ..., |el| el.dialog_root(..).relative().child(self.typing(..)))`.

  `dialogs.rs` undo (`render_undo_dialog`, the panel at ~378): replace the `error_row` closure (~259-265) with `let error_row = |error: String| error_note(error, theme);`. Then:

```rust
        let body = dialog_body()
            .child(dialog_text(
                if nothing_changed {
                    format!("{intro}: there is nothing to undo.")
                } else {
                    format!("{intro}. The changes below are discarded and cannot be recovered.")
                },
                theme,
            ))
            .children(body)
            .children(dialog.error.clone().map(error_row));
        let mut foot = dialog_foot(theme).child(dialog_hint(hint.map(SharedString::from), theme));
        foot = if nothing_changed {
            foot.child(
                primary_button("undo-close", "Close", None, theme)
                    .focus_stop_filled(theme)
                    .tooltip(tooltip(theme, "Close", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.close_undo_dialog(window, cx);
                    })),
            )
        } else {
            foot.child(
                button("undo-cancel", "Cancel", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Keep the changes", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.close_undo_dialog(window, cx);
                    })),
            )
            .child(match confirm {
                Ok(_) => danger_button("undo-confirm", "Discard changes", theme)
                    .focusable_bordered(theme)
                    .tooltip(tooltip(theme, "Discard every change listed above", ""))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.confirm_undo(window, cx);
                    })),
                Err(_) => {
                    let label = if dialog.busy { "Discarding…" } else { "Discard changes" };
                    disabled_button("undo-confirm", label, theme, false)
                        .bg(rgb(theme.diff_del_bg))
                        .text_color(rgb(theme.diff_del_text))
                }
            })
        };
        let panel = dialog_frame(
            floating_panel("undo-dialog", theme)
                .w(px(theme::UNDO_DIALOG_WIDTH))
                .dialog_root(
                    &self.dialog_focus,
                    false,
                    cx.listener(Self::dialog_guard),
                    cx.listener(Self::undo_dialog_key_down),
                ),
            DialogHead {
                icon: Icon::Undo2,
                tone: DialogTone::Danger,
                title: dialog.heading().into(),
                sub: None,
            },
            Some(dialog_close("undo-close-x", theme).on_click(cx.listener(|shell, _, window, cx| {
                shell.close_undo_dialog(window, cx);
            }))),
            Some(body),
            Some(foot),
            theme,
        );
```
  (`body` in the old code is the `Vec<AnyElement>` named `body`; the new `let body = dialog_body()...children(body)` consumes it; rename the new binding `column` if the compiler complains about the shadow.) `dialog.heading()` returns `String`; `.into()` makes the `SharedString`.

  `git_dialogs.rs` discard (`render_git_discard`, ~359): replace the `error_row` closure at ~270-276 as in recipe step 3; head `DialogHead { icon: Icon::Trash2, tone: DialogTone::Danger, title: dialog.title().into(), sub: None }`, close id `git-discard-close` calling `shell.close_git_discard(window, cx)`, body `dialog_body().child(dialog_text(dialog.sentence(), theme)).child(body).children(dialog.error.clone().map(error_row))` (the old `body` is a `match` value, rename the old binding to `shown_body` if needed), foot `dialog_foot(theme)` with Cancel and `danger_button("git-discard-confirm", dialog.confirm_label(), theme).focusable_bordered(theme)...` / the disabled tint as it is now. No hint.

  Commit after 8a: `git add` the files listed under 8a plus `widgets.rs`, `theme.rs`; message `feat(ui): dialog_frame and error_note, confirm, task, undo and discard on them`. Run `cargo check -p mkrew-ui` first: every other `floating_panel(..).p(XL)` dialog still compiles untouched (the radius change applies to all).

  **8b: the rest of the spec's list.** Same recipe; the per-dialog specifics:

| Dialog (fn, file) | `DialogHead` | close fn | Body | Foot |
|---|---|---|---|---|
| worktree (`render_worktree_dialog`, `worktrees.rs` ~636) | icon `Icon::GitBranch`, tone `Danger` when `dialog.kind == Kind::Remove` else `Normal`, title `dialog.heading().into()` | `close_worktree_dialog` | `dialog_text(dialog.intro(), theme)`, then old `body`, then the `error_row` | `dialog_foot(theme).child(dialog_hint(hint.map(SharedString::from), theme)).children(buttons)`; the Merge confirm is `primary_button("worktree-confirm", text, None, theme).focus_stop_filled(theme)`; every `danger_button(..).focus_stop` was rewritten in Task 7 |
| compare (`render_compare`, `worktrees.rs` ~796) | `Icon::GitCompare`, `Normal`, title `"Compare agents"` | `close_compare` | `dialog_text(format!(...), theme)` (the existing sentence) then `div().flex().gap(px(space::M)).children(columns)` | `dialog_foot(theme).child(button("compare-close", "Close", theme, false)...)` |
| send (`render_send_dialog` + `doc_dialog_panel`, `send_dialog.rs`) | `Icon::Send`, `Normal`, title the existing `match (selection, path)` string | `close_doc_dialog` | `dialog_text(sentence, theme)`, the `send-sessions` list, the error as `error_note` | the existing `footer` rebuilt with `dialog_foot(theme)`; Send and Open composer are `primary_button(.., Some("↵".to_owned()), theme).focus_stop_filled(theme)` |
| changes (`render_changes`, same file) | `Icon::FileDiff`, `Normal`, title `format!("Changes · {}", changes.file_path)` | `close_doc_dialog` | `dialog_text(sentence, theme)`, the `changes-diff` box, the "more lines" meta | `dialog_foot(theme).child(button("changes-close", "Close", theme, true)...)` becomes `primary_button("changes-close", "Close", None, theme).focus_stop_filled(theme)` |
| ports (`render_ports`, `ports.rs` ~318) | `Icon::Server`, `Normal`, title `"Ports"` | `close_ports` | the `errors` Vec mapped through `error_note(text.clone(), theme)`, then `body` | the existing `footer` match rebuilt on `dialog_foot(theme)` (its confirm question becomes `dialog_hint(ports::question(row), theme)` with `text` color kept by `.text_color(rgb(theme.text))`) |
| phone (`render_phone_access`, `phone_access.rs` ~344) | `Icon::Smartphone`, `Normal`, title `"Phone access"` | `close_phone_access` | `status_row`, `phone.notice` and `serve.error` through `error_row`, `serve_row`, `push_row`, `push_hint`, `link` (all existing locals, in the existing order) | `dialog_foot(theme).child(button("phone-close", "Close", theme, false)...)` (the old right-aligned row without `pt`) |
| ask (`render_ask`, `ask_dialog.rs` ~311) | `Icon::Sparkles`, `Normal`, title `"Ask AI"`, `sub: Some("Any question, outside your projects: the agent runs in an empty folder.".into())` (replaces the `muted(..)` child) | `close_ask` | `agents`, `web`, `field`, `answer` (its `Failed` arm becomes `error_note(error.clone(), theme).into_any_element()`) | the existing `footer` rebuilt on `dialog_foot(theme)`; the send button is `primary_button("ask-send", ask_label, Some(mkrew_ui::os_text!("Ctrl+Enter", "⌘↩").to_owned()), theme).focus_stop_filled(theme)` |
| turns (`render_turns_dialog`, `history.rs` ~642) | `Icon::History`, `Normal`, title `format!("Turns of {}", dialog.agent)` | `close_turns_dialog` | `list` (its `Failed` arm becomes `error_note(format!("Cannot list the turns: {error}"), theme).into_any_element()`); the selected row `.when(selected, |el| el.bg(rgb(theme.surface_raised)))` becomes `with_pick(div()..., selected, theme)` (wrap the row div: `with_pick(div().id(..).flex().items_center()..., selected, theme).pressable_unless(..)` is not possible on a non-Stateful, so build the `div().id(..)` first and call `with_pick` on it before `.pressable_unless`) | `dialog_foot(theme).child(primary_button("turns-close", "Close", None, theme).focus_stop_filled(theme)...)` |

  Fix the two panel call sites that wrap `doc_dialog_panel`: it now builds the panel only (`floating_panel(id, theme).w(px(width)).dialog_root(..)`) and returns `Stateful<Div>`; its `title` and `sentence` parameters go away, the callers pass them in `DialogHead` and the body. Update its doc ("A Docs dialog's panel: sized, with the dialog keys").

  Run `cargo check -p mkrew-ui`. Commit: `feat(ui): worktree, compare, send, changes, ports, phone, ask and turns dialogs on dialog_frame`.

  **8c: the remaining dialogs and call sites** (same recipe):

| Dialog | `DialogHead` | Notes |
|---|---|---|
| broadcast (`dialogs.rs` ~589) | `Icon::Send`, `Normal`, `"Broadcast to agents"` | body: the sentence as `dialog_text`, the session list, the key hint as `dialog_hint`-styled `meta_line`, the `broadcast-text` field; foot from the existing `footer`; `broadcast-send` is `primary_button(.., Some("↵".to_owned()), ..)` |
| git base (`git_dialogs.rs` ~445) | `Icon::GitBranch`, `Normal`, `"Base branch"`, `sub: Some("“Against base” shows everything on this branch since it left the base.".into())` | body: `dialog.current` as `meta_line`, the `git-base-ref` field, error; foot: Cancel, Auto-detect, `primary_button("git-base-use", use_label, Some("↵".to_owned()), theme)` |
| plan path (`path_field.rs` ~135) | `Icon::Link`, `Normal`, `"Link plan"`, `sub: Some(format!("Plan file on {}", dialog.machine).into())` | the `meta_line` child goes (it is the sub); foot: Cancel, `primary_button("plan-path-link", "Link", Some("↵".to_owned()), theme)` |
| add workspace (`workspace_actions.rs` ~1030) | `Icon::FolderPlus`, `Normal`, `"Add workspace"` | the head row's "Open a folder…" secondary button moves to the first child of the body (inside `machine.is_none().then(..)`); foot: Cancel, `primary_button("add-workspace-clone", "Clone", Some("↵".to_owned()), theme)` |
| open ask (`window_mode.rs` ~606, `render_open_ask`) | `Icon::ExternalLink`, `Normal`, `format!("Open {}", self.workspace_name(ask.project))` | body: `check`; foot: `buttons` row rebuilt on `dialog_foot(theme)`; "New window" is `primary_button(.., Some("↵".to_owned()), ..)`; its width stays `CONFIRM_DIALOG_WIDTH`; the anchored variant keeps working (the frame is the panel) |
| split picker (`window_mode.rs` ~945) | `Icon::Columns2`, `Normal`, `"Open workspaces in windows"` | body: `body`; foot: `secondary`, then the `primary` (the enabled one becomes `primary_button("split-open", label, Some("↵".to_owned()), theme).focus_stop_filled(theme)`) |
| run setup (`run.rs` ~975) | `Icon::Play`, `Normal`, `"Run setup"`, `sub: Some("Each command runs in its own pane of the “run” tab, in a shell started in its folder.".into())` | the workspace name `· {name}` goes into the title (`format!("Run setup · {name}")` when `workspace` is `Some`); body: the scroll area with `scrollbar::bar`, the error; foot: `autostart` first then `div().flex_1()`, Cancel, `primary_button("run-save", "Save", Some(os_text!(..).to_owned()), theme)`; the foot already had a `divider` top line, now it comes from `dialog_foot` (delete the old border and `pt`) |
| run picker (`run.rs` ~1197) | `Icon::FolderOpen`... if absent use `Icon::Folder`, title `if folder {"Choose the folder"} else {"Choose the env file"}`, `sub: Some(location.into())` | body: `up` button row, `list`; hint line as `dialog_hint` in the foot; rows with `picker.cursor == Some(index)` use `.when(.., bg(surface_raised))` today: switch them to `with_pick` (build the row `div().id(..)`, call `with_pick(.., picker.cursor == Some(index), theme)`, then `.pressable_unless(..)`) |
| history (`history.rs` ~826) | `Icon::History`, `Normal`, `format!("History · {}", dialog.workspace)` | fixed size `.w(relative(..)).h(relative(..))` stays on the panel; body: the error and the two-pane row (`flex_1 min_h_0`, as now); foot: `resume` button via `.children(resume)` and a Close button (the old head row's two buttons move to the foot); the list rows' `.when(selected, bg(surface_raised))` become `with_pick` as above |
| diagram (`diagram_dialog.rs` ~46) | `Icon::Share2`, `Normal`, `"Diagram"` | `foot: None`, the image scroll is the body (`dialog_body().child(<the existing diagram-dialog-body div>)`), close `dialog_close("diagram-close", theme)` |

  Not converted, on purpose: pickers and viewers that are not question-and-answer dialogs and keep `floating_panel` with their own layout: `palette` (`navigation.rs`), `slash-picker` (`slash.rs`), `prompt-picker` (`composer.rs` ~640), `settings` (Task 9), and the two attachment preview panels of `tsp_view.rs` (~3197, ~3489, Parts C and D). They still get the 16 px radius, the `glass_hi` line and the modal shadow from `floating_panel`. The prompt picker's row at `composer.rs` ~550 (`.when(picked, |el| el.bg(rgb(theme.surface_raised)))`) and `slash.rs` ~301, `ports.rs` ~203, `send_dialog.rs` ~383, `prompts_render.rs` ~97 use the same selected-row bg: switch each to `with_pick` in this commit (`composer.rs` is Part D's file: a one-line edit in `render_prompt_picker`).

  `machines.rs` `error_line`: `error_note(text, theme).into_any_element()` (the Settings and Prompts page errors; Task 9 does `settings_banner`).

- [ ] **Step 4: Verify.** `cargo check -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, `cargo test -p mkrew-ui -j 2`. `rg "text_color\(rgb\(theme.status_exited\)\).child\(\"✕\"\)" crates/mkrew-ui/src/shell` must print only the cards, comments and composer inline status lines (`shell/cards.rs`, `shell/comments.rs`, `shell/composer.rs`, `prompts_render.rs`'s broken-file row), not a dialog. Screenshots dark and light: the confirm prompt (palette "Close all panes"-style prompt or a workspace remove), "New task" (`^+n`), Ports, Settings is Task 9. Files `tern-polish-confirm-*.png`, `tern-polish-task-dialog-*.png`.

- [ ] **Step 5: Commit** (after 8c; ADR 0117 bullet: "Every dialog is a `dialog_frame`: icon tile head, body, foot band, primary with `kbd`; `error_note` replaces the ✕ rows; `DIALOG_WIDTH` 560; floating panels are `radius::PANEL`." SPEC: update the dialog sentences at ~1942 and ~2030 and "Esc / Close" mentions).
```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell docs/decisions/0117-tern-polish.md docs/SPEC.md docs/screenshots/tern-polish-confirm-dark.png docs/screenshots/tern-polish-confirm-light.png docs/screenshots/tern-polish-task-dialog-dark.png docs/screenshots/tern-polish-task-dialog-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the remaining dialogs on dialog_frame, selected rows on with_pick"
```

---

### Task 9: Settings frame, navigation, page heads, sections, rows

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`SETTINGS_DIALOG_WIDTH` 960, `SETTINGS_DIALOG_HEIGHT` 720, `SETTINGS_NAV_WIDTH` 232, `SETTINGS_CONTROL_WIDTH` 260; new `SETTINGS_CONTENT_MAX_WIDTH`, `SETTINGS_NAV_ROW_HEIGHT`, `SETTINGS_ROW_MIN_HEIGHT`)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (`Tab::icon`, `Tab::lead`, `SettingsDialog::lead`, `mod tests`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (new `section_title`)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`render_settings_dialog`, `settings_cards`, `settings_banner`, `settings_row`, the scroll-to-item index in `settings_key_down`)
- Modify: `crates/mkrew-ui/src/shell/theme_gallery.rs` (two `overline` calls), `crates/mkrew-ui/src/shell/prompts_render.rs` (list rows)
- No change: `crates/mkrew-ui/src/prompts_page.rs` (pure model: drafts, spots, library; nothing is drawn there)
- Test: `crates/mkrew-ui/src/settings_dialog.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 6 `with_pick`; Task 7 `small_icon_button` is superseded by Task 8 `dialog_close`; Task 8 `error_note`, `dialog_close`; Part A `shadow_modal` (through `floating_panel`).
- Produces:
  - `Tab::icon(self) -> Icon`, `Tab::lead(self) -> &'static str`, `SettingsDialog::lead(&self) -> &'static str`
  - `widgets::section_title(text: &'static str, theme: &Theme) -> Div` (sentence case, META MEDIUM `text_subtle`)

- [ ] **Step 1: Write the failing test** (`settings_dialog.rs`, `mod tests`):

```rust
    /// Every page opens with one plain sentence under its title, and has its own nav icon.
    #[test]
    fn every_page_has_a_one_sentence_lead_and_its_own_icon() {
        let mut seen: Vec<Icon> = Vec::new();
        for tab in Tab::all() {
            let lead = tab.lead();
            assert!(
                lead.ends_with('.') && lead.matches('.').count() == 1,
                "{tab:?}: one sentence with one full stop: {lead}"
            );
            assert!(
                lead.chars().next().is_some_and(char::is_uppercase),
                "{tab:?}: starts with a capital: {lead}"
            );
            assert!(lead.len() <= 90, "{tab:?}: a lead fits one line: {lead}");
            assert!(!lead.contains(['-', '–']), "{tab:?}: no dash: {lead}");
            assert!(!seen.contains(&tab.icon()), "{tab:?}: icon already used");
            seen.push(tab.icon());
        }
    }

    #[test]
    fn a_search_leads_with_its_own_sentence() {
        let mut d = dialog();
        let page = d.lead();
        d.find.insert("theme");
        assert_ne!(d.lead(), page);
        assert!(d.lead().ends_with('.'));
    }
```
(the module imports `Icon` through `use super::*` once the file imports it, see Step 3).

- [ ] **Step 2: Run it, expect FAIL** (no `Tab::lead`, `Tab::icon`): `cargo test -p mkrew-ui settings_dialog -j 2`

- [ ] **Step 3: Implement.**

`settings_dialog.rs`: add `use crate::icons::Icon;` and in `impl Tab`:

```rust
    /// The page's icon in the navigation (Lucide, 14 px). Every page has its own.
    pub fn icon(self) -> Icon {
        match self {
            Tab::Settings(Page::General) => Icon::Settings,
            Tab::Settings(Page::Appearance) => Icon::SunMoon,
            Tab::Settings(Page::Notifications) => Icon::Bell,
            Tab::Settings(Page::Voice) => Icon::MessageSquare,
            Tab::Settings(Page::Features) => Icon::Sparkles,
            Tab::Settings(Page::Jira) => Icon::SquareKanban,
            Tab::Settings(Page::Phone) => Icon::Smartphone,
            Tab::Machines => Icon::Server,
            Tab::Keyboard => Icon::CornerDownLeft,
            Tab::Prompts => Icon::Library,
        }
    }

    /// One sentence under the page title: what the page is for.
    pub fn lead(self) -> &'static str {
        match self {
            Tab::Settings(Page::General) => {
                "Windows, terminals, workspaces and how Mkrew behaves on this computer."
            }
            Tab::Settings(Page::Appearance) => "Theme, fonts, and how documents and diffs look.",
            Tab::Settings(Page::Notifications) => {
                "Choose when Mkrew alerts you and where the alerts go."
            }
            Tab::Settings(Page::Voice) => {
                "Dictate into any field and choose how your speech is recorded and transcribed."
            }
            Tab::Settings(Page::Features) => "Panels and session behavior you can switch on or off.",
            Tab::Settings(Page::Jira) => {
                "Connect your Jira site and choose which issues the board shows."
            }
            Tab::Settings(Page::Phone) => "Let your phone reach Mkrew over your private network.",
            Tab::Machines => "Other computers whose agents you manage from this window.",
            Tab::Keyboard => "Every command and its shortcut; select one to record a new key.",
            Tab::Prompts => "Reusable prompts you can insert into the composer and tasks.",
        }
    }
}
```
`Page` has exactly the 7 variants matched, so the `match` is exhaustive. In `impl SettingsDialog` next to `heading`:

```rust
    /// The sentence under the right side's title.
    pub fn lead(&self) -> &'static str {
        if self.searching() {
            "Matches from every page; select one to change it."
        } else {
            self.tab.lead()
        }
    }
```

- [ ] **Step 4: Run, expect PASS.** `cargo test -p mkrew-ui settings_dialog -j 2`

- [ ] **Step 5: Tokens** (`theme.rs`):

```rust
pub const SETTINGS_DIALOG_WIDTH: f32 = 960.0;   // was 880
pub const SETTINGS_DIALOG_HEIGHT: f32 = 720.0;  // was 640
pub const SETTINGS_NAV_WIDTH: f32 = 232.0;      // was 200
pub const SETTINGS_CONTROL_WIDTH: f32 = 260.0;  // was 240
/// The page column: its widest and the height of a navigation row and of a setting row.
pub const SETTINGS_CONTENT_MAX_WIDTH: f32 = 740.0;
pub const SETTINGS_NAV_ROW_HEIGHT: f32 = 30.0;
pub const SETTINGS_ROW_MIN_HEIGHT: f32 = 56.0;
```

- [ ] **Step 6: `section_title`** (`widgets.rs`, beside `overline`; `overline` stays for the palette's group lines):

```rust
/// A section title in sentence case: META MEDIUM in `text_subtle` (Settings groups and cards).
pub(crate) fn section_title(text: &'static str, theme: &Theme) -> gpui::Div {
    div()
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::MEDIUM))
        .text_color(rgb(theme.text_subtle))
        .child(text)
}
```

- [ ] **Step 7: The dialog** (`settings.rs`, `render_settings_dialog`). Layout: a full-height navigation on the left, and on the right the scrolling page and the footer; the close button floats top right; the page title lives in the scroll.

Navigation items and groups (replace `tab_item`, `nav_items` and `nav`):

```rust
        let tab_item = |tab: Tab| {
            let active = tab == dialog.tab && !dialog.searching();
            let focused = tab == dialog.tab && dialog.focus == Focus::Tabs;
            with_pick(
                div()
                    .id(SharedString::from(format!("settings-tab-{}", tab.title())))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .h(theme::rems_px(theme::SETTINGS_NAV_ROW_HEIGHT))
                    .px(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(if focused {
                        rgb(theme.selected_ring).into()
                    } else {
                        gpui::transparent_black()
                    })
                    .text_size(font_size::BODY)
                    .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                    // The active page keeps its tint and the pointer; the others hover.
                    .pressable_unless(active, Look::Row, theme)
                    // Dimmed, still readable and still clickable.
                    .when(dimmed.contains(&tab), |el| el.opacity(opacity::DIMMED))
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            shell.settings_do(|d, _| d.set_tab(tab), window, cx);
                        }),
                    )
                    .child(icon_svg(
                        tab.icon(),
                        theme::ICON_SIZE_SMALL,
                        if active { theme.accent_text } else { theme.text_subtle },
                    ))
                    .child(tab.title()),
                active,
                theme,
            )
            .into_any_element()
        };
        let nav_groups = div().flex().flex_col().gap(px(space::L)).children(
            settings_dialog::NAV.iter().map(|(title, tabs)| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(section_title(title, theme).px(px(space::S)))
                    .children(tabs.iter().map(|&tab| tab_item(tab)))
            }),
        );
```
(`NAV` titles are `&'static str`.) The `find` search box stays as built. Nav:

```rust
        let nav = div()
            .w(px(theme::SETTINGS_NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::L))
            .px(px(space::M))
            .py(px(space::L))
            .bg(rgb(theme.well))
            // gpui does not clip a fill to the panel's rounded corners.
            .rounded_tl(px(radius::PANEL))
            .rounded_bl(px(radius::PANEL))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .child(find)
            .child(nav_groups);
```
The body (replace `body`; each direct child of the scroll container is wrapped in a 740 px column so `settings_scroll.scroll_to_item(n)` still indexes one child per item, plus the head):

```rust
        let column = |el: gpui::AnyElement| {
            div()
                .w_full()
                .max_w(px(theme::SETTINGS_CONTENT_MAX_WIDTH))
                .child(el)
                .into_any_element()
        };
        let page_head = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .pb(px(space::L))
            .child(
                div()
                    .text_size(font_size::VIEW_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text_subtle))
                    .child(dialog.lead()),
            );
        let body = div()
            .id("settings-rows")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .flex()
            .flex_col()
            .items_center()
            .gap(px(if one_card { 0.0 } else { space::M }))
            .px(px(space::XXL))
            .py(px(space::XXL))
            .child(column(page_head.into_any_element()))
            .children(banner.map(|banner| column(banner.mb(px(below)).into_any_element())))
            .children(daemon_note.map(|note| column(note.mb(px(below)).into_any_element())))
            .children(
                (dialog.tab == Tab::Machines && !dialog.searching())
                    .then(|| self.render_machines_page(dialog, typing, theme, cx))
                    .into_iter()
                    .flatten()
                    .map(column),
            )
            .children(
                (dialog.tab == Tab::Prompts && !dialog.searching())
                    .then(|| self.render_prompts_page(dialog, typing, theme, cx))
                    .into_iter()
                    .flatten()
                    .map(column),
            )
            .children(search.map(|search| column(search.mb(px(below)).into_any_element())))
            .children(
                dialog
                    .gallery_shown()
                    .then(|| column(self.render_theme_gallery(dialog, theme, cx))),
            )
            .children(
                self.settings_cards(dialog, &rows, typing, theme, cx)
                    .into_iter()
                    .map(column),
            );
```
(`search` is a `Stateful<Div>` and `.mb` is `Styled`; `render_theme_gallery` returns an `AnyElement`, keep `column` applied as shown.) Update the scroll-follow in `settings_key_down` (~131-137): `.scroll_to_item(1 + card + banner + note + search + gallery)` (the page head is the first scroll child; add that to the comment).

Delete `header`. The footer keeps its content, `.text_color` of its two text pieces becomes `text_subtle`. The right column and panel:

```rust
        let right = div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(body)
            .child(footer)
            .child(
                div().absolute().top(px(space::M)).right(px(space::M)).child(
                    dialog_close("settings-close", theme).on_click(cx.listener(
                        |shell, _, window, cx| shell.close_settings_dialog(window, cx),
                    )),
                ),
            );
        let panel = floating_panel("settings-dialog", theme)
            .w(px(theme::SETTINGS_DIALOG_WIDTH))
            .h(px(theme::SETTINGS_DIALOG_HEIGHT))
            .max_w(max_width)
            .max_h(max_height)
            .flex()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::settings_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::settings_key_down, cx))
            .child(nav)
            .child(right);
        Self::modal_shade("settings-shade", panel, cx)
```
(the panel is a row now: the old `.flex_col()` is dropped.) `floating_panel` already carries `radius::PANEL` (Task 8) and `shadow_modal` (Part A). Task 7 pointed the old header's close button at `small_icon_button`; with the header gone, that import is no longer used here, `dialog_close` (Task 8) calls it.

- [ ] **Step 8: Sections, banner, rows** (`settings.rs`):
  - `settings_cards`: replace `overline(title, theme).px(px(space::S))` with `section_title(title, theme).px(px(space::S)).pt(px(space::M))` (12 gap + 12 pad = 24 above); the group column `gap(px(space::XS))` becomes `gap(px(space::S))`; both cards' `.border_color(rgb(theme.border))` become `.border_color(gpui::rgba(theme.layer[1]))` (keep `.bg(rgb(theme.surface))` and the `LARGE` radius). The empty-search state:

```rust
            return vec![
                empty_state(
                    theme,
                    None,
                    None,
                    format!("No setting matches “{}”.", dialog.find.trim()).into(),
                    None,
                )
                .child(
                    secondary_button("settings-clear-search", "Clear search (Esc)", theme)
                        .mt(px(space::XS))
                        .on_press(true, cx.listener(|shell, _, window, cx| {
                            shell.settings_do(|d, _| d.clear_find(), window, cx);
                        })),
                )
                .into_any_element(),
            ];
```
  - `settings_banner`: 

```rust
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(error_note(text, theme).flex_1().min_w_0())
            .child(self.settings_link("settings-fix", "Fix in Zed", theme, cx, move |shell, cx| {
                if settings_file { shell.edit_settings_file(cx); } else { shell.edit_keymap_file(cx); }
            }))
```
  - `settings_row`: the returned element becomes

```rust
        with_pick(
            div()
                .id(SharedString::from(format!("settings-row-{}", row.id())))
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .min_h(theme::rems_px(theme::SETTINGS_ROW_MIN_HEIGHT))
                .px(px(space::L))
                .py(px(space::M))
                // The first and last row follow the card's rounded corners: gpui does not clip a
                // fill to them.
                .when(first, |el| el.rounded_t(px(radius::LARGE)))
                .when(last, |el| el.rounded_b(px(radius::LARGE)))
                .when(!first, |el| {
                    el.border_t(px(theme::BORDER_WIDTH)).border_color(rgb(theme.divider))
                })
                .pressable_unless(focused, Look::Row, theme),
            focused,
            theme,
        )
        .on_press(true, Self::press_row(index, cx))
```
  then the existing `.children(crumb..)` and inner row, with: inner row `.gap(px(space::XL))`; the label div `.font_weight(FontWeight(weight::MEDIUM))` (keeps `text_color(text)`); the help `meta_line(theme, help.into()).text_color(rgb(theme.text_subtle))`; the "default" tag `text_subtle`; the control column keeps `w(SETTINGS_CONTROL_WIDTH)` (260 now); the row error `.text_color(rgb(theme.danger_text))` is left as is. The old `.when(focused, |el| el.bg(rgb(theme.selected)))` is gone (the pick is the tint plus bar).
  - `theme_gallery.rs` ~162 and ~177: `overline("Themes", theme)` and `overline(title, theme)` become `section_title("Themes", theme)` and `section_title(title, theme)` (both literals are `&'static str`; if `title` is a `String` there, change the gallery's group title type to `&'static str` or call `section_title` through `SharedString`; check with `cargo check`).
  - `prompts_render.rs` ~86-99 list rows: replace `.when(active, |el| el.bg(rgb(theme.surface_raised)))` by building the row `div().id(..)` and wrapping `with_pick(div().id(..).flex_none()...rounded(px(radius::CHIP))..., active, theme)` before `.pressable_unless(active, Look::Row, theme)`; the broken-file row keeps its two lines but uses `error_note(file, theme)` for its first line (`.child(error_note(file, theme))`, dropping the `✕` div).

- [ ] **Step 9: Verify.** `cargo check -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, `cargo test -p mkrew-ui -j 2`. Check by keyboard that Down on a row scrolls to it (the `+ 1` for the head) and that Tab goes nav, find, rows as before. Screenshots dark and light: `keys -Keys "^,"` then Appearance (page head, lead, switch, segmented, fonts card) and General; Keyboard page (one card); Machines (empty state and add button): `tern-polish-settings-general-*.png`, `tern-polish-settings-appearance-*.png` (replaces Task 7's), `tern-polish-settings-keyboard-*.png`. Window narrower than 960 + margin: the panel shrinks through `max_w`.

- [ ] **Step 10: Commit.** ADR 0117 bullet: "Settings is 960x720 with a 232 px well navigation (icon per page, groups in sentence case), a page head (VIEW_TITLE plus a one-sentence lead) inside the scroll, a 740 px content column, sentence-case section titles, 56 px rows." SPEC: rewrite the Settings paragraph at ~1952-1960 (`grep -n "SETTINGS_NAV_WIDTH" docs/SPEC.md`).
```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/shell/prompts_render.rs docs/decisions/0117-tern-polish.md docs/SPEC.md docs/screenshots/tern-polish-settings-general-dark.png docs/screenshots/tern-polish-settings-general-light.png docs/screenshots/tern-polish-settings-appearance-dark.png docs/screenshots/tern-polish-settings-appearance-light.png docs/screenshots/tern-polish-settings-keyboard-dark.png docs/screenshots/tern-polish-settings-keyboard-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Settings frame, navigation, page heads, sections and rows (Tern)"
```

---

## Notes for the coordinator

- **Handoffs between parts:** `with_pick`, `pop_surface`, `dialog_frame` family, `error_note`, `primary_button`, `section_title`, `focus_halo`, `pressable_on` are the new shared names. Part C/D/E/G tasks that draw a selected row should use `with_pick`; a popover (terminal find bar, Part G) uses `pop_surface(theme, radius::LARGE)`; a dialog-like panel uses `dialog_frame`.
- **Cross-part edits Part B makes** (one-liners): `composer.rs` (`composer_tools` loses `hover`; model pill `pressable_on`; prompt picker row `with_pick`), `chat_view.rs` (~1052), `preview.rs` (~1274), `window_mode.rs` (`render_need_you_chip`, open-ask, split picker), `field.rs`. `shell/tsp_view.rs` `tsp_attachment` (~3459) needs `pressable_on` from its owner (see Task 5 note); `empty_state` keeps its signature so `chrome.rs`, `pane_render.rs`, `git_render.rs`, `sidebar.rs` call sites do not change (an action under it adds `.mt(px(space::XS))`, the sidebar's is Part F's).
- **Not covered, by design:** peek popover and error-toast ring cannot be driven in the smoke sandbox (they need a needs-input session and a failing action): add both to `docs/HUMAN_CHECKS.md` in Task 26, dark and light. `fit_bar` tests are untouched by Part B (`CONTROL_HEIGHT` is not a `dock.rs` input).
- **Why few unit tests:** the testable logic in this part is the layer arithmetic (`flatten`, `stack`, press styles, with a per-theme visibility rule: Task 5) and the page copy and icon rules (Task 9). Everything else is styling; tests of it would only echo constants, so those tasks end in clippy and screenshots.


---

## Part C: omp transcript and Chat view (Tasks 10-13, spec §5)

Plan against master (c775413). Part A (Tasks 1-4) is merged first, so `Theme.layer`, `Theme.code_bg`, `Theme::tern_*` exist; Part B gives `Look::Row` / `Look::Text` the `layer[0]` hover rule. Nothing here touches `push_runs`, `tsp_image`, `tsp_attachment` or `thumbnail` (the functions `fix/composer-polish` edits). `theme.rs` gets only added constants plus one value change (`doc::CODE_LINE_HEIGHT`), so a rebase onto that branch is a trivial merge.

Facts the tasks rely on (all read from master):
- Rows of the omp page are items of a `gpui::list` (`render_tsp`, `tsp_page_row`, `tsp_view.rs` around 1290 and 1400). A row that is `RowPlan::Folded` returns an empty div; a `RowKind::Hidden` row (hidden node, or a column without shown children) also draws nothing.
- `tsp_node` returns an empty div for a hidden node, and `tsp_children` still lays that div out, so a flex `gap` would count it. Task 10 filters hidden children out.
- Chat Markdown (`preview::message`, `Flavor::Chat`) inherits `font_size::BODY` (13 px) from the pane root; only `doc::LINE_HEIGHT` is set by `tsp_md` (`tsp_view.rs:1987`) and by the Chat view (`chat_view.rs:609`). Part E will change `doc::LINE_HEIGHT` (docs 1.65) and `doc::HEADINGS` (docs 32/22/17/15), so Chat must stop reading both: Task 12 adds `doc::CHAT_LINE_HEIGHT`, `doc::CHAT_HEADINGS` and `doc::CHAT_HEADING_LINE_HEIGHT` and routes every chat call to them.
- `preview.rs` shares `block_in`, `list_item`, `lined_table`, `table_row`, `code_card`, `text` between Chat and Docs through `Ctx.flavor`. Every change below is behind `chat_flavor(ctx)` / a `chat` flag; the Docs branch keeps today's behavior, except `doc::CODE_LINE_HEIGHT` (shared, owned here). Part E (Tasks 17-20) edits the Docs branches of the same functions; where an `if chat { .. } else { .. }` is written here, E rewrites only the `else` arm.
- omp's roles (reference `tsp-reference-omp-18.13.2.md:307`, `tsp.rs:4310`): the prompt is `card` with `role: "omp.user"`, tool calls are `card` with `role: "omp.tool"` or `"omp.tool.<name>"`, thinking is `section role=omp.thinking`, an assistant turn is a `col` whose role starts with `omp.assistant` (spec §5). An omp pane cannot run in the smoke sandbox, so Tasks 10, 11 and 13 verify by clippy plus unit tests plus a HUMAN_CHECKS item (the coordinator's Task 26 writes them; each task lists its item). Task 12 is also visible in the Chat view of a fake agent, so it gets screenshots.

New tokens (all in `theme.rs`, doc-commented, owned by Part C): `TSP_ROW_GAP`, `TSP_HEAD_HEIGHT` (Task 10), `CHAT_PROMPT_MAX_WIDTH` (Task 11), `doc::CHAT_LINE_HEIGHT`, `doc::CHAT_HEADINGS`, `doc::CHAT_HEADING_LINE_HEIGHT`, `CODE_HEAD_HEIGHT`, `LINK_UNDERLINE_ALPHA` (Task 12).

---

### Task 10: Turn gap, assistant gap, thinking rows, working row

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (add `TSP_ROW_GAP`, `TSP_HEAD_HEIGHT` after `TSP_ASK_BAR_WIDTH`, line 453)
- Modify: `crates/mkrew-ui/src/tsp.rs` (add `row_gap` after `thought_plan`, `col_gap` after `gap_px`)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (functions `render_tsp` bottom spacer, `tsp_page_row`, `tsp_children`, `tsp_col_el`, `tsp_card_el`, `thinking_head_el`, `tsp_md`, `tsp_dock_status`, `tsp_dock_working`; new `parent_is`)
- Modify: `docs/SPEC.md` (the omp transcript paragraph, after the line starting `  A thinking head (\`omp.thinking\`)`, line 1324)
- Modify: `docs/decisions/0117-tern-polish.md` (bullet under `## Decision`)
- Test: `crates/mkrew-ui/src/tsp.rs` (new `mod gap_tests` at the end of the file)

**Interfaces:**
- Consumes: `tsp::RowKind`, `tsp::RowPlan`, `tsp::row_kind`, `tsp::thought_plan`, `tsp::gap_px`, `tsp::role_of`, `tsp::is_thinking` (all on master).
- Produces: `theme::TSP_ROW_GAP: f32` (16.0), `theme::TSP_HEAD_HEIGHT: f32` (24.0, used again by Task 13), `tsp::row_gap(kind: RowKind, plan: Option<&RowPlan>) -> f32`, `tsp::col_gap(role: Option<&str>, gap: Option<&str>) -> f32`, `fn parent_is(p: &Paint<'_>, node: &Node, want: fn(&Node) -> bool) -> bool` (private in `tsp_view.rs`, used by Tasks 11 and 12).

- [ ] **Step 1: Write the failing tests**

Append to the end of `crates/mkrew-ui/src/tsp.rs`:

```rust
#[cfg(test)]
mod gap_tests {
    use super::RowKind::{Hidden, Other, Thought};
    use super::*;
    use crate::theme::{TSP_ROW_GAP, space};

    #[test]
    fn rows_that_draw_nothing_leave_no_gap() {
        let kinds = [Thought(Some(1000)), Hidden, Thought(Some(1000)), Other];
        let ids = ["r0", "r1", "r2", "r3"];
        let plan = thought_plan(ids.iter().copied().zip(kinds), &HashSet::new());
        let gap = |i: usize| row_gap(kinds[i], plan.get(ids[i]));
        assert_eq!(gap(0), TSP_ROW_GAP, "the lead draws the group head");
        assert_eq!(gap(1), 0.0, "a hidden row");
        assert_eq!(gap(2), 0.0, "folded into the lead");
        assert_eq!(gap(3), TSP_ROW_GAP);

        let open = HashSet::from(["r0".to_owned()]);
        let plan = thought_plan(ids.iter().copied().zip(kinds), &open);
        assert_eq!(
            row_gap(kinds[2], plan.get("r2")),
            TSP_ROW_GAP,
            "an open run draws every member"
        );
    }

    #[test]
    fn an_assistant_column_gets_at_least_a_small_gap() {
        assert_eq!(col_gap(Some("omp.assistant"), None), space::S);
        assert_eq!(col_gap(Some("omp.assistant.turn"), Some("xs")), space::S);
        assert_eq!(col_gap(Some("omp.assistant"), Some("lg")), space::L, "never less than asked");
        assert_eq!(col_gap(Some("omp.tool.bash"), None), 0.0);
        assert_eq!(col_gap(None, Some("md")), space::M);
    }
}
```

- [ ] **Step 2: Run it, expect FAIL** (does not compile: `row_gap`, `col_gap`, `TSP_ROW_GAP` missing)

`cargo test -p mkrew-ui gap_tests -j 2`

- [ ] **Step 3: Implement**

`theme.rs`, after `pub const TSP_ASK_BAR_WIDTH: f32 = 2.0;`:

```rust
/// Space below every top-level row of an omp transcript that draws something (Tern's 20 px
/// turn gap on the 4/8/12/16/24/32 scale; `tsp::row_gap`).
pub const TSP_ROW_GAP: f32 = space::L;

/// Least height of a thinking row, a tool head and the working row (Tern's 24 px heads).
pub const TSP_HEAD_HEIGHT: f32 = space::XL;
```

`tsp.rs`, after `thought_plan` (before `/// Where a \`card\` or \`section\` gets its header`):

```rust
/// The space below a top-level row: [`theme::TSP_ROW_GAP`] when the row draws something, none
/// for a hidden row or a thought folded into its run's head (it draws nothing, so it must not
/// add a gap of its own). `plan` is the row's entry of [`thought_plan`].
pub fn row_gap(kind: RowKind, plan: Option<&RowPlan>) -> f32 {
    if kind == RowKind::Hidden || matches!(plan, Some(RowPlan::Folded)) {
        0.0
    } else {
        theme::TSP_ROW_GAP
    }
}
```

`tsp.rs`, after `gap_px`:

```rust
/// The gap of a `col`: its `gap` token, and at least `space::S` inside an assistant turn
/// (role `omp.assistant*`), where omp sends thinking, text and tool rows with no gap.
pub fn col_gap(role: Option<&str>, gap: Option<&str>) -> f32 {
    let gap = gap_px(gap);
    if role.is_some_and(|role| role.starts_with("omp.assistant")) {
        gap.max(space::S)
    } else {
        gap
    }
}
```

`tsp_view.rs`:

1. `render_tsp`, the list closure (line 1291-1295). The rows now carry their own gap, so the bottom spacer only parts the last row from the dock:

```rust
                                // The top spacer keeps the first row clear of the drag strip;
                                // the bottom one is what is left of the dock gap (every row
                                // that draws something carries `TSP_ROW_GAP` below it).
                                if ix == 0 {
                                    return div().h(px(space::L)).into_any_element();
                                }
                                if ix > count {
                                    return div().h(px(space::S)).into_any_element();
                                }
```

2. `tsp_page_row`: replace the `let el = match ui.thoughts.get(&node.id) { .. }` header and the last line.

```rust
        let plan = ui.thoughts.get(&node.id);
        let el = match plan {
            // Folded into the lead's head: the row draws nothing, and has no gap.
            Some(tsp::RowPlan::Folded) => return div().into_any_element(),
            Some(tsp::RowPlan::Lead { label, open }) => {
                // ... unchanged ...
            }
            None => self.tsp_node(&paint, node, cx),
        };
        let gap = tsp::row_gap(tsp::row_kind(doc, node), plan);
        page_column(el, max, false).pb(px(gap)).into_any_element()
```

3. New helper above `impl Shell` blocks (next to `thinking_head_el` is fine):

```rust
/// Whether `node`'s parent satisfies `want` (a role test such as `tsp::is_thinking`).
fn parent_is(p: &Paint<'_>, node: &Node, want: fn(&Node) -> bool) -> bool {
    node.parent
        .as_deref()
        .and_then(|id| p.doc.get(id))
        .is_some_and(want)
}
```

4. `tsp_children`: skip hidden children, so a flex `gap` is not spent on an empty div:

```rust
        p.doc
            .children(&node.id)
            .filter(|child| !bool_prop(child, "hidden"))
            .map(|child| self.tsp_node(p, child, cx))
            .collect()
```

5. `tsp_col_el`: `let gap = px(tsp::col_gap(tsp::role_of(node), str_prop(node, "gap")));` (replaces the `tsp::gap_px` line; `tsp_row_el` keeps `gap_px`).

6. `tsp_card_el`: the open body of a thinking section is indented and italic. Replace the last two lines (`let el = Self::tsp_flow(el, node).children(head_el).children(body);`):

```rust
        let el = Self::tsp_flow(el, node).children(head_el);
        let el = if tsp::is_thinking(node) && !body.is_empty() {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .min_w_0()
                    .pl(px(space::L))
                    .italic()
                    .children(body),
            )
        } else {
            el.children(body)
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
```

7. `tsp_md`: the text of a thinking body is `text_subtle`. Preview paragraphs set their own color, so the node is drawn with a theme whose `text` is the subtle tier. Insert after `let theme = p.theme;`:

```rust
        // A thinking body is `text_subtle`: paragraphs take `theme.text`, so give them that tier.
        // ponytail: one Theme clone per painted thinking body; a Flavor variant if it shows up in a profile.
        let thought_theme;
        let theme = if parent_is(p, node, tsp::is_thinking) {
            thought_theme = Theme {
                text: p.theme.text_subtle,
                ..p.theme.clone()
            };
            &thought_theme
        } else {
            p.theme
        };
```

8. `thinking_head_el`: BODY (not META), 24 px min height, `text_subtle` resting and `text` on hover; update its doc comment.

```rust
/// The head of an `omp.thinking*` section, or of a run of them: BODY italic `text_subtle`
/// (`text` on hover), 24 px at least, with the chevron after the label. `collapsed`: the
/// chevron's state, or `None` without one. The caller adds the press.
fn thinking_head_el(
    theme: &Theme,
    key: String,
    label: String,
    collapsed: Option<bool>,
) -> gpui::Stateful<Div> {
    let group = SharedString::from(key.clone());
    let chevron = collapsed.map(|collapsed| {
        let icon = if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        icon_svg(icon, theme::GROUP_CHEVRON_SIZE, theme.text_subtle)
            .group_hover(group.clone(), |style| style.text_color(rgb(theme.text)))
    });
    div()
        .id(SharedString::from(key))
        .group(group.clone())
        .self_start()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .min_h(px(theme::TSP_HEAD_HEIGHT))
        .text_size(font_size::BODY)
        .italic()
        .child(
            div()
                .text_color(rgb(theme.text_subtle))
                .group_hover(group, |style| style.text_color(rgb(theme.text)))
                .child(label),
        )
        .children(chevron)
        .pressable(Look::Text, true, theme)
}
```

9. `tsp_dock_status`: drop `.px(px(space::S))` (the label now lines up with the transcript's text column: the dock sits in the same `chat_column` padding); `tsp_dock_working`: 24 px row, label MEDIUM:

```rust
        div()
            .flex()
            .flex_1()
            .items_center()
            .gap(px(space::S))
            .min_w_0()
            .min_h(px(theme::TSP_HEAD_HEIGHT))
            .text_size(font_size::META)
            // ... dots, elapsed, divider unchanged ...
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child(w.label.clone()),
            )
```

   Note: `tsp_dock_status` is also read by Part D (composer card). Task 10 only deletes the one `px` line there.
   ponytail: the dock keeps the Chat column (spec §6), so with several panes a pane wider than 760 aligns the working row with the dock, not the pane-wide transcript; alone and below 760 they are the same column. Upgrade: pass `max` into `tsp_dock_status` and wrap in `page_column`.

10. SPEC: insert after the line starting `  Top-level rows that show only one collapsed thinking section` (1325):

```
  Every top-level row that draws something has `TSP_ROW_GAP` (16 px) below it; a hidden row and a thought folded into its run's head add none. A `col` with role `omp.assistant*` has at least `space::S` between its children. A thinking head is BODY italic `text_subtle` (`text` on hover), at least 24 px high; its open body is italic `text_subtle`, indented by `space::L`. The working row is 24 px high, its label MEDIUM `text_muted`, in the text column (ADR 0117).
```
   and change the earlier sentence `A thinking head (\`omp.thinking\`) is META italic \`text_subtle\` with its chevron after the label.` to `A thinking head (\`omp.thinking\`) has its chevron after the label.`

   ADR 0117, under `## Decision`, add:
```
- **Transcript rhythm (omp pane).** 16 px below every top-level row that draws something (`TSP_ROW_GAP`), at least 8 px inside an assistant turn, thinking rows 24 px high in BODY italic `text_subtle`, the working row 24 px in the text column.
```

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui gap_tests -j 2`, then `cargo test -p mkrew-ui thought_tests -j 2` (the existing plan tests must stay green), then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

   The omp pane cannot run in the smoke sandbox: HUMAN_CHECKS item (Task 26 writes it): "omp pane, dark and light: 16 px between turns, no extra space after folded or hidden rows, 8 px between thinking, text and tool rows of one turn, thinking rows 24 px tall, the working row's label starts at the text's left edge."

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/tsp.rs crates/mkrew-ui/src/shell/tsp_view.rs docs/SPEC.md docs/decisions/0117-tern-polish.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): omp transcript turn gap, assistant gap, 24 px thinking and working rows"
```

---

### Task 11: User prompt bubble (omp pane and Chat view)

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (add `CHAT_PROMPT_MAX_WIDTH` after `CHAT_PROMPT_MAX_FRACTION`, line 473)
- Modify: `crates/mkrew-ui/src/tsp.rs` (add `is_user_prompt` after `is_thinking`)
- Modify: `crates/mkrew-ui/src/shell/chat_view.rs` (new `prompt_bubble` next to `chat_column`; `chat_turn` prompt, lines 683-693)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (`tsp_card_el`, new `tsp_user_prompt`, `tsp_card_head_row`; import `prompt_bubble`, line 28)
- Modify: `docs/SPEC.md`, `docs/decisions/0117-tern-polish.md`

**Interfaces:**
- Consumes: Task 10's `parent_is` is not needed here; Part A's `Theme.layer[1]`; `theme::CHAT_PROMPT_MAX_FRACTION`, `theme::UI_LINE_HEIGHT`, `radius::LARGE`, `radius::SMALL`, `theme::opacity::{HIDDEN, FULL}`.
- Produces: `theme::CHAT_PROMPT_MAX_WIDTH: f32` (540.0), `tsp::is_user_prompt(node: &Node) -> bool`, `pub(crate) fn prompt_bubble(body: impl IntoElement, head: Option<(gpui::SharedString, gpui::AnyElement)>, theme: &Theme) -> gpui::Div` in `shell/chat_view.rs` (Tasks 12 and 13 do not use it again).

No unit test: this task is layout wiring and role matching only (spec §13 asks for tests of logic, not wiring). It ends with clippy and a HUMAN_CHECKS item; the Chat view half is covered by the Task 12 screenshots (the fake agent's prompt shows the bubble).

- [ ] **Step 1: Implement the token and the role test**

`theme.rs`, after `CHAT_PROMPT_MAX_FRACTION`:

```rust
/// Widest a prompt bubble is, whatever the pane (Tern's 540 px; the bubble is also at most
/// `CHAT_PROMPT_MAX_FRACTION` of the column).
pub const CHAT_PROMPT_MAX_WIDTH: f32 = 540.0;
```

`tsp.rs`, after `is_thinking`:

```rust
/// The user's prompt: `card` with role `omp.user`.
pub fn is_user_prompt(node: &Node) -> bool {
    role_of(node) == Some("omp.user")
}
```

- [ ] **Step 2: The shared bubble** (`chat_view.rs`, after `chat_column`)

```rust
/// The user's prompt as Tern's bubble: at the right, at most `CHAT_PROMPT_MAX_FRACTION` of the
/// column and `CHAT_PROMPT_MAX_WIDTH` px wide, `layer[1]` fill, radii 12/12/4/12. `head`: a
/// group name and the row (copy, rewind, time of an omp prompt) drawn under the bubble, right
/// aligned in META `text_subtle`, visible only while the pointer is over the prompt.
pub(crate) fn prompt_bubble(
    body: impl IntoElement,
    head: Option<(gpui::SharedString, gpui::AnyElement)>,
    theme: &Theme,
) -> gpui::Div {
    let bubble = div()
        .max_w(px(theme::CHAT_PROMPT_MAX_WIDTH))
        .px(px(space::L))
        .py(px(space::S))
        .rounded(px(radius::LARGE))
        .rounded_br(px(radius::SMALL))
        .bg(gpui::rgba(theme.layer[1]))
        .text_size(font_size::BODY)
        .line_height(relative(theme::UI_LINE_HEIGHT))
        .child(body);
    let column = div()
        .max_w(relative(theme::CHAT_PROMPT_MAX_FRACTION))
        .min_w_0()
        .flex()
        .flex_col()
        .items_end()
        .child(bubble);
    let column = match head {
        Some((group, head)) => column.group(group.clone()).child(
            div()
                .pt(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_subtle))
                .opacity(theme::opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(theme::opacity::FULL))
                .child(head),
        ),
        None => column,
    };
    div().w_full().flex().justify_end().child(column)
}
```

   `chat_turn` (`chat_view.rs:683-693`), replace the whole `.child(div().flex().justify_end().child(div().max_w(..)...))` with:

```rust
            .child(prompt_bubble(turn.prompt.to_owned(), None, theme))
```
   (`radius`, `font_size`, `relative` already come in through `use crate::*`; if `font_size` is not in scope there, it is used at `chat_view.rs:381`, so it is.)

- [ ] **Step 3: The omp prompt** (`tsp_view.rs`)

Import: `use super::chat_view::{chat_column, page_column, prompt_bubble};`

`tsp_card_el`: first lines after `let card = node.kind == "card";`:

```rust
        if card && !p.in_layer && tsp::is_user_prompt(node) {
            return self.tsp_user_prompt(p, node, head_el, body, cx);
        }
```

New method (same `impl Shell` block):

```rust
    /// The prompt card of the user: Tern's right-aligned bubble; its head row (copy, rewind,
    /// time) leaves the bubble and shows under it while the row is hovered.
    fn tsp_user_prompt(
        &self,
        p: &Paint<'_>,
        node: &Node,
        head_el: Option<AnyElement>,
        body: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("tsp-user-{}-{}", p.session, node.id));
        let body = div().flex().flex_col().gap(px(space::S)).min_w_0().children(body);
        let bubble = prompt_bubble(body, head_el.map(|head| (group, head)), p.theme);
        self.tsp_decorate(bubble, p, node, Look::Text, false, cx)
    }
```

`tsp_card_head_row`: the prompt's head is META `text_subtle`, regular weight, right aligned. Replace the three spots:

```rust
        let user = tsp::is_user_prompt(node);
        let head_color = if user {
            theme.text_subtle
        } else {
            str_prop(node, "tone").map_or(theme.text, |tone| tsp::tone_color(tone, theme))
        };
```
```rust
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .group(group.clone());
        if user {
            row = row.justify_end();
        }
```
```rust
            None => row.child(
                div()
                    .when(!user, |el| el.font_weight(FontWeight(weight::SEMIBOLD)))
                    .child(styled_runs(head, head_color, false, theme)),
            ),
```
(the META size comes from the wrapper in `prompt_bubble`.)

SPEC: after the Task 10 paragraph add:
```
  The prompt (`card` role `omp.user`) is a right-aligned bubble in both the omp pane and the Chat view: at most 80 % of the column and 540 px (`CHAT_PROMPT_MAX_WIDTH`), `py(S) px(L)`, radii 12/12/4/12, `layer[1]` fill, BODY at line height 1.5. omp's head row (copy, rewind, time) shows under the bubble, right aligned in META `text_subtle`, while the prompt is hovered.
```
and in the ADR `## Decision` list: `- **Prompt bubble.** Tern's right-aligned bubble (\`prompt_bubble\`) in the omp pane and the Chat view; omp's head row moves under it on hover.`

- [ ] **Step 4: Run**

`cargo clippy -p mkrew-ui --all-targets -- -D warnings`, `cargo test -p mkrew-ui tsp -j 2`.
HUMAN_CHECKS item: "omp pane: the prompt is a right-aligned rounded bubble (square bottom-right corner), at most 540 px, its copy / rewind / time row appears under it only on hover; the head row's buttons still work. Chat view: same bubble."

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/tsp.rs crates/mkrew-ui/src/shell/chat_view.rs crates/mkrew-ui/src/shell/tsp_view.rs docs/SPEC.md docs/decisions/0117-tern-polish.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): right-aligned prompt bubble in the omp pane and the Chat view"
```

---

### Task 12: Chat Markdown flavor (line height, headings, lists, tables, code card, inline code, links, quote)

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`mod doc`: `CODE_LINE_HEIGHT` 1.45 to 1.6, add `CHAT_LINE_HEIGHT`, `CHAT_HEADING_LINE_HEIGHT`, `CHAT_HEADINGS`; top level: `CODE_HEAD_HEIGHT`, `LINK_UNDERLINE_ALPHA`)
- Modify: `crates/mkrew-ui/src/preview.rs` (`Ctx`, `blocks`, `message`, `page`, `block`, new `heading_metrics`, `block_in`, `list_item`, `lined_table`, `table_row`, `code_card`, `text`)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (`tsp_md`), `crates/mkrew-ui/src/shell/chat_view.rs` (line 609)
- Modify: `docs/SPEC.md`, `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/preview.rs` (`mod tests`)

**Interfaces:**
- Consumes: Part A `Theme.layer`, `Theme.code_bg`; `theme::with_alpha`; Task 10's `parent_is`; Task 11's `tsp::is_user_prompt`.
- Produces: `theme::doc::CHAT_LINE_HEIGHT` (1.6), `theme::doc::CHAT_HEADING_LINE_HEIGHT` (1.35), `theme::doc::CHAT_HEADINGS: [f32; 6]`, `theme::CODE_HEAD_HEIGHT` (32.0), `theme::LINK_UNDERLINE_ALPHA` (0.4), `preview::heading_metrics(level: u8, chat: bool) -> (f32, f32)` (private), `Ctx::new(page, flavor)`, `code_card(.., chat: bool, ..)`.

Coordination with Part E: this task changes the Docs flavor in exactly one way (`doc::CODE_LINE_HEIGHT` 1.6, spec §7). Everything else is behind `chat`. E's heading, bullet, table, quote, code and inline-code edits replace the `else` arms written here.

- [ ] **Step 1: Write the failing test** (`preview.rs`, inside `mod tests`)

```rust
    #[test]
    fn chat_headings_shrink_with_the_level_and_out_of_range_levels_clamp() {
        for chat in [true, false] {
            let sizes: Vec<f32> = (1..=6).map(|level| heading_metrics(level, chat).0).collect();
            assert!(sizes.windows(2).all(|w| w[0] >= w[1]), "chat={chat}: {sizes:?}");
            assert_eq!(heading_metrics(0, chat), heading_metrics(1, chat));
            assert_eq!(heading_metrics(9, chat), heading_metrics(6, chat));
        }
        // Chat keeps its own scale and line height: the Docs ones may grow without it.
        let (size, line) = heading_metrics(1, true);
        assert_eq!(size, theme::doc::CHAT_HEADINGS[0]);
        assert_eq!(line, theme::doc::CHAT_HEADING_LINE_HEIGHT);
    }
```

- [ ] **Step 2: Run it, expect FAIL** (does not compile)

`cargo test -p mkrew-ui chat_headings_shrink -j 2`

- [ ] **Step 3: Implement**

**theme.rs.** In `mod doc` change and add:

```rust
    /// Line heights as multiples of the font size.
    pub const LINE_HEIGHT: f32 = 1.5;
    pub const HEADING_LINE_HEIGHT: f32 = 1.3;
    /// Code blocks, Chat and Docs (ADR 0117: 1.45 to 1.6).
    pub const CODE_LINE_HEIGHT: f32 = 1.6;
    /// Markdown in the omp transcript and the Chat view (ADR 0117; ADR 0116's 1.5 stays for the UI).
    pub const CHAT_LINE_HEIGHT: f32 = 1.6;
    pub const CHAT_HEADING_LINE_HEIGHT: f32 = 1.35;
    /// h1-h6 of a chat message: one step under the Docs sizes, kept apart from `HEADINGS`.
    pub const CHAT_HEADINGS: [f32; 6] = [20.0, 17.0, 15.0, 14.0, 14.0, 14.0];
```
Top level, after `DOC_QUOTE_BAR_WIDTH` (line 576):
```rust
/// Height of a code card's head row (language and Copy), Tern's 32 px.
pub const CODE_HEAD_HEIGHT: f32 = space::XXL;

/// Alpha of the accent underline under a link in a chat message.
pub const LINK_UNDERLINE_ALPHA: f32 = 0.4;
```

**Line height call sites** (Chat stops reading `doc::LINE_HEIGHT`):
- `chat_view.rs:609`: `.line_height(relative(theme::doc::CHAT_LINE_HEIGHT))`
- `tsp_view.rs` `tsp_md` (line 1987): the prompt bubble keeps the UI's 1.5.
```rust
        let line_height = if parent_is(p, node, tsp::is_user_prompt) {
            theme::UI_LINE_HEIGHT
        } else {
            theme::doc::CHAT_LINE_HEIGHT
        };
        ...
            .line_height(relative(line_height))
```

**preview.rs.**

1. `Ctx` (line 64) and constructor; the four literal sites (`blocks` line 100, `message` line 136, `page` line 627, `block` line 679) become `Ctx::new(false, flavor)`, `Ctx::new(false, flavor)`, `Ctx::new(true, flavor)`, `Ctx::new(false, Flavor::Doc)`:

```rust
#[derive(Debug, Clone, Copy)]
struct Ctx {
    page: bool,
    flavor: Flavor,
    /// Inside a list item (Chat: a nested item gets a ring bullet).
    nested: bool,
    /// Chat: paragraphs in `text_muted` (nested list items, quotes).
    muted: bool,
}

impl Ctx {
    fn new(page: bool, flavor: Flavor) -> Self {
        Self {
            page,
            flavor,
            nested: false,
            muted: false,
        }
    }
}
```

2. `heading_metrics` (above `block_in`) and the heading arm:

```rust
/// The size and line height of a heading of `level` (clamped to 1..=6). A chat message has its
/// own scale ([`theme::doc::CHAT_HEADINGS`]) so the Docs one can change on its own.
fn heading_metrics(level: u8, chat: bool) -> (f32, f32) {
    let ix = usize::from(level).saturating_sub(1);
    if chat {
        let last = theme::doc::CHAT_HEADINGS.len() - 1;
        (
            theme::doc::CHAT_HEADINGS[ix.min(last)],
            theme::doc::CHAT_HEADING_LINE_HEIGHT,
        )
    } else {
        let last = theme::doc::HEADINGS.len() - 1;
        (
            theme::doc::HEADINGS[ix.min(last)],
            theme::doc::HEADING_LINE_HEIGHT,
        )
    }
}
```
```rust
        BlockKind::Heading { level, runs } => {
            let chat = chat_flavor(ctx);
            // Chat headings: `text`, no rule, `XS` above (plus the block gap), the block gap below.
            let (size, line_height) = heading_metrics(*level, chat);
            let major = *level <= 2 && !chat;
            div()
                .pt(px(if chat {
                    space::XS
                } else if major {
                    space::L
                } else {
                    space::S
                }))
                .text_size(px(size))
                .line_height(relative(line_height))
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .when(major, |el| {
                    el.pb(px(space::XS))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                })
                .child(selectable(pick, links(runs, ctx), |sel, bg| {
                    text(runs, theme, sel, bg, chat_flavor(ctx))
                }))
                .into_any_element()
        }
```

3. Paragraph color: `.text_color(rgb(if ctx.muted { theme.text_muted } else { theme.text }))`.

4. List arm: `.gap(px(if chat_flavor(ctx) { space::S } else { space::XS }))`.

5. Quote arm: at the top `let chat = chat_flavor(ctx);`; bar fill `look.map_or(if chat { theme.border } else { theme.pane_focus_stripe }, |(_, _, color)| color)`; text `.text_color(rgb(if chat { theme.text_muted } else { theme.text }))`; children `.children(blocks_in(blocks, shown, pick, Ctx { muted: ctx.muted || chat, ..ctx }, theme))`.

6. Code arm: both `code_card(..)` calls get `chat_flavor(ctx)` before `theme`: `code_card(lang.as_deref(), body, text, beside_edit, chat_flavor(ctx), theme)` and `code_card(Some(&label), body, text, beside_edit, chat_flavor(ctx), theme)`.

7. `list_item`: `let chat = chat_flavor(ctx);` at the top. Bullet arm `(None, None)`:

```rust
        (None, None) => {
            let dot = div()
                .mt(px(space::S))
                .size(px(theme::doc::BULLET_SIZE))
                .rounded(px(radius::PILL));
            if !chat {
                dot.bg(rgb(theme.accent)) // Docs: Part E
            } else if ctx.nested {
                dot.border_1().border_color(rgb(theme.text_subtle))
            } else {
                dot.bg(rgb(theme.text_subtle))
            }
            .into_any_element()
        }
```
   item column: `.gap(px(if chat { space::S } else { space::XS }))` and
   `.children(blocks_in(&item.blocks, shown, pick, Ctx { nested: true, muted: ctx.muted || (chat && ctx.nested), ..ctx }, theme))`.

8. `lined_table` and `table_row`:

```rust
fn lined_table(cols: Cols<'_>, head: &Row, rows: &[Row], pick: Option<&Pick>, ctx: Ctx, theme: &Theme) -> gpui::Div {
    let chat = chat_flavor(ctx);
    div()
        .flex()
        .flex_col()
        .child(
            table_row(head, cols, pick, ctx, true, theme.text_muted, theme)
                .font_weight(FontWeight(weight::MEDIUM))
                .when(chat, |el| el.text_size(font_size::META))
                .border_b_1()
                .border_color(rgb(if chat { theme.border } else { theme.divider })),
        )
        .children(rows.iter().enumerate().map(|(i, row)| {
            table_row(row, cols, pick, ctx, true, theme.text, theme)
                .when(i > 0, |el| el.border_t_1().border_color(rgb(theme.divider)))
        }))
}
```
   in `table_row`: replace `.py(px(space::S))` with `.py(px(if chat { space::XS } else { space::S }))`.

9. `code_card` (add `chat: bool` before `theme`; update its doc comment to say Chat is a sunken `code_bg` card with a `layer[1]` ring, a 32 px head with a `divider` line under it and a text-only Copy):

```rust
fn code_card(
    lang: Option<&str>,
    body: gpui::Div,
    text: &str,
    beside_edit: bool,
    chat: bool,
    theme: &Theme,
) -> gpui::Div {
    let right = if beside_edit {
        space::XS + theme::ICON_BUTTON_SIZE + space::XS
    } else {
        space::XS
    };
    // Hashed only while a "Copied" mark exists, not for every block on every render.
    let done = copied().is_some_and(|key| key == code_key(text));
    let copy_text = text.to_owned();
    let label = if chat { theme.text_subtle } else { theme.text_muted };
    let copy = div()
        .id(("code-copy", text.as_ptr() as usize))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::META)
        .text_color(rgb(label))
        .pressable(if chat { Look::Text } else { Look::Raised }, true, theme)
        .when(!chat, |el| {
            el.child(icons::icon_svg(
                if done { Icon::Check } else { Icon::Copy },
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
        })
        .child(if done { "Copied" } else { "Copy" })
        // Not a press on the page (text selection) or a pick of the block.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_press(true, move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(Box::new(CopyCode(copy_text.clone())), cx);
        });
    div()
        .flex()
        .flex_col()
        .border(px(theme::BORDER_WIDTH))
        .when(chat, |el| {
            el.rounded(px(radius::LARGE))
                .bg(rgb(theme.code_bg))
                .border_color(gpui::rgba(theme.layer[1]))
        })
        .when(!chat, |el| {
            el.rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .border_color(rgb(theme.divider))
        })
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(space::S))
                .pl(px(space::M))
                .pr(px(right))
                .when(chat, |el| {
                    el.h(px(theme::CODE_HEAD_HEIGHT))
                        .border_b_1()
                        .border_color(rgb(theme.divider))
                })
                .when(!chat, |el| el.py(px(space::XS)))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(font_size::META)
                        .text_color(rgb(label))
                        .child(lang.filter(|l| !l.is_empty()).unwrap_or("text").to_owned()),
                )
                .child(copy),
        )
        .child(body.p(px(space::M)))
}
```
   The code lines already read `doc::CODE_LINE_HEIGHT` (now 1.6) in `code_lines` and `plain_lines`: no change there.

10. `text` (inline code and links); update its doc comment ("code spans on a `layer[0]` fill in Chat, links `text` with a 1 px accent underline at 40 % in Chat, accent colored and underlined in Docs"):

```rust
    let accent = Hsla::from(rgb(theme.accent));
    let muted = Hsla::from(rgb(theme.text_muted));
    // Chat: `text` links under a quiet accent line, inline code on `layer[0]`. Docs: Part E.
    let link_color = if chat { Hsla::from(rgb(theme.text)) } else { accent };
    let link_line = if chat {
        Hsla::from(gpui::rgba(theme::with_alpha(theme.accent, theme::LINK_UNDERLINE_ALPHA)))
    } else {
        accent
    };
    let code_bg = Hsla::from(if chat {
        gpui::rgba(theme.layer[0])
    } else {
        rgb(theme.surface_raised)
    });
```
   and in `base`: `color: link.then_some(link_color)`, `underline: link.then_some(UnderlineStyle { thickness: px(theme::BORDER_WIDTH), color: Some(link_line), wavy: false })`.

**SPEC and ADR.** `docs/SPEC.md` line 1290: replace `the agent's text has accent headings and bullet dots, inline code and file paths are pills` with `the agent's text has \`text\` headings and \`text_subtle\` bullets, inline code and file paths are pills`, and `code blocks are cards with a language label and a Copy button` with `code blocks are sunken cards (\`code_bg\`, a \`layer[1]\` ring) with a 32 px head holding the language and a text-only Copy`. Insert after the Task 11 paragraph:
```
  Chat Markdown: line height 1.6 (`doc::CHAT_LINE_HEIGHT`), headings `text` SEMIBOLD at 1.35 on their own scale (20/17/15/14), list items `space::S` apart with a `text_subtle` dot (a nested item has a ring bullet and `text_muted` text), table cells `py(XS)` with a META MEDIUM `text_muted` head over a `border` line and `divider` lines between rows, inline code on `layer[0]`, links in `text` under a 1 px accent line at 40 %, quote bar `border` with `text_muted` text. Code lines are 13 px at 1.6 in Chat and Docs (`doc::CODE_LINE_HEIGHT`).
```
ADR 0117 `## Decision`: `- **Chat Markdown.** Line height 1.6 for chat only (amends ADR 0116's 1.5; the UI stays 1.5), \`text\` headings and \`text_subtle\` bullets (amends ADR 0105's accent headings and dots), code cards sunken on \`code_bg\`, code lines 1.6 everywhere.`

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui chat_headings_shrink -j 2`, `cargo test -p mkrew-ui preview -j 2`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. No snapshot pins these values (the insta snapshots are theme palettes and markdown block trees).

   Screenshots (smoke sandbox; read the header of `scripts/smoke.ps1`): build `cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins`; `powershell -File scripts/smoke.ps1 start -Theme dark`; run the fake agent with `smoke.ps1 mk ...` (profile `fake`), open its Chat view (`smoke.ps1 keys`), send a prompt whose answer holds a heading, a nested list, a table, a code block, inline code and a link; `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-chat-dark.png`; `smoke.ps1 stop`; repeat with `-Theme light` and `-Out docs/screenshots/tern-polish-chat-light.png`. If the fake agent cannot produce that Markdown, say so in the task notes and make it a HUMAN_CHECKS item (omp pane and Chat view: the same list).

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/shell/chat_view.rs docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-chat-dark.png docs/screenshots/tern-polish-chat-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Tern Markdown for chat messages, code lines at 1.6"
```

---

### Task 13: Tool rows without a box

**Files:**
- Modify: `crates/mkrew-ui/src/tsp.rs` (add `is_tool` after `is_user_prompt`)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (`tsp_card`, `tsp_card_el`, `tsp_card_head_row`; new `tool_well`)
- Modify: `docs/SPEC.md`, `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/tsp.rs` (`mod gap_tests`, from Task 10)

**Interfaces:**
- Consumes: `theme::TSP_HEAD_HEIGHT` (Task 10), `tsp::is_user_prompt` and the `user` branch of `tsp_card_head_row` (Task 11), `Theme.code_bg`, `Theme.layer[0]`, `Look::Row` (Part B hover fill `layer[0]`).
- Produces: `tsp::is_tool(node: &Node) -> bool`.

- [ ] **Step 1: Write the failing test** (append to `mod gap_tests` in `tsp.rs`; add `use serde_json::json;` to the module)

```rust
    #[test]
    fn only_tool_roles_lose_their_box() {
        let node = |role: &str| {
            let mut d = Document::new("s:1");
            let tree = json!({"id": "main", "k": "col", "c": [
                {"id": "c", "k": "card", "p": {"role": role}},
            ]});
            let applied = d.apply(&[json!(["add", "main", "s:1", null, tree])]);
            assert!(applied.errors.is_empty(), "{:?}", applied.errors);
            is_tool(d.get("c").unwrap())
        };
        assert!(node("omp.tool"));
        assert!(node("omp.tool.edit"));
        assert!(!node("omp.toolbar"));
        assert!(!node("omp.welcome"));
        assert!(!node("omp.user"));
    }
```

- [ ] **Step 2: Run it, expect FAIL** (does not compile: `is_tool`)

`cargo test -p mkrew-ui only_tool_roles -j 2`

- [ ] **Step 3: Implement**

`tsp.rs`, after `is_user_prompt`:

```rust
/// A tool call: `card` with role `omp.tool` or `omp.tool.<name>`.
pub fn is_tool(node: &Node) -> bool {
    role_of(node).is_some_and(|role| role == "omp.tool" || role.starts_with("omp.tool."))
}
```

`tsp_view.rs`:

1. New free function (next to `thinking_head_el`): the code, diff and ANSI body of an open tool row sits on `code_bg` with a `layer[0]` ring.

```rust
/// An `ansi` or `rows` body of a tool row (command output, a diff): `code_bg` with a `layer[0]`
/// ring, `radius::MEDIUM`, 8 by 12 padding. A `code` node is a code card already.
fn tool_well(body: AnyElement, theme: &Theme) -> AnyElement {
    div()
        .min_w_0()
        .overflow_hidden()
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(theme::radius::MEDIUM))
        .bg(rgb(theme.code_bg))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[0]))
        .child(body)
        .into_any_element()
}
```

2. `tsp_card` body (replace the `.map(|child| self.tsp_node(p, child, cx))` of the last `else` branch):

```rust
            let tool = tsp::is_tool(node);
            (p.doc.children(&node.id))
                .filter(|child| head_child.is_none_or(|head| head.id != child.id))
                .map(|child| {
                    let el = self.tsp_node(p, child, cx);
                    if tool && matches!(child.kind.as_str(), "ansi" | "rows") {
                        tool_well(el, p.theme)
                    } else {
                        el
                    }
                })
                .collect()
```

3. `tsp_card_el`, final form (Tasks 10 and 11 pieces kept; the box is skipped for tools, so `omp.welcome` and the overlays keep theirs):

```rust
    fn tsp_card_el(
        &self,
        p: &Paint<'_>,
        node: &Node,
        head_el: Option<AnyElement>,
        body: Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let card = node.kind == "card";
        if card && !p.in_layer && tsp::is_user_prompt(node) {
            return self.tsp_user_prompt(p, node, head_el, body, cx);
        }
        let mut el = div().flex().flex_col().gap(px(space::S)).min_w_0();
        if p.bounded {
            // The pane (or the overlay container) bounds this card: its list or body scrolls.
            el = el.overflow_hidden();
        }
        // A tool row is plain rows on the page (Tern); other cards keep their raised frame.
        if !p.in_layer && card && !tsp::is_tool(node) {
            el = el
                .p(px(space::M))
                .rounded(px(theme::radius::MEDIUM))
                .bg(rgb(theme.surface_raised));
        }
        let el = Self::tsp_flow(el, node).children(head_el);
        let el = if tsp::is_thinking(node) && !body.is_empty() {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .min_w_0()
                    .pl(px(space::L))
                    .italic()
                    .children(body),
            )
        } else {
            el.children(body)
        };
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
```
   The body keeps `gap(space::S)` under the head, which is Tern's `pt(S)`.

4. `tsp_card_head_row`, final form. The tool head is 24 px high, `px(XS)` inset with a matching `-XS` margin (so the label stays on the text edge) and a `radius::SMALL` hover fill; the chevron is `text_subtle` and shows on hover; the title is MEDIUM `text`.

```rust
    #[inline(never)]
    fn tsp_card_head_row(
        p: &Paint<'_>,
        node: &Node,
        head: &[(String, SpanStyle)],
        child: Option<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = p.theme;
        let collapsible = bool_prop(node, "collapsible");
        let collapsed = bool_prop(node, "collapsed");
        let user = tsp::is_user_prompt(node);
        let tool = tsp::is_tool(node);
        let head_color = if user {
            theme.text_subtle
        } else {
            str_prop(node, "tone").map_or(theme.text, |tone| tsp::tone_color(tone, theme))
        };
        let group = SharedString::from(format!("tsp-hg-{}-{}", p.session, node.id));
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .group(group.clone());
        if user {
            row = row.justify_end();
        }
        if tool {
            row = row
                .min_h(px(theme::TSP_HEAD_HEIGHT))
                .mx(px(-space::XS))
                .px(px(space::XS))
                .rounded(px(theme::radius::SMALL));
        }
        if collapsible {
            let chevron = if collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            let color = if tool { theme.text_subtle } else { theme.text_muted };
            let mark = icon_svg(chevron, theme::ICON_SIZE_SMALL, color);
            // A tool card's chevron shows while its head is hovered.
            row = if tool {
                row.child(
                    mark.opacity(theme::opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(theme::opacity::FULL)),
                )
            } else {
                row.child(mark)
            };
        }
        let row = match child {
            Some(child) => row.child(child),
            None if tool => row.child(styled_runs_w(
                head,
                head_color,
                false,
                weight::MEDIUM,
                theme,
            )),
            None => row.child(
                div()
                    .when(!user, |el| el.font_weight(FontWeight(weight::SEMIBOLD)))
                    .child(styled_runs(head, head_color, false, theme)),
            ),
        };
        if !collapsible {
            return row.into_any_element();
        }
        let (session, id) = (p.session, node.id.clone());
        row.id(SharedString::from(format!(
            "tsp-head-{}-{}",
            p.session, node.id
        )))
        .pressable(if tool { Look::Row } else { Look::Text }, true, theme)
        .on_press(
            true,
            cx.listener(move |shell, _, _, cx| {
                if let Some(event) = tsp::toggle_event(&id, !collapsed) {
                    shell.tsp_emit(session, event, cx);
                }
            }),
        )
        .into_any_element()
    }
```
   (Only `.group(group.clone())` clones the name; the `group` moved into `group_hover` is the last use, as in master.) The old `starts_with("omp.tool.")` test is gone: `is_tool` replaces it, so a bare `omp.tool` card behaves the same as `omp.tool.<name>`.
   [INFERENCE] The title / target / meta split of Tern (title MEDIUM `text`, target `text`, meta in the terminal font META `text_subtle`) comes from omp's span tokens (`mono`, `dim` map to the terminal font and `text_muted` in `tsp::span_style`); this task sets the head's base weight and colors but cannot recolor omp's meta spans without seeing a real omp head. The HUMAN_CHECKS item asks the reviewer to look at it; if a meta run is still `text_muted`, map `dim` to `text_subtle` in `tsp::color` as a follow-up decision for Part A's token owner.

5. SPEC, after the Task 12 paragraph:
```
  A tool call (`card` role `omp.tool*`) has no box: its head is 24 px high with `space::XS` inset and a `radius::SMALL` hover fill, the chevron (`text_subtle`) shows on hover, the title is MEDIUM; open, its `ansi` and `rows` bodies sit on `code_bg` with a `layer[0]` ring and `radius::MEDIUM`. Other cards (`omp.welcome`, overlays) keep their raised frame.
```
   ADR 0117 `## Decision`: `- **Tool rows.** No box around \`omp.tool*\` cards; their output sits on \`code_bg\` with a \`layer[0]\` ring.`

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui only_tool_roles -j 2`, `cargo test -p mkrew-ui tsp -j 2`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.
HUMAN_CHECKS item: "omp pane, dark and light: a tool call is a plain 24 px row (chevron on hover, hover fill `layer[0]`), open it: output on a darker well with a faint ring; the welcome card and overlays keep their frame; the whole transcript reads as in Tern (spec §5)."

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/tsp.rs crates/mkrew-ui/src/shell/tsp_view.rs docs/SPEC.md docs/decisions/0117-tern-polish.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): omp tool rows without a box"
```

---

Coordination with Part E (agreed with PlanE_Files by IRC): E adds a Docs-only `BlockKind::Heading .. if !chat_flavor(ctx)` arm and `doc::PAGE_HEADINGS` before Task 12's heading arm, so after Task 19 Task 12's arm only runs for chat. E also adds `quoted: bool` to `Ctx` and `Ctx::new` (default false) and reads `ctx.muted || ctx.quoted` for Docs paragraph color; Task 12 introduces `Ctx::new(page, flavor)` with `nested` and `muted`, so E touches no literal sites.


---

# Part D: Composer and dock (Tasks 14-16, spec section 6)

Baseline: master at c775413, after Parts A to C have landed (Task 14 consumes their names). All paths are under `crates/mkrew-ui/src/`. Tests run with `cargo test -p mkrew-ui <filter> -j 2`. The Windows rule applies: never run a binary by a relative path.

Facts found while reading the code (they shape the tasks):

- `composer_card_frame`, `meter_bar`, `model_pill`, `quiet_chip`, `placeholder_row`, `composer_room` live in `shell/composer.rs`. The Chat composer card (`Shell::composer_card`) and the omp dock (`Shell::tsp_composer` in `shell/tsp_view.rs`) both call them, so every change shows in both.
- The omp dock cannot run in the smoke sandbox (no omp). Screenshots cover the Chat composer; the omp dock goes to HUMAN_CHECKS (Task 26 writes that entry; this part lists what to look at in each task's Notes).
- No existing test pins an absolute px of the bar. `chat.rs` `chips_drop_from_the_right_in_order` and `dock.rs` `a_narrow_bar_drops_facts_by_priority_then_the_cost_then_truncates_the_model` use the constants symbolically, so they stay valid when the constants change; Task 15 adds tests that pin the new reckoning formulas.
- `float_button` has two callers (`tsp_view.rs:1342` and `chat_view.rs:402`). Task 16 replaces it with `scroll_end_button` and migrates both (the `chat_view.rs` edit is one call, listed in Task 16).
- Hover rule from Part B: every `Look` hovers to `layer[0]`. A control that is itself filled with `layer[0]` (model pill, Send chip) would not show a hover, so Task 15 adds one helper `lift_press` in `shell/widgets.rs` (hover `layer[1]`). It does not depend on the internals of `PressStyle`, so it is independent of how Part B types `hover_bg`.
- Overlap with other work: the `composer-polish` worktree edits `tsp_attachment` and the chip row in `tsp_composer` (`.when(!chips.is_empty(), ...)`). Task 14 edits the lines next to that row (`let card = composer_card_frame(theme)` and the outer column) and does not touch the chip row itself, so expect a small textual conflict at most. Part C owns the transcript rows; the working row label weight (spec 5, "Working row") is applied here in Task 15 because `tsp_dock_working` is in this part; Part C must not edit `tsp_dock_working`.

New and removed names of this part (for the coordinator):

| Where | Change |
|---|---|
| `shell/widgets.rs` | removed: `round_frame`, `round_neutral`, `round_button`, `round_stop_button`, `float_button` |
| `shell/widgets.rs` | `pub(crate) fn send_key_chip(id: &'static str, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>` (replaces `round_button`; the glyph is always `CornerDownLeft`, so the `glyph` argument goes) |
| `shell/widgets.rs` | `pub(crate) fn stop_text_button(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div>` (replaces `round_stop_button`) |
| `shell/widgets.rs` | `pub(crate) fn lift_press(el: gpui::Stateful<gpui::Div>, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>` |
| `shell/widgets.rs` | `pub(crate) fn scroll_end_button(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div>` (replaces `float_button`) |
| `shell/composer.rs` | `composer_room(column: f32, cx: &gpui::App) -> f32` (was `width`, now the text column without page padding); private `room_in(column, ui_size)`; `meter_bar(.., fill: Option<u32>, ..)` returns an absolutely placed `Div` |
| `tsp/dock.rs` | `DockRole::Queue`, `DockRole::QueueItem`; `pub fn fact_icon_color(color: u32, theme: &Theme) -> u32`; private `usage_color` |
| `theme.rs` | removed: `SEND_BUTTON_SIZE`, `STOP_GLYPH_SIZE`; added tokens listed per task |

---

### Task 14: Composer card look, placement, meter on the card edge

**Files:**
- Modify: `theme.rs` (new meter tokens next to `CONTEXT_METER_HEIGHT`, `CONTEXT_LABEL_WIDTH`)
- Modify: `shell/composer.rs` (`composer_card_frame`, `meter_bar`, `composer_room`, new `room_in`, `Shell::composer_card`: card chain, collapsed pill, `composer_drops` corner, wrapper; new `mod tests`)
- Modify: `shell/tsp_view.rs` (`render_tsp` dock placement, `tsp_dock`, `tsp_dock_node`, `tsp_composer`, `tsp_dock_bar` parameter rename `width` to `column`)
- Modify: `docs/SPEC.md` (omp dock paragraph, Chat composer paragraph)
- Test: `shell/composer.rs` (`mod tests`)

**Interfaces:**
- Consumes (Parts A and B): `Theme.composer_fill`, `Theme.layer: [u32; 4]` (RGBA), `theme::shadow_composer(&Theme) -> Vec<gpui::BoxShadow>`, `radius::PANEL`, `widgets::top_highlight(theme: &Theme, radius: f32) -> Div`.
- Consumes (existing): `tsp::column_px(pane_width, several)`, `tsp::column_max(several)`, `page_column(el, max, bounded)`, `theme::with_alpha(color, alpha) -> u32` (RGBA).
- Produces: `composer_room(column: f32, cx: &gpui::App) -> f32` where `column` is the text column width in px with the page padding already removed; `meter_bar(fraction: f32, fill: Option<u32>, left: String, right: String, marks: Vec<(f32, String)>, theme: &Theme) -> gpui::Div` (`fill: None` is the neutral `text` at 42 %; `Some(rgb)` is a tone color, opaque). The returned `Div` is `absolute()` and must be a child of a `relative()` parent (the card).

- [ ] **Step 1: Write the failing test**

Append to `shell/composer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_room_is_the_text_column_less_the_cards_padding_in_13px_rems() {
        // Chat at 900 px: the 760 column less the page padding, less the card's two sides.
        let chat = theme::CHAT_COLUMN_WIDTH - 2.0 * theme::CHAT_COLUMN_PADDING;
        assert_eq!(room_in(chat, 13.0), chat - 2.0 * space::M);
        // A 26 px UI font halves the room that is measured in 13 px rems.
        assert_eq!(room_in(chat, 26.0), (chat - 2.0 * space::M) / 2.0);
        // An omp pane among several spans its pane: no cap, only the page padding off.
        let pane = mkrew_ui::tsp::column_px(500.0, true);
        assert_eq!(
            room_in(pane, 13.0),
            500.0 - 2.0 * theme::CHAT_COLUMN_PADDING - 2.0 * space::M
        );
        // Alone, the column caps at the Chat width however wide the pane is.
        assert_eq!(
            room_in(mkrew_ui::tsp::column_px(5000.0, false), 13.0),
            room_in(chat, 13.0)
        );
    }
}
```

- [ ] **Step 2: Run it, expect FAIL**

`cargo test -p mkrew-ui the_bar_room_is_the_text_column -j 2`
Expected: compile error, `cannot find function room_in`.

- [ ] **Step 3: Implement**

3a. `theme.rs`, after `CONTEXT_LABEL_WIDTH`:

```rust
/// How far the context meter rises above the composer card's top edge (Tern: `top: -12px`).
pub const CONTEXT_METER_RISE: f32 = 12.0;
/// The meter's box: the labels ([`CONTEXT_LABEL_HEIGHT`]) with the 1 px track right under them.
/// The track therefore lies on the card's top border.
pub const CONTEXT_METER_BOX_HEIGHT: f32 = 14.0;
/// Height of the label row of the context meter (OVERLINE text, 11 px).
pub const CONTEXT_LABEL_HEIGHT: f32 = 11.0;
/// Alpha of the meter's rail and of its neutral fill: `text` at these (Tern `t1` 9 % / 42 %).
pub const METER_RAIL_ALPHA: f32 = 0.09;
pub const METER_FILL_ALPHA: f32 = 0.42;
```

3b. `shell/composer.rs`: replace `composer_room` and `composer_card_frame`:

```rust
/// Room for the bar of a composer card, in 13 px rems like the `COMPOSER_*` widths: the text
/// `column` (px, page padding already off: `tsp::column_px`, or the Chat column less its padding)
/// less the card's own padding.
pub(super) fn composer_room(column: f32, cx: &gpui::App) -> f32 {
    room_in(column, cx.global::<theme::Fonts>().ui_size)
}

fn room_in(column: f32, ui_size: f32) -> f32 {
    (column - 2.0 * space::M) / (ui_size / 13.0)
}

/// The card of a composer: Chat's, and an omp pane's dock (spec 6). `composer_fill`, a 1 px
/// `layer[1]` ring, the `glass_hi` line along the top, `shadow_composer`. `relative()` because the
/// context meter is its absolute child. Callers add the children and the press handler.
pub(super) fn composer_card_frame(theme: &Theme) -> gpui::Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .pt(px(space::M))
        .pb(px(space::S))
        .px(px(space::M))
        .rounded(px(radius::PANEL))
        .bg(rgb(theme.composer_fill))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[1]))
        .shadow(theme::shadow_composer(theme))
        .font_family(theme.ui_font_family.clone())
        .child(top_highlight(theme, radius::PANEL))
}
```

Replace `meter_bar` (the `ticks` closure stays as it is except for its parent, below):

```rust
/// The context meter on the card's top edge: an absolute box `CONTEXT_METER_RISE` above the card,
/// `space::L` in from both sides. `left` (centered on the end of the fill) and `right` sit in the
/// label row (OVERLINE, mono, MEDIUM, `text_subtle`), the `CONTEXT_METER_HEIGHT` track right under
/// it. The rail is `text` at 9 %; the fill is `text` at 42 %, or `fill` (a tone color) when given.
/// `marks` (position 0..1, tooltip) are ticks on the track (omp's auto-compact point) that show
/// only while the meter is hovered; Chat has none.
pub(super) fn meter_bar(
    fraction: f32,
    fill: Option<u32>,
    left: String,
    right: String,
    marks: Vec<(f32, String)>,
    theme: &Theme,
) -> gpui::Div {
    let rail = gpui::rgba(theme::with_alpha(theme.text, theme::METER_RAIL_ALPHA));
    let fill = gpui::rgba(match fill {
        Some(tone) => theme::with_alpha(tone, 1.0),
        None => theme::with_alpha(theme.text, theme::METER_FILL_ALPHA),
    });
    let mark = theme.text_muted;
    let ticks = marks.into_iter().enumerate().map(|(i, (at, title))| {
        div()
            .id(("context-mark", i))
            .absolute()
            .left(relative(at.clamp(0.0, 1.0)))
            .ml(px(-theme::CONTEXT_MARK_HIT_WIDTH / 2.0))
            .top(px(-(theme::CONTEXT_MARK_HIT_HEIGHT - theme::CONTEXT_METER_HEIGHT) / 2.0))
            .w(px(theme::CONTEXT_MARK_HIT_WIDTH))
            .h(px(theme::CONTEXT_MARK_HIT_HEIGHT))
            .flex()
            .justify_center()
            .tooltip(tooltip(theme, &title, ""))
            .child(
                div()
                    .w(px(theme::BORDER_WIDTH))
                    .h_full()
                    .group_hover(METER_GROUP, move |style| style.bg(rgb(mark))),
            )
    });
    div()
        .group(METER_GROUP)
        .absolute()
        .top(px(-theme::CONTEXT_METER_RISE))
        .left(px(space::L))
        .right(px(space::L))
        .h(px(theme::CONTEXT_METER_BOX_HEIGHT))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(theme::CONTEXT_LABEL_HEIGHT))
                .flex()
                .justify_end()
                .font_family(theme.terminal_font_family.clone())
                .text_size(font_size::OVERLINE)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_subtle))
                .child(
                    div()
                        .absolute()
                        .left(relative(fraction))
                        .ml(px(-theme::CONTEXT_LABEL_WIDTH / 2.0))
                        .w(px(theme::CONTEXT_LABEL_WIDTH))
                        .flex()
                        .justify_center()
                        .child(left),
                )
                .child(right),
        )
        .child(
            div()
                .absolute()
                .top(px(theme::CONTEXT_LABEL_HEIGHT))
                .left_0()
                .right_0()
                .h(px(theme::CONTEXT_METER_HEIGHT))
                .rounded(px(radius::PILL))
                .bg(rail)
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded(px(radius::PILL))
                        .bg(fill),
                )
                .children(ticks),
        )
}
```

3c. `Shell::composer_card` in `shell/composer.rs`:

Chat meter call (the block `let meter = stats.context...`) becomes:

```rust
        let meter = stats
            .context
            .and_then(|(used, window)| chat::context_meter(used, window?))
            .map(|meter| {
                meter_bar(
                    meter.fraction,
                    meter.warning.then_some(theme.status_needs_input),
                    format!("{}%", meter.percent),
                    meter.window_label,
                    Vec::new(),
                    theme,
                )
            });
```

The room (replaces `let room = composer_room(width, cx);`):

```rust
        let room = composer_room(
            width.min(theme::CHAT_COLUMN_WIDTH) - 2.0 * theme::CHAT_COLUMN_PADDING,
            cx,
        );
```

The card chain: `composer_card_frame(theme)` gets `.children(meter)` as its first call:

```rust
        let card = composer_card_frame(theme)
            .children(meter)
            .on_mouse_down(
```

The end of the function (the meter no longer has its own column, the corner is renamed, the card sits `space::L` above the pane bottom):

```rust
        let card = Self::composer_drops(card, session, radius::PANEL, theme, cx);
        super::chat_view::chat_column(card)
            .pb(px(space::L))
            .into_any_element()
```

The collapsed pill at the top of `composer_card` also moves to `pb(px(space::L))` (replace `.pb(px(space::M))` after `chat_column(pill)`), and its fill follows the card: replace `.bg(rgb(theme.surface_raised))` with `.bg(rgb(theme.composer_fill))` and `.border_color(gpui::rgba(theme.glass_border))` with `.border_color(gpui::rgba(theme.layer[1]))`.

3d. `shell/tsp_view.rs`:

In `render_tsp`, the call and the wrapper:

```rust
            (Some(m), _) => self.tsp_dock(&paint, m, tsp::column_px(width, several), cx),
```

```rust
        .child(
            page_column(
                div()
                    .flex()
                    .flex_col()
                    .when(!card_dock, |el| el.pb(px(space::S)))
                    .children(dock),
                max,
                false,
            )
            // The card sits `space::L` above the pane's bottom edge and spans the transcript
            // column like the rows above it.
            .when(card_dock, |el| el.pb(px(space::L))),
        )
```

(`max` is already `tsp::column_max(several)` in that function. Remove `chat_column` from the `use super::chat_view::{chat_column, page_column};` line if no other use remains: `grep -n chat_column shell/tsp_view.rs` must show none, otherwise clippy fails with an unused import.)

Rename the parameter `width: f32` to `column: f32` in `tsp_dock`, `tsp_dock_node`, `tsp_composer`, `tsp_dock_bar`, and in `tsp_dock_bar` use `composer_room(column, cx)`. In `tsp_dock` / `tsp_dock_node` update the doc of the argument: "the text column in px (page padding off)".

In `tsp_composer` the meter moves into the card:

```rust
        let meter = m.context.as_ref().map(|c| {
            let marks = c.marks.iter().map(|k| (k.at, k.title.clone())).collect();
            // Neutral fill; omp's warning and error tones past its thresholds keep theirs.
            let tone = (c.tone != "accent").then(|| tsp::tone_color(c.tone, theme));
            let bar = meter_bar(
                c.fraction.unwrap_or(0.0),
                tone,
                c.label.clone(),
                c.total.clone(),
                marks,
                theme,
            );
            self.tsp_chip(p, &c.id, Look::Text, bar, cx)
        });
```

and the card and the outer column:

```rust
        let card = composer_card_frame(theme)
            .children(meter)
            .children(focus)
            .when(!chips.is_empty(), |el| {
                el.child(div().flex().flex_wrap().gap(px(space::S)).children(chips))
            })
            .child(field)
            .child(self.tsp_dock_bar(p, m, column, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .children(status)
            .child(card)
            .into_any_element()
```

(The chip row lines are unchanged: the composer-polish worktree edits that row.) The `meter` is an absolute element, so `tsp_chip`'s `id`/`pressable_unless` keeps its position style; the outer column no longer lists it.

3e. `docs/SPEC.md`, in the same commit. In the omp dock paragraph (line starting `The dock is drawn as Tern's composer`): replace `the context meter above the card (a 1 px line, \`text_muted\` fill, \`status_needs_input\` past \`thresholds.warn\`, \`danger_text\` past \`bad\`, the percent above the end of the fill and the total at the right,` with `the context meter on the card's top edge (an absolute box 12 px above the card, 16 px in from both sides, a 1 px track at the foot of its 14 px box, the rail \`text\` at 9 %, the fill \`text\` at 42 %, \`status_needs_input\` past \`thresholds.warn\`, \`danger_text\` past \`bad\`, the percent above the end of the fill and the total at the right in OVERLINE mono MEDIUM \`text_subtle\`,` and replace `the card (\`surface_raised\`, \`radius::COMPOSER\`, glass border, float shadow)` with `the card (\`composer_fill\`, \`radius::PANEL\`, a 1 px \`layer[1]\` ring, the \`glass_hi\` top line, \`shadow_composer\`, padding \`pt(M) pb(S) px(M)\`, as wide as the transcript column and \`space::L\` above the pane bottom)`.

In the Chat composer paragraph (`In Chat the composer is a floating card`): replace `a 1 px context meter above it (\`text_muted\` fill, the percent above the end of the fill and the window at the right;` with `a context meter on its top edge (the rail and fill are \`text\` at 9 % and 42 %, the percent above the end of the fill and the window at the right;` and add after the first sentence group `The card is \`composer_fill\` with a 1 px \`layer[1]\` ring, a \`glass_hi\` top line and \`shadow_composer\`, \`space::L\` above the pane bottom.` Read both lines with `read docs/SPEC.md:1312:raw` and `:2585:raw` first; keep every other clause.

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui the_bar_room_is_the_text_column -j 2` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshot and commit**

```
cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins
powershell -File scripts/smoke.ps1 start -Theme dark
powershell -File scripts/smoke.ps1 mk session spawn api --agent spin
```
Open the session pane, switch it to Chat (click the Chat toggle; use `smoke.ps1 shot -Out x.png` to find it), then
`powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-composer-card-dark.png`, `powershell -File scripts/smoke.ps1 stop`, and the same with `-Theme light` for `...-light.png`. Check by reading the PNGs: dark card one step lighter than the pane with a faint ring and no heavy shadow; light card white; the meter labels and track sit on the top edge (no row of their own); the card is `space::L` above the pane bottom. If the meter labels collide with the card ring, move `CONTEXT_LABEL_HEIGHT` and the `top` of the track by 1 px, nothing else.

Notes for HUMAN_CHECKS (omp pane): the meter on the card edge with a warning tone; the card as wide as the transcript among several panes; with a status row present, the meter's labels clear the working row and hud pill (the meter rises 4 px into the 8 px gap plus the row's bottom padding; if they touch, use `space::M` for the dock's outer `gap` when `m.context.is_some()`).

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/tsp_view.rs docs/SPEC.md docs/screenshots/tern-polish-composer-card-dark.png docs/screenshots/tern-polish-composer-card-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): composer card on composer_fill with the context meter on its top edge"
```

---

### Task 15: Bar controls, facts, status rows and the fit reckoning

**Files:**
- Modify: `theme.rs` (`COMPOSER_PILL_HEIGHT`, `COMPOSER_MONO_CHAR_WIDTH`, `COMPOSER_CHIP_MODEL_WIDTH`, `COMPOSER_CHIP_STATS_WIDTH`, `COMPOSER_CHIP_CLOCK_WIDTH`, `COMPOSER_FIXED_WIDTH`, `TSP_BAR_FIXED_WIDTH`, `TSP_MODEL_CHROME_WIDTH`; remove `SEND_BUTTON_SIZE`, `STOP_GLYPH_SIZE`; add tokens below)
- Modify: `shell/widgets.rs` (remove `round_frame`, `round_neutral`, `round_button`, `round_stop_button`; add `lift_press`, `send_key_chip`, `stop_text_button`; `float_button` stays until Task 16 and is edited there)
- Modify: `shell/composer.rs` (`model_pill`, `quiet_chip`, `placeholder_row`, collapsed pill text color, Chat middle cell glyph colors and gap, `send` button)
- Modify: `shell/tsp_view.rs` (`tsp_dock_bar`, `tsp_dock_fact`, `tsp_dock_primary`, `tsp_dock_status`, `tsp_dock_working`, `tsp_dock_pill`, `tsp_dock_pills`, placeholder color in `tsp_editor`)
- Modify: `tsp/dock.rs` (`usage_color`, `fact_icon_color`, `fact_width`, `bar_width`)
- Modify: `chat.rs` (`branch_width`)
- Modify: `docs/SPEC.md`
- Test: `tsp/dock.rs` (`mod tests`), `chat.rs` (`mod tests`)

**Interfaces:**
- Consumes (Parts A and B): `Theme.layer`, `Theme.subtle`, `Theme.status_exited`, `Theme.danger_text`, `radius::MEDIUM`, `Look::Chip` hover = `layer[0]` (so a segment inside the hud pill and a clickable fact hover with `layer[0]`).
- Consumes (Task 14): `composer_room(column, cx)`.
- Produces: the signatures in the table above; tokens:

```rust
pub const COMPOSER_PILL_HEIGHT: f32 = 24.0;       // every control of the bar and the hud pill
pub const SEND_CHIP_MIN_WIDTH: f32 = 28.0;
pub const SEND_GLYPH_SIZE: f32 = 12.0;
pub const STOP_FILL_ALPHA: f32 = 0.08;
pub const STOP_RING_ALPHA: f32 = 0.22;
pub const STOP_HOVER_ALPHA: f32 = 0.14;
pub const STOP_BUTTON_WIDTH: f32 = 2.0 * space::M + 4.0 * TSP_UI_CHAR_WIDTH; // "Stop" at META
pub const COMPOSER_FACT_CHROME_WIDTH: f32 = 3.0 * space::XS; // px(XS) both sides + the gap after
pub const TSP_STATUS_ROW_HEIGHT: f32 = 26.0;
pub const TSP_HUD_SEGMENT_HEIGHT: f32 = 20.0;
pub const TSP_HUD_PAD: f32 = 2.0;                  // stated exception to the 4 px grid (spec 6)
```

- [ ] **Step 1: Write the failing tests**

`tsp/dock.rs`, in `mod tests`: replace the body of `facts_and_the_cost_are_subtle_unless_omp_colors_them` (rename it `facts_are_text_subtle_and_the_cost_is_subtle_unless_omp_colors_them`) so its assertions read:

```rust
    #[test]
    fn facts_are_text_subtle_and_the_cost_is_subtle_unless_omp_colors_them() {
        let t = Theme::dark();
        let m = model_of(real_children(true)).expect("composer");
        assert!(m.bar.facts.iter().all(|f| f.color == t.text_subtle));
        // The cost is one tier below the facts.
        assert_eq!(m.bar.usage.as_ref().map(|u| u.color), Some(t.subtle));

        let d = doc(vec![
            json!(["add", "f", "s:1", null, {"id": "f", "k": "seg", "p": {"role": "omp.composer.fact", "tone": "warning", "spans": [{"t": "*1 ", "s": "warning"}, {"t": "?1 ", "s": "muted"}]}}]),
            json!(["add", "u", "s:1", null, {"id": "u", "k": "text", "p": {"role": "omp.composer.usage", "tone": "error", "text": "$9"}}]),
            json!(["add", "n", "s:1", null, {"id": "n", "k": "text", "p": {"role": "omp.composer.usage", "tone": "accent", "text": "$1"}}]),
        ]);
        let fact = fact_of(&d, d.get("f").expect("fact"), &t).expect("a fact");
        assert_eq!(fact.color, t.status_needs_input, "tone warning");
        assert_eq!(
            fact.runs,
            [
                (
                    "*1 ".to_owned(),
                    SpanStyle {
                        color: Some(t.status_needs_input),
                        ..SpanStyle::default()
                    }
                ),
                (
                    "?1".to_owned(),
                    SpanStyle {
                        color: Some(t.text_muted),
                        ..SpanStyle::default()
                    }
                ),
            ],
            "span colors stay, the whole text is trimmed"
        );
        assert_eq!(usage_color(d.get("u").expect("cost"), &t), t.danger_text);
        assert_eq!(usage_color(d.get("n").expect("cost"), &t), t.subtle);
        assert_eq!(quiet_color(d.get("n").expect("cost"), &t), t.text_subtle);
    }

    #[test]
    fn a_quiet_fact_has_a_quieter_icon_than_its_text_and_a_toned_one_keeps_its_color() {
        let t = Theme::dark();
        assert_ne!(t.subtle, t.text_subtle, "the tiers differ");
        assert_eq!(fact_icon_color(t.text_subtle, &t), t.subtle);
        assert_eq!(fact_icon_color(t.status_needs_input, &t), t.status_needs_input);
        assert_eq!(fact_icon_color(t.danger_text, &t), t.danger_text);
    }
```

and replace the last assertion block of `the_model_name_and_effort_glyph_are_reckoned_in_the_ui_font_the_facts_in_mono` (the `assert!((fact_width(&m.bar.facts[0]) ...` statement) with:

```rust
        // A fact: icon and gap, the mono text, and the chips' own padding and the gap after it.
        assert!(
            (fact_width(&yolo) - (4.0 * theme::COMPOSER_MONO_CHAR_WIDTH + theme::COMPOSER_FACT_CHROME_WIDTH))
                .abs()
                < 0.01
        );
        let with_icon = Fact { icon: Some(Icon::Clock), ..yolo.clone() };
        assert!(
            (fact_width(&with_icon) - fact_width(&yolo) - (theme::ICON_SIZE_SMALL + space::XS)).abs()
                < 0.01
        );
```

(`Fact` derives `Clone`; `Icon::Clock` exists in `icons.rs` and is already used in `composer.rs`. Add `use crate::icons::Icon;` is already imported at the top of `dock.rs` and reaches `mod tests` through `use super::*`.)

`chat.rs`, in `mod tests` after `a_long_branch_never_pushes_the_send_button_out_of_the_card`:

```rust
    #[test]
    fn a_branch_chip_is_reckoned_with_its_own_padding() {
        // `a *1` is four characters in the terminal font; the chip pads both sides by `space::XS`.
        let want = theme::ICON_SIZE_SMALL
            + theme::space::XS
            + 4.0 * theme::COMPOSER_MONO_CHAR_WIDTH
            + 2.0 * theme::space::XS
            + theme::space::S;
        assert!((branch_width(&branch_chip("a")) - want).abs() < 0.01);
    }
```

- [ ] **Step 2: Run them, expect FAIL**

`cargo test -p mkrew-ui facts_are_text_subtle -j 2`, `cargo test -p mkrew-ui a_quiet_fact_has_a_quieter_icon -j 2`, `cargo test -p mkrew-ui a_branch_chip_is_reckoned -j 2`, `cargo test -p mkrew-ui the_model_name_and_effort_glyph -j 2`.
Expected: compile errors (`usage_color`, `fact_icon_color`, `COMPOSER_FACT_CHROME_WIDTH` missing).

- [ ] **Step 3: Implement**

3a. `theme.rs`. Replace the `STOP_GLYPH_SIZE` and `COMPOSER_PILL_HEIGHT` items near the top:

```rust
/// The composer bar's controls (Tern: 24 px): the model pill, the Send chip, the Stop button
/// and the omp hud pill are all this tall.
pub const COMPOSER_PILL_HEIGHT: f32 = 24.0;
/// The Send key chip is at least this wide; its return glyph is [`SEND_GLYPH_SIZE`].
pub const SEND_CHIP_MIN_WIDTH: f32 = 28.0;
pub const SEND_GLYPH_SIZE: f32 = 12.0;
/// The Stop text button: `status_exited` at these alphas as fill, ring and hover fill.
pub const STOP_FILL_ALPHA: f32 = 0.08;
pub const STOP_RING_ALPHA: f32 = 0.22;
pub const STOP_HOVER_ALPHA: f32 = 0.14;
```

In the composer constants block, delete `SEND_BUTTON_SIZE` and change:

```rust
/// One character of the terminal font at `OVERLINE` (11 px) size.
pub const COMPOSER_MONO_CHAR_WIDTH: f32 = 6.6;
```
```rust
/// The model pill: 2 x `space::S` padding, no border, `opus 5.5` / `sonnet 4.5`, a gap and the
/// chevron, and the gap after it.
pub const COMPOSER_CHIP_MODEL_WIDTH: f32 = 106.0;
```
```rust
/// Context tokens, `tok/s` and the elapsed time: each with its icon, its own `px(XS)` padding
/// ([`COMPOSER_FACT_CHROME_WIDTH`]) and the `space::XS` gap between them.
pub const COMPOSER_CHIP_STATS_WIDTH: f32 = 200.0;
/// The clock time of the last turn's end (icon, `09:44:27` and the chip's padding).
pub const COMPOSER_CHIP_CLOCK_WIDTH: f32 = 72.0;
/// Prompts (icon only: the same button as ✦) and the Send chip, with their gaps.
pub const COMPOSER_FIXED_WIDTH: f32 = COMPOSER_ENHANCE_WIDTH + SEND_CHIP_MIN_WIDTH + space::S;
/// What one fact of a bar takes besides its icon and its text: `px(XS)` on both sides and the
/// `space::XS` gap to the next one.
pub const COMPOSER_FACT_CHROME_WIDTH: f32 = 3.0 * space::XS;
```

(`COMPOSER_CHIP_EFFORT_WIDTH` 56 and `COMPOSER_CHIP_PERMISSION_WIDTH` 92 stay: their text is UI font at OVERLINE, which is narrower than the old META estimate, and the added `px(XS)` fits inside the old margin. `COMPOSER_FIXED_WIDTH` keeps its value 82: it is now derived, and the old 32 px Send was under-reckoned.)

In the TSP block:

```rust
/// Room reckoned for the bar of an omp dock (px at 13 px UI size, like the `COMPOSER_*` ones):
/// the Stop button (the wider of Send and Stop) and its gap.
pub const TSP_BAR_FIXED_WIDTH: f32 = STOP_BUTTON_WIDTH + space::S;
/// The Stop button: `px(M)` both sides and `Stop` (four UI characters at `META`).
pub const STOP_BUTTON_WIDTH: f32 = 2.0 * space::M + 4.0 * TSP_UI_CHAR_WIDTH;
/// The model pill without its name: padding, chevron, gaps and the gap after it (no border).
pub const TSP_MODEL_CHROME_WIDTH: f32 = 46.0;
/// The omp status row (working row, hud) is this tall (Tern: 26).
pub const TSP_STATUS_ROW_HEIGHT: f32 = 26.0;
/// A segment of the hud pill (Tern: 20) and the pill's inner padding (2 px: a stated exception
/// to the 4 px grid, spec 6).
pub const TSP_HUD_SEGMENT_HEIGHT: f32 = 20.0;
pub const TSP_HUD_PAD: f32 = 2.0;
```

3b. `shell/widgets.rs`: delete `round_frame`, `round_neutral`, `round_button`, `round_stop_button` (lines from `/// The frame of the round icon buttons` through the end of `round_stop_button`). `float_button` uses `round_neutral`, so in this task keep it compiling by replacing its body temporarily is not allowed (no shims): do Task 16's `scroll_end_button` edit in this task's commit order instead, see the Note at the end of Task 16. Therefore this task's commit includes only: add the new functions, migrate the two Send/Stop callers, and delete `round_button` and `round_stop_button`. `round_frame` and `round_neutral` stay until Task 16 deletes them with `float_button`.

Add after `button_fg`:

```rust
/// Press feel for a control that is itself filled with `layer[0]` (the model pill, the Send
/// chip): the plain looks hover to `layer[0]`, which would not show on it, so this one hovers
/// to `layer[1]`. `enabled` false dims it with no hover and no pointer.
pub(crate) fn lift_press(
    el: gpui::Stateful<gpui::Div>,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = gpui::rgba(theme.layer[1]);
    el.when(enabled, |el| {
        el.cursor_pointer()
            .hover(move |style| style.bg(hover))
            .active(|style| style.opacity(opacity::ACTIVE))
    })
    .when(!enabled, |el| el.opacity(opacity::DISABLED))
}

/// The composer's Send as a key chip (Tern): `COMPOSER_PILL_HEIGHT` tall, at least
/// `SEND_CHIP_MIN_WIDTH` wide, `radius::MEDIUM`, a `layer[0]` fill with a `layer[1]` ring and a
/// `SEND_GLYPH_SIZE` return glyph in `text_subtle` that turns `text` on hover (an svg does not
/// inherit its parent's color: through the id group). Dimmed with no hover while `enabled` is
/// false. Attach the handler with `.on_press(enabled, ..)`.
pub(crate) fn send_key_chip(
    id: &'static str,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let lit = theme.text;
    let glyph = icon_svg(Icon::CornerDownLeft, theme::SEND_GLYPH_SIZE, theme.text_subtle)
        .when(enabled, |svg| {
            svg.group_hover(id, move |style| style.text_color(rgb(lit)))
        });
    lift_press(
        div()
            .id(id)
            .group(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(theme::rems_px(theme::COMPOSER_PILL_HEIGHT))
            .min_w(theme::rems_px(theme::SEND_CHIP_MIN_WIDTH))
            .rounded(px(radius::MEDIUM))
            .bg(gpui::rgba(theme.layer[0]))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .child(glyph),
        enabled,
        theme,
    )
}

/// The Stop button of an omp pane's dock (Tern): a text button "Stop", `COMPOSER_PILL_HEIGHT`
/// tall, `px(M)`, `radius::MEDIUM`, META SEMIBOLD `danger_text` on `status_exited` at 8 % with a
/// 22 % ring; hover 14 %. Attach the tooltip (with its key) and `.on_press(true, ..)`.
pub(crate) fn stop_text_button(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let tint = |alpha: f32| gpui::rgba(theme::with_alpha(theme.status_exited, alpha));
    let hover = tint(theme::STOP_HOVER_ALPHA);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(theme::rems_px(theme::COMPOSER_PILL_HEIGHT))
        .px(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(tint(theme::STOP_FILL_ALPHA))
        .border(px(theme::BORDER_WIDTH))
        .border_color(tint(theme::STOP_RING_ALPHA))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.danger_text))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child("Stop")
}
```

(`opacity::ACTIVE` / `opacity::DISABLED` resolve through `use crate::*` as in the existing `filled_press`.)

3c. `shell/composer.rs`.

`model_pill` body (height 24, `radius::MEDIUM`, `layer[0]` fill, no ring, chevron `text_subtle`; the lift press instead of `Look::Chip`):

```rust
/// The model pill (Tern): `COMPOSER_PILL_HEIGHT` tall, `radius::MEDIUM`, a `layer[0]` fill with
/// no ring (hover `layer[1]`), the name in `text` at MEDIUM, the optional `lead` (the omp dock's
/// effort glyph) before it and a `text_subtle` chevron after. Callers add the tooltip and
/// `.on_press(..)`.
pub(super) fn model_pill(
    id: impl Into<gpui::ElementId>,
    lead: Option<gpui::AnyElement>,
    name: impl IntoElement,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    lift_press(
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(theme::COMPOSER_PILL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(gpui::rgba(theme.layer[0]))
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .children(lead)
            .child(name)
            .child(icon_svg(
                Icon::ChevronDown,
                theme::MODEL_CHEVRON_SIZE,
                theme.text_subtle,
            )),
        enabled,
        theme,
    )
}
```

`quiet_chip` (OVERLINE, `px(XS)`, `radius::CHIP`; the facts' hover comes from `tsp_chip(.., Look::Chip, ..)`):

```rust
/// A quiet chip (a fact, the usage, the effort): OVERLINE `text_subtle`, icon and text with
/// `space::XS` between, `px(XS)` of its own, never shrinking.
pub(super) fn quiet_chip(el: gpui::Div, theme: &Theme) -> gpui::Div {
    el.flex()
        .flex_none()
        .items_center()
        .gap(px(space::XS))
        .px(px(space::XS))
        .rounded(px(radius::CHIP))
        .text_size(font_size::OVERLINE)
        .text_color(rgb(theme.text_subtle))
}
```

`placeholder_row`: `text_color(rgb(theme.subtle))`, doc "in `subtle` (the t4 tier)". In `Shell::composer_card`: the collapsed pill's inner placeholder `div().text_color(rgb(theme.text_subtle))` becomes `rgb(theme.subtle)`; in the Chat middle cell change the three `glyph(Icon::Database, theme.text_subtle)`, `glyph(Icon::Clock, theme.text_subtle)` (two places) and `glyph(Icon::GitBranch, theme.text_subtle)` to `theme.subtle`; the stats cell `.gap(px(space::S))` becomes `.gap(px(space::XS))`.

The Send button call:

```rust
        let send = send_key_chip("composer-send", can_send, theme)
            .tooltip(tooltip(theme, composer::send_tip(), ""))
```

(the `.on_press(can_send, ..)` that follows is unchanged).

3d. `shell/tsp_view.rs`.

`tsp_dock_bar`, the facts cell gap:

```rust
                            .gap(px(space::XS))
```

(replaces `.gap(px(space::S))` in the `BarPart::Facts` arm; the usage chip arm is unchanged: `quiet_chip` now draws it OVERLINE with `px(XS)` and its color comes from `usage_color`).

`tsp_dock_fact`, the icon color:

```rust
                if let Some(icon) = fact.icon {
                    el = el.child(icon_svg(
                        icon,
                        theme::ICON_SIZE_SMALL,
                        tsp::fact_icon_color(fact.color, theme),
                    ));
                }
```

`tsp_dock_primary` (Stop with its key in the tooltip, Send as the chip):

```rust
            tsp::Primary::Stop(id) => {
                let (tip, id) = (title(id, "Stop"), id.clone());
                stop_text_button("tsp-stop", theme)
                    .when(p.tips(), |el| el.tooltip(tooltip(theme, &tip, "Esc")))
```
```rust
                send_key_chip("tsp-send", can, theme)
                    .when(p.tips(), |el| el.tooltip(tooltip(theme, &tip, "")))
```

(Esc is omp's interrupt: `tsp_key_down` forwards a plain Escape to omp when nothing local consumes it. [INFERENCE] that omp's own Stop title may already carry a key; if the tooltip then shows it twice, pass `""` instead.) Update the doc of `tsp_dock_primary` to "The Send key chip ... or the Stop text button".

`tsp_dock_status` (row height 26, left padding to line up with the card text, no bottom padding: the dock's outer `gap(space::S)` is the 8 px above the card):

```rust
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .min_w_0()
                .h(theme::rems_px(theme::TSP_STATUS_ROW_HEIGHT))
                .px(px(space::M))
```

(replaces `.px(px(space::S)).pb(px(space::XS))`. Deviation from the spec text: spec says "left padding 16"; the card's text starts `space::M` (12) in from its edge, and the stated goal is to line up with the card text, so the row uses `space::M`. Say so in the commit body.)

`tsp_dock_working` (divider glyph `subtle`, label MEDIUM per spec 5; the doc comment says "the card's Stop is the one"):

```rust
            .child(
                div()
                    .flex_none()
                    .w(px(theme::BORDER_WIDTH))
                    .h(theme::rems_px(space::M))
                    .bg(rgb(theme.subtle)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child(w.label.clone()),
            )
```

`tsp_dock_pill` (segment 20 tall; hover is `Look::Chip` through `tsp_chip`):

```rust
        let el = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(theme::TSP_HUD_SEGMENT_HEIGHT))
            .px(px(space::S))
            .rounded(px(theme::radius::PILL))
```

`tsp_dock_pills` (24 tall, `layer[1]` ring, MEDIUM, 2 px inner padding, `layer[1]` dividers 12 tall; doc: "ONE pill with a `layer[1]` ring and no fill"):

```rust
                segs.push(
                    div()
                        .flex_none()
                        .w(px(theme::BORDER_WIDTH))
                        .h(theme::rems_px(space::M))
                        .bg(gpui::rgba(theme.layer[1]))
                        .into_any_element(),
                );
```
```rust
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(theme::rems_px(theme::COMPOSER_PILL_HEIGHT))
            .px(px(theme::TSP_HUD_PAD))
            .rounded(px(theme::radius::PILL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .children(segs)
            .into_any_element()
```

Placeholder in `tsp_editor` (`EditorLook::Composer` arm): `styled_runs(&placeholder, theme.subtle, false, theme)` (was `theme.text_muted`; the runs carry their own color, so `placeholder_row`'s color alone would not apply). omp's own span colors in the placeholder are ignored by `styled_runs` for the fallback color only; spans styled by omp keep theirs.

3e. `tsp/dock.rs`.

Add next to `quiet_color`:

```rust
/// The color of the cost: `subtle` (one tier below the facts); omp's tone `warning` / `error`
/// keep their colors.
fn usage_color(node: &Node, theme: &Theme) -> u32 {
    match node.props.get("tone").and_then(Value::as_str) {
        Some(tone @ ("warning" | "error")) => tone_color(tone, theme),
        _ => theme.subtle,
    }
}

/// The icon of a fact: `subtle` while the fact is quiet (`text_subtle`), so the icon is a tier
/// below its text; a color omp gave the fact (its tone) stays.
pub fn fact_icon_color(color: u32, theme: &Theme) -> u32 {
    if color == theme.text_subtle {
        theme.subtle
    } else {
        color
    }
}
```

In `bar_model`, the `DockRole::Usage` arm uses `color: usage_color(node, theme)`. In `fact_width`: `icon + body + theme::COMPOSER_FACT_CHROME_WIDTH`. In `bar_width`: usage `text_room(&u.text) + theme::COMPOSER_FACT_CHROME_WIDTH`. Update the doc comments of the fact/usage `color` fields: `Fact::color` unchanged, `Usage::color` "`subtle`, or the tone color".

3f. `chat.rs` `branch_width`:

```rust
    let natural = theme::ICON_SIZE_SMALL
        + theme::space::XS
        + chars as f32 * theme::COMPOSER_MONO_CHAR_WIDTH
        + 2.0 * theme::space::XS;
```

(the chip pads both sides by `px(XS)` now; the cap `COMPOSER_BRANCH_MAX_WIDTH` still bounds it, and `composer_branch_chip`'s `max_w` includes the padding).

3g. `docs/SPEC.md` (same commit). In the omp dock paragraph: replace `quiet mono fact chips in \`text_subtle\`` with `quiet mono fact chips (OVERLINE, \`text_subtle\`, icons in \`subtle\`, \`px(XS)\` each, 4 px apart, a \`layer[0]\` hover when clickable)`; replace `the model pill (a 1 px \`border\` ring with no fill, \`text\` at 500, chevron;` with `the model pill (24 px, \`radius::MEDIUM\`, a \`layer[0]\` fill with no ring, hover \`layer[1]\`, \`text\` at 500, a \`text_subtle\` chevron;`; replace `the rate and cost chips, and the round Send (\`CornerDownLeft\`, a neutral fill that turns to \`text\` on hover, never accent) or Stop (a \`text\` circle with a square) at the end of the row.` with `the rate chip, the cost (OVERLINE mono \`subtle\`; omp's warning and error tones stay), and at the end of the row the Send key chip (24 px high, at least 28 wide, \`radius::MEDIUM\`, a \`layer[0]\` fill with a \`layer[1]\` ring, a 12 px \`CornerDownLeft\` in \`text_subtle\` that turns \`text\` on hover; dimmed while nothing can be sent) or the Stop text button (24 px high, \`px(M)\`, META SEMIBOLD \`danger_text\` on \`status_exited\` at 8 % with a 22 % ring, 14 % on hover; its tooltip shows Esc).` and `the placeholder in \`text_subtle\`` with `the placeholder in \`subtle\``.
In the Chat paragraph: `the field with the session title in quotes as its placeholder (\`text_subtle\`,` becomes `(\`subtle\`,`; `the model as a ring pill` becomes `the model as a \`layer[0]\` pill`; `all quiet in \`text_subtle\`` becomes `all quiet (OVERLINE \`text_subtle\`, icons \`subtle\`)`; `a round send button (a neutral fill that turns to \`text\` on hover, never accent; dimmed while the draft is blank or busy)` becomes `the Send key chip (24 px, \`layer[0]\` fill, \`layer[1]\` ring, return glyph that turns \`text\` on hover; dimmed while the draft is blank or busy)`.
In the "Above the meter, the status row" paragraph (Chat/omp, line after the Chat paragraph): replace `Above the meter, the status row has the working row (loading dots, the elapsed time in mono \`text_subtle\`, a divider, the label in \`text_muted\`; the card's Stop is the only Stop), the todo HUD, and the hud as one pill with a \`border\` ring and no fill whose segments (the agent and jobs pills, each an icon and a count in mono) are split by dividers.` with `Above the card, the status row (26 px high, \`space::M\` at the left, 8 px above the card) has the working row (loading dots, the elapsed time in mono \`text_subtle\`, a \`subtle\` divider, the label in \`text_muted\` at MEDIUM; the card's Stop is the only Stop), the todo HUD, and the hud as one 24 px pill with a \`layer[1]\` ring and no fill (MEDIUM, 2 px inner padding) whose 20 px segments (the agent and jobs pills, each an icon and a count in mono, a \`layer[0]\` hover) are split by \`layer[1]\` dividers.`

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui facts_are_text_subtle a_quiet_fact_has_a_quieter_icon a_branch_chip_is_reckoned the_model_name_and_effort_glyph a_narrow_bar_drops chips_drop_from_the_right the_tools_drop_whole a_long_branch_never_pushes omp_and_codex_sessions_show -j 2` (run each filter separately if cargo rejects several; or `cargo test -p mkrew-ui -j 2 dock::` and `cargo test -p mkrew-ui -j 2 chat::`), then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.
The untouched `chips_drop_from_the_right_in_order` and `a_narrow_bar_drops_facts_by_priority_then_the_cost_then_truncates_the_model` must pass without edits (they use the constants symbolically). If `omp_and_codex_sessions_show_their_stats_at_900_px` fails, the estimates grew past `ROOM_900` (688): lower `COMPOSER_CHIP_STATS_WIDTH` toward the measured width, never raise `ROOM_900`.

- [ ] **Step 5: Screenshot and commit**

Same smoke recipe as Task 14 with files `docs/screenshots/tern-polish-composer-bar-dark.png` and `...-light.png` (Chat composer focused, so the bar shows the model pill, facts and the Send chip; type a character to enable Send). Read the PNGs: all bar controls are one 24 px line; the model pill is a soft fill with no ring; Send is a small key chip, dimmed while the field is blank; facts are one size smaller than the model name, icons quieter than their text; the placeholder is the quietest text on the card.

HUMAN_CHECKS (omp pane): the Stop text button while omp works (red tint, hover step, tooltip with Esc); the hud pill ring and segment hover; working row 26 px with its label MEDIUM; facts drop at the same pane widths as before (narrow the pane: facts, then cost, then the model name; Send or Stop never clipped).

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/tsp/dock.rs crates/mkrew-ui/src/chat.rs docs/SPEC.md docs/screenshots/tern-polish-composer-bar-dark.png docs/screenshots/tern-polish-composer-bar-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): 24 px composer bar with a Send key chip, a Stop text button and quieter facts"
```

---

### Task 16: Queue roles and the quiet scroll-to-bottom button

**Files:**
- Modify: `tsp/dock.rs` (`DockRole`, `dock_role`)
- Modify: `shell/tsp_view.rs` (`tsp_row`, new `tsp_queue_item`, `tsp_col_el`, scroll button in `render_tsp`)
- Modify: `shell/widgets.rs` (remove `round_frame`, `round_neutral`, `float_button`; add `scroll_end_button`)
- Modify: `shell/chat_view.rs` (one call, `render_chat`)
- Modify: `theme.rs` (two tokens)
- Modify: `docs/SPEC.md`
- Test: `tsp/dock.rs` (`mod tests`)

**Interfaces:**
- Consumes: `tsp::dock_role` (via `pub use dock::*`), `Look::Icon` (hover `layer[0]` from Part B), `Theme.layer`, `theme::radius::PILL`.
- Produces: `DockRole::Queue`, `DockRole::QueueItem`; `scroll_end_button(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div>`; tokens `TSP_QUEUE_ITEM_HEIGHT: f32 = 26.0`, `SCROLL_END_BUTTON_SIZE: f32 = 28.0`.

- [ ] **Step 1: Write the failing test**

In `tsp/dock.rs` `mod tests`, after `dock_roles_are_matched_whole_and_the_ask_card_is_not_the_composer`:

```rust
    #[test]
    fn the_queue_roles_are_matched_whole() {
        assert_eq!(dock_role("omp.queue"), Some(DockRole::Queue));
        assert_eq!(dock_role("omp.queue.item"), Some(DockRole::QueueItem));
        // The count and the edit chip of an item are drawn generically inside it.
        assert_eq!(dock_role("omp.queue.count"), None);
        assert_eq!(dock_role("omp.queue.edit"), None);
        assert_eq!(dock_role("omp.queued"), None);
    }
```

- [ ] **Step 2: Run it, expect FAIL**

`cargo test -p mkrew-ui the_queue_roles_are_matched_whole -j 2` : compile error, no variant `Queue`.

- [ ] **Step 3: Implement**

3a. `tsp/dock.rs`: add the variants at the end of `DockRole` and the arms:

```rust
    /// `col role=omp.queue`: the texts queued while omp works.
    Queue,
    /// `row role=omp.queue.item` of the queue: an icon, the text, a count and an edit chip.
    QueueItem,
```
```rust
        "omp.queue" => DockRole::Queue,
        "omp.queue.item" => DockRole::QueueItem,
```

3b. `theme.rs`:

```rust
/// A queued text of the omp dock is a pill this tall (Tern: 26).
pub const TSP_QUEUE_ITEM_HEIGHT: f32 = 26.0;
/// The scroll-to-bottom button of a transcript: a quiet icon button this size (Tern: a bare chevron).
pub const SCROLL_END_BUTTON_SIZE: f32 = 28.0;
```

3c. `shell/tsp_view.rs`. The queue column (gap 4, a gap above the card) in `tsp_col_el`, right after `let mut el = div()...;`:

```rust
        if tsp::role_of(node).and_then(tsp::dock_role) == Some(tsp::DockRole::Queue) {
            el = el.gap(px(space::XS)).pb(px(space::S));
        }
```

The item in `tsp_row`:

```rust
    #[inline(never)]
    fn tsp_row(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        if tsp::role_of(node).and_then(tsp::dock_role) == Some(tsp::DockRole::QueueItem) {
            return self.tsp_queue_item(p, node, cx);
        }
        let kids = self.tsp_children(p, node, cx);
        self.tsp_row_el(p, node, kids, cx)
    }

    /// One queued text (`omp.queue.item`): a `TSP_QUEUE_ITEM_HEIGHT` pill with a `layer[0]` fill, its
    /// icon at 14 px in `text_subtle`, the text in META `text_muted` clipped at the row's end, then
    /// whatever omp put after it (the count, the edit chip) drawn generically.
    #[inline(never)]
    fn tsp_queue_item(&self, p: &Paint<'_>, node: &Node, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = p.theme;
        let kids: Vec<AnyElement> = (p.doc.children(&node.id))
            .filter(|child| !bool_prop(child, "hidden"))
            .map(|child| match child.kind.as_str() {
                "icon" => str_prop(child, "name")
                    .and_then(|name| self.tsp_icon(p, name, theme.text_subtle))
                    .map_or_else(
                        || div().into_any_element(),
                        |icon| div().flex_none().child(icon).into_any_element(),
                    ),
                "text" => div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.tsp_node(p, child, cx))
                    .into_any_element(),
                _ => self.tsp_node(p, child, cx),
            })
            .collect();
        let el = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .min_w_0()
            .h(theme::rems_px(theme::TSP_QUEUE_ITEM_HEIGHT))
            .pl(px(space::M))
            .pr(px(space::XS))
            .rounded(px(theme::radius::PILL))
            .bg(gpui::rgba(theme.layer[0]))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .children(kids);
        self.tsp_decorate(el, p, node, Look::Text, false, cx)
    }
```

(`Look::Text` because the pill's own fill is `layer[0]` and the `Chip` hover, also `layer[0]`, would not show on it.)

The scroll button in `render_tsp`:

```rust
                            .child(page_column(
                                div().flex().justify_end().child(
                                    scroll_end_button("tsp-scroll-end", theme)
```

(the rest of the chain, tooltip and `.on_press`, is unchanged).

3d. `shell/chat_view.rs` line 402: `float_button("chat-scroll-end", Icon::ArrowDown, theme)` becomes `scroll_end_button("chat-scroll-end", theme)`. If `Icon` and `float_button` imports in both files become unused, clippy names them: remove only those.

3e. `shell/widgets.rs`: delete `round_frame`, `round_neutral` and `float_button` (this is what Task 15 deferred), and add:

```rust
/// The scroll-to-the-end button of a transcript list: a quiet `SCROLL_END_BUTTON_SIZE` icon button
/// with no frame and no shadow at rest (`Look::Icon` hover), a 16 px chevron in `text_subtle`. It
/// floats over the list, so it takes the mouse itself: only the wheel passes through.
pub(crate) fn scroll_end_button(
    id: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(theme::rems_px(theme::SCROLL_END_BUTTON_SIZE))
        .rounded(px(radius::MEDIUM))
        .block_mouse_except_scroll()
        .pressable(Look::Icon, true, theme)
        .child(icon_svg(Icon::ChevronDown, theme::ICON_SIZE, theme.text_subtle))
}
```

3f. `docs/SPEC.md`: Chat paragraph: replace `When the chat list is scrolled up, a small round down-arrow button floats above the composer at the column's right edge and scrolls to the end.` with `When the chat list is scrolled up, a quiet 28 px chevron button (no frame or shadow, a \`layer[0]\` hover) floats above the composer at the column's right edge and scrolls to the end; the omp transcript has the same button.` In the omp dock paragraph add after the status-row sentence: `Texts queued while omp works (\`col role=omp.queue\`, items \`row role=omp.queue.item\`) are 26 px pills (\`radius::PILL\`, \`layer[0]\` fill, META \`text_muted\`, a 14 px \`text_subtle\` icon, 4 px apart) above the card.`

- [ ] **Step 4: Run, expect PASS**

`cargo test -p mkrew-ui the_queue_roles_are_matched_whole -j 2`, `cargo test -p mkrew-ui dock:: -j 2`, then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshot and commit**

Smoke recipe as Task 14 with `docs/screenshots/tern-polish-scroll-end-dark.png` / `-light.png`: spawn `--agent spin` or the long fake agent so the Chat list scrolls, scroll up, shot. Read: a bare chevron at the right of the column with no circle, shadow or border; hover (move the mouse onto it, `shot`) shows a soft fill.

HUMAN_CHECKS (omp pane): queue two messages while omp works (type and press Enter twice): two 26 px pills above the card with the count and the edit chip at their right, and the transcript scroll button as a bare chevron. [INFERENCE] The tree under `omp.queue.item` follows Tern's CSS (icon, text, `omp.queue.count`, `omp.queue.edit`); the roles were not recorded from a live omp, so if an item shows the text unclipped, check the child kinds first.

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/tsp/dock.rs crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/chat_view.rs docs/SPEC.md docs/screenshots/tern-polish-scroll-end-dark.png docs/screenshots/tern-polish-scroll-end-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): omp queue pills and a quiet scroll-to-bottom button"
```

Note on ordering: Task 15 must leave the tree compiling. `float_button` still calls `round_neutral` and `round_frame`, so those two stay in Task 15 (it only deletes `round_button`, `round_stop_button`), and Task 16 deletes all three together with the new `scroll_end_button`.

---

## Open points for the coordinator

1. Working row left padding: spec says 16, this plan uses `space::M` (12) so the row lines up with the card text (the card's padding is `px(M)`). One token to change in `tsp_dock_status` if 16 is wanted.
2. `tsp_dock_working` label MEDIUM (spec 5, Part C's table) is applied in Task 15; Part C must not touch `tsp_dock_working`.
3. `tsp_dock_focus` (the focus strip, gap report row 24) is not in spec 6 and is left as it is; it still calls `theme::raised_border`, which Part A keeps.
4. The meter's labels rise 4 px into the 8 px gap above the card plus the status row's bottom padding; Task 14 names the fallback (`space::M` outer gap when a meter is present) if the HUMAN_CHECKS look shows a collision.
5. The effort glyph colour inside the model pill is drawn by omp's `effort` node through `tsp_node`, so spec 6's "effort glyph `text_subtle`" is not applied; changing it needs a colour argument in `tsp_live`, which is outside this part.


---

# Part E: Files tree, file pane header, Docs flavor, editor (Tasks 17-20, spec §7)

Plan against master c775413 plus Parts A-D. Run all cargo commands from the repo root with `-j 2`.
Part E touches none of the `fix/composer-polish` functions (`push_runs`, `tsp_image`, `tsp_attachment`, `thumbnail`, `tsp_dock*`, `composer_card_frame`).
`crates/mkrew-ui/src/preview.rs`, `icons.rs`, `theme.rs`, `editor.rs` are in the lib crate; `shell/*` is in the bin crate (`crate::*` glob). `cargo test -p mkrew-ui` runs both.

**Preconditions (check once before Task 17):**
- `rg "pub fn with_pick" crates/mkrew-ui/src/shell/widgets.rs` (Part B), `rg "pub subtle|pub layer" crates/mkrew-ui/src/theme.rs` (Part A: `Theme.subtle`, `Theme.layer`), `rg "CHAT_LINE_HEIGHT|CODE_LINE_HEIGHT: f32 = 1.6" crates/mkrew-ui/src/theme.rs` (Part C), `rg "doc::LINE_HEIGHT" crates` lists only `preview.rs` `page` (Part C moved `chat_view.rs` and `tsp_view.rs` to `CHAT_LINE_HEIGHT`), `rg "fn new" crates/mkrew-ui/src/preview.rs` shows `Ctx::new(page, flavor)` with fields `nested`, `muted` (Part C Task 12).
- `Theme.folder` stays (Part A sets it `= text_subtle`); Task 17 uses it for folder icons.
- Part C agreed (IRC): I add `doc::PAGE_HEADINGS` and a separate Docs Heading arm before theirs; `Ctx.quoted`; bullet/number marker edits.

---

### Task 17: Files tree (tiers, selected row, indent guides, icon map, git dot)

**Files:**
- Modify: `crates/mkrew-ui/src/shell/files_render.rs` (`file_row`; new `name_tone`, `marker_look`, `indent_guides`)
- Modify: `crates/mkrew-ui/src/icons.rs` (`icons!` list, new `Icon::for_file`, `mod tests`)
- Add: `icon/lucide/file-code.svg`, `file-cog.svg`, `file-image.svg`, `file-json.svg`, `file-spreadsheet.svg`, `lock.svg` (Lucide `lucide-static` 1.55.0, ISC, same version and source as ADR 0051; each file keeps its `@license` header)
- Modify: `crates/mkrew-ui/src/theme.rs` (doc comment of `FILE_CHANGES_DOT_SIZE`)
- Modify: `docs/SPEC.md` §12A Tree bullet; `docs/decisions/0117-tern-polish.md` (append a bullet under `## Decision`)
- Test: `crates/mkrew-ui/src/icons.rs` (`mod tests`), `crates/mkrew-ui/src/shell/files_render.rs` (new `mod tests`)

**Interfaces:**
- Consumes: `shell::widgets::with_pick<E: ParentElement + Styled>(el: E, picked: bool, theme: &Theme) -> E` (Part B); `Look::Chip` (hover fill `layer[0]`, label to `text`, pressed adds `layer[1]`); `Theme.layer: [u32; 4]`, `Theme.subtle`, `Theme.folder`.
- Produces: `Icon::for_file(name: &str) -> Icon`; private `files_render::name_tone(theme: &Theme, lit: bool, dim: bool) -> u32`.

- [ ] **Step 1: Write the failing tests**

In `icons.rs` `mod tests`, after `every_icon_is_served_and_is_a_lucide_svg`:

```rust
    #[test]
    fn files_map_to_icons_by_name_then_extension() {
        let cases = [
            ("main.rs", Icon::FileCode),
            ("App.TSX", Icon::FileCode),
            ("README.MD", Icon::FileText),
            ("notes.txt", Icon::FileText),
            ("package.json", Icon::FileJson),
            ("package-lock.json", Icon::Lock),
            ("pnpm-lock.yaml", Icon::Lock),
            ("Cargo.lock", Icon::Lock),
            (".env", Icon::Lock),
            (".env.local", Icon::Lock),
            ("Cargo.toml", Icon::FileCog),
            ("ci.yml", Icon::FileCog),
            (".gitignore", Icon::Settings),
            (".bashrc", Icon::Settings),
            ("vite.config.ts", Icon::Settings),
            ("logo.PNG", Icon::FileImage),
            ("data.csv", Icon::FileSpreadsheet),
            ("Makefile", Icon::File),
            ("archive.tar.gz", Icon::File),
            (".hidden", Icon::File),
            ("", Icon::File),
        ];
        for (name, icon) in cases {
            assert_eq!(Icon::for_file(name), icon, "{name}");
        }
    }
```

In `files_render.rs`, at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_have_three_tiers_and_a_lit_row_reads_at_full_strength() {
        for theme in [Theme::dark(), Theme::light()] {
            assert_eq!(name_tone(&theme, false, false), theme.text_muted);
            assert_eq!(name_tone(&theme, false, true), theme.text_subtle);
            assert_eq!(name_tone(&theme, true, false), theme.text);
            assert_eq!(name_tone(&theme, true, true), theme.text, "an opened dot file");
        }
    }
}
```

- [ ] **Step 2: Run, expect FAIL** (compile error: `Icon::for_file`, `name_tone` missing)
`cargo test -p mkrew-ui files_map_to_icons -j 2` and `cargo test -p mkrew-ui names_have_three_tiers -j 2`

- [ ] **Step 3: Implement**

a) Vendor the six icons (all return HTTP 200 for 1.55.0):
```
for n in file-code file-cog file-image file-json file-spreadsheet lock; do curl -fL -o icon/lucide/$n.svg https://unpkg.com/lucide-static@1.55.0/icons/$n.svg; done
```
Each must start with `<!-- @license lucide-static v1.55.0 - ISC -->`; `every_icon_is_served_and_is_a_lucide_svg` checks the rest.

b) `icons.rs`: replace the lines `File => "file",` through `FileText => "file-text",` with
```rust
        File => "file",
        FileCode => "file-code",
        FileCog => "file-cog",
        FileDiff => "file-diff",
        FileImage => "file-image",
        FileJson => "file-json",
        Files => "files",
        FileSpreadsheet => "file-spreadsheet",
        FileText => "file-text",
```
and after `ListFilter => "list-filter",` add `Lock => "lock",`.
Add after the second `icons!` block (before `impl AgentLogo`... any spot after `icons!` for `Icon`; put it directly under the first `icons!` invocation):

```rust
impl Icon {
    /// The icon of a Files tree file (SPEC §12A): lock files and `.env`, tool config
    /// (dotfiles ending `rc`, `.gitignore`, `*.config.*`), then by extension. Names are
    /// compared without case and without allocating (rows are drawn every frame).
    pub fn for_file(name: &str) -> Icon {
        let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
        let ext_in = |list: &[&str]| list.iter().any(|e| ext.eq_ignore_ascii_case(e));
        let ends = |suffix: &str| {
            name.len() >= suffix.len()
                && name.as_bytes()[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
        };
        let env = name.len() >= 4 && name.as_bytes()[..4].eq_ignore_ascii_case(b".env");
        if env || ext_in(&["lock"]) || ends("-lock.json") || ends("-lock.yaml") {
            Icon::Lock
        } else if matches!(name, ".gitignore" | ".gitattributes" | ".editorconfig")
            || (name.starts_with('.') && ends("rc"))
            || name.contains(".config.")
        {
            Icon::Settings
        } else if ext_in(&["json", "jsonc", "json5"]) {
            Icon::FileJson
        } else if ext_in(&["toml", "yaml", "yml", "ini", "cfg", "conf", "properties"]) {
            Icon::FileCog
        } else if ext_in(&["md", "markdown", "mdx", "txt", "rst", "log"]) {
            Icon::FileText
        } else if ext_in(&["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif"]) {
            Icon::FileImage
        } else if ext_in(&["csv", "tsv", "xls", "xlsx", "ods"]) {
            Icon::FileSpreadsheet
        } else if ext_in(&[
            "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "c", "h",
            "cc", "cpp", "hpp", "cs", "rb", "php", "swift", "sh", "bash", "ps1", "bat", "cmd",
            "html", "htm", "css", "scss", "vue", "svelte", "sql", "lua", "zig", "dart",
        ]) {
            Icon::FileCode
        } else {
            Icon::File
        }
    }
}
```
Check against the test: `.hidden` has ext `hidden` and does not end `rc` → File; `.bashrc` starts with `.` and ends `rc` → Settings; `""` → File; `.env.local` → Lock.

c) `files_render.rs`: replace the whole `file_row` (from its doc comment to its closing `}`, lines 280-442) with the code below, and add the helpers before it.

```rust
/// The row's own text color; hover lifts it to `text` (`Look::Chip`), a git-changed name
/// keeps its tint on top of it. The opened row and the cursor row are `text`, ignored and
/// dot files `text_subtle`, every other name `text_muted` (SPEC §12A).
fn name_tone(theme: &Theme, lit: bool, dim: bool) -> u32 {
    if lit {
        theme.text
    } else if dim {
        theme.text_subtle
    } else {
        theme.text_muted
    }
}

/// A git marker's color (the name's tint and the row-end dot) and its tooltip, which carries
/// the letter the tree no longer draws.
fn marker_look(marker: GitMarker, theme: &Theme) -> (u32, &'static str) {
    match marker {
        GitMarker::Modified => (theme.git_modified, "Modified (M)"),
        GitMarker::Added => (theme.diff_add_text, "Added (A)"),
        GitMarker::Deleted => (theme.danger_text, "Deleted (D)"),
        GitMarker::Untracked => (theme.diff_add_text, "Untracked (?)"),
    }
}

/// One cell per ancestor level, each as wide as one indent, with a 1 px `layer[1]` line at
/// the centre of that level's chevron. The cells are in the flow (a `uniform_list` row has no
/// neighbours to draw a continuous line), so the row's content starts where `pl(S)` plus
/// `depth` indents put it before.
fn indent_guides(depth: usize, theme: &Theme) -> gpui::Div {
    let line = gpui::rgba(theme.layer[1]);
    div()
        .flex()
        .flex_none()
        .h_full()
        .children((0..depth).map(move |_| {
            div()
                .flex_none()
                .w(px(theme::FILE_TREE_INDENT))
                .h_full()
                .child(
                    div()
                        .ml(px(theme::TREE_CHEVRON_SIZE / 2.0))
                        .w(px(theme::BORDER_WIDTH))
                        .h_full()
                        .bg(line),
                )
        }))
}

/// One Files tree row (SPEC §12A): a guide per ancestor level, a chevron on folders, an icon
/// by file type, the name, and a 6 px git dot at the end (a folder holding changes has one
/// too). Every row is one body line high (the tree is a uniform list).
pub(crate) fn file_row(
    project_id: Ulid,
    row: &files_view::Row,
    state: &FileRowState<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (entry, depth, expanded) = match row {
        files_view::Row::Note { depth, text } => {
            return div()
                .w_full()
                .h(theme::rems_px(theme::TREE_ROW_HEIGHT))
                .flex()
                .items_center()
                .pl(px(space::S))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(indent_guides(*depth, theme))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(text.clone()),
                )
                .into_any_element();
        }
        files_view::Row::Entry {
            entry,
            depth,
            expanded,
        } => (entry, *depth, *expanded),
    };
    let at_cursor = state.cursor == Some(entry.path.as_str());
    let opened = state.opened == Some(entry.path.as_str());
    let lit = at_cursor || opened;
    let dim = entry.ignored || entry.name.starts_with('.');
    let chevron = match (entry.is_dir, expanded) {
        (false, _) => None,
        (true, false) => Some(Icon::ChevronRight),
        (true, true) => Some(Icon::ChevronDown),
    };
    let icon = if entry.is_dir {
        Icon::Folder
    } else {
        Icon::for_file(&entry.name)
    };
    let icon_color = match (entry.is_dir, opened) {
        (true, _) => theme.folder,
        (false, true) => theme.accent,
        (false, false) => theme.text_subtle,
    };
    let marker = entry.marker.map(|marker| marker_look(marker, theme));
    let path = entry.path.clone();
    let style = PopupStyle::of(theme);
    let accent = theme.accent;
    let (menu_path, is_dir) = (entry.path.clone(), entry.is_dir);
    // The border is always there (so rows never shift); it only shows as the focus ring.
    let ring: gpui::Hsla = if at_cursor && state.focused {
        rgb(theme.pane_focus_stripe).into()
    } else {
        gpui::transparent_black()
    };
    div()
        .id(SharedString::from(format!("file-{}", entry.path)))
        .w_full()
        .h(theme::rems_px(theme::TREE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .pl(px(space::S))
        .pr(px(space::S))
        .rounded(px(radius::CHIP))
        .border(px(theme::BORDER_WIDTH))
        .border_color(ring)
        .text_size(font_size::BODY)
        .text_color(rgb(name_tone(theme, lit, dim)))
        // The cursor row without the opened tint is a plain hover fill; focus adds the ring.
        .when(at_cursor && !opened, |el| el.bg(gpui::rgba(theme.layer[0])))
        .map(|el| with_pick(el, opened, theme))
        .pressable_unless(lit, Look::Chip, theme)
        .when(at_cursor, |el| {
            el.relative()
                .child(record_bounds(Anchor::FileRow(entry.path.clone()), cx))
        })
        .child(indent_guides(depth, theme))
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::TREE_CHEVRON_SIZE))
                .children(chevron.map(|icon| {
                    icon_svg(icon, theme::TREE_CHEVRON_SIZE, theme.subtle)
                })),
        )
        .child(icon_svg(icon, theme::TREE_ICON_SIZE, icon_color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                // Inherits the row's color (so hover can lift it); a changed name keeps its tint.
                .when_some(marker, |el, (color, _)| el.text_color(rgb(color)))
                .when(opened, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(entry.name.clone()),
        )
        .children(marker.map(|(color, what)| {
            div()
                .id(SharedString::from(format!("file-marker-{}", entry.path)))
                .flex_none()
                .size(px(theme::FILE_CHANGES_DOT_SIZE))
                .rounded_full()
                .bg(rgb(color))
                .tooltip(tooltip(theme, what, ""))
        }))
        .when(entry.is_dir && entry.has_changes, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("file-changes-{}", entry.path)))
                    .flex_none()
                    .size(px(theme::FILE_CHANGES_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(theme.git_modified))
                    .tooltip(tooltip(theme, "Contains changes", "")),
            )
        })
        // .on_drag(..), .on_press(..), .on_mouse_down(Right, ..) and `.into_any_element()`:
        // unchanged from master (lines 403-441).
```
Keep the `.on_drag(...)`, `.on_press(...)`, `.on_mouse_down(MouseButton::Right, ...)` and `.into_any_element()` tail of the old function exactly as it is; only the part above changes.

d) `theme.rs`: doc of `FILE_CHANGES_DOT_SIZE` becomes `/// Side of the git dot of a Files tree row: a changed file's marker and a folder holding changes.`

e) `docs/SPEC.md` §12A Tree bullet: replace "git status markers next to names (M modified, A added, D deleted, ? untracked), a small dot on folders containing changes." with "git status as a 6 px dot at the row end in the status color with the name tinted (modified, added or untracked, deleted; the letter M / A / D / ? is in the dot's tooltip), the same dot on folders containing changes; names `text_muted` (hover, cursor and opened rows `text`, ignored and dot files `text_subtle`), a Lucide icon by file type in `text_subtle` (`Icon::for_file`; folders `folder`), 1 px indent guides, the opened row with the selected-row tint and bar and an accent icon (ADR 0117)."
Append to `docs/decisions/0117-tern-polish.md` after the last bullet of `## Decision`: "- **Files tree.** Names `text_muted`, hover / cursor / opened `text`, ignored and dot files `text_subtle`; opened row = selected-row helper with an accent icon; 1 px `layer[1]` indent guides; icons by extension (`Icon::for_file`, six Lucide icons added from `lucide-static` 1.55.0: file-code, file-cog, file-image, file-json, file-spreadsheet, lock); git status = 6 px dot plus tinted name, the letter moves to the tooltip."

- [ ] **Step 4: Run, expect PASS**
`cargo test -p mkrew-ui files_map_to_icons -j 2`; `cargo test -p mkrew-ui names_have_three_tiers -j 2`; `cargo test -p mkrew-ui every_icon_is_served -j 2`; then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshots** (dark and light; the sandbox workspace `api` has `src/main.rs` modified, `src/new.rs` untracked, `docs/plan.md`, `README.md`)
`cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins -j 2`; `powershell -File scripts/smoke.ps1 start -Theme dark`; `powershell -File scripts/smoke.ps1 keys -Keys "^+3"` (Files tab); click the `src` row, then `docs`, then `README.md` (fractions read off a first `shot`); `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-files-tree-dark.png`; `stop`. Repeat with `-Theme light` into `...-light.png`. Check: names muted, opened row tint plus bar with accent icon, guides under `src`, a dot on `main.rs` and `new.rs` with tinted names, hover lifts a name to full text.

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/theme.rs icon/lucide/file-code.svg icon/lucide/file-cog.svg icon/lucide/file-image.svg icon/lucide/file-json.svg icon/lucide/file-spreadsheet.svg icon/lucide/lock.svg docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-files-tree-dark.png docs/screenshots/tern-polish-files-tree-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Tern files tree with text tiers, guides, type icons and git dots"`

---

### Task 18: File pane header (breadcrumb, icon buttons, dirty dot)

**Files:**
- Modify: `crates/mkrew-ui/src/shell/file_pane.rs` (`render_file_pane` header and its doc comment; new `crumbs`, `crumb`; `mod tests`)
- Modify: `docs/SPEC.md` (§12A header sentences), `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/shell/file_pane.rs` (`mod tests`)

**Interfaces:**
- Consumes: `icon_button(id, Icon, &Theme)`, `tooltip(theme, &str, impl Into<SharedString>)`, `Theme.subtle`, `theme::FILE_CHANGES_DOT_SIZE`.
- Produces: private `crumbs(path: &str) -> (impl Iterator<Item = &str>, &str)`.

- [ ] **Step 1: Write the failing test** (new module at the end of `file_pane.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::crumbs;

    #[test]
    fn a_path_splits_into_its_folders_and_its_name() {
        let (folders, name) = crumbs("docs/superpowers/plans/x.md");
        assert_eq!(folders.collect::<Vec<_>>(), ["docs", "superpowers", "plans"]);
        assert_eq!(name, "x.md");
        let (folders, name) = crumbs("README.md");
        assert_eq!(folders.count(), 0, "a root file has no crumbs");
        assert_eq!(name, "README.md");
        let (folders, name) = crumbs("a//b.rs");
        assert_eq!(folders.collect::<Vec<_>>(), ["a"], "empty segments are dropped");
        assert_eq!(name, "b.rs");
    }
}
```

- [ ] **Step 2: Run, expect FAIL** (compile error) - `cargo test -p mkrew-ui a_path_splits -j 2`

- [ ] **Step 3: Implement**

Module-level helpers (below the `impl Shell` block, above `mod tests`):

```rust
/// A file's path (`/` separated, relative to its folder) as the header's breadcrumb: its
/// folder segments, then its name.
fn crumbs(path: &str) -> (impl Iterator<Item = &str>, &str) {
    let (folders, name) = path.rsplit_once('/').unwrap_or(("", path));
    (folders.split('/').filter(|segment| !segment.is_empty()), name)
}

/// One crumb or its `›`: META, never shrinks.
fn crumb(text: impl Into<SharedString>, color: u32) -> gpui::Div {
    div()
        .flex_none()
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(text.into())
}
```

In `render_file_pane`: update the doc comment (header = breadcrumb, `Ln, Col`, Markdown modes, icon buttons Review, Save, Find, Open in Zed, close). Delete the `zed_button` closure (lines 88-103) but keep `zed_keys`, `zed_tip`. Keep `markdown`, `worktree`, `review_path`. `zed_button("file-zed-notice", true)` at the binary-file notice (line ~506) is the one remaining user: replace that call with
```rust
.child(
    button("file-zed-notice", icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, true)), theme, true)
        .flex_none()
        .tooltip(tooltip(theme, &zed_tip, zed_keys.clone()))
        .on_click({
            let path = path.clone();
            cx.listener(move |shell, _, _, cx| shell.open_file_in_zed(project_id, &path, cx))
        }),
)
```
Replace the whole `let header = div()...;` (lines 112-255) with:

```rust
        let (folders, name) = crumbs(&path.path);
        // A quiet icon button with its tooltip and keys.
        let tool = |id: &'static str, icon: Icon, tip: &str, keys: String| {
            icon_button(id, icon, theme).tooltip(tooltip(theme, tip, keys))
        };
        let save_tip = if dirty {
            "Save this file (it has unsaved changes)"
        } else {
            "Save this file"
        };
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w(px(theme::FILE_NAME_MIN_WIDTH))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            // The folders are cut on their left side first, the name last.
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap(px(space::XS))
                                    .children(folders.flat_map(|segment| {
                                        [
                                            crumb(segment.to_owned(), theme.text_subtle),
                                            crumb("›", theme.subtle),
                                        ]
                                    })),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(font_size::BODY)
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .text_color(rgb(theme.text))
                                    .child(name.to_owned()),
                            ),
                    )
                    // A worktree's file says which worktree: it looks like the project's otherwise.
                    // `flex_1` (basis 0): a long title is cut first, the path only when it alone does not fit.
                    .children(worktree.map(|worktree| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(worktree)
                    })),
            )
            // The buttons give way first: in a narrow pane the left ones are cut, the name stays.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::XS))
                    .min_w_0()
                    .overflow_hidden()
                    .children(doc.shown().filter(|_| editable).and_then(|shown| {
                        let at = shown.editor.cursor();
                        // The caret is always inside the text (the editor clamps it).
                        let line = shown.editor.lines().get(at.line)?;
                        let col = editor::display_col(line, at.col);
                        // The terminal font: its digits are all as wide (tabular).
                        Some(
                            div()
                                .flex_none()
                                .mr(px(space::S))
                                .font_family(theme.terminal_font_family.clone())
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_subtle))
                                .child(format!("Ln {}, Col {}", at.line + 1, col + 1)),
                        )
                    }))
                    .children(mode_toggle)
                    // The Docs view reviews the project folder's files, not a worktree's.
                    .when(
                        markdown && path.task.is_none() && self.features.docs,
                        |el| {
                            el.child(
                                tool(
                                    "file-review",
                                    Icon::FileText,
                                    "Review in Docs: review and comment on this file",
                                    String::new(),
                                )
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        shell.review_in_docs(
                                            project_id,
                                            review_path.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                        },
                    )
                    .when(editable, |el| {
                        el.child(
                            tool(
                                "file-save",
                                Icon::Save,
                                save_tip,
                                self.keymap.label(Command::SaveFile),
                            )
                            .relative()
                            // Unsaved: a dot on the icon's top right corner.
                            .when(dirty, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .top(px(space::XS))
                                        .right(px(space::XS))
                                        .size(px(theme::FILE_CHANGES_DOT_SIZE))
                                        .rounded_full()
                                        .bg(rgb(theme.accent)),
                                )
                            })
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.save_file(project_id, path.clone(), None, window, cx);
                                })
                            }),
                        )
                    })
                    .child(
                        tool(
                            "file-find",
                            Icon::Search,
                            "Find in this file",
                            self.keymap.label(Command::Find),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.open_file_find(false, window, cx)
                        })),
                    )
                    .child(
                        tool("file-zed", Icon::ExternalLink, &zed_tip, zed_keys.clone()).on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, _, cx| {
                                shell.open_file_in_zed(project_id, &path, cx);
                            })
                        }),
                    )
                    .child(
                        icon_button("file-close", Icon::X, theme)
                            .tooltip(tooltip(
                                theme,
                                "Close file",
                                self.keymap.label(Command::CloseFileTab),
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.close_file_pane(project_id, path.clone(), window, cx);
                                })
                            }),
                    ),
            );
```
The `●` prefix on the name is gone (the dot sits on Save; the tab keeps its own `●`). The Ln/Col text no longer changes the header's width class: it uses the terminal font so digits do not jiggle.

Docs: SPEC §12A Editor bullet "Unsaved changes show `●` before the path; Ctrl+S or "Save" writes the file" becomes "Unsaved changes show a dot on the Save button; Ctrl+S or Save writes the file". Add to the File pane bullet: "The header is a breadcrumb (folders META `text_subtle` with `›` in `subtle`, the name BODY MEDIUM `text`), `Ln, Col` (terminal font META `text_subtle`), the Markdown mode switch, then icon buttons with tooltips and keys: Review in Docs, Save, Find, Open in Zed, close." Append to ADR 0117 `## Decision`: "- **File pane header.** Breadcrumb instead of the full path; Save, Find, Open in Zed and Review in Docs are icon buttons; the unsaved mark is a dot on Save."

- [ ] **Step 4: Run, expect PASS** - `cargo test -p mkrew-ui a_path_splits -j 2`; `cargo clippy -p mkrew-ui --all-targets -- -D warnings`

- [ ] **Step 5: Screenshots** - `smoke.ps1 start -Theme dark`, double-click `src/main.rs` in the tree (a nested path shows the breadcrumb), type a character (dirty dot on Save), `shot -Out docs/screenshots/tern-polish-file-header-dark.png`, `stop`; same with `-Theme light`. Check the name stays visible when the pane is dragged narrow, and each icon button shows its tooltip with keys.

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/shell/file_pane.rs docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-file-header-dark.png docs/screenshots/tern-polish-file-header-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): file pane header breadcrumb with icon buttons and a dirty dot"`

---

### Task 19: Docs flavor typography and blocks

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`DOC_PREVIEW_MAX_WIDTH`, `doc::BODY`, `doc::LINE_HEIGHT`, new `doc::PAGE_HEADINGS`)
- Modify: `crates/mkrew-ui/src/preview.rs` (`Ctx` and `Ctx::new`, `page`, `block_in` arms Heading/Paragraph/Quote/Table/Rule, `list_item`, `text`, new `body_color`, `doc_heading`)
- Modify: `docs/SPEC.md` §14 Docs preview line, `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/preview.rs` (`mod tests`)

**Interfaces:**
- Consumes (Part C Task 12): `Ctx::new(page: bool, flavor: Flavor) -> Ctx` with fields `page`, `flavor`, `nested`, `muted`; `doc::CODE_LINE_HEIGHT` 1.6; `doc::CHAT_LINE_HEIGHT`; chat's Quote arm passes `Ctx { muted: ctx.muted || chat, ..ctx }` and its Paragraph color is `if ctx.muted { text_muted } else { text }`. `Theme::tern_dark()`, `tern_light()` (Part A), `Theme.layer`, `Theme.divider`, `Theme.text_subtle`.
- Produces: `Ctx.quoted: bool`; `fn body_color(ctx: Ctx, theme: &Theme) -> u32`; `fn doc_heading(level: usize) -> (f32, f32)` (size, space above); `theme::doc::PAGE_HEADINGS: [f32; 6]`.

- [ ] **Step 1: Write the failing tests** (in `preview.rs` `mod tests`)

```rust
    fn ctx(flavor: Flavor) -> Ctx {
        Ctx::new(true, flavor)
    }

    #[test]
    fn running_text_is_muted_on_dark_docs_and_in_quotes_only() {
        let chat = Flavor::Chat {
            session: Ulid::nil(),
        };
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::tern_dark(),
            Theme::tern_light(),
        ] {
            let doc = body_color(ctx(Flavor::Doc), &theme);
            let want = if theme.dark { theme.text_muted } else { theme.text };
            assert_eq!(doc, want, "{}", theme.name);
            assert_eq!(body_color(ctx(chat), &theme), theme.text, "{}", theme.name);
            for flavor in [Flavor::Doc, chat] {
                let quoted = Ctx {
                    quoted: true,
                    ..ctx(flavor)
                };
                assert_eq!(body_color(quoted, &theme), theme.text_muted, "{}", theme.name);
            }
        }
    }

    #[test]
    fn doc_headings_shrink_stay_above_body_and_get_less_air() {
        let sizes: Vec<f32> = (1..=6).map(|level| doc_heading(level).0).collect();
        assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]), "{sizes:?}");
        assert!(sizes.iter().all(|size| *size >= theme::doc::BODY), "{sizes:?}");
        let tops: Vec<f32> = (2..=6).map(|level| doc_heading(level).1).collect();
        assert!(tops.windows(2).all(|pair| pair[0] >= pair[1]), "{tops:?}");
        assert_eq!(doc_heading(1).1, 0.0, "the title sits on the page's own top padding");
        assert_eq!(doc_heading(0), doc_heading(1));
        assert_eq!(doc_heading(9), doc_heading(6));
    }
```

- [ ] **Step 2: Run, expect FAIL** (compile error: `quoted`, `body_color`, `doc_heading`) - `cargo test -p mkrew-ui running_text_is_muted -j 2`

- [ ] **Step 3: Implement**

a) `theme.rs`:
```rust
/// Max width of the Docs flavor's column (file preview, Docs view): about 75 characters of
/// 15 px body text. Wide tables and diagrams shrink or scroll inside it.
pub const DOC_PREVIEW_MAX_WIDTH: f32 = 720.0;
```
In `pub mod doc`: `pub const BODY: f32 = 15.0;`, `pub const LINE_HEIGHT: f32 = 1.65;` (comment: Docs page only; chat has `CHAT_LINE_HEIGHT`, the UI 1.5), and after `HEADINGS` (leave `HEADINGS` and its comment untouched; chat reads it):
```rust
    /// The Docs flavor's h1-h6 (SPEC §14): the title is 32, no heading is under the body.
    pub const PAGE_HEADINGS: [f32; 6] = [32.0, 22.0, 17.0, 15.0, 15.0, 15.0];
```
`diagram.rs` takes `doc::BODY` as its font size, so drawn diagrams get 15 px text: expected, and `cargo test -p mkrew-ui diagram -j 2` in step 4.

b) `preview.rs` `struct Ctx`: add (with Part C's fields kept)
```rust
    /// Inside a block quote: running text is `text_muted` (SPEC §14).
    quoted: bool,
```
and `quoted: false` in `Ctx::new`. No per-site edits (Part C's `new` replaced the literals).

c) Helpers next to `chat_flavor`:
```rust
/// The color of running text (paragraphs, list items): `text_muted` in a quote, where Part C's
/// `muted` says so, and on the Docs flavor in a dark theme (Tern: body t2, bold and headings
/// t1); `text` otherwise (light themes, Chat).
fn body_color(ctx: Ctx, theme: &Theme) -> u32 {
    if ctx.muted || ctx.quoted || (theme.dark && !chat_flavor(ctx)) {
        theme.text_muted
    } else {
        theme.text
    }
}

/// A Docs-flavor heading: its size and the space above it. No rule under h1 or h2; the
/// bigger the heading, the more air before it (the block gap below is the page's 16).
fn doc_heading(level: usize) -> (f32, f32) {
    let level = level.clamp(1, theme::doc::PAGE_HEADINGS.len());
    let above = match level {
        1 => 0.0,
        2 => space::XXL,
        3 => space::XL,
        4 => space::L,
        _ => space::M,
    };
    (theme::doc::PAGE_HEADINGS[level - 1], above)
}
```

d) `page`: replace the wrapper padding
```rust
        div()
            .w_full()
            .px(px(space::XL))
            .when(index == 0, |el| el.pt(px(space::L)))
            .pb(px(if index == last { space::XXL } else { space::L }))
```
(the inner column div with `max_w(DOC_PREVIEW_MAX_WIDTH)`, `text_size(doc::BODY)`, `line_height(doc::LINE_HEIGHT)` is unchanged). Update the doc comment of `page`: "block gap 16, 32 below the last block".

e) `block_in`, add a new arm directly above the existing `BlockKind::Heading { level, runs } => {`:
```rust
        BlockKind::Heading { level, runs } if !chat_flavor(ctx) => {
            let (size, above) = doc_heading(usize::from(*level));
            div()
                .pt(px(above))
                .text_size(px(size))
                .line_height(relative(theme::doc::HEADING_LINE_HEIGHT))
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(selectable(pick, links(runs, ctx), |sel, bg| {
                    text(runs, theme, sel, bg, false)
                }))
                .into_any_element()
        }
```
After it, the generic Heading arm only sees Chat; Part C owns it (it may drop its `chat`/`major` branches; do not edit it here).
Paragraph arm: `.text_color(rgb(theme.text))` becomes `.text_color(rgb(body_color(ctx, theme)))`.
Quote arm (final state; Part C's chat edits, if any, are kept by this):
```rust
        BlockKind::Quote { alert, blocks } => {
            let look = alert.map(|alert| alert_look(alert, theme));
            let inner = Ctx {
                quoted: true,
                ..ctx
            };
            div()
                .flex()
                .gap(px(space::L))
                .child(
                    div()
                        .flex_none()
                        .w(px(theme::DOC_QUOTE_BAR_WIDTH))
                        .rounded(px(radius::SMALL))
                        .bg(rgb(look.map_or(theme.border, |(_, _, color)| color))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(space::S))
                        .text_color(rgb(theme.text_muted))
                        .children(look.map(|(icon, title, color)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .font_weight(FontWeight(weight::SEMIBOLD))
                                .text_color(rgb(color))
                                .child(icons::icon_svg(icon, theme::ICON_SIZE_SMALL, color))
                                .child(title)
                        }))
                        .children(blocks_in(blocks, shown, pick, inner, theme)),
                )
                .into_any_element()
        }
```
(If Part C's arm passes `Ctx { muted: ctx.muted || chat, ..ctx }`, write `Ctx { quoted: true, muted: ctx.muted || chat_flavor(ctx), ..ctx }` so both survive.)
Docs table (the non-chat branch of `BlockKind::Table`):
```rust
            div()
                .flex()
                .flex_col()
                .rounded(px(radius::LARGE))
                .border_1()
                .border_color(gpui::rgba(theme.layer[1]))
                .overflow_hidden()
                .child(
                    table_row(head, cols, pick, ctx, false, theme.text_muted, theme)
                        .bg(rgb(theme.surface))
                        .text_size(font_size::META),
                )
                .children(rows.iter().map(|row| {
                    table_row(row, cols, pick, ctx, false, theme.text, theme)
                        .border_t_1()
                        .border_color(rgb(theme.divider))
                }))
                .into_any_element()
```
(header is regular weight: the SEMIBOLD call is removed; cells already pad `py(S) px(M)`.)
Rule: `.my(px(space::S))` becomes `.my(px(space::XL))`.
`list_item`: the unnumbered marker `.bg(rgb(theme.accent))` (every flavor now; collapse Part C's chat-only `text_subtle` arm into this one) becomes `.bg(rgb(theme.text_subtle))`; the number marker becomes
```rust
        (None, Some(number)) => div()
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::META)
            .text_color(rgb(theme.text_subtle))
            .child(format!("{number}."))
            .into_any_element(),
```
`text()`: bold runs read as `text` over the muted body. Before the loop add `let strong = Hsla::from(rgb(theme.text));` and in `base` append `.or(style.bold.then_some(strong))` to the existing `color:` expression (keep whatever link color Part C left: `color: <link expr>.or(style.bold.then_some(strong)),`).

f) Docs: SPEC §14 "Docs preview" line becomes "Docs preview (ADR 0021, 0117): body 15 px, line height 1.65, headings 32/22/17/15/15/15 px with no rule (h2 `space::XXL` above, h3 `space::XL`, h4 `space::L`), code 13 px at 1.6, block gap 16, a centered column of at most 720 px (ADR 0083); paragraph and list text `text_muted` in dark themes and `text` in light, bold and headings `text`." ADR 0117 bullet: "- **Docs flavor.** Column 960 → 720, body 14 → 15 at 1.5 → 1.65 (amends ADR 0116's 1.5), h1 32 / h2 22 with no rules, bullets `text_subtle` (amends ADR 0105's accent dot), ordered numbers terminal META `text_subtle`, quote gap 16, table header META `text_muted`, ring `layer[1]`, hr `my(XL)`."

- [ ] **Step 4: Run, expect PASS** - `cargo test -p mkrew-ui running_text_is_muted -j 2`; `cargo test -p mkrew-ui doc_headings -j 2`; `cargo test -p mkrew-ui preview -j 2`; `cargo test -p mkrew-ui diagram -j 2`; `cargo test -p mkrew-ui markdown -j 2` (the markdown snapshots do not depend on styling and must stay unchanged); `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshots** - `smoke.ps1 start -Theme dark`, then write a sample into the tree (it updates live): 
```
@'
# Title
Intro with **bold**, `code` and a [link](https://example.com).
## Section
Body paragraph that is long enough to wrap onto a second line so the line height and the column width show: lorem ipsum dolor sit amet, consectetur adipiscing elit.
### Sub
- one
- two
1. first
2. second
> quoted text
---
| a | b |
|---|---|
| 1 | 2 |
#### Small
```rust
fn main() {}
```
'@ | Set-Content "$env:TEMP\mkrew-sandbox\ws\api\docs\guide.md"
```
open `docs/guide.md` (Preview mode), `shot -Out docs/screenshots/tern-polish-docs-preview-dark.png`, `stop`; same for light. Check: column about 720 wide, no rules under h1/h2, muted body in dark and bold at full text, subtle bullets and numbers (if the numbers sit visibly high against the 15 px line, add `.mt(px(1.0))` to that marker only), 16 gap between blocks, 32 below the last.

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/preview.rs docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-docs-preview-dark.png docs/screenshots/tern-polish-docs-preview-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Docs flavor reading rhythm (720 column, 15/1.65, quiet headings and blocks)"`

---

### Task 20: Editor gutter, current line, caret, code line height

**Files:**
- Modify: `crates/mkrew-ui/src/preview.rs` (`code_row` and its doc comment, `source` gutter)
- Modify: `crates/mkrew-ui/src/theme.rs` (`shared()` `code_line_height`; `problems()` rule "code" grounds)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__*.snap` (re-accept; includes the Tern snapshots Part A added)
- Modify: `docs/SPEC.md` line "Editor code rows keep line height 1.35", `docs/decisions/0117-tern-polish.md`
- Test: `crates/mkrew-ui/src/theme.rs` (`mod tests`)

**Interfaces:**
- Consumes: `Theme.row_hover`, `Theme.text_subtle`, `Theme.accent`; `doc::CODE_LINE_HEIGHT` 1.6 is Part C's (not touched here; Docs code blocks already use it).
- Produces: `code_line_height = 1.5` for every theme (Files editor, `source`, the picking and PageUp math read it, so they follow).

- [ ] **Step 1: Write the failing test** (theme.rs `mod tests`, after `each_contrast_rule_fires_when_its_token_is_broken`)

```rust
    #[test]
    fn code_colors_are_checked_on_the_current_line_band() {
        let t = Theme::dark();
        // Fine on `bg`, unreadable on the band that the caret row is drawn on.
        let broken = Theme {
            row_hover: t.syntax_keyword,
            ..t.clone()
        };
        let found = problems(&broken);
        let want = format!("{:06X} on {:06X}", t.syntax_keyword, t.syntax_keyword);
        assert!(
            found
                .iter()
                .any(|p| p.rule == "code" && p.detail.starts_with(&want)),
            "{found:?}"
        );
    }
```

- [ ] **Step 2: Run, expect FAIL** - `cargo test -p mkrew-ui code_colors_are_checked -j 2` (no "code" problem yet: the band is not a checked ground)

- [ ] **Step 3: Implement**

a) `theme.rs` `problems()`: replace
```rust
    // `surface_raised` is the Files viewer's selected line.
    for fg in code.into_iter().chain([t.accent]) {
        for bg in [t.bg, t.surface, t.surface_raised] {
```
with
```rust
    // `row_hover` is the band under the Files editor's caret row; `surface_raised` is the
    // Docs view's selected line.
    for fg in code.into_iter().chain([t.accent]) {
        for bg in [t.bg, t.surface, t.surface_raised, t.row_hover] {
```
In `Theme::shared()`: `code_line_height: 1.35,` → `code_line_height: 1.5,`. Field doc (line ~1057): "Line height of code rows in the Files editor and the source view (tighter than the terminal's, Tern's 1.5)."

b) `preview.rs` `code_row`: doc comment sentence "The caret's row is raised and its number accented." → "The caret's row has a faint `row_hover` band and its number at full `text`; the caret is `accent` and does not blink." Replace
```rust
        // A raised band plus an accent number: syntax colors and muted comments stay
        // ≥ 4.5:1 on it.
        .when(current, |el| el.bg(rgb(theme.surface_raised)))
```
with
```rust
        // A faint band (`problems()` keeps syntax colors and comments ≥ 4.5:1 on it).
        .when(current, |el| el.bg(rgb(theme.row_hover)))
```
the gutter color
```rust
                .text_color(rgb(if current {
                    theme.text
                } else {
                    theme.text_subtle
                }))
```
and the caret `.bg(rgb(theme.text))` → `.bg(rgb(theme.accent))` (the editor already has no blink).
In `source()`: `let (text, muted, selection_bg) = (theme.text, theme.text_muted, theme.terminal_selection_bg);` → `let (text, subtle, selection_bg) = (theme.text, theme.text_subtle, theme.terminal_selection_bg);` and `.text_color(rgb(muted))` → `.text_color(rgb(subtle))`.

c) Re-accept: `cargo test -p mkrew-ui theme -j 2` fails on the theme snapshots; `cargo insta accept`; then `git diff -U0 crates/mkrew-ui/src/snapshots | rg "^[+-] "` must show only `code_line_height: 1.35` → `1.5` lines (two per snapshot file, Mkrew Dark/Light and Tern Dark/Light). Anything else means another edit leaked: revert it.

d) Docs: SPEC §14 "Editor code rows keep line height 1.35 (`code_line_height`)." → "Editor code rows have line height 1.5 (`code_line_height`); the editor gutter is `text_subtle` with the caret row's number `text`, the caret row has a `row_hover` band and the caret is `accent` (ADR 0117)." ADR bullet: "- **Editor.** Gutter `text_subtle`, current number `text` (no accent), current line `row_hover` band (now a ground of the "code" contrast rule), caret `accent`, `code_line_height` 1.35 → 1.5."

- [ ] **Step 4: Run, expect PASS** - `cargo test -p mkrew-ui code_colors_are_checked -j 2`; `cargo test -p mkrew-ui theme -j 2` (every built-in has empty `problems()`, snapshots green after accept); `cargo test -p mkrew-ui editor -j 2`; `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.

- [ ] **Step 5: Screenshots** - `smoke.ps1 start -Theme dark`, open `src/main.rs`, click into line 2 (the caret row), `shot -Out docs/screenshots/tern-polish-editor-dark.png`, `stop`; same with `-Theme light`. Check: dim numbers, the current number bright, a faint band, an accent caret, 19.5 px rows, clicks still land on the right column (row math reads `code_line_height`).

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-editor-dark.png docs/screenshots/tern-polish-editor-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): quieter editor gutter, faint current line, accent caret, 1.5 line height"`


---

## Part F: Window chrome (Task 21, spec §8)

Spec §8 has four decisions: the sidebar ground and rows, active panel tabs, the 1 px focus stripe, and "Tabs / Panes unchanged". Tabs (`tab_frame`) and panes need no code: `tab_frame` reads `tab_active` / `text` / `text_muted` / `Look::Chip` and so follows Part A's new values and Part B's `layer[0]` hover on its own. `chrome.rs` has no value to change: the title bar already paints `pane_gap`, and the sidebar now paints the same ground, so the left of the title bar and the sidebar join into one band (the divider the sidebar lost is replaced by the 1.06:1 ground step to `bg`, held by `PANE_GAP_MIN_CONTRAST`).

Task 21 is written as two commits in one task, so each half has its own reviewer gate:

- **21a** the focus stripe (pure, tested): `status::pane_stripe` returns the colour and the thickness.
- **21b** the sidebar and panel tabs (styling, screenshots): no `classic`, no `border_r`, tint-plus-bar rows, 28 px `well` filter, accent panel tab.

Both come after Part A (classic removed, `layer`, Tern values) and Part B (`with_pick`, `Look::Row` hover = `layer[0]` + label lift to `text`, `field_frame` ring and placeholder). 21a only needs master; 21b needs A and B merged.

Notes for the coordinator:
- `theme.sidebar` must equal `pane_gap` in every theme. Builtins get it from §3.2; `from_palette` currently sets `sidebar: p.chrome` (theme.rs ~1816). Asked PlanA_Theme to set it and to test it (message sent).
- Touches none of the `fix/composer-polish` functions (`push_runs`, `tsp_image`, `tsp_attachment`, `thumbnail`, `space_render.rs`, `window_mode.rs`). It does edit `theme.rs`, which that branch also edits (only a constant added next to `SESSION_ROW_HEIGHT` and a doc comment on `PANE_ACCENT_WIDTH`).
- Left alone on purpose: `pane_edge` keeps `top(PANE_ACCENT_WIDTH)` (the grip sits under the thicker 2 px stripe; with the 1 px focus stripe there is a 1 px gap, invisible); `browser.rs:813` uses `PANE_ACCENT_WIDTH` for the load line, not the stripe; the other `pane_focus_stripe` row borders (git, jira, cards, files) belong to Parts E and G.

### Task 21: Window chrome: 1 px focus stripe, sidebar on the page ground, tint-plus-bar rows, accent panel tab

#### 21a. Focus stripe 1 px, needs-input and session colour stay 2 px

**Files:**
- Modify: `crates/mkrew-ui/src/status.rs` (`pane_stripe`, new `Stripe`, `mod tests`)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`pane_frame`, the `.when_some(stripe, ..)` block and the doc comment)
- Modify: `crates/mkrew-ui/src/theme.rs` (doc comment of `PANE_ACCENT_WIDTH`)
- Modify: `docs/SPEC.md` (§14 Pane block, "One `PANE_ACCENT_WIDTH` (2 px) stripe ..." and the "Focused pane" line)
- Test: `crates/mkrew-ui/src/status.rs` (`mod tests`: three tests replaced)

**Interfaces:**
- Consumes: `theme::BORDER_WIDTH` (1.0), `theme::PANE_ACCENT_WIDTH` (2.0), `Theme::pane_focus_stripe`, `Theme::status_needs_input`.
- Produces: `pub struct Stripe { pub color: u32, pub width: f32 }` (`Clone, Copy, Debug, PartialEq`) and `pub fn pane_stripe(theme: &Theme, state: Option<SessionState>, color: Option<u32>, several: bool, focused: bool) -> Option<Stripe>`. Only `pane_render.rs` calls it.

- [ ] **Step 1: Write the failing tests.** In `status.rs` `mod tests`, replace the three tests `needs_input_wins_the_stripe_over_a_color_and_focus`, `a_session_color_wins_the_stripe_over_focus` and `focus_marks_the_stripe_only_among_several_panes` with:

```rust
    /// The 2 px stripes (needs input, a session colour).
    fn thick(color: u32) -> Option<Stripe> {
        Some(Stripe {
            color,
            width: theme::PANE_ACCENT_WIDTH,
        })
    }

    #[test]
    fn needs_input_wins_the_stripe_over_a_color_and_focus() {
        for theme in [Theme::dark(), Theme::light()] {
            let needs = Some(SessionState::NeedsInput);
            let color = Some(0x112233);
            for (several, focused) in [(false, false), (true, false), (true, true)] {
                assert_eq!(
                    pane_stripe(&theme, needs, color, several, focused),
                    thick(theme.status_needs_input),
                    "several={several} focused={focused}"
                );
            }
            assert_eq!(
                pane_stripe(&theme, needs, None, false, false),
                thick(theme.status_needs_input),
                "a lone pane needing input still shows it"
            );
        }
    }

    #[test]
    fn a_session_color_wins_the_stripe_over_focus_and_stays_thick() {
        for theme in [Theme::dark(), Theme::light()] {
            let working = Some(SessionState::Working);
            assert_eq!(
                pane_stripe(&theme, working, Some(0x112233), true, true),
                thick(0x112233),
                "focused among several: the colour, not the thin focus stripe"
            );
            assert_eq!(
                pane_stripe(&theme, working, Some(0x112233), true, false),
                thick(0x112233)
            );
            assert_eq!(
                pane_stripe(&theme, None, Some(0x112233), false, false),
                thick(0x112233),
                "a lone colored pane keeps its stripe (ADR 0037)"
            );
        }
    }

    #[test]
    fn focus_marks_a_one_pixel_stripe_only_among_several_panes() {
        for theme in [Theme::dark(), Theme::light()] {
            let working = Some(SessionState::Working);
            assert_eq!(
                pane_stripe(&theme, working, None, true, true),
                Some(Stripe {
                    color: theme.pane_focus_stripe,
                    width: theme::BORDER_WIDTH,
                })
            );
            assert_eq!(pane_stripe(&theme, working, None, true, false), None);
            assert_eq!(
                pane_stripe(&theme, working, None, false, true),
                None,
                "lone pane"
            );
            assert_eq!(pane_stripe(&theme, None, None, false, false), None);
        }
    }
```

- [ ] **Step 2: Run, expect FAIL** (compile error: `cannot find type Stripe`): `cargo test -p mkrew-ui status:: -j 2`

- [ ] **Step 3: Implement.**

`status.rs`: replace the doc comment and body of `pane_stripe` (and add the struct above it):

```rust
/// The stripe along the top of a pane: its colour and thickness in px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stripe {
    pub color: u32,
    pub width: f32,
}

/// The stripe along the top of a pane, first match wins: amber while the session needs input
/// and the session's own colour (both [`theme::PANE_ACCENT_WIDTH`], they must be seen at a
/// glance), then the faint accent when focused among several panes (one
/// [`theme::BORDER_WIDTH`] line, ADR 0117), else none (ADR 0108; a lone pane blends into
/// the frame).
pub fn pane_stripe(
    theme: &Theme,
    state: Option<SessionState>,
    color: Option<u32>,
    several: bool,
    focused: bool,
) -> Option<Stripe> {
    let thick = |color| Stripe {
        color,
        width: theme::PANE_ACCENT_WIDTH,
    };
    if state == Some(SessionState::NeedsInput) {
        Some(thick(theme.status_needs_input))
    } else if let Some(color) = color {
        Some(thick(color))
    } else if several && focused {
        Some(Stripe {
            color: theme.pane_focus_stripe,
            width: theme::BORDER_WIDTH,
        })
    } else {
        None
    }
}
```

`pane_render.rs`, in `pane_frame` replace

```rust
            .when_some(stripe, |el, stripe| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(theme::PANE_ACCENT_WIDTH))
                        .bg(rgb(stripe)),
                )
            })
```

with

```rust
            .when_some(stripe, |el, stripe| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(stripe.width))
                        .bg(rgb(stripe.color)),
                )
            })
```

and in the doc comment of `pane_frame` change `(`status::pane_stripe`: needs input, the session color or the focus among several; ADR 0108)` to `(`status::pane_stripe`: needs input and the session color 2 px, the focus among several 1 px; ADR 0108, 0117)`.

`theme.rs`, the doc comment of `PANE_ACCENT_WIDTH` becomes:

```rust
/// Thickness of the stripe along the top edge of a pane that needs input or has a session
/// color (ADR 0108), and of an underlined tab (the Git panel's repo and section tabs). The
/// focus stripe among several panes is the thinner [`BORDER_WIDTH`] (ADR 0117).
pub const PANE_ACCENT_WIDTH: f32 = 2.0;
```

`docs/SPEC.md` §14: replace `One `PANE_ACCENT_WIDTH` (2 px) stripe runs along the pane's full top edge.` with `A stripe runs along the pane's full top edge: 2 px (`PANE_ACCENT_WIDTH`) for needs input and a pane color, 1 px (`BORDER_WIDTH`) for the focus.`; replace `- Focused pane: a 2 px stripe of` with `- Focused pane: a 1 px stripe of`. Add to `docs/decisions/0117-tern-polish.md` (Consequences) one bullet: `The focus stripe among several panes is 1 px; needs input and a session colour stay 2 px.`

- [ ] **Step 4: Run, expect PASS:** `cargo test -p mkrew-ui status:: -j 2`, then `cargo clippy -p mkrew-ui --all-targets -- -D warnings` (this also compiles `pane_render.rs`).

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/status.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/decisions/0117-tern-polish.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): 1 px focus stripe among several panes, needs-input and color stay 2 px"
```

Screenshot for 21a is covered by 21b's step (one split window shows both): the focus stripe is visible only in a split tab. In 21b Step 5 open a second pane (`smoke.ps1 keys -Keys "^+d"` is the split-right key per the keymap; if it differs, use the title bar's `tab-split` button with `click`).

---

#### 21b. Sidebar on the page ground, tint-plus-bar rows, 28 px `well` filter, accent panel tab

**Files:**
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` (`render_sidebar`, `render_compact_sidebar`, `render_run_group` line rows, `workspace_row`, `workspace_card`, `session_row`, `compact_workspace_row`, `compact_session_row`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`panel_tab`)
- Modify: `crates/mkrew-ui/src/theme.rs` (new const `SIDEBAR_FILTER_HEIGHT`)
- Modify: `docs/SPEC.md` (§14 Pane block line "Two panes meet at a 1 px `divider` line, the same line that sets the sidebar and the right panel apart."; the right-panel heights line "28 px icon tabs")
- Test: none. Pure styling; the only rule that can be tested (`sidebar == pane_gap` in every theme) is Part A's theme test. Gate: clippy plus screenshots.

**Interfaces:**
- Consumes (Part A): `Theme::layer: [u32; 4]` (RGBA), `Theme::sidebar == Theme::pane_gap`, `Theme::well`, `Theme::accent_bg`, `Theme::accent_text`, `Theme::divider`; `Theme::classic()` is gone.
- Consumes (Part B): `widgets::with_pick<E: ParentElement + Styled>(el: E, picked: bool, theme: &Theme) -> E` (adds `relative()`, `accent_bg` fill, a 2x14 accent bar at the left; label colour stays the caller's); `Look::Row` hover = `layer[0]` and label lift to `text`; `caret_field` / `field_frame` focus ring (accent border plus halo) and placeholder `subtle` (set once in `field.rs` `empty_field`, which this task does not touch).
- Produces: `theme::SIDEBAR_FILTER_HEIGHT: f32 = 28.0`; `panel_tab` keeps its signature `panel_tab(id, glyph: Icon, active: bool, theme: &Theme) -> Stateful<Div>`.

- [ ] **Step 1: Tests.** None to write (see Files). Confirm the rule it depends on exists: `cargo test -p mkrew-ui theme:: -j 2` is green after Part A, including its `sidebar == pane_gap` test.

- [ ] **Step 2: Baseline.** `cargo clippy -p mkrew-ui --all-targets -- -D warnings` is clean before editing (so any warning after is ours: `classic` removal leaves `theme` shadowing and the unused `INPUT_HEIGHT` use to check).

- [ ] **Step 3: Implement.**

`theme.rs`, next to the other sidebar heights (after `SESSION_ROW_HEIGHT`):

```rust
/// The sidebar's filter field (spec §8): one step lower than a dialog input
/// ([`INPUT_HEIGHT`]) so it sits quietly above the All / Working / Needs you control.
pub const SIDEBAR_FILTER_HEIGHT: f32 = 28.0;
```

`sidebar.rs`, `render_sidebar`:

1. Delete the two lines (comment and shadow):
```rust
            // The session list keeps its pre-restyle look (out of the restyle's scope).
            let theme = theme.classic();
```
2. Ground: delete `.border_r(px(theme::BORDER_WIDTH))` and `.border_color(rgb(theme.divider))` from the outer `div()` (keep `.bg(rgb(theme.sidebar))`), so it reads:
```rust
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .relative()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .font_family(theme.ui_font_family.clone())
            .children(header)
```
3. Filter field: change its height and fill and ring (the ring stays the accent while focused; `field_frame` owns focus):
```rust
            .flex_1()
            .min_w_0()
            .h(theme::rems_px(theme::SIDEBAR_FILTER_HEIGHT))
            .bg(rgb(theme.well))
            .when(!filter_focused, |el| {
                el.border_color(gpui::rgba(theme.layer[1]))
            })
            .flex()
            .items_center()
            .py_0()
```
(replaces `.h(theme::rems_px(theme::INPUT_HEIGHT))` and sits before `.flex()`; the rest of the chain is unchanged.)

`render_compact_sidebar`: delete the two lines
```rust
        // The rail is the session list folded: it keeps the pre-restyle look too.
        let theme = theme.classic();
```
and the `.border_r(px(theme::BORDER_WIDTH))` / `.border_color(rgb(theme.divider))` pair of its outer `div()`.

`session_row` (selected = tint plus bar; name `text_muted` until hovered, `text` while selected or unseen). Replace the head of the chain, from `div()` through the `.pressable_unless(selected, Look::Row, theme)` line, with:

```rust
    let row_id = SharedString::from(format!("session-{session_id}"));
    let frame = div()
        .id(row_id.clone())
        .group(row_id.clone())
        .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .when(progress.is_some(), |el| el.py(px(space::XS)))
        .rounded(px(radius::CHIP))
        // `Look::Row` lifts this label to `text` on hover.
        .text_color(rgb(if selected || unseen {
            theme.text
        } else {
            theme.text_muted
        }));
    // The selected row is picked, not disabled: the pointer, no hover fill.
    with_pick(frame, selected, theme)
        .pressable_unless(selected, Look::Row, theme)
        .when(selected, |el| {
            el.child(record_bounds(Anchor::SessionRow(session_id), cx))
        })
```
(the old `.border(..)`, `.border_color(transparent)` and the `.when(selected, bg+ring)` block go; `with_pick` already calls `relative()`). Then in the name child drop its own colour so the hover lift reaches it:

```rust
                .child(
                    div()
                        .truncate()
                        .text_size(font_size::BODY)
                        .when(selected || unseen, |el| {
```
(the `.text_color(rgb(theme.text))` line is removed; the weight block is unchanged). Update the doc comment of `session_row`: `The selected row is ringed.` becomes `The selected row has the accent tint and bar (`with_pick`).`

`render_run_group` line rows: same pattern. Replace

```rust
            let line = div()
                .id(id.clone())
                .group(id.clone())
                .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(selected, |el| {
                    el.bg(rgb(theme.selected))
                        .border_color(rgb(theme.selected_ring))
                })
                .tooltip(tooltip(
```
with
```rust
            let line = with_pick(
                div()
                    .id(id.clone())
                    .group(id.clone())
                    .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .rounded(px(radius::CHIP))
                    .text_color(rgb(match (running, selected) {
                        (true, true) => theme.text,
                        (true, false) => theme.text_muted,
                        (false, _) => theme.text_subtle,
                    })),
                selected,
                theme,
            )
            .tooltip(tooltip(
```
and drop the explicit colour of the name child (`.text_color(rgb(if running { theme.text } else { theme.text_muted }))` is deleted so it inherits the row colour and the hover lift).

`workspace_row`: the active row is tinted with a bar; the name stays `text` SEMIBOLD (it is the group head, one tier above the muted session rows). Replace

```rust
    div()
        .id(group.clone())
        .group(group.clone())
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .rounded(px(radius::SMALL))
        .when(active, |el| el.bg(rgb(theme.selected)))
        .pressable_unless(active, Look::Row, theme)
        .when(active, |el| {
            el.relative()
                .child(record_bounds(Anchor::WorkspaceRow(project_id), cx))
        })
```
with
```rust
    with_pick(
        div()
            .id(group.clone())
            .group(group.clone())
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .rounded(px(radius::CHIP)),
        active,
        theme,
    )
    .pressable_unless(active, Look::Row, theme)
    .when(active, |el| {
        el.child(record_bounds(Anchor::WorkspaceRow(project_id), cx))
    })
```
(the rest of the chain, from `.tooltip(` on, is unchanged).

`compact_workspace_row`: replace `.rounded(px(radius::SMALL)).when(active, |el| el.bg(rgb(theme.selected))).pressable_unless(active, Look::Row, theme).when(active, |el| { el.relative().child(..) })` with the same `with_pick(<div chain with .rounded(px(radius::CHIP))>, active, theme).pressable_unless(active, Look::Row, theme).when(active, |el| el.child(record_bounds(Anchor::WorkspaceRow(project_id), cx)))` shape, keeping the lines of its existing `.child(record_bounds(..))` call as they are (read the 8 lines first: they were collapsed in the file view, apply the same transformation).

`compact_session_row`: same transformation. `with_pick(div()....rounded(px(radius::CHIP)), selected, theme)` replaces `.when(selected, |el| el.bg(rgb(theme.selected)))`; keep the `unseen && !selected` tint `when` after it; keep `.pressable_unless(selected, Look::Row, theme)` and the `.when(selected, ..)` that adds `record_bounds` (drop its `.relative()`).

`workspace_card`: ring and radius per spec §10 (cards: `card` fill, `radius::CARD`, `divider` ring at rest):
```rust
        .rounded(px(radius::CARD))
        .bg(rgb(theme.card))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
```

`widgets.rs`, `panel_tab` (active: `accent_bg` fill, icon `accent_text`; sizes stay, ADR 0116):

```rust
/// An icon tab of the right panel's tab row (Files, Session, Git), Tern style (ADR 0110): the
/// shown one has the `accent_bg` fill and an `accent_text` icon (ADR 0117), the others a muted
/// icon and the `pressable` hover. The caller adds a count chip, the tooltip and the press.
pub(crate) fn panel_tab(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    active: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::PANEL_TAB_HEIGHT))
        .px(px(space::S))
        .rounded(px(radius::MEDIUM))
        .when(active, |el| el.bg(rgb(theme.accent_bg)))
        .pressable_unless(active, Look::Icon, theme)
        .child(icon_svg(
            glyph,
            theme::ICON_SIZE,
            if active {
                theme.accent_text
            } else {
                theme.text_muted
            },
        ))
}
```

`docs/SPEC.md` §14 Pane block: replace `Two panes meet at a 1 px `divider` line, the same line that sets the sidebar and the right panel apart.` with `Two panes meet at a 1 px `divider` line; the right panel keeps the same line, the sidebar has none (it sits on `pane_gap`, the title bar's ground, so the step to `bg` sets it apart, ADR 0117).` Heights line: after `28 px icon tabs` add `(the shown one: `accent-bg` fill, `accent-text` icon)`. Add to ADR 0117 Consequences: `The sidebar sits on pane_gap without border_r; selected sidebar rows use the tint plus 2 px bar; the right panel's shown icon tab is accent_bg with an accent_text icon.` (`Theme::classic` removal is Part A's bullet.)

- [ ] **Step 4: Check.**
  - `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
  - `cargo test -p mkrew-ui -j 2` (a sidebar or theme test that pinned `classic` was already adapted in Part A; nothing here should fail. A failing insta snapshot here is not intended: do not accept it, report it.)
  - grep the diff: `git diff -U0 crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/widgets.rs | rg "\.hover\(|\.active\(|cursor_pointer|#[0-9A-Fa-f]{6}"` must print nothing; `rg "classic" crates/mkrew-ui/src/shell` must print nothing.

- [ ] **Step 5: Screenshots** (build once: `cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins`). For `dark` then `light`:
  `powershell -File scripts/smoke.ps1 start -Theme dark`; open two fake-agent panes in the `api` workspace (split right, so a focus stripe shows), click one session row so it is selected, focus the filter (`keys -Keys "/"`), open the right panel on Session; then
  `powershell -File scripts/smoke.ps1 shot -Out docs/screenshots/tern-polish-chrome-dark.png`; `powershell -File scripts/smoke.ps1 stop`. Repeat with `-Theme light` and `tern-polish-chrome-light.png`.
  Check by eye: no vertical line between sidebar and the pane area but a visible step; selected session row has the tint and the 2x14 bar, no ring; hover lifts a muted row to full text; filter is 28 px with a faint ring and a `subtle` placeholder, accent ring while typing; the focused pane has a hairline (1 px) stripe, a pane that needs input or has a colour a 2 px one; the shown panel tab is an accent tint with an accent icon.
  If a shot is black use `-Mode screen`. If a state cannot be reached in the sandbox, add `check chrome (sidebar rows, filter, panel tabs, focus stripe) visually, dark and light` under "To verify" in `docs/HUMAN_CHECKS.md` instead.

- [ ] **Step 6: Commit**

```
git add crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/decisions/0117-tern-polish.md docs/screenshots/tern-polish-chrome-dark.png docs/screenshots/tern-polish-chrome-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): sidebar on the page ground with tint-plus-bar rows, quiet filter, accent panel tab"
```


---

## Part G: Git, diff, cards, lists and pane chrome (Tasks 22-25, spec §10)

Spec §10 touches four areas. They map onto four tasks, each one reviewer gate:

| Task | Scope | Needs |
|---|---|---|
| 22 | Four new `Theme` tokens, the 7 % / +8 % diff rule, the diff view (20 px lines, gutter band, hunk header, hunk chips) | Part A (Tern constructors, `surface` values) |
| 23 | Git rows (status chip, selection, group header), section tabs as `segmented()`, commit box | Part A, Part B (`with_pick`, `segment`) |
| 24 | 24a: cards (session panel, board, Jira), usage rows, plan check boxes. 24b: Jira rows and sections, History, Docs | Part A, Part B (`with_pick`), Task 23 (`count_chip`) |
| 25 | Terminal find bar, pane badges | Part A (`layer`, `glass`, `glass_hi`, `shadow_pop`) |

### Findings that shape the plan

- **Diff ground is `surface`.** `render_git_diff` paints `theme.surface` (`git_diff.rs:26`) and the Undo / Discard dialogs paint the same. So all diff fills are computed over `surface`, not `bg`. The spec writes `mix(bg, accent, 0.045)` for `hunk_bg`; over `bg` the hunk bar would not match the ground the rows sit on, so the plan reads "bg" as "the diff ground". Flag this to the reviewer.
- **Number alpha deviates from the spec in light themes.** Spec: numbers are "hue at 60 % over the gutter". Over a pale light gutter that reads 2.1:1 to 2.4:1 (computed). Tern's own light value is 75 %, which reads 2.7:1 to 3.2:1. The plan uses 60 % in dark themes and 75 % in light ones (`DIFF_NUM_MIX_LIGHT`). One constant to revert if the coordinator wants the literal 60 %.
- **Modified fills use the amber `status_needs_input` hue** (dark `0xE4A452`, Tern dark `0xF5A524`, light `0x8B570D`). Existing `diff_mod_*` tokens are unused by `shell/` rendering; they only feed `problems()`. They are retuned with the rest so the `diff lines` contrast rules keep meaning.
- **`diff_hunk_header` stays.** `widgets::diff_row` (Undo preview) still paints it; Part A sets it to `well`. The Git diff stops using `well` for hunk rows and uses the new `hunk_bg`.
- **No Inbox row list exists.** Inbox is the sidebar footer entry (`shell/sidebar.rs:912`, Part F) and the peek popover (`shell/peek.rs`, Part B). Task 24 has nothing to change for "inbox rows"; this is stated in the task so the reviewer does not look for it.
- **`terminal/view.rs` is in the lib crate, `shell/widgets.rs` in the bin crate.** The find bar cannot call Part B's `top_highlight`; Task 25 draws the 1 px `glass_hi` line inline (3 lines).
- **`git_untracked` becomes unused by the Git rows** (the `?` chip uses `success_text` and a `status_done` ring). The token stays: `problems()` and `from_palette` still reference it. Removing it is a separate cleanup.
- **Board columns stay on `surface`; cards move from `surface_raised` to `card`.** After Part A `card == surface` in every built-in theme, so the card is told apart by its 1 px `divider` ring only (as in Tern's `.sf-card`). Task 24a's screenshot step checks that the ring is visible in both themes.
- Coordination: Task 22 and Task 23 add to `theme.rs`, which Part A and Part F also edit (use text anchors, not line numbers). Task 25 edits `widgets::badge_frame` (Part B also edits `widgets.rs`, different functions) and `pane_render.rs::pane_badges` (Part F edits `pane_frame` in the same file). None of Part G touches `push_runs`, `tsp_image`, `tsp_attachment`, `thumbnail`, `space_render.rs` or `window_mode.rs` from `fix/composer-polish`.

### Literal token values (computed with `mix`, same rounding as `theme::mix`)

Rule: ground = `surface`; row = `mix(ground, hue, 0.07)`; gutter = `mix(row, hue, 0.08)`; number = `mix(gutter, hue, 0.60)` dark / `0.75` light; word = `mix(ground, hue, 0.26)` add / `0.25` del. Hues: add = `status_done`, del = `status_exited`, mod = `status_needs_input`. `hunk_bg = mix(surface, accent, 0.045)`, `hunk_current_bg = mix(surface, accent, 0.09)`.

**Mkrew Dark** (surface `0x161619`; hues `57C78A` / `EF7B6C` / `E4A452`; accent `8E8CF5`)

| Token | Value |
|---|---|
| `diff_add_bg` / `diff_add_gutter` / `diff_add_num` / `diff_add_word_bg` | `0x1B2221` / `0x202F29` / `0x418A63` / `0x274436` |
| `diff_del_bg` / `diff_del_gutter` / `diff_del_num` / `diff_del_word_bg` | `0x251D1F` / `0x352525` / `0xA55950` / `0x4C2F2E` |
| `diff_mod_bg` / `diff_mod_gutter` | `0x24201D` / `0x332B21` |
| `hunk_bg` / `hunk_current_bg` | `0x1B1B23` / `0x21212D` |

**Mkrew Light** (surface `0xFFFFFF`; hues `23854F` / `C2412F` / `8B570D`; accent `5452D0`)

| Token | Value |
|---|---|
| add bg / gutter / num / word | `0xF0F6F3` / `0xE0EDE6` / `0x529F75` / `0xC6DFD1` |
| del bg / gutter / num / word | `0xFBF2F0` / `0xF6E4E1` / `0xCF6A5C` / `0xF0D0CB` |
| mod bg / gutter | `0xF7F3EE` / `0xEEE7DC` |
| `hunk_bg` / `hunk_current_bg` | `0xF7F7FD` / `0xF0EFFB` |

**Tern Dark** (surface `0x161619`; hues `3ECF8E` / `FF6166` / `F5A524`; accent `5A8CFF`)

| Token | Value |
|---|---|
| add bg / gutter / num / word | `0x192321` / `0x1C312A` / `0x309066` / `0x204637` |
| del bg / gutter / num / word | `0x261B1E` / `0x372124` / `0xAF474C` / `0x50292C` |
| mod bg / gutter | `0x26201A` / `0x372B1B` |
| `hunk_bg` / `hunk_current_bg` | `0x191B23` / `0x1C212E` |

**Tern Light** (surface `0xFFFFFF`; hues `167A4D` / `BF3038` / `8B570D`; accent `2F63F0`)

| Token | Value |
|---|---|
| add bg / gutter / num / word | `0xEFF6F3` / `0xDEECE6` / `0x489773` / `0xC2DCD1` |
| del bg / gutter / num / word | `0xFBF1F1` / `0xF6E2E2` / `0xCD5D63` / `0xEFCBCD` |
| mod bg / gutter | `0xF7F3EE` / `0xEEE7DC` |
| `hunk_bg` / `hunk_current_bg` | `0xF6F8FE` / `0xECF1FE` |

Contrast of the existing rules with these fills (computed, all pass 4.5:1): `text_dim` on every gutter 8.2 to 9.4; `text_muted` on every row 6.2 to 6.6.

---

### Task 22: Diff tokens, 7 % rule, and the diff view

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (consts near `DIFF_FONT_SIZE`; `Theme` struct; new `DiffTone` + `diff_tone`; `shared()`, `dark()`, `light()`, `tern_dark()`, `tern_light()`; `from_palette`; `problems`; `mod tests`)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`, `..._mkrew_light.snap` and the two Tern snapshots Part A added (re-accepted)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs` (`gutter`, `line_colors` + new `LineColors`, `unified_line`, `split_half`, `span_row`, `hunk_stage_button`, `hunk_discard_button`, new `hunk_chip`)
- Modify: `docs/SPEC.md` (§14 token table rows `diff-*`, §12B "Extra tokens" bullet)
- Test: `crates/mkrew-ui/src/theme.rs` (`mod tests`)

**Interfaces:**
- Consumes: Part A's `Theme::tern_dark()`, `Theme::tern_light()`, `Theme.layer`, the retuned `surface` (`0x161619` dark, `0xFFFFFF` light), `ThemeEntry::build`, `all_themes()` (existing test helper).
- Produces: `Theme.diff_add_num: u32`, `Theme.diff_del_num: u32`, `Theme.hunk_bg: u32`, `Theme.hunk_current_bg: u32`; `theme::DIFF_FONT_SIZE = 12.0`, `DIFF_LINE_HEIGHT = 20.0`, `GIT_HUNK_HEADER_HEIGHT = 24.0`, `GIT_HUNK_BUTTON_HEIGHT = 18.0`, `HUNK_BAR_WIDTH = 3.0`, `HUNK_RING_ALPHA = 0.18`; private `diff_tone(ground, hue, word_mix, dark) -> DiffTone`.

- [ ] **Step 1: Write the failing tests.** In `theme.rs` `mod tests`, next to `each_contrast_rule_fires_when_its_token_is_broken`:

```rust
    /// Channel-wise distance between two RGB colors (sum of |a - b|).
    fn distance(a: u32, b: u32) -> u32 {
        [16, 8, 0]
            .into_iter()
            .map(|s| (((a >> s) & 0xFF) as i32 - ((b >> s) & 0xFF) as i32).unsigned_abs())
            .sum()
    }

    #[test]
    fn the_hand_written_diff_fills_follow_the_seven_percent_rule() {
        // One step per channel of slack: the literals are rounded once, the rule rounds per mix.
        let close = |a: u32, b: u32| distance(a, b) <= 3;
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::tern_dark(),
            Theme::tern_light(),
        ] {
            let name = theme.name.clone();
            let ground = theme.surface;
            let add = diff_tone(ground, theme.status_done, DIFF_ADD_WORD_MIX, theme.dark);
            let del = diff_tone(ground, theme.status_exited, DIFF_DEL_WORD_MIX, theme.dark);
            let changed = diff_tone(
                ground,
                theme.status_needs_input,
                DIFF_ADD_WORD_MIX,
                theme.dark,
            );
            for (what, got, want) in [
                ("add row", theme.diff_add_bg, add.row),
                ("add gutter", theme.diff_add_gutter, add.gutter),
                ("add number", theme.diff_add_num, add.num),
                ("add word", theme.diff_add_word_bg, add.word),
                ("del row", theme.diff_del_bg, del.row),
                ("del gutter", theme.diff_del_gutter, del.gutter),
                ("del number", theme.diff_del_num, del.num),
                ("del word", theme.diff_del_word_bg, del.word),
                ("mod row", theme.diff_mod_bg, changed.row),
                ("mod gutter", theme.diff_mod_gutter, changed.gutter),
                ("hunk", theme.hunk_bg, mix(ground, theme.accent, HUNK_MIX)),
                (
                    "current hunk",
                    theme.hunk_current_bg,
                    mix(ground, theme.accent, HUNK_CURRENT_MIX),
                ),
            ] {
                assert!(
                    close(got, want),
                    "{name} {what}: {got:06X}, the rule gives {want:06X}"
                );
            }
        }
    }

    #[test]
    fn diff_tiers_step_away_from_the_ground_in_every_theme() {
        for theme in all_themes() {
            let name = &theme.name;
            let ground = theme.surface;
            for (what, row, gutter) in [
                ("add", theme.diff_add_bg, theme.diff_add_gutter),
                ("del", theme.diff_del_bg, theme.diff_del_gutter),
                ("mod", theme.diff_mod_bg, theme.diff_mod_gutter),
            ] {
                assert!(
                    distance(ground, row) < distance(ground, gutter),
                    "{name}: the {what} gutter is not a step stronger than its row"
                );
            }
            assert!(
                distance(ground, theme.hunk_bg) < distance(ground, theme.hunk_current_bg),
                "{name}: the current hunk is not stronger than the others"
            );
        }
    }
```
In the `broken` table of `each_contrast_rule_fires_when_its_token_is_broken`, right after the `"diff lines"` tuple add (anchor: the tuple `"diff lines", Theme { git_modified: t.bg, ..t.clone() }`):

```rust
            (
                "hunk header",
                Theme {
                    accent_text: t.hunk_current_bg,
                    ..t.clone()
                },
            ),
```
- [ ] **Step 2: Run, expect FAIL** (does not compile: `diff_tone`, `hunk_bg`, `diff_add_num` missing)
`cargo test -p mkrew-ui theme -j 2`

- [ ] **Step 3: Implement.**

3a. `theme.rs`, replace the diff size constants (anchor `pub const DIFF_FONT_SIZE`) and the hunk header height:

```rust
/// Git diff lines: monospace size and line height, px (a wrapped line is a multiple of it).
pub const DIFF_FONT_SIZE: f32 = 12.0;
pub const DIFF_LINE_HEIGHT: f32 = 20.0;
```
```rust
/// A diff hunk header bar's height (one grid step above a line), the height of the Stage /
/// Discard chips on it, the bar of the current hunk, and the alpha of the Stage chip's ring.
pub const GIT_HUNK_HEADER_HEIGHT: f32 = 24.0;
pub const GIT_HUNK_BUTTON_HEIGHT: f32 = 18.0;
pub const HUNK_BAR_WIDTH: f32 = 3.0;
pub const HUNK_RING_ALPHA: f32 = 0.18;
```
and, after `GIT_HUNK_HEADER_HEIGHT`, the diff rule (private; used by the constructors and the test):

```rust
/// The diff rule (spec §10): a changed row is `DIFF_ROW_MIX` of its hue over the diff ground
/// (`surface`); its gutter is a step on (`DIFF_GUTTER_MIX` more of the hue); the line numbers are
/// the hue at `DIFF_NUM_MIX_*` over the gutter (60 % over a pale light gutter reads 2:1, so light
/// themes use 75 %, Tern's own light value); the changed word is `DIFF_*_WORD_MIX` over the ground.
const DIFF_ROW_MIX: f32 = 0.07;
const DIFF_GUTTER_MIX: f32 = 0.08;
const DIFF_NUM_MIX_DARK: f32 = 0.60;
const DIFF_NUM_MIX_LIGHT: f32 = 0.75;
const DIFF_ADD_WORD_MIX: f32 = 0.26;
const DIFF_DEL_WORD_MIX: f32 = 0.25;
/// A hunk header bar and the current one: the accent over the diff ground.
const HUNK_MIX: f32 = 0.045;
const HUNK_CURRENT_MIX: f32 = 0.09;
```
Add after `pub fn mix` (anchor `pub fn mix(a: u32, b: u32, t: f32) -> u32`):

```rust
/// The fills of one diff hue (see [`DIFF_ROW_MIX`]).
struct DiffTone {
    row: u32,
    gutter: u32,
    num: u32,
    word: u32,
}

fn diff_tone(ground: u32, hue: u32, word_mix: f32, dark: bool) -> DiffTone {
    let row = mix(ground, hue, DIFF_ROW_MIX);
    let gutter = mix(row, hue, DIFF_GUTTER_MIX);
    let number = if dark {
        DIFF_NUM_MIX_DARK
    } else {
        DIFF_NUM_MIX_LIGHT
    };
    DiffTone {
        row,
        gutter,
        num: mix(gutter, hue, number),
        word: mix(ground, hue, word_mix),
    }
}
```
3b. `Theme` struct: after `pub diff_mod_gutter: u32,` add

```rust
    /// Line numbers of added and removed lines: the hue at 60 % (75 % light) over the gutter
    /// (the sign keeps the full `success_text` / `danger_text`).
    pub diff_add_num: u32,
    pub diff_del_num: u32,
    /// A hunk header bar, and the current hunk's: the accent at 4.5 % / 9 % over `surface`.
    pub hunk_bg: u32,
    pub hunk_current_bg: u32,
```
Fix the doc comment of `diff_add_gutter` ("a step stronger than the row's own fill" stays true). In `shared()` (anchor `diff_mod_gutter: 0,`) add `diff_add_num: 0, diff_del_num: 0, hunk_bg: 0, hunk_current_bg: 0,`.

3c. Literals. In `dark()` replace the comment and these fields (anchors: the `// Removed / added lines: danger / success at 9 % ...` comment through `diff_mod_gutter: 0x322B23,`, and `diff_del_word_bg` / `diff_add_word_bg`):

```rust
            // Spec §10, over `surface` (0x161619): hue at 7 % (row), +8 % (gutter), 60 % over the
            // gutter (number), 26 % / 25 % (word); the hunk bars are the accent at 4.5 % / 9 %.
            diff_add_bg: 0x1B2221,
            diff_add_text: 0x6FD39B,
            diff_del_bg: 0x251D1F,
            diff_del_text: 0xEF8F82,
            diff_mod_bg: 0x24201D,
            diff_mod_text: 0xE4A452,
            diff_hunk_header: 0x17191D, // keep Part A's value here
            diff_add_gutter: 0x202F29,
            diff_del_gutter: 0x352525,
            diff_mod_gutter: 0x332B21,
            diff_add_num: 0x418A63,
            diff_del_num: 0xA55950,
            hunk_bg: 0x1B1B23,
            hunk_current_bg: 0x21212D,
```
(the `diff_hunk_header` line is whatever Part A left; do not change it) and `diff_del_word_bg: 0x4C2F2E, diff_add_word_bg: 0x274436,`.
`light()`:
`diff_add_bg: 0xF0F6F3, diff_del_bg: 0xFBF2F0, diff_mod_bg: 0xF7F3EE, diff_add_gutter: 0xE0EDE6, diff_del_gutter: 0xF6E4E1, diff_mod_gutter: 0xEEE7DC, diff_add_num: 0x529F75, diff_del_num: 0xCF6A5C, hunk_bg: 0xF7F7FD, hunk_current_bg: 0xF0EFFB, diff_del_word_bg: 0xF0D0CB, diff_add_word_bg: 0xC6DFD1,` (the text colors and `diff_hunk_header` stay).
`tern_dark()` and `tern_light()` (Part A's override literals; add the block next to the `terminal_selection_bg` override, before `..Self::dark()` / `..Self::light()`), because Tern's hues differ from Mkrew's:
```rust
            // Tern Dark
            diff_add_bg: 0x192321,
            diff_add_gutter: 0x1C312A,
            diff_add_num: 0x309066,
            diff_add_word_bg: 0x204637,
            diff_del_bg: 0x261B1E,
            diff_del_gutter: 0x372124,
            diff_del_num: 0xAF474C,
            diff_del_word_bg: 0x50292C,
            diff_mod_bg: 0x26201A,
            diff_mod_gutter: 0x372B1B,
            hunk_bg: 0x191B23,
            hunk_current_bg: 0x1C212E,
```
```rust
            // Tern Light
            diff_add_bg: 0xEFF6F3,
            diff_add_gutter: 0xDEECE6,
            diff_add_num: 0x489773,
            diff_add_word_bg: 0xC2DCD1,
            diff_del_bg: 0xFBF1F1,
            diff_del_gutter: 0xF6E2E2,
            diff_del_num: 0xCD5D63,
            diff_del_word_bg: 0xEFCBCD,
            diff_mod_bg: 0xF7F3EE,
            diff_mod_gutter: 0xEEE7DC,
            hunk_bg: 0xF6F8FE,
            hunk_current_bg: 0xECF1FE,
```
3d. `from_palette`: replace the `diff` closure block and the diff entries of `grounds` and of the struct literal:

```rust
        let add = diff_tone(p.surface, p.green, DIFF_ADD_WORD_MIX, p.dark);
        let del = diff_tone(p.surface, p.red, DIFF_DEL_WORD_MIX, p.dark);
        let changed = diff_tone(p.surface, p.yellow, DIFF_ADD_WORD_MIX, p.dark);
        let hunk_bg = mix(p.surface, p.accent, HUNK_MIX);
        let hunk_current_bg = mix(p.surface, p.accent, HUNK_CURRENT_MIX);
        let grounds = [
            p.bg,
            p.chrome,
            p.surface,
            surface_raised,
            hover,
            add.row,
            del.row,
            changed.row,
            add.gutter,
            del.gutter,
            changed.gutter,
            add.word,
            del.word,
            hunk_bg,
            hunk_current_bg,
        ];
```
and in the `Self { .. }` literal:
```rust
            diff_add_bg: add.row,
            diff_del_bg: del.row,
            diff_mod_bg: changed.row,
            diff_add_gutter: add.gutter,
            diff_del_gutter: del.gutter,
            diff_mod_gutter: changed.gutter,
            diff_add_num: add.num,
            diff_del_num: del.num,
            hunk_bg,
            hunk_current_bg,
            diff_add_word_bg: add.word,
            diff_del_word_bg: del.word,
```
`text(..)` readable now also clears both hunk fills, so `accent_text` and the derived text colors stay 4.5:1 on them.

3e. `problems()`: after the `for (fg, bg) in [ ... ] { check("diff lines", fg, bg, 4.5); }` block add:

```rust
    // A hunk header: `@@` in `accent_text`, its section in `text_dim`, on the plain and on the
    // current hunk's fill.
    for bg in [t.hunk_bg, t.hunk_current_bg] {
        for fg in [t.accent_text, t.text_dim] {
            check("hunk header", fg, bg, 4.5);
        }
    }
```
3f. `git_diff.rs`. Hatch and line colors:

```rust
/// A line's colors: the row fill and the gutter band (none on context), the line-number color,
/// the sign and its color. Removed lines are red and added lines green (a paired change too: its
/// changed token has its own fill); the numbers are the dimmer `diff_*_num`, the sign the full
/// hue; context numbers are `subtle`.
struct LineColors {
    row: Option<u32>,
    gutter: Option<u32>,
    number: u32,
    sign_color: u32,
    sign: &'static str,
}

fn line_colors(kind: DiffLineKind, theme: &Theme) -> LineColors {
    match kind {
        DiffLineKind::Add => LineColors {
            row: Some(theme.diff_add_bg),
            gutter: Some(theme.diff_add_gutter),
            number: theme.diff_add_num,
            sign_color: theme.success_text,
            sign: "+",
        },
        DiffLineKind::Del => LineColors {
            row: Some(theme.diff_del_bg),
            gutter: Some(theme.diff_del_gutter),
            number: theme.diff_del_num,
            sign_color: theme.danger_text,
            sign: "−",
        },
        DiffLineKind::Context => LineColors {
            row: None,
            gutter: None,
            number: theme.subtle,
            sign_color: theme.text_subtle,
            sign: " ",
        },
    }
}
```
```rust
/// The gutter of a line (numbers and sign): its own band in `fill` (the line's gutter token,
/// none on context), the accent fill while picked.
fn gutter(fill: Option<u32>, picked: bool, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .when_some(fill, |el, fill| el.bg(rgb(fill)))
        .when(picked, |el| el.bg(rgb(theme.accent_bg)))
}
```
`unified_line` body:
```rust
    let look = line_colors(line.kind, theme);
    let is_picked = is_picked(line.new_line, picked);
    let number = if is_picked { theme.accent_text } else { look.number };
    let width = gutter_width(shown.number_width);
    diff_frame(wrap, theme)
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(if is_picked {
            rgb(theme.accent).into()
        } else {
            gpui::transparent_black()
        })
        .when_some(look.row, |el, bg| el.bg(rgb(bg)))
        .child(
            gutter(look.gutter, is_picked, theme)
                .child(line_number(line.old_line, number, width))
                .child(line_number(line.new_line, number, width))
                .child(sign_cell(look.sign, look.sign_color)),
        )
        .child(diff_text(shown, index, line, theme))
```
`split_half`: hatch `rgb(theme.divider)`; and
```rust
    let look = line_colors(line.kind, theme);
    ...
    let number_color = if is_picked { theme.accent_text } else { look.number };
    half.when_some(look.row, |el, bg| el.bg(rgb(bg)))
        .when(is_picked, |el| el.border_color(rgb(theme.accent)))
        .child(
            gutter(look.gutter, is_picked, theme)
                .child(line_number(number, number_color, gutter_width(shown.number_width)))
                .child(sign_cell(look.sign, look.sign_color)),
        )
        .child(diff_text(shown, at, line, theme))
```
`span_row`: replace the doc comment's "a 34 px `well` bar ..." with "a 24 px bar on `hunk_bg` (`hunk_current_bg` and a 3 px accent bar on the current hunk) with `divider` lines above and below: `@@ ... @@` in `accent_text`, its section in `text_dim`, a Stage hunk chip on the right and a Discard hunk chip after it on unstaged modified files". Replace the `Hunk` arm:

```rust
        Some(git_view::DiffRow::Hunk(at)) => {
            let Some(hunk) = shown.diff.hunks.get(*at) else {
                return frame;
            };
            let is_current = current == Some(*at);
            frame
                .relative()
                .h(px(theme::GIT_HUNK_HEADER_HEIGHT))
                .border_t(px(theme::BORDER_WIDTH))
                .border_b(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.divider))
                .bg(rgb(if is_current {
                    theme.hunk_current_bg
                } else {
                    theme.hunk_bg
                }))
                // The current hunk: a bar down its left edge (an overlay, so nothing shifts).
                .when(is_current, |el| {
                    el.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px(theme::HUNK_BAR_WIDTH))
                            .bg(rgb(theme.accent)),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.accent_text))
                        .child(format!(
                            "@@ -{},{} +{},{} @@",
                            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines,
                        )),
                )
                // Shrinks, so the hunk action after it stays on screen.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text_dim))
                        .child(hunk.header.clone()),
                )
                // ...the two `.children(...)` of the Stage and Discard buttons stay as they are
```
The `Gap` arm keeps `well`. Remove the now unused `FontWeight` / `weight` use only if clippy says so (the file still uses `FontWeight` in `diff_text`? it does not; `weight::SEMIBOLD` is used by nothing else here: let the compiler decide and drop the import it flags).

Chips:
```rust
/// A hunk header's action chip: `GIT_HUNK_HEADER_HEIGHT` minus 6 px tall, `radius::TAG`, a 1 px
/// ring. `accent` (Stage) is the accent tint with an 18 % accent ring; the other (Discard) is
/// neutral: `layer[0]` fill, `layer[1]` ring.
fn hunk_chip(
    chip: gpui::Stateful<gpui::Div>,
    accent: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    chip.h(px(theme::GIT_HUNK_BUTTON_HEIGHT))
        .flex()
        .items_center()
        .px(px(space::S))
        .rounded(px(radius::TAG))
        .border(px(theme::BORDER_WIDTH))
        .when(accent, |el| {
            el.bg(rgb(theme.accent_bg))
                .border_color(gpui::rgba(theme::with_alpha(
                    theme.accent,
                    theme::HUNK_RING_ALPHA,
                )))
                .text_color(rgb(theme.accent_text))
        })
        .when(!accent, |el| {
            el.bg(gpui::rgba(theme.layer[0]))
                .border_color(gpui::rgba(theme.layer[1]))
        })
}
```
In `hunk_stage_button` replace `.h(px(space::XL)).flex().items_center().px(px(space::S)).rounded(px(radius::CHIP))` by wrapping: `hunk_chip(row_action(...), true, theme).ml_auto()...`. In `hunk_discard_button`: `hunk_chip(row_action(...), false, theme).text_color(rgb(theme.danger_text))...`. Delete the "A small ghost button, a little taller than a list row's action." comment.

3g. `docs/SPEC.md`: in the §14 token table replace the four rows `diff-add-bg / diff-del-bg`, `diff-add-word-bg / diff-del-word-bg`, keep `diff-add-text / diff-del-text`, and `diff-hunk-header`, and add:

```
| `diff-add-bg` / `diff-del-bg` | #1B2221 / #251D1F | #F0F6F3 / #FBF2F0 |
| `diff-add-gutter` / `diff-del-gutter` | #202F29 / #352525 | #E0EDE6 / #F6E4E1 |
| `diff-add-num` / `diff-del-num` | #418A63 / #A55950 | #529F75 / #CF6A5C |
| `diff-add-word-bg` / `diff-del-word-bg` | #274436 / #4C2F2E | #C6DFD1 / #F0D0CB |
| `hunk-bg` / `hunk-current-bg` | #1B1B23 / #21212D | #F7F7FD / #F0EFFB |
```
and after the table a sentence: "Diff fills are the hue (`status-done`, `status-exited`, `status-needs-input`) at 7 % over `surface`; the gutter adds 8 %; line numbers are the hue at 60 % (75 % in light themes) over the gutter; changed words 26 % / 25 % over `surface`; hunk bars are the accent at 4.5 % / 9 %. Tern Dark: add row #192321, del row #261B1E, hunk #191B23; Tern Light: add row #EFF6F3, del row #FBF1F1, hunk #F6F8FE (full values in `theme.rs`)." In §12B "Extra tokens" bullet append "`diff-*-gutter`, `diff-add-num`, `diff-del-num`, `hunk-bg`, `hunk-current-bg`". In the SPEC line about Git diff text add "12 px on 20 px lines; hunk header 24 px" next to `Diff viewer:` bullet (§12B).

- [ ] **Step 4: Run, expect PASS** (except the theme snapshots, which pin the old values)
`cargo test -p mkrew-ui theme -j 2`. Expect failures only in the snapshot tests (`the_mkrew_themes_never_change` and Part A's Tern snapshot test). Run `cargo insta accept`, then `git diff -- crates/mkrew-ui/src/snapshots`: the only changed lines must be the `diff_*` fills, `diff_*_gutter`, `diff_*_word_bg` and the four added fields. Re-run `cargo test -p mkrew-ui theme -j 2`: green. Then `cargo test -p mkrew-ui -j 2` for the other suites.

- [ ] **Step 5: Styling check and screenshots.** `cargo fmt -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, `cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins`. The sandbox workspace `api` needs a repository with one modified file: `powershell -File scripts/smoke.ps1 mk project list` prints its folder; in it `git init`, commit a file with ~40 lines, then edit 3 spread lines and add one line. Then:
`powershell -File scripts/smoke.ps1 start -Theme dark`, `... keys -Keys "^+4"` (Git panel), select the changed file (`... click -FromX .85 -FromY .30`; take a first `shot` to adjust), `... shot -Out docs/screenshots/tern-polish-git-diff-dark.png`; switch the diff toolbar to Split and `shot -Out docs/screenshots/tern-polish-git-diff-split-dark.png`; `... stop`. Repeat with `-Theme light`. Check by eye: 20 px lines, gutter band one step stronger than its row, dim numbers with full-hue signs, hunk header 24 px with `@@` in accent, the current hunk (press `n`) shows the 3 px bar and the stronger fill, chips 18 px.

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/snapshots docs/SPEC.md docs/screenshots/tern-polish-git-diff-dark.png docs/screenshots/tern-polish-git-diff-split-dark.png docs/screenshots/tern-polish-git-diff-light.png docs/screenshots/tern-polish-git-diff-split-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): diff on the 7 % rule with a gutter band, hunk tokens and 20 px lines"`

---

### Task 23: Git rows, section segmented control, commit box

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs` (`ChangeChip`, `change_chip`, `use crate::theme::{self, Theme}`)
- Test: `crates/mkrew-ui/src/git_view/tests.rs`
- Modify: `crates/mkrew-ui/src/git_actions.rs` (`summary_counter`; `mod tests`)
- Modify: `crates/mkrew-ui/src/field.rs` (`summary_highlights`, `summary_text`, `field_text`; new `mod tests`)
- Modify: `crates/mkrew-ui/src/main.rs` (the `use mkrew_ui::field::{...}` line: add `summary_text`)
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs` (`change_letter` removed, `status_chip`, `count_chip`, `row_frame`, `git_file_row`, `group_header`, `git_commit_row`)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs` (toolbar badge, lines ~257-274)
- Modify: `crates/mkrew-ui/src/shell/git_render.rs` (`git_tabs`, `git_commit_box`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`GIT_BADGE_SIZE`, remove `GIT_SECTION_TAB_HEIGHT`, add `STATUS_RING_ALPHA`)
- Modify: `docs/SPEC.md` (§12B tab line, §14 heights line)

**Interfaces:**
- Consumes: Part B `widgets::with_pick<E: ParentElement + Styled>(el: E, picked: bool, theme: &Theme) -> E`; `widgets::segmented(theme)` and `segment(id, active, theme) -> Stateful<Div>` (Part B restyles them but keeps the signatures; if Part B changes a signature, follow it); `Pressable::on_press(enabled, handler)`; Task 22's tokens are not used here.
- Produces: `git_view::ChangeChip { letter: &'static str, color: u32, fill: Option<u32>, ring: Option<u32>, meaning: &'static str }`, `git_view::change_chip(ChangeKind, &Theme) -> ChangeChip`; `git_rows::status_chip(id: impl Into<gpui::ElementId>, chip: ChangeChip, theme) -> Stateful<Div>`; `git_rows::count_chip(count: usize, theme: &Theme) -> gpui::Div` (Task 24b reuses it); `git_actions::summary_counter(len: usize) -> Option<(String, bool)>`; `field::summary_highlights`, `field::summary_text`.

- [ ] **Step 1: Write the failing tests.**

`git_view/tests.rs` (end of file):
```rust
#[test]
fn status_chips_are_tinted_by_kind_and_only_an_untracked_file_has_a_ring() {
    let theme = Theme::dark();
    let kinds = [
        ChangeKind::Added,
        ChangeKind::Modified,
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::Copied,
        ChangeKind::TypeChanged,
        ChangeKind::Untracked,
        ChangeKind::Conflicted,
    ];
    let letters: HashSet<_> = kinds.iter().map(|k| change_chip(*k, &theme).letter).collect();
    assert_eq!(letters.len(), kinds.len(), "every kind has its own letter");
    for kind in kinds {
        let chip = change_chip(kind, &theme);
        if kind == ChangeKind::Untracked {
            assert_eq!(chip.fill, None, "an untracked chip is an outline");
            assert_eq!(
                chip.ring,
                Some(theme::with_alpha(theme.status_done, theme::STATUS_RING_ALPHA))
            );
        } else {
            assert!(chip.fill.is_some() && chip.ring.is_none(), "{kind:?}");
        }
    }
    for kind in [ChangeKind::Renamed, ChangeKind::Copied] {
        assert_eq!(change_chip(kind, &theme).fill, Some(theme.accent));
    }
}
```
`git_actions.rs` `mod tests`:
```rust
    #[test]
    fn the_counter_hides_at_zero_and_warns_past_the_limit() {
        assert_eq!(summary_counter(0), None);
        assert_eq!(summary_counter(1), Some(("1 / 72".to_owned(), false)));
        assert_eq!(summary_counter(SUMMARY_LIMIT), Some(("72 / 72".to_owned(), false)));
        assert_eq!(summary_counter(SUMMARY_LIMIT + 1), Some(("73 / 72".to_owned(), true)));
    }
```
`field.rs` (new module at the end of the file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(text: &str, selection: Option<Range<usize>>) -> Vec<(Range<usize>, bool, bool)> {
        let lit = Hsla::from(gpui::rgb(0x112233));
        summary_highlights(text, selection, lit)
            .into_iter()
            .map(|(range, style)| {
                (range, style.font_weight.is_some(), style.background_color.is_some())
            })
            .collect()
    }

    #[test]
    fn the_first_line_is_semibold_and_the_body_plain() {
        assert_eq!(ranges("feat: x\n\nbody", None), vec![(0..7, true, false)]);
        assert_eq!(ranges("abc", None), vec![(0..3, true, false)]);
        assert!(ranges("", None).is_empty());
    }

    #[test]
    fn a_selection_cuts_the_summary_instead_of_overlapping_it() {
        assert_eq!(
            ranges("feat: x\n\nbody", Some(3..10)),
            vec![(0..3, true, false), (3..7, true, true), (7..10, false, true)]
        );
        assert_eq!(
            ranges("abc", Some(1..2)),
            vec![(0..1, true, false), (1..2, true, true), (2..3, true, false)]
        );
    }
}
```
- [ ] **Step 2: Run, expect FAIL** (missing items): `cargo test -p mkrew-ui status_chips -j 2`, `cargo test -p mkrew-ui the_counter -j 2`, `cargo test -p mkrew-ui summary -j 2` (field tests: `cargo test -p mkrew-ui field -j 2`)

- [ ] **Step 3: Implement.**

`theme.rs`: replace `GIT_BADGE_SIZE` and its doc, remove `GIT_SECTION_TAB_HEIGHT` (only `git_render.rs` used it), add the ring alpha:
```rust
/// The box of the change letter (M, A, D ...) of a Git file row: a 16 px chip with a tint of
/// the letter's hue (an outline for `?`); it keeps the names in one column.
pub const GIT_BADGE_SIZE: f32 = 16.0;
/// The ring of an untracked file's chip: `status_done` at 35 %.
pub const STATUS_RING_ALPHA: f32 = 0.35;
```
`git_view.rs`: add `use crate::theme::{self, Theme};` with the other `crate::` imports and, after `counts`:
```rust
/// How a changed file's status chip looks (the Git list and the diff toolbar): the letter in
/// `color`, the hue whose tint fills the chip (none for an untracked file) and the RGBA ring an
/// untracked file gets instead. R and C take the accent; `?` is an outline of `status_done`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeChip {
    pub letter: &'static str,
    pub color: u32,
    pub fill: Option<u32>,
    pub ring: Option<u32>,
    pub meaning: &'static str,
}

pub fn change_chip(kind: ChangeKind, theme: &Theme) -> ChangeChip {
    let tinted = |letter, color, meaning| ChangeChip {
        letter,
        color,
        fill: Some(color),
        ring: None,
        meaning,
    };
    let accent = |letter, meaning| ChangeChip {
        letter,
        color: theme.accent_text,
        fill: Some(theme.accent),
        ring: None,
        meaning,
    };
    match kind {
        ChangeKind::Added => tinted("A", theme.diff_add_text, "Added"),
        ChangeKind::Modified => tinted("M", theme.git_modified, "Modified"),
        ChangeKind::Deleted => tinted("D", theme.diff_del_text, "Deleted"),
        ChangeKind::Renamed => accent("R", "Renamed"),
        ChangeKind::Copied => accent("C", "Copied"),
        ChangeKind::TypeChanged => tinted("T", theme.git_modified, "File type changed"),
        ChangeKind::Untracked => ChangeChip {
            letter: "?",
            color: theme.success_text,
            fill: None,
            ring: Some(theme::with_alpha(theme.status_done, theme::STATUS_RING_ALPHA)),
            meaning: "Untracked",
        },
        ChangeKind::Conflicted => tinted("!", theme.diff_del_text, "Conflicted"),
    }
}
```
`git_actions.rs`, after `SUMMARY_LIMIT`:
```rust
/// The length counter under the commit box: `"n / 72"` and whether the summary is past the
/// limit; `None` while the summary is empty (nothing to count yet).
pub fn summary_counter(summary_len: usize) -> Option<(String, bool)> {
    (summary_len > 0).then(|| {
        (
            format!("{summary_len} / {SUMMARY_LIMIT}"),
            summary_len > SUMMARY_LIMIT,
        )
    })
}
```
`field.rs`: replace `caret_text`'s body so both entry points share one function, and add the highlights:
```rust
pub fn caret_text(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    placeholder: impl IntoElement,
    focused: bool,
    theme: &Theme,
) -> (AnyElement, Option<TextLayout>) {
    field_text(text, cursor, selection, false, placeholder, focused, theme)
}

/// [`caret_text`] for a commit message: its first line (the summary) is semibold.
pub fn summary_text(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    placeholder: impl IntoElement,
    focused: bool,
    theme: &Theme,
) -> (AnyElement, Option<TextLayout>) {
    field_text(text, cursor, selection, true, placeholder, focused, theme)
}

fn field_text(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    summary: bool,
    placeholder: impl IntoElement,
    focused: bool,
    theme: &Theme,
) -> (AnyElement, Option<TextLayout>) {
    if text.is_empty() {
        return (empty_field(placeholder, focused, theme), None);
    }
    let lit = selection_bg(theme);
    let selection = selection.filter(|s| !s.is_empty());
    let show_caret = focused && selection.is_none();
    let highlights = if summary {
        summary_highlights(text, selection, lit)
    } else {
        selection
            .map(|s| {
                (
                    s,
                    HighlightStyle {
                        background_color: Some(lit),
                        ..HighlightStyle::default()
                    },
                )
            })
            .into_iter()
            .collect()
    };
    let styled = StyledText::new(text.to_owned()).with_highlights(highlights);
    let (shown, layout) = caret_over(styled, cursor.min(text.len()), show_caret, theme);
    (shown, Some(layout))
}

/// The highlights of a commit message: the first line semibold, the bytes of `selection` on
/// `lit`. Sorted and not overlapping, as `StyledText::with_highlights` needs, so a selection cuts
/// the summary's range instead of overlapping it; the body has none.
pub fn summary_highlights(
    text: &str,
    selection: Option<Range<usize>>,
    lit: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let end = text.len();
    let eol = text.find('\n').unwrap_or(end);
    let selection = selection.filter(|s| !s.is_empty()).unwrap_or(0..0);
    let mut cuts = [0, eol, selection.start.min(end), selection.end.min(end), end];
    cuts.sort_unstable();
    cuts.windows(2)
        .filter(|cut| cut[0] < cut[1])
        .filter_map(|cut| {
            let part = cut[0]..cut[1];
            let bold = part.end <= eol;
            let selected = selection.start <= part.start && part.end <= selection.end;
            (bold || selected).then(|| {
                (
                    part,
                    HighlightStyle {
                        font_weight: bold.then_some(gpui::FontWeight(theme::weight::SEMIBOLD)),
                        background_color: selected.then_some(lit),
                        ..HighlightStyle::default()
                    },
                )
            })
        })
        .collect()
}
```
(Keep `caret_text`'s existing doc comment above it.) `main.rs`: `use mkrew_ui::field::{Caret, caret_mouse, caret_text, copy_selection, summary_text};`.

`git_rows.rs`. Delete `change_letter` (its two callers move to the chip). Add the chip views after `change_counts`:
```rust
/// The status chip of a changed file: `GIT_BADGE_SIZE` square, `radius::SMALL`, the letter in the
/// terminal font (OVERLINE, semibold) on the tint of its hue; an untracked file is an outline.
pub(crate) fn status_chip(
    id: impl Into<gpui::ElementId>,
    chip: git_view::ChangeChip,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::GIT_BADGE_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::OVERLINE)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(chip.color))
        .when_some(chip.fill, |el, hue| el.bg(theme.tint(hue)))
        .when_some(chip.ring, |el, ring| {
            el.border(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(ring))
        })
        .tooltip(tooltip(theme, chip.meaning, ""))
        .child(chip.letter)
}

/// A count after a group title (Git group headers, Jira status sections): digits in the
/// terminal font on a `well` chip.
pub(crate) fn count_chip(count: usize, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .bg(rgb(theme.well))
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_subtle))
        .child(count.to_string())
}
```
`row_frame`: `.rounded(px(radius::CHIP))`. In `git_file_row`: replace `let (letter, color, what) = change_letter(file.kind, theme);` with `let chip = git_view::change_chip(file.kind, theme);`; replace the selected-fill block and the badge div, and drop `MEDIUM` from the name:
```rust
        .text_size(font_size::BODY)
        // The keyboard ring stays; the selection itself is the tint plus bar (spec §4).
        .when(picked && focused, |el| el.border_color(rgb(theme.accent)))
        .map(|el| with_pick(el, picked, theme))
        .pressable_unless(picked, Look::Row, theme)
        .child(
            status_chip(SharedString::from(format!("{id}-kind")), chip, theme)
                // A viewed row steps back (its content, not the ✓ box); the selected one stays readable.
                .when(dim, |el| el.opacity(opacity::VIEWED)),
        )
```
and the name element becomes `.text_color(rgb(theme.text)).child(name)` (no `font_weight`). `git_commit_row`: same selection swap:
```rust
        .text_size(font_size::BODY)
        .when(picked && focused, |el| el.border_color(rgb(theme.accent)))
        .map(|el| with_pick(el, picked, theme))
        .pressable_unless(picked, Look::Row, theme)
```
`group_header`: title and count:
```rust
        .child(
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(group.title()),
        )
        .child(count_chip(count, theme))
```
Fix the doc comments of `git_file_row` ("the `hover` fill and an accent ring ..." becomes "the accent tint and bar (`with_pick`) and an accent ring while the list has the keyboard") and of `group_header`.

`git_diff.rs` toolbar: replace the `badge` block with
```rust
        // The file's status chip, like its row in the list.
        let badge = position.map(|(_, _, kind)| {
            status_chip("git-diff-status", git_view::change_chip(kind, theme), theme)
        });
```
`git_render.rs` `git_tabs`: doc "Working N / Vs. base / History: a compact segmented control (N = changed files; spec §10)". Replace the closure and the container:
```rust
        let tab =
            |section: git_view::Section, id: &'static str, label: String, tip: &'static str| {
                let active = shown == section;
                let keys = if section == git_view::Section::History {
                    history_keys.clone()
                } else {
                    toggle_keys.clone()
                };
                segment(id, active, theme)
                    .flex_1()
                    .min_w_0()
                    .px(px(space::S))
                    .text_size(font_size::META)
                    .tooltip(tooltip(theme, tip, keys))
                    .on_press(
                        !active,
                        cx.listener(move |shell, _, window, cx| {
                            shell.show_git_section(project_id, section, window, cx);
                        }),
                    )
                    // Cut with an ellipsis at the narrowest panel (240 px) instead of overflowing.
                    .child(div().min_w_0().truncate().child(label))
            };
        div()
            .flex_none()
            .px(px(space::M))
            .py(px(space::S))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(
                segmented(theme)
                    .w_full()
                    .child(tab(git_view::Section::Working, "git-mode-working", working,
                        "Uncommitted changes: staged, unstaged, untracked"))
                    .child(tab(git_view::Section::Base, "git-mode-base", "Vs. base".to_owned(),
                        "Everything on this branch since it left the base"))
                    .child(tab(git_view::Section::History, "git-mode-history", "History".to_owned(),
                        "The last 50 commits and their changes")),
            )
```
(`underline_tab` stays for the repo tabs in `git_menus.rs`.)

`git_commit_box`: delete `let too_long = ...;`. Generate keeps the muted label only:
```rust
            quiet_button(
                "git-commit-generate",
                icon_text(Icon::Sparkles, "Generate", theme.text_muted),
                theme,
                theme.hover,
            )
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.generate_commit_message(project_id, window, cx)
            }))
```
(drop `.text_color(rgb(theme.accent_text))`). The container's top line shows the focus (the field has no frame):
```rust
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(if focused { theme.accent } else { theme.divider }))
```
Replace the `caret_field(...)` child (and the two lines `// Flat like the panel's other fields ...` / `.when(!focused, ...)`) with a borderless field; every method after it in the chain (`.max_h`, `.overflow_y_scroll`, `.when(busy..)`, `.tooltip`, `.track_focus`, `.on_key_down`, `.relative()`, `.child(self.typing(..))`, `.on_click`) stays:
```rust
            .child({
                let input = draft.text.input();
                let (shown, layout) = summary_text(
                    input.as_str(),
                    input.cursor(),
                    input.selection(),
                    div()
                        .text_color(rgb(theme.subtle))
                        .child("Summary, then a blank line and details"),
                    focused,
                    theme,
                );
                caret_mouse(
                    div()
                        .id("git-commit-message")
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .cursor(CursorStyle::IBeam)
                        .child(shown),
                    &cx.entity(),
                    layout,
                    move |shell: &mut Shell, caret| {
                        let text = &mut shell.git_view(project_id).commit.text;
                        text.click(caret.at, caret.extend, caret.clicks);
                    },
                )
                .max_h(px(theme::GIT_COMMIT_BOX_MAX_HEIGHT))
                .overflow_y_scroll()
                // ... (the existing .when / .tooltip / .track_focus / .on_key_down / .relative()
                // / .child(self.typing(..)) / .on_click(..) chain continues here unchanged)
            })
```
Counter row, replacing the `.child(div().id("git-commit-length")...)` inside the counts row:
```rust
                    .children(git_actions::summary_counter(summary_len).map(|(label, over)| {
                        div()
                            .id("git-commit-length")
                            .flex_none()
                            .font_family(theme.terminal_font_family.clone())
                            .text_color(rgb(if over {
                                theme.status_needs_input
                            } else {
                                theme.subtle
                            }))
                            .tooltip(tooltip(
                                theme,
                                "Length of the summary line (keep it under 72)",
                                "",
                            ))
                            .child(label)
                    }))
```
Update the doc comment of `git_commit_box` ("then Generate and Commit"; counter "shown once a summary exists").

`docs/SPEC.md`: §12B tab sentence "(underlined text tabs under the repository tabs)" becomes "(a compact segmented control under the repository tabs)"; §14 heights line "Git section tabs 36" becomes "Git section segmented control (META size) in a 40 px strip, Git file rows 28 with a 16 px status chip"; §14 radius line mention nothing else.

- [ ] **Step 4: Run, expect PASS**
`cargo test -p mkrew-ui status_chips -j 2`, `cargo test -p mkrew-ui the_counter -j 2`, `cargo test -p mkrew-ui field -j 2`, then `cargo test -p mkrew-ui git -j 2` (the existing Git view tests still pass; none pins `change_letter` or the tab height).

- [ ] **Step 5: Styling check and screenshots.** `cargo fmt -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, build as in Task 22. Use the same sandbox repo plus one untracked file and one staged file (`git add` it). `smoke.ps1 start -Theme dark`, `keys -Keys "^+4"`, `shot -Out docs/screenshots/tern-polish-git-list-dark.png` (list with chips M / A / ?, a selected row with tint and bar, group headers with count chips, the segmented control, the commit box with a typed two-line message: the first line semibold, the counter visible; clear it and check the counter hides). `stop`; repeat for light. Check the panel at its narrowest (drag the panel edge to 240 px): the three segments cut with an ellipsis, none overflows. Check the `well` count chip is visible on the panel ground in both themes (spec says `well`; if invisible in dark, report to the coordinator rather than changing the token).

- [ ] **Step 6: Commit**
`git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs crates/mkrew-ui/src/git_actions.rs crates/mkrew-ui/src/field.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/git_rows.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_render.rs docs/SPEC.md docs/screenshots/tern-polish-git-list-dark.png docs/screenshots/tern-polish-git-list-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Git rows with status chips and tint selection, segmented sections, borderless commit box"`

---

### Task 24: Cards, usage rows, plan check boxes, Jira, History and Docs lists

Two commits (24a, 24b), same task. No Inbox row list exists (see the findings), so there is nothing to change for it here.

#### 24a. Cards (session panel, board, Jira), usage rows, plan check boxes

**Files:**
- Modify: `crates/mkrew-ui/src/shell/inspector.rs` (`inspector_card`, `render_session_card` usage rows, `plan_row`)
- Modify: `crates/mkrew-ui/src/shell/cards.rs` (`task_card` frame, two stale comments)
- Modify: `crates/mkrew-ui/src/shell/jira.rs` (`issue_card` frame)
- Modify: `crates/mkrew-ui/src/theme.rs` (add `CHECK_MARK_SIZE`)
- Modify: `docs/SPEC.md` (§14 line "A `card` is `surface_raised` with `radius::MEDIUM`.")

**Interfaces:**
- Consumes: `Theme.card`, `Theme.divider`, `Theme.layer[3]`, `radius::CARD` (10), `icon_svg`, `Icon::Check`.
- Produces: `theme::CHECK_MARK_SIZE = 10.0`.

Pure styling, no pure logic: no unit test; the step list ends in clippy and screenshots (a unit test of colors would only echo the constants).

- [ ] **Step 1: Implement.**

`theme.rs`, near `PLAN_BAR_HEIGHT`:
```rust
/// The check glyph inside a 14 px check box (Plan card).
pub const CHECK_MARK_SIZE: f32 = 10.0;
```
`inspector.rs`:
```rust
/// A card of the session panel: a muted title over its content (a `card` block with a 1 px
/// `divider` ring, like a board card). The Changes and Plan cards use it too.
pub(crate) fn inspector_card(theme: &Theme, title: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .p(px(space::M))
        .rounded(px(radius::CARD))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
        .bg(rgb(theme.card))
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(title),
        )
}
```
Usage rows in `render_session_card` (label `text_subtle`, value in the terminal font `text_dim`):
```rust
            .children(card.usage.iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .w(theme::rems_px(theme::INSPECTOR_LABEL_WIDTH))
                            .flex_none()
                            .text_color(rgb(theme.text_subtle))
                            .child(*label),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(theme.terminal_font_family.clone())
                            .text_color(rgb(theme.text_dim))
                            .child(value.clone()),
                    )
            }))
```
`plan_row` check box (14 px, `layer[3]` ring, accent fill and the check when done; `on_accent` is the check's color, white in light themes and the dark ink on Tern Dark's bright accent so the mark keeps its contrast; spec says "white", this is the same in light):
```rust
    let check = div()
        .flex_none()
        .mt(px(space::XS / 2.0))
        .size(px(theme::ICON_SIZE_SMALL))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(if row.done {
            rgb(theme.accent)
        } else {
            gpui::rgba(theme.layer[3])
        })
        .when(row.done, |el| {
            el.bg(rgb(theme.accent)).child(icon_svg(
                Icon::Check,
                theme::CHECK_MARK_SIZE,
                theme.on_accent,
            ))
        });
```
`DOC_TASK_BOX_SIZE` is now used by `preview.rs` only; leave it.

`cards.rs` `task_card` (anchor `.bg(rgb(theme.surface_raised))`):
```rust
        .rounded(px(radius::CARD))
        .bg(rgb(theme.card))
        // Always ringed (divider at rest, accent when selected), so neither shifts the content.
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if selected { theme.accent } else { theme.divider }))
        .map(|el| card_press(el, selected, theme))
```
(`card_press` still lights the border to `border` on hover when not selected.) Comments: in `run_with_button` change "One step above the card's own `surface_raised`, like its other quiet buttons." to "One step above the card, like its other quiet buttons."; in `card_action` "would vanish on the raised card" to "would vanish on the card". The quiet buttons still pass `theme.border` as hover.

`jira.rs` `issue_card`:
```rust
            .rounded(px(radius::CARD))
            .bg(rgb(theme.card))
            // Always ringed, so hovering and selecting do not shift the content.
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(if selected { theme.accent } else { theme.divider }))
            .map(|el| card_press(el, selected, theme))
```
`docs/SPEC.md` line "A `card` is `surface_raised` with `radius::MEDIUM`.": "A `card` is `card` with `radius::CARD`, a 1 px `divider` ring at rest, `border` on hover and `accent` when selected." Also §14 radius list already has 10 for cards.

- [ ] **Step 2: Styling check and screenshots.** `cargo fmt -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, build as in Task 22. `powershell -File scripts/smoke.ps1 mk task add api "Wire the login form"` twice (different titles), then `smoke.ps1 start -Theme dark`; open the Board from the title bar view switch (take a first `shot` to find it), select one card, `shot -Out docs/screenshots/tern-polish-board-dark.png`; open the session panel's Session tab with the fake agent (`smoke.ps1 mk task run <id>` for one task) and `shot -Out docs/screenshots/tern-polish-session-card-dark.png`. `stop`, repeat light. Check the card ring is visible against the column in both themes (`card == surface` there; if the ring is not visible, report it to the coordinator, who decides between a ring token step and a column ground change). The Plan card needs a linked checklist: link a Markdown file with `- [x]` / `- [ ]` items from the card's "Link plan..." and check the 14 px boxes; if that is not practical in the sandbox, add "check Plan card check boxes (dark/light)" to `docs/HUMAN_CHECKS.md`. Jira cards need a connected tracker: add "check Jira cards, rows and section heads (dark/light)" to `docs/HUMAN_CHECKS.md` under "To verify" in 24b.

- [ ] **Step 3: Commit**
`git add crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/screenshots/tern-polish-board-dark.png docs/screenshots/tern-polish-board-light.png docs/screenshots/tern-polish-session-card-dark.png docs/screenshots/tern-polish-session-card-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): ringed card fills for session, board and Jira cards, dim usage rows, 14 px plan check boxes"`

#### 24b. Jira rows and sections, History and Docs lists

**Files:**
- Modify: `crates/mkrew-ui/src/shell/jira.rs` (`issue_row`, `section_header`)
- Modify: `crates/mkrew-ui/src/shell/history.rs` (`render_turns_dialog` row, `render_history` row)
- Modify: `crates/mkrew-ui/src/shell/docs.rs` (`doc_row`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`BOARD_ROW_HEIGHT` 38 to 32)
- Modify: `docs/SPEC.md` (§14 heights line; Jira list row sentence near "A row: issue type, key, summary")
- Modify: `docs/HUMAN_CHECKS.md` ("To verify": Jira and Docs lines)

**Interfaces:**
- Consumes: Part B `with_pick`; Task 23 `count_chip(count: usize, theme: &Theme) -> gpui::Div` (a `shell::git_rows` item, in scope through `use crate::*`); `radius::CHIP`.
- Produces: nothing new.

Pure styling again, no unit test.

- [ ] **Step 1: Implement.**

`theme.rs`: `pub const BOARD_ROW_HEIGHT: f32 = 32.0;` (it was 38; only `jira.rs` uses it).

`jira.rs` `issue_row`: replace the radius / border block and the pressable chain:
```rust
            .h(theme::rems_px(theme::BOARD_ROW_HEIGHT))
            .pl(px(space::S + row.depth as f32 * space::XL))
            .pr(px(space::S))
            .rounded(px(radius::CHIP))
            .text_color(rgb(theme.text))
            .map(|el| with_pick(el, selected, theme))
            .pressable_unless(selected, Look::Row, theme)
```
(the always-bordered transparent `.border(..)` / `.border_color(..)` pair goes: selection is the tint and bar now.) `section_header`: title MEDIUM `text_dim`, count chip:
```rust
        .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
        .child(
            div()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_dim))
                .child(section.status.to_owned()),
        )
        .child(count_chip(section.issues.len(), theme))
```
`history.rs`, both row builders (`turn-{n}` and `history-{id}`): radius and selection:
```rust
                                .rounded(px(radius::CHIP))
                                .map(|el| with_pick(el, selected, theme))
                                .pressable_unless(selected, Look::Row, theme)
```
(the `.when(selected, |el| el.bg(rgb(theme.surface_raised)))` line goes). In the history row, drop `.when(selected, |el| el.font_weight(...MEDIUM))` from the label (the bar and tint say "selected"); the turn row's title keeps MEDIUM (it is a title over a detail line, not a selected-state cue). Both detail lines (`history::details(session, now)` and the turn `details`) use `text_subtle` instead of `text_muted`.

`docs.rs` `doc_row`:
```rust
        .rounded(px(radius::CHIP))
        .map(|el| with_pick(el, selected, theme))
        .pressable_unless(selected, Look::Row, theme)
```
Name: drop `.when(selected, |el| el.font_weight(...))`. Folder line `text_color(rgb(theme.text_subtle))`. The "N open" chip (no swap on selection, a `well` fill with a `divider` ring):
```rust
                div()
                    .flex_none()
                    .px(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.divider))
                    .bg(rgb(theme.well))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{} open", file.open_comments)),
```
`docs/SPEC.md`: heights line: add "Jira rows 32". `docs/HUMAN_CHECKS.md`: under "To verify" add "check Jira list rows (32 px, tint plus bar when selected), section heads with a count chip, Kanban cards, History dialog rows and Docs list rows (dark/light)" with a note that the sandbox has no tracker.

- [ ] **Step 2: Styling check and screenshots.** `cargo fmt -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, build. Docs list: put two Markdown files with a few comments in the sandbox workspace folder, open Docs, select one, `shot -Out docs/screenshots/tern-polish-docs-list-dark.png` (and light). History: finish a fake-agent task, open the History dialog, `shot -Out docs/screenshots/tern-polish-history-dark.png` (and light). Jira: HUMAN_CHECKS only. Check a selected row shows the tint and the 2 px bar, hover on another row lifts its label to `text` and shows the layer fill.

- [ ] **Step 3: Commit**
`git add crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/shell/history.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/HUMAN_CHECKS.md docs/screenshots/tern-polish-docs-list-dark.png docs/screenshots/tern-polish-docs-list-light.png docs/screenshots/tern-polish-history-dark.png docs/screenshots/tern-polish-history-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): tint-plus-bar selection and 32 px rows for Jira, History and Docs lists"`

---

### Task 25: Terminal find bar and pane badges

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`render_find_bar`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`badge_frame`, new `badge_look`)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`pane_badges`, the conflict badge only)
- Modify: `crates/mkrew-ui/src/theme.rs` (add `FIND_COUNT_MIN_WIDTH`, `FIND_INPUT_HEIGHT`)
- Modify: `docs/SPEC.md` (§14 Pane bullet "Floating badges ...")

**Interfaces:**
- Consumes: Part A `Theme.layer`, `Theme.glass`, `Theme.glass_hi`, `theme::shadow_pop(&Theme) -> Vec<gpui::BoxShadow>`; `radius::LARGE`, `radius::MEDIUM`, `radius::PILL`.
- Produces: `widgets::badge_look<E: Styled>(el: E, theme: &Theme) -> E` (the pill every floating pane badge shares).

Pure styling: no unit test (the status colouring is a two-variant `matches!`).

- [ ] **Step 1: Implement.**

`theme.rs`, next to `FIND_INPUT_WIDTH`:
```rust
/// The find bar's query well height, and the least width its match count takes (right-aligned,
/// so the bar does not jump as the count changes).
pub const FIND_INPUT_HEIGHT: f32 = 28.0;
pub const FIND_COUNT_MIN_WIDTH: f32 = 72.0;
```
`terminal/view.rs` `render_find_bar`. The status is a no-match / invalid pattern in `danger_text`; the count is in the terminal font:
```rust
        let failed = matches!(find.status(), FindStatus::NoMatch | FindStatus::Invalid);
        let status = match find.status() {
            FindStatus::Invalid => format!("✕ {}", find.status().label()),
            status => status.label().to_owned(),
        };
        // A 24 px icon button: the bar's only one.
        let close = div()
            .id("find-close")
            .flex_none()
            .size(px(space::XL))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::CHIP))
            .pressable(Look::Icon, true, theme)
            .on_press(
                true,
                cx.listener(|pane, _, window, cx| pane.close_find(window, cx)),
            )
            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted));
```
and the frame (the `canvas` input-handler child at the end stays as it is; `glass_hi` is drawn inline because `shell::widgets::top_highlight` is in the bin crate):
```rust
        Some(
            div()
                .absolute()
                .top(px(space::S))
                .right(px(space::L))
                .flex()
                .items_center()
                .gap(px(space::S))
                .p(px(space::XS))
                .rounded(px(radius::LARGE))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(theme.layer[1]))
                .bg(gpui::rgba(theme.glass))
                .shadow(theme::shadow_pop(theme))
                .font_family(theme.ui_font_family.clone())
                .text_size(font_size::BODY)
                .cursor(CursorStyle::IBeam)
                .track_focus(&self.find_focus)
                .on_key_down(cx.listener(Self::find_key_down))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::find_mouse_down))
                // The 1 px `glass_hi` line inside the top edge of a raised panel.
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(radius::LARGE))
                        .right(px(radius::LARGE))
                        .h(px(theme::BORDER_WIDTH))
                        .bg(gpui::rgba(theme.glass_hi)),
                )
                // The query: a `well` with `radius::MEDIUM` in the terminal font; the accent
                // border is the keyboard focus.
                .child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(theme::FIND_INPUT_HEIGHT))
                        .px(px(space::S))
                        .rounded(px(radius::MEDIUM))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(if focused {
                            rgb(theme.accent).into()
                        } else {
                            gpui::transparent_black()
                        })
                        .bg(rgb(theme.well))
                        .font_family(theme.terminal_font_family.clone())
                        .child(caret_mouse(
                            div()
                                .w(px(theme::FIND_INPUT_WIDTH))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(query),
                            &cx.entity(),
                            layout,
                            |pane: &mut Self, caret| {
                                if let Some(find) = pane.find.as_mut() {
                                    find.click(caret.at, caret.extend, caret.clicks);
                                }
                            },
                        )),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(theme::FIND_COUNT_MIN_WIDTH))
                        .text_right()
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::META)
                        .text_color(rgb(if failed {
                            theme.danger_text
                        } else {
                            theme.text_muted
                        }))
                        .child(status),
                )
                .child(close)
                // ... the existing `.child({ let (focus, view) = ...; canvas(...) })` stays here
        )
```
`widgets.rs`:
```rust
/// A floating badge over a pane (`pane_badges`): a pill on `surface_raised` with a `layer[1]`
/// ring and muted META text, so it reads over any terminal output. Not a button.
pub(crate) fn badge_frame(theme: &Theme) -> gpui::Div {
    badge_look(div(), theme)
        .flex()
        .items_center()
        .gap(px(space::S))
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .whitespace_nowrap()
}

/// The pill every floating pane badge shares (also the conflict badge, which is a button row
/// and so cannot start from [`badge_frame`]).
pub(crate) fn badge_look<E: Styled>(el: E, theme: &Theme) -> E {
    el.flex_none()
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::PILL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[1]))
        .bg(rgb(theme.surface_raised))
}
```
`pane_render.rs` `pane_badges`, the conflict badge: replace the chain after `conflict_badge(..)`:
```rust
            badges.push(
                badge_look(
                    conflict_badge(
                        format!("pane-{session}"),
                        &paths,
                        theme,
                        move |shell| shell.conflicts.dismiss_session(session),
                        cx,
                    )
                    .max_w(px(theme::PANE_BADGE_MAX_WIDTH)),
                    theme,
                )
                .into_any_element(),
            );
```
`docs/SPEC.md` Pane bullet: "Floating badges (`pane_badges`: `surface_raised` fill, 1 px `layer[1]` ring, pill radius, META text) sit ...". Add a sentence under the terminal find bar mention (§ "Terminal" near the `Ctrl+Shift+F` line is behaviour only; leave it) and to §14 Shared controls: "the terminal find bar is a glass panel (`radius::LARGE`, `layer[1]` ring, `shadow_pop`) with a `well` query field in the terminal font, a 72 px right-aligned match count (`danger_text` with no match) and a 24 px close button."

- [ ] **Step 2: Styling check and screenshots.** `cargo fmt -p mkrew-ui`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`, build. `smoke.ps1 start -Theme dark`, click into the terminal, `keys -Keys "^+f"`, type a query that matches (`keys -Keys "api"`) and `shot -Out docs/screenshots/tern-polish-find-dark.png`; type one that does not match (`keys -Keys "zzzz"`), check the count text turns `danger_text` and `shot -Out docs/screenshots/tern-polish-find-nomatch-dark.png`. Copy mode badge: `keys -Keys "^+x"` is the default copy mode key only if the keymap says so; check `scripts`/keymap for `CopyMode` and press it, or split the pane and zoom it to show "Pane zoomed", then `shot -Out docs/screenshots/tern-polish-pane-badge-dark.png`. `stop`, repeat light. Check the badge is a pill with a faint ring and the find bar's top highlight line is visible in dark.

- [ ] **Step 3: Commit**
`git add crates/mkrew-ui/src/terminal/view.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/screenshots/tern-polish-find-dark.png docs/screenshots/tern-polish-find-light.png docs/screenshots/tern-polish-find-nomatch-dark.png docs/screenshots/tern-polish-pane-badge-dark.png docs/screenshots/tern-polish-pane-badge-light.png`
`git -c user.email=tungnv975@gmail.com commit -m "feat(ui): glass find bar with a well query and pill pane badges"`

---

### Notes for the coordinator

- Part G needs Part A (Tasks 1-4) merged before Task 22 (Tern constructors, new `surface`) and Part B's `with_pick`, `segment`/`segmented` before Tasks 23 and 24. Task 25 needs only Part A.
- Part B should not rename `segment(id, active, theme)`, `segmented(theme)` or `Pressable::on_press`; Task 23 uses them as they are today.
- Two deliberate deviations to confirm: numbers at 75 % in light themes (computed 2:1 at 60 %); `hunk_bg` is computed over `surface`, the real diff ground, not `bg`.
- `git_untracked` is left in place unused by rows; `problems()` and `from_palette` still use it.
- `GIT_SECTION_TAB_HEIGHT` is removed; `underline_tab` stays (the repo tabs in `git_menus.rs` use it).
- Files Part G edits that other parts also edit: `theme.rs` (A, F), `widgets.rs` (B: `badge_frame`/`badge_look` only), `pane_render.rs` (F: `pane_frame`; G: `pane_badges`), `docs/SPEC.md` §14 (A rewrites the colour table; Task 22 edits only the `diff-*` rows and adds the `hunk-*` rows).


---


---

## Part H: Finish (Task 26)

### Task 26: Screenshots, human checks, SPEC sweep, full verify

**Files:**
- Create: `docs/screenshots/tern-polish-<view>-<theme>.png`
- Modify: `docs/HUMAN_CHECKS.md` ("To verify"), `docs/SPEC.md` §14 / §12A (sweep), `docs/decisions/0117-tern-polish.md` (Consequences), `docs/PROGRESS.md` (notes for the next session)

**Interfaces:**
- Consumes: everything from Tasks 1-25.
- Produces: the evidence the final review reads.

- [ ] **Step 1: Full verify**

Run (Git Bash, from the worktree): `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test pass. Fix any failure at its root before continuing; known flaky tests (spike-term `grid_snapshots` in a workspace run, the archive test) are rerun alone to confirm.

- [ ] **Step 2: Screenshots, Mkrew Dark / Mkrew Light / Tern Dark / Tern Light**

For each theme, start the sandbox with `powershell -File scripts/smoke.ps1 start -Theme dark` (or `light`; set `[ui] dark_theme` / `light_theme` in the sandbox `settings.toml` to the Tern names for the Tern shots), drive these views and save `shot -Out docs/screenshots/tern-polish-<view>-<theme>.png` where `<theme>` is `mkrew-dark`, `mkrew-light`, `tern-dark`, `tern-light`:
`split` (fake agent pane plus a file pane), `chat` (Chat view with a reply that has a heading, a list, a table and a code block), `files` (tree plus Markdown preview), `menu` (a context menu open), `dialog` (a confirm dialog), `settings` (Settings → Appearance), `git` (Git diff), `toast`.
Then one shot with `[ui] font_size = 18`: `tern-polish-split-font18-mkrew-dark.png`.
Stop the sandbox (`mk.exe daemon stop` for the sandbox, then kill only mkrew processes under the sandbox path).
Look at every image next to the Tern screenshots: fix anything that clips, overlaps or misaligns before continuing.

- [ ] **Step 3: Human checks**

Append under "To verify" in `docs/HUMAN_CHECKS.md` one line per view the sandbox cannot show:
- omp pane transcript (turn gaps, prompt bubble with its head on hover, tool rows, thinking rows, code blocks) in Mkrew Dark and Tern Light.
- omp dock (meter on the card edge, model pill, Send key chip, Stop text button, facts, hud, queue pills) in both themes.
- Composer fit at narrow pane widths (chips drop from the right).
- Any check earlier tasks deferred here (collect them from the task reports).

- [ ] **Step 4: SPEC and ADR sweep**

`rg -n "radius::COMPOSER|glass_tint|shadow_float|classic\(|round_button|round_stop_button|SEND_BUTTON_SIZE" crates docs/SPEC.md` must return nothing.
Re-read SPEC §14 and §12A against the code values; fix any stale number.
Add to ADR 0117 "Consequences": the screenshots list and the HUMAN_CHECKS entries.
Add a short "Notes for the next session" entry to `docs/PROGRESS.md` (branch, spec, plan, ADR 0117, no protocol change).

- [ ] **Step 5: Commit**

```bash
git add docs/screenshots/tern-polish-*.png docs/HUMAN_CHECKS.md docs/SPEC.md docs/decisions/0117-tern-polish.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: Tern polish screenshots, human checks and SPEC sweep"
```
