// Mkrew web (ADR 0052): the phone/browser client. Talks SPEC §7 JSON to the daemon through
// `mk web`; never resizes a session (the desktop pane keeps its size, the font shrinks here).
import { Terminal } from './vendor/xterm.mjs';
import {
  backoff, buildList, chipFor, displayName, esc, fitFontSize, fromBase64Url, keyBytes, loadReplies,
  machineName, metaText, newAgentPlan, pillText, pushOffer, readerText, rememberReply, sameBytes,
  sessionFromHash, statusLabel, toBase64, tokenFromHash, tokenFromText, wsClass,
} from './model.mjs';

const $ = (id) => document.getElementById(id);
// iOS 27 Home Screen app: `display-mode: standalone` is false there, so app.css keys the notch/blur gap off this class.
if (navigator.standalone === true) document.documentElement.classList.add('standalone');
const TOKEN_KEY = 'mkrew.token';
const VIEW_KEY = 'mkrew.view';
/** Session views, in the order the view button cycles them. */
const VIEWS = { fit: 'Fit', zoom: 'Zoom', read: 'Read' };
const REPLIES_KEY = 'mkrew.replies';
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
  /** The ended session whose `session.history` reply is still awaited. */
  replaying: null,
  /** A session a tapped notification named (`#s=` or the service worker); opened once the list loads. */
  wantOpen: null,
  replies: loadReplies(localStorage.getItem(REPLIES_KEY)),
};
const machine = machineName(location.hostname);
const idDecoder = new TextDecoder();
let term = null;
let toastTimer = 0;
/** Phone notifications: does the PC offer them, and is this phone registered (ADR 0067). */
const push = { enabledByPc: false, subscribed: false, refused: '' };

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
  setTokenError(error);
  $('token-input').value = '';
}

const TOKEN_ERROR = "That token didn't work. Check it and try again.";

function setTokenError(text) {
  const el = $('token-error');
  el.textContent = '';
  if (!text) return;
  const icon = document.createElement('span');
  icon.className = 'x';
  icon.setAttribute('aria-hidden', 'true');
  icon.textContent = '✕';
  el.append(icon, text);
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
    showToken(TOKEN_ERROR);
    return;
  }
  if (!res.ok) {
    retry();
    return;
  }
  linkManifest();
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
  try {
    await load();
    // Only a connection that answers counts: backoff and the banner keep going otherwise.
    state.attempt = 0;
    state.everConnected = true;
    setConnected(true);
    if (state.open && state.sessions.get(state.open)?.state !== 'exited') await attach(state.open);
    await openWanted();
    initPush();
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
  // Ended sessions are not listed; the open one stays (even when it ended while this page was
  // disconnected) so its screen shows why it ended instead of freezing.
  const next = new Map(sessions.filter((s) => s.state !== 'exited' || s.id === state.open).map((s) => [s.id, s]));
  const open = state.sessions.get(state.open);
  if (open && !next.has(open.id)) next.set(open.id, open);
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
      s.ended_at = Date.now();
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

// ---- phone notifications (ADR 0067) ----

/** The Home Screen app is built from this manifest; it carries the token (see `http.rs`). */
function linkManifest() {
  let link = document.querySelector('link[rel="manifest"]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'manifest';
    document.head.append(link);
  }
  link.href = `/manifest.webmanifest?t=${state.token}`; // a token entered again must replace the old one
}

function pushSupported() {
  return 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window;
}

function pushInfo() {
  return {
    enabledByPc: push.enabledByPc,
    secure: window.isSecureContext,
    supported: pushSupported(),
    standalone: navigator.standalone === true || matchMedia('(display-mode: standalone)').matches,
    ios: /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.maxTouchPoints > 1 && /Macintosh/.test(navigator.userAgent)),
    permission: 'Notification' in window ? Notification.permission : 'default',
    subscribed: push.subscribed,
    refused: push.refused,
  };
}

function renderPush() {
  const offer = pushOffer(pushInfo());
  const bell = $('push');
  bell.hidden = !offer.show;
  bell.setAttribute('aria-pressed', String(Boolean(offer.on)));
  bell.title = offer.hint ?? '';
}

async function currentSubscription() {
  if (!pushSupported()) return null;
  const reg = await navigator.serviceWorker.getRegistration('/');
  return reg ? reg.pushManager.getSubscription() : null;
}

/** The PC's VAPID public key as bytes; `null` while `[web] push` is off there. */
async function pcKey() {
  const res = await fetch(`/push/key?t=${state.token}`, { cache: 'no-store' });
  return res.ok ? fromBase64Url(await res.text()) : null;
}

