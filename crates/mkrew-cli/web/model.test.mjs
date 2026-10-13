// Run: node --test crates/mkrew-cli/web/model.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_REPLIES, ago, backoff, buildList, chipFor, displayName, endedAt, esc, fitFontSize,
  keepSession, keyBytes, loadReplies, machineName, metaText, newAgentPlan, pillText, readerText,
  rememberReply, statusLabel, taskTitle, toBase64, tokenFromHash, tokenFromText, wsClass,
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

test('ended sessions stay listed for 24 h after they end, or while the page already shows them', () => {
  const now = 48 * 3_600_000;
  const ended = (at, extra = {}) => session('s', 'p', { state: 'exited', ended_at: at, ...extra });
  assert.equal(keepSession(session('s', 'p'), false, now), true);
  assert.equal(keepSession(ended(now - 3_600_000), false, now), true);
  assert.equal(keepSession(ended(now - 25 * 3_600_000), false, now), false);
  assert.equal(keepSession(ended(now - 25 * 3_600_000), true, now), true);
  // No end time recorded: the last state change counts.
  assert.equal(endedAt(ended(null, { state_changed_at: 7 })), 7);
});

test('ended sessions sit below the live ones and show how long ago they ended', () => {
  const { groups } = buildList([project('a', 'w')], [
    session('1', 'a', { state: 'exited', started_at: 0, pinned: true }),
    session('2', 'a', { started_at: 5 }),
  ]);
  assert.deepEqual(groups[0].sessions.map((s) => s.id), ['2', '1']);
  const now = 3 * 3_600_000;
  const ended = session('s', 'p', { state: 'exited', ended_at: 0, state_changed_at: now });
  assert.equal(metaText(ended, now), '3h');
  assert.equal(pillText(ended, now), 'Exited · 3h');
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

test('recent replies: newest first, no repeats, at most 5; blank, long or multi-line text is not kept', () => {
  assert.deepEqual(rememberReply(['a', 'b'], ' b '), ['b', 'a']);
  assert.deepEqual(rememberReply(['1', '2', '3', '4', '5'], '6'), ['6', '1', '2', '3', '4']);
  assert.deepEqual(rememberReply(['a'], '   '), ['a']);
  assert.deepEqual(rememberReply(['a'], 'x'.repeat(81)), ['a']);
  assert.deepEqual(rememberReply(['a'], 'two\nlines'), ['a']);
});

test('stored replies: missing or broken data gives the defaults; junk entries are dropped', () => {
  assert.deepEqual(loadReplies(null), DEFAULT_REPLIES);
  assert.deepEqual(loadReplies('{oops'), DEFAULT_REPLIES);
  assert.deepEqual(loadReplies('{"a":1}'), DEFAULT_REPLIES);
  assert.deepEqual(loadReplies('["ok", 3, "  "]'), ['ok']);
  assert.deepEqual(loadReplies('[3, "  "]'), DEFAULT_REPLIES);
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

test('read view joins soft-wrapped rows, tames boxes and squeezes blank rows', () => {
  const row = (text, wrapped = false) => ({ text, wrapped });
  const rows = [
    row(''), row('╭' + '─'.repeat(100) + '╮'),
    row('│ a long line that the terminal wr'), row('apped                    │', true),
    row('│   indented │ cell  │'),
    row(''), row(''), row(''),
    row('  indented  code'),
    row(''), row(''),
  ];
  assert.equal(readerText(rows), '╭───╮\na long line that the terminal wrapped\n  indented │ cell\n\n  indented  code');
  assert.equal(readerText([row(''), row('│   │')]), '');
  // A continuation row with no row before it starts the text.
  assert.equal(readerText([row('tail', true)]), 'tail');
});
