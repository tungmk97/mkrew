# Phone Push Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a session enters `needs_input` or `done`, a phone that opted in gets a system notification "omp · mkrew — needs you" (title `omp · mkrew`, body `needs you`); tapping it opens the `mk web` page on that session.

**Architecture:** `mk web` (the process that already serves the phone page) opens its own daemon connection, watches `session.state` events, applies the existing `[notify]` switches and the 30 s per-session throttle (`mkrew_daemon::decide`), and sends a Web Push message (RFC 8030 + RFC 8291 `aes128gcm` encryption + RFC 8292 VAPID) to the push service of each subscribed phone. A service worker on the page shows the notification and opens the session on tap. Opt-in twice: `[web] push = true` on the PC (Phone access dialog row) and the bell button on the phone (browser permission prompt).

**Tech Stack:** Rust 2024; `ring` 0.17 (P-256 ECDH + ECDSA, HKDF, AES-128-GCM) and `ureq` 3.4 (blocking HTTPS POST) — both already in `Cargo.lock`, no new package; `base64` 0.22 (already in `Cargo.lock`); vanilla JS service worker + ES modules tested with `node --test`.

**Spec:** `docs/SPEC.md` §1 (non-goals), §8 (Notifications), §9 (`mk web`), §15 (Security row), §20.0 (settings, Phone access); `docs/decisions/0052-mobile-web.md`, `0053-phone-access-ui.md`; design decided in chat 2026-10-19, recorded in the "Design" section of this plan.

---

## Feasibility verdict (read first)

**FEASIBLE. No hard blocker.** Everything needed is in the tree already. What the human must do or accept (none of it blocks writing the code):

| # | Human action / acceptance | Why |
| --- | --- | --- |
| 1 | **Accept ADR 0066**: with `[web] push = true`, `mk web` makes outbound HTTPS requests to Apple / Google / Mozilla / Microsoft push services (payload end-to-end encrypted to the phone; the push service sees only that *a* message went to *that* phone, its size and timing). SPEC §1's "no cloud" wording changes (Task 1). | Web Push has no serverless mode: the browser vendor's push service is the only way to wake a sleeping phone. |
| 2 | Tailscale admin console: **MagicDNS + HTTPS Certificates on** and Phone access **Set up** done — the same steps README already requires for the phone page. No new Tailscale step. | Service workers + Push need a secure context (HTTPS). |
| 3 | iPhone: **iOS 16.4+**, and the page must be **re-added to the Home Screen** once after this lands (the manifest is new), then notifications allowed from the bell. | WebKit: Web Push only works in a Home Screen web app, permission only from a tap. |
| 4 | **Device check [INFERENCE until done]**: that iOS treats the Home Screen app made from our dynamic manifest as a push-capable web app, and that Apple's push service accepts the VAPID `sub` claim we send (`mailto:mkrew@example.com`). Both are in HUMAN_CHECKS (Task 8). If Apple answers 403 `BadJwtToken`, the fix is one constant (`SUBJECT` in `crypto.rs`) — see Task 2. | Could not be run here: no iPhone, no network from tests. |

### Research findings (what was verified, what is inferred)

