# Mkrew web extras (icon, Stop, quick replies, ended sessions) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Four small additions to the phone page of `mk web`: a Home Screen icon, a Stop button,
recent replies as one-tap chips, and recently ended sessions with the end of their output.

**Architecture:** Everything except Task 4b is page-only (`crates/mkrew-cli/web/`). Task 4b teaches
the relay (`crates/mkrew-cli/src/web/relay.rs`) one thing: when the browser asks
`session.history`, the relay reads the newest log bytes (the same `read_tail` the desktop History
dialog uses, moved to `mkrew-core`) and sends them as an ordinary output message just before the
reply. No daemon change, no protocol change, no `PROTOCOL_VERSION` bump.

**Tech Stack:** Rust (tokio, tokio-tungstenite), vanilla ES modules + xterm.js 6, `node --test`.

**Spec:** `docs/superpowers/specs/2026-10-10-mobile-web-design.md` (the original design) and
ADR `docs/decisions/0052-mobile-web.md`. This plan carries the design of the four additions.

## Design decisions (agreed in chat on 2026-10-13)

| # | Feature | Decision |
|---|---|---|
| 1 | Home Screen icon | `apple-touch-icon.png` (180×180, the `icon/mkrew-1-lanes/tile-dark.svg` art drawn full-bleed: iOS rounds the corners itself and fills transparent ones black). Served by `mk web` like the other files; `<link rel="apple-touch-icon">` in `index.html`. No web manifest. |
| 2 | Stop | A Lucide `square` icon button in the session top bar (right of "Aa"), hidden for ended sessions. Native `confirm()` "Stop session "<name>"? …" → `session.kill {force:false}` (the daemon asks the process to end, then force-kills after its grace period). The `session.exited` event updates the screen. Wording follows the desktop (`inspector::STOP_ANSWERS` = "Stop session"). |
| 3 | Quick replies | A row of chips above the quick keys: the last 5 distinct one-line replies sent from the Reply box (newest first, at most 80 characters), stored in `localStorage` `mkrew.replies`; before anything is stored the row shows `continue` and `yes, go ahead`. Tapping a chip sends it at once (`session.send`, submit), like the quick keys. No editing UI. |
| 4 | Ended sessions | The list also keeps sessions that ended in the last 24 h (`ended_at`, else `state_changed_at`), below the live ones of their workspace, with "3h" (time since the end) as their meta and "Exited · 3h" as their pill. Opening one shows the newest 512 KiB of its output log (replayed into the terminal at the session's own size), read-only, with the existing "Session ended (exit N)" bar. |

## Global Constraints

- The daemon is unchanged; the browser keeps speaking SPEC §7 JSON; `mk web` keeps refusing `hello` and `session.resize`.
- The web client never resizes a session.
- Look: the existing `app.css` tokens (`--surface`, `--border`, `--text`, `--hover`, `--divider`, …); 44 px minimum touch targets; dark and light via `prefers-color-scheme`.
- No new dependency (Rust or JS).
- No `unwrap`/`expect` outside tests.
- No test calls a real agent or the network (the fake agent `mkrew-fakeagent` is fine).
- Shell: `bash` in omp is WSL. Run `cargo`, `git` and `node` directly; run scripts with `sh scripts/verify.sh`. Build with `-j 4`. Rebuild `mk` and `mkrew-fakeagent` before the `mk web` integration test (`cargo build -j 4 -p mkrew-cli -p mkrew-fakeagent`).
- Git: Conventional Commits; stage explicit paths. **Prerequisite:** `app.css`, `app.mjs` and `index.html` hold the user's uncommitted "Larger text" (`Aa`) change. Do not commit it as yours: if it is still uncommitted when you start, stage only your own hunks (`git add -p <file>`), and say so in the final report.

## Review Focus

1. **The `session.history` reply ends with an error** (session removed, store error): the relay must forget the id and pass the error through; the page toasts it. Covered by Task 4b `a_history_reply_names_the_files_once`.
2. **The log cannot be read** (file locked or permission denied): the page gets an error reply ("Cannot read the session log: …"), never a silent blank terminal. Covered by Task 4b (`daemon_to_browser` replaces the reply).
3. **Corrupt `localStorage` value** (`mkrew.replies` hand-edited, other JSON): the chips fall back to the defaults instead of crashing the page. Covered by Task 3 `loadReplies` test.
4. **A session that ends while the page shows it** keeps its terminal content and gets the "Exited · now" pill (no replay, no blank screen). Checked in the Task 5 browser smoke.
5. **Tapping Stop and then Cancel** sends nothing. Checked in the Task 5 browser smoke.

---

### Task 1: Home Screen icon

**Files:**
- Create: `crates/mkrew-cli/web/apple-touch-icon.png`
- Modify: `crates/mkrew-cli/src/web/http.rs` (const `PNG`, `ASSETS`, tests)
- Modify: `crates/mkrew-cli/web/index.html` (`<head>`)
- Modify: `crates/mkrew-cli/tests/cli.rs` (`web_serves_the_page_and_relays_a_session`, asset list)
- Modify: `docs/decisions/0052-mobile-web.md` (served files)

**Interfaces:**
- Produces: `GET /apple-touch-icon.png` → `200`, `Content-Type: image/png`, no token needed (static files are not secret, ADR 0052).

- [ ] **Step 1: Write the failing test** in `http.rs` `mod tests`:

```rust
    #[test]
    fn serves_the_home_screen_icon_as_png() {
        let Route::Asset(icon) = route(
            &head("GET /apple-touch-icon.png HTTP/1.1\r\n\r\n"),
            &token(),
        ) else {
            panic!("no icon");
        };
        assert_eq!(icon.content_type, "image/png");
        assert!(icon.body.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    }
```

- [ ] **Step 2: Run it, expect FAIL** ("no icon"): `cargo test -j 4 -p mkrew-cli --lib serves_the_home_screen_icon`

- [ ] **Step 3: Make the PNG.** No ImageMagick on this machine. Use the eval `browser` (read `xd://eval/browser` first): open `about:blank`, then run this in the tab with the SVG text as the argument:

```js
// svg = contents of icon/mkrew-1-lanes/tile-dark.svg with ` rx="56"` removed (full bleed)
async (svg) => {
  const img = new Image();
  img.src = 'data:image/svg+xml;base64,' + btoa(svg);
  await img.decode();
  const c = document.createElement('canvas');
  c.width = c.height = 180;
  c.getContext('2d').drawImage(img, 0, 0, 180, 180);
  return c.toDataURL('image/png');
}
```

Decode the base64 after `data:image/png;base64,` and write the bytes to `crates/mkrew-cli/web/apple-touch-icon.png`. Open the file with `read` (`:img`) and check that it shows the lanes glyph on the accent square with square corners.

- [ ] **Step 4: Serve it.** In `http.rs` add next to the other content types:

```rust
const PNG: &str = "image/png";
```

and append to `ASSETS`:

```rust
    Asset {
        path: "/apple-touch-icon.png",
        content_type: PNG,
        body: include_bytes!("../../web/apple-touch-icon.png"),
    },
```

In `index.html` `<head>`, after the `apple-mobile-web-app-title` meta:

```html
<link rel="apple-touch-icon" href="apple-touch-icon.png">
```

In `cli.rs` `web_serves_the_page_and_relays_a_session`, add `"/apple-touch-icon.png"` to the asset list that expects `200`.

- [ ] **Step 5: Run, expect PASS:** `cargo test -j 4 -p mkrew-cli --lib web::http` and (after `cargo build -j 4 -p mkrew-cli -p mkrew-fakeagent`) `cargo test -j 4 -p mkrew-cli --test cli web_`

- [ ] **Step 6: ADR.** In `docs/decisions/0052-mobile-web.md`, the bullet that lists the served files: add `apple-touch-icon.png` (the Home Screen icon, 180 px, from `icon/mkrew-1-lanes/tile-dark.svg`).

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-cli/web/apple-touch-icon.png crates/mkrew-cli/src/web/http.rs crates/mkrew-cli/tests/cli.rs docs/decisions/0052-mobile-web.md
git add -p crates/mkrew-cli/web/index.html
git commit -m "feat(web): Home Screen icon for the phone page"
```

---

### Task 2: Stop a session from the phone

**Files:**
- Modify: `crates/mkrew-cli/web/index.html` (session `topbar`)
- Modify: `crates/mkrew-cli/web/app.mjs` (`renderSession`, `wire`)

**Interfaces:**
- Consumes: `session.kill` `{session_id, force}` (SPEC §7; succeeds for an already exited session too).

No permanent test: it is one button wired to an existing request. The Task 5 browser smoke checks it.

- [ ] **Step 1: Button.** In `index.html`, in `<header class="topbar">` right after the `text-size` button:

```html
    <button class="icon-btn" id="stop" type="button" aria-label="Stop session"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="18" height="18" x="3" y="3" rx="2"/></svg></button>
```

(Path from `icon/lucide/square.svg`; check it matches that file.)

- [ ] **Step 2: Hide it once ended.** In `renderSession()`, after `$('ended').hidden = !ended;`:

```js
  $('stop').hidden = ended;
```

- [ ] **Step 3: Wire it.** In `wire()`, after the `text-size` listener:

```js
  $('stop').addEventListener('click', () => {
    const s = state.sessions.get(state.open);
    if (!s) return;
    if (!confirm(`Stop session "${displayName(s, state.profiles)}"? It ends on ${machine}.`)) return;
    call('session.kill', { session_id: s.id, force: false }).catch((err) => toast(err.message));
  });
```

- [ ] **Step 4: Run the JS tests** (unchanged, must still pass): `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 5: Commit** (own hunks only, see Global Constraints)

```bash
git add -p crates/mkrew-cli/web/index.html crates/mkrew-cli/web/app.mjs
git commit -m "feat(web): stop a session from the phone"
```

---

### Task 3: Quick replies

**Files:**
- Modify: `crates/mkrew-cli/web/model.mjs` (new exports)
- Modify: `crates/mkrew-cli/web/model.test.mjs`
- Modify: `crates/mkrew-cli/web/index.html` (composer)
- Modify: `crates/mkrew-cli/web/app.mjs` (state, composer submit, wire, new `renderReplies`)
- Modify: `crates/mkrew-cli/web/app.css` (`.replies`, `.reply-chip`)

**Interfaces:**
- Produces (model.mjs): `DEFAULT_REPLIES: string[]`, `rememberReply(replies: string[], text: string): string[]`, `loadReplies(raw: string | null): string[]`.

- [ ] **Step 1: Write the failing tests** in `model.test.mjs` (add `DEFAULT_REPLIES, loadReplies, rememberReply` to the import):

```js
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
});
```

- [ ] **Step 2: Run, expect FAIL** (import error): `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 3: Implement** in `model.mjs`, after `keyBytes`:

```js
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
```

- [ ] **Step 4: Run, expect PASS:** `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 5: Markup.** In `index.html`, inside `<form class="composer" id="composer">`, before `<div class="keys" …>`:

```html
    <div class="replies" id="replies" role="group" aria-label="Recent replies"></div>
```

- [ ] **Step 6: CSS.** In `app.css`, after the `.key:active` rule:

```css
.replies{display:flex;gap:6px;overflow-x:auto;scrollbar-width:none}
.replies::-webkit-scrollbar{display:none}
.replies:empty{display:none}
.reply-chip{flex:none;max-width:70vw;min-height:44px;padding:0 14px;border:1px solid var(--border);border-radius:999px;background:var(--surface);color:var(--text);font:500 14px/1 inherit;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.reply-chip:active{background:var(--hover)}
```

- [ ] **Step 7: App.** In `app.mjs`:

Import `loadReplies, rememberReply` from `./model.mjs`. Next to `TOKEN_KEY`:

```js
const REPLIES_KEY = 'mkrew.replies';
```

Add to `state`: `replies: loadReplies(localStorage.getItem(REPLIES_KEY)),`

Add after `grow()`:

```js
function renderReplies() {
  $('replies').innerHTML = state.replies
    .map((text, i) => `<button class="reply-chip" type="button" data-reply="${i}">${esc(text)}</button>`)
    .join('');
}

async function sendReply(text) {
  await call('session.send', { session_id: state.open, text, submit: true });
  state.replies = rememberReply(state.replies, text);
  localStorage.setItem(REPLIES_KEY, JSON.stringify(state.replies));
  renderReplies();
}
```

In the `composer` submit listener replace the `call('session.send', …)` line with `await sendReply(text);` (keep the `box.value = ''` / `grow(box)` lines and the `catch`).

In `wire()`, after `keepKeyboard($('send'));`:

```js
  keepKeyboard($('replies'));
  $('replies').addEventListener('click', (e) => {
    const i = e.target.closest('[data-reply]')?.dataset.reply;
    if (i === undefined || !state.open) return;
    sendReply(state.replies[Number(i)]).catch((err) => toast(err.message));
  });
```

In `start()`, call `renderReplies();` once before `wire()` runs (or at its top).

- [ ] **Step 8: Run the JS tests again, expect PASS:** `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-cli/web/model.mjs crates/mkrew-cli/web/model.test.mjs
git add -p crates/mkrew-cli/web/index.html crates/mkrew-cli/web/app.mjs crates/mkrew-cli/web/app.css
git commit -m "feat(web): recent replies as one-tap chips"
```

---

### Task 4a: Move `read_tail` to `mkrew-core`

**Files:**
- Create: `crates/mkrew-core/src/log_tail.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (`pub mod log_tail;`, alphabetical, after `lookup`)
- Modify: `crates/mkrew-ui/src/history.rs` (remove `Tail`, `read_tail` and the `read_tail_joins_the_parts_and_cuts_at_a_line_break` test; keep `TAIL_BYTES`)
- Modify: `crates/mkrew-ui/src/shell/history.rs:320` (call site)

**Interfaces:**
- Produces: `mkrew_core::log_tail::{Tail { bytes: Vec<u8>, cut: bool }, read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail>}`, same behavior as today.

Pure move, no behavior change; the moved test is the check.

- [ ] **Step 1: Create `log_tail.rs`** with a module doc (`//! The newest bytes of a session's output log (SPEC §8), read by the desktop History dialog and by \`mk web\` (ADR 0052).`), the `Tail` struct and `read_tail` copied verbatim from `crates/mkrew-ui/src/history.rs:167-205` with their imports (`std::fs::File`, `std::io::{self, Read, Seek, SeekFrom}`, `std::path::PathBuf`), and a `#[cfg(test)] mod tests` holding `read_tail_joins_the_parts_and_cuts_at_a_line_break` (from `history.rs:344-388`) with `TAIL_BYTES` replaced by `1024`.

- [ ] **Step 2: Remove them from `mkrew-ui/src/history.rs`**, drop imports that are now unused, and in `a_replayed_tail_shows_the_sessions_last_screen` call `mkrew_core::log_tail::read_tail`. In `shell/history.rs:320` replace `history::read_tail(` with `mkrew_core::log_tail::read_tail(`.

- [ ] **Step 3: Run:** `cargo test -j 4 -p mkrew-core log_tail` and `cargo test -j 4 -p mkrew-ui history` → PASS; `cargo clippy -j 4 -p mkrew-core -p mkrew-ui --all-targets -- -D warnings` → clean.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-core/src/log_tail.rs crates/mkrew-core/src/lib.rs crates/mkrew-ui/src/history.rs crates/mkrew-ui/src/shell/history.rs
git commit -m "refactor(core): move the session log tail reader to mkrew-core"
```

---

### Task 4b: The relay sends an ended session's log tail

**Files:**
- Modify: `crates/mkrew-cli/src/web/relay.rs` (`FromBrowser::Forward`, `daemon_request`, `relay`, `browser_to_daemon`, `daemon_to_browser`, new `Tails`/`TAIL_BYTES`/`wanted_tail`, module doc, tests)
- Modify: `crates/mkrew-cli/tests/cli.rs` (`web_serves_the_page_and_relays_a_session`)
- Modify: `docs/decisions/0052-mobile-web.md` (Relay bullet)

**Interfaces:**
- Consumes: `mkrew_core::log_tail::read_tail` (Task 4a); `mkrew_protocol::SessionHistory { files: Vec<PathBuf> }`.
- Produces (browser contract): after a `session.history` request, the browser receives **one binary output message** (26-character session id + the newest ≤ 512 KiB of the log) **and then** the normal JSON reply. If the log cannot be read, it receives only an error reply `internal` "Cannot read the session log: …".

- [ ] **Step 1: Write the failing unit tests** in `relay.rs` `mod tests` (add `use std::path::PathBuf;` there if needed):

```rust
    #[test]
    fn history_requests_ask_for_the_log_tail() {
        let session_id = Ulid::new();
        let text = format!(
            r#"{{"type":"request","id":9,"method":"session.history","params":{{"session_id":"{session_id}"}}}}"#
        );
        let FromBrowser::Forward { tail, .. } = daemon_request(&text) else {
            panic!("not forwarded");
        };
        assert_eq!(tail, Some((9, session_id)));
        let list = r#"{"type":"request","id":3,"method":"session.list","params":{}}"#;
        assert!(matches!(
            daemon_request(list),
            FromBrowser::Forward { tail: None, .. }
        ));
    }

    #[test]
    fn a_history_reply_names_the_files_once() {
        let session_id = Ulid::new();
        let tails = Tails::default();
        tails.lock().unwrap().extend([(9, session_id), (10, session_id)]);
        let reply = |id, outcome| Frame::Message(Message::Response { id, outcome });
        let files = serde_json::json!({ "files": ["/x/s.log"] });
        assert_eq!(
            wanted_tail(&reply(9, Outcome::Result(files.clone())), &tails),
            Some((9, session_id, vec![PathBuf::from("/x/s.log")]))
        );
        // Answered: a later reply with that id is passed through as is.
        assert_eq!(wanted_tail(&reply(9, Outcome::Result(files)), &tails), None);
        // An error reply is passed through, and its id forgotten.
        let gone = Outcome::error(ErrorCode::NotFound, "gone");
        assert_eq!(wanted_tail(&reply(10, gone), &tails), None);
        assert!(tails.lock().unwrap().is_empty());
    }
```

In `forwards_requests_as_frames` change the pattern to `let FromBrowser::Forward { frame: bytes, tail: None } = daemon_request(text) else {`.

- [ ] **Step 2: Run, expect compile FAIL:** `cargo test -j 4 -p mkrew-cli --lib web::relay`

- [ ] **Step 3: Implement.**

Imports: add `std::collections::HashMap`, `std::path::PathBuf`, `std::sync::{Arc, Mutex, PoisonError}`, `mkrew_core::log_tail::read_tail`, `mkrew_protocol::SessionHistory`, `ulid::Ulid` (`ulid = "1"` is already a dependency of `mkrew-cli`).

Constants and type, after `HELLO_ID`:

```rust
/// Newest log bytes the page replays for an ended session (the desktop History dialog shows 2 MiB).
const TAIL_BYTES: u64 = 512 * 1024;

/// Browser `session.history` requests waiting for their reply: id → session. The relay sends the
/// log's tail as output just before that reply, so the page can replay an ended session.
type Tails = Arc<Mutex<HashMap<u64, Ulid>>>;
```

`FromBrowser::Forward` becomes:

```rust
    /// An encoded §7 frame for the daemon; `tail` = (id, session) of a `session.history`
    /// request, whose reply gets the log's tail in front.
    Forward {
        frame: Vec<u8>,
        tail: Option<(u64, Ulid)>,
    },
```

In `daemon_request` replace the generic request arm with:

```rust
        Ok(message @ Message::Request { .. }) => {
            let tail = match &message {
                Message::Request {
                    id,
                    request: Request::SessionHistory(params),
                } => Some((*id, params.session_id)),
                _ => None,
            };
            encode(&Frame::Message(message))
                .map_or_else(|e| refuse(e.to_string()), |frame| FromBrowser::Forward { frame, tail })
        }
```

`relay()`: create `let tails = Tails::default();` and pass `Arc::clone(&tails)` to `daemon_to_browser` and `tails` to `browser_to_daemon`.

`browser_to_daemon(stream, tx, replies, tails: Tails)`; the forward arm records the id **before** writing (the reply cannot overtake it):

```rust
                FromBrowser::Forward { frame, tail } => {
                    if let Some((id, session_id)) = tail {
                        tails.lock().unwrap_or_else(PoisonError::into_inner).insert(id, session_id);
                    }
                    tx.write_all(&frame).await?;
                }
```

New helper, after `browser_message`:

```rust
/// The (id, session, log files) of a reply to a browser `session.history` request. Any reply
/// with a waiting id clears it; only a successful one asks for the tail.
fn wanted_tail(frame: &Frame, tails: &Tails) -> Option<(u64, Ulid, Vec<PathBuf>)> {
    let Frame::Message(Message::Response { id, outcome }) = frame else {
        return None;
    };
    let session_id = tails.lock().unwrap_or_else(PoisonError::into_inner).remove(id)?;
    let Outcome::Result(result) = outcome else {
        return None;
    };
    let history = SessionHistory::deserialize(result).ok()?;
    Some((*id, session_id, history.files))
}
```

`daemon_to_browser(rx, decoder, out, tails: Tails)`; the inner loop becomes:

```rust
        while let Some(mut frame) = decoder.next_frame()? {
            if let Some((id, session_id, files)) = wanted_tail(&frame, &tails) {
                match tokio::task::spawn_blocking(move || read_tail(&files, TAIL_BYTES)).await? {
                    Ok(tail) => {
                        let output = Frame::Output {
                            session_id,
                            data: tail.bytes,
                        };
                        out.send(browser_message(output)?).await?;
                    }
                    Err(e) => {
                        frame = Frame::Message(Message::Response {
                            id,
                            outcome: Outcome::error(
                                ErrorCode::Internal,
                                format!("Cannot read the session log: {e}"),
                            ),
                        });
                    }
                }
            }
            out.send(browser_message(frame)?).await?;
        }
```

Update the module doc (lines 1-5): add "and sends the newest log bytes of a session as output just before the reply to a `session.history` request (the page replays ended sessions)."

- [ ] **Step 4: Run, expect PASS:** `cargo test -j 4 -p mkrew-cli --lib web::relay`

- [ ] **Step 5: Integration test.** In `cli.rs` `web_serves_the_page_and_relays_a_session`, after the `session.exited` assertion at the end:

```rust
    // An ended session: its log comes back as output, before the `session.history` reply.
    let history = mkrew_protocol::Message::Request {
        id: 7,
        request: Request::SessionHistory(SessionRef { session_id }),
    };
    ws.send(tungstenite::Message::text(
        serde_json::to_string(&history).unwrap(),
    ))
    .unwrap();
    let mut replayed = Vec::new();
    loop {
        match ws.read().unwrap() {
            tungstenite::Message::Binary(bytes) => {
                assert_eq!(&bytes[..26], id.as_bytes(), "output of another session");
                replayed.extend_from_slice(&bytes[26..]);
            }
            tungstenite::Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["id"] == 7 {
                    assert!(value.get("error").is_none(), "{value}");
                    break;
                }
            }
            _ => {}
        }
    }
    let replayed = String::from_utf8_lossy(&replayed);
    assert!(replayed.contains("(y/n)"), "the log tail misses the question: {replayed}");
```

Run: `cargo build -j 4 -p mkrew-cli -p mkrew-fakeagent` then `cargo test -j 4 -p mkrew-cli --test cli web_` → PASS. If `replayed` is empty, the log may not be flushed at exit: use superpowers:systematic-debugging on the daemon's `session_log` flush; do not add a sleep.

- [ ] **Step 6: ADR.** In `docs/decisions/0052-mobile-web.md`, Relay bullet: add "; a reply to a browser `session.history` request is preceded by one output message holding the newest 512 KiB of the session's log (`mkrew_core::log_tail`), or replaced by an `internal` error when the log cannot be read (2026-10-13)".

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-cli/src/web/relay.rs crates/mkrew-cli/tests/cli.rs docs/decisions/0052-mobile-web.md
git commit -m "feat(web): relay the log tail of a session before its history reply"
```

---

### Task 4c: The page lists and replays recently ended sessions

**Files:**
- Modify: `crates/mkrew-cli/web/model.mjs` (`ENDED_SHOWN_MS`, `endedAt`, `keepSession`, `bySession`, `timed`/`metaText`/`pillText`)
- Modify: `crates/mkrew-cli/web/model.test.mjs`
- Modify: `crates/mkrew-cli/web/app.mjs` (`load`, `onEvent` `session.exited`, `openSession`, new `replay`)

**Interfaces:**
- Consumes: the Task 4b browser contract (binary tail, then the reply).
- Produces (model.mjs): `ENDED_SHOWN_MS = 86_400_000`, `endedAt(session): number`, `keepSession(session, known: boolean, now: number): boolean`.

- [ ] **Step 1: Write the failing tests** (add `endedAt, keepSession` to the import):

```js
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
```

- [ ] **Step 2: Run, expect FAIL:** `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 3: Implement in `model.mjs`.**

After `NEEDS`:

```js
/** How long an ended session stays listed on the phone. */
export const ENDED_SHOWN_MS = 24 * 3_600_000;

/** Unix ms the session ended; its last state change when no end time was recorded. */
export function endedAt(session) {
  return session.ended_at ?? session.state_changed_at;
}

/** Live sessions, ones this page already shows, and ones that ended in the last 24 h. */
export function keepSession(session, known, now) {
  return session.state !== 'exited' || known || endedAt(session) >= now - ENDED_SHOWN_MS;
}
```

`bySession` puts ended ones last:

```js
function bySession(a, b) {
  return (
    Number(a.state === 'exited') - Number(b.state === 'exited') ||
    Number(Boolean(b.pinned)) - Number(Boolean(a.pinned)) ||
    a.started_at - b.started_at
  );
}
```

Update the `buildList` doc comment: "…(pinned first, then oldest first; ended ones last)".

Times: replace `timed` and add a `since` helper used by `metaText` and `pillText`:

```js
function timed(session) {
  return session.state === 'working' || session.state === 'idle' || session.state === 'exited';
}

function since(session) {
  return session.state === 'exited' ? endedAt(session) : session.state_changed_at;
}
```

In `metaText` and `pillText` replace `session.state_changed_at` with `since(session)`. Update the `metaText` doc: "the time for working/idle (since the change) and exited (since the end), else the state in words."

- [ ] **Step 4: Run, expect PASS:** `node --test crates/mkrew-cli/web/model.test.mjs` (the existing "row meta and pill" test must still pass).

- [ ] **Step 5: App.** In `app.mjs`, import `keepSession`.

`load()`: replace the comment and the `const next = …` line with:

```js
  // Ended sessions: the ones of the last 24 h, plus any this page already knew (they ended
  // while it was disconnected), so the open one shows why it ended instead of freezing.
  const now = Date.now();
  const next = new Map(sessions.filter((s) => keepSession(s, state.sessions.has(s.id), now)).map((s) => [s.id, s]));
```

(keep the following `for … if (s.state === 'exited' && !next.has(id))` line.)

`onEvent` `session.exited`: after `s.exit_code = data.exit_code;` add `s.ended_at = Date.now();`

`openSession`: replace from the `// An ended session has nothing to attach to` comment to the end of the function with:

```js
  // An ended session cannot be attached: `mk web` replays the end of its log instead.
  try {
    if (state.sessions.get(id)?.state === 'exited') await replay(id);
    else await attach(id);
  } catch (err) {
    toast(err.message);
  }
}
```

New function after `attach`:

```js
/** The newest output of an ended session: `mk web` sends it as output just before this reply. */
async function replay(id) {
  const s = state.sessions.get(id);
  term.resize(s.cols, s.rows);
  fitTerminal();
  await call('session.history', { session_id: id });
}
```

- [ ] **Step 6: Run the JS tests, expect PASS:** `node --test crates/mkrew-cli/web/model.test.mjs`

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-cli/web/model.mjs crates/mkrew-cli/web/model.test.mjs
git add -p crates/mkrew-cli/web/app.mjs
git commit -m "feat(web): list sessions that ended today and replay their output"
```

---

### Task 5: Smoke, manual checks, progress

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new section at the top of the "To verify" list)
- Modify: `docs/PROGRESS.md` (one note line, next to the 2026-10-12 Mkrew web line)

- [ ] **Step 1: Full verify:** `sh scripts/verify.sh` → fmt, clippy `-D warnings` and tests all green; `node --test crates/mkrew-cli/web/model.test.mjs` → green.

- [ ] **Step 2: Browser smoke, isolated from the user's daemon.** Never stop or touch the user's own daemon. Set `MKREW_DATA_DIR`, `MKREW_CONFIG_DIR` (temp folders) and `MKREW_SOCKET` (a unique pipe, e.g. `\\.\pipe\mkrew-smoke`), write `agents/fake.toml` in that config dir like `Env::write_fake_profile` in `crates/mkrew-cli/tests/cli.rs`, then start `target/debug/mk.exe web --listen 127.0.0.1:8788 --host localhost` as a named background service. Add a project and spawn two fake sessions with `mk` (same env). Open `http://127.0.0.1:8788/#t=<token from <config>/web_token>` in the eval `browser` at a 390×844 viewport and check, with a screenshot of each:
  1. `GET /apple-touch-icon.png` shows the icon.
  2. Open session 1: the Stop button shows; tap Stop → Cancel → the session still runs; tap Stop → OK → the pill becomes "Exited · now", the composer goes, Stop hides, and the terminal keeps its content.
  3. Open session 2: the chips show `continue` and `yes, go ahead`; type `y` in Reply and Send → a `y` chip leads the row; reload the page → the chips are still there.
  4. Back to the list: session 1 sits below session 2 in its workspace with meta "now". Reload the page: session 1 is still listed. Open it → the terminal shows the end of its output (the fake agent's question) and the "Session ended" bar.
  5. Dark and light (`prefers-color-scheme` emulation): the chips and the Stop button use the theme colors.
  Close the tab, stop the service, delete the temp folders.

- [ ] **Step 3: HUMAN_CHECKS.** Add, above "To verify — 2026-10-12 Phone access from the UI":

```markdown
### To verify — 2026-10-13 Mkrew web extras (plan docs/superpowers/plans/2026-10-13-mobile-web-extras.md)

- [ ] **Icon:** remove the old Home Screen bookmark, open the link in Safari, Share → Add to Home
  Screen: the preview and the Home Screen show the Mkrew lanes icon (not a page screenshot).
- [ ] **Stop:** open a running Claude session, tap the square button → "Stop session …?" → OK:
  within a few seconds the pill shows "Exited", and the desktop pane shows the session ended.
  Cancel leaves it running.
- [ ] **Quick replies:** send "continue" from the Reply box, open another session: the chip
  "continue" leads the row; tapping it sends at once and the keyboard does not jump.
- [ ] **Ended sessions:** a session that ended in the last hours is listed (✕, muted, "2h") below
  the live ones; opening it shows the end of its output at its own width and "Session ended".
  A session that ended yesterday morning is gone from the list.
```

- [ ] **Step 4: PROGRESS.** Under the notes, next to the 2026-10-12 Mkrew web line, add: `- 2026-10-13 Mkrew web extras (plan docs/superpowers/plans/2026-10-13-mobile-web-extras.md): Home Screen icon, Stop (\`session.kill\`), recent-reply chips (\`localStorage\` \`mkrew.replies\`), ended sessions of the last 24 h replayed from the log (relay sends \`mkrew_core::log_tail\` before the \`session.history\` reply; ADR 0052 amended). Manual checks in HUMAN_CHECKS "2026-10-13 Mkrew web extras". Outside the milestone loop.`

- [ ] **Step 5: Commit**

```bash
git add docs/PROGRESS.md
git add -p docs/HUMAN_CHECKS.md
git commit -m "docs(web): manual checks and progress for the phone page extras"
```
