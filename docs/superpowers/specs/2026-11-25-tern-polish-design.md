# Tern polish: surfaces, text tiers, reading rhythm across the app - design

- Date: 2026-11-25
- ADR: 0117 (amends 0116, 0105, 0097, 0085, 0078)
- Approval: the user approved the gap review and the plan in chat on 2026-11-25 with Q1 = B (retune Mkrew, add Tern Dark/Light), Q2 = no pane tiles, Q3 = Tern's Send/Stop, Q4 = chat 1.6 / docs 15 px 1.65 / UI 13 px 1.5, Q5 = toast stays bottom right, restyled.
- Evidence: Tern's CSS extracted from `%LOCALAPPDATA%\Programs\Tern\tern.exe` (`core.css` tokens, `c180811611.css` components, `c174307499.css` tree/menus/dialogs); six gap reports (palette, chat, composer, files, overlays, panels) with selector, file and line for every value.
- Builds on ADR 0116 (Geist, line height 1.5, quiet tabs, orbit mark); nothing here undoes its fonts or running marks.

## 1. Goal

Mkrew reads as calmly as Tern everywhere, not only in the chat.
Success, judged on dark and light screenshots next to Tern:

- grounds are neutral grey and ordered by lightness (page < panel < card < raised < floating);
- body text, names, meta and placeholders sit on four visibly different text tiers;
- the omp transcript has air between turns, a right-hand prompt bubble, sunken code and no boxed tool rows;
- the composer is one quiet card with the context line on its top edge;
- menus, dialogs, Settings, Git and the file tree use tint-plus-bar selection and alpha hover steps;
- the user can pick "Tern Dark" / "Tern Light" (blue accent, Tern status and ANSI colors) in Settings → Appearance.

Constraints kept: every value from `theme.rs`; spacing 4/8/12/16/24/32 (stated exceptions only); both themes; no continuous animation (ADR 0086); clickables through `shell/widgets.rs` / `pressable.rs`; theme files keep loading; body text ≥ 4.5:1.

## 2. Why Tern reads better (the principles applied)

1. Grounds ordered by lightness, neutral (blue at most +3 over red/green).
2. Four text tiers with fixed jobs: t1 content, t2 names and labels, t3 meta, t4 placeholders, line numbers, chevrons.
3. Lines and hover fills are alpha layers (l1..l4), so the same step shows on any ground.
4. Space shows structure; only the user's prompt is a filled shape in the transcript.
5. Selection is an accent tint plus a short 2 px bar; rings are for keyboard focus.
6. Code is sunken (darker than the page) with a faint ring; floating panels are lighter than what they cover.
7. Quiet chrome: icon buttons, sentence-case labels in t3, no borders between regions.
8. One height rhythm: controls 24 in bars, rows 24-28, panel heads 34-36, diff lines 20.

## 3. Theme system (`crates/mkrew-ui/src/theme.rs`)

### 3.1 New and changed `Theme` fields

| Field | Kind | Meaning |
|---|---|---|
| `layer: [u32; 4]` | new, RGBA | l1..l4 overlays: dark white at 7/10/15/24 %; light `0x181A2C` at 6/9/14/22 %. Hover (l1), pressed (l2), rings (l2), strong rings and scrollbar thumbs (l3), check box borders (l4). |
| `code_bg` | new, RGB | Sunken code ground: dark `0x0C0C0D`, light `0xF8F8F6`. |
| `composer_fill` | new, RGB | The composer card: dark `0x1D1D21`, light `0xFFFFFF`. |
| `glass_hi` | new, RGBA | 1 px top inner highlight of raised and floating panels: dark white 5 %, light white 100 %. Replaces `glass_tint`. |
| `glass_tint` | removed | The 120 px accent wash of floating panels goes (ADR 0097 amended); `GLASS_TINT_*` constants and `glass_gradient` go with it. |
| `classic` | removed | The pre-restyle sidebar palette (`classic_dark/light`, `Theme::classic()`, the "sidebar text" contrast rule, the two `sidebar.rs` call sites) goes; the sidebar uses the main tokens. |