| Question | Finding | Evidence |
| --- | --- | --- |
| How does `mk web` serve today? | Plain **HTTP on `127.0.0.1:8787`** (`TcpListener::bind(listen)`, default `127.0.0.1:8787`); HTTPS comes from **`tailscale serve --bg 8787`**, which publishes `https://<MagicDNS name>/` with a Tailscale-issued certificate. `mk web` prints `https://{host}/#t=<token>`. No `tailscale cert` needed. | verified: `crates/mkrew-cli/src/web.rs` (`serve`: bind + `println!("Open on your phone: https://{host}/#t=…")`), `crates/mkrew-core/src/tailscale.rs` (`serve_proxies_port` requires `TCP/443/HTTPS`, `serve_setup` runs `tailscale serve --bg <port>`), `README.md` "Use Mkrew from your phone" (MagicDNS + HTTPS Certificates). |
| Secure context for service worker / Push? | Already satisfied through `tailscale serve`. On plain `http://<tailscale-ip>:8787` it would **not** work (only `localhost` is exempt). The bell therefore appears only when the page is loaded over HTTPS or localhost (`isSecureContext`), see Task 6. | verified (code paths above); WebKit rules from https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/ (fetched). |
| iOS requirement | Web Push only in a **Home Screen web app**, iOS/iPadOS 16.4+, permission requested from a user tap; a push service host `*.push.apple.com`. A Safari tab has no `PushManager`. | verified (WebKit blog above). |
| Manifest needed? | Apple developer forum reports solving "no push" by adding a manifest with `display: standalone` (https://developer.apple.com/forums/thread/725772); OneSignal's iOS guide says the same. The page today has only `apple-mobile-web-app-capable`. A *static* manifest would make the Home Screen app start at `/` and lose the token (iOS gives the Home Screen app its own storage — ADR 0052). So the plan serves a **dynamic, token-gated manifest** whose `start_url` is `/#t=<token>`. | search results fetched; "works for our app" is **[INFERENCE]** → device check (#4). |
| Crypto/HTTP crates in `Cargo.lock` | **Present:** `ring 0.17.14` (via `rustls`), `ureq 3.4.2` (+`ureq-proto`, `rustls`, `webpki-roots`; used by `mkrew-ui` for speech to text `crates/mkrew-ui/src/stt.rs:100`), `base64 0.22.1`, `hkdf 0.12.4`, `hmac 0.12.1`, `aes 0.8.4`, `sha2`, `getrandom`. **Absent:** `p256`, `aes-gcm`, `ece`, `web-push`. `mkrew-cli` has none of the three it needs yet (`ring`, `ureq`, `base64`) but adding them adds **no package** to the lock — only three dependency lines in the `mkrew-cli` entry. | verified: `Cargo.lock` grep; `crates/mkrew-cli/Cargo.toml`; `crates/mkrew-ui/Cargo.toml:59-60` (`ureq = { version = "3.4.2", default-features = false, features = ["rustls"] }`). |
| `ring` covers the whole message? | Yes: `agreement::{EphemeralPrivateKey, agree_ephemeral, ECDH_P256}` (ECDH), `signature::{EcdsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING}` (VAPID ES256, fixed 64-byte r‖s = JWT form), `hkdf::{Salt, HKDF_SHA256}`, `aead::{AES_128_GCM, LessSafeKey, Nonce}`. Limit: `ring` cannot import an ECDH private key, so the RFC 8291 vector is tested through the pure `seal` step fed with the RFC's *shared secret*; the ECDH step is tested by a round trip with a second `ring` key. | verified in `~/.cargo/registry/src/*/ring-0.17.14/src/{agreement.rs,rand.rs,hkdf.rs,aead/less_safe_key.rs,ec/suite_b/ecdsa/signing.rs}`. |
| Where does it run? | In **`mk web`**, not the daemon. `mk web` already opens daemon connections (`relay::open_daemon`), and the daemon sends every greeted connection every event (`server.rs`: `let events = self.hub.events.subscribe()` after `hello`), including `session.state`. Nothing in the daemon depends on `ClientKind`. | verified: `crates/mkrew-daemon/src/server.rs:203-206`, `crates/mkrew-cli/src/web/relay.rs:159-202`. |
| SPEC §1 conflict | Non-goals say "no cloud sync", "never calls LLM APIs itself", with the voice exception. Push is a new outbound call. → ADR 0066 + SPEC §1/§8/§9/§15/§20.0 edits (Task 1). | verified: `docs/SPEC.md` §1. |
| Push service hosts | Allowlist `fcm.googleapis.com`, `updates.push.services.mozilla.com`, `*.push.apple.com`, `*.notify.windows.com`. Apple host verified (WebKit blog); the others **[INFERENCE]** from vendor docs (not re-fetched). An unknown host is refused at subscribe time (Review Focus 1); a new vendor host is a one-line change. | partly verified. |
| VAPID `sub` | Apple rejects `mailto:…@localhost` with 403 `BadJwtToken` (GitHub issue reports found by search); a dotted domain is what those reports ask for. `example.com` passing is **[INFERENCE]** → device check. | search results. |
| Alternatives | *Payload-less push*: still needs VAPID + the same outbound POST, and the service worker would have to call back to the PC (needs the token, and a sleeping phone's network may not be up) — no gain, rejected. *Keep-alive page / polling*: iOS suspends Home Screen apps in the background — does not work, rejected. *Full Web Push*: chosen. | reasoning; RFC 8030/8291/8292. |

---

## Global Constraints

- Numbers are reserved for planning; before creating, re-check the newest ADR/migration/PROTOCOL_VERSION and take the next free one if another plan landed first. Reserved here: **ADR 0066**. Migration 0022 and PROTOCOL_VERSION 46 are **not used** (subscriptions live in a private file next to `web_token`; no protocol change). Last seen: ADR 0062, migration 0019, `PROTOCOL_VERSION = 43` (`crates/mkrew-protocol/src/lib.rs:16`).
- Deviating from the spec needs an ADR in `docs/decisions/` **and** a `docs/SPEC.md` update in the same commit (`AGENTS.md`). Task 1 does both. Other plans edit `docs/SPEC.md` too: find SPEC anchors by the **quoted text**, not by line number.
- No `unwrap()` / `expect()` outside tests (`[workspace.lints.clippy] unwrap_used/expect_used`); `unsafe_code = "deny"`.
- Every change ships with tests. **No test may call a real agent or the network.** The push sender is injected (`trait Sender`); `MKREW_PUSH=<file>` swaps the network sender for a JSON-lines file (like `MKREW_NOTIFY`).
- Settings keys follow `mkrew_core::settings` rules: unknown keys are errors; a missing key means its default; writers keep comments (`write_value`).
- `[notify] needs_input` / `done` and the 30 s per-session limit (`mkrew_daemon::THROTTLE`) apply to phone push (SPEC §8, §20A.2). `notify.sound` does not (the OS plays its own sound for a web push).
- Secrets: the subscription URLs and keys are capabilities. Store them in `<config>/web_push.json` with the same rule as `web_token` (owner-only on Unix). Never print a full endpoint URL or key to stdout/stderr; log the push service **host** only.
- UI (Task 7): only `crates/mkrew-ui/src/theme.rs` tokens, dark + light, spacing 4/8/12/16/24/32, radius 4/8/12; add a `docs/HUMAN_CHECKS.md` entry. The phone page uses its own CSS variables in `crates/mkrew-cli/web/app.css`.
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, stage **explicit paths only** (never `git add -A`). Never push. `Cargo.lock`: stage it only for the hunk that adds `base64`, `ring`, `ureq` to the `mkrew-cli` entry (`git add -p Cargo.lock` if other work touched it).
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`; per-crate loop `cargo test -p <crate> <filter> -j 4`. After a CLI change, `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` before the `mkrew-cli` integration tests. JS: `node --test crates/mkrew-cli/web/model.test.mjs`. `cargo insta` is not installed (no snapshots are touched here). The C: drive is nearly full: no needless builds.
- Phone page copy: sentence case, no jargon ("Notify this phone", not "Subscribe to Web Push").

## Review Focus

The inputs the spec implies but no happy-path test covers. Each line names the task whose tests pin it.

1. **A subscription pointing at an arbitrary URL** (anyone holding the token can POST one; `mk web` would then POST to it from the user's PC — SSRF). Expected: refused with 400 and nothing is ever sent to a host outside the Apple/Google/Mozilla/Microsoft list; look-alike hosts (`web.push.apple.com.evil.test`, `https://user@host`, `http://…`, ports) refused. → Task 3 (`endpoint_allowed` table, `add` refuses), Task 5 (HTTP 400 end to end).
2. **Push service says the phone is gone (404/410) vs. a transient failure (5xx, timeout).** Expected: gone → subscription removed and the file rewritten; failed → kept; one dead phone never stops the others and never stops later events. → Task 4 (`deliver` tests with two phones).
3. **`[web] push` switched off while `mk web` runs, or `settings.toml` broken.** Expected: no message is sent, `/push/key` answers 404 (the bell disappears on the next load), the 30 s window is not consumed by suppressed events, and a broken settings file means *off* (never on). → Task 4 (`wanted`, `enabled`, `public_key`).
4. **Corrupt or missing `web_push.json`, `mk web --new-token`.** Expected: a corrupt file makes a fresh VAPID key and an empty phone list (one stderr line, no crash); `--new-token` empties the phone list but keeps the VAPID key; a phone that subscribes twice is one entry; at most 8 phones. → Task 3.
5. **Long or odd session names.** Expected: title/body are clipped so the encrypted record always stays under 3993 bytes (RFC 8291 §4); an oversize payload is refused, not truncated mid-codepoint. → Task 2 (`MAX_PLAINTEXT`), Task 4 (`payload` clips).
6. **Tapping a notification** when the app is closed, open on the list, or open on another session. Expected: it opens on the notified session; an id the page does not know is ignored (no crash, no blank screen). → Task 6 (`sessionFromHash`, `pushOffer` tests) + HUMAN_CHECKS (service worker cannot run under `node --test`).

## File structure

| File | Responsibility |
| --- | --- |
| `docs/decisions/0066-phone-push-notifications.md` (new) | The decision: opt-in, end-to-end encrypted, external push service. |
| `crates/mkrew-core/src/settings.rs` | `WebSettings.push`, `write_web_push`, template lines. |
| `crates/mkrew-cli/src/web/push.rs` (new) | `Push` (switches, subscribe/unsubscribe, deliver), `Sender` trait + `UreqSender` / `FileSender`, `Payload`, `wanted`, the event loop `run`. |
| `crates/mkrew-cli/src/web/push/crypto.rs` (new) | RFC 8291 encryption, VAPID JWT. Pure + `ring`. |
| `crates/mkrew-cli/src/web/push/store.rs` (new) | `<config>/web_push.json`: VAPID key + subscriptions, endpoint allowlist. |
| `crates/mkrew-cli/src/web/relay.rs` | `Daemon::next_event` / `Daemon::call` for the push loop. |
| `crates/mkrew-cli/src/web/http.rs` | `POST /push/subscribe`, `/push/unsubscribe`, `GET /push/key`, `/manifest.webmanifest`, `/sw.js`; `Content-Length`. |
| `crates/mkrew-cli/src/web.rs` | Wires `Push` into `serve`/`handle`; `--new-token` drops subscriptions. |
| `crates/mkrew-daemon/src/notify.rs`, `lib.rs` | Export `decide` / `Throttle` / `Alert` so `mk web` reuses the exact §8 policy. |
| `crates/mkrew-cli/web/sw.js` (new), `app.mjs`, `model.mjs`, `model.test.mjs`, `index.html`, `app.css` | The service worker, the bell, helpers + tests. |
| `crates/mkrew-ui/src/phone_dialog.rs`, `shell/phone_access.rs`, `main.rs` | "Phone notifications" row in the Phone access dialog. |
| `README.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`, ADR 0052 note | Docs. |

## Design

Decisions made in chat 2026-10-19, plus the defaults picked here (one line each):

- **Where it runs: `mk web`**, own daemon connection (`ClientKind::Ui`, like the relay). The daemon stays free of network code (SPEC §15 "daemon has no network listener" stays true; it also makes no outbound call).
- **Switches:** `[web] push = false` (default) is the PC owner's consent for outbound calls *and* the gate that stops a token holder from making the PC contact push services; the bell on the phone is the per-device consent. Reason for both: one without the other leaves either the PC or the phone without a say.
- **Decision logic reused, not copied:** `mkrew_daemon::decide` (+`Throttle`) is exported; phone push has its own `Throttle` (per `mk web` process), so a toast and a push for the same event both happen.
- **Text:** title `"{session name} · {workspace name}"` (session title, else terminal title, else profile id — same order as the desktop toast, `notify.rs` `notice`), body `needs you` / `done`. Each part is clipped to 60 chars.
- **Delivery headers:** `TTL: 3600` (a notice older than an hour is noise), `Urgency: high` (a waiting agent should wake the phone), no `Topic`. The notification `tag` is the session id, so a newer notice for the same session replaces the older one on the phone.
- **Subscriptions persisted** in `<config>/web_push.json` (not SQLite): `mk web` is a separate process and has no store; no migration 0022. The file also holds the VAPID key (PKCS#8, base64url). `mk web --new-token` empties the list, keeps the key.
- **Push service allowlist** (SSRF guard) and **at most 8 phones**.
- **Sender:** blocking `ureq` inside `spawn_blocking`, 10 s timeout, sequential over ≤ 8 phones. `Gone` = HTTP 404/410 → subscription removed.
- **Manifest:** dynamic `GET /manifest.webmanifest?t=<token>` (token-gated, `start_url = /#t=<token>`), linked by the page after the token is proven. Existing Home Screen icons keep working as bookmarks but the user must re-add to get push.
- **The page re-sends its subscription after every connect** (idempotent upsert) so a `--new-token` followed by re-entering the token re-registers the phone without a tap; a phone whose browser subscription was made with an older VAPID key is unsubscribed so the bell offers a fresh one.
- **Always show a notification** on `push` in the service worker (even if the page is visible): iOS revokes subscriptions that receive silent pushes.
- **No new protocol request, no daemon state, no migration.**

---

### Task 1: ADR 0066, SPEC, and the `[web] push` setting

**Files:**
- Create: `docs/decisions/0066-phone-push-notifications.md`
- Modify: `docs/SPEC.md` (§1 non-goals, §8 Notifications bullet, §9 `mk web` line, §15 Security row, §20 intro "no cloud" sentence, §20.0 settings block and Phone access paragraph)
- Modify: `docs/decisions/0052-mobile-web.md` (one amendment line)
- Modify: `crates/mkrew-core/src/settings.rs` (`WebSettings` ~line 47; `write_web_enabled` ~line 344; `template()` ~line 427; tests `full_file_parses` ~line 545, `web_section_defaults_off_and_rejects_other_keys` ~line 923)
- Test: `crates/mkrew-core/src/settings.rs` (inline `mod tests`)

**Interfaces:**
- Produces: `mkrew_core::settings::WebSettings { pub enabled: bool, pub push: bool }` (still `Copy + Default`); `pub fn write_web_push(path: &Path, enabled: bool) -> Result<(), SettingsError>`. Later tasks read `settings.web.push`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-core/src/settings.rs`, replace the body of `web_section_defaults_off_and_rejects_other_keys` and add a writer test next to `write_web_enabled_keeps_the_rest_of_the_file`:

```rust
    #[test]
    fn web_section_defaults_off_and_rejects_other_keys() {
        let web = |src: &str| parse(Path::new("s.toml"), src).unwrap().web;
        assert!(!web("").enabled);
        assert!(!web("[web]").enabled);
        assert!(web("[web]\nenabled = true").enabled);
        // Phone notifications (ADR 0066) are off until the user turns them on.
        assert!(!web("").push);
        assert!(!web("[web]\nenabled = true").push);
        assert!(web("[web]\npush = true").push);
        assert!(parse_err("[web]\npush = \"yes\"").contains("invalid type"));
        assert!(parse_err("[web]\nenabled = \"yes\"").contains("invalid type"));
        // The port is fixed at 8787 (ADR 0053), so it is not a setting.
        assert!(parse_err("[web]\nport = 8787").contains("unknown field `port`"));
    }

    #[test]
    fn write_web_push_keeps_enabled_and_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[web]\nenabled = true\n\n[features]\ndocs = true\n").unwrap();
        write_web_push(&path, true).unwrap();
        let settings = load(&path).unwrap();
        assert!(settings.web.enabled && settings.web.push && settings.features.docs);
        write_web_push(&path, false).unwrap();
        let settings = load(&path).unwrap();
        assert!(settings.web.enabled && !settings.web.push);
        // No file: the commented template plus the switch.
        let fresh = dir.path().join("fresh.toml");
        write_web_push(&fresh, true).unwrap();
        assert!(load(&fresh).unwrap().web.push);
    }
```

In `full_file_parses` change the `[web]` block to `[web]\nenabled = true\npush = true` and the expectation to `web: WebSettings { enabled: true, push: true },`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: FAIL to compile (`no field push`, `write_web_push` not found).

- [ ] **Step 3: Implement**

`WebSettings` (replace the struct at `settings.rs` ~line 43-49):

```rust
/// `[web]` (ADR 0053, ADR 0066): the daemon runs `mk web` while `enabled` is true; `mk web`
/// sends phone notifications while `push` is true. The Phone access dialog writes both; the
/// port is fixed (`mkrew_core::tailscale::WEB_PORT`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebSettings {
    pub enabled: bool,
    /// Notifications to subscribed phones through their browser's push service (ADR 0066).
    pub push: bool,
}
```

After `write_web_enabled`:

```rust
/// Saves `[web] push` (the Phone access dialog's notifications switch); see [`write_value`].
/// `mk web` reads it on every request and event, so no restart is needed.
pub fn write_web_push(path: &Path, enabled: bool) -> Result<(), SettingsError> {
    write_value(path, "web", "push", &enabled.to_string())
}
```

In `template()` replace the `[web]` tail with:

```rust
     # Phone access (ADR 0053): while enabled is true the daemon runs `mk web`, so a phone on your\n\
     # tailnet can reach Mkrew. The phone icon in the title bar writes it; the port is fixed (8787).\n\
     # Phone notifications (ADR 0066): push = true lets `mk web` notify phones that opted in when an\n\
     # agent needs you or finishes. The message is encrypted for the phone and goes through its\n\
     # browser's push service (Apple, Google, Mozilla or Microsoft). [notify] needs_input / done apply.\n\
     [web]\n\
     # enabled = false\n\
     # push = false\n"
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: PASS (including the unchanged `write_web_enabled_*` tests).

- [ ] **Step 5: Write ADR 0066**

Create `docs/decisions/0066-phone-push-notifications.md`:

```markdown
# 0066 — Push notifications to the phone (Web Push from `mk web`)

- Date: 2026-10-19
- Status: accepted (extends ADR 0052 and ADR 0053; changes SPEC §1 non-goals)
- Milestone: none (user request: when the user leaves the desk, agents wait a long time for an answer)

## Context

- `mk web` lets a phone browser on the tailnet watch and answer sessions, and the page can be added
  to the iPhone Home Screen. Nothing tells the phone that an agent now needs an answer.
- A web page can only be woken in the background by Web Push (service worker + Push API). That needs
  a secure context (HTTPS, already provided by `tailscale serve`), VAPID keys (RFC 8292), payload
  encryption (RFC 8291) and an HTTPS POST from the application server to the **push service of the
  phone's browser vendor** (Apple for Safari/iOS, Google FCM for Chrome, Mozilla, Microsoft).
- SPEC §1 says "no cloud" and that Mkrew never calls APIs except voice input (ADR 0045). A push
  service is an external network endpoint, so this needs an explicit decision.

## Options

1. Keep-alive page or polling: iOS suspends a Home Screen app in the background; nothing arrives.
2. Payload-less push (the service worker fetches the text from the PC): same VAPID and same outbound
   POST, and a sleeping phone cannot reliably call back; no gain.
3. A relay service run by Mkrew: a cloud and accounts; rejected by §1.
4. Web Push with an encrypted payload, sent by `mk web`. (Chosen.)

## Decision

- **Opt-in twice.** `[web] push = true` in `settings.toml` (default `false`; the Phone access dialog
  writes it) lets `mk web` contact push services at all. Each phone then opts in with the bell
  button on the page, which makes the browser ask the user for permission. Neither alone sends anything.
- **What leaves the PC:** one HTTPS POST per notified phone to that phone's push service host. The
  body is RFC 8291 `aes128gcm` ciphertext (title = "session · workspace", body = "needs you" / "done",
  the session id) readable only by that phone; the push service sees the destination, size and time.
  Hosts are limited to Apple (`*.push.apple.com`), Google (`fcm.googleapis.com`), Mozilla
  (`updates.push.services.mozilla.com`) and Microsoft (`*.notify.windows.com`); a subscription for any
  other host is refused (a token holder must not be able to make the PC call arbitrary URLs).
- **Where it runs:** `mk web` opens its own daemon connection and reads `session.state` events. The
  daemon gets no network code and no protocol change. The §8 policy is shared: `[notify] needs_input` /
  `done` and the 30 s per-session limit apply (`mkrew_daemon::decide`, `Throttle`).
- **Keys and state:** `<config>/web_push.json` (owner-only on Unix) holds the VAPID key (P-256, PKCS#8)
  and the subscriptions (at most 8). `mk web --new-token` empties the subscriptions and keeps the key.
  A push service answer 404 / 410 removes that subscription.
- **Crypto and HTTP:** `ring` (P-256 ECDH and ECDSA, HKDF, AES-128-GCM) and `ureq` (HTTPS POST),
  both already in the dependency tree for rustls / speech to text; no new package.
- **Page:** a service worker (`/sw.js`), a token-gated Web App Manifest (`/manifest.webmanifest`,
  `start_url` carries the token because the iOS Home Screen app has its own storage) and a bell button.
  iOS needs iOS 16.4+ and the page added to the Home Screen.
- Tests never use the network: the sender is injected; `MKREW_PUSH=<file>` makes `mk web` append
  each message to a JSON-lines file instead of sending it (like `MKREW_NOTIFY`).

## Consequences

- SPEC §1, §8, §9, §15, §20.0 change. No protocol version change, no migration.
- With `push = true` the PC makes outbound HTTPS requests to the four push services. With the
  default `false`, behaviour is exactly as before.
- Push is best effort (TTL 1 h): a phone offline longer than that misses the notice.
- iOS revokes subscriptions that receive pushes without showing a notification, so the service worker
  always shows one.
- Not done: several PCs on one phone page, notification actions (Approve / Deny buttons), badge count.
```

- [ ] **Step 6: Edit SPEC.md and ADR 0052**

In `docs/SPEC.md` (find each by the quoted text; do **not** rely on line numbers):

1. §1 Non-goals — after the bullet beginning `- No native mobile app, cloud sync or user accounts.` append to that bullet: ` Optional phone notifications (§20.0 \`[web] push\`, ADR 0066) are the other exception to "no cloud" and are described below.` Then, after the bullet ending `(ADR 0045).` of "Mkrew never calls LLM APIs itself", add a new bullet:
   ```
   - Mkrew runs no cloud service of its own. Its only outbound requests are voice input (above) and,
     only while the user has turned `[web] push` on and a phone has opted in, end-to-end encrypted
     Web Push messages to that phone's push service (Apple, Google, Mozilla or Microsoft; ADR 0066).
   ```
2. §8 — the bullet `- **Notifications:** OS notification when a session enters needs_input or done; at most one per session per 30 s. Clicking it emits \`notification.clicked\`.` gets appended: ` While \`[web] push\` is on, \`mk web\` also sends a Web Push notification to each phone that opted in, under the same \`[notify]\` switches and the same 30 s limit (ADR 0066; §20.0).`
3. §9 — the line `mk web [--listen ADDR] [--host NAME] [--new-token]  # phone page over the tailnet (ADR 0052)` becomes `mk web [--listen ADDR] [--host NAME] [--new-token]  # phone page over the tailnet (ADR 0052); --new-token also drops push subscriptions (ADR 0066)`.
4. §15 Security row — before the closing ` \| ` of the `Security` row insert after `requires its token on every WebSocket, ADR 0052)`: `; with \`[web] push = true\` \`mk web\` also POSTs encrypted Web Push messages to the push service host (Apple, Google, Mozilla, Microsoft only) of each subscribed phone, ADR 0066`.
5. §20 intro — in `§1 non-goals still hold: no editor, no LLM API calls, no cloud, no native mobile app (a browser client exists, ADR 0052).` append ` Phone push (ADR 0066) is opt-in and end-to-end encrypted.`
6. §20.0 settings block — the lines
   ```
   [web]                       # ADR 0053; phone access
   enabled = false             # the daemon runs `mk web` while true; the Phone access dialog writes it
   ```
   gain a third line:
   ```
   push = false                # ADR 0066; mk web notifies phones that opted in (needs enabled); the Phone access dialog writes it
   ```
   and after the `**Phone access** (ADR 0053).` paragraph add:
   ```
   **Phone notifications** (ADR 0066). While `[web] push = true`, `mk web` watches `session.state` on its
   own daemon connection and, for each session entering `needs_input` or `done` (subject to
   `[notify] needs_input` / `done` and one per session per 30 s), sends a Web Push message
   (RFC 8291 `aes128gcm`, VAPID RFC 8292) to every phone that tapped the bell on the page. Title
   `<session> · <workspace>`, body `needs you` / `done`; tapping it opens the page on that session.
   Subscriptions and the VAPID key live in `<config>/web_push.json`; a push service answer 404 / 410
   removes a subscription; `mk web --new-token` empties the list. Only Apple, Google, Mozilla and
   Microsoft push hosts are accepted. The page needs HTTPS (`tailscale serve`), and on iPhone iOS
   16.4+ with the page added to the Home Screen.
   ```

In `docs/decisions/0052-mobile-web.md`, change the Consequences line `- Not done: push notifications, several PCs, board/files/git/docs on the phone.` to `- Not done: several PCs, board/files/git/docs on the phone. (Push notifications: ADR 0066, 2026-10-19.)`.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-core/src/settings.rs docs/decisions/0066-phone-push-notifications.md docs/decisions/0052-mobile-web.md docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "docs(web): ADR 0066 phone push notifications, [web] push setting"
```
(`docs/SPEC.md` may hold other agents' hunks: use `git add -p docs/SPEC.md` and stage only the ones above.)

---

### Task 2: Web Push crypto (RFC 8291 encryption, VAPID) on `ring`

**Files:**
- Modify: `crates/mkrew-cli/Cargo.toml`
- Modify: `Cargo.lock` (three dependency lines under `mkrew-cli`)
- Create: `crates/mkrew-cli/src/web/push/crypto.rs`
- Create: `crates/mkrew-cli/src/web/push.rs` (only `mod crypto;` for now)
- Modify: `crates/mkrew-cli/src/web.rs` (add `mod push;` next to `mod http; mod relay; mod token;`, line 5-7)
- Test: inline `mod tests` in `crypto.rs`

**Interfaces:**
- Produces (all in `web::push::crypto`):
  - `pub const MAX_PLAINTEXT: usize = 3993;`
  - `pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8]) -> anyhow::Result<Vec<u8>>` — full `aes128gcm` request body (86-byte header + record) for one phone.
  - `pub fn seal(ecdh_secret: &[u8], auth: &[u8], ua_public: &[u8], as_public: &[u8], salt: &[u8; 16], plaintext: &[u8]) -> anyhow::Result<Vec<u8>>` — deterministic core (tests, RFC vector).
  - `pub struct Vapid` with `generate() -> Result<Self>`, `from_pkcs8(&[u8]) -> Result<Self>`, `pkcs8(&self) -> &[u8]`, `public_key(&self) -> &[u8]` (65 bytes), `authorization(&self, endpoint: &str, now: u64) -> Result<String>` (`vapid t=<jwt>, k=<key>`).
  - `pub fn origin(url: &str) -> Option<&str>` — `https://host[:port]` of a URL.

- [ ] **Step 1: Add the dependencies**

In `crates/mkrew-cli/Cargo.toml` `[dependencies]` (keep alphabetical; replace the `serde` line):

```toml
anyhow = "1.0.104"
# `mk web` push (ADR 0066): base64url of keys and tokens; 0.22 is already in the tree.
base64 = "0.22"
```
```toml
mkrew-protocol = { version = "0.1.0", path = "../mkrew-protocol" }
# `mk web` push (ADR 0066): P-256 ECDH + ECDSA, HKDF, AES-GCM; already in the tree through rustls.
ring = "0.17"
# `derive`: the push subscription file (ADR 0066); mkrew-core already turns it on workspace-wide.
serde = { version = "1", features = ["derive"] }
```
```toml
ulid = "1"
# `mk web` push (ADR 0066): the HTTPS POST to the phone's push service; same build as mkrew-ui's
# speech to text (rustls, so no OpenSSL).
ureq = { version = "3.4.2", default-features = false, features = ["rustls"] }
```

Run `cargo tree -p mkrew-cli -i ring -j 4` — Expected: lists `ring v0.17.14` with `mkrew-cli` as a dependent (no download: it is already in `Cargo.lock`).

- [ ] **Step 2: Write the failing tests**

Create `crates/mkrew-cli/src/web/push.rs` with only:

```rust
//! Web Push from `mk web` (ADR 0066).

mod crypto;
```
and add `mod push;` to `web.rs` (after `mod http;`). Create `crates/mkrew-cli/src/web/push/crypto.rs` containing the tests below *only* (the implementation comes in Step 4; `use super::*;` fails to compile until then, which is the expected red):

