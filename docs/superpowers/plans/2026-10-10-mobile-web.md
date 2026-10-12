# Mkrew web (phone control) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From an iPhone (or any browser on the user's tailnet), see which Mkrew sessions need the user, read and answer a session's terminal, and start a new agent with a prompt, while the PC at home does the work.

**Architecture:** A new CLI command `mk web` serves a small embedded web page and relays each WebSocket to its own async connection on the daemon's local socket. The browser speaks the unchanged SPEC §7 JSON; `mk web` greets the daemon, refuses `hello` and `session.resize`, adds `ansi` (the `mk attach` painter) next to screen snapshots, and forwards terminal output as binary messages. `tailscale serve` publishes `127.0.0.1:8787` as HTTPS inside the tailnet; a token in `<config>/web_token` guards `/check` and `/ws`.

**Tech Stack:** Rust 2024, tokio 1, `tokio-tungstenite` 0.30 (new), `getrandom` 0.3 (already in the tree), `interprocess` 2.4.4 tokio; vanilla HTML/CSS/ES modules, `@xterm/xterm` 6.0.0 vendored; `node --test` for the pure JS helpers.

**Spec:** `docs/superpowers/specs/2026-10-10-mobile-web-design.md`. UI mockups (user-approved): `docs/design/mobile-web/*.html` (open them in a browser; dark theme shown, light tokens are in each file's `.mk--light`).

## Global Constraints

- Work only in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/mobile-web` (branch `feat/mobile-web`).
- Disk is nearly full: never create a second `target/`. Run every cargo command with `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` and `-j 4`.
- Shell: run `cargo`, `git`, `node`, `npm`, `tar` directly; Git Bash scripts with `sh`; in the omp shell `bash` is WSL — never use it.
- The daemon is unchanged: no new request/event, `PROTOCOL_VERSION` stays as it is, no migration.
- No test calls a real agent or the network (`mk web` binds `127.0.0.1:0`; tests pass `--host test.invalid` so `tailscale` is never run).
- No `unwrap`/`expect` outside tests (SPEC §17); `anyhow` with context in the binary.
- ADR number: run `ls docs/decisions` in Task 3 and take the next free number ≥ 0046 (0041–0045 are taken or reserved by other plans). It is written `NNNN` below; replace every `NNNN` in this plan's changes (code comments, ADR, SPEC, README, HUMAN_CHECKS) with that number.
- UI copy, sizes and colors come from the mockups; no all-caps; status always icon + text (SPEC §11, §14).
- The web client never sends `session.resize`.
- After the CLI changes, rebuild the binaries the CLI tests run: `cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4` before `cargo test -p mkrew-cli`.
- Commits: after each task, commit only that task's files (`git add <paths>`, never `-A` or `.`). Conventional Commits. Never push.

## Review Focus

1. **A phone opens a busy agent session.** Expected: the desktop pane keeps its size; the phone shrinks the font instead. Pinned by Task 3 `web_serves_the_page_and_relays_a_session` (`session.resize` from the browser → `invalid_request`) and Task 1 `fitFontSize` tests.
2. **Someone in the tailnet (or a local process) without the token.** Expected: `/check` and `/ws` answer 401 and no daemon connection opens; static files carry no secrets. Pinned by Task 3 `routes_check_the_token` and the 401 asserts in the integration test.
3. **The daemon stops or restarts while the page is open.** Expected: the page shows "Can't reach Mkrew on <machine>. Retrying…" and recovers; `mk web` closes the socket with a reason instead of hanging. Pinned by Task 3 `web_says_why_when_the_daemon_is_gone`; the visual part is a HUMAN_CHECKS line.
4. **A huge output burst over a slow phone link.** Expected: memory stays bounded; the daemon sends `session.snapshot` and the page repaints from its `ansi` without duplicating history. Pinned by Task 3 `snapshot_event_is_painted_without_history`.
5. **A garbled or unknown message from the browser.** Expected: an `invalid_request` reply with the same id, and the connection stays usable. Pinned by Task 3 `refuses_hello_resize_and_unknown_methods` and the `no.such` step of the integration test.

---

### Task 1: Pure web-client helpers (`model.mjs`)

**Files:**
- Create: `crates/mkrew-cli/web/model.mjs`
- Test: `crates/mkrew-cli/web/model.test.mjs`

**Interfaces:**
- Produces (ES module exports, used by Task 2's `app.mjs`):
  - `statusLabel(state: string) -> {mark, cls, label, pill}`
  - `WORKSPACE_COLORS: string[]`, `wsClass(color?: string) -> string`
  - `tokenFromHash(hash: string) -> string|null`, `tokenFromText(text: string) -> string|null`
  - `machineName(hostname: string) -> string`
  - `displayName(session, profiles) -> string`, `chipFor(session, profiles) -> {text, shell}`
  - `buildList(projects, sessions) -> {needs: Session[], groups: {project, sessions}[]}`
  - `ago(ms, now) -> string`, `metaText(session, now) -> string`, `pillText(session, now) -> string`
  - `keyBytes(key: 'enter'|'esc'|'up'|'down'|'tab'|'y'|'n'|'ctrl_c', appCursor: boolean) -> string`
  - `backoff(attempt: number) -> seconds`, `fitFontSize(width, cols) -> px`
  - `taskTitle(prompt) -> string`, `newAgentPlan(profile, prompt) -> {task: {title, prompt}|null, sendAfter: string|null}`
  - `esc(text) -> string`, `toBase64(text) -> string`
  - Session/Project/AgentProfileInfo objects are the SPEC §7 JSON shapes (`crates/mkrew-protocol/src/message.rs` `Session`, `Project`, `AgentProfileInfo`).

- [ ] **Step 1: Write the failing tests**

`crates/mkrew-cli/web/model.test.mjs`:

```js
// Run: node --test crates/mkrew-cli/web/model.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  ago, backoff, buildList, chipFor, displayName, esc, fitFontSize, keyBytes, machineName,
  metaText, newAgentPlan, pillText, statusLabel, taskTitle, toBase64, tokenFromHash,
  tokenFromText, wsClass,
} from './model.mjs';

const T = 'ab'.repeat(32);
const profiles = [
  { id: 'claude', name: 'Claude Code', kind: 'agent', icon: '✻', installed: true },
  { id: 'pwsh', name: 'PowerShell', kind: 'shell', icon: null, installed: true },
];
const project = (id, name, extra = {}) => ({ id, name, archived: false, pinned: false, color: null, ...extra });
const session = (id, projectId, extra = {}) => ({
  id, project_id: projectId, agent_profile: 'claude', kind: 'agent', title: null,
  state: 'working', state_changed_at: 0, started_at: 0, pinned: false, ...extra,
});

test('status labels cover every state and fall back to unknown', () => {
  assert.deepEqual(statusLabel('needs_input'), { mark: '?', cls: 'st-needs', label: 'Needs input', pill: 'needs' });
  assert.equal(statusLabel('done').mark, '✓');
  assert.equal(statusLabel('exited').pill, 'exited');
  assert.equal(statusLabel('something-new').label, 'Unknown');
});

test('workspace colors map to classes; unknown names get the neutral one', () => {
  assert.equal(wsClass('teal'), 'ws-teal');
  assert.equal(wsClass(null), 'ws-none');
  assert.equal(wsClass('mauve'), 'ws-none');
});

test('the token comes from #t= only when it is 64 lowercase hex characters', () => {
  assert.equal(tokenFromHash(`#t=${T}`), T);
  assert.equal(tokenFromHash(`#x=1&t=${T}`), T);
  assert.equal(tokenFromHash(`#t=${T}0`), null);
  assert.equal(tokenFromHash('#t=abc'), null);
  assert.equal(tokenFromHash(''), null);
});

test('a pasted token or a whole pasted link both work', () => {
  assert.equal(tokenFromText(`  ${T.toUpperCase()} `), T);
  assert.equal(tokenFromText(`https://pc.tail1.ts.net/#t=${T}`), T);
  assert.equal(tokenFromText(`${T}0`), null);
  assert.equal(tokenFromText('nope'), null);
});

test('the machine name is the first DNS label, IPs stay whole', () => {
  assert.equal(machineName('studio-pc.tail1234.ts.net'), 'studio-pc');
  assert.equal(machineName('127.0.0.1'), '127.0.0.1');
  assert.equal(machineName('::1'), '::1');
  assert.equal(machineName(''), 'your PC');
});

test('names follow user title > terminal title > profile name > id', () => {
  assert.equal(displayName(session('s', 'p', { title: 'Mine', terminal_title: 'Term' }), profiles), 'Mine');
  assert.equal(displayName(session('s', 'p', { terminal_title: 'Term' }), profiles), 'Term');
  assert.equal(displayName(session('s', 'p'), profiles), 'Claude Code');
  assert.equal(displayName(session('s', 'p', { agent_profile: 'gone' }), profiles), 'gone');
});

test('chips: shells show >_, agents their icon, else the first letter', () => {
  assert.deepEqual(chipFor(session('s', 'p', { kind: 'shell', agent_profile: 'pwsh' }), profiles), { text: '>_', shell: true });
  assert.deepEqual(chipFor(session('s', 'p'), profiles), { text: '✻', shell: false });
  assert.deepEqual(chipFor(session('s', 'p', { agent_profile: 'codex' }), profiles), { text: 'C', shell: false });
});

test('buildList: needs-you first (newest first), not repeated in groups; archived and unknown workspaces hidden', () => {
  const projects = [project('a', 'zeta'), project('b', 'alpha'), project('c', 'old', { archived: true }), project('d', 'pinned', { pinned: true })];
  const sessions = [
    session('1', 'a', { state: 'needs_input', state_changed_at: 10 }),
    session('2', 'b', { state: 'done', state_changed_at: 20 }),
    session('3', 'a', { started_at: 5 }),
    session('4', 'a', { started_at: 1 }),
    session('5', 'a', { started_at: 9, pinned: true }),
    session('6', 'c'),
    session('7', 'zz'),
    session('8', 'd'),
  ];
  const { needs, groups } = buildList(projects, sessions);
  assert.deepEqual(needs.map((s) => s.id), ['2', '1']);
  assert.deepEqual(groups.map((g) => g.project.name), ['pinned', 'zeta']);
  assert.deepEqual(groups[1].sessions.map((s) => s.id), ['5', '4', '3']);
});

test('times: now / minutes / hours / days', () => {
  assert.equal(ago(0, 59_000), 'now');
  assert.equal(ago(0, 17 * 60_000), '17m');
  assert.equal(ago(0, 3 * 3_600_000), '3h');
  assert.equal(ago(0, 49 * 3_600_000), '2d');
  assert.equal(ago(10, 0), 'now');
});

test('row meta and pill: time for working/idle, the label otherwise', () => {
  const now = 12 * 60_000;
  assert.equal(metaText(session('s', 'p', { state: 'working' }), now), '12m');
  assert.equal(metaText(session('s', 'p', { state: 'starting' }), now), 'Starting');
  assert.equal(pillText(session('s', 'p', { state: 'idle' }), now), 'Idle · 12m');
  assert.equal(pillText(session('s', 'p', { state: 'needs_input' }), now), 'Needs input');
});

test('quick keys send the bytes a terminal would, arrows follow application cursor mode', () => {
  assert.equal(keyBytes('enter', false), '\r');
  assert.equal(keyBytes('esc', false), '\x1b');
  assert.equal(keyBytes('ctrl_c', false), '\x03');
  assert.equal(keyBytes('up', false), '\x1b[A');
  assert.equal(keyBytes('down', true), '\x1bOB');
  assert.throws(() => keyBytes('f13', false));
});

test('reconnect waits 1, 2, 4, 8, then 10 seconds', () => {
  assert.deepEqual([0, 1, 2, 3, 4, 50].map(backoff), [1, 2, 4, 8, 10, 10]);
});

test('the terminal font shrinks to fit the width and stays between 4 and 14 px', () => {
  assert.equal(fitFontSize(390, 120), 5.4);
  assert.equal(fitFontSize(2000, 80), 14);
  assert.equal(fitFontSize(100, 300), 4);
  assert.equal(fitFontSize(0, 120), 8);
});

test('new agent: agent + prompt = task; shell prompt = send after spawn; no prompt = plain spawn', () => {
  const agent = profiles[0];
  const shell = profiles[1];
  assert.deepEqual(newAgentPlan(agent, '  Fix the login bug\nand add a test '), {
    task: { title: 'Fix the login bug', prompt: 'Fix the login bug\nand add a test' },
    sendAfter: null,
  });
  assert.deepEqual(newAgentPlan(agent, '   '), { task: null, sendAfter: null });
  assert.deepEqual(newAgentPlan(shell, 'cargo test'), { task: null, sendAfter: 'cargo test' });
});

test('task titles are the first line, cut at 60 characters', () => {
  assert.equal(taskTitle('x'.repeat(80)), `${'x'.repeat(59)}…`);
  assert.equal(taskTitle('one\ntwo'), 'one');
});

test('escaping and base64 of UTF-8', () => {
  assert.equal(esc(`<a href="x">'&'</a>`), '&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;');
  assert.equal(toBase64('é\r'), 'w6kN');
});
```

- [ ] **Step 2: Run the tests to make sure they fail**

Run: `node --test crates/mkrew-cli/web/model.test.mjs`
Expected: FAIL with `Cannot find module …/model.mjs` (ERR_MODULE_NOT_FOUND).

- [ ] **Step 3: Write `model.mjs`**

`crates/mkrew-cli/web/model.mjs`:

```js
// Pure helpers of the Mkrew web client (ADR NNNN). No DOM; tested with `node --test`.

const STATES = {
  starting: { mark: '◌', cls: 'st-idle', label: 'Starting', pill: 'idle' },
  working: { mark: '•••', cls: 'st-working', label: 'Working', pill: 'working' },
  needs_input: { mark: '?', cls: 'st-needs', label: 'Needs input', pill: 'needs' },
  idle: { mark: '○', cls: 'st-idle', label: 'Idle', pill: 'idle' },
  done: { mark: '✓', cls: 'st-done', label: 'Done', pill: 'done' },
  exited: { mark: '✕', cls: 'st-exited', label: 'Exited', pill: 'exited' },
  unknown: { mark: '·', cls: 'st-idle', label: 'Unknown', pill: 'idle' },
};

/** Mark, CSS class, words and pill style of a session state (SPEC §11). */
export function statusLabel(state) {
  return STATES[state] ?? STATES.unknown;
}

