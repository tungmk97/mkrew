# 0067 — Push notifications to the phone (Web Push from `mk web`)

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
  other host is refused (a token holder must not be able to make the PC call arbitrary URLs). Every
  send checks the host again (the stored file may be edited by hand), and the HTTP client follows no
  redirects, so a push service cannot send `mk web` to another host.
- **Where it runs:** `mk web` opens its own daemon connection and reads `session.state` events. The
  daemon gets no network code and no protocol change. The §8 policy is shared: `[notify] needs_input` /
  `done` and the 30 s per-session limit apply (`mkrew_daemon::decide`, `Throttle`).
- **Keys and state:** `<config>/web_push.json` (written atomically; owner-only on Unix) holds the
  VAPID key (P-256, PKCS#8) and the subscriptions (at most 8). With `push` off the key and the file
  are not created. A store or key failure is logged and `mk web` keeps serving the page.
  `mk web --new-token` empties the subscriptions and keeps the key. A push service answer 404 / 410
  removes that subscription.
- **The token and the payload:** the access token never travels through a push service. No
  notification payload carries it; the payload holds only the title, the body and the session id.
  The token is in the Web App Manifest's `start_url` (served at `/manifest.webmanifest`, gated by the
  token), because the iOS Home Screen app has its own storage and must open the page with the token.
  iOS stores that `start_url` with the Home Screen entry. Tapping a notification focuses the open
  page or opens `/#s=<session>`; the page takes the token from its own storage (saved from
  `start_url` on first launch), so the notification itself carries no token.
- **Crypto and HTTP:** `ring` (P-256 ECDH and ECDSA, HKDF, AES-128-GCM) and `ureq` (HTTPS POST),
  both already in the dependency tree for rustls / speech to text; no new package.
- **Page:** a service worker (`/sw.js`), the token-gated manifest above and a bell button.
  iOS needs iOS 16.4+ and the page added to the Home Screen.
- Tests never use the network: the sender is injected; `MKREW_PUSH=<file>` makes `mk web` append
  each message to a JSON-lines file instead of sending it (like `MKREW_NOTIFY`).

## Consequences

- SPEC §1, §8, §9, §15, the §20 intro and §20.0 change. No protocol version change, no migration.
- With `push = true` the PC makes outbound HTTPS requests to the four push services. With the
  default `false`, behaviour is exactly as before.
- Push is best effort (TTL 1 h): a phone offline longer than that misses the notice.
- iOS revokes subscriptions that receive pushes without showing a notification, so the service worker
  always shows one.
- Anyone who sees the Home Screen entry's `start_url` sees the token; that is the same trust as the
  link the user already shares (ADR 0052). `mk web --new-token` revokes it.
- Not done: several PCs on one phone page, notification actions (Approve / Deny buttons), badge count.
- Restart the daemon after installing: `WebSettings` rejects unknown keys, so once the dialog writes
  `[web] push = true`, an older daemon rejects the whole `settings.toml` and keeps its last good
  settings; Stop/Start and every later setting are ignored until it restarts (same trap as
  `flash_taskbar`, ADR 0064).