```rust
#[cfg(test)]
mod tests {
    use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey as SigPublicKey};

    use super::*;

    fn b(text: &str) -> Vec<u8> {
        B64.decode(text).unwrap()
    }

    // RFC 8291 Appendix A (and §5): every value below is copied from the RFC.
    const PLAINTEXT: &str = "V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24";
    const AS_PUBLIC: &str = "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const UA_PUBLIC: &str = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const ECDH_SECRET: &str = "kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs";
    const IKM: &str = "S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg";
    const CEK: &str = "oIhVW04MRdy2XN9CiKLxTg";
    const NONCE: &str = "4h_95klXJ5E_qnoN";
    const HEADER: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const CIPHERTEXT: &str = "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ";
    /// The whole body of RFC 8291 §5 (the three base64url lines joined).
    const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

    #[test]
    fn the_rfc_8291_keys_come_out_as_in_the_appendix() {
        let ikm = input_key(&b(ECDH_SECRET), &b(AUTH), &b(UA_PUBLIC), &b(AS_PUBLIC)).unwrap();
        assert_eq!(ikm.to_vec(), b(IKM));
        let (cek, nonce) = content_keys(&ikm, &b(SALT)).unwrap();
        assert_eq!(cek.to_vec(), b(CEK));
        assert_eq!(nonce.to_vec(), b(NONCE));
    }

    #[test]
    fn the_rfc_8291_message_is_reproduced_byte_for_byte() {
        let salt: [u8; 16] = b(SALT).try_into().unwrap();
        let body = seal(
            &b(ECDH_SECRET),
            &b(AUTH),
            &b(UA_PUBLIC),
            &b(AS_PUBLIC),
            &salt,
            &b(PLAINTEXT),
        )
        .unwrap();
        // The appendix gives the 86-byte header and the ciphertext separately ...
        let mut expected = b(HEADER);
        expected.extend(b(CIPHERTEXT));
        assert_eq!(body, expected);
        // ... and §5 gives the same bytes as one body (144 octets: 86 + 41 + 1 + 16).
        assert_eq!(body, b(BODY));
        assert_eq!(body.len(), 144);
    }

    /// The phone's side of RFC 8291, with `ring`: proves `encrypt`'s ECDH half.
    fn decrypt(ua_private: EphemeralPrivateKey, ua_public: &[u8], auth: &[u8], body: &[u8]) -> Vec<u8> {
        let (salt, rest) = body.split_at(16);
        assert_eq!(&rest[..4], &RECORD_SIZE.to_be_bytes());
        assert_eq!(rest[4], 65, "keyid is the 65-byte sender key");
        let as_public = &rest[5..70];
        let secret = agreement::agree_ephemeral(
            ua_private,
            &UnparsedPublicKey::new(&agreement::ECDH_P256, as_public),
            |s| s.to_vec(),
        )
        .unwrap();
        let ikm = input_key(&secret, auth, ua_public, as_public).unwrap();
        let (cek, nonce) = content_keys(&ikm, salt).unwrap();
        let key = LessSafeKey::new(UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let mut record = rest[70..].to_vec();
        let plain = key
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut record)
            .unwrap();
        assert_eq!(plain.last(), Some(&2), "padding delimiter");
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn a_phone_can_read_what_encrypt_made_and_nobody_else() {
        let rng = SystemRandom::new();
        let ua_private = EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let text = br#"{"title":"omp · mkrew","body":"needs you","session":"01J0"}"#;
        let body = encrypt(&ua_public, &auth, text).unwrap();
        assert_eq!(body.len(), 86 + text.len() + 1 + 16);
        assert_eq!(decrypt(ua_private, &ua_public, &auth, &body), text);
        // A second message differs (fresh sender key and salt every time).
        assert_ne!(body, encrypt(&ua_public, &auth, text).unwrap());
    }

    #[test]
    fn bad_phone_keys_and_oversize_payloads_are_refused() {
        let ok_key = b(UA_PUBLIC);
        let auth = b(AUTH);
        assert!(encrypt(&ok_key, &auth, b"{}").is_ok());
        assert!(encrypt(&[4u8; 65], &auth, b"{}").is_err(), "not a point on P-256");
        assert!(encrypt(&ok_key[..64], &auth, b"{}").is_err(), "short key");
        assert!(encrypt(&ok_key, &auth[..15], b"{}").is_err(), "short auth secret");
        assert!(encrypt(&ok_key, &auth, "x".repeat(MAX_PLAINTEXT).as_bytes()).is_ok());
        assert!(encrypt(&ok_key, &auth, "x".repeat(MAX_PLAINTEXT + 1).as_bytes()).is_err());
    }

    /// RFC 8292 §2.4: a real token and the key that signed it.
    #[test]
    fn the_rfc_8292_example_token_verifies_with_our_checker() {
        // The three dot-separated parts of the RFC's `t=` value (header, claims, signature).
        let jwt = format!(
            "{}.{}.{}",
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9",
            "eyJhdWQiOiJodHRwczovL3B1c2guZXhhbXBsZS5uZXQiLCJleHAiOjE0NTM1MjM3NjgsInN1YiI6Im1haWx0bzpwdXNoQGV4YW1wbGUuY29tIn0",
            "i3CYb7t4xfxCDquptFOepC9GAu_HLGkMlMuCGSK2rpiUfnK9ojFwDXb1JrErtmysazNjjvW2L9OkSSHzvoD1oA",
        );
        let key = b("BA1Hxzyi1RUM1b5wjxsn7nGxAszw2u61m164i3MrAIxHF6YK5h4SDYic-dRuU_RCPCfA5aq9ojSwk5Y2EmClBPs");
        assert!(verify_jwt(&jwt, &key));
        let tampered = jwt.replacen("eyJhdWQ", "eyJhdWR", 1);
        assert!(!verify_jwt(&tampered, &key));
    }

    fn verify_jwt(jwt: &str, public_key: &[u8]) -> bool {
        let (signed, signature) = jwt.rsplit_once('.').unwrap();
        SigPublicKey::new(&ECDSA_P256_SHA256_FIXED, public_key)
            .verify(signed.as_bytes(), &b(signature))
            .is_ok()
    }

    #[test]
    fn our_vapid_header_has_the_rfc_8292_shape() {
        let vapid = Vapid::generate().unwrap();
        let now = 1_700_000_000;
        let header = vapid
            .authorization("https://web.push.apple.com/abc123?x=1", now)
            .unwrap();
        let rest = header.strip_prefix("vapid t=").expect("vapid scheme");
        let (jwt, key) = rest.split_once(", k=").expect("t and k parameters");
        assert_eq!(b(key), vapid.public_key(), "k is the public key");
        assert_eq!(vapid.public_key().len(), 65);
        assert!(verify_jwt(jwt, vapid.public_key()), "signature");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(b(parts[0]), br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims: serde_json::Value = serde_json::from_slice(&b(parts[1])).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com", "origin only");
        assert_eq!(claims["sub"], SUBJECT);
        let exp = claims["exp"].as_u64().unwrap();
        assert!(exp > now && exp - now <= 24 * 3600, "exp within 24 h (RFC 8292 §2)");
    }

    #[test]
    fn the_vapid_key_survives_a_save_and_load() {
        let vapid = Vapid::generate().unwrap();
        let again = Vapid::from_pkcs8(vapid.pkcs8()).unwrap();
        assert_eq!(vapid.public_key(), again.public_key());
        assert!(Vapid::from_pkcs8(b"not a key").is_err());
    }

    #[test]
    fn origin_keeps_scheme_host_and_port_only() {
        assert_eq!(origin("https://fcm.googleapis.com/fcm/send/x"), Some("https://fcm.googleapis.com"));
        assert_eq!(origin("https://h.example:8443?q=1"), Some("https://h.example:8443"));
        assert_eq!(origin("http://h.example/x"), None);
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p mkrew-cli web::push::crypto -j 4`
Expected: FAIL to compile (`cannot find function input_key`, `Vapid`, …).

- [ ] **Step 4: Write the implementation** (above the tests in `crypto.rs`)

```rust
//! Web Push message encryption (RFC 8291, `aes128gcm` of RFC 8188) and the VAPID header
//! (RFC 8292), on `ring` (ADR 0066). `seal` is the deterministic core the RFC vector tests;
//! `encrypt` adds a fresh sender key and salt.

use anyhow::{Context, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey};
use ring::hkdf::{self, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};

/// One record holds the whole message; the header says 4096 like the RFC example.
const RECORD_SIZE: u32 = 4096;
/// 4096 − 86 (header) − 1 (padding delimiter) − 16 (tag): the most a push service must accept
/// (RFC 8291 §4).
pub const MAX_PLAINTEXT: usize = 3993;
const P256_POINT: usize = 65;
const AUTH_SECRET: usize = 16;
/// VAPID tokens live 12 h (RFC 8292 allows at most 24 h).
const JWT_LIFETIME: u64 = 12 * 3600;
/// The VAPID `sub` claim. Apple's push service answers 403 `BadJwtToken` to `…@localhost`;
/// a contact on a dotted domain is what its error reports ask for.
const SUBJECT: &str = "mailto:mkrew@example.com";

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand<const N: usize>(prk: &hkdf::Prk, info: &[&[u8]]) -> anyhow::Result<[u8; N]> {
    let mut out = [0u8; N];
    prk.expand(info, Len(N))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| anyhow!("hkdf expand"))?;
    Ok(out)
}

/// RFC 8291 §3.3: the input keying material from the ECDH secret and the auth secret.
fn input_key(
    ecdh_secret: &[u8],
    auth: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
) -> anyhow::Result<[u8; 32]> {
    let prk = hkdf::Salt::new(HKDF_SHA256, auth).extract(ecdh_secret);
    expand::<32>(&prk, &[b"WebPush: info\0", ua_public, as_public])
}

/// RFC 8291 §3.4 / RFC 8188 §2.2: the content encryption key and nonce.
fn content_keys(ikm: &[u8; 32], salt: &[u8]) -> anyhow::Result<([u8; 16], [u8; 12])> {
    let prk = hkdf::Salt::new(HKDF_SHA256, salt).extract(ikm);
    Ok((
        expand::<16>(&prk, &[b"Content-Encoding: aes128gcm\0"])?,
        expand::<12>(&prk, &[b"Content-Encoding: nonce\0"])?,
    ))
}

/// The request body for one phone given the ECDH secret, sender key and salt: header
/// (salt, record size, key id = sender key) then the single AES-128-GCM record.
pub fn seal(
    ecdh_secret: &[u8],
    auth: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
    salt: &[u8; 16],
    plaintext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if plaintext.len() > MAX_PLAINTEXT {
        bail!("push message of {} bytes is over {MAX_PLAINTEXT}", plaintext.len());
    }
    if ua_public.len() != P256_POINT || as_public.len() != P256_POINT || auth.len() != AUTH_SECRET {
        bail!("push keys have the wrong size");
    }
    let ikm = input_key(ecdh_secret, auth, ua_public, as_public)?;
    let (cek, nonce) = content_keys(&ikm, salt)?;
    let mut record = Vec::with_capacity(plaintext.len() + 1 + 16);
    record.extend_from_slice(plaintext);
    record.push(0x02); // the last (and only) record's padding delimiter
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| anyhow!("aes key"))?,
    );
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut record)
        .map_err(|_| anyhow!("aes-gcm seal"))?;
    let mut body = Vec::with_capacity(86 + record.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(65);
    body.extend_from_slice(as_public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// Encrypts `plaintext` for the phone with public key `ua_public` (65-byte uncompressed P-256
/// point) and auth secret `auth` (16 bytes), both from its push subscription.
pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let private = EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| anyhow!("no randomness"))?;
    let as_public = private
        .compute_public_key()
        .map_err(|_| anyhow!("sender public key"))?;
    // `ring` checks that the phone's key is a point on the curve.
    let secret = agreement::agree_ephemeral(
        private,
        &UnparsedPublicKey::new(&agreement::ECDH_P256, ua_public),
        |secret| secret.to_vec(),
    )
    .map_err(|_| anyhow!("the phone's key is not a valid P-256 point"))?;
    let mut salt = [0u8; 16];
    rng.fill(&mut salt).map_err(|_| anyhow!("no randomness"))?;
    seal(&secret, auth, ua_public, as_public.as_ref(), &salt, plaintext)
}

/// `https://host[:port]` of `url`: the VAPID `aud`. `None` unless it is an `https` URL.
pub fn origin(url: &str) -> Option<&str> {
    const SCHEME: &str = "https://";
    let rest = url.strip_prefix(SCHEME)?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&url[..SCHEME.len() + end])
}

/// The application server's VAPID key pair (P-256 ECDSA).
pub struct Vapid {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
}

impl Vapid {
    pub fn generate() -> anyhow::Result<Self> {
        let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
            .map_err(|_| anyhow!("no randomness"))?;
        Self::from_pkcs8(document.as_ref())
    }

    pub fn from_pkcs8(pkcs8: &[u8]) -> anyhow::Result<Self> {
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8, &SystemRandom::new())
            .map_err(|e| anyhow!("VAPID key: {e}"))?;
        Ok(Self {
            pair,
            pkcs8: pkcs8.to_vec(),
        })
    }

    /// The key as stored (`<config>/web_push.json`).
    pub fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    /// The 65-byte uncompressed public key: the page's `applicationServerKey`.
    pub fn public_key(&self) -> &[u8] {
        self.pair.public_key().as_ref()
    }

    /// The `Authorization` header for a POST to `endpoint` at unix time `now`.
    pub fn authorization(&self, endpoint: &str, now: u64) -> anyhow::Result<String> {
        let aud = origin(endpoint).context("push endpoint is not an https URL")?;
        let claims = serde_json::json!({ "aud": aud, "exp": now + JWT_LIFETIME, "sub": SUBJECT });
        let signed = format!(
            "{}.{}",
            B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#),
            B64.encode(claims.to_string())
        );
        let signature = self
            .pair
            .sign(&SystemRandom::new(), signed.as_bytes())
            .map_err(|_| anyhow!("VAPID signature"))?;
        Ok(format!(
            "vapid t={signed}.{}, k={}",
            B64.encode(signature.as_ref()),
            B64.encode(self.public_key())
        ))
    }
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p mkrew-cli web::push::crypto -j 4`
Expected: PASS (8 tests). If the RFC tests fail, re-copy the constants from RFC 8291 Appendix A / RFC 8292 §2.4 (https://www.rfc-editor.org/rfc/rfc8291, /rfc8292) — a typo in a constant is more likely than a bug in HKDF. Run `cargo clippy -p mkrew-cli --all-targets -j 4 -- -D warnings` once.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-cli/Cargo.toml Cargo.lock crates/mkrew-cli/src/web.rs crates/mkrew-cli/src/web/push.rs crates/mkrew-cli/src/web/push/crypto.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(web): Web Push encryption and VAPID on ring (RFC 8291, RFC 8292)"
```

---

### Task 3: Subscription store and the push-service allowlist

**Files:**
- Create: `crates/mkrew-cli/src/web/push/store.rs`
- Modify: `crates/mkrew-cli/src/web/push.rs` (add `mod store;`)
- Modify: `crates/mkrew-cli/src/web/token.rs` (`write_private` becomes `pub(super)`, line 62)
- Test: inline `mod tests` in `store.rs`