/** SPEC §14 workspace color names (`mkrew_protocol::WORKSPACE_COLORS`). */
export const WORKSPACE_COLORS = ['red', 'orange', 'yellow', 'lime', 'green', 'teal', 'blue', 'purple', 'pink'];

export function wsClass(color) {
  return WORKSPACE_COLORS.includes(color) ? `ws-${color}` : 'ws-none';
}

/** The access token from the URL fragment (`#t=<64 hex>`), as `mk web` prints it. */
export function tokenFromHash(hash) {
  const match = /(?:^#|&)t=([0-9a-f]{64})(?:&|$)/.exec(hash ?? '');
  return match ? match[1] : null;
}

/** The token from pasted text: the bare token or the whole link. */
export function tokenFromText(text) {
  const match = /(?<![0-9a-fA-F])[0-9a-fA-F]{64}(?![0-9a-fA-F])/.exec(text ?? '');
  return match ? match[0].toLowerCase() : null;
}

/** "studio-pc" from "studio-pc.tail1234.ts.net"; addresses stay whole. */
export function machineName(hostname) {
  if (!hostname) return 'your PC';
  if (/^[\d.]+$/.test(hostname) || hostname.includes(':')) return hostname;
  return hostname.split('.')[0];
}

/** User title > terminal title > profile name > profile id (SPEC §11). */
export function displayName(session, profiles) {
  return (
    session.title ||
    session.terminal_title ||
    profiles.find((p) => p.id === session.agent_profile)?.name ||
    session.agent_profile
  );
}

/** The agent chip: `>_` for shells, else the profile badge or the id's first letter. */
export function chipFor(session, profiles) {
  if (session.kind === 'shell') return { text: '>_', shell: true };
  const icon = profiles.find((p) => p.id === session.agent_profile)?.icon;
  return { text: icon || session.agent_profile.slice(0, 1).toUpperCase(), shell: false };
}

const NEEDS = new Set(['needs_input', 'done']);

/**
 * "Needs you" (needs input or done, newest change first) and one group per live workspace
 * (pinned first, then by name) holding its other sessions (pinned first, then oldest first).
 * Sessions of archived or unknown workspaces are left out.
 */
export function buildList(projects, sessions) {
  const live = new Map(projects.filter((p) => !p.archived).map((p) => [p.id, p]));
  const shown = sessions.filter((s) => live.has(s.project_id));
  const needs = shown
    .filter((s) => NEEDS.has(s.state))
    .sort((a, b) => b.state_changed_at - a.state_changed_at);
  const rest = shown.filter((s) => !NEEDS.has(s.state));
  const groups = [...live.values()]
    .map((project) => ({
      project,
      sessions: rest.filter((s) => s.project_id === project.id).sort(bySession),
    }))
    .filter((group) => group.sessions.length > 0)
    .sort(
      (a, b) =>
        Number(Boolean(b.project.pinned)) - Number(Boolean(a.project.pinned)) ||
        a.project.name.localeCompare(b.project.name),
    );
  return { needs, groups };
}

function bySession(a, b) {
  return Number(Boolean(b.pinned)) - Number(Boolean(a.pinned)) || a.started_at - b.started_at;
}

/** "now", "17m", "3h", "2d" since `ms` (unix ms). */
export function ago(ms, now) {
  const seconds = Math.max(0, Math.floor((now - ms) / 1000));
  if (seconds < 60) return 'now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function timed(session) {
  return session.state === 'working' || session.state === 'idle';
}

/** Right column of a row: the time for working/idle, else the state in words. */
export function metaText(session, now) {
  return timed(session) ? ago(session.state_changed_at, now) : statusLabel(session.state).label;
}

/** The session screen's pill: "Working · 12m", "Needs input", … */
export function pillText(session, now) {
  const { label } = statusLabel(session.state);
  return timed(session) ? `${label} · ${ago(session.state_changed_at, now)}` : label;
}

const KEYS = { enter: '\r', esc: '\x1b', tab: '\t', y: 'y', n: 'n', ctrl_c: '\x03' };

/** Bytes of a quick key; arrows follow the terminal's application cursor mode. */
export function keyBytes(key, appCursor) {
  if (key === 'up' || key === 'down') {
    return `\x1b${appCursor ? 'O' : '['}${key === 'up' ? 'A' : 'B'}`;
  }
  const bytes = KEYS[key];
  if (bytes === undefined) throw new Error(`unknown key ${key}`);
  return bytes;
}

/** Seconds before reconnect attempt `attempt` (0-based). */
export function backoff(attempt) {
  return [1, 2, 4, 8][attempt] ?? 10;
}

/** Font size (px, one decimal) that fits `cols` monospace columns into `width` px. */
export function fitFontSize(width, cols) {
  if (!(width > 0) || !(cols > 0)) return 8;
  return Math.min(14, Math.max(4, Math.floor((width / (cols * 0.6)) * 10) / 10));
}

/** The task card title for a prompt: its first line, at most 60 characters. */
export function taskTitle(prompt) {
  const line = prompt.trim().split('\n', 1)[0].trim();
  return line.length > 60 ? `${line.slice(0, 59)}…` : line;
}

/**
 * How "Start" runs: an agent with a prompt becomes a task (`task.create` + `task.run`, so the
 * profile's `prompt_mode` delivers it); otherwise `session.spawn`, and a shell gets the prompt
 * typed after it starts.
 */
export function newAgentPlan(profile, prompt) {
  const text = prompt.trim();
  if (profile.kind === 'agent' && text) {
    return { task: { title: taskTitle(text), prompt: text }, sendAfter: null };
  }
  return { task: null, sendAfter: profile.kind === 'shell' && text ? text : null };
}

const ENTITIES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

export function esc(text) {
  return String(text).replace(/[&<>"']/g, (c) => ENTITIES[c]);
}

/** Base64 of the UTF-8 bytes of `text` (`session.input` data). */
export function toBase64(text) {
  let binary = '';
  for (const byte of new TextEncoder().encode(text)) binary += String.fromCharCode(byte);
  return btoa(binary);
}
```

- [ ] **Step 4: Run the tests to make sure they pass**

Run: `node --test crates/mkrew-cli/web/model.test.mjs`
Expected: PASS, 16 tests, 0 failures.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli/web/model.mjs crates/mkrew-cli/web/model.test.mjs
git commit -m "feat(web): pure helpers of the phone client (list, status, keys, tokens)"
```

---

### Task 2: The page (`index.html`, `app.css`, `app.mjs`, vendored xterm.js)

Read the mockups first: `docs/design/mobile-web/sessions.html`, `sessions-loading.html`, `sessions-empty.html`, `sessions-disconnected.html`, `session-needs-input.html`, `session-ended.html`, `new-agent.html`, `token.html`. The page below reproduces them; the mockups' fixed `52px` top padding (a fake status bar) becomes `env(safe-area-inset-top)`.

**Files:**
- Create: `crates/mkrew-cli/web/index.html`
- Create: `crates/mkrew-cli/web/app.css`
- Create: `crates/mkrew-cli/web/app.mjs`
- Create: `crates/mkrew-cli/web/vendor/xterm.mjs`, `crates/mkrew-cli/web/vendor/xterm.css`, `crates/mkrew-cli/web/vendor/LICENSE-xterm` (from `@xterm/xterm@6.0.0`)

**Interfaces:**
- Consumes: every export of `model.mjs` listed in Task 1.
- Consumes (from Task 3's server, same origin): `GET /check?t=<token>` → 204 / 401; `WebSocket /ws?t=<token>`; text messages = SPEC §7 JSON (`{"type":"request","id":N,"method":…,"params":…}` out; `response`/`event` in); a response or `session.snapshot` event with a `snapshot` also carries `ansi` (string); binary messages = 26 ASCII bytes of session id + raw PTY bytes.
- Produces: the asset paths Task 3 embeds: `/` (`index.html`), `/app.css`, `/app.mjs`, `/model.mjs`, `/vendor/xterm.mjs`, `/vendor/xterm.css`. `index.html` contains `<title>Mkrew</title>`.

- [ ] **Step 1: Vendor xterm.js 6.0.0**

```bash
mkdir -p crates/mkrew-cli/web/vendor "$TEMP/xterm-pack"
npm pack @xterm/xterm@6.0.0 --pack-destination "$TEMP/xterm-pack" --silent
tar -xzf "$TEMP/xterm-pack/xterm-xterm-6.0.0.tgz" -C "$TEMP/xterm-pack"
cp "$TEMP/xterm-pack/package/lib/xterm.mjs" crates/mkrew-cli/web/vendor/xterm.mjs
cp "$TEMP/xterm-pack/package/css/xterm.css" crates/mkrew-cli/web/vendor/xterm.css
cp "$TEMP/xterm-pack/package/LICENSE" crates/mkrew-cli/web/vendor/LICENSE-xterm
grep -o 'export{[^}]*}' crates/mkrew-cli/web/vendor/xterm.mjs
```

Expected: the last command prints `export{Dl as Terminal}` (the bundle exports only `Terminal`). Then remove the source-map comment so browsers do not request a missing file: delete the trailing `//# sourceMappingURL=xterm.mjs.map` line from `vendor/xterm.mjs` with the `edit` tool (it is the file's last line).

- [ ] **Step 2: `index.html`**

`crates/mkrew-cli/web/index.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="color-scheme" content="dark light">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<meta name="apple-mobile-web-app-title" content="Mkrew">
<meta name="referrer" content="no-referrer">
<title>Mkrew</title>
<link rel="stylesheet" href="vendor/xterm.css">
<link rel="stylesheet" href="app.css">
<script type="module" src="app.mjs"></script>
</head>
<body>
<div class="mk" id="app">

<section class="screen screen-token" id="screen-token" hidden>
  <form class="token-form" id="token-form" novalidate>
    <h1 class="token-brand">Mkrew</h1>
    <p class="token-lead">Paste the access token from Mkrew on your PC to connect.</p>
    <div class="field">
      <label class="field-label" for="token-input">Token</label>
      <input class="input input--mono" id="token-input" type="password" placeholder="Paste token" autocomplete="off" autocapitalize="off" spellcheck="false" aria-describedby="token-error">
      <p class="field-error" id="token-error" role="alert"></p>
    </div>
    <button class="btn btn--primary btn--block" type="submit">Connect</button>
  </form>
</section>

<section class="screen" id="screen-list" hidden>
  <header class="header">
    <h1 class="brand">Mkrew</h1>
    <span class="conn"><span class="conn-dot" id="conn-dot" aria-hidden="true"></span><span id="conn-label"></span></span>
    <span class="spacer"></span>
    <button class="btn btn--primary btn--compact" id="new-agent" type="button"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" aria-hidden="true"><path d="M8 3v10M3 8h10"/></svg>New agent</button>
  </header>
  <main class="main">
    <div class="banner" id="banner" role="status" hidden><span class="spin" aria-hidden="true"></span><span id="banner-text"></span></div>
    <div id="loading" aria-busy="true" aria-label="Loading sessions">
      <div class="skel skel-head"></div>
      <div class="skel-row"><span class="skel skel-dot"></span><span class="skel skel-chip"></span><span class="skel"></span><span class="skel"></span></div>
      <div class="skel-row"><span class="skel skel-dot"></span><span class="skel skel-chip"></span><span class="skel"></span><span class="skel"></span></div>
      <div class="skel-row"><span class="skel skel-dot"></span><span class="skel skel-chip"></span><span class="skel"></span><span class="skel"></span></div>
      <div class="skel skel-head"></div>
      <div class="skel-row"><span class="skel skel-dot"></span><span class="skel skel-chip"></span><span class="skel"></span><span class="skel"></span></div>
      <div class="skel-row"><span class="skel skel-dot"></span><span class="skel skel-chip"></span><span class="skel"></span><span class="skel"></span></div>
    </div>
    <div class="empty" id="empty" hidden>
      <div class="mark" aria-hidden="true">&gt;_</div>
      <p class="empty-text">No sessions yet.</p>
      <button class="btn" id="empty-new" type="button"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" aria-hidden="true"><path d="M8 3v10M3 8h10"/></svg>New agent</button>
    </div>
    <div id="groups"></div>
  </main>
</section>

<section class="screen" id="screen-session" hidden>
  <header class="topbar">
    <button class="icon-btn" id="back" type="button" aria-label="Back to sessions"><svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12.5 4.5 7 10l5.5 5.5"/></svg></button>
    <span class="chip" id="s-chip" aria-hidden="true"></span>
    <h1 class="s-title" id="s-title"></h1>
    <span class="pill" id="s-pill"></span>
  </header>
  <div class="term" id="term" role="region" aria-label="Terminal output"></div>
  <form class="composer" id="composer">
    <div class="keys" id="keys" role="group" aria-label="Quick keys">
      <button class="key" type="button" data-key="enter">Enter</button>
      <button class="key" type="button" data-key="esc" aria-label="Escape">Esc</button>
      <button class="key" type="button" data-key="up" aria-label="Arrow up">↑</button>
      <button class="key" type="button" data-key="down" aria-label="Arrow down">↓</button>
      <button class="key" type="button" data-key="tab">Tab</button>
      <button class="key" type="button" data-key="y">y</button>
      <button class="key" type="button" data-key="n">n</button>
      <button class="key" type="button" data-key="ctrl_c" aria-label="Control C">Ctrl+C</button>
    </div>
    <div class="reply">
      <label class="vh" for="reply">Reply to session</label>
      <textarea class="input reply-input" id="reply" rows="1" placeholder="Reply…"></textarea>
      <button class="send" id="send" type="submit" aria-label="Send"><svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14.5v-11M4.5 8 9 3.5 13.5 8"/></svg></button>
    </div>
  </form>
  <div class="ended" id="ended" role="status" hidden><span class="x" aria-hidden="true">✕</span><span id="ended-text"></span></div>
</section>

<div class="scrim" id="sheet" hidden>
  <form class="sheet" id="new-form" role="dialog" aria-modal="true" aria-labelledby="na-title">
    <div class="grab" aria-hidden="true"></div>
    <div class="sheet-head">
      <h2 class="sheet-title" id="na-title">New agent</h2>
      <button class="icon-btn" id="sheet-close" type="button" aria-label="Close"><svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" aria-hidden="true"><path d="M5 5l10 10M15 5 5 15"/></svg></button>
    </div>
    <div class="sheet-body">
      <fieldset class="fs"><legend class="legend">Workspace</legend><div class="opts" id="na-workspaces"></div></fieldset>
      <fieldset class="fs"><legend class="legend">Agent</legend><div class="opts opts--grid" id="na-agents"></div></fieldset>
      <div class="field">
        <label class="legend" for="na-prompt">Prompt</label>
        <textarea class="input ta" id="na-prompt" rows="4" placeholder="What should the agent do?"></textarea>
      </div>
    </div>
    <div class="sheet-foot"><button class="btn btn--primary btn--block" id="na-start" type="submit">Start</button></div>
  </form>
</div>

<div class="toast" id="toast" role="status" hidden></div>
</div>
</body>
</html>
```

- [ ] **Step 3: `app.css`**

`crates/mkrew-cli/web/app.css` (tokens = SPEC §14 / mockups; light via `prefers-color-scheme`):

```css
:root{--bg:#17181C;--surface:#1F2127;--surface-raised:#2C2E37;--hover:#22242B;--selected:#262938;--border:#2A2C33;--divider:#24262D;--text:#E6E7EB;--text-dim:#B4B7C0;--text-muted:#9A9CA6;--subtle:#5D616C;--accent:#7C8CFF;--accent-text:#9A9FFF;--accent-bg:#25264A;--on-accent:#17181C;--warning-bg:#3A3020;--success-bg:#1D3A31;--danger-bg:#3A2327;--s-working:#7C8CFF;--s-needs:#E3A63E;--s-done:#4CB782;--s-exited:#E5676B;--s-idle:#6E7079;--scrim:rgba(0,0,0,.55);--ws-red:#F87171;--ws-orange:#FB923C;--ws-yellow:#FACC15;--ws-lime:#A3E635;--ws-green:#4ADE80;--ws-teal:#2DD4BF;--ws-blue:#60A5FA;--ws-purple:#C084FC;--ws-pink:#F472B6;--mono:"SF Mono",Menlo,"Cascadia Mono",Consolas,monospace;color-scheme:dark}
@media (prefers-color-scheme:light){:root{--bg:#FFFFFF;--surface:#F6F7F9;--surface-raised:#EEF0F3;--hover:#EEF0F3;--selected:#E9EAFF;--border:#E3E5EA;--divider:#ECEEF1;--text:#1B1D22;--text-dim:#474B55;--text-muted:#5F6473;--subtle:#A0A4AF;--accent:#4F5BD5;--accent-text:#4A52D9;--accent-bg:#ECEEFF;--on-accent:#FFFFFF;--warning-bg:#FDF3E1;--success-bg:#E1F5EC;--danger-bg:#FDECEC;--s-working:#4F5BD5;--s-needs:#B7791F;--s-done:#2F855A;--s-exited:#C53030;--s-idle:#9A9CA6;--scrim:rgba(17,18,22,.32);--ws-red:#DC2626;--ws-orange:#D9560B;--ws-yellow:#A16207;--ws-lime:#4D7C0F;--ws-green:#15803D;--ws-teal:#0F766E;--ws-blue:#2563EB;--ws-purple:#9333EA;--ws-pink:#DB2777;color-scheme:light}}
*{box-sizing:border-box}
html,body{margin:0;height:100%;background:var(--bg)}
body{overscroll-behavior:none}
[hidden]{display:none!important}
button{font:inherit;color:inherit;cursor:pointer}
button:disabled{cursor:not-allowed;opacity:.5}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.mk{height:100dvh;max-width:900px;margin:0 auto;position:relative;overflow:hidden;display:flex;flex-direction:column;background:var(--bg);color:var(--text);font:400 15px/1.45 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;-webkit-font-smoothing:antialiased}
.screen{flex:1;min-height:0;display:flex;flex-direction:column}
.vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}

/* Sessions */
.header{display:flex;align-items:center;gap:12px;min-height:56px;padding:calc(env(safe-area-inset-top) + 8px) 16px 8px;border-bottom:1px solid var(--divider)}
.brand{margin:0;font-size:17px;font-weight:600;letter-spacing:-.01em}
.conn{display:inline-flex;align-items:center;gap:6px;font-size:12px;color:var(--text-muted)}
.conn-dot{width:8px;height:8px;border-radius:50%;background:var(--s-done)}
.conn-dot--off{background:transparent;box-shadow:inset 0 0 0 1.5px var(--s-needs);animation:mkpulse 1.6s ease-in-out infinite}
.spacer{flex:1}
.btn{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:44px;padding:0 16px;border-radius:8px;border:1px solid var(--border);background:var(--surface);color:var(--text);font-size:15px;font-weight:500;line-height:1;white-space:nowrap}
.btn--primary{background:var(--accent);border-color:transparent;color:var(--on-accent)}
.btn--compact{min-height:36px;padding:0 12px;font-size:14px}
.btn--block{width:100%}
.main{flex:1;min-height:0;overflow-y:auto;-webkit-overflow-scrolling:touch;padding:8px 16px calc(env(safe-area-inset-bottom) + 32px)}
.banner{display:flex;align-items:center;gap:8px;margin:12px 0 0;padding:10px 12px;border-radius:8px;background:var(--warning-bg);font-size:14px;color:var(--text)}
.spin{width:14px;height:14px;flex:none;border-radius:50%;border:1.5px solid var(--s-needs);border-right-color:transparent;animation:mkspin 1.2s linear infinite}
.group{margin-top:24px}
.group--first{margin-top:16px}
.group-head{display:flex;align-items:center;gap:8px;margin:0 0 4px;padding:0 8px;font-size:13px;font-weight:600;color:var(--text-dim)}
.count{font-weight:400;font-size:12px;color:var(--text-muted)}
.list{list-style:none;margin:0;padding:0}
.needs{background:var(--surface);border:1px solid var(--border);border-radius:8px;padding:4px}
.row{display:grid;grid-template-columns:16px 24px minmax(0,1fr) auto;align-items:center;gap:12px;width:100%;min-height:48px;padding:0 8px;border:0;border-radius:8px;background:none;text-align:left}
.row:hover{background:var(--hover)}
.title{font-size:15px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.title--muted{color:var(--text-muted)}
.meta{display:inline-flex;align-items:center;gap:6px;font-size:12px;color:var(--text-muted);white-space:nowrap}
.chip{width:24px;height:24px;flex:none;display:inline-grid;place-items:center;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--text-dim);font:500 13px/1 var(--mono)}
.chip--shell{font-size:10px;letter-spacing:-.5px}
.ws{width:18px;height:18px;flex:none;display:inline-grid;place-items:center;border-radius:4px;font-size:11px;font-weight:600;line-height:1}
.ws--sm{width:16px;height:16px;font-size:10px}
.ws-red{color:var(--ws-red);background:color-mix(in srgb,var(--ws-red) 16%,transparent)}
.ws-orange{color:var(--ws-orange);background:color-mix(in srgb,var(--ws-orange) 16%,transparent)}
.ws-yellow{color:var(--ws-yellow);background:color-mix(in srgb,var(--ws-yellow) 16%,transparent)}
.ws-lime{color:var(--ws-lime);background:color-mix(in srgb,var(--ws-lime) 16%,transparent)}
.ws-green{color:var(--ws-green);background:color-mix(in srgb,var(--ws-green) 16%,transparent)}
.ws-teal{color:var(--ws-teal);background:color-mix(in srgb,var(--ws-teal) 16%,transparent)}
.ws-blue{color:var(--ws-blue);background:color-mix(in srgb,var(--ws-blue) 16%,transparent)}
.ws-purple{color:var(--ws-purple);background:color-mix(in srgb,var(--ws-purple) 16%,transparent)}
.ws-pink{color:var(--ws-pink);background:color-mix(in srgb,var(--ws-pink) 16%,transparent)}
.ws-none{color:var(--text-muted);background:var(--surface-raised)}
.st{width:16px;height:16px;display:inline-grid;place-items:center;font-size:13px;font-weight:600;line-height:1}
.st-idle{color:var(--s-idle)}.st-working{color:var(--s-working)}.st-needs{color:var(--s-needs)}.st-done{color:var(--s-done)}.st-exited{color:var(--s-exited)}
.q{width:14px;height:14px;display:grid;place-items:center;border-radius:50%;border:1.5px solid currentColor;font-size:9.5px;font-weight:700}
.dots{display:inline-flex;gap:2px}
.dots i{width:3px;height:3px;border-radius:50%;background:currentColor;animation:mkdot 1.8s ease-in-out infinite}
.dots i:nth-child(2){animation-delay:.3s}.dots i:nth-child(3){animation-delay:.6s}
.skel{height:10px;border-radius:4px;background:var(--surface-raised);animation:mkbreathe 1.6s ease-in-out infinite alternate}
.skel-head{width:96px;margin:24px 8px 12px}
.skel-row{display:grid;grid-template-columns:16px 24px 1fr 32px;gap:12px;align-items:center;min-height:48px;padding:0 8px}
.skel-dot{width:12px;height:12px;border-radius:50%}
.skel-chip{width:24px;height:24px}
.empty{display:flex;flex-direction:column;align-items:center;gap:16px;padding-top:28vh}
.mark{width:48px;height:48px;display:grid;place-items:center;border-radius:12px;border:1px solid var(--border);color:var(--text-muted);font:500 16px/1 var(--mono)}
.empty-text{margin:0;color:var(--text-dim)}

/* Token */
.screen-token{align-items:center;justify-content:center;padding:24px}
.token-form{width:100%;max-width:360px;display:flex;flex-direction:column;gap:16px}
.token-brand{margin:0;font-size:20px;font-weight:600;letter-spacing:-.01em}
.token-lead{margin:0;color:var(--text-dim)}
.field{display:flex;flex-direction:column;gap:6px}
.field-label{font-size:13px;font-weight:500;color:var(--text-dim)}
.field-error{margin:0;min-height:1.2em;font-size:13px;color:var(--s-exited)}
.input{width:100%;min-height:44px;padding:10px 12px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text);font-family:inherit;font-size:16px}
.input--mono{font-family:var(--mono)}
.input::placeholder{color:var(--text-muted)}
.input:focus{outline:none;border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}

/* Session */
.topbar{display:flex;align-items:center;gap:8px;min-height:52px;padding:calc(env(safe-area-inset-top) + 4px) 16px 4px 4px;border-bottom:1px solid var(--divider)}
.icon-btn{width:44px;height:44px;flex:none;display:grid;place-items:center;border:0;border-radius:8px;background:none;color:var(--text-dim)}
.s-title{flex:1;min-width:0;margin:0;font-size:15px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.pill{display:inline-flex;align-items:center;gap:6px;flex:none;height:26px;padding:0 10px 0 8px;border-radius:999px;font-size:12px;font-weight:500;color:var(--text);white-space:nowrap}
.pill--needs{background:var(--warning-bg)}.pill--done{background:var(--success-bg)}.pill--exited{background:var(--danger-bg)}.pill--working{background:var(--accent-bg)}.pill--idle{background:var(--surface)}
.term{flex:1;min-height:0;overflow:auto;background:#111216;padding:8px}
.composer{display:flex;flex-direction:column;gap:8px;padding:8px 16px calc(env(safe-area-inset-bottom) + 8px);border-top:1px solid var(--divider);background:var(--bg)}
.keys{display:flex;gap:6px;overflow-x:auto;margin:0 -16px;padding:0 16px;scrollbar-width:none}
.keys::-webkit-scrollbar{display:none}
.key{flex:none;min-width:44px;min-height:44px;padding:0 12px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text);font:500 14px/1 var(--mono)}
.key:active{background:var(--hover)}
.reply{display:flex;align-items:flex-end;gap:8px}
.reply-input{flex:1;max-height:132px;padding:11px 12px;line-height:1.35;resize:none}
.send{width:44px;height:44px;flex:none;display:grid;place-items:center;border:0;border-radius:8px;background:var(--accent);color:var(--on-accent)}
.ended{display:flex;align-items:center;justify-content:center;gap:8px;min-height:56px;padding:8px 16px calc(env(safe-area-inset-bottom) + 8px);border-top:1px solid var(--divider);background:var(--surface);color:var(--text-dim);font-size:14px}
.x{color:var(--s-exited);font-weight:700}

/* New agent */
.scrim{position:absolute;inset:0;z-index:10;display:flex;align-items:flex-end;background:var(--scrim)}
.sheet{width:100%;max-height:calc(100% - env(safe-area-inset-top) - 24px);display:flex;flex-direction:column;background:var(--bg);border:1px solid var(--border);border-bottom:0;border-radius:12px 12px 0 0;animation:mkrise 160ms ease-out}
.grab{width:36px;height:4px;margin:8px auto 0;border-radius:2px;background:var(--subtle)}
.sheet-head{display:flex;align-items:center;gap:8px;padding:4px 4px 4px 16px}
.sheet-title{flex:1;margin:0;font-size:17px;font-weight:600}
.sheet-body{flex:1;min-height:0;overflow-y:auto;padding:8px 16px 16px;display:flex;flex-direction:column;gap:24px}
.fs{margin:0;padding:0;border:0;min-width:0}
.legend{padding:0 0 8px;font-size:13px;font-weight:600;color:var(--text-dim)}
.opts{display:flex;flex-direction:column;gap:2px}
.opts--grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px}
.opt{position:relative;display:flex;align-items:center;gap:12px;min-height:48px;padding:0 12px;border-radius:8px;border:1px solid transparent;cursor:pointer}
.opts--grid .opt{border-color:var(--border)}
.opt input{position:absolute;opacity:0;pointer-events:none}
.opt:has(input:checked){background:var(--selected);border-color:color-mix(in srgb,var(--accent) 45%,transparent)}
.opt:has(input:focus-visible){outline:2px solid var(--accent);outline-offset:2px}
.opt--off{cursor:not-allowed}
.opt--off .name{color:var(--text-muted)}
.opt--off .chip{opacity:.6}
.opt-text{flex:1;min-width:0;line-height:1.25}
.name{display:block;flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.note{display:block;font-size:12px;color:var(--text-muted)}
.check{display:none;flex:none}
.opt:has(input:checked) .check{display:block}
.ta{min-height:96px;line-height:1.4;resize:vertical}
.sheet-foot{padding:12px 16px calc(env(safe-area-inset-bottom) + 12px);border-top:1px solid var(--divider)}

.toast{position:absolute;left:16px;right:16px;bottom:calc(env(safe-area-inset-bottom) + 112px);z-index:20;padding:10px 12px;border-radius:8px;background:var(--surface-raised);color:var(--text);font-size:14px;box-shadow:0 4px 16px rgba(0,0,0,.25)}

@keyframes mkdot{0%,100%{opacity:.3}40%{opacity:1}}
@keyframes mkpulse{0%,100%{opacity:1}50%{opacity:.35}}
@keyframes mkspin{to{transform:rotate(360deg)}}
@keyframes mkbreathe{from{opacity:.45}to{opacity:1}}
@keyframes mkrise{from{transform:translateY(24px);opacity:0}}
@media (prefers-reduced-motion:reduce){.dots i,.conn-dot--off,.spin,.skel,.sheet{animation:none}.dots i{opacity:1}}
```

- [ ] **Step 4: `app.mjs`**

`crates/mkrew-cli/web/app.mjs`:

```js
// Mkrew web (ADR NNNN): the phone/browser client. Talks SPEC §7 JSON to the daemon through
// `mk web`; never resizes a session (the desktop pane keeps its size, the font shrinks here).
import { Terminal } from './vendor/xterm.mjs';
import {
  backoff, buildList, chipFor, displayName, esc, fitFontSize, keyBytes, machineName, metaText,
  newAgentPlan, pillText, statusLabel, toBase64, tokenFromHash, tokenFromText, wsClass,
} from './model.mjs';

const $ = (id) => document.getElementById(id);
const TOKEN_KEY = 'mkrew.token';
/** Size of sessions started from the web: the desktop default; the phone never resizes. */
const NEW_SIZE = { cols: 120, rows: 30 };
const ID_LEN = 26;
const PIN = '<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" role="img" aria-label="Pinned"><path d="M4.5 1.5h3M5 1.5v3.2L3 6.8h6L7 4.7V1.5M6 6.8v3.7"/></svg>';
const CHECK = '<svg class="check" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="var(--accent-text)" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 4.5"/></svg>';
/** The desktop's dark ANSI palette (`theme.rs`); the terminal stays dark in both themes. */
const TERMINAL_THEME = {
  background: '#111216', foreground: '#D9DBE1', cursor: '#D9DBE1', selectionBackground: '#353B60',
  black: '#25262C', red: '#E5676B', green: '#4CB782', yellow: '#E3A63E', blue: '#7C8CFF',
  magenta: '#C28CF0', cyan: '#5EC4D6', white: '#C9CBD1', brightBlack: '#6E7079',
  brightRed: '#F2A0A4', brightGreen: '#7DD3A0', brightYellow: '#F0C674', brightBlue: '#A3AFFF',
  brightMagenta: '#D8B4FE', brightCyan: '#8BE0EC', brightWhite: '#F5F6F8',
};

const state = {
  token: null,
  ws: null,
  nextId: 1,
  pending: new Map(),
  connected: false,
  everConnected: false,
  attempt: 0,
  retryTimer: 0,
  loaded: false,
  projects: [],
  profiles: [],
  sessions: new Map(),
  open: null,
  reloadTimer: 0,
};
const machine = machineName(location.hostname);
const idDecoder = new TextDecoder();
let term = null;
let toastTimer = 0;

// ---- screens ----

function show(name) {
  $('screen-token').hidden = name !== 'token';
  $('screen-list').hidden = name !== 'list';
  $('screen-session').hidden = name !== 'session';
}

function showToken(error) {
  stopRetry();
  if (state.ws) {
    const ws = state.ws;
    state.ws = null;
    ws.onclose = null;
    ws.close();
  }
  failPending();
  setConnected(false);
  show('token');
  $('token-error').textContent = error;
  $('token-input').value = '';
}

function toast(text) {
  const el = $('toast');
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, 4000);
}

// ---- connection ----

async function connect() {
  stopRetry();
  let res;
  try {
    res = await fetch(`/check?t=${state.token}`, { cache: 'no-store' });
  } catch {
    retry();
    return;
  }
  if (res.status === 401) {
    showToken("That token doesn't work. Run mk web on your PC to see the link again.");
    return;
  }
  if (!res.ok) {
    retry();
    return;
  }
  const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?t=${state.token}`);
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => {
    state.ws = ws;
    onConnected();
  };
  ws.onmessage = (e) => {
    if (typeof e.data === 'string') onJson(JSON.parse(e.data));
    else onOutput(new Uint8Array(e.data));
  };
  ws.onclose = (e) => {
    if (e.reason) console.warn('mk web closed the connection:', e.reason);
    if (state.ws === ws) state.ws = null;
    failPending();
    retry();
  };
}

function retry() {
  setConnected(false);
  clearTimeout(state.retryTimer);
  state.retryTimer = setTimeout(connect, backoff(state.attempt) * 1000);
  state.attempt += 1;
}

function stopRetry() {
  clearTimeout(state.retryTimer);
  state.retryTimer = 0;
}

async function onConnected() {
  state.attempt = 0;
  state.everConnected = true;
  setConnected(true);
  try {
    await load();
    if (state.open) await attach(state.open);
  } catch (err) {
    toast(err.message);
  }
}

function setConnected(on) {
  state.connected = on;
  render();
}

function call(method, params) {
  const ws = state.ws;
  if (!ws) return Promise.reject(new Error(`Not connected to ${machine}.`));
  const id = state.nextId++;
  const message = params === undefined ? { type: 'request', id, method } : { type: 'request', id, method, params };
  return new Promise((resolve, reject) => {
    state.pending.set(id, { resolve, reject });
    ws.send(JSON.stringify(message));
  });
}

function failPending() {
  for (const { reject } of state.pending.values()) reject(new Error(`Lost the connection to ${machine}.`));
  state.pending.clear();
}

function onJson(message) {
  if (message.type === 'response') {
    const waiting = state.pending.get(message.id);
    if (!waiting) return;
    state.pending.delete(message.id);
    if (message.error) waiting.reject(new Error(message.error.message));
    else waiting.resolve(message.result);
  } else if (message.type === 'event') {
    onEvent(message.event, message.data);
  }
}

function onOutput(bytes) {
  if (!term || bytes.length < ID_LEN) return;
  if (idDecoder.decode(bytes.subarray(0, ID_LEN)) === state.open) term.write(bytes.subarray(ID_LEN));
}

// ---- data ----

async function load() {
  const [projects, sessions, profiles] = await Promise.all([
    call('project.list'),
    call('session.list', {}),
    call('agent.profiles'),
  ]);
  state.projects = projects;
  state.profiles = profiles.profiles;
  const next = new Map(sessions.filter((s) => s.state !== 'exited').map((s) => [s.id, s]));
  // Sessions that exited while this page was open stay listed (✕) until the next full load
  // that no longer has them; the open one stays so its screen can say why it ended.
  for (const [id, s] of state.sessions) if (s.state === 'exited' && !next.has(id)) next.set(id, s);
  state.sessions = next;
  state.loaded = true;
  render();
}

function scheduleReload() {
  clearTimeout(state.reloadTimer);
  state.reloadTimer = setTimeout(() => load().catch((err) => toast(err.message)), 300);
}

function onEvent(name, data) {
  const s = data && data.session_id ? state.sessions.get(data.session_id) : undefined;
  switch (name) {
    case 'session.state':
      if (!s) { scheduleReload(); return; }
      s.state = data.state;
      s.state_changed_at = data.changed_at;
      break;
    case 'session.exited':
      if (!s) return;
      s.state = 'exited';
      s.exit_code = data.exit_code;
      break;
    case 'session.updated': {
      const old = state.sessions.get(data.id);
      if (!old) { scheduleReload(); return; }
      state.sessions.set(data.id, { ...old, ...data });
      break;
    }
    case 'session.terminal_title':
      if (!s) return;
      s.terminal_title = data.title ?? null;
      break;
    case 'project.updated': {
      const i = state.projects.findIndex((p) => p.id === data.id);
      if (i < 0) state.projects.push(data);
      else state.projects[i] = data;
      break;
    }
    case 'session.snapshot':
      // The daemon dropped output this page was too slow for; `ansi` repaints the screen.
      if (term && data.session_id === state.open) term.write(data.ansi);
      return;
    default:
      return;
  }
  render();
}

// ---- rendering ----

function render() {
  renderHeader();
  if (!$('screen-list').hidden) renderList();
  if (!$('screen-session').hidden) renderSession();
}

function renderHeader() {
  const trying = !state.connected && (state.everConnected || state.attempt > 0);
  $('conn-dot').classList.toggle('conn-dot--off', !state.connected);
  $('conn-label').textContent = state.connected ? machine : trying ? 'Reconnecting…' : 'Connecting…';
  $('banner').hidden = !trying;
  $('banner-text').textContent = `Can't reach Mkrew on ${machine}. Retrying…`;
  $('new-agent').disabled = !state.connected;
  $('empty-new').disabled = !state.connected;
}

function avatar(project, small) {
  if (!project) return '';
  const letter = esc(project.name.slice(0, 1).toUpperCase());
  return `<span class="ws${small ? ' ws--sm' : ''} ${wsClass(project.color)}" aria-hidden="true">${letter}</span>`;
}

function markHtml(stateName) {
  if (stateName === 'working') return '<span class="dots"><i></i><i></i><i></i></span>';
  if (stateName === 'needs_input') return '<span class="q">?</span>';
  return esc(statusLabel(stateName).mark);
}

function chipHtml(chip) {
  return `<span class="chip${chip.shell ? ' chip--shell' : ''}" aria-hidden="true">${esc(chip.text)}</span>`;
}

function row(s, now, inNeeds) {
  const st = statusLabel(s.state);
  const project = state.projects.find((p) => p.id === s.project_id);
  const meta = inNeeds ? `${avatar(project, true)}${st.label}` : `${s.pinned ? PIN : ''}${esc(metaText(s, now))}`;
  return (
    `<li><button class="row" type="button" data-id="${s.id}">` +
    `<span class="st ${st.cls}" role="img" aria-label="${st.label}">${markHtml(s.state)}</span>` +
    chipHtml(chipFor(s, state.profiles)) +
    `<span class="title${s.state === 'exited' ? ' title--muted' : ''}">${esc(displayName(s, state.profiles))}</span>` +
    `<span class="meta">${meta}</span></button></li>`
  );
}

function renderList() {
  const now = Date.now();
  const { needs, groups } = buildList(state.projects, [...state.sessions.values()]);
  $('loading').hidden = state.loaded;
  $('empty').hidden = !state.loaded || needs.length > 0 || groups.length > 0;
  let html = '';
  if (needs.length) {
    html +=
      `<section class="group group--first"><h2 class="group-head">Needs you <span class="count">${needs.length}</span></h2>` +
      `<ul class="list needs">${needs.map((s) => row(s, now, true)).join('')}</ul></section>`;
  }
  groups.forEach((group, i) => {
    const first = !needs.length && i === 0 ? ' group--first' : '';
    html +=
      `<section class="group${first}"><h2 class="group-head">${avatar(group.project, false)}${esc(group.project.name)}</h2>` +
      `<ul class="list">${group.sessions.map((s) => row(s, now, false)).join('')}</ul></section>`;
  });
  $('groups').innerHTML = html;
}

function renderSession() {
  const s = state.sessions.get(state.open);
  if (!s) return;
  const chip = chipFor(s, state.profiles);
  $('s-chip').textContent = chip.text;
  $('s-chip').classList.toggle('chip--shell', chip.shell);
  $('s-title').textContent = displayName(s, state.profiles);
  const st = statusLabel(s.state);
  const pill = $('s-pill');
  pill.className = `pill pill--${st.pill}`;
  pill.innerHTML = `<span class="st ${st.cls}" aria-hidden="true">${markHtml(s.state)}</span>${esc(pillText(s, Date.now()))}`;
  const ended = s.state === 'exited';
  $('composer').hidden = ended;
  $('ended').hidden = !ended;
  $('ended-text').textContent = s.exit_code == null ? 'Session ended' : `Session ended (exit ${s.exit_code})`;
}

// ---- session screen ----

function ensureTerminal() {
  if (term) return;
  term = new Terminal({
    disableStdin: true,
    cursorBlink: false,
    fontFamily: '"SF Mono", Menlo, "Cascadia Mono", Consolas, monospace',
    fontSize: 8,
    scrollback: 2000,
    theme: TERMINAL_THEME,
  });
  term.open($('term'));
}

/** Shrinks the font until the session's columns fit (the session itself keeps its size). */
function fitTerminal() {
  if (!term) return;
  const box = $('term');
  const width = box.clientWidth - 16;
  term.options.fontSize = fitFontSize(width, term.cols);
  requestAnimationFrame(() => {
    const screen = box.querySelector('.xterm-screen');
    if (screen && screen.offsetWidth > width) {
      term.options.fontSize = Math.max(4, Math.floor(((term.options.fontSize * width) / screen.offsetWidth) * 10) / 10);
    }
  });
}

async function openSession(id) {
  if (state.open && state.open !== id) detach(state.open);
  state.open = id;
  if (!history.state?.session) history.pushState({ session: id }, '', location.hash);
  show('session');
  ensureTerminal();
  term.reset();
  $('reply').value = '';
  grow($('reply'));
  render();
  try {
    await attach(id);
  } catch (err) {
    toast(err.message);
  }
}

async function attach(id) {
  const result = await call('session.attach', { session_id: id });
  if (state.open !== id) {
    detach(id);
    return;
  }
  term.reset();
  term.resize(result.snapshot.cols, result.snapshot.rows);
  fitTerminal();
  term.write(result.ansi);
  const s = state.sessions.get(id);
  if (s) {
    s.state = result.state;
    render();
  }
}

function detach(id) {
  call('session.detach', { session_id: id }).catch(() => {});
}

function closeSession() {
  if (state.open) detach(state.open);
  state.open = null;
  show('list');
  render();
}

function grow(box) {
  box.style.height = 'auto';
  box.style.height = `${Math.min(box.scrollHeight, 132)}px`;
}

// ---- new agent ----

function selected(name) {
  return document.querySelector(`#new-form input[name="${name}"]:checked`)?.value ?? null;
}

function updateStart() {
  $('na-start').disabled = !state.connected || !selected('ws') || !selected('agent');
}

function pickDefaultAgent(project) {
  const inputs = [...document.querySelectorAll('#na-agents input:not(:disabled)')];
  const pick = inputs.find((i) => i.value === project?.default_agent) ?? inputs.find((i) => i.checked) ?? inputs[0];
  if (pick) pick.checked = true;
}

function openSheet() {
  const projects = state.projects.filter((p) => !p.archived);
  if (!projects.length) {
    toast('Add a workspace in Mkrew on your PC first.');
    return;
  }
  $('na-workspaces').innerHTML = projects
    .map(
      (p, i) =>
        `<label class="opt"><input type="radio" name="ws" value="${p.id}"${i === 0 ? ' checked' : ''}>` +
        `${avatar(p, false)}<span class="name">${esc(p.name)}</span>${CHECK}</label>`,
    )
    .join('');
  $('na-agents').innerHTML = state.profiles
    .map((p) => {
      const chip = chipFor({ kind: p.kind, agent_profile: p.id }, state.profiles);
      return (
        `<label class="opt${p.installed ? '' : ' opt--off'}">` +
        `<input type="radio" name="agent" value="${esc(p.id)}"${p.installed ? '' : ' disabled'}>` +
        `${chipHtml(chip)}<span class="opt-text"><span class="name">${esc(p.name)}</span>` +
        `${p.installed ? '' : '<span class="note">not installed</span>'}</span></label>`
      );
    })
    .join('');
  pickDefaultAgent(projects[0]);
  $('na-prompt').value = '';
  $('sheet').hidden = false;
  updateStart();
}

function closeSheet() {
  $('sheet').hidden = true;
}

async function startAgent() {
  const projectId = selected('ws');
  const profile = state.profiles.find((p) => p.id === selected('agent'));
  if (!projectId || !profile) return;
  const plan = newAgentPlan(profile, $('na-prompt').value);
  $('na-start').disabled = true;
  try {
    let sessionId;
    if (plan.task) {
      const task = await call('task.create', {
        project_id: projectId,
        title: plan.task.title,
        prompt: plan.task.prompt,
        agent_profile: profile.id,
      });
      ({ session_id: sessionId } = await call('task.run', { task_id: task.id, ...NEW_SIZE }));
    } else {
      ({ session_id: sessionId } = await call('session.spawn', {
        project_id: projectId,
        agent_profile: profile.id,
        kind: profile.kind,
        ...NEW_SIZE,
      }));
      if (plan.sendAfter) await call('session.send', { session_id: sessionId, text: plan.sendAfter, submit: true });
    }
    closeSheet();
    await load();
    await openSession(sessionId);
  } catch (err) {
    toast(err.message);
    updateStart();
  }
}

// ---- wiring ----

function keepKeyboard(el) {
  // A tap on a quick key or Send must not take focus from the reply box (iOS would close
  // the keyboard); `click` still fires.
  el.addEventListener('pointerdown', (e) => e.preventDefault());
}

function fitViewport() {
  const vv = window.visualViewport;
  const apply = () => {
    if (vv) {
      $('app').style.height = `${vv.height}px`;
      window.scrollTo(0, 0);
    }
    fitTerminal();
  };
  (vv ?? window).addEventListener('resize', apply);
  apply();
}

function wire() {
  $('token-form').addEventListener('submit', (e) => {
    e.preventDefault();
    const token = tokenFromText($('token-input').value);
    if (!token) {
      $('token-error').textContent = 'A token is 64 characters: 0–9 and a–f.';
      return;
    }
    state.token = token;
    localStorage.setItem(TOKEN_KEY, token);
    history.replaceState(null, '', `#t=${token}`);
    state.attempt = 0;
    show('list');
    render();
    connect();
  });

  $('groups').addEventListener('click', (e) => {
    const id = e.target.closest('.row')?.dataset.id;
    if (id) openSession(id);
  });
  $('new-agent').addEventListener('click', openSheet);
  $('empty-new').addEventListener('click', openSheet);

  $('back').addEventListener('click', () => {
    if (history.state?.session) history.back();
    else closeSession();
  });
  window.addEventListener('popstate', () => {
    if (state.open && !history.state?.session) closeSession();
  });

  $('composer').addEventListener('submit', async (e) => {
    e.preventDefault();
    const box = $('reply');
    const text = box.value;
    if (!text.trim() || !state.open) return;
    try {
      await call('session.send', { session_id: state.open, text, submit: true });
      box.value = '';
      grow(box);
    } catch (err) {
      toast(err.message);
    }
  });
  $('reply').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      $('composer').requestSubmit();
    }
  });
  $('reply').addEventListener('input', (e) => grow(e.target));
  keepKeyboard($('keys'));
  keepKeyboard($('send'));
  $('keys').addEventListener('click', (e) => {
    const key = e.target.closest('[data-key]')?.dataset.key;
    if (!key || !state.open) return;
    const appCursor = term?.modes.applicationCursorKeysMode ?? false;
    call('session.input', { session_id: state.open, data: toBase64(keyBytes(key, appCursor)) }).catch((err) => toast(err.message));
  });

  $('sheet-close').addEventListener('click', closeSheet);
  $('sheet').addEventListener('click', (e) => {
    if (e.target === $('sheet')) closeSheet();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !$('sheet').hidden) closeSheet();
  });
  $('na-workspaces').addEventListener('change', (e) => {
    pickDefaultAgent(state.projects.find((p) => p.id === e.target.value));
    updateStart();
  });
  $('na-agents').addEventListener('change', updateStart);
  $('new-form').addEventListener('submit', (e) => {
    e.preventDefault();
    startAgent();
  });

  // iOS suspends background pages: reconnect at once when the page comes back.
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && state.retryTimer) {
      state.attempt = 0;
      connect();
    }
  });
  setInterval(render, 30_000);
}

