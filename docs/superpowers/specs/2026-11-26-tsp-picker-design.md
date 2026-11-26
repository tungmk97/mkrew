# omp picker sheet and Markdown accent - Design

- Date: 2026-11-26
- Status: approved by the user (2026-11-26: colors back on headings and bullets, and omp's picker drawn).
- Branch and worktree: `feat/tsp-look` in `.worktrees/tsp-look`, from `master` at `7c84fdb3` (v0.10.2).
- ADR: `docs/decisions/0119-omp-picker-and-markdown-accent.md`.
- Protocol: no change (63). Migration: none.

## 1. Problem

- Markdown headings and bullets are `text` and `text_subtle` since ADR 0117; the user wants them colored again.
- omp 19.2.1 opens its sheets (model switcher, `/agents`, agent hub, sessions, tree, `/copy`, history search, logins, plugins, extensions) as one `picker` node in `layer`.
  Mkrew drew it generically: the model switcher showed one muted line, `/agents` showed only the selected agent's system prompt.

## 2. Evidence

- Snapshot of a live omp 19.2.1 in the smoke sandbox: `layer` holds `picker #layer/0` with `icon: cpu`, `size: md`, `anchor: composer`, `search: list`, `density: snug`, `scopeStyle: rail`, `scopes` (provider marks and counts), `columns` (Int, t/s, Ctx, $/M), `items` (mono ids, marks, facts, chips), `order` with `Recent` and `All` groups, `selected`, `current`, `total`, `level` (Thinking), `actions`.
- `/agents`: `picker` with `title: Agents`, `size: lg`, `preview: side`, scopes `all` / `source:*`, columns, actions configure/toggle/model/new/reload/close; its children are the preview (title, `md`, `kv`, a `section` with the prompt).
- Types: omp `packages/tui/src/tsp.ts` v19.2.1 `TspPickerProps` and its item, scope, column, action, level types.
- Events: omp's own terminal painter: a press on an unselected row sends `select`, on the selected one `activate`; scopes, tabs, strip, level and buttons send `action` with `act` (and `value`) to the picker node; Esc is `close`.
- omp filters and keeps `query`, `cursor`, `order`, `hits`, `selected`; it patches `items` with `itemsAdd` / `itemsDel`.
- Tern's look: `tern.exe` CSS (`.pk-*` rules); sizes in `theme::picker`.
- omp 19.2.1 does not send its theme palette (`t`): `connection.send(verb: TspVerb, ...)` and `TspVerb` is `q|o|f|b|x|r|e`; the sandbox snapshot had none. Tern colors omp's Markdown from its own copy of omp's themes.

## 3. Design

### 3.1 Markdown color

`preview.rs`: headings (Docs and Chat flavors) and list bullets (filled and ring) are `theme.accent`.

### 3.2 Picker

- Core: `Document::set` on a `picker` folds `itemsAdd` (upsert by id) and `itemsDel` into `items` and does not store them.
- Model (`mkrew_ui::tsp_picker`, pure, tested): `Chrome::of(props)` reads every prop but the items; `rows(props)` resolves `order` (ids and group headers) to item indexes; `label_runs` (hits as UTF-16 ranges, mono paths), `fact_text`, `count_label`, `key_label`, `initials`, `mark_color`, `item_event`, `action_event`, `level_position`.
- View (`shell/tsp_view/picker.rs`): `PickerList` per picker node in `TspUi` keeps the chrome, the rows and a `gpui::ListState`, read again only when the node's revision changes; a new row count resets the list (scrolled to the selection), the same count in another order measures again, a new selection scrolls into view.
  The sheet is a `floating_panel`; parts and sizes as in SPEC §11 and ADR 0119.
- `tsp_node` draws `picker`; `tsp::icon` maps the icon names omp uses in its sheets.

### 3.3 Not done (recorded in ADR 0119)

- No animations (selection bar slide, caret blink, level heat), per ADR 0086.
- Fact columns are not hidden by `priority` on narrow sheets.
- `elapsed` facts do not count up.
- omp's own Markdown colors (needs omp's settings and a copy of its themes).

## 4. Tests

- Core: item patches fold into `items` and are not kept; another kind keeps the props.
- UI model: rows with order and groups, selection index, chrome of the model switcher, `query: null`, a bad prop falls back, mono label runs and hits (UTF-16), facts, count, events, keys and marks.
- UI view: fit height of a short and a long `md` sheet.
- Manual: HUMAN_CHECKS "2026-11-26 omp picker", screenshots in the smoke sandbox with a real omp.