**Interfaces:**
- Consumes: `crypto::{Vapid, encrypt}` (Task 2), `token::write_private(&Path, &str) -> io::Result<()>`.
- Produces (in `web::push::store`):
  - `pub fn endpoint_allowed(endpoint: &str) -> bool`
  - `#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)] pub struct Subscription { pub endpoint: String, pub p256dh: String, pub auth: String }` with `pub fn parse(body: &[u8]) -> anyhow::Result<Subscription>` (browser `PushSubscription.toJSON()` JSON) and `pub fn keys(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)>` (decoded p256dh, auth)
  - `pub fn endpoint_in(body: &[u8]) -> anyhow::Result<String>` (`{"endpoint": "…"}`)
  - `pub struct Store` with `open(file: &Path, drop_subscriptions: bool) -> anyhow::Result<Store>`, `vapid(&self) -> &Vapid`, `subscriptions(&self) -> &[Subscription]`, `add(&mut self, Subscription) -> anyhow::Result<()>`, `remove(&mut self, endpoint: &str) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing tests**

`token.rs`: change `fn write_private` to `pub(super) fn write_private`. Create `store.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // The RFC 8291 receiver: a valid P-256 point and a 16-byte auth secret.
    const P256DH: &str = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

    fn sub(endpoint: &str) -> Subscription {
        Subscription {
            endpoint: endpoint.into(),
            p256dh: P256DH.into(),
            auth: AUTH.into(),
        }
    }

    fn file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("config").join("web_push.json")
    }

    #[test]
    fn only_the_four_push_services_are_allowed() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
            "https://web.push.apple.com/QZ12",
            "https://api.push.apple.com/3/device/x",
            "https://wns2-par02p.notify.windows.com/w/?token=abc",
        ] {
            assert!(endpoint_allowed(ok), "{ok}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com.evil.test/x",
            "https://evil.test/fcm.googleapis.com",
            "https://evil.test/?h=web.push.apple.com",
            "https://web.push.apple.com.evil.test/x",
            "https://user@web.push.apple.com/x",
            "https://web.push.apple.com:8443/x",
            "https://.push.apple.com/x",
            "https://WEB.PUSH.APPLE.COM/x",
            "https://localhost/x",
            "https://127.0.0.1/x",
            "https://192.168.1.5/x",
            "https:///x",
            "ftp://web.push.apple.com/x",
            "",
        ] {
            assert!(!endpoint_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn a_browser_subscription_parses() {
        let body = format!(
            r#"{{"endpoint":"https://fcm.googleapis.com/fcm/send/abc","expirationTime":null,"keys":{{"p256dh":"{P256DH}","auth":"{AUTH}"}}}}"#
        );
        let parsed = Subscription::parse(body.as_bytes()).unwrap();
        assert_eq!(parsed, sub("https://fcm.googleapis.com/fcm/send/abc"));
        let (p256dh, auth) = parsed.keys().unwrap();
        assert_eq!((p256dh.len(), auth.len()), (65, 16));
        assert!(Subscription::parse(b"nope").is_err());
        assert!(Subscription::parse(br#"{"endpoint":"x"}"#).is_err(), "keys missing");
        assert_eq!(endpoint_in(br#"{"endpoint":"https://x"}"#).unwrap(), "https://x");
        assert!(endpoint_in(b"{}").is_err());
    }

    #[test]
    fn add_refuses_other_hosts_bad_keys_and_too_many_phones() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&file(&dir), false).unwrap();
        assert!(store.add(sub("https://evil.test/x")).is_err(), "host");
        let mut short = sub("https://fcm.googleapis.com/fcm/send/a");
        short.auth = "AAAA".into();
        assert!(store.add(short).is_err(), "auth secret size");
        let mut off_curve = sub("https://fcm.googleapis.com/fcm/send/a");
        off_curve.p256dh = B64.encode([4u8; 65]);
        assert!(store.add(off_curve).is_err(), "not a P-256 point");
        assert!(store.subscriptions().is_empty());
        for i in 0..MAX_SUBSCRIPTIONS {
            store.add(sub(&format!("https://fcm.googleapis.com/fcm/send/{i}"))).unwrap();
        }
        assert!(store.add(sub("https://fcm.googleapis.com/fcm/send/extra")).is_err());
        // A phone that is already known may refresh its keys when the list is full.
        store.add(sub("https://fcm.googleapis.com/fcm/send/0")).unwrap();
        assert_eq!(store.subscriptions().len(), MAX_SUBSCRIPTIONS);
    }

    #[test]
    fn the_same_phone_twice_is_one_entry_and_remove_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&file(&dir), false).unwrap();
        let endpoint = "https://web.push.apple.com/abc";
        store.add(sub(endpoint)).unwrap();
        store.add(sub(endpoint)).unwrap();
        assert_eq!(store.subscriptions().len(), 1);
        store.remove("https://web.push.apple.com/other").unwrap();
        assert_eq!(store.subscriptions().len(), 1);
        store.remove(endpoint).unwrap();
        assert!(store.subscriptions().is_empty());
    }

    #[test]
    fn phones_and_key_survive_a_restart_and_new_token_drops_only_the_phones() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        let mut store = Store::open(&path, false).unwrap();
        store.add(sub("https://web.push.apple.com/abc")).unwrap();
        let key = store.vapid().public_key().to_vec();

        let again = Store::open(&path, false).unwrap();
        assert_eq!(again.vapid().public_key(), key);
        assert_eq!(again.subscriptions(), [sub("https://web.push.apple.com/abc")]);

        let renewed = Store::open(&path, true).unwrap();
        assert!(renewed.subscriptions().is_empty());
        assert_eq!(renewed.vapid().public_key(), key, "phones keep working with the same key");
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("web.push.apple.com"), "{on_disk}");
    }

    #[test]
    fn a_corrupt_file_makes_a_fresh_key_instead_of_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for garbage in ["", "not json", r#"{"vapid":"AAAA","subscriptions":[]}"#] {
            std::fs::write(&path, garbage).unwrap();
            let store = Store::open(&path, false).unwrap();
            assert!(store.subscriptions().is_empty(), "{garbage:?}");
            assert_eq!(store.vapid().public_key().len(), 65);
            // And the repaired file loads.
            assert_eq!(
                Store::open(&path, false).unwrap().vapid().public_key(),
                store.vapid().public_key()
            );
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-cli web::push::store -j 4`
Expected: FAIL to compile (`Store`, `Subscription`, … not found).

- [ ] **Step 3: Write the implementation** (above the tests in `store.rs`; add `mod store;` to `push.rs`)

```rust
//! What `mk web` push remembers (ADR 0066): the VAPID key and the phones that opted in, in one
//! private file `<config>/web_push.json`. A subscription URL is a capability to push to that
//! phone, so it is kept like `web_token` and never printed.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde::{Deserialize, Serialize};

use super::crypto::{self, Vapid};
use crate::web::token::write_private;

/// Phones one PC keeps. More than that is a mistake, or someone with the token filling the file.
pub const MAX_SUBSCRIPTIONS: usize = 8;

const HOSTS: [&str; 2] = ["fcm.googleapis.com", "updates.push.services.mozilla.com"];
const HOST_SUFFIXES: [&str; 2] = [".push.apple.com", ".notify.windows.com"];

/// Only the push services of Apple, Google, Mozilla and Microsoft get a request. Whoever holds
/// the page's token can send a subscription; without this check `mk web` would POST to any URL
/// they name from the user's PC. `https`, no user, no port, plain lowercase host.
pub fn endpoint_allowed(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && (HOSTS.contains(&host)
            || HOST_SUFFIXES
                .iter()
                .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    /// The phone's P-256 public key, base64url (65 bytes decoded).
    pub p256dh: String,
    /// The phone's auth secret, base64url (16 bytes decoded).
    pub auth: String,
}

impl Subscription {
    /// From the browser's `PushSubscription.toJSON()`:
    /// `{"endpoint", "expirationTime", "keys": {"p256dh", "auth"}}`.
    pub fn parse(body: &[u8]) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Keys {
            p256dh: String,
            auth: String,
        }
        #[derive(Deserialize)]
        struct Wire {
            endpoint: String,
            keys: Keys,
        }
        let wire: Wire =
            serde_json::from_slice(body).context("the subscription is not the JSON a browser makes")?;
        Ok(Self {
            endpoint: wire.endpoint,
            p256dh: wire.keys.p256dh,
            auth: wire.keys.auth,
        })
    }

    /// The decoded phone key and auth secret.
    pub fn keys(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        Ok((
            B64.decode(&self.p256dh).context("p256dh is not base64url")?,
            B64.decode(&self.auth).context("auth is not base64url")?,
        ))
    }

    fn check(&self) -> anyhow::Result<()> {
        if !endpoint_allowed(&self.endpoint) {
            bail!("that push service is not one Mkrew sends to");
        }
        let (p256dh, auth) = self.keys()?;
        if p256dh.len() != 65 || auth.len() != 16 {
            bail!("the subscription keys have the wrong size");
        }
        // A key that is not a P-256 point fails here, not on the first notification.
        crypto::encrypt(&p256dh, &auth, b"{}").map(drop)
    }
}

/// The `endpoint` of an unsubscribe request, `{"endpoint": "…"}`.
pub fn endpoint_in(body: &[u8]) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Wire {
        endpoint: String,
    }
    Ok(serde_json::from_slice::<Wire>(body)
        .context("the request is not JSON with an endpoint")?
        .endpoint)
}

#[derive(Serialize, Deserialize)]
struct Saved {
    /// The VAPID key, PKCS#8 as base64url.
    vapid: String,
    #[serde(default)]
    subscriptions: Vec<Subscription>,
}

pub struct Store {
    file: PathBuf,
    vapid: Vapid,
    subs: Vec<Subscription>,
}

impl Store {
    /// Loads `file`. A missing or unreadable file makes a new VAPID key (phones must then tap
    /// the bell again; one stderr line says so). `drop_subscriptions` (`mk web --new-token`)
    /// forgets the phones and keeps the key.
    pub fn open(file: &Path, drop_subscriptions: bool) -> anyhow::Result<Self> {
        let (mut store, mut changed) = match std::fs::read_to_string(file) {
            Ok(text) => match Self::parse(file, &text) {
                Ok(store) => (store, false),
                Err(e) => {
                    eprintln!(
                        "mk web: {}: {e:#}; making a new push key (phones must tap the bell again)",
                        file.display()
                    );
                    (Self::fresh(file)?, true)
                }
            },
            Err(_) => (Self::fresh(file)?, true),
        };
        if drop_subscriptions && !store.subs.is_empty() {
            store.subs.clear();
            changed = true;
        }
        if changed {
            store.save()?;
        }
        Ok(store)
    }

    fn fresh(file: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            file: file.to_owned(),
            vapid: Vapid::generate()?,
            subs: Vec::new(),
        })
    }

    fn parse(file: &Path, text: &str) -> anyhow::Result<Self> {
        let saved: Saved = serde_json::from_str(text)?;
        let pkcs8 = B64.decode(&saved.vapid)?;
        Ok(Self {
            file: file.to_owned(),
            vapid: Vapid::from_pkcs8(&pkcs8)?,
            subs: saved.subscriptions,
        })
    }

    pub fn vapid(&self) -> &Vapid {
        &self.vapid
    }

    pub fn subscriptions(&self) -> &[Subscription] {
        &self.subs
    }

    /// Adds a phone, or refreshes the keys of a known one (same endpoint).
    pub fn add(&mut self, sub: Subscription) -> anyhow::Result<()> {
        sub.check()?;
        if let Some(known) = self.subs.iter_mut().find(|s| s.endpoint == sub.endpoint) {
            *known = sub;
        } else if self.subs.len() >= MAX_SUBSCRIPTIONS {
            bail!("this PC already notifies {MAX_SUBSCRIPTIONS} phones");
        } else {
            self.subs.push(sub);
        }
        self.save()
    }

    pub fn remove(&mut self, endpoint: &str) -> anyhow::Result<()> {
        let before = self.subs.len();
        self.subs.retain(|s| s.endpoint != endpoint);
        if self.subs.len() == before {
            return Ok(());
        }
        self.save()
    }

    fn save(&self) -> anyhow::Result<()> {
        let saved = Saved {
            vapid: B64.encode(self.vapid.pkcs8()),
            subscriptions: self.subs.clone(),
        };
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        write_private(&self.file, &serde_json::to_string_pretty(&saved)?)
            .with_context(|| format!("write {}", self.file.display()))
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-cli web::push -j 4`
Expected: PASS (Task 2's 8 + these 6).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli/src/web/push.rs crates/mkrew-cli/src/web/push/store.rs crates/mkrew-cli/src/web/token.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(web): push subscription store with a push-service allowlist"
```

---

### Task 4: The decision, the sender and delivery

**Files:**
- Modify: `crates/mkrew-daemon/src/notify.rs` (`Alert` ~line 103, `decide` ~line 111, `Throttle` ~line 154: add `pub`)
- Modify: `crates/mkrew-daemon/src/lib.rs` (line 40: `pub use notify::ENV_NOTIFY;`)
- Modify: `crates/mkrew-cli/src/web/push.rs` (everything below)
- Test: inline `mod tests` in `push.rs`

**Interfaces:**
- Consumes: `store::{Store, Subscription, endpoint_in}`, `crypto::{encrypt, origin}` (Tasks 2–3); `mkrew_core::settings::{load, Settings, WebSettings}`; `mkrew_daemon::{decide, Throttle}`.
- Produces (in `web::push`):
  - `pub const ENV_PUSH: &str = "MKREW_PUSH";` and `pub fn sender_from_env() -> Arc<dyn Sender>`
  - `pub struct Post { pub url: String, pub headers: Vec<(&'static str, String)>, pub body: Vec<u8> }` (`Clone`)
  - `pub enum Sent { Delivered, Gone, Failed(String) }` (`Clone`); `pub trait Sender: Send + Sync + 'static { fn send(&self, post: &Post) -> Sent; }`; `pub struct UreqSender;` (`ureq`)
  - `pub struct Payload { pub title: String, pub body: String, pub session: String }` (`Serialize`), `pub fn payload(session_id: Ulid, name: &str, project: &str, state: SessionState) -> Option<Payload>`
  - `pub fn wanted(settings: &Settings, subscribers: usize, session_id: Ulid, state: SessionState, throttle: &mut Throttle, now: Instant) -> bool`
  - `pub enum Reply { Done, Off, Bad(String) }`
  - `pub struct Push` with `open(file: &Path, settings_file: PathBuf, renew: bool, sender: Arc<dyn Sender>) -> anyhow::Result<Push>`, `enabled(&self) -> bool`, `settings(&self) -> Option<Settings>`, `subscribers(&self) -> usize`, `public_key(&self) -> Option<String>`, `subscribe(&self, body: &[u8]) -> Reply`, `unsubscribe(&self, body: &[u8]) -> Reply`, `deliver(&self, payload: &Payload, now: u64)` (blocking), `async fn notify(self: Arc<Self>, payload: Payload)`.

- [ ] **Step 1: Export the §8 policy from the daemon**

In `notify.rs` change `struct Alert {` → `pub struct Alert {`, `fn decide(` → `pub fn decide(`, `struct Throttle {` → `pub struct Throttle {` (leave fields private). Add to the doc of `decide`: `/// Also used by \`mk web\` for phone push (ADR 0066): one policy for toast and push.` In `lib.rs` replace `pub use notify::ENV_NOTIFY;` with `pub use notify::{Alert, ENV_NOTIFY, Throttle, decide};`. (`Throttle` already derives `Default`.)

Run `cargo test -p mkrew-daemon notify -j 4` — Expected: PASS (nothing else changed).

- [ ] **Step 2: Write the failing tests** (bottom of `push.rs`)

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mkrew_core::settings::WebSettings;

    use super::*;

    const SUB_JSON: &str = r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc","expirationTime":null,"keys":{"p256dh":"BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4","auth":"BTBZMqHH6r4Tts7J_aSIgg"}}"#;
    const OTHER_JSON: &str = r#"{"endpoint":"https://web.push.apple.com/zzz","keys":{"p256dh":"BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4","auth":"BTBZMqHH6r4Tts7J_aSIgg"}}"#;

    /// Records every post; answers per endpoint (default: delivered).
    struct Fake {
        posts: Mutex<Vec<Post>>,
        answers: HashMap<String, Sent>,
    }

    impl Sender for Fake {
        fn send(&self, post: &Post) -> Sent {
            self.posts.lock().unwrap().push(post.clone());
            self.answers.get(&post.url).cloned().unwrap_or(Sent::Delivered)
        }
    }

    fn push_with(on: bool, answers: &[(&str, Sent)]) -> (tempfile::TempDir, Push, Arc<Fake>) {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.toml");
        std::fs::write(&settings, format!("[web]\npush = {on}\n")).unwrap();
        let fake = Arc::new(Fake {
            posts: Mutex::new(Vec::new()),
            answers: answers.iter().map(|(u, s)| ((*u).to_owned(), s.clone())).collect(),
        });
        let sender: Arc<dyn Sender> = fake.clone();
        let push = Push::open(&dir.path().join("web_push.json"), settings, false, sender).unwrap();
        (dir, push, fake)
    }

    fn note() -> Payload {
        Payload {
            title: "omp · mkrew".into(),
            body: "needs you".into(),
            session: "01J00000000000000000000000".into(),
        }
    }

    fn settings(push: bool) -> Settings {
        Settings {
            web: WebSettings { enabled: true, push },
            ..Settings::default()
        }
    }

    #[test]
    fn wanted_follows_the_switches_the_phones_and_the_30_second_limit() {
        let t0 = Instant::now();
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut throttle = Throttle::default();
        // Push off, or nobody subscribed: nothing, and the window is not used up.
        assert!(!wanted(&settings(false), 1, a, SessionState::NeedsInput, &mut throttle, t0));
        assert!(!wanted(&settings(true), 0, a, SessionState::NeedsInput, &mut throttle, t0));
        assert!(wanted(&settings(true), 1, a, SessionState::NeedsInput, &mut throttle, t0));
        // The same session again within 30 s: no (SPEC §8); another session: yes.
        assert!(!wanted(&settings(true), 1, a, SessionState::Done, &mut throttle, t0 + Duration::from_secs(5)));
        assert!(wanted(&settings(true), 1, b, SessionState::Done, &mut throttle, t0 + Duration::from_secs(5)));
        assert!(wanted(&settings(true), 1, a, SessionState::Done, &mut throttle, t0 + Duration::from_secs(31)));
        // Only needs_input and done.
        for state in [SessionState::Working, SessionState::Idle, SessionState::Exited, SessionState::Starting] {
            assert!(!wanted(&settings(true), 1, Ulid::new(), state, &mut throttle, t0), "{state:?}");
        }
    }

    #[test]
    fn the_notify_switches_apply_to_phones_too() {
        let t0 = Instant::now();
        let mut throttle = Throttle::default();
        let mut quiet = settings(true);
        quiet.notify.needs_input = false;
        assert!(!wanted(&quiet, 1, Ulid::new(), SessionState::NeedsInput, &mut throttle, t0));
        assert!(wanted(&quiet, 1, Ulid::new(), SessionState::Done, &mut throttle, t0));
        quiet.notify.done = false;
        assert!(!wanted(&quiet, 1, Ulid::new(), SessionState::Done, &mut throttle, t0));
    }

    #[test]
    fn the_payload_names_the_session_and_clips_long_text() {
        let id = Ulid::new();
        let needs = payload(id, "omp", "mkrew", SessionState::NeedsInput).unwrap();
        assert_eq!(
            (needs.title.as_str(), needs.body.as_str(), needs.session.as_str()),
            ("omp · mkrew", "needs you", id.to_string().as_str())
        );
        assert_eq!(payload(id, "omp", "mkrew", SessionState::Done).unwrap().body, "done");
        assert_eq!(payload(id, "omp", "mkrew", SessionState::Working), None);
        let long = "é".repeat(500);
        let clipped = payload(id, &long, &long, SessionState::Done).unwrap();
        assert!(clipped.title.chars().count() <= 60 + 3 + 60, "{}", clipped.title);
        assert!(clipped.title.contains('…'));
        assert!(serde_json::to_vec(&clipped).unwrap().len() < crypto::MAX_PLAINTEXT);
    }

    #[test]
    fn the_key_and_subscriptions_exist_only_while_push_is_on() {
        let (_dir, off, _) = push_with(false, &[]);
        assert_eq!(off.public_key(), None);
        assert!(matches!(off.subscribe(SUB_JSON.as_bytes()), Reply::Off));
        assert_eq!(off.subscribers(), 0);

        let (_dir, on, _) = push_with(true, &[]);
        assert_eq!(on.public_key().unwrap().len(), 87, "65 bytes as base64url");
        assert!(matches!(on.subscribe(SUB_JSON.as_bytes()), Reply::Done));
        assert!(matches!(on.subscribe(b"nope"), Reply::Bad(_)));
        let evil = SUB_JSON.replace("fcm.googleapis.com", "evil.test");
        let Reply::Bad(why) = on.subscribe(evil.as_bytes()) else { panic!("accepted") };
        assert!(why.contains("not one Mkrew sends to"), "{why}");
        assert_eq!(on.subscribers(), 1);
        assert!(matches!(on.unsubscribe(br#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc"}"#), Reply::Done));
        assert_eq!(on.subscribers(), 0);
    }

    #[test]
    fn a_broken_settings_file_means_push_is_off() {
        let (dir, push, _) = push_with(true, &[]);
        assert!(push.enabled());
        std::fs::write(dir.path().join("settings.toml"), "[web\npush = true").unwrap();
        assert!(!push.enabled());
        assert_eq!(push.public_key(), None);
        std::fs::remove_file(dir.path().join("settings.toml")).unwrap();
        assert!(!push.enabled(), "no file = the defaults = off");
    }

    #[test]
    fn deliver_sends_one_encrypted_vapid_post_per_phone() {
        let (_dir, push, fake) = push_with(true, &[]);
        push.subscribe(SUB_JSON.as_bytes());
        push.subscribe(OTHER_JSON.as_bytes());
        push.deliver(&note(), 1_700_000_000);
        let posts = fake.posts.lock().unwrap();
        assert_eq!(posts.len(), 2);
        let header = |post: &Post, name: &str| {
            post.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.clone())
        };
        for post in posts.iter() {
            assert!(header(post, "Authorization").unwrap().starts_with("vapid t="));
            assert_eq!(header(post, "Content-Encoding").as_deref(), Some("aes128gcm"));
            assert_eq!(header(post, "TTL").as_deref(), Some("3600"));
            assert_eq!(header(post, "Urgency").as_deref(), Some("high"));
            let text = serde_json::to_vec(&note()).unwrap();
            assert_eq!(post.body.len(), 86 + text.len() + 1 + 16);
            assert!(!String::from_utf8_lossy(&post.body).contains("needs you"), "ciphertext only");
        }
        let urls: Vec<&str> = posts.iter().map(|p| p.url.as_str()).collect();
        assert_eq!(urls, ["https://fcm.googleapis.com/fcm/send/abc", "https://web.push.apple.com/zzz"]);
    }

    #[test]
    fn a_gone_phone_is_dropped_a_failing_one_stays_and_the_others_still_get_it() {
        let gone = "https://fcm.googleapis.com/fcm/send/abc";
        let flaky = "https://web.push.apple.com/zzz";
        let (dir, push, fake) = push_with(
            true,
            &[(gone, Sent::Gone), (flaky, Sent::Failed("HTTP 503".into()))],
        );
        push.subscribe(SUB_JSON.as_bytes());
        push.subscribe(OTHER_JSON.as_bytes());
        push.deliver(&note(), 1_700_000_000);
        assert_eq!(fake.posts.lock().unwrap().len(), 2, "the first failure does not stop the second");
        assert_eq!(push.subscribers(), 1, "404/410 removes the phone");
        let saved = std::fs::read_to_string(dir.path().join("web_push.json")).unwrap();
        assert!(!saved.contains(gone) && saved.contains(flaky), "{saved}");
    }

    #[test]
    fn the_file_sender_appends_one_json_line_per_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sink.jsonl");
        let sender = FileSender(path.clone());
        let post = Post {
            url: "https://web.push.apple.com/x".into(),
            headers: vec![("TTL", "3600".into())],
            body: vec![0; 5],
        };
        assert!(matches!(sender.send(&post), Sent::Delivered));
        assert!(matches!(sender.send(&post), Sent::Delivered));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["url"], "https://web.push.apple.com/x");
        assert_eq!(lines[0]["body_len"], 5);
        assert_eq!(lines[0]["headers"][0][0], "TTL");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p mkrew-cli web::push -j 4`
Expected: FAIL to compile (`Push`, `Sender`, `wanted`, … not found).

- [ ] **Step 4: Implement** (replace `push.rs` above the tests; keep `mod crypto; mod store;`)

```rust
//! Web Push from `mk web` (ADR 0066): when a session enters needs_input or done, every phone
//! that tapped the bell gets a system notification through its browser's push service. The
//! policy is the desktop toast's (`mkrew_daemon::decide`: `[notify]` switches, one per session
//! per 30 s) plus `[web] push`. Nothing here talks to the network except [`UreqSender`]; tests
//! use a fake [`Sender`], and `MKREW_PUSH=<file>` swaps in a file sink.

mod crypto;
mod store;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use mkrew_core::settings::{self, Settings};
use mkrew_daemon::{Throttle, decide};
use mkrew_protocol::SessionState;
use serde::Serialize;
use ulid::Ulid;

use store::{Store, Subscription, endpoint_in};

/// `MKREW_PUSH=<file>`: append each message to the file as a JSON line instead of sending it
/// (tests read it, like `MKREW_NOTIFY`).
pub const ENV_PUSH: &str = "MKREW_PUSH";
/// How long a push service holds a message for a phone that is off: a notice older than this is noise.
const TTL_SECONDS: &str = "3600";
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest session or workspace name in a notification.
const NAME_CHARS: usize = 60;

/// One HTTPS POST to a push service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    Delivered,
    /// The push service says the subscription is gone (404 / 410): forget the phone.
    Gone,
    Failed(String),
}

pub trait Sender: Send + Sync + 'static {
    /// Blocking.
    fn send(&self, post: &Post) -> Sent;
}