function start() {
  const fromHash = tokenFromHash(location.hash);
  if (fromHash) localStorage.setItem(TOKEN_KEY, fromHash);
  state.token = fromHash ?? localStorage.getItem(TOKEN_KEY);
  // Keep `#t=` in the URL: "Add to Home Screen" bookmarks it, and iOS gives the Home Screen
  // app its own storage.
  if (state.token && !fromHash) history.replaceState(history.state, '', `#t=${state.token}`);
  wire();
  fitViewport();
  if (state.token) {
    show('list');
    render();
    connect();
  } else {
    showToken('');
  }
}

start();
```

- [ ] **Step 5: Check the modules parse and the helpers still pass**

Run: `node --check crates/mkrew-cli/web/app.mjs && node --test crates/mkrew-cli/web/model.test.mjs`
Expected: no output from `--check`, then PASS (16 tests). (The page runs for real in Task 3's smoke test.)

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-cli/web/index.html crates/mkrew-cli/web/app.css crates/mkrew-cli/web/app.mjs crates/mkrew-cli/web/vendor/xterm.mjs crates/mkrew-cli/web/vendor/xterm.css crates/mkrew-cli/web/vendor/LICENSE-xterm
git commit -m "feat(web): phone page: sessions, terminal with quick keys, new agent, token screen"
```