/** After each connect: does the PC send notifications, and is this phone still registered there? */
async function initPush() {
  try {
    const key = await pcKey();
    push.enabledByPc = key !== null;
    push.subscribed = false;
    push.refused = '';
    if (key && pushSupported() && Notification.permission === 'granted') {
      const sub = await currentSubscription();
      if (sub && sameBytes(new Uint8Array(sub.options.applicationServerKey ?? []), key)) {
        // The PC may have forgotten this phone (`mk web --new-token`): tell it again. A refusal
        // (too many phones, store unavailable) keeps the bell, so the user can retry or turn off.
        try {
          await sendSubscription(sub);
          push.subscribed = true;
        } catch (err) {
          push.refused = err.message;
        }
      } else if (sub) {
        // Made for another VAPID key: it can never be pushed to. The bell offers a new one.
        await sub.unsubscribe();
      }
    }
  } catch {
    // Offline or old `mk web` without push: no bell.
    push.enabledByPc = false;
  }
  renderPush();
}

async function sendSubscription(sub) {
  const res = await fetch(`/push/subscribe?t=${state.token}`, { method: 'POST', body: JSON.stringify(sub) });
  if (!res.ok) throw new Error((await res.text()).trim() || 'The PC refused the notification setup.');
}

async function enablePush() {
  // Asked first, straight from the tap: iOS only shows the prompt for a user gesture.
  if ((await Notification.requestPermission()) !== 'granted') throw new Error('Notifications were not allowed.');
  const key = await pcKey();
  if (!key) throw new Error(`Phone notifications are off on ${machine}.`);
  const reg = await navigator.serviceWorker.register('/sw.js');
  await navigator.serviceWorker.ready;
  const sub = (await reg.pushManager.getSubscription())
    ?? (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key }));
  try {
    await sendSubscription(sub);
  } catch (err) {
    await sub.unsubscribe(); // do not keep a subscription the PC will never push to
    throw err;
  }
  push.subscribed = true;
  push.refused = '';
}

async function disablePush() {
  const sub = await currentSubscription();
  if (sub) {
    await fetch(`/push/unsubscribe?t=${state.token}`, { method: 'POST', body: JSON.stringify({ endpoint: sub.endpoint }) }).catch(() => {});
    await sub.unsubscribe();
  }
  push.subscribed = false;
}

async function togglePush() {
  const offer = pushOffer(pushInfo());
  if (!offer.usable) {
    if (offer.hint) toast(offer.hint);
    return;
  }
  try {
    if (push.subscribed) await disablePush();
    else await enablePush();
    toast(push.subscribed ? 'You will get a notification when an agent needs you.' : 'Notifications are off on this phone.');
  } catch (err) {
    toast(err.message);
  }
  renderPush();
}

/** Opens the session a tapped notification named, once the list has loaded. */
async function openWanted() {
  const id = state.wantOpen;
  if (!id) return;
  state.wantOpen = null;
  if (state.sessions.has(id) && state.open !== id) await openSession(id);
}

function openFromNotice(id) {
  state.wantOpen = id;
  if (state.loaded) openWanted();
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
    `<span class="title">${esc(displayName(s, state.profiles))}</span>` +
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
  $('stop').hidden = ended;
  $('ended-text').textContent = s.exit_code == null ? 'Session ended' : `Session ended (exit ${s.exit_code})`;
}

// ---- session screen ----

function ensureTerminal() {
  if (term) return;
  term = new Terminal({
    disableStdin: true,
    cursorBlink: false,
    // First: its unicode-range limits it to icon code points, and a later font may claim those as tofu.
    fontFamily: '"Symbols Nerd Font Mono", "SF Mono", Menlo, "Cascadia Mono", Consolas, monospace',
    fontSize: 8,
    // Like the desktop pane (SPEC §10); the attach itself brings at most the daemon's 2,000 lines.
    scrollback: 10000,
    theme: TERMINAL_THEME,
  });
  term.open($('term'));
  term.onWriteParsed(scheduleReader);
}

function currentView() {
  const view = localStorage.getItem(VIEW_KEY);
  return view in VIEWS ? view : 'fit';
}

function nextView() {
  const names = Object.keys(VIEWS);
  return names[(names.indexOf(currentView()) + 1) % names.length];
}