/// The real thing: `ureq` over rustls.
pub struct UreqSender;

impl Sender for UreqSender {
    fn send(&self, post: &Post) -> Sent {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(SEND_TIMEOUT))
            // 4xx / 5xx are answers to read, not transport errors.
            .http_status_as_error(false)
            .build()
            .into();
        let mut request = agent.post(post.url.as_str());
        for (name, value) in &post.headers {
            request = request.header(*name, value.as_str());
        }
        match request.send(&post.body[..]) {
            Ok(response) => match response.status().as_u16() {
                200..=299 => Sent::Delivered,
                404 | 410 => Sent::Gone,
                other => Sent::Failed(format!("HTTP {other}")),
            },
            Err(e) => Sent::Failed(e.to_string()),
        }
    }
}

/// `MKREW_PUSH` sink: one JSON line per message (url, headers, body length).
struct FileSender(PathBuf);

impl Sender for FileSender {
    fn send(&self, post: &Post) -> Sent {
        let line = serde_json::json!({
            "url": post.url,
            "headers": post.headers,
            "body_len": post.body.len(),
        });
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .and_then(|mut file| writeln!(file, "{line}"));
        match written {
            Ok(()) => Sent::Delivered,
            Err(e) => Sent::Failed(e.to_string()),
        }
    }
}

pub fn sender_from_env() -> Arc<dyn Sender> {
    match std::env::var(ENV_PUSH).ok().filter(|v| !v.is_empty()) {
        Some(path) => Arc::new(FileSender(PathBuf::from(path))),
        None => Arc::new(UreqSender),
    }
}

/// What the phone shows; encrypted into the push message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payload {
    pub title: String,
    pub body: String,
    /// The session id the page opens when the notification is tapped.
    pub session: String,
}

/// `session · workspace` / `needs you` or `done`; `None` for any other state.
pub fn payload(session_id: Ulid, name: &str, project: &str, state: SessionState) -> Option<Payload> {
    let body = match state {
        SessionState::NeedsInput => "needs you",
        SessionState::Done => "done",
        _ => return None,
    };
    Some(Payload {
        title: format!("{} · {}", clip(name), clip(project)),
        body: body.to_owned(),
        session: session_id.to_string(),
    })
}

