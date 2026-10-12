// Pure helpers of the Mkrew web client (ADR 0052). No DOM; tested with `node --test`.

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

/** Chips before the user sent anything from the Reply box. */
export const DEFAULT_REPLIES = ['continue', 'yes, go ahead'];
const MAX_REPLIES = 5;
const MAX_REPLY_LEN = 80;

/** The chip list after sending `text`: newest first, no repeats; only short one-line replies. */
export function rememberReply(replies, text) {
  const reply = text.trim();
  if (!reply || reply.length > MAX_REPLY_LEN || reply.includes('\n')) return replies;
  return [reply, ...replies.filter((r) => r !== reply)].slice(0, MAX_REPLIES);
}

/** The chips stored in `localStorage` (JSON array of strings); anything else gives the defaults. */
export function loadReplies(raw) {
  try {
    const list = JSON.parse(raw);
    if (Array.isArray(list)) return list.filter((r) => typeof r === 'string' && r.trim()).slice(0, MAX_REPLIES);
  } catch {
    // Not JSON: fall through to the defaults.
  }
  return DEFAULT_REPLIES;
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