/** Fit / Zoom show the terminal; Read shows its text wrapped to the phone width. */
function applyView() {
  const view = currentView();
  $('view-label').textContent = VIEWS[view];
  $('view').setAttribute('aria-label', `View: ${VIEWS[view]}. Switch to ${VIEWS[nextView()]}`);
  $('term').hidden = view === 'read';
  $('reader').hidden = view !== 'read';
  if (view === 'read') {
    reader.stick = true;
    renderReader();
  } else if (term) {
    fitTerminal();
    term.refresh(0, term.rows - 1);
    term.scrollToBottom();
  }
}

/** Read view state: a pending re-render, and whether the next one jumps to the end. */
const reader = { timer: null, stick: true };

function scheduleReader() {
  if (reader.timer || $('reader').hidden) return;
  // ponytail: rebuilds the whole text per update (≤ 10,000 lines, at most 5×/s); diff rows if a phone lags.
  reader.timer = setTimeout(renderReader, 200);
}

function renderReader() {
  clearTimeout(reader.timer);
  reader.timer = null;
  const box = $('reader');
  if (!term || box.hidden) return;
  const buffer = term.buffer.active;
  const lines = [];
  for (let i = 0; i < buffer.length; i++) {
    const line = buffer.getLine(i);
    if (line) lines.push({ text: line.translateToString(true), wrapped: line.isWrapped });
  }
  const atEnd = reader.stick || box.scrollTop + box.clientHeight >= box.scrollHeight - 24;
  box.textContent = readerText(lines);
  if (atEnd) box.scrollTop = box.scrollHeight;
  reader.stick = false;
}

/** Scrolls `lines` (> 0 = back in time) like the desktop wheel (terminal/view.rs `scroll_wheel`):
 *  the history, or arrow keys for a full-screen program that does not take the mouse. */
function scrollTerminal(lines) {
  if (term.buffer.active.type === 'alternate' && term.modes.mouseTrackingMode === 'none') {
    if (!state.open || state.sessions.get(state.open)?.state === 'exited') return;
    const arrow = keyBytes(lines > 0 ? 'up' : 'down', term.modes.applicationCursorKeysMode);
    call('session.input', { session_id: state.open, data: toBase64(arrow.repeat(Math.abs(lines))) }).catch((err) => toast(err.message));
  } else {
    term.scrollLines(-lines);
  }
}

/** xterm 6 has no touch scrolling (its Gesture class is never attached): a one-finger
 *  vertical drag scrolls here; sideways panning stays native (`touch-action` in app.css). */
function touchScroll(box) {
  let lastY = null;
  let rest = 0;
  box.addEventListener('touchstart', (e) => {
    lastY = e.touches.length === 1 ? e.touches[0].clientY : null;
    rest = 0;
  }, { passive: true });
  box.addEventListener('touchmove', (e) => {
    const screen = box.querySelector('.xterm-screen');
    if (lastY === null || e.touches.length !== 1 || !term || !screen) return;
    const y = e.touches[0].clientY;
    const lines = (y - lastY) / (screen.offsetHeight / term.rows) + rest;
    lastY = y;
    const whole = Math.trunc(lines);
    rest = lines - whole;
    if (whole) scrollTerminal(whole);
  }, { passive: true });
}

/** Fit: shrinks the font until the session's columns fit the width (the session itself keeps
 *  its size). Zoom: the font that fits the session's rows in the height; the box pans
 *  sideways. */
function fitTerminal() {
  if (!term) return;
  const large = currentView() === 'zoom';
  const box = $('term');
  const width = box.clientWidth - 16;
  const height = box.clientHeight - 16;
  term.options.fontSize = large
    ? Math.min(24, Math.max(6, Math.floor((height / term.rows / 1.2) * 10) / 10))
    : fitFontSize(width, term.cols);
  // The first size is an estimate: scale it by what the browser laid out, then step down while
  // rounded cell sizes still overflow.
  const correct = (pass) => requestAnimationFrame(() => {
    const screen = box.querySelector('.xterm-screen');
    if (!screen || !(width > 0) || !(height > 0)) return;
    const ratio = large ? screen.offsetHeight / height : screen.offsetWidth / width;
    if (ratio <= 1 && !(pass === 0 && large && ratio < 0.97)) return;
    const size = term.options.fontSize;
    const next = Math.min(24, Math.max(4, pass === 0 ? Math.floor((size / ratio) * 10) / 10 : Math.round((size - 0.2) * 10) / 10));
    if (next === size) return;
    term.options.fontSize = next;
    if (pass < 6) correct(pass + 1);
  });
  correct(0);
}