/// At most [`NAME_CHARS`] characters, cut on a character boundary, with `…` when cut.
fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= NAME_CHARS {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(NAME_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// Phones should be told: push is on, somebody subscribed, and the §8 policy allows it now.
/// Cheap checks first, so a suppressed event never uses up the session's 30 s window.
pub fn wanted(
    settings: &Settings,
    subscribers: usize,
    session_id: Ulid,
    state: SessionState,
    throttle: &mut Throttle,
    now: Instant,
) -> bool {
    settings.web.push
        && subscribers > 0
        && decide(session_id, state, &settings.notify, throttle, now).is_some()
}

/// The answer to a subscribe / unsubscribe request.
#[derive(Debug)]
pub enum Reply {
    Done,
    /// `[web] push` is off on this PC.
    Off,
    Bad(String),
}

pub struct Push {
    settings_file: PathBuf,
    store: Mutex<Store>,
    sender: Arc<dyn Sender>,
}

impl Push {
    /// `renew` (`mk web --new-token`) forgets the phones; the VAPID key stays.
    pub fn open(
        file: &Path,
        settings_file: PathBuf,
        renew: bool,
        sender: Arc<dyn Sender>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            settings_file,
            store: Mutex::new(Store::open(file, renew)?),
            sender,
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `settings.toml` as it is now. `None` when it cannot be read: push then stays off.
    pub fn settings(&self) -> Option<Settings> {
        settings::load(&self.settings_file).ok()
    }

    pub fn enabled(&self) -> bool {
        self.settings().is_some_and(|s| s.web.push)
    }

    pub fn subscribers(&self) -> usize {
        self.store().subscriptions().len()
    }

    /// The VAPID public key (base64url) the page subscribes with; `None` while push is off.
    pub fn public_key(&self) -> Option<String> {
        self.enabled()
            .then(|| B64.encode(self.store().vapid().public_key()))
    }

    pub fn subscribe(&self, body: &[u8]) -> Reply {
        if !self.enabled() {
            return Reply::Off;
        }
        match Subscription::parse(body).and_then(|sub| self.store().add(sub)) {
            Ok(()) => Reply::Done,
            Err(e) => Reply::Bad(format!("{e:#}")),
        }
    }

    /// Always allowed, even with push off: a phone must be able to leave.
    pub fn unsubscribe(&self, body: &[u8]) -> Reply {
        match endpoint_in(body).and_then(|endpoint| self.store().remove(&endpoint)) {
            Ok(()) => Reply::Done,
            Err(e) => Reply::Bad(format!("{e:#}")),
        }
    }

    /// Sends `payload` to every phone at unix time `now`; a phone the push service reports gone
    /// is forgotten. Blocking (up to [`SEND_TIMEOUT`] per phone): run it off the async threads.
    pub fn deliver(&self, payload: &Payload, now: u64) {
        let Ok(text) = serde_json::to_vec(payload) else {
            return;
        };
        let phones = self.store().subscriptions().to_vec();
        let mut gone = Vec::new();
        for phone in &phones {
            let host = crypto::origin(&phone.endpoint).unwrap_or("?");
            match self.post(phone, &text, now) {
                Ok(Sent::Delivered) => {}
                Ok(Sent::Gone) => gone.push(phone.endpoint.clone()),
                Ok(Sent::Failed(why)) => eprintln!("mk web: push to {host}: {why}"),
                Err(e) => eprintln!("mk web: push to {host}: {e:#}"),
            }
        }
        for endpoint in gone {
            if let Err(e) = self.store().remove(&endpoint) {
                eprintln!("mk web: push: {e:#}");
            }
        }
    }

    fn post(&self, phone: &Subscription, text: &[u8], now: u64) -> anyhow::Result<Sent> {
        let (p256dh, auth) = phone.keys()?;
        let body = crypto::encrypt(&p256dh, &auth, text)?;
        let authorization = self.store().vapid().authorization(&phone.endpoint, now)?;
        Ok(self.sender.send(&Post {
            url: phone.endpoint.clone(),
            headers: vec![
                ("Authorization", authorization),
                ("Content-Encoding", "aes128gcm".to_owned()),
                ("TTL", TTL_SECONDS.to_owned()),
                ("Urgency", "high".to_owned()),
            ],
            body,
        }))
    }

    /// [`Push::deliver`] on a blocking thread.
    pub async fn notify(self: Arc<Self>, payload: Payload) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Err(e) = tokio::task::spawn_blocking(move || self.deliver(&payload, now)).await {
            eprintln!("mk web: push: {e}");
        }
    }
}
```

Add `use mkrew_protocol::SessionState;` — already above. (`Request`, `Session`, `relay` imports arrive in Task 5.)

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p mkrew-cli web::push -j 4`
Expected: PASS. Then `cargo clippy -p mkrew-cli -p mkrew-daemon --all-targets -j 4 -- -D warnings`. Clippy may warn `dead_code` for `Push`, `Reply` and `payload`, which only Task 5 uses: if it warns about nothing else, move on and run clippy again after Task 5 (do not add `allow` attributes).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-daemon/src/notify.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-cli/src/web/push.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(web): push delivery, sender trait and the shared notification policy"
```

---

### Task 5: Event loop, HTTP routes and wiring into `mk web`

**Files:**
- Modify: `crates/mkrew-cli/src/web/relay.rs` (`impl Daemon` after the struct at line 49-53)
- Modify: `crates/mkrew-cli/src/web/push.rs` (add `run`, `lookup`)
- Modify: `crates/mkrew-cli/src/web/http.rs` (`ASSETS`, `Head`, `parse_head`, `Route`, `route`, `response`, tests)
- Modify: `crates/mkrew-cli/src/web.rs` (`run`, `serve`, `handle`, new `Shared`, `read_body`)
- Create: `crates/mkrew-cli/web/sw.js` (content in Task 6; create it here with the final content so `include_bytes!` compiles)
- Modify: `crates/mkrew-cli/tests/cli.rs` (`Env::web`, `Web::get`, new `Web::post`, new test)
- Test: `http.rs` tests, `cli.rs`

**Interfaces:**
- Consumes: Task 4's `Push`, `Reply`, `payload`, `wanted`, `sender_from_env`.
- Produces:
  - `relay::Daemon::next_event(&mut self) -> anyhow::Result<Event>`; `relay::Daemon::call(self, Request) -> anyhow::Result<Outcome>`
  - HTTP: `GET /push/key?t=` → 200 base64url public key / 404 when push is off; `POST /push/subscribe?t=` and `/push/unsubscribe?t=` (JSON body ≤ 4096 bytes) → 204 / 400 (bad JSON, refused host, body too large) / 404 (push off); `GET /manifest.webmanifest?t=` → manifest JSON; `GET /sw.js` → the service worker.
  - `http::Route::{PushKey, PushSubscribe, PushUnsubscribe, Manifest(String)}`, `http::Head.content_length: usize`, `http::plain(status, body) -> Vec<u8>`.

- [ ] **Step 1: Write the failing `http.rs` tests**

Add to `mod tests` in `http.rs`, and in `parses_an_upgrade_request` add `content_length: 0,` to the expected `Head`:

```rust
    #[test]
    fn push_and_manifest_routes_need_the_token() {
        let good = "ab".repeat(32);
        let get = |path: &str| route(&head(&format!("GET {path} HTTP/1.1\r\n\r\n")), &token());
        let post = |path: &str| {
            route(
                &head(&format!("POST {path} HTTP/1.1\r\nContent-Length: 2\r\n\r\n")),
                &token(),
            )
        };
        assert!(matches!(get(&format!("/push/key?t={good}")), Route::PushKey));
        assert!(matches!(get("/push/key"), Route::Unauthorized));
        assert!(matches!(post(&format!("/push/subscribe?t={good}")), Route::PushSubscribe));
        assert!(matches!(post(&format!("/push/unsubscribe?t={good}")), Route::PushUnsubscribe));
        assert!(matches!(post("/push/subscribe"), Route::Unauthorized));
        assert!(matches!(post(&format!("/push/key?t={good}")), Route::MethodNotAllowed));
        assert!(matches!(get(&format!("/push/subscribe?t={good}")), Route::NotFound));
        assert!(matches!(get("/manifest.webmanifest"), Route::Unauthorized));
        match get(&format!("/manifest.webmanifest?t={good}")) {
            Route::Manifest(body) => {
                let json: serde_json::Value = serde_json::from_str(&body).unwrap();
                // The Home Screen app keeps its own storage: the token rides in start_url.
                assert_eq!(json["start_url"], format!("/#t={good}"));
                assert_eq!(json["display"], "standalone");
                assert_eq!(json["scope"], "/");
            }
            other => panic!("{other:?}"),
        }
        let Route::Asset(sw) = get("/sw.js") else {
            panic!("no service worker");
        };
        assert_eq!(sw.content_type, "text/javascript; charset=utf-8");
        assert!(sw.body.starts_with(b"// Mkrew web push"));
    }

    #[test]
    fn the_request_body_length_is_read() {
        assert_eq!(head("POST /x HTTP/1.1\r\ncontent-length: 12\r\n\r\n").content_length, 12);
        assert_eq!(head("POST /x HTTP/1.1\r\n\r\n").content_length, 0);
        assert_eq!(head("POST /x HTTP/1.1\r\nContent-Length: abc\r\n\r\n").content_length, 0);
    }

    #[test]
    fn plain_answers_are_never_cached() {
        let text = String::from_utf8(plain("404 Not Found", "off\n")).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"), "{text}");
        assert!(text.contains("Cache-Control: no-store\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\noff\n"), "{text}");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-cli web::http -j 4`
Expected: FAIL to compile (`Route::PushKey` etc. missing; `sw.js` missing).

- [ ] **Step 3: Implement `http.rs` changes**

1. `ASSETS`: append after the `apple-touch-icon.png` asset (so `ASSETS[0]` stays the page):
```rust
    Asset {
        path: "/sw.js",
        content_type: JS,
        body: include_bytes!("../../web/sw.js"),
    },
```
2. `Head` gets `/// \`Content-Length\`, 0 when absent or not a number.` `pub content_length: usize,`. In `parse_head` add `let mut content_length = 0;`, in the header loop an arm `else if name.eq_ignore_ascii_case("content-length") { content_length = value.parse().unwrap_or(0); }`, and `content_length,` in the returned `Head`.
3. `Route` gets:
```rust
    /// `GET /push/key` with the right token: answered with the VAPID public key (or 404 while
    /// `[web] push` is off) by `web.rs`.
    PushKey,
    /// `POST /push/subscribe` / `/push/unsubscribe` with the right token: the body is read by `web.rs`.
    PushSubscribe,
    PushUnsubscribe,
    /// `/manifest.webmanifest` with the right token: the Home Screen app's manifest.
    Manifest(String),
```
4. `route`: replace the first line (`if head.method != "GET" { return Route::MethodNotAllowed; }`) and extend the match:
```rust
pub fn route(head: &Head, token: &Token) -> Route {
    let authorized = token.matches(head.token.as_deref());
    if head.method == "POST" {
        return match (head.path.as_str(), authorized) {
            ("/push/subscribe", true) => Route::PushSubscribe,
            ("/push/unsubscribe", true) => Route::PushUnsubscribe,
            ("/push/subscribe" | "/push/unsubscribe", false) => Route::Unauthorized,
            _ => Route::MethodNotAllowed,
        };
    }
    if head.method != "GET" {
        return Route::MethodNotAllowed;
    }
    match head.path.as_str() {
        "/check" if authorized => Route::Check,
        "/check" => Route::Unauthorized,
        "/push/key" if authorized => Route::PushKey,
        "/manifest.webmanifest" if authorized => Route::Manifest(manifest(token)),
        "/push/key" | "/manifest.webmanifest" => Route::Unauthorized,
        // … the existing "/ws" and asset arms stay as they are
```
and add
```rust
/// The Web App Manifest. `start_url` carries the token: an iOS Home Screen app has its own
/// storage, so it could not find the token the page saved in Safari (ADR 0052, ADR 0066).
fn manifest(token: &Token) -> String {
    serde_json::json!({
        "name": "Mkrew",
        "short_name": "Mkrew",
        "start_url": format!("/#t={}", token.as_str()),
        "scope": "/",
        "display": "standalone",
        "background_color": "#17181C",
        "theme_color": "#17181C",
        "icons": [{ "src": "/apple-touch-icon.png", "sizes": "180x180", "type": "image/png" }],
    })
    .to_string()
}
```
5. `Route::response`: add arms (the `PushKey | PushSubscribe | PushUnsubscribe` ones are answered by `web.rs`; give them the same fallback as `Upgrade`):
```rust
            Route::Manifest(body) => respond(
                "200 OK",
                &[
                    ("Content-Type", "application/manifest+json"),
                    ("Cache-Control", "no-store"),
                ],
                body.as_bytes(),
            ),
            Route::BadRequest
            | Route::Upgrade { .. }
            | Route::PushKey
            | Route::PushSubscribe
            | Route::PushUnsubscribe => respond("400 Bad Request", &[], b"bad request\n"),
```
(replacing the existing `BadRequest | Upgrade` arm), and change `MethodNotAllowed` to `respond("405 Method Not Allowed", &[("Allow", "GET, POST")], b"GET or POST only\n")`. Update the module doc line 1-2 to say "GET, and POST for the push routes".
6. New helper next to `unavailable`:
```rust
/// A plain-text answer of the push routes, never cached.
pub fn plain(status: &str, body: &str) -> Vec<u8> {
    respond(
        status,
        &[
            ("Content-Type", "text/plain; charset=utf-8"),
            ("Cache-Control", "no-store"),
        ],
        body.as_bytes(),
    )
}
```

Create `crates/mkrew-cli/web/sw.js` with the exact content of Task 6 Step 3 (the test above pins its first line).

- [ ] **Step 4: Run `http` tests**

Run: `cargo test -p mkrew-cli web::http -j 4`
Expected: PASS.

- [ ] **Step 5: `relay.rs` — two helpers on `Daemon`**

After `pub struct Daemon { … }` (line 53) add:

```rust
impl Daemon {
    /// The next event the daemon pushes; replies and output on the way are skipped. `mk web`'s
    /// push loop (ADR 0066) lives on one of these connections.
    pub async fn next_event(&mut self) -> anyhow::Result<Event> {
        let mut buf = vec![0u8; READ_BUFFER];
        loop {
            while let Some(frame) = self.decoder.next_frame()? {
                if let Frame::Message(Message::Event(event)) = frame {
                    return Ok(event);
                }
            }
            let n = self.rx.read(&mut buf).await?;
            if n == 0 {
                bail!("the Mkrew daemon closed the connection");
            }
            self.decoder.push(&buf[..n]);
        }
    }

    /// Sends one request and waits for its reply; events on the way are skipped.
    pub async fn call(mut self, request: Request) -> anyhow::Result<Outcome> {
        const ID: u64 = 1;
        let frame = Frame::Message(Message::Request { id: ID, request });
        self.tx.write_all(&encode(&frame)?).await?;
        let mut buf = vec![0u8; READ_BUFFER];
        loop {
            while let Some(frame) = self.decoder.next_frame()? {
                if let Frame::Message(Message::Response { id: ID, outcome }) = frame {
                    return Ok(outcome);
                }
            }
            let n = self.rx.read(&mut buf).await?;
            if n == 0 {
                bail!("the Mkrew daemon closed the connection");
            }
            self.decoder.push(&buf[..n]);
        }
    }
}
```

- [ ] **Step 6: `push.rs` — the event loop**

Add imports `use mkrew_protocol::{Event, Project, Request, Session, SessionListParams};` (extend the existing `SessionState` import) and `use super::relay;`; then:

```rust
/// Watches the daemon's `session.state` events for ever and notifies the phones. A daemon that
/// goes away (it restarts, the PC wakes from sleep) is waited for, not a reason to stop.
pub async fn run(push: Arc<Push>, socket: String) {
    let mut last_error = String::new();
    loop {
        if let Err(e) = watch(&push, &socket).await {
            let text = format!("{e:#}");
            if text != last_error {
                eprintln!("mk web: push: {text}");
                last_error = text;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn watch(push: &Arc<Push>, socket: &str) -> anyhow::Result<()> {
    let mut daemon = relay::open_daemon(socket).await?;
    let mut throttle = Throttle::default();
    loop {
        let Event::SessionState(change) = daemon.next_event().await? else {
            continue;
        };
        let Some(settings) = push.settings() else {
            continue;
        };
        let (id, state) = (change.session_id, change.state);
        if !wanted(&settings, push.subscribers(), id, state, &mut throttle, Instant::now()) {
            continue;
        }
        match lookup(socket, id).await {
            Ok(Some((name, project))) => {
                if let Some(note) = payload(id, &name, &project, state) {
                    tokio::spawn(Arc::clone(push).notify(note));
                }
            }
            // The session ended between the event and the lookup.
            Ok(None) => {}
            Err(e) => eprintln!("mk web: push: {e:#}"),
        }
    }
}

/// The (session name, workspace name) of a session: its title, else the agent's own terminal
/// title, else the profile id (the desktop toast's order). Each request uses a connection of its
/// own so the event connection never has to stop reading.
async fn lookup(socket: &str, session_id: Ulid) -> anyhow::Result<Option<(String, String)>> {
    let sessions: Vec<Session> = relay::open_daemon(socket)
        .await?
        .call(Request::SessionList(SessionListParams::default()))
        .await?
        .into_result()?;
    let Some(session) = sessions.into_iter().find(|s| s.id == session_id) else {
        return Ok(None);
    };
    let projects: Vec<Project> = relay::open_daemon(socket)
        .await?
        .call(Request::ProjectList)
        .await?
        .into_result()?;
    let Some(project) = projects.into_iter().find(|p| p.id == session.project_id) else {
        return Ok(None);
    };
    let name = session
        .title
        .or(session.terminal_title)
        .unwrap_or(session.agent_profile);
    Ok(Some((name, project.name)))
}
```

- [ ] **Step 7: `web.rs` — wire it in**

Replace `run`/`serve`/`handle` parts (keep `read_head` and the head constants):

```rust
pub fn run(paths: &Paths, args: Args) -> anyhow::Result<ExitCode> {
    // … unchanged until block_on
    runtime.block_on(serve(
        args.listen,
        host,
        paths.socket.clone(),
        paths.config_dir.clone(),
        paths.settings_file(),
        args.new_token,
    ))
}

/// What every connection handler shares.
struct Shared {
    socket: String,
    token: token::Token,
    push: Arc<push::Push>,
}

/// A subscription is a few hundred bytes; anything bigger is not from the page.
const BODY_LIMIT: usize = 4096;

async fn serve(
    listen: SocketAddr,
    host: Option<String>,
    socket: String,
    config_dir: PathBuf,
    settings_file: PathBuf,
    new_token: bool,
) -> anyhow::Result<ExitCode> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("listen on {listen}"))?;
    // After the bind: a second `mk web --new-token` must not replace the token of the running one.
    let token = token::load_or_create(&config_dir.join("web_token"), new_token)?;
    // `--new-token` revokes the old links, so it also forgets the phones that got push through them.
    let push = Arc::new(push::Push::open(
        &config_dir.join("web_push.json"),
        settings_file,
        new_token,
        push::sender_from_env(),
    )?);
    // … the existing println! block (listening address, link, tailscale hint) stays here
    tokio::spawn(push::run(Arc::clone(&push), socket.clone()));
    let shared = Arc::new(Shared { socket, token, push });
    loop {
        // … accept as before
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            if let Err(e) = handle(stream, &shared).await {
                eprintln!("mk web: {e:#}");
            }
        });
    }
}

async fn handle(mut stream: TcpStream, shared: &Shared) -> anyhow::Result<()> {
    let (head, rest) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut stream))
        .await
        .context("the request head did not arrive in time")??;
    let parsed = http::parse_head(&head);
    let route = parsed
        .as_ref()
        .map_or(http::Route::BadRequest, |head| http::route(head, &shared.token));
    let length = parsed.map_or(0, |head| head.content_length);
    match &route {
        http::Route::Upgrade { key } => {
            // Connect first: a browser must never see an open WebSocket that dies at once.
            let daemon = match relay::open_daemon(&shared.socket).await {
                Ok(daemon) => daemon,
                Err(e) => {
                    stream
                        .write_all(&http::unavailable(&format!("{e:#}")))
                        .await?;
                    stream.shutdown().await?;
                    return Err(e);
                }
            };
            stream.write_all(&http::switching_protocols(key)).await?;
            let ws = WebSocketStream::from_partially_read(stream, rest, Role::Server, None).await;
            return relay::relay(ws, daemon).await;
        }
        http::Route::PushKey => {
            let key_answer = match shared.push.public_key() {
                Some(key) => http::plain("200 OK", &key),
                None => answer(push::Reply::Off),
            };
            stream.write_all(&key_answer).await?;
        }
        http::Route::PushSubscribe | http::Route::PushUnsubscribe => {
            let reply = if length > BODY_LIMIT {
                push::Reply::Bad("the request is too large".into())
            } else {
                let body = read_body(&mut stream, rest, length).await?;
                if matches!(route, http::Route::PushSubscribe) {
                    shared.push.subscribe(&body)
                } else {
                    shared.push.unsubscribe(&body)
                }
            };
            stream.write_all(&answer(reply)).await?;
        }
        _ => stream.write_all(&route.response()).await?,
    }
    stream.shutdown().await?;
    Ok(())
}

/// The answer to a push request: 204 like `/check`, 404 while push is off, 400 with the reason.
fn answer(reply: push::Reply) -> Vec<u8> {
    match reply {
        push::Reply::Done => http::Route::Check.response(),
        push::Reply::Off => http::plain("404 Not Found", "phone notifications are off on this PC\n"),
        push::Reply::Bad(why) => http::plain("400 Bad Request", &format!("{why}\n")),
    }
}

/// `length` bytes of request body: what arrived with the head, then the rest.
async fn read_body(
    stream: &mut TcpStream,
    mut body: Vec<u8>,
    length: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut chunk = [0u8; 1024];
    while body.len() < length {
        let n = tokio::time::timeout(HEAD_TIMEOUT, stream.read(&mut chunk))
            .await
            .context("the request body did not arrive in time")??;
        if n == 0 {
            bail!("the connection closed before the request body ended");
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok(body)
}
```
Also print one line after the existing `println!`s: `if push.enabled() { println!("Phone notifications are on (ADR 0066): phones that tapped the bell are notified through their browser's push service."); }`.

- [ ] **Step 8: Write the end-to-end test** (`crates/mkrew-cli/tests/cli.rs`)

Refactor the helpers: change `Env::web` into a wrapper and give `Web` a raw sender.

```rust
impl Env {
    /// Starts `mk web` (which starts the daemon when none runs). `--host` keeps `tailscale` out.
    fn web(&self) -> Web {
        self.web_with(|_| {})
    }

    fn web_with(&self, configure: impl FnOnce(&mut Command)) -> Web {
        let mut command = self.command(&["web", "--listen", "127.0.0.1:0", "--host", "test.invalid"]);
        configure(&mut command);
        let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
        // … the rest of the old `web()` body (read the address and token) unchanged
    }
}

impl Web {
    /// `GET target` → (status, body).
    fn get(&self, target: &str) -> (u16, String) {
        self.send(&format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"))
    }

    /// `POST target` with a JSON body → (status, body).
    fn post(&self, target: &str, json: &str) -> (u16, String) {
        self.send(&format!(
            "POST {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{json}",
            json.len()
        ))
    }

    fn send(&self, raw: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        // … the old `get` tail: read_to_end, status = text[9..12], body after the blank line
    }
}
```

The test:

```rust
/// RFC 8291's receiver: a valid P-256 key and auth secret to subscribe with.
const PHONE_KEYS: &str = r#""keys":{"p256dh":"BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4","auth":"BTBZMqHH6r4Tts7J_aSIgg"}"#;

#[test]
fn web_push_sends_an_encrypted_message_when_a_session_needs_you() {
    let env = Env::new();
    env.write_fake_profile();
    let config = env.dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let settings = config.join("settings.toml");
    let sink = env.dir.path().join("push.jsonl");
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    let web = env.web_with(|command| {
        command.env("MKREW_PUSH", &sink);
    });
    let key_url = format!("/push/key?t={}", web.token);
    let subscribe_url = format!("/push/subscribe?t={}", web.token);
    let endpoint = "https://fcm.googleapis.com/fcm/send/e2e";
    let subscription = format!(r#"{{"endpoint":"{endpoint}",{PHONE_KEYS}}}"#);

    // `[web] push` defaults to off: no key, no subscribing, so a phone page shows no bell.
    assert_eq!(web.get(&key_url).0, 404);
    assert_eq!(web.post(&subscribe_url, &subscription).0, 404);

    std::fs::write(&settings, "[web]\npush = true\n").unwrap();
    let (status, key) = web.get(&key_url);
    assert_eq!((status, key.len()), (200, 87), "{key}");
    assert_eq!(web.get("/push/key").0, 401, "no token");
    assert_eq!(web.post("/push/subscribe", &subscription).0, 401, "no token");
    // A subscription for any other host is refused: `mk web` never POSTs to it.
    let evil = format!(r#"{{"endpoint":"https://evil.test/x",{PHONE_KEYS}}}"#);
    assert_eq!(web.post(&subscribe_url, &evil).0, 400);
    assert_eq!(web.post(&subscribe_url, "not json").0, 400);
    assert_eq!(web.post(&subscribe_url, &subscription).0, 204);
    // The token-gated manifest the Home Screen app needs, and the service worker.
    let (status, manifest) = web.get(&format!("/manifest.webmanifest?t={}", web.token));
    assert_eq!(status, 200);
    assert!(manifest.contains(&format!("/#t={}", web.token)), "{manifest}");
    assert_eq!(web.get("/manifest.webmanifest").0, 401);
    assert_eq!(web.get("/sw.js").0, 200);

    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let id = env.ok(&["session", "spawn", "demo", "--agent", "fake"]).trim().to_owned();
    // The fake agent asks "(y/n)": the session needs input, and the phone is told.
    env.ok(&["wait", &id, "--until", "needs_input", "--timeout", "30000"]);
    let deadline = Instant::now() + Duration::from_secs(30);
    let line = loop {
        if let Ok(text) = std::fs::read_to_string(&sink)
            && let Some(line) = text.lines().next()
        {
            break line.to_owned();
        }
        assert!(Instant::now() < deadline, "no push was sent");
        std::thread::sleep(Duration::from_millis(100));
    };
    let sent: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(sent["url"], endpoint);
    let header = |name: &str| {
        sent["headers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].as_str().unwrap().to_owned())
    };
    assert!(header("Authorization").unwrap().starts_with("vapid t="));
    assert_eq!(header("Content-Encoding").as_deref(), Some("aes128gcm"));
    // 86-byte header + {"title":…,"body":"needs you","session":…} + delimiter + tag.
    assert!(sent["body_len"].as_u64().unwrap() > 86 + 60 + 17, "{sent}");

    // Unsubscribing works and keeps working with push off.
    let gone = format!(r#"{{"endpoint":"{endpoint}"}}"#);
    assert_eq!(web.post(&format!("/push/unsubscribe?t={}", web.token), &gone).0, 204);
    env.ok(&["send", &id, "y"]);
    env.ok(&["wait", &id, "--until", "exited", "--timeout", "30000"]);
}
```
(If `Env::web_with`'s `command.env("MKREW_PUSH", &sink)` needs the constant, use `mkrew_cli`'s? It is a binary crate: keep the literal string.)

- [ ] **Step 9: Build and run the integration test**

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` then `cargo test -p mkrew-cli --test cli web_ -j 4`
Expected: PASS (`web_serves_the_page_and_relays_a_session`, `web_answers_503…`, the new `web_push_…`). Then `cargo clippy -p mkrew-cli -p mkrew-daemon --all-targets -j 4 -- -D warnings` and `cargo test -p mkrew-cli -j 4`.

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-cli/src/web.rs crates/mkrew-cli/src/web/http.rs crates/mkrew-cli/src/web/relay.rs crates/mkrew-cli/src/web/push.rs crates/mkrew-cli/web/sw.js crates/mkrew-cli/tests/cli.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(web): mk web watches session state and pushes to subscribed phones"
```

---

### Task 6: The page — service worker, bell, tap-to-open

**Files:**
- Modify: `crates/mkrew-cli/web/model.mjs` (new exports at the end)
- Modify: `crates/mkrew-cli/web/model.test.mjs` (import list line 4-8, new tests)
- Modify: `crates/mkrew-cli/web/sw.js` (created in Task 5; final content here)
- Modify: `crates/mkrew-cli/web/app.mjs` (imports line 4-8, `state` ~line 30, `connect` ~line 100, `onConnected` ~line 147, `wire` ~line 682, `start` ~line 789, new `// ---- phone notifications ----` section)
- Modify: `crates/mkrew-cli/web/index.html` (the list header, line 37-39)
- Modify: `crates/mkrew-cli/web/app.css` (after `.icon-btn`, line 87)
- Test: `node --test crates/mkrew-cli/web/model.test.mjs`

**Interfaces:**
- Consumes: the Task 5 endpoints (`/push/key`, `/push/subscribe`, `/push/unsubscribe`, `/manifest.webmanifest`, `/sw.js`).
- Produces (`model.mjs`): `sessionFromHash(hash) → string|null`, `fromBase64Url(text) → Uint8Array`, `sameBytes(a, b) → boolean`, `pushOffer({enabledByPc, secure, supported, standalone, ios, permission, subscribed}) → {show, usable, on, hint}`.

- [ ] **Step 1: Write the failing tests**

In `model.test.mjs` add `fromBase64Url, pushOffer, sameBytes, sessionFromHash` to the import list and append:

```js
test('a tapped notification opens the session named in #s=', () => {
  const id = '01J0000000000000000000ABCD';
  assert.equal(sessionFromHash(`#s=${id}`), id);
  assert.equal(sessionFromHash(`#t=${T}&s=${id}`), id);
  assert.equal(sessionFromHash(`#s=${id}0`), null);
  assert.equal(sessionFromHash('#s=abc'), null);
  assert.equal(sessionFromHash(`#t=${T}`), null);
  assert.equal(sessionFromHash(''), null);
  assert.equal(sessionFromHash(undefined), null);
});

test('the VAPID key decodes from base64url and compares by bytes', () => {
  // The RFC 8291 sender key: 65 bytes, an uncompressed point (starts with 4).
  const key = fromBase64Url('BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8\n');
  assert.equal(key.length, 65);
  assert.equal(key[0], 4);
  assert.ok(sameBytes(key, fromBase64Url('BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8')));
  assert.ok(!sameBytes(key, key.slice(0, 64)));
  assert.ok(!sameBytes(new Uint8Array([1, 2]), new Uint8Array([1, 3])));
});

test('the bell shows only when the PC offers notifications, and says why it cannot work', () => {
  const base = { enabledByPc: true, secure: true, supported: true, standalone: true, ios: true, permission: 'default', subscribed: false };
  assert.deepEqual(pushOffer({ ...base, enabledByPc: false }), { show: false });
  // Plain http on a tailnet IP: no service worker, nothing to offer.
  assert.deepEqual(pushOffer({ ...base, secure: false, supported: false }), { show: false });
  // iPhone in a Safari tab: no PushManager until the page is on the Home Screen.
  const tab = pushOffer({ ...base, supported: false, standalone: false });
  assert.equal(tab.show, true);
  assert.equal(tab.usable, false);
  assert.match(tab.hint, /Home Screen/);
  // A browser without Push that is not an iPhone: nothing to say.
  assert.deepEqual(pushOffer({ ...base, supported: false, ios: false }), { show: false });
  const denied = pushOffer({ ...base, permission: 'denied' });
  assert.equal(denied.usable, false);
  assert.match(denied.hint, /blocked/i);
  const off = pushOffer(base);
  assert.deepEqual([off.show, off.usable, off.on], [true, true, false]);
  const on = pushOffer({ ...base, permission: 'granted', subscribed: true });
  assert.deepEqual([on.show, on.usable, on.on], [true, true, true]);
  assert.match(on.hint, /turn off/i);
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `node --test crates/mkrew-cli/web/model.test.mjs`
Expected: FAIL (`The requested module './model.mjs' does not provide an export named 'fromBase64Url'`).

- [ ] **Step 3: Implement `model.mjs` and `sw.js`**

Append to `model.mjs`:

```js
/** The session id from `#s=<26-char id>`: a tapped notification opens it (ADR 0066). */
export function sessionFromHash(hash) {
  const match = /(?:^#|&)s=([0-9A-Z]{26})(?:&|$)/.exec(hash ?? '');
  return match ? match[1] : null;
}

/** Bytes of a base64url string (the VAPID public key the browser subscribes with). */
export function fromBase64Url(text) {
  const base64 = text.trim().replace(/-/g, '+').replace(/_/g, '/');
  const binary = atob(base64.padEnd(Math.ceil(base64.length / 4) * 4, '='));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

export function sameBytes(a, b) {
  return a.length === b.length && a.every((byte, i) => byte === b[i]);
}

/**
 * What the bell does (ADR 0066). `enabledByPc`: `/push/key` answered (`[web] push`); `secure`:
 * HTTPS or localhost; `supported`: service worker + Push API + Notification exist (an iPhone
 * Safari tab has none of Push); `ios`/`standalone`: iPhone and running from the Home Screen.
 */
export function pushOffer({ enabledByPc, secure, supported, standalone, ios, permission, subscribed }) {
  if (!enabledByPc || !secure) return { show: false };
  if (!supported) {
    if (!ios || standalone) return { show: false };
    return {
      show: true, usable: false, on: false,
      hint: 'To get notifications, add Mkrew to the Home Screen (Share, then Add to Home Screen) and open it from there.',
    };
  }
  if (permission === 'denied') {
    return { show: true, usable: false, on: false, hint: 'Notifications are blocked for Mkrew. Allow them in the phone settings.' };
  }
  return {
    show: true, usable: true, on: subscribed,
    hint: subscribed ? 'Notifications are on. Tap to turn off.' : 'Tap to get a notification when an agent needs you.',
  };
}
```

`crates/mkrew-cli/web/sw.js` (first line is pinned by a Rust test):

```js
// Mkrew web push (ADR 0066). Every push shows a notification, even when the page is open:
// iOS removes a subscription that receives pushes without showing one.
self.addEventListener('push', (event) => {
  let note = {};
  try {
    note = event.data ? event.data.json() : {};
  } catch {
    // Not JSON: show the generic text below.
  }
  event.waitUntil(
    self.registration.showNotification(note.title || 'Mkrew', {
      body: note.body || 'An agent needs you.',
      // A newer notice for the same session replaces the older one.
      tag: note.session || 'mkrew',
      icon: '/apple-touch-icon.png',
      data: { session: note.session || null },
    }),
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const session = event.notification.data?.session;
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
      const open = windows[0];
      if (open) {
        try {
          await open.focus();
        } catch {
          // Some browsers refuse; the message below still reaches the page.
        }
        if (session) open.postMessage({ type: 'open-session', session });
        return;
      }
      await self.clients.openWindow(session ? `/#s=${session}` : '/');
    })(),
  );
});
```

- [ ] **Step 4: Run to verify it passes**

Run: `node --test crates/mkrew-cli/web/model.test.mjs && node --check crates/mkrew-cli/web/sw.js`
Expected: PASS (existing tests + 3 new), `--check` silent.

- [ ] **Step 5: Wire the page** (`index.html`, `app.css`, `app.mjs`)

`index.html` — in the list header, before the New agent button (line 39), add:

```html
    <button class="icon-btn" id="push" type="button" hidden aria-pressed="false" aria-label="Phone notifications"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/></svg></button>
```

`app.css` — after the `.icon-btn{…}` line:

```css
.icon-btn[aria-pressed="true"]{color:var(--accent-text);background:var(--accent-bg)}
```

`app.mjs`:

1. Import list: add `fromBase64Url, pushOffer, sameBytes, sessionFromHash` (keep it alphabetical).
2. `state` gets `wantOpen: null,` and below it a module-level `const push = { enabledByPc: false, subscribed: false };` (after `let toastTimer = 0;`).
3. `connect()`: right after the `if (!res.ok) { retry(); return; }` block (token proven), add `linkManifest();`. Add the function in the new section.
4. `onConnected()`: after `if (state.open && … ) await attach(state.open);` add
```js
    await openWanted();
    initPush();
```
   (`initPush` is not awaited: the bell must never delay the list.)
5. New section before `// ---- rendering ----`:

```js
// ---- phone notifications (ADR 0066) ----

/** The Home Screen app is built from this manifest; it carries the token (see `http.rs`). */
function linkManifest() {
  if (document.querySelector('link[rel="manifest"]')) return;
  const link = document.createElement('link');
  link.rel = 'manifest';
  link.href = `/manifest.webmanifest?t=${state.token}`;
  document.head.append(link);
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
    if (key && pushSupported() && Notification.permission === 'granted') {
      const sub = await currentSubscription();
      if (sub && sameBytes(new Uint8Array(sub.options.applicationServerKey ?? []), key)) {
        // The PC may have forgotten this phone (`mk web --new-token`): tell it again.
        await sendSubscription(sub);
        push.subscribed = true;
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
  await sendSubscription(sub);
  push.subscribed = true;
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
```
6. `wire()`: after the `'new-agent'` listener add `$('push').addEventListener('click', togglePush);` and, inside `wire()` at the end:
```js
  if ('serviceWorker' in navigator) {
    // The service worker found this page already open: it sends the tapped session.
    navigator.serviceWorker.addEventListener('message', (e) => {
      if (e.data?.type === 'open-session' && sessionFromHash(`#s=${e.data.session}`)) openFromNotice(e.data.session);
    });
  }