`raised_border()` stays as a function but returns `layer[1]` flattened over `surface_raised`.

### 3.2 Mkrew Dark / Mkrew Light retune (accent family kept)

Grounds, lines and text move to Tern's values; accent, status, brand, diff hues, syntax, ANSI and workspace colors stay Mkrew's.
Values that derive from the accent are recomputed over the new grounds with the same formulas `from_palette` uses (`accent_bg = mix(bg, accent, 0.14)`, `selected_ring` and `pane_focus_stripe = mix(accent, bg, 0.5)`).

| Field | Dark | Light |
|---|---|---|
| `pane_gap` (page, title bar) | `0x050506` | `0xEFEFEC` |
| `sidebar` | `= pane_gap` | `= pane_gap` |
| `bg`, `terminal_bg` | `0x0F0F12` | `0xFBFBFA` |
| `surface`, `card` | `0x161619` | `0xFFFFFF` |
| `surface_raised` | `0x232327` | `0xF4F4F2` |
| `selected` | `0x232327` | `0xE7E7E7` |
| `chrome` (fields on the bar) | `0x19191B` | `0xE3E3E2` |
| `tab_active` | `0x1E1E1F` | `0xD6D6D4` |
| `hover`, `row_hover` (l1 over `bg`) | `0x202023` | `0xEDEEEE` |
| `divider` (l1) | `0x202023` | `0xEDEEEE` |
| `border` (l2) | `0x27272A` | `0xE7E7E7` |
| `control_border` (l3) | `0x333336` | `0xDBDCDD` |
| `well`, `diff_hunk_header` | `0x17171A` | `0xF4F4F2` |
| `text` (t1) | `0xEDEDED` | `0x1B1C20` |
| `text_dim` | `0xC7C7C7` | `0x383A40` |
| `text_muted` (t2) | `0xA1A1A1` | `0x55575F` |
| `text_subtle` (t3) | `0x8A8A92` | `0x6A6C74` |
| `text_disabled` | `0x707077` | `0x8F9198` |
| `subtle` (t4, graphics, placeholders) | `0x55555C` | `0xB3B5BB` |
| `status_idle` | `0x8A8A92` | `0x6A6C74` |
| `folder` | `= text_subtle` | `= text_subtle` |
| `warning_bg` / `success_bg` / `danger_bg` | 14 % of the status hue over `bg` | same rule |
| `terminal_selection_bg` | `mix(bg, accent, 0.30)` checked by `fill` for 4.5:1 | same rule |