async function openSession(id) {
  if (state.open && state.open !== id) detach(state.open);
  state.open = id;
  if (!history.state?.session) history.pushState({ session: id }, '', location.hash);
  show('session');
  ensureTerminal();
  term.reset();
  reader.stick = true;
  scheduleReader();
  $('reply').value = '';
  grow($('reply'));
  render();
  // An ended session cannot be attached: `mk web` replays the end of its log instead.
  try {
    if (state.sessions.get(id)?.state === 'exited') await replay(id);
    else await attach(id);
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

/** The newest output of an ended session: `mk web` sends it as output just before this reply. */
async function replay(id) {
  // A replay of this session is still on its way: its output will land in the fresh terminal.
  if (state.replaying === id) return;
  const s = state.sessions.get(id);
  term.resize(s.cols, s.rows);
  fitTerminal();
  state.replaying = id;
  try {
    await call('session.history', { session_id: id });
  } finally {
    if (state.replaying === id) state.replaying = null;
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

function renderReplies() {
  $('replies').innerHTML = state.replies
    .map((text, i) => `<button class="reply-chip" type="button" data-reply="${i}">${esc(text)}</button>`)
    .join('');
}

async function sendReply(text) {
  await call('session.send', { session_id: state.open, text, submit: true });
  state.replies = rememberReply(state.replies, text);
  try {
    localStorage.setItem(REPLIES_KEY, JSON.stringify(state.replies));
  } catch {
    // Storage full or off: the text was sent; the chips just are not kept.
  }
  renderReplies();
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

/** A Home Screen app's visual viewport on iOS 26 is short by the top safe area (WebKit bug
 *  301108), which left a gap under the composer: without the keyboard it fills the screen. */
function appHeight(vv) {
  const typing = document.activeElement?.matches('input, textarea');
  if (!navigator.standalone || typing) return vv.height;
  const [short, long] = [screen.width, screen.height].sort((a, b) => a - b);
  return innerWidth > innerHeight ? short : long;
}

function fitViewport() {
  const vv = window.visualViewport;
  const apply = () => {
    // Pinch-zoom also resizes the visual viewport; only the keyboard (scale 1) is handled.
    if (vv && vv.scale > 1.01) return;
    if (vv) {
      $('app').style.height = `${appHeight(vv)}px`;
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
      setTokenError(TOKEN_ERROR);
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
  $('push').addEventListener('click', togglePush);

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
      await sendReply(text);
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
  touchScroll($('term'));
  keepKeyboard($('replies'));
  $('replies').addEventListener('click', (e) => {
    const i = e.target.closest('[data-reply]')?.dataset.reply;
    if (i === undefined || !state.open) return;
    sendReply(state.replies[Number(i)]).catch((err) => toast(err.message));
  });
  $('keys').addEventListener('click', (e) => {
    const key = e.target.closest('[data-key]')?.dataset.key;
    if (!key || !state.open) return;
    const appCursor = term?.modes.applicationCursorKeysMode ?? false;
    call('session.input', { session_id: state.open, data: toBase64(keyBytes(key, appCursor)) }).catch((err) => toast(err.message));
  });
  $('view').addEventListener('click', () => {
    localStorage.setItem(VIEW_KEY, nextView());
    applyView();
  });
  applyView();

  $('stop').addEventListener('click', () => {
    const s = state.sessions.get(state.open);
    if (!s) return;
    if (!confirm(`Stop session "${displayName(s, state.profiles)}"? It ends on ${machine}.`)) return;
    call('session.kill', { session_id: s.id, force: false }).catch((err) => toast(err.message));
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
  if ('serviceWorker' in navigator) {
    // The service worker found this page already open: it sends the tapped session.
    navigator.serviceWorker.addEventListener('message', (e) => {
      if (e.data?.type === 'open-session' && sessionFromHash(`#s=${e.data.session}`)) openFromNotice(e.data.session);
    });
  }
}

function start() {
  // Before `tokenFromHash` and any `replaceState`: that rewrites the hash to `#t=…`.
  state.wantOpen = sessionFromHash(location.hash);
  const fromHash = tokenFromHash(location.hash);
  if (fromHash) localStorage.setItem(TOKEN_KEY, fromHash);
  state.token = fromHash ?? localStorage.getItem(TOKEN_KEY);
  // Keep `#t=` in the URL: "Add to Home Screen" bookmarks it, and iOS gives the Home Screen
  // app its own storage.
  if (state.token && !fromHash) history.replaceState(history.state, '', `#t=${state.token}`);
  renderReplies();
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