```
7. `start()`: first statement `state.wantOpen = sessionFromHash(location.hash);` (before `tokenFromHash` and before any `replaceState`, which rewrites the hash to `#t=…`).

- [ ] **Step 6: Check**

Run: `node --check crates/mkrew-cli/web/app.mjs && node --test crates/mkrew-cli/web/model.test.mjs`, then `cargo test -p mkrew-cli web:: -j 4` (the assets are embedded; `serves_the_page_and_its_files` still passes) and the integration test of Task 5 (`/sw.js` 200).
Expected: all PASS. The service worker and bell cannot run headless here: they are covered by the HUMAN_CHECKS entry (Task 8).

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-cli/web/model.mjs crates/mkrew-cli/web/model.test.mjs crates/mkrew-cli/web/sw.js crates/mkrew-cli/web/app.mjs crates/mkrew-cli/web/index.html crates/mkrew-cli/web/app.css
git -c user.email=tungnv975@gmail.com commit -m "feat(web): bell button, service worker and tap-to-open for phone notifications"
```

---

### Task 7: "Phone notifications" row in the Phone access dialog

**Files:**
- Modify: `crates/mkrew-ui/src/phone_dialog.rs` (`Local` ~line 188 and `Local::read` ~195; `Phone` struct ~212 and `Default` ~228; `polled` ~297; new `PushSwitch`, `PushRow`, `push_row` after `serve_row` ~168; tests `local()` ~403 and `set_up_runs_once…` ~675)
- Modify: `crates/mkrew-ui/src/shell/phone_access.rs` (`poll_phone` line 44-80; new `set_phone_push`; `render_phone_access` line 167-354)
- Modify: `crates/mkrew-ui/src/main.rs` (line 81: import `PushSwitch`)
- Test: `phone_dialog.rs` inline tests

**Interfaces:**
- Consumes: `mkrew_core::settings::{load, write_web_push}` (Task 1).
- Produces: `phone_dialog::{PushSwitch, PushRow, push_row(&Daemon, bool) -> Option<PushRow>}`, `Phone.push: bool`, `Local.push: bool`, `Local::read(config_dir: &Path, settings_file: &Path)`, `Shell::set_phone_push(&mut self, bool, &mut Window, &mut Context<Self>)`.

Follow `skill://mkrew-ui-design`: tokens only (`theme.*`, `space::*`, `font_size::*`), both themes; no new token is needed (the row reuses `glyph`, `line`, `button`, `meta_line` of the existing dialog).