---

### Task 3: `mk web` server, relay, docs and smoke

**Current state (investigated):**
- `crates/mkrew-cli/src/main.rs`: `mod` list (alphabetical, `mod task;` last), `enum Command` (last variant `Hook`), `fn run` dispatches after `let paths = Paths::from_env()?;`.
- `crates/mkrew-cli/src/daemon.rs`: `pub fn try_connect(paths: &Paths) -> anyhow::Result<Option<Client>>`, `pub fn start(paths: &Paths) -> anyhow::Result<ExitCode>` (prints `daemon started (pid N)`).
- `crates/mkrew-ui/src/client.rs:256-294`: the async connect + hello pattern (`interprocess::local_socket::tokio::{RecvHalf, SendHalf, Stream, prelude::*}`, `mkrew_daemon::ipc::name(socket)`). Its comment: racing single pipe reads in `select!` loses data on Windows, so each loop runs to its end.
- `mkrew_protocol::paint::paint(&ScreenSnapshot) -> String` is what `mk attach` paints (`crates/mkrew-cli/src/attach.rs:65`, `:193-195` clears history for `session.snapshot`).
- `crates/mkrew-cli/tests/cli.rs`: `Env::new()`, `env.command(&[..])` (sets `MKREW_*`, `MKREW_NOTIFY=off`), `env.ok(&[..])`, `env.write_fake_profile()`; the fake agent's default scenario asks `Do you want to proceed? (y/n)`, prints `Done.` after `y\r` and exits 0.
- `tokio-tungstenite` 0.30: `WebSocketStream::from_partially_read(stream, part, Role::Server, None)` is `async`; `tungstenite::handshake::derive_accept_key(&[u8]) -> String`; `CloseFrame { code: CloseCode, reason: Utf8Bytes }`. `getrandom` 0.3: `getrandom::fill(&mut [u8])`.