`PANE_GAP_MIN_CONTRAST` stays 1.06; its doc comment is corrected (Tern's default tokens measure 1.02-1.05; the 1.07 came from a themed screenshot).

### 3.3 Tern Dark / Tern Light (new built-ins)

`Theme::tern_dark()` / `Theme::tern_light()`: the retuned Mkrew theme of the same appearance with these overrides, then `with_glass()`:

| Field | Tern Dark | Tern Light |
|---|---|---|
| `accent` | `0x5A8CFF` | `0x2F63F0` |
| `accent_text` | `0x9DBCFF` | `0x2448C4` |
| `accent_bg` | `0x1A2133` | `0xDEE6F9` |
| `on_accent` | `0x08080A` | `0xFFFFFF` |
| `selected_ring` | `0x354E89` | `0x95ADF5` |
| `pane_focus_stripe` | `0x3B6CFF` | `0x3B6CFF` |
| `status_working` | `0x5A8CFF` | `0x2F63F0` |
| `status_needs_input` | `0xF5A524` | `0x8B570D` (Tern's `#d98a0b` fails 4.5:1) |
| `status_done` | `0x3ECF8E` | `0x167A4D` |
| `status_exited` | `0xFF6166` | `0xBF3038` |
| `warning_bg` / `success_bg` / `danger_bg` | `0x2F2415` / `0x162A23` / `0x311A1E` | `0xF6EBD9` / `0xDEF0E7` / `0xF8E2E2` |
| `run_pill_bg` / `_border` / `_text` | `0x152621` / `0x1F523D` / `0x3ECF8E` | `0xE3F2EA` / `0xB4E0CB` / `0x167A4D` |
| `success_text` / `danger_text` | `0x3ECF8E` / `0xFF6166` | keep Mkrew Light's |
| `terminal_selection_bg` / `terminal_match_bg` | `0x27375E` / `0x5D4621` | `0xD6E0FC` / `0xF8DCAB` |
| `ansi` | `50525A FF6166 3ECF8E F5B13D 5B8DFF B98CFF 35C7E6 C9CAD0 7A7C85 FF8A8E 6FE3B0 FFD27A 8FB6FF D2B3FF 7FDCF0 F7F7F8` | `1F2126 CF3A3F 1D8A5A A86A00 2F5FE0 8A4FD8 0F7F93 6F727B 80838C E5484D 2FAE74 C98A0B 3D6FFF A86AF4 16A3C4 9EA1A8` |
| `name` | "Tern Dark" | "Tern Light" |

Catalog: `THEME_NAMES` (mkrew-core) and `THEMES` (UI) list "Tern Dark" right after "Mkrew Dark" and "Tern Light" right after "Mkrew Light".
`ThemeEntry` replaces `palette: Option<Palette>` with a recipe enum (`Mkrew`, `Tern`, `Palette(Palette)`) so `build()` reaches the hand-written constructors; `palette()` returns `Some` only for `Palette`.
Defaults stay `Mkrew Dark` / `Mkrew Light`.
"New theme from current" (`Palette::from_theme`) works for Tern as for Mkrew (the tiers are re-derived).

### 3.4 Derived themes (`from_palette`) and theme files

They gain the new fields and a real tier split, so One Dark, Nord and user theme files get the same hierarchy:
`layer` from `fg` (dark) or `0x181A2C` (light) at the §3.1 alphas; `code_bg = mix(bg, 0x000000, 0.25)` dark / `mix(bg, fg, 0.02)` light; `composer_fill = mix(bg, fg, 0.06)` dark / `surface` light; `text_subtle = readable(mix(muted, bg, 0.25), fg, grounds, 4.5)`; `subtle = mix(muted, bg, 0.45)`; `sidebar = pane_gap`.

### 3.5 Floating panels, scrim and shadows

- `with_glass`: the fill is `surface_raised` (dark) or `surface` (light) at the current contrast loop's alpha, so a floating panel is always lighter than the page in dark.
- `scrim`: dark black at 55 %; light `pane_gap` at 50 % (a milky veil, not black).
- `glass_border` becomes `layer[1]`.
- New `shadow_pop(theme)`: `0 18 50 -12` black 30 %, used by menus, tooltips, peek, toasts, the find bar.
- New `shadow_modal(theme)`: dark `0 28 72 -16` black 78 %; light `0 24 64 -18` black 32 % plus `0 2 6 -2` black 6 %. Used by dialogs, palette, Settings. `shadow_float` is removed.
- New `shadow_composer(theme)`: dark `0 12 30 -16` black 60 %; light `0 18 50 -14` black 28 %.
- Raised and floating panels draw `glass_hi` as a 1 px line inside their top edge.

### 3.6 Contrast rules added to `problems()`

- In dark themes the glass fill over `bg` is lighter than `bg`.
- `selected` differs from `surface_raised` and from the flattened glass fill (a picked menu row must show).
- `text_subtle` ≥ 4.5:1 on `bg`, `surface`, `card`, `terminal_bg`, `surface_raised`, `composer_fill`.
- `subtle` ≥ 3:1 on `bg` is not required (graphics and placeholders only); no rule.
- Every existing rule keeps passing for every built-in theme, including the two new ones.

## 4. Shared controls and floating UI (`shell/widgets.rs`, `pressable.rs`, menus, dialogs)

| Item | Decision |
|---|---|
| Hover / pressed | `Look::Row`, `Chip`, `Icon`, `Raised` hover = `layer[0]` overlay, pressed adds `layer[1]`; `Row` and `Chip` hover also lift the label to `text`. `Raised` is no longer `border` (which was darker than the menu fill). |
| Selected row (lists, menus, tree, Git, Jira, History, Docs, palette) | One helper: `accent_bg` fill plus a 2 px accent bar, 14 px tall, vertically centred at the left inside the row; label `text`. Keyboard focus keeps its ring. Replaces `selected` fills and `pane_focus_stripe` borders on rows. |
| Menu panel | glass fill, padding `space::S`, `radius::LARGE`, `layer[1]` ring, `shadow_pop`, `glass_hi`; rows fixed 28 px, `radius::CHIP`, gap `space::S`; separator `divider` with `mx(space::XS)`; heading META MEDIUM `text_subtle`, `pt(S) pb(XS)`. |
| Tooltip | `radius::MEDIUM`, glass fill, `layer[1]` ring, `shadow_pop`, max width 280, keys drawn with `kbd()`. |
| Dialogs | One `dialog_frame(icon, title, sub, body, foot)` helper: head with a 32 px icon tile (`radius::MEDIUM`, `accent_bg` fill, `layer[1]` ring; danger tint for destructive), title CARD_TITLE SEMIBOLD, sub META `text_subtle`, close 26 px icon button; body `px(XL) pb(L)`, gap `L`; foot band with a `divider` top line, `py(M) px(L)`, primary button min width 84 with `kbd("↵")`. Radius `radius::COMPOSER` (16, renamed `radius::PANEL`). `DIALOG_WIDTH` 520 → 560. Used by confirm, task, undo, discard, worktree, send, ports, phone, ask, turns dialogs. |
| Error note | One `error_note(text)` widget: `danger_bg` fill, 1 px ring `mix(danger_bg, status_exited, 0.3)`, CircleAlert 14, META. |
| Buttons | `CONTROL_HEIGHT` 30 → 32. Secondary: `surface_raised` fill, `layer[1]` ring, `glass_hi`, label `text` MEDIUM; hover ring `layer[2]`. Primary: accent fill, MEDIUM (was SEMIBOLD), `glass_hi`. Danger: `danger_bg` fill at rest plus 1 px ring, `danger_text`. Quiet and icon buttons unchanged except the hover rule above. |
| Focus | Keyboard focus: 1 px `accent` border plus a 3 px halo (`BoxShadow` spread 3, accent at 18 %). On an accent-filled button the border is `focus_ring` (text color) as today. ADR 0085 amended. |
| Inputs | Fill `layer[0]` over the dialog, 1 px `layer[1]` ring, `INPUT_HEIGHT` 32 for single-line fields, placeholder `subtle`; focus per the row above. Search field `px(M)`. |
| Switch | `SWITCH_WIDTH` 36, `SWITCH_HEIGHT` 22, knob 18 always white with `0 1 3` black 25 % shadow; off track `layer[2]` plus `layer[0]` ring; on track `accent`. |
| Segmented | Track `layer[0]` with padding 2; active segment `surface_raised` fill, `layer[1]` ring, `glass_hi`; labels MEDIUM. |
| Kbd | Fixed height 18, min width 18, centred, `KBD_BOTTOM_BORDER` 2 → 1, MEDIUM, `layer[0]` fill, `layer[1]` ring. |
| Chips / badges | 1 px ring of the tone at 22 % over the fill; weight 600 → 500. |
| Toast | Stays bottom right. Min height 40, `radius::LARGE`, glass fill, `layer[1]` ring, `shadow_pop`, BODY; an error has a 1 px `status_exited` ring at 40 % instead of only a red icon; the ✕ shows on hover (`group_hover`). Clamp and click-to-copy stay. |
| Empty state | Icon in a 44 px circle (`radius::PILL`) with `layer[0]` fill and `layer[1]` ring, icon 20; gap 12; title 14 px SEMIBOLD; sentence `text_subtle`; action `mt(L)`. |
| Peek popover | `radius::LARGE`, glass, `shadow_pop`; its hint line META `text_subtle` in a foot band with a `divider` line. |
| Palette | Selected row = layer hover fill plus a 2 px bar inset 8 px top and bottom (`PALETTE_SELECTED_BAR` 3 → 2). |
| Scrollbars | Idle thumb `layer[2]`, held `text_subtle`. |

## 5. omp transcript and Chat view (`shell/tsp_view.rs`, `tsp.rs`, `preview.rs`, `shell/chat_view.rs`)

| Item | Decision |
|---|---|
| Turn gap | 16 px (`TSP_ROW_GAP = space::L`) below every top-level row that draws something; folded rows (`RowPlan::Folded`) add nothing. |
| Inside an assistant turn | A `col` whose role starts with `omp.assistant` gets at least `space::S` gap. |
| User prompt (`card` role `omp.user`) | Right-aligned bubble: `max_w` = min(80 %, 540 px) (`CHAT_PROMPT_MAX_WIDTH = 540`), `py(S) px(L)`, radii 12/12/4/12 (bottom right 4), fill `layer[1]`, BODY text at 14/1.5. Its head row (copy, rewind, time) leaves the bubble and shows under it, right-aligned, META `text_subtle`, on hover of the row (`group_hover`). The Chat view's prompt card gets the same look. |
| Markdown line height | Chat flavor 1.6 (`doc::CHAT_LINE_HEIGHT`); Docs flavor see §7; UI 1.5 unchanged. |
| Headings | `text` (no accent), SEMIBOLD, line height 1.35; `pt(XS)` above (plus the 12 px block gap), 12 below. |
| Lists | Item gap `space::S` in chat; bullet `text_subtle` 6 px dot; nested items `text_muted` with a hollow ring bullet. ADR 0105's accent dot is amended. |
| Tables | Cells `py(XS)`; header META MEDIUM `text_muted`; line under the head `border`, between rows `divider`. |
| Code block | `code_bg` fill, 1 px `layer[1]` ring, `radius::LARGE`; head 32 px (`CODE_HEAD_HEIGHT`) with a `divider` line under it, language META `text_subtle`; "Copy" is plain text (no icon) META `text_subtle` → `text` on hover (`Look::Text`); code lines 13 px at 1.6 in chat. |
| Inline code | `layer[0]` fill in chat and Docs (was `surface_raised` / `hover`). |
| Links | `text` with a 1 px accent underline at 40 % (`with_alpha`). |
| Blockquote | Bar `border`, text `text_muted`. |
| Tool rows (`omp.tool.*` cards) | No box: head min height 24, `px(XS)` with `radius::SMALL` hover fill (`layer[0]`), icon 14 `text_subtle`, title MEDIUM `text`, meta in the terminal font META `subtle`; body `pt(S)`; code, diff and ANSI bodies on `code_bg` with a `layer[0]` ring, `radius::MEDIUM`. Other cards (`omp.welcome`, overlays) keep their frame. |
| Thinking rows | Min height 24, BODY (13) italic `text_subtle`, hover `text`; open body `pl(L)` italic `text_subtle`. |
| Working row | Aligned with the text column (same `page_column`), label MEDIUM `text_muted`. |

## 6. Composer and dock (`shell/composer.rs`, `tsp/dock.rs`, dock functions in `shell/tsp_view.rs`)

| Item | Decision |
|---|---|
| Card | `composer_fill`, 1 px `layer[1]` ring, `glass_hi` top line, `shadow_composer`, `radius::PANEL` (16, the renamed `radius::COMPOSER`); padding `pt(M) pb(S) px(M)`. Shared by the omp dock and the Chat composer. |
| Placement | Card spans the transcript column (`tsp::column_max`), `pb(L)` from the pane bottom. |
| Context meter | Drawn absolutely on the card's top edge (`top(-12)`, `left/right(space::L)`), height 14, the 1 px track at its middle; rail `text` at 9 %, fill `text` at 42 % (tone colors past the thresholds stay); labels OVERLINE (11) mono MEDIUM `text_subtle`. No separate row above the card. |
| Working / hud rows | Row height 26, left padding 16 to line up with the card text; divider glyphs `subtle`; hud pill height 24, `layer[1]` ring, MEDIUM, 2 px inner padding, segments 20 px with `layer[0]` hover; gap above the card 8. |
| Model pill | Height 24 (`COMPOSER_PILL_HEIGHT` 28 → 24), `radius::MEDIUM`, `layer[0]` fill, no ring, META MEDIUM `text`, effort glyph and chevron `text_subtle`; hover `layer[1]`. |
| Send (Q3) | A key chip: height 24, min width 28, `radius::MEDIUM`, `layer[0]` fill, `layer[1]` ring, return glyph 12 px `text_subtle`; hover `layer[1]` fill and `text` glyph. Disabled dims. |
| Stop (Q3) | A text button "Stop": height 24, `px(M)`, `radius::MEDIUM`, META SEMIBOLD `danger_text`, fill `status_exited` at 8 %, ring at 22 %, hover fill 14 %; tooltip shows its key. |
| Facts | OVERLINE (11) mono `text_subtle`, icons `subtle`, gap 4 between facts with `px(XS)` per fact; `layer[0]` hover when clickable. |
| Usage / cost | OVERLINE mono `subtle`; warning / error tones unchanged. |
| Placeholder | `subtle`. |
| Queue (`omp.queue`) | New dock roles `Queue` / `QueueItem`: pills 26 px, `radius::PILL`, `layer[0]` fill, META `text_muted`, icon 14 `text_subtle`, gap 4. |
| Scroll to bottom | A 28 px icon button with no frame or shadow at rest (`Look::Icon`), chevron 16 `text_subtle`. |
| Fit reckoning | `COMPOSER_*` widths, `TSP_*` widths and `COMPOSER_MONO_CHAR_WIDTH` updated to the new sizes so `fit_bar` drops the same parts at the same pane widths. |

## 7. Files tree, file pane, editor and Markdown preview

| Item | Decision |
|---|---|
| Tree names | `text_muted`; hover, cursor and opened rows `text`; ignored and dot files `text_subtle`. |
| Opened row | The selected-row helper of §4 (tint plus bar), icon `accent`. Cursor without focus: hover fill only. |
| Indent guides | One 1 px vertical `layer[1]` line per depth level, at the chevron centre of that level, drawn inside each `uniform_list` row. |
| Icons | Extension map to Lucide icons already vendored or added to `icons.rs` (FileCode, FileText, FileJson, FileCog, FileImage, FileSpreadsheet, Lock, Settings) in `text_subtle`; folders `text_subtle` (no blue); chevron `subtle`. |
| Git marks | A 6 px dot (`FILE_CHANGES_DOT_SIZE`) in the status color at the row end plus the name tinted `git_modified` / `diff_add_text` / `danger_text`; the letter moves to the tooltip. |
| Row | Icon-to-name gap `space::S`. |
| File pane header | Breadcrumb: folder segments META `text_subtle` with `›` in `subtle`, file name BODY MEDIUM `text`; "Ln, Col" META `text_subtle` tabular; Save, Find, Open in Zed and Review in Docs become icon buttons with tooltips and keys; unsaved state is a dot on Save. |
| Docs flavor (file preview, Docs view) | Column max 720 (`DOC_PREVIEW_MAX_WIDTH`), bottom padding 32; body 15 px at 1.65 (`doc::BODY`, `doc::LINE_HEIGHT`); paragraph and list text `text_muted` in dark themes, `text` in light; bold and headings `text`; block gap 16. |
| Doc headings | h1 32, h2 22 with `pt(XXL)`, h3 17 with `pt(XL)`, h4 15 with `pt(L)`; no rule under h1/h2. |
| Doc blocks | Bullets `text_subtle`; ordered numbers terminal font META `text_subtle`; hr `my(XL)`; quote bar `border`, gap 16, text `text_muted`; table header META regular `text_muted` on `surface`, cells `py(S) px(M)`, ring `layer[1]`, `radius::LARGE`. |
| Editor | Gutter numbers `text_subtle`, the current one `text` (no accent); current line `row_hover` band; caret `accent`, no blink; `code_line_height` (Files editor) 1.35 → 1.5; code blocks in the Docs flavor 13 px at 1.6 (`doc::CODE_LINE_HEIGHT` 1.45 → 1.6, shared with chat). |

## 8. Window chrome (`shell/chrome.rs`, `sidebar*.rs`, `widgets.rs`, `pane_render.rs`)

| Item | Decision |
|---|---|
| Sidebar | Sits on `pane_gap` (sidebar = page) with no `border_r`; rows `text_muted`, hover `layer[0]` and `text`; filter field 28 px, `well` fill, `layer[1]` ring, placeholder `subtle`. |
| Tabs | Sizes stay (ADR 0116); colors follow §3. |
| Panel tabs (Files / Session / Git) | Active: `accent_bg` fill, icon `accent_text`. |
| Focus stripe among several panes | 1 px (`BORDER_WIDTH`) `pane_focus_stripe`; needs-input and session color stay 2 px. |
| Panes | No gaps or rounded panes (Q2); the 1 px divider stays. |

## 9. Settings dialog (`settings_dialog.rs`, `shell/settings.rs`)

| Item | Decision |
|---|---|
| Frame | 960 × 720 (`SETTINGS_DIALOG_*`), `radius::PANEL`, `shadow_modal`. |
| Nav | Width 232, `well` fill, `layer[1]` right ring; rows 30 px, `radius::MEDIUM`, a 14 px icon per page, gap 8, `text_muted` → `text`; group heads sentence case META MEDIUM `text_subtle`; groups gap 16. |
| Page head | Inside the scroll: page title VIEW_TITLE SEMIBOLD plus one lead sentence BODY `text_subtle`; content max width 740, centred, `px(XXL) pt(XXL)`. Each page gets a one-sentence lead. |
| Sections | Title sentence case META MEDIUM `text_subtle` (no uppercase overline), 24 above. Card `surface` with `layer[1]` ring, `radius::LARGE`, `divider` lines between rows. |
| Rows | Min height 56, `px(L) py(M)`, gap 24; label BODY MEDIUM `text`, help META `text_subtle`; control column 260. |

## 10. Git, diff, board, lists and pane chrome

| Item | Decision |
|---|---|
| Diff text | `DIFF_FONT_SIZE` 12, `DIFF_LINE_HEIGHT` 20. |
| Diff rows | Add / delete / modified fills at 7 % of the hue over the diff ground; word highlights 26 % / 25 %. |
| Diff gutter | Its own band using `diff_add_gutter` / `diff_del_gutter` / `diff_mod_gutter` (row fill plus 8 %); numbers in new tokens `diff_add_num` / `diff_del_num` (hue at 60 % over the gutter); the +/- sign keeps the full hue; context numbers `subtle`. Widths stay 40 / 16. |
| Hunk header | Height 24 (`GIT_HUNK_HEADER_HEIGHT`), fill `mix(bg, accent, 0.045)` (new token `hunk_bg`), `divider` lines top and bottom, `@@` in `accent_text`, section text `text_dim`; current hunk `mix(bg, accent, 0.09)` plus a 3 px accent bar, no ▸ and no bold. Hunk Stage / Discard are 18 px chips (`radius::TAG`, META, accent tint with a 18 % ring; Discard neutral `layer[0]`). |
| Changed-file rows | Name regular weight; status chip 16 px, `radius::SMALL`, `theme.tint(color)` fill, letter terminal font OVERLINE SEMIBOLD; R/C use `accent`, `?` a ring of `status_done` at 35 % with no fill; selection per §4; rows `radius::CHIP`. |
| Group headers | META SEMIBOLD `text`; count as a terminal-font chip on `well`. |
| Section tabs | A compact `segmented()` at META size instead of underline tabs. |
| Commit box | Borderless; summary line BODY SEMIBOLD; placeholder `subtle`; counter terminal font `subtle`, hidden at 0, `status_needs_input` past 72; Generate button `text_muted`. |
| Cards (session panel, board, Jira) | `card` fill, `radius::CARD`, 1 px `divider` ring at rest, `border` on hover, `accent` ring when selected. |
| Jira rows | `BOARD_ROW_HEIGHT` 38 → 32; selection per §4; section head MEDIUM `text_dim` with a count chip. |
| History, Docs list, Inbox rows | Selection per §4; folder META `text_subtle`; `radius::CHIP`. |
| Usage rows | Label `text_subtle`, value terminal font `text_dim`. |
| Plan check boxes | 14 px, `layer[3]` border; done = `accent` fill with a white check icon. |
| Terminal find bar | `radius::LARGE`, glass, `layer[1]` ring, `shadow_pop`; input `well` with `radius::MEDIUM` in the terminal font; count terminal font, min width 72, right-aligned, `danger_text` at 0 matches; 24 px icon buttons. |
| Pane badges | Pill, META, `layer[1]` ring on `surface_raised`. |

## 11. Not copied

- Backdrop blur, saturate, specular radial gradients, masked gradient borders: GPUI has none; near-opaque fills plus rings instead.
- Continuous animations (omp comet, shimmer, 0.9 s spinner, caret blink, pulse dots, scale-in): ADR 0086.
- Letter spacing; Tern's 12.5 / 13.5 / 15.5 px steps (mapped onto META / BODY / CARD_TITLE).
- The `--grad` accent gradient on buttons and the accent / red glows behind dialogs.
- Folder-before-name order in Git rows.
- Tern's red "waiting" status: Mkrew keeps amber for needs input.
- Rounded word highlights and padded inline code (GPUI `HighlightStyle` cannot).
- Pane tiles with gaps and radius (Q2).

## 12. Docs

- ADR 0117 "Tern polish": records §3-§10 and amends 0116 (composer pill ring → fill, round Send/Stop → key chip and Stop text button, Markdown 1.5 → chat 1.6 / docs 1.65), 0105 (accent bullet → `text_subtle`, accent headings → `text`), 0097 (glass tint wash → `glass_hi` line), 0085 (focus ring → accent plus halo), 0078 (built-in list gains Tern Dark/Light; `ThemeEntry` recipe).
- SPEC §14 token tables and §12A file pane header updated in the same commits as the code.
- `docs/HUMAN_CHECKS.md`: omp pane (transcript, dock) dark and light, because an omp pane cannot run in the smoke sandbox.

## 13. Testing

- Unit: `problems()` empty for every built-in theme including Tern Dark/Light; the new rules (glass lighter than `bg` in dark, `selected` ≠ menu fill, `text_subtle` 4.5:1) each fail on a hand-broken theme; `THEMES` names equal `THEME_NAMES`; `ThemeEntry::build` returns the Tern constructors; `from_palette` gives `text_subtle` darker than `text_muted` on a dark palette and still ≥ 4.5:1; `press_style` for each look returns the layer hover; `fit_bar` drops the same chips at the boundary widths after the size changes; the selected-row helper and thinking/turn gap skip folded rows.
- Snapshots: `the_mkrew_themes_never_change` re-accepted once with ADR 0117; new snapshots for Tern Dark/Light.
- Screens: `scripts/smoke.ps1` dark and light shots (Mkrew and Tern) of the split with a fake agent pane and the Chat view, the file pane in preview, the tree, a menu, a confirm dialog, Settings → Appearance and General, the Git diff, a toast, as `docs/screenshots/tern-polish-<view>-<theme>.png`.
- `scripts/verify.sh` green before each merge.

## 14. Coordination

`fix/composer-polish` (worktree `.worktrees/composer-polish`) has uncommitted edits to `theme.rs`, `shell/tsp_view.rs`, `shell/space_render.rs`, `shell/window_mode.rs`, SPEC and ADR 0115.
Implementation starts on a branch from master after that branch merges, or rebases onto it; the composer and transcript tasks touch the same functions (`tsp_dock*`, `push_runs`, `composer_card_frame`).