- [ ] **Step 1: Write the failing tests** (in `phone_dialog.rs` `mod tests`)

Update the two `Local { … }` literals with `push: false,` (helper `local()` ~line 403 and the one in `set_up_runs_once_and_can_be_pressed_again_after_a_failure` ~675), and add:

```rust
    #[test]
    fn the_notifications_row_exists_only_while_phone_access_is_on() {
        assert_eq!(push_row(&Daemon::Loading, true), None);
        assert_eq!(push_row(&Daemon::Failed("closed".into()), true), None);
        assert_eq!(push_row(&known(false, false, None), true), None, "phone access off");
        let off = push_row(&known(true, true, None), false).unwrap();
        assert_eq!(
            (off.mark, off.text, off.switch),
            (Mark::Off, "Phone notifications: off", PushSwitch::TurnOn)
        );
        // Enabled but `mk web` not up yet (restarting): the row stays so it does not flicker.
        let on = push_row(&known(true, false, Some("address in use")), true).unwrap();
        assert_eq!(
            (on.mark, on.text, on.switch),
            (Mark::On, "Phone notifications: on", PushSwitch::TurnOff)
        );
    }

    #[test]
    fn a_poll_brings_the_push_switch_from_settings_toml() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(!phone.push);
        phone.polled(
            Ok(web(true, true, None)),
            Some(Local { push: true, ..local(Some("pc.ts.net"), None) }),
            t0,
            COLORS,
        );
        assert!(phone.push);
        // The icon's own poll (dialog closed) has no `Local`: the last value stays.
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.push);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui phone_dialog -j 4`
Expected: FAIL to compile (`push` field, `push_row` missing).

- [ ] **Step 3: Implement**

`phone_dialog.rs`:
- `Local`: add `/// \`[web] push\` in settings.toml (ADR 0066).` `pub push: bool,`; `Local::read(config_dir: &Path, settings_file: &Path)` sets `push: mkrew_core::settings::load(settings_file).is_ok_and(|settings| settings.web.push),`.
- `Phone`: add `/// \`[web] push\` as last read; the notifications row shows it.` `pub push: bool,` (default `false`); in `polled`, after `self.token = local.token;` add `self.push = local.push;`.
- After `serve_row`:
```rust
/// The notifications switch of the dialog (ADR 0066).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushSwitch {
    TurnOn,
    TurnOff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRow {
    pub mark: Mark,
    pub text: &'static str,
    pub switch: PushSwitch,
}

/// The Phone notifications row. It shows while phone access is on: the phone page is what asks
/// for notifications, so the switch means nothing before.
pub fn push_row(daemon: &Daemon, push: bool) -> Option<PushRow> {
    let Daemon::Known(web) = daemon else {
        return None;
    };
    if !web.enabled {
        return None;
    }
    Some(if push {
        PushRow {
            mark: Mark::On,
            text: "Phone notifications: on",
            switch: PushSwitch::TurnOff,
        }
    } else {
        PushRow {
            mark: Mark::Off,
            text: "Phone notifications: off",
            switch: PushSwitch::TurnOn,
        }
    })
}
```

`main.rs` line 81: add `PushSwitch` to the `use mkrew_ui::phone_dialog::{…}` list.

`shell/phone_access.rs`:
- `poll_phone`: add `let settings_file = self.settings_file.clone();` next to `let config_dir`, and change the spawn to `Local::read(&config_dir, &settings_file)`.
- Below `set_phone_enabled`:
```rust
    /// "Turn on" / "Turn off" of the Phone notifications row: `[web] push` in `settings.toml`.
    /// `mk web` reads it on every request, so there is nothing to wait for.
    pub(crate) fn set_phone_push(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match mkrew_core::settings::write_web_push(&self.settings_file, enabled) {
            Ok(()) => self.phone.push = enabled,
            Err(error) => {
                tracing::warn!("cannot write [web] push: {error}");
                self.phone.write_failed(error.to_string());
            }
        }
        cx.notify();
        self.poll_phone(window, cx);
    }
```
- `render_phone_access`: after `serve_row` is built add
```rust
        let push_row = phone_dialog::push_row(&phone.daemon, phone.push).map(|row| {
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(glyph(row.mark))
                .child(line(row.text.to_owned()))
                .child(match row.switch {
                    PushSwitch::TurnOn => button("phone-push-on", "Turn on", theme, false)
                        .tooltip(tooltip(
                            theme,
                            "Notify the phone when an agent needs you or finishes. The message is encrypted for the phone and goes through Apple's, Google's or Mozilla's push service",
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_push(true, window, cx);
                        })),
                    PushSwitch::TurnOff => button("phone-push-off", "Turn off", theme, false)
                        .tooltip(tooltip(theme, "Stop notifying the phone", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_push(false, window, cx);
                        })),
                })
        });
        let push_hint = (phone.push && push_row.is_some()).then(|| {
            meta_line(
                theme,
                "On the phone, open Mkrew from the Home Screen and tap the bell.".into(),
            )
        });
```
  and in the panel chain, after `.children(serve.error.map(error_row))` add `.children(push_row)` and `.children(push_hint)`.
  (If `PushSwitch` is not visible through `use crate::*;`, it is: step 3 added it to the `main.rs` import list that `crate::*` re-exports, like `SetUp`.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui phone -j 4` then `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: PASS, no warnings. (The dialog rendering itself is checked by eye in HUMAN_CHECKS, Task 8.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/phone_dialog.rs crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Phone notifications row in the Phone access dialog"
```

---

### Task 8: README, HUMAN_CHECKS, PROGRESS, full verify

**Files:**
- Modify: `README.md` ("Use Mkrew from your phone", ends before `## Uninstall`)
- Modify: `docs/HUMAN_CHECKS.md` (new section at the top of "To verify (non-blocking)", above `### To verify — 2026-10-15 Ask AI dialog`)
- Modify: `docs/PROGRESS.md` ("Notes for the next session": one line at the end of the list)

- [ ] **Step 1: README** — before `## Uninstall` add:

```markdown
### Notifications on the phone

Want a notification when an agent needs you while you are away from the desk?

1. In the Phone access dialog, next to **Phone notifications**, click **Turn on**. With this on, this PC
   sends a short, encrypted message to your phone's push service (Apple on iPhone, Google on Android
   Chrome) when an agent needs you or finishes. Only your phone can read it.
2. On the iPhone (iOS 16.4 or newer): remove the old Mkrew icon from the Home Screen if it is there, open
   the link in Safari, then Share → Add to Home Screen, and open Mkrew **from that icon**.
3. Tap the bell at the top of the Sessions list and allow notifications.

The same `[notify]` switches and the 30-second limit as the desktop notification apply.
`mk web --new-token` also forgets all phones.
```

- [ ] **Step 2: HUMAN_CHECKS** — add the section (date 2026-10-19):

```markdown
### To verify — 2026-10-19 Phone push notifications (ADR 0066, plan docs/superpowers/plans/2026-10-19-phone-push-notifications.md)

- [ ] **Install first:** the daemon starts `mk.exe` from the install folder; install the new build, then
  Phone access → **Stop** → **Start** (no daemon restart needed: no protocol change).
- [ ] **Prerequisites (same as the phone page):** Tailscale MagicDNS + HTTPS Certificates on, **Set up** done.
- [ ] **Dialog (dark + light):** with phone access On, a row "Phone notifications: off" + **Turn on** shows under
  the Tailscale row; Turn on → "Phone notifications: on" + **Turn off** and the hint line "On the phone, open
  Mkrew from the Home Screen and tap the bell."; Stop phone access → the row disappears. No clipped text at 520 px.
- [ ] **iPhone, first time [the open risk of the plan]:** remove the old Home Screen icon, open the link in Safari,
  wait until the Sessions list shows, Share → Add to Home Screen, open the new icon: it opens **without** asking for
  the token (the manifest's `start_url` carries it). The bell shows at the top; tap it → iOS asks to allow
  notifications → Allow → the bell turns accent and a toast says you will get a notification.
  *If the bell is missing in the Home Screen app:* open `https://<pc>/manifest.webmanifest?t=<token>` in Safari and
  check it shows JSON; if the app still has no bell, iOS did not take the manifest — report it.
- [ ] **Needs you:** in Mkrew start an agent that asks something (or `mk session spawn <project> --agent fake`), lock the
  phone → within ~5 s a notification "fake · <workspace>" / "needs you" appears on the Lock Screen. Tap it → the Mkrew
  app opens on that session (do it from a closed app, from the list, and from another open session).
- [ ] **Done:** let an agent finish → "<name> · <workspace>" / "done".
- [ ] **Limits:** two quick `needs_input` / `done` changes of one session → one notification (30 s). Set `[notify]
  needs_input = false` in `settings.toml` → no "needs you" notification, "done" still comes.
- [ ] **Off switches:** Turn off in the dialog → no notification; reload the phone page → the bell is gone. Turn on
  again → the bell is back and notifications resume without tapping it (the page re-registers itself).
- [ ] **Bell off:** tap the bell on the phone (on → off) → no more notifications; `%APPDATA%\mkrew\web_push.json`
  lost that phone's entry.
- [ ] **Revoke:** with phone access Stopped, run `mk web --new-token`, Ctrl+C, Start → `web_push.json` has an empty
  `subscriptions` list and the same `vapid`; the phone shows the token screen; paste the new link → it works and
  notifications resume.
- [ ] **Apple accepts the sender:** if no notification arrives on iPhone but Chrome works, run `mk web` by hand and read
  stderr: `mk web: push to https://web.push.apple.com: HTTP 403` means Apple rejected the VAPID contact
  (`SUBJECT` in `crates/mkrew-cli/src/web/push/crypto.rs`); report it.
- [ ] **Offline phone:** airplane mode, trigger a notification, leave airplane mode within an hour → it arrives.
- [ ] **Daemon restart:** with notifications on, `mk daemon stop` / start (or reboot) → a new `needs_input` still notifies
  (the push loop reconnects within ~2 s).
- [ ] **Privacy:** `web_push.json` is not readable by other users (Unix); no endpoint URL appears in any log line.
```

- [ ] **Step 3: PROGRESS** — append to "Notes for the next session":

```markdown
- 2026-10-19 phone push notifications (plan docs/superpowers/plans/2026-10-19-phone-push-notifications.md, ADR 0066): `[web] push` (`mkrew_core::settings::write_web_push`, Phone access dialog row), `mk web` watches `session.state` on its own daemon connection (`web/push.rs`, `mkrew_daemon::decide` shared with the desktop toast) and sends RFC 8291 + VAPID Web Push (`ring`, `ureq`; `web/push/crypto.rs`, `store.rs`; subscriptions in `<config>/web_push.json`, allowlisted hosts, 404/410 drop a phone, `--new-token` empties the list); page: `sw.js`, bell, dynamic token-gated manifest. No protocol change, no migration. Tests never send: `MKREW_PUSH=<file>` sink. Manual checks in HUMAN_CHECKS "2026-10-19 Phone push notifications" (iPhone Home Screen + Apple's VAPID acceptance are unverified until then). Outside the milestone loop.
```

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` then `node --test crates/mkrew-cli/web/model.test.mjs`
Expected: fmt, clippy `-D warnings` and all tests green. Run `cargo fmt --all` first if the format check fails (then re-stage only your files).

- [ ] **Step 5: Commit**

```bash
git add README.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(web): phone notifications README, human checks, progress note"
```

---

## Self-review

**Spec coverage** (feature brief → task):
- Research recorded (HTTP vs HTTPS, secure context, iOS 16.4 Home Screen, crates, `mk web` vs daemon, SPEC conflict, alternatives): "Feasibility verdict" + "Research findings".
- Web Push with VAPID + RFC 8291 + HTTPS POST: Tasks 2 (crypto), 4 (`UreqSender`, `deliver`).
- ADR 0066 + SPEC §1 non-goals, §8, §9, §15, §20 intro, §20.0 settings + Phone access paragraph: Task 1.
- Runs in `mk web`, receives session state events (verified `server.rs:203-206`): Task 5 (`Daemon::next_event`, `run`).
- Respect `[notify]` switches and 30 s limit: Task 4 (`wanted` → `mkrew_daemon::decide`), tests `wanted_*`, `the_notify_switches_apply_to_phones_too`.
- Opt-in switch `[web] push = false` + per-phone bell: Tasks 1, 6, 7.
- Subscriptions persisted (file, no migration 0022), removed on 404/410, `--new-token` drops them: Tasks 3, 4, 5.
- Tests: RFC 8291 vector (Task 2), VAPID JWT shape + RFC 8292 example token (Task 2), subscription store (Task 3), decision logic (Task 4), no network (injected `Sender`, `MKREW_PUSH` sink, Task 5 e2e), JS `node --test` (Task 6), service worker manual check in HUMAN_CHECKS (Task 8).
- HTTPS design with Tailscale: findings table (already served by `tailscale serve`; user must have MagicDNS + HTTPS certs — README step reused, no `tailscale cert`).
- Feasibility verdict at the top with blockers/human actions: yes.

**Placeholder scan:** no "TBD"/"similar to". Task 5's code blocks say `// … unchanged` where existing code (`run`'s runtime build, the `println!` block, the accept loop, the tail of `web()` and of `get`) stays as it is; the executor edits around those lines. SPEC edits are given as exact replacement text with quoted anchors.

**Type consistency:** `Push::open(file, settings_file, renew, sender)` (Task 4) = call in `serve` (Task 5). `Reply::{Done, Off, Bad}` used by `answer()` (Task 5). `Post`/`Sent`/`Sender` consistent in tests and `UreqSender`/`FileSender`. `Store::open(file, drop_subscriptions)` (Task 3) = `Push::open` call. `Subscription::parse`, `keys`, `endpoint_in` consistent. `crypto::{encrypt, seal, Vapid, origin, MAX_PLAINTEXT}` consistent (`origin` is used from `push.rs` as `crypto::origin`; `MAX_PLAINTEXT` in the payload test). `WebSettings { enabled, push }` used in Task 4 tests. `Local.push` / `Local::read(config_dir, settings_file)` consistent between Task 7 model and shell. JS: `pushOffer` argument names (`enabledByPc, secure, supported, standalone, ios, permission, subscribed`) match `pushInfo()`.

**Review Focus coverage:** 1 → Tasks 3, 5; 2 → Task 4; 3 → Task 4 (+ e2e 404 before settings written); 4 → Task 3; 5 → Tasks 2, 4; 6 → Task 6 tests for the pure parts, HUMAN_CHECKS for the worker.

**Known unverified (inference, not blockers):** iOS honoring the dynamic manifest for push; Apple accepting `mailto:mkrew@example.com`; push host names for FCM / Mozilla / Windows; `ureq` default proxy behaviour on the user's network. Each has a HUMAN_CHECKS line.