**Files:**
- Create: `crates/mkrew-cli/src/web.rs`, `crates/mkrew-cli/src/web/token.rs`, `crates/mkrew-cli/src/web/http.rs`, `crates/mkrew-cli/src/web/relay.rs`, `crates/mkrew-cli/src/web/tailscale.rs`
- Create: `docs/decisions/NNNN-mobile-web.md`
- Modify: `crates/mkrew-cli/Cargo.toml`, `crates/mkrew-cli/src/main.rs`, `crates/mkrew-cli/tests/cli.rs`, `Cargo.lock`
- Modify: `docs/SPEC.md` (§1, §9, §15, §19, §20 intro), `README.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`
- Commit with it (written during planning): `docs/superpowers/specs/2026-10-10-mobile-web-design.md`, `docs/superpowers/plans/2026-10-10-mobile-web.md`, `docs/design/mobile-web/*.html`

**Interfaces:**
- Consumes: Task 2's asset files (paths above) via `include_bytes!`.
- Produces: `mk web [--listen ADDR] [--host NAME] [--new-token]`; stdout lines `Mkrew web is listening on http://<addr>` and `Open on your phone: https://<host>/#t=<token>` (tests parse both).
- Produces (crate-internal): `web::run(&Paths, web::Args) -> anyhow::Result<ExitCode>`; `token::Token { parse, as_str, matches }`, `token::load_or_create(&Path, renew: bool)`; `http::{ASSETS, Head, head_end, parse_head, Route, route, switching_protocols}`; `relay::{relay, daemon_request, FromBrowser, browser_message, json_with_ansi}`; `tailscale::{dns_name, parse_dns_name}`.

- [ ] **Step 1: Dependencies**

In `crates/mkrew-cli/Cargo.toml` replace `interprocess = "2.4.4"` with:

```toml
# `tokio`: `mk web` relays each browser to its own async daemon connection (ADR NNNN).
interprocess = { version = "2.4.4", features = ["tokio"] }
```

and add (keep the table alphabetical: `futures-util` after `dunce`, `getrandom` after it, `tokio`/`tokio-tungstenite` after `serde_json`, before `toml_edit`):

```toml
# `mk web`: split the WebSocket into a sink and a stream (ADR NNNN).
futures-util = { version = "0.3", default-features = false, features = ["sink", "std"] }
# `mk web`: the access token (ADR NNNN); 0.3 is already in the tree.
getrandom = "0.3"
# `mk web`: HTTP listener and relay runtime (ADR NNNN); tokio is already in the tree.
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "sync", "time"] }
# `mk web`: WebSocket framing only; the HTTP head is parsed by hand (ADR NNNN).
tokio-tungstenite = { version = "0.30", default-features = false, features = ["handshake"] }
```

Under `[dev-dependencies]` add:

```toml
# `mk web` tests: a blocking WebSocket client (same version tokio-tungstenite uses).
tungstenite = "0.30"
```

Run: `df -h /c` (need ≥ 5 GB free), then `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo check -p mkrew-cli -j 4`
Expected: compiles (new crates download); `git diff --stat Cargo.lock` shows only added packages (`tokio-tungstenite`, `tungstenite` and their deps).

- [ ] **Step 2: Write the failing unit tests**

Create `crates/mkrew-cli/src/web.rs` with only the module list for now:

```rust
//! `mk web` (ADR NNNN): serves the phone page and relays its WebSocket to the daemon.

mod http;
mod relay;
mod tailscale;
mod token;
```

Add `mod web;` to `crates/mkrew-cli/src/main.rs` after `mod task;`.

Create each module with its tests and declarations that do not exist yet (so the tests fail to compile):

`crates/mkrew-cli/src/web/token.rs`:

```rust
//! The `mk web` access token (ADR NNNN): 32 random bytes as 64 lowercase hex characters in
//! `<config>/web_token`, required by `/check` and `/ws`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_lowercase_hex() {
        assert!(Token::parse(&"a1".repeat(32)).is_some());
        assert!(Token::parse(&"A1".repeat(32)).is_none());
        assert!(Token::parse(&"a1".repeat(31)).is_none());
        assert!(Token::parse(&format!("{}g", "a".repeat(63))).is_none());
    }

    #[test]
    fn matches_only_the_same_token() {
        let token = Token::parse(&"0f".repeat(32)).unwrap();
        assert!(token.matches(Some(&"0f".repeat(32))));
        assert!(!token.matches(Some(&"0e".repeat(32))));
        assert!(!token.matches(Some("0f")));
        assert!(!token.matches(None));
    }

    #[test]
    fn load_or_create_keeps_the_token_until_renewed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config").join("web_token");
        let first = load_or_create(&file, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), first.as_str());
        assert_eq!(load_or_create(&file, false).unwrap().as_str(), first.as_str());
        let renewed = load_or_create(&file, true).unwrap();
        assert_ne!(renewed.as_str(), first.as_str());
        // A hand-broken file is replaced instead of being used.
        std::fs::write(&file, "not a token").unwrap();
        let replaced = load_or_create(&file, false).unwrap();
        assert!(Token::parse(replaced.as_str()).is_some());
    }
}
```

`crates/mkrew-cli/src/web/tailscale.rs`:

```rust
//! Tailscale's MagicDNS name of this PC, for the link `mk web` prints (ADR NNNN).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_magicdns_name_without_the_trailing_dot() {
        let json = r#"{"Self":{"HostName":"studio-pc","DNSName":"studio-pc.tail1234.ts.net."}}"#;
        assert_eq!(parse_dns_name(json).as_deref(), Some("studio-pc.tail1234.ts.net"));
    }

    #[test]
    fn no_name_without_magicdns() {
        assert_eq!(parse_dns_name(r#"{"Self":{"DNSName":""}}"#), None);
        assert_eq!(parse_dns_name(r#"{"BackendState":"Stopped"}"#), None);
        assert_eq!(parse_dns_name("not json"), None);
    }
}
```

`crates/mkrew-cli/src/web/http.rs`:

```rust
//! The few HTTP requests `mk web` answers (ADR NNNN): the page, a token check and the
//! WebSocket upgrade. GET only; every answer closes the connection.

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> Token {
        Token::parse(&"ab".repeat(32)).unwrap()
    }

    fn head(text: &str) -> Head {
        parse_head(text.as_bytes()).unwrap()
    }

    #[test]
    fn finds_the_end_of_the_head() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n\r\nrest"), Some(27));
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n"), None);
    }

    #[test]
    fn parses_an_upgrade_request() {
        let text = format!(
            "GET /ws?x=1&t={} HTTP/1.1\r\nHost: pc\r\nUpgrade: WebSocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
            "ab".repeat(32)
        );
        assert_eq!(
            head(&text),
            Head {
                method: "GET".into(),
                path: "/ws".into(),
                token: Some("ab".repeat(32)),
                upgrade: true,
                websocket_key: Some("dGhlIHNhbXBsZSBub25jZQ==".into()),
            }
        );
        assert_eq!(parse_head(b"\xff\xfe"), None);
        assert_eq!(parse_head(b"GARBAGE\r\n\r\n"), None);
    }

    #[test]
    fn serves_the_page_and_its_files() {
        for (path, kind) in [
            ("/", "text/html; charset=utf-8"),
            ("/index.html", "text/html; charset=utf-8"),
            ("/app.mjs", "text/javascript; charset=utf-8"),
            ("/vendor/xterm.css", "text/css; charset=utf-8"),
        ] {
            match route(&head(&format!("GET {path} HTTP/1.1\r\n\r\n")), &token()) {
                Route::Asset(asset) => assert_eq!(asset.content_type, kind, "{path}"),
                other => panic!("{path}: {other:?}"),
            }
        }
        assert!(matches!(route(&head("GET /nope HTTP/1.1\r\n\r\n"), &token()), Route::NotFound));
        assert!(matches!(route(&head("POST / HTTP/1.1\r\n\r\n"), &token()), Route::MethodNotAllowed));
    }

    #[test]
    fn routes_check_the_token() {
        let good = "ab".repeat(32);
        let bad = "cd".repeat(32);
        let check = |query: &str| route(&head(&format!("GET /check{query} HTTP/1.1\r\n\r\n")), &token());
        assert!(matches!(check(&format!("?t={good}")), Route::Check));
        assert!(matches!(check(&format!("?t={bad}")), Route::Unauthorized));
        assert!(matches!(check(""), Route::Unauthorized));
        let upgrade = |t: &str| {
            route(
                &head(&format!(
                    "GET /ws?t={t} HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k==\r\n\r\n"
                )),
                &token(),
            )
        };
        assert!(matches!(upgrade(&good), Route::Upgrade { key } if key == "k=="));
        assert!(matches!(upgrade(&bad), Route::Unauthorized));
        let plain = route(&head(&format!("GET /ws?t={good} HTTP/1.1\r\n\r\n")), &token());
        assert!(matches!(plain, Route::BadRequest));
    }

    #[test]
    fn answers_with_status_length_and_close() {
        let text = String::from_utf8(Route::Unauthorized.response()).unwrap();
        assert!(text.starts_with("HTTP/1.1 401 Unauthorized\r\n"), "{text}");
        assert!(text.contains("Content-Length: 23\r\nConnection: close\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\nwrong or missing token\n"), "{text}");
    }

    #[test]
    fn accepts_the_upgrade_with_the_rfc_6455_key() {
        let text = String::from_utf8(switching_protocols("dGhlIHNhbXBsZSBub25jZQ==")).unwrap();
        assert!(text.starts_with("HTTP/1.1 101 Switching Protocols\r\n"), "{text}");
        assert!(text.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\n"));
    }
}
```

`crates/mkrew-cli/src/web/relay.rs`:

```rust
//! One browser WebSocket ↔ one daemon connection (ADR NNNN).

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{
        CellRun, CursorSnapshot, SessionAttachResult, SessionListParams, SessionSnapshotEvent,
        SessionState, SessionStateEvent, StateSource, TermColor, TermModes,
    };
    use ulid::Ulid;

    fn snapshot(line: &str, history: &str) -> ScreenSnapshot {
        let run = |text: &str| {
            vec![CellRun {
                text: text.into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        ScreenSnapshot {
            cols: 80,
            rows: 24,
            lines: vec![run(line)],
            history: vec![run(history)],
            cursor: CursorSnapshot { row: 0, col: 0, visible: true },
            modes: TermModes::default(),
        }
    }

    fn reply(answer: FromBrowser) -> serde_json::Value {
        match answer {
            FromBrowser::Reply(text) => serde_json::from_str(&text).unwrap(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn forwards_requests_as_frames() {
        let text = r#"{"type":"request","id":3,"method":"session.list","params":{}}"#;
        let FromBrowser::Forward(bytes) = daemon_request(text) else {
            panic!("not forwarded");
        };
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes);
        assert_eq!(
            decoder.next_frame().unwrap(),
            Some(Frame::Message(Message::Request {
                id: 3,
                request: Request::SessionList(SessionListParams::default()),
            }))
        );
    }

    #[test]
    fn refuses_hello_resize_and_unknown_methods() {
        let hello = r#"{"type":"request","id":1,"method":"hello","params":{"client":"ui","protocol_version":1}}"#;
        let resize = format!(
            r#"{{"type":"request","id":2,"method":"session.resize","params":{{"session_id":"{}","cols":40,"rows":20}}}}"#,
            Ulid::new()
        );
        let unknown = r#"{"type":"request","id":3,"method":"no.such"}"#;
        for (text, id) in [(hello.to_owned(), 1), (resize, 2), (unknown.to_owned(), 3)] {
            let value = reply(daemon_request(&text));
            assert_eq!(value["id"], id, "{value}");
            assert_eq!(value["error"]["code"], "invalid_request", "{value}");
        }
        assert_eq!(daemon_request("not json"), FromBrowser::Ignore);
        assert_eq!(daemon_request(r#"{"method":"no.such"}"#), FromBrowser::Ignore);
    }

    #[test]
    fn attach_result_gets_the_painted_screen() {
        let result = SessionAttachResult { state: SessionState::NeedsInput, snapshot: snapshot("Apply? (y/n)", "older line") };
        let message = Message::Response { id: 9, outcome: Outcome::ok(&result).unwrap() };
        let value: serde_json::Value = serde_json::from_str(&json_with_ansi(&message).unwrap()).unwrap();
        let ansi = value["result"]["ansi"].as_str().unwrap();
        assert!(ansi.contains("Apply? (y/n)") && ansi.contains("older line"), "{ansi}");
        assert_eq!(value["result"]["state"], "needs_input");
    }

    #[test]
    fn snapshot_event_is_painted_without_history() {
        let event = Event::SessionSnapshot(SessionSnapshotEvent { session_id: Ulid::new(), snapshot: snapshot("now", "already scrolled") });
        let value: serde_json::Value = serde_json::from_str(&json_with_ansi(&Message::Event(event)).unwrap()).unwrap();
        let ansi = value["data"]["ansi"].as_str().unwrap();
        assert!(ansi.contains("now") && !ansi.contains("already scrolled"), "{ansi}");
    }

    #[test]
    fn other_messages_pass_unchanged() {
        let event = Message::Event(Event::SessionState(SessionStateEvent {
            session_id: Ulid::new(),
            state: SessionState::Working,
            source: StateSource::Hook,
            changed_at: 5,
        }));
        let value: serde_json::Value = serde_json::from_str(&json_with_ansi(&event).unwrap()).unwrap();
        assert_eq!(value, serde_json::to_value(&event).unwrap());
    }

    #[test]
    fn output_goes_out_binary_behind_the_session_id() {
        let session_id = Ulid::new();
        let message = browser_message(Frame::Output { session_id, data: b"hi".to_vec() }).unwrap();
        let bytes = message.into_data();
        assert_eq!(&bytes[..26], session_id.to_string().as_bytes());
        assert_eq!(&bytes[26..], b"hi");
    }
}
```

Run: `CARGO_TARGET_DIR=…/mkrew/target cargo test -p mkrew-cli --bin mk web:: -j 4`
Expected: FAIL to compile (`cannot find type Token`, `parse_head`, `daemon_request`, … in this scope).

- [ ] **Step 3: Implement `token.rs`**

Put above the tests in `crates/mkrew-cli/src/web/token.rs`:

```rust
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, anyhow};

pub struct Token(String);

impl Token {
    /// A token as stored and typed: exactly 64 lowercase hex characters.
    pub fn parse(text: &str) -> Option<Self> {
        let valid = text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares in time independent of where the first difference is.
    pub fn matches(&self, candidate: Option<&str>) -> bool {
        let Some(candidate) = candidate else { return false };
        let (ours, theirs) = (self.0.as_bytes(), candidate.as_bytes());
        ours.len() == theirs.len() && ours.iter().zip(theirs).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

/// The stored token, or a new one when `renew`, the file is missing or holds no valid token.
pub fn load_or_create(file: &Path, renew: bool) -> anyhow::Result<Token> {
    if !renew
        && let Some(token) = std::fs::read_to_string(file).ok().and_then(|text| Token::parse(text.trim()))
    {
        return Ok(token);
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness from the OS: {e}"))?;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    write_private(file, &text).with_context(|| format!("write {}", file.display()))?;
    Ok(Token(text))
}

/// Owner-only on Unix; the per-user config folder already is on Windows.
fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(file)?.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(file, text)
    }
}
```

(If the toolchain rejects the `if … && let` chain, rewrite it as `if !renew { if let Some(token) = … { return Ok(token); } }`.)

- [ ] **Step 4: Implement `tailscale.rs`**

Above its tests:

```rust
use std::process::Command;

/// `tailscale status --json` → `Self.DNSName`; `None` when Tailscale is missing, stopped or
/// has no MagicDNS name.
pub fn dns_name() -> Option<String> {
    let output = Command::new("tailscale").args(["status", "--json"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_dns_name(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_dns_name(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let name = value.get("Self")?.get("DNSName")?.as_str()?.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}
```

- [ ] **Step 5: Implement `http.rs`**

Above its tests:

```rust
use super::token::Token;

#[derive(Debug)]
pub struct Asset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub body: &'static [u8],
}

const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";

/// The page (Task 2 of the mobile-web plan), embedded in `mk`.
pub const ASSETS: &[Asset] = &[
    Asset { path: "/", content_type: HTML, body: include_bytes!("../../web/index.html") },
    Asset { path: "/app.css", content_type: CSS, body: include_bytes!("../../web/app.css") },
    Asset { path: "/app.mjs", content_type: JS, body: include_bytes!("../../web/app.mjs") },
    Asset { path: "/model.mjs", content_type: JS, body: include_bytes!("../../web/model.mjs") },
    Asset { path: "/vendor/xterm.mjs", content_type: JS, body: include_bytes!("../../web/vendor/xterm.mjs") },
    Asset { path: "/vendor/xterm.css", content_type: CSS, body: include_bytes!("../../web/vendor/xterm.css") },
];

/// What `mk web` reads from a request head.
#[derive(Debug, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    /// The `t` query parameter.
    pub token: Option<String>,
    /// `Upgrade: websocket` was sent.
    pub upgrade: bool,
    pub websocket_key: Option<String>,
}

/// Length of the head including its blank line, once the buffer holds all of it.
pub fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|at| at + 4)
}

pub fn parse_head(raw: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let target = first.next()?;
    first.next()?.strip_prefix("HTTP/")?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let token = query.split('&').find_map(|pair| pair.strip_prefix("t=")).map(str::to_owned);
    let mut upgrade = false;
    let mut websocket_key = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("upgrade") {
            upgrade = value.eq_ignore_ascii_case("websocket");
        } else if name.eq_ignore_ascii_case("sec-websocket-key") {
            websocket_key = Some(value.to_owned());
        }
    }
    Some(Head { method, path: path.to_owned(), token, upgrade, websocket_key })
}

#[derive(Debug)]
pub enum Route {
    Asset(&'static Asset),
    /// `/check` with the right token: the page may open the WebSocket.
    Check,
    /// `/ws` with the right token: answered by [`switching_protocols`], then relayed.
    Upgrade { key: String },
    Unauthorized,
    NotFound,
    BadRequest,
    MethodNotAllowed,
}

pub fn route(head: &Head, token: &Token) -> Route {
    if head.method != "GET" {
        return Route::MethodNotAllowed;
    }
    let authorized = token.matches(head.token.as_deref());
    match head.path.as_str() {
        "/check" if authorized => Route::Check,
        "/check" => Route::Unauthorized,
        "/ws" => match (&head.websocket_key, head.upgrade) {
            (Some(key), true) if authorized => Route::Upgrade { key: key.clone() },
            (Some(_), true) => Route::Unauthorized,
            _ => Route::BadRequest,
        },
        "/index.html" => Route::Asset(&ASSETS[0]),
        path => ASSETS.iter().find(|asset| asset.path == path).map_or(Route::NotFound, Route::Asset),
    }
}

impl Route {
    /// The whole answer. `Upgrade` is answered by [`switching_protocols`] instead (`handle`
    /// never calls this for it); it falls back to 400.
    pub fn response(&self) -> Vec<u8> {
        match self {
            Route::Asset(asset) => respond(
                "200 OK",
                &[("Content-Type", asset.content_type), ("Cache-Control", "no-cache"), ("X-Content-Type-Options", "nosniff")],
                asset.body,
            ),
            Route::Check => respond("204 No Content", &[("Cache-Control", "no-store")], b""),
            Route::Unauthorized => respond("401 Unauthorized", &[], b"wrong or missing token\n"),
            Route::NotFound => respond("404 Not Found", &[], b"not found\n"),
            Route::BadRequest | Route::Upgrade { .. } => respond("400 Bad Request", &[], b"bad request\n"),
            Route::MethodNotAllowed => respond("405 Method Not Allowed", &[("Allow", "GET")], b"GET only\n"),
        }
    }
}

fn respond(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

pub fn switching_protocols(key: &str) -> Vec<u8> {
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .into_bytes()
}
```

- [ ] **Step 6: Implement `relay.rs`**

Replace the module doc and put above its tests:

```rust
//! One browser WebSocket ↔ one daemon connection (ADR NNNN). The browser speaks SPEC §7 JSON
//! in text messages. The relay greets the daemon itself, refuses `hello` and `session.resize`
//! (a phone width would reflow the agent), adds `ansi` (the `mk attach` painter) next to every
//! screen snapshot, and sends terminal output as binary messages: the 26-character session id,
//! then the raw PTY bytes.

use std::io::ErrorKind;

use anyhow::{anyhow, bail};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use interprocess::local_socket::tokio::{RecvHalf, SendHalf, Stream, prelude::*};
use mkrew_protocol::paint::paint;
use mkrew_protocol::{
    ClientKind, ErrorCode, Event, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, Request, ResponseError, ScreenSnapshot, encode,
};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const READ_BUFFER: usize = 64 * 1024;
/// Messages waiting for a slow browser. When full the relay stops reading the daemon, so the
/// daemon drops queued output and sends a `session.snapshot` instead (SPEC §7 backpressure).
const OUTBOX: usize = 64;
/// Browser ids start at 1.
const HELLO_ID: u64 = 0;

type Ws = WebSocketStream<TcpStream>;

pub async fn relay(ws: Ws, socket: &str) {
    let (mut sink, stream) = ws.split();
    let (rx, tx, decoder) = match open_daemon(socket).await {
        Ok(daemon) => daemon,
        Err(e) => {
            let _ = sink.send(close(&format!("{e:#}"))).await;
            return;
        }
    };
    let (out, mut outbox) = mpsc::channel::<WsMessage>(OUTBOX);
    let replies = out.clone();
    // Each loop runs until its side closes; none is cancelled and resumed (racing single pipe
    // reads in `select!` loses data on Windows, see mkrew-ui `client.rs`).
    let ended = tokio::select! {
        r = write_browser(&mut sink, &mut outbox) => r,
        r = daemon_to_browser(rx, decoder, out) => r,
        r = browser_to_daemon(stream, tx, replies) => r,
    };
    let last = match ended {
        Ok(()) => WsMessage::Close(None),
        Err(e) => close(&format!("{e:#}")),
    };
    let _ = sink.send(last).await;
}

async fn write_browser(sink: &mut SplitSink<Ws, WsMessage>, outbox: &mut mpsc::Receiver<WsMessage>) -> anyhow::Result<()> {
    while let Some(message) = outbox.recv().await {
        sink.send(message).await?;
    }
    Ok(())
}

async fn daemon_to_browser(mut rx: RecvHalf, mut decoder: FrameDecoder, out: mpsc::Sender<WsMessage>) -> anyhow::Result<()> {
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        while let Some(frame) = decoder.next_frame()? {
            out.send(browser_message(frame)?).await?;
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            bail!("the Mkrew daemon closed the connection");
        }
        decoder.push(&buf[..n]);
    }
}

async fn browser_to_daemon(mut stream: SplitStream<Ws>, mut tx: SendHalf, replies: mpsc::Sender<WsMessage>) -> anyhow::Result<()> {
    while let Some(message) = stream.next().await {
        match message? {
            WsMessage::Text(text) => match daemon_request(&text) {
                FromBrowser::Forward(bytes) => tx.write_all(&bytes).await?,
                FromBrowser::Reply(reply) => replies.send(WsMessage::text(reply)).await?,
                FromBrowser::Ignore => {}
            },
            WsMessage::Close(_) => break,
            // Pings are answered by tungstenite; the page sends no binary messages.
            _ => {}
        }
    }
    Ok(())
}

/// Connects and greets like the UI does (`mkrew-ui` `client.rs`).
async fn open_daemon(socket: &str) -> anyhow::Result<(RecvHalf, SendHalf, FrameDecoder)> {
    let stream = match Stream::connect(mkrew_daemon::ipc::name(socket)?).await {
        Ok(stream) => stream,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            bail!("the Mkrew daemon is not running on this PC")
        }
        Err(e) => return Err(anyhow::Error::new(e).context("connect to the Mkrew daemon")),
    };
    let (mut rx, mut tx) = stream.split();
    let hello = Frame::Message(Message::Request {
        id: HELLO_ID,
        request: Request::Hello(HelloParams { client: ClientKind::Ui, protocol_version: PROTOCOL_VERSION }),
    });
    tx.write_all(&encode(&hello)?).await?;
    let mut decoder = FrameDecoder::new();
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        if let Some(frame) = decoder.next_frame()? {
            return match frame {
                Frame::Message(Message::Response { id: HELLO_ID, outcome }) => {
                    outcome.into_result::<HelloResult>().map_err(|e| match e {
                        ResponseError::Daemon(body) => anyhow!("the Mkrew daemon refused mk web: {}", body.message),
                        other => anyhow!("invalid hello reply: {other}"),
                    })?;
                    Ok((rx, tx, decoder))
                }
                other => Err(anyhow!("the daemon sent {other:?} before greeting")),
            };
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            bail!("the Mkrew daemon closed the connection during hello");
        }
        decoder.push(&buf[..n]);
    }
}

/// What a browser text message becomes.
#[derive(Debug, PartialEq, Eq)]
pub enum FromBrowser {
    /// An encoded §7 frame for the daemon.
    Forward(Vec<u8>),
    /// An error response for the browser (same id).
    Reply(String),
    /// Not JSON, or no id to answer.
    Ignore,
}

pub fn daemon_request(text: &str) -> FromBrowser {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return FromBrowser::Ignore;
    };
    let id = value.get("id").and_then(serde_json::Value::as_u64);
    let refuse = |why: String| match id {
        Some(id) => serde_json::to_string(&Message::Response { id, outcome: Outcome::error(ErrorCode::InvalidRequest, why) })
            .map_or(FromBrowser::Ignore, FromBrowser::Reply),
        None => FromBrowser::Ignore,
    };
    match serde_json::from_value::<Message>(value) {
        Ok(Message::Request { request: Request::Hello(_), .. }) => refuse("mk web already greeted the daemon".into()),
        Ok(Message::Request { request: Request::SessionResize(_), .. }) => {
            refuse("the web client never resizes a session".into())
        }
        Ok(message @ Message::Request { .. }) => {
            encode(&Frame::Message(message)).map_or_else(|e| refuse(e.to_string()), FromBrowser::Forward)
        }
        Ok(_) => refuse("only requests are accepted".into()),
        Err(e) => refuse(format!("invalid request: {e}")),
    }
}

pub fn browser_message(frame: Frame) -> serde_json::Result<WsMessage> {
    Ok(match frame {
        Frame::Output { session_id, data } => {
            let mut bytes = session_id.to_string().into_bytes();
            bytes.extend_from_slice(&data);
            WsMessage::binary(bytes)
        }
        Frame::Message(message) => WsMessage::text(json_with_ansi(&message)?),
    })
}

/// The message as JSON; a response holding a `snapshot` (e.g. `session.attach`) and a
/// `session.snapshot` event also get `ansi`: the escape sequences that draw it.
pub fn json_with_ansi(message: &Message) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(message)?;
    match message {
        Message::Response { outcome: Outcome::Result(result), .. } => {
            if let Some(snapshot) = result.get("snapshot").and_then(|s| ScreenSnapshot::deserialize(s).ok()) {
                value["result"]["ansi"] = paint(&snapshot).into();
            }
        }
        Message::Event(Event::SessionSnapshot(event)) => {
            // The history is already in the page's scrollback from the attach.
            let snapshot = ScreenSnapshot { history: Vec::new(), ..event.snapshot.clone() };
            value["data"]["ansi"] = paint(&snapshot).into();
        }
        _ => {}
    }
    serde_json::to_string(&value)
}

fn close(reason: &str) -> WsMessage {
    // A close reason must fit in 123 bytes.
    let mut reason = reason.to_owned();
    while reason.len() > 120 {
        reason.pop();
    }
    WsMessage::Close(Some(CloseFrame { code: CloseCode::Error, reason: reason.into() }))
}
```

- [ ] **Step 7: Implement `web.rs` and the command**

`crates/mkrew-cli/src/web.rs`:

```rust
//! `mk web` (ADR NNNN): serves the phone page and relays its WebSocket to the daemon. Listens
//! on 127.0.0.1 by default; `tailscale serve` publishes it with HTTPS inside the user's
//! tailnet. The daemon itself never listens on the network (SPEC §15).

mod http;
mod relay;
mod tailscale;
mod token;

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use mkrew_core::Paths;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Role;

pub struct Args {
    pub listen: SocketAddr,
    pub host: Option<String>,
    pub new_token: bool,
}

const HEAD_LIMIT: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

pub fn run(paths: &Paths, args: Args) -> anyhow::Result<ExitCode> {
    let token = token::load_or_create(&paths.config_dir.join("web_token"), args.new_token)?;
    // After a reboot the phone may be the only way in: make sure there is a daemon.
    if crate::daemon::try_connect(paths)?.is_none() {
        crate::daemon::start(paths)?;
    }
    let host = args.host.or_else(tailscale::dns_name);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start the async runtime")?;
    runtime.block_on(serve(args.listen, host, paths.socket.clone(), token))
}

async fn serve(listen: SocketAddr, host: Option<String>, socket: String, token: token::Token) -> anyhow::Result<ExitCode> {
    let listener = TcpListener::bind(listen).await.with_context(|| format!("listen on {listen}"))?;
    let addr = listener.local_addr()?;
    println!("Mkrew web is listening on http://{addr}");
    match &host {
        Some(host) => println!("Open on your phone: https://{host}/#t={}", token.as_str()),
        None => {
            println!("Open on your phone: https://<this-pc>.<your-tailnet>.ts.net/#t={}", token.as_str());
            println!("(Tailscale's name for this PC was not found; pass --host <name> for the exact link.)");
        }
    }
    println!("First time on this PC: run `tailscale serve --bg {}` so the link works on your tailnet.", addr.port());
    let shared = Arc::new((socket, token));
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("mk web: accept: {e}");
                continue;
            }
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let (socket, token) = &*shared;
            // A client that leaves mid-request needs no report.
            let _ = handle(stream, socket, token).await;
        });
    }
}

async fn handle(mut stream: TcpStream, socket: &str, token: &token::Token) -> anyhow::Result<()> {
    let (head, rest) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut stream))
        .await
        .context("the request head did not arrive in time")??;
    let route = http::parse_head(&head).map_or(http::Route::BadRequest, |head| http::route(&head, token));
    if let http::Route::Upgrade { key } = &route {
        stream.write_all(&http::switching_protocols(key)).await?;
        let ws = WebSocketStream::from_partially_read(stream, rest, Role::Server, None).await;
        relay::relay(ws, socket).await;
        return Ok(());
    }
    stream.write_all(&route.response()).await?;
    stream.shutdown().await?;
    Ok(())
}

/// The request head and whatever arrived after it.
async fn read_head(stream: &mut TcpStream) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = http::head_end(&buf) {
            let rest = buf.split_off(end);
            return Ok((buf, rest));
        }
        if buf.len() > HEAD_LIMIT {
            bail!("request head over {HEAD_LIMIT} bytes");
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            bail!("the connection closed before the request head ended");
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}
```

In `crates/mkrew-cli/src/main.rs`, add the variant after `Hook { … }` in `enum Command`:

```rust
    /// Serve the phone page and relay it to the daemon; publish it with `tailscale serve`.
    Web {
        /// Address to listen on; keep 127.0.0.1 and let `tailscale serve` publish it.
        #[arg(long, default_value = "127.0.0.1:8787")]
        listen: std::net::SocketAddr,
        /// Host name in the printed link (default: this PC's Tailscale name).
        #[arg(long)]
        host: Option<String>,
        /// Replace the access token; every link printed before stops working.
        #[arg(long)]
        new_token: bool,
    },
```

and in `fn run`, before `Command::Hook { .. } | Command::Integration { .. } => unreachable!(…)`:

```rust
        Command::Web {
            listen,
            host,
            new_token,
        } => web::run(
            &paths,
            web::Args {
                listen,
                host,
                new_token,
            },
        ),
```

- [ ] **Step 8: Run the unit tests**

Run: `CARGO_TARGET_DIR=…/mkrew/target cargo test -p mkrew-cli --bin mk web:: -j 4`
Expected: PASS — token (3), tailscale (2), http (6), relay (6) = 17 tests.

- [ ] **Step 9: Write the integration tests**

In `crates/mkrew-cli/tests/cli.rs`, add `use std::io::{BufRead, BufReader};` and `use std::net::TcpStream;` to the imports, then append:

```rust
/// `mk web` on a free port, with its address and token read from stdout; killed on drop.
struct Web {
    child: Child,
    port: u16,
    token: String,
}

impl Env {
    /// Starts `mk web` (which starts the daemon when none runs). `--host` keeps `tailscale` out.
    fn web(&self) -> Web {
        let mut child = self
            .command(&["web", "--listen", "127.0.0.1:0", "--host", "test.invalid"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let (mut port, mut token) = (None, None);
        while port.is_none() || token.is_none() {
            let line = lines.next().expect("mk web ended before printing its address").unwrap();
            if let Some(addr) = line.strip_prefix("Mkrew web is listening on http://") {
                port = Some(addr.rsplit(':').next().unwrap().parse().unwrap());
            }
            if let Some((_, t)) = line.split_once("#t=") {
                token = Some(t.trim().to_owned());
            }
        }
        // Keep reading so a later line never blocks `mk web` on a full pipe.
        std::thread::spawn(move || lines.for_each(drop));
        Web {
            child,
            port: port.unwrap(),
            token: token.unwrap(),
        }
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Web {
    /// `GET target` → (status, body).
    fn get(&self, target: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let text = String::from_utf8_lossy(&response).into_owned();
        let status = text[9..12].parse().unwrap();
        let body = text.split_once("\r\n\r\n").map(|(_, body)| body.to_owned()).unwrap_or_default();
        (status, body)
    }

    fn socket(&self, token: &str) -> Result<tungstenite::WebSocket<TcpStream>, tungstenite::Error> {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let url = format!("ws://127.0.0.1:{}/ws?t={token}", self.port);
        tungstenite::client(url, stream).map(|(ws, _)| ws).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => e,
            tungstenite::HandshakeError::Interrupted(_) => unreachable!("blocking stream"),
        })
    }
}

/// Reads text messages until one matches.
fn ws_until(ws: &mut tungstenite::WebSocket<TcpStream>, mut wanted: impl FnMut(&serde_json::Value) -> bool) -> serde_json::Value {
    loop {
        if let tungstenite::Message::Text(text) = ws.read().unwrap() {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if wanted(&value) {
                return value;
            }
        }
    }
}

fn ws_request(ws: &mut tungstenite::WebSocket<TcpStream>, id: u64, request: mkrew_protocol::Request) -> serde_json::Value {
    let text = serde_json::to_string(&mkrew_protocol::Message::Request { id, request }).unwrap();
    ws.send(tungstenite::Message::text(text)).unwrap();
    ws_until(ws, |v| v["type"] == "response" && v["id"] == id)
}

#[test]
fn web_serves_the_page_and_relays_a_session() {
    use mkrew_protocol::{Request, SessionInputParams, SessionListParams, SessionRef, SessionResizeParams};
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    // No `mk daemon start`: `mk web` starts the daemon itself.
    let web = env.web();
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let id = env.ok(&["session", "spawn", "demo", "--agent", "fake"]).trim().to_owned();
    let session_id: Ulid = id.parse().unwrap();

    let (status, page) = web.get("/");
    assert_eq!(status, 200);
    assert!(page.contains("<title>Mkrew</title>"), "{page}");
    for asset in ["/app.mjs", "/model.mjs", "/app.css", "/vendor/xterm.mjs", "/vendor/xterm.css"] {
        assert_eq!(web.get(asset).0, 200, "{asset}");
    }
    assert_eq!(web.get("/nope").0, 404);
    // Without the token nothing reaches the daemon.
    let wrong = "0".repeat(64);
    assert_eq!(web.get("/check").0, 401);
    assert_eq!(web.get(&format!("/check?t={wrong}")).0, 401);
    assert_eq!(web.get(&format!("/check?t={}", web.token)).0, 204);
    match web.socket(&wrong) {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 401),
        other => panic!("{other:?}"),
    }

    let mut ws = web.socket(&web.token).unwrap();
    // A request without params and one with params.
    let projects = ws_request(&mut ws, 1, Request::ProjectList);
    assert_eq!(projects["result"][0]["name"], "demo", "{projects}");
    let sessions = ws_request(&mut ws, 2, Request::SessionList(SessionListParams::default()));
    assert!(sessions["result"].as_array().unwrap().iter().any(|s| s["id"] == id.as_str()), "{sessions}");
    // The phone never resizes a session.
    let resize = ws_request(
        &mut ws,
        3,
        Request::SessionResize(SessionResizeParams { session_id, cols: 40, rows: 20 }),
    );
    assert_eq!(resize["error"]["code"], "invalid_request", "{resize}");
    // A bad message gets an answer and the connection keeps working.
    ws.send(tungstenite::Message::text(r#"{"type":"request","id":4,"method":"no.such"}"#)).unwrap();
    let broken = ws_until(&mut ws, |v| v["id"] == 4);
    assert_eq!(broken["error"]["code"], "invalid_request", "{broken}");

    // Attach: the reply carries the painted screen; output comes behind the session id.
    let attach = mkrew_protocol::Message::Request { id: 5, request: Request::SessionAttach(SessionRef { session_id }) };
    ws.send(tungstenite::Message::text(serde_json::to_string(&attach).unwrap())).unwrap();
    let mut screen = String::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !screen.contains("(y/n)") {
        assert!(Instant::now() < deadline, "no question on screen: {screen}");
        match ws.read().unwrap() {
            tungstenite::Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["id"] == 5 {
                    screen.push_str(value["result"]["ansi"].as_str().expect("attach reply has ansi"));
                }
            }
            tungstenite::Message::Binary(bytes) => {
                assert_eq!(&bytes[..26], id.as_bytes(), "output of another session");
                screen.push_str(&String::from_utf8_lossy(&bytes[26..]));
            }
            _ => {}
        }
    }
    // Answer as the `y` and Enter quick keys do; the exit reaches the page.
    let input = ws_request(
        &mut ws,
        6,
        Request::SessionInput(SessionInputParams { session_id, data: b"y\r".to_vec() }),
    );
    assert!(input.get("error").is_none(), "{input}");
    let exited = ws_until(&mut ws, |v| v["event"] == "session.exited" && v["data"]["session_id"] == id.as_str());
    assert_eq!(exited["data"]["exit_code"], 0, "{exited}");
}

#[test]
fn web_says_why_when_the_daemon_is_gone() {
    let env = Env::new();
    let web = env.web();
    env.ok(&["daemon", "stop"]);
    let mut ws = web.socket(&web.token).unwrap();
    match ws.read() {
        Ok(tungstenite::Message::Close(Some(frame))) => {
            assert!(frame.reason.as_str().contains("not running"), "{frame:?}")
        }
        other => panic!("{other:?}"),
    }
}
```

- [ ] **Step 10: Run the integration tests**

Run: `CARGO_TARGET_DIR=…/mkrew/target cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4 && CARGO_TARGET_DIR=…/mkrew/target cargo test -p mkrew-cli --test cli web_ -j 4`
Expected: PASS, 2 tests. (If `daemon stop` returns before the pipe is gone and the second test sees a greeting instead of a close, the message is the real bug to report — do not add sleeps.)

- [ ] **Step 11: ADR, SPEC, README, HUMAN_CHECKS, PROGRESS**

Run `ls docs/decisions`; NNNN = next free number ≥ 0046. Replace every `NNNN` in the files of Tasks 1–3 (`grep -rn NNNN crates/mkrew-cli docs`).

Create `docs/decisions/NNNN-mobile-web.md`:

```markdown
# NNNN — Mkrew web: watch and answer sessions from a phone over the tailnet

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request: an app on the phone to keep work
  moving while away from the PC)

## Context

- SPEC §1 listed "No mobile app" and §15 "no network listeners". The user wants to see which
  sessions need them, answer agents and start new ones from an iPhone, and already uses
  Tailscale.
- The daemon owns all state and every needed action exists in §7 (`project.list`,
  `agent.profiles`, `session.list/attach/detach/input/send/spawn`, `task.create/run`).
- omp's `/collab` covers omp sessions only, one link per session, and cannot start sessions.

## Options

1. A native iOS app: needs a Mac, signing, the App Store or 7-day sideloading; a second UI stack.
2. A TCP listener in the daemon: a network surface in the process that owns every agent.
3. `mk web`: a command the user starts, serving a small page and relaying its WebSocket to the
   daemon's local socket; `tailscale serve` publishes it with HTTPS inside the tailnet. (Chosen.)

## Decision

- `mk web [--listen 127.0.0.1:8787] [--host NAME] [--new-token]` (§9) starts the daemon if none
  runs, prints `https://<MagicDNS name>/#t=<token>` and serves `index.html`, `app.css`,
  `app.mjs`, `model.mjs` and xterm.js 6 (vendored, MIT) from the binary.
- Token: 32 random bytes as hex in `<config>/web_token` (owner-only on Unix), required by
  `/check` and `/ws`; `--new-token` replaces it. It travels in the URL fragment of the printed
  link, so it is not in any request line except the page's own `/check` and `/ws` calls.
- Relay: one async daemon connection per WebSocket; `mk web` sends `hello` itself and refuses
  `hello` and `session.resize` from the browser; responses and `session.snapshot` events with a
  snapshot gain `ansi` (`mkrew_protocol::paint`); output frames go out as binary messages
  (26-character session id + PTY bytes); at most 64 messages wait for a slow browser.
- The page (mockups in `docs/design/mobile-web/`): Sessions ("Needs you" + one group per
  workspace), Session (terminal scaled to fit, quick keys Enter/Esc/↑/↓/Tab/y/n/Ctrl+C, reply
  box = `session.send`), New agent (agent + prompt → task + run; shell → spawn + send), token
  screen, reconnect banner. It never resizes a session.

## Consequences

- SPEC §1, §9, §15, §19 and the §20 intro change. No protocol version change, no migration.
- Whoever has the token and reaches the port controls the agents: keep `--listen` on
  127.0.0.1, publish only with `tailscale serve` (never Funnel); `--new-token` revokes links.
- `mk web` must keep running on the PC (README: Task Scheduler at logon).
- Not done: push notifications, several PCs, board/files/git/docs on the phone.
```

`docs/SPEC.md` edits:
- §1 Non-goals, replace `- No mobile app, cloud sync or user accounts.` with
  `- No native mobile app, cloud sync or user accounts. A browser on the user's own tailnet can watch and answer sessions through \`mk web\` (ADR NNNN).`
- §9 code block, add after the `mk attach <session-id>` line:
  `mk web [--listen ADDR] [--host NAME] [--new-token]  # phone page over the tailnet (ADR NNNN)`
- §15 Security row, replace `IPC endpoint accessible by the current user only; no network listeners;` with
  `IPC endpoint accessible by the current user only; the daemon has no network listener (\`mk web\`, started by the user, listens on 127.0.0.1 by default and requires its token on every WebSocket, ADR NNNN);`
- §19 backlog, replace `GitHub (PR/issue → task); remote control via web/phone; money` with `GitHub (PR/issue → task); money`.
- §20 intro, replace `no mobile app.` (end of the sentence starting `§1 non-goals still hold`) with `no native mobile app (a browser client exists, ADR NNNN).`

`README.md`: add before `## Uninstall`:

```markdown
## Use Mkrew from your phone

`mk web` serves a small page for your phone (or any browser) to see which sessions need you,
answer them and start new agents, while the agents keep running on this PC. It uses
[Tailscale](https://tailscale.com) (the free Personal plan is enough) so nothing is open to the
internet.

1. Install Tailscale on this PC and on the phone, signed in to the same account. In the
   Tailscale admin console, under DNS, turn on MagicDNS and HTTPS Certificates.
2. On this PC run `tailscale serve --bg 8787` once (it is remembered across reboots).
3. Run `mk web`. It prints a link like `https://my-pc.tail1234.ts.net/#t=…`. Open it on the
   phone (send it to yourself, e.g. in Notes). On iPhone: Share → Add to Home Screen.
4. Keep `mk web` running: e.g. Windows Task Scheduler → Create Task → trigger "At log on",
   action `mk.exe web`. It starts the daemon if none runs.

The phone never changes a session's size; the terminal font shrinks to fit (pinch to zoom).
Lost the phone or shared the link by mistake? Run `mk web --new-token`: old links stop working.
```

`docs/HUMAN_CHECKS.md`: add under `## To verify (non-blocking)` (above the newest section):

```markdown
### To verify — 2026-10-10 Mkrew web on a phone (plan docs/superpowers/plans/2026-10-10-mobile-web.md)

- [ ] **Link:** `tailscale serve --bg 8787`, then `mk web` prints `https://<pc>.<tailnet>.ts.net/#t=…`.
  Opening it on the iPhone (Safari) shows Sessions with "Needs you" on top, the PC name next to
  a green dot. Add to Home Screen, then open the icon: it connects without asking for the token.
- [ ] **Answer:** a Claude/Codex session asks a question on the PC → it shows under "Needs you"
  within a few seconds. Tap it: the terminal shows the question at the session's own width
  (small font, pinch-zoom works); the desktop pane keeps its size. Tap `y` / Enter or type a
  reply and Send: the agent continues; the keyboard stays open between quick keys.
- [ ] **New agent:** + New agent → pick a workspace and Claude, type a prompt, Start → the
  session opens on the phone and a task card shows on the desktop board; an uninstalled agent
  is greyed "not installed".
- [ ] **Disconnect:** stop `mk web` → within seconds the banner "Can't reach Mkrew on <pc>.
  Retrying…" shows; start it again → the banner goes and an open session repaints.
- [ ] **Token:** `mk web --new-token` → the old Home Screen icon shows the token screen with
  "That token doesn't work…"; pasting the new link (or token) connects.
- [ ] **Light and dark:** switch iOS appearance; colors follow it, the terminal stays dark.
- [ ] **Chrome on another computer in the tailnet:** the same link works; Ctrl+Enter sends.
```

`docs/PROGRESS.md` "Notes for the next session": add

```markdown
- 2026-10-10 Mkrew web (plan docs/superpowers/plans/2026-10-10-mobile-web.md, ADR NNNN): `mk web` (crates/mkrew-cli/src/web/, page in crates/mkrew-cli/web/, xterm.js 6 vendored) relays a browser on the tailnet to the daemon; no protocol change. JS helpers: `node --test crates/mkrew-cli/web/model.test.mjs`. Manual checks in HUMAN_CHECKS "2026-10-10 Mkrew web". Outside the milestone loop.
```

- [ ] **Step 12: Browser smoke test (real page, fake agent)**

In an isolated environment (never the user's real config):

```bash
export MKREW_DATA_DIR="$TEMP/mkweb-smoke/data" MKREW_CONFIG_DIR="$TEMP/mkweb-smoke/config" MKREW_SOCKET='\\.\pipe\mkrew-web-smoke' MKREW_NOTIFY=off
mkdir -p "$MKREW_CONFIG_DIR/agents" "$TEMP/mkweb-smoke/repo"
```

Write `$MKREW_CONFIG_DIR/agents/fake.toml` = `agents/fake.toml` with `command` set to the absolute path of `target/debug/mkrew-fakeagent.exe` (as `fake_profile()` in `cli.rs` does). Start `target/debug/mk.exe web --listen 127.0.0.1:8799 --host localhost` as a named background service (ready log `listening`), then `mk project add "$TEMP/mkweb-smoke/repo" --name demo` and `mk session spawn demo --agent fake` with the same env.

With the `browser` tool (read `xd://eval/browser` first), set a 390×844 viewport and open `http://127.0.0.1:8799/#t=<token from the service log>`. Check, with a screenshot each, in dark and light (`prefers-color-scheme` emulation):
1. Sessions: the fake session under "Needs you" once it asks; the PC label reads `127`; green dot.
2. Tap it: the terminal shows `Do you want to proceed? (y/n)`; tap `y` then `Enter` → `Done.` appears, the footer reads `Session ended (exit 0)`.
3. Back → New agent → demo + Fake agent, prompt empty → Start → a new session opens.
4. Open `http://127.0.0.1:8799/` without `#t=` in a fresh context → token screen; paste a wrong 64-hex token → "That token doesn't work…"; paste the right link → Sessions.
5. Stop the `mk web` service → banner "Can't reach Mkrew on 127.0.0.1. Retrying…" (the label is the whole IP); restart it → banner gone.

Close the tab, stop the service, `mk daemon stop` (same env), delete `$TEMP/mkweb-smoke`. Fix anything that fails in the files of Tasks 2–3 and re-run the affected tests.

- [ ] **Step 13: Full verify and commit**

```bash
cargo fmt --all
CARGO_TARGET_DIR=…/mkrew/target sh scripts/verify.sh
node --test crates/mkrew-cli/web/model.test.mjs
git add crates/mkrew-cli/Cargo.toml Cargo.lock crates/mkrew-cli/src/main.rs crates/mkrew-cli/src/web.rs crates/mkrew-cli/src/web/token.rs crates/mkrew-cli/src/web/http.rs crates/mkrew-cli/src/web/relay.rs crates/mkrew-cli/src/web/tailscale.rs crates/mkrew-cli/tests/cli.rs crates/mkrew-cli/web docs/decisions/NNNN-mobile-web.md docs/SPEC.md README.md docs/HUMAN_CHECKS.md docs/PROGRESS.md docs/superpowers/specs/2026-10-10-mobile-web-design.md docs/superpowers/plans/2026-10-10-mobile-web.md docs/design/mobile-web
git commit -m "feat(cli): mk web serves the phone page and relays it to the daemon (ADR NNNN)"
```

(`NNNN` in the paths and message is the real number by now. `crates/mkrew-cli/web` is staged again only if Step 12 changed it.)
