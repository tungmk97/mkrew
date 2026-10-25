# Toast click through a `mkrew://` link — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On Windows a click on a Mkrew toast — banner or Notification Center, any time — shows the Mkrew window on that session's pane, starting the UI if it is closed.

**Architecture:** The daemon shows WinRT toasts with `activationType="protocol"` and `launch="mkrew://session/<id>?project=<id>"`, and registers `mkrew://` under HKCU for the `mkrew.exe` beside it. `mkrew.exe` started with such a link forwards it to the running UI (`WM_COPYDATA` to the tray window, after `AllowSetForegroundWindow`) or, when none runs, opens the pane after its first session list.

**Tech Stack:** Rust, `windows` 0.62 (WinRT `UI_Notifications`, `Data_Xml_Dom`; Win32 `DataExchange`), `windows-sys` 0.61 (`Win32_System_Registry`), GPUI shell, `ulid`.

**Spec:** `docs/superpowers/specs/2026-10-25-toast-click-protocol-design.md`

## Global Constraints

- Link shape, exactly: `mkrew://session/<session ULID>?project=<project ULID>`; scheme case-insensitive, one optional `/` after the session id.
- Toast AUMID stays PowerShell's: `{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe`.
- Registry key: `HKCU\Software\Classes\mkrew`; `(default)` = `URL:Mkrew`, `URL Protocol` = `""`, `shell\open\command\(default)` = `"<dir>\mkrew.exe" "%1"`.
- The daemon registers only when `MKREW_NOTIFY` is unset, `MKREW_SOCKET` is unset and `mkrew.exe` is a file next to `mkrew-daemon.exe`; written on every such start; failure = `tracing::warn!`.
- Linux and macOS behaviour, the `[notify]` policy, the 30 s throttle, phone push, the installer: unchanged.
- No new package in `Cargo.lock`: `windows` 0.62 and `windows-sys` 0.61 are already in the tree.
- `unsafe_code = "deny"` workspace-wide: every unsafe block sits in an item with `#[allow(unsafe_code)]` and a `// SAFETY:` comment.
- No `unwrap/expect` outside tests unless provably infallible (comment why).
- Tests never show a real toast, never touch `HKCU\Software\Classes\mkrew`, never call the network.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, stage only the task's files.
- ADR number: 0078 (re-check `docs/decisions/` before writing it; take the next free number if 0078 is gone and update every reference in this plan's files).

## Review Focus

1. A session or workspace name with `&`, `<`, `>`, `"` or `'` → the toast still shows (XML escaped). Pinned in Task 2 (`toast_xml` escaping + real `XmlDocument::LoadXml` parse).
2. A link Windows passes with an upper-case scheme, a trailing `/`, surrounding spaces, or a crafted link with extra segments / bad ids → valid ones open the pane, the rest start or raise Mkrew as a plain launch. Pinned in Task 1.
3. A test or dev daemon (own `MKREW_SOCKET`) or a daemon without `mkrew.exe` beside it → never writes the handler. Pinned in Task 3 (`link_target`).
4. An install path with spaces → the command line quotes the exe and `%1`. Pinned in Task 3 (`open_command`).
5. The running UI is hung or its tray window is gone → the link launch must not hang (2 s `SendMessageTimeoutW`, `SMTO_ABORTIFHUNG`) and a garbled `WM_COPYDATA` is ignored. Pinned in Task 4 (`decode_jump` + tray round trip).

---

### Task 1: `mkrew_core::link`

**Files:**
- Create: `crates/mkrew-core/src/link.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (add `pub mod link;` between `pub mod jira;` and `pub mod log_tail;`)

**Interfaces:**
- Produces: `pub const SCHEME: &str = "mkrew";`, `pub fn session_link(session: Ulid, project: Ulid) -> String`, `pub fn parse_session_link(link: &str) -> Option<(Ulid, Ulid)>` (returns `(session, project)`).

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-core/src/link.rs` with the module doc, the `use`, bodies `todo!()`, and the tests:

```rust
//! `mkrew://` links (ADR 0078). A Windows toast opens `mkrew://session/<session>?project=<project>`;
//! `mkrew.exe` started with it shows that session's pane (in the running UI, or a new one).

use ulid::Ulid;

/// The URL scheme registered for Mkrew on Windows.
pub const SCHEME: &str = "mkrew";

/// The link a toast of `session` (in workspace `project`) opens.
pub fn session_link(session: Ulid, project: Ulid) -> String {
    todo!()
}

/// `(session, project)` of a [`session_link`]; `None` for anything else. The scheme is
/// case-insensitive and one `/` may follow the session id (Windows may pass either);
/// surrounding whitespace is ignored.
pub fn parse_session_link(link: &str) -> Option<(Ulid, Ulid)> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_link_round_trips() {
        let (session, project) = (Ulid::new(), Ulid::new());
        let link = session_link(session, project);
        assert_eq!(link, format!("mkrew://session/{session}?project={project}"));
        assert_eq!(parse_session_link(&link), Some((session, project)));
    }

    #[test]
    fn the_forms_windows_may_pass_are_accepted() {
        let (session, project) = (Ulid::new(), Ulid::new());
        for link in [
            format!("MKREW://session/{session}?project={project}"),
            format!("mkrew://session/{session}/?project={project}"),
            format!("  mkrew://session/{session}?project={project} "),
        ] {
            assert_eq!(parse_session_link(&link), Some((session, project)), "{link}");
        }
    }

    #[test]
    fn anything_else_is_rejected() {
        let (session, project) = (Ulid::new(), Ulid::new());
        for link in [
            String::new(),
            "mkrew://".into(),
            format!("http://session/{session}?project={project}"),
            format!("mkrew://task/{session}?project={project}"),
            format!("mkrew://session/{session}"),
            format!("mkrew://session/{session}?workspace={project}"),
            format!("mkrew://session/{session}/x?project={project}"),
            format!("mkrew://session/{session}?project={project}&x=1"),
            format!("mkrew://session/not-a-ulid?project={project}"),
            format!("mkrew://session/{session}?project=zz"),
        ] {
            assert_eq!(parse_session_link(&link), None, "{link}");
        }
    }
}
```

- [ ] **Step 2: Run the tests, expect a panic** — Run: `cargo test -p mkrew-core link::` · Expected: 3 tests FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

```rust
pub fn session_link(session: Ulid, project: Ulid) -> String {
    format!("{SCHEME}://session/{session}?project={project}")
}

pub fn parse_session_link(link: &str) -> Option<(Ulid, Ulid)> {
    let (scheme, rest) = link.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let (session, query) = rest.strip_prefix("session/")?.split_once('?')?;
    let session = session.strip_suffix('/').unwrap_or(session);
    let project = query.strip_prefix("project=")?;
    Some((Ulid::from_string(session).ok()?, Ulid::from_string(project).ok()?))
}
```

- [ ] **Step 4: Run the tests** — Run: `cargo test -p mkrew-core link::` · Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/link.rs crates/mkrew-core/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): mkrew:// session links"
```

---

### Task 2: Windows toast with a protocol launch

**Files:**
- Create: `crates/mkrew-daemon/src/wintoast.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (after `mod notify;` add `#[cfg(windows)]` + `mod wintoast;`)
- Modify: `crates/mkrew-daemon/src/notify.rs` (module doc lines 1-4, `OsNotifier` lines 172-215, delete the Windows `set_sound` lines 235-241)
- Modify: `crates/mkrew-daemon/Cargo.toml`

**Interfaces:**
- Consumes: `mkrew_core::link::session_link(session: Ulid, project: Ulid) -> String` (Task 1); `crate::notify::Notice { session_id, project_id, title, body, sound }` (existing, all fields `pub`).
- Produces: `pub(crate) fn toast_xml(notice: &Notice) -> String`, `pub(crate) fn show(notice: &Notice) -> windows::core::Result<()>` in `crate::wintoast` (Windows only).

- [ ] **Step 1: Dependencies** — in `crates/mkrew-daemon/Cargo.toml`:
  - Move the `notify-rust` line and its comment out of `[dependencies]` into a new table placed after `[target.'cfg(unix)'.dependencies]`:

```toml
[target.'cfg(not(windows))'.dependencies]
# OS notifications on needs_input/done (SPEC §8); default `z` = pure-Rust zbus on Linux.
# Windows shows WinRT toasts itself (ADR 0078).
notify-rust = "4.18.1"
```

  - In `[target.'cfg(windows)'.dependencies]` add:

```toml
# WinRT toasts whose click opens a `mkrew://` link (ADR 0078); same 0.62 and features
# tauri-winrt-notification already builds, so the tree gains no package.
windows = { version = "0.62", features = ["Data_Xml_Dom", "Foundation", "UI_Notifications"] }
```

- [ ] **Step 2: Write the failing tests** — create `crates/mkrew-daemon/src/wintoast.rs`:

```rust
//! Windows toasts (ADR 0078): shown through WinRT with `activationType="protocol"`, so a click
//! on the banner, or later in the Notification Center, opens the session's `mkrew://` link
//! even after this process forgot the toast.

use mkrew_core::link::session_link;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::core::HSTRING;

use crate::notify::Notice;

/// PowerShell's AUMID: an unpackaged app may show toasts under it without a Start Menu
/// shortcut of its own (the toast header reads "Windows PowerShell").
const POWERSHELL_APP_ID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

/// The toast for `notice`: title, body, the OS default sound or silence, and the link a click
/// opens.
pub(crate) fn toast_xml(notice: &Notice) -> String {
    todo!()
}

/// Shows `notice`; returns once Windows has it (nothing waits for a click).
pub(crate) fn show(notice: &Notice) -> windows::core::Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use ulid::Ulid;

    use super::*;

    fn notice(title: &str, sound: bool) -> Notice {
        Notice {
            session_id: Ulid::new(),
            project_id: Ulid::new(),
            title: title.into(),
            body: "Needs input".into(),
            sound,
        }
    }

    #[test]
    fn a_click_opens_the_session_link() {
        let notice = notice("repo · claude", true);
        let xml = toast_xml(&notice);
        let launch = format!(
            r#"activationType="protocol" launch="{}""#,
            session_link(notice.session_id, notice.project_id)
        );
        assert!(xml.contains(&launch), "{xml}");
        assert!(xml.contains("<text>repo · claude</text><text>Needs input</text>"), "{xml}");
    }

    #[test]
    fn sound_follows_the_notice() {
        assert!(toast_xml(&notice("a", true))
            .contains(r#"<audio src="ms-winsoundevent:Notification.Default"/>"#));
        assert!(toast_xml(&notice("a", false)).contains(r#"<audio silent="true"/>"#));
    }

    /// A name may hold any character; Windows must still parse the toast.
    #[test]
    fn markup_in_names_is_escaped_and_parses() {
        let xml = toast_xml(&notice(r#"a & <b> "c" 'd'"#, false));
        assert!(
            xml.contains("<text>a &amp; &lt;b&gt; &quot;c&quot; &apos;d&apos;</text>"),
            "{xml}"
        );
        let doc = XmlDocument::new().unwrap();
        doc.LoadXml(&HSTRING::from(xml)).unwrap();
    }
}
```

- [ ] **Step 3: Run the tests, expect a panic** — Run: `cargo test -p mkrew-daemon wintoast::` · Expected: 3 tests FAIL with `not yet implemented`.

- [ ] **Step 4: Implement `toast_xml` and `show`**

```rust
pub(crate) fn toast_xml(notice: &Notice) -> String {
    let audio = if notice.sound {
        r#"<audio src="ms-winsoundevent:Notification.Default"/>"#
    } else {
        r#"<audio silent="true"/>"#
    };
    format!(
        r#"<toast activationType="protocol" launch="{}"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>"#,
        escape(&session_link(notice.session_id, notice.project_id)),
        escape(&notice.title),
        escape(&notice.body),
    )
}

pub(crate) fn show(notice: &Notice) -> windows::core::Result<()> {
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(toast_xml(notice)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(POWERSHELL_APP_ID))?
        .Show(&toast)
}

/// XML text / attribute escaping.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}
```

  Check the exact 0.62 method names (`LoadXml`, `CreateToastNotification`, `CreateToastNotifierWithId`, `Show`) on docs.rs (`windows` 0.62.2, `windows::UI::Notifications`) before compiling; do not guess.

- [ ] **Step 5: Run the tests** — Run: `cargo test -p mkrew-daemon wintoast::` · Expected: 3 passed.

- [ ] **Step 6: Switch `OsNotifier`** — in `notify.rs`:
  - Module doc lines 3-4 become:
    ```rust
    //! [`THROTTLE`], with the OS default sound when `notify.sound`. A click shows the session: on
    //! Windows the toast opens its `mkrew://` link (ADR 0078), elsewhere it becomes a
    //! `notification.clicked` event.
    ```
  - Replace the `OsNotifier` doc + `impl` (lines 172-215) with:

```rust
/// The OS notifier. Windows: a WinRT toast whose click opens the session's `mkrew://` link
/// (ADR 0078), so `clicked` is not used there. Elsewhere `notify-rust`: `show` and waiting for
/// the click both block, so each notice gets its own short-lived thread (it ends on click or
/// dismissal; the throttle bounds how many exist).
struct OsNotifier;

#[cfg(windows)]
impl Notifier for OsNotifier {
    fn show(&self, notice: Notice, _clicked: Clicked) {
        // WinRT calls block and the notify task is async; the thread ends right after `Show`.
        let spawned = std::thread::Builder::new()
            .name("notification".into())
            .spawn(move || {
                if let Err(e) = crate::wintoast::show(&notice) {
                    tracing::warn!("showing a notification failed: {e}");
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("starting a notification thread failed: {e}");
        }
    }
}

#[cfg(not(windows))]
impl Notifier for OsNotifier {
    fn show(&self, notice: Notice, clicked: Clicked) {
        let spawned = std::thread::Builder::new()
            .name("notification".into())
            .spawn(move || {
                let mut notification = notify_rust::Notification::new();
                notification
                    .appname("Mkrew")
                    .summary(&notice.title)
                    .body(&notice.body);
                set_sound(&mut notification, notice.sound);
                // Linux servers only report a body click for a "default" action.
                #[cfg(not(target_os = "macos"))]
                notification.action("default", "Open");
                let handle = match notification.show() {
                    Ok(handle) => handle,
                    Err(e) => return tracing::warn!("showing a notification failed: {e}"),
                };
                // macOS only sends from `wait_*` with a running main run loop, which the
                // daemon lacks: dropping the handle sends it, clicks are not reported.
                #[cfg(not(target_os = "macos"))]
                {
                    use notify_rust::NotificationResponse;
                    let _ = handle.wait_for_response(|response: &NotificationResponse| {
                        if matches!(
                            response,
                            NotificationResponse::Default | NotificationResponse::Action(_)
                        ) {
                            let _ = clicked.send(notice.clone());
                        }
                    });
                }
                #[cfg(target_os = "macos")]
                drop((handle, clicked));
            });
        if let Err(e) = spawned {
            tracing::warn!("starting a notification thread failed: {e}");
        }
    }
}
```

  - Delete the `#[cfg(target_os = "windows")] fn set_sound` (doc line + function). The macOS and Linux `set_sound` stay.

- [ ] **Step 7: Build and test the crate** — Run: `cargo test -p mkrew-daemon notify` then `cargo clippy -p mkrew-daemon --all-targets -- -D warnings` · Expected: all pass, no warnings (fix an unused import on Windows by narrowing its `cfg`, not with `allow`).

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-daemon/Cargo.toml crates/mkrew-daemon/src/wintoast.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/notify.rs Cargo.lock
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): Windows toasts open a mkrew:// link on click"
```

  `Cargo.lock` should only gain `windows` in mkrew-daemon's dependency list; if `git diff Cargo.lock` shows anything else, do not stage it (it is the user's edit).

---

### Task 3: Register `mkrew://` at daemon start

**Files:**
- Modify: `crates/mkrew-daemon/src/wintoast.rs`
- Modify: `crates/mkrew-daemon/src/notify.rs` (`from_env`, lines 49-60)
- Modify: `crates/mkrew-daemon/Cargo.toml` (`windows-sys` features + its comment)

**Interfaces:**
- Consumes: `mkrew_core::paths::ENV_SOCKET` (existing const `"MKREW_SOCKET"`).
- Produces: `pub(crate) fn register_link_handler()`; private `link_target(daemon_exe: &Path, socket_override: bool) -> Option<PathBuf>`, `open_command(ui: &Path) -> String`, `register(key: &str, ui: &Path) -> std::io::Result<()>`.

- [ ] **Step 1: Feature** — `windows-sys` features gain `"Win32_System_Registry"`; append to its comment: `` registry writes register `mkrew://` (ADR 0078). ``

- [ ] **Step 2: Write the failing tests** — add to `wintoast.rs` (above `#[cfg(test)]`) these items with `todo!()` bodies:

```rust
use std::io;
use std::path::{Path, PathBuf};

/// Where `mkrew://` is registered for the user.
const CLASSES_KEY: &str = r"Software\Classes\mkrew";

/// At daemon start (OS notifier on): points `mkrew://` at the UI next to this daemon.
pub(crate) fn register_link_handler() {
    todo!()
}

/// `mkrew.exe` next to `daemon_exe` when this daemon owns the link: the user's own daemon on
/// the default pipe (tests and dev daemons on their own `MKREW_SOCKET` never take it), with
/// the UI beside it.
fn link_target(daemon_exe: &Path, socket_override: bool) -> Option<PathBuf> {
    todo!()
}

/// The handler command line: both parts quoted (install paths may hold spaces).
fn open_command(ui: &Path) -> String {
    todo!()
}

/// Writes the `mkrew://` handler under `HKCU\<key>`.
fn register(key: &str, ui: &Path) -> io::Result<()> {
    todo!()
}
```

  and to its `mod tests`:

```rust
    #[test]
    fn only_the_default_pipe_daemon_with_the_ui_beside_it_owns_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = dir.path().join("mkrew-daemon.exe");
        assert_eq!(link_target(&daemon, false), None, "no mkrew.exe yet");
        let ui = dir.path().join("mkrew.exe");
        std::fs::write(&ui, b"").unwrap();
        assert_eq!(link_target(&daemon, false), Some(ui));
        assert_eq!(link_target(&daemon, true), None, "own MKREW_SOCKET");
    }

    #[test]
    fn the_command_quotes_the_exe_and_the_link() {
        assert_eq!(
            open_command(Path::new(r"C:\Program Files\Mkrew\bin\mkrew.exe")),
            r#""C:\Program Files\Mkrew\bin\mkrew.exe" "%1""#
        );
    }

    /// Writes only under a throwaway `HKCU\Software\MkrewTest-<pid>`, deleted before asserting.
    #[test]
    fn the_handler_is_written_under_the_key() {
        let key = format!(r"Software\MkrewTest-{}", std::process::id());
        let ui = Path::new(r"C:\Program Files\Mkrew\bin\mkrew.exe");
        let written = register(&key, ui);
        let read = (
            get(&key, None),
            get(&key, Some("URL Protocol")),
            get(&format!(r"{key}\shell\open\command"), None),
        );
        delete_tree(&key);
        written.unwrap();
        assert_eq!(
            read,
            (
                Some("URL:Mkrew".into()),
                Some(String::new()),
                Some(open_command(ui))
            )
        );
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[allow(unsafe_code)]
    fn get(key: &str, name: Option<&str>) -> Option<String> {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
        let (key, name) = (wide(key), name.map(wide));
        let mut buf = [0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: NUL-terminated buffers alive for the call; `len` is `buf`'s size in bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        (status == 0)
            .then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
    }

    #[allow(unsafe_code)]
    fn delete_tree(key: &str) {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteTreeW};
        let key = wide(key);
        // SAFETY: NUL-terminated buffer alive for the call.
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
    }
```

  (add `use std::path::Path;` to the test module if `super::*` does not bring it.)

- [ ] **Step 3: Run the tests, expect a panic** — Run: `cargo test -p mkrew-daemon wintoast::` · Expected: the 3 new tests FAIL with `not yet implemented`; Task 2's 3 pass.

- [ ] **Step 4: Implement**

```rust
pub(crate) fn register_link_handler() {
    let socket_override = std::env::var_os(mkrew_core::paths::ENV_SOCKET).is_some();
    let Some(ui) = std::env::current_exe()
        .ok()
        .and_then(|exe| link_target(&exe, socket_override))
    else {
        return;
    };
    match register(CLASSES_KEY, &ui) {
        Ok(()) => tracing::info!(ui = %ui.display(), "mkrew:// links open the UI"),
        Err(e) => tracing::warn!("registering mkrew:// failed, toast clicks do nothing: {e}"),
    }
}

fn link_target(daemon_exe: &Path, socket_override: bool) -> Option<PathBuf> {
    if socket_override {
        return None;
    }
    let ui = daemon_exe.parent()?.join("mkrew.exe");
    ui.is_file().then_some(ui)
}

fn open_command(ui: &Path) -> String {
    format!("\"{}\" \"%1\"", ui.display())
}

fn register(key: &str, ui: &Path) -> io::Result<()> {
    set(key, None, "URL:Mkrew")?;
    set(key, Some("URL Protocol"), "")?;
    set(&format!(r"{key}\shell\open\command"), None, &open_command(ui))
}

/// One `REG_SZ` value under `HKCU\<key>` (created if missing); `None` = the key's default.
#[allow(unsafe_code)]
fn set(key: &str, name: Option<&str>, value: &str) -> io::Result<()> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let (key, name, value) = (wide(key), name.map(wide), wide(value));
    let bytes = u32::try_from(value.len() * 2).map_err(io::Error::other)?;
    // SAFETY: every pointer is a NUL-terminated UTF-16 buffer alive for the call; `bytes` is
    // `value`'s size including its NUL.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
            REG_SZ,
            value.as_ptr().cast(),
            bytes,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}
```

  Check `RegSetKeyValueW` / `RegGetValueW` / `RegDeleteTreeW` signatures in `windows-sys` 0.61 (`Win32::System::Registry`) on docs.rs; adjust casts only.

- [ ] **Step 5: Call it** — in `notify.rs` `from_env`, the `None` arm becomes:

```rust
        None => {
            #[cfg(windows)]
            {
                // Before any WinRT call, see `keep_mta_alive`.
                keep_mta_alive();
                crate::wintoast::register_link_handler();
            }
            Some(Arc::new(OsNotifier))
        }
```

- [ ] **Step 6: Run the tests** — Run: `cargo test -p mkrew-daemon wintoast::` then `cargo clippy -p mkrew-daemon --all-targets -- -D warnings` · Expected: 6 passed, no warnings. Then `reg query "HKCU\Software\Classes\mkrew"` · Expected: `ERROR: The system was unable to find the specified registry key` (tests never write it).

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-daemon/Cargo.toml crates/mkrew-daemon/src/wintoast.rs crates/mkrew-daemon/src/notify.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): register mkrew:// for the UI beside the daemon"
```

---

### Task 4: A link launch reaches the running UI

**Files:**
- Modify: `crates/mkrew-ui/Cargo.toml` (`windows` features: add `"Win32_System_DataExchange"`)
- Modify: `crates/mkrew-ui/src/desktop.rs` (`TrayEvent`, `raise_running_instance`)
- Modify: `crates/mkrew-ui/src/desktop/win32.rs` (imports, constants after line 112, `window_proc` lines 378-394, `raise` lines 457-467, tests lines 609-627)
- Modify: `crates/mkrew-ui/src/desktop/none.rs` (`raise`)
- Modify: `crates/mkrew-ui/src/shell/attention.rs` (`tray_event`)
- Modify: `crates/mkrew-ui/src/main.rs` (lines 168-173)

**Interfaces:**
- Consumes: `mkrew_core::link::parse_session_link(&str) -> Option<(Ulid, Ulid)>` (Task 1); `Shell::notification_clicked(&mut self, session: Ulid, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)` (existing, `shell/workspace.rs:247`).
- Produces: `TrayEvent::Notification { session: Ulid, project: Ulid }`; `pub fn raise_running_instance(socket: &str, jump: Option<(Ulid, Ulid)>) -> bool`; `os::raise(key: &str, jump: Option<(Ulid, Ulid)>) -> bool`; in `main`, a local `jump: Option<(Ulid, Ulid)>` that Task 5 passes on.

- [ ] **Step 1: Write the failing tests** — in `win32.rs` `mod tests`, change the existing test's two `raise(&key)` calls to `raise(&key, None)` and add:

```rust
    #[test]
    fn a_jump_survives_the_copy() {
        let jump = (Ulid::new(), Ulid::new());
        assert_eq!(decode_jump(&encode_jump(jump)), Some(jump));
        assert_eq!(decode_jump(&encode_jump(jump)[..31]), None);
        assert_eq!(decode_jump(&[0u8; 33]), None);
    }

    #[test]
    fn a_link_launch_reaches_the_tray_window_with_its_session() {
        let key = format!("test-jump-{}", std::process::id());
        let jump = (Ulid::new(), Ulid::new());
        assert!(!raise(&key, Some(jump)), "nobody listens yet");
        let (tray, events) = Tray::start(&key).expect("the tray window");
        // `SendMessageTimeoutW` returns once the tray thread handled it: no polling needed.
        assert!(raise(&key, Some(jump)));
        assert_eq!(
            events.try_recv().ok(),
            Some(TrayEvent::Notification { session: jump.0, project: jump.1 })
        );
        tray.remove();
    }
```

- [ ] **Step 2: Run, expect compile errors** — Run: `cargo test -p mkrew-ui desktop::` · Expected: FAIL to compile (`raise` takes 1 argument; `encode_jump`, `decode_jump`, `TrayEvent::Notification` not found).

- [ ] **Step 3: `TrayEvent` and the forwarding entry point** — in `desktop.rs`, add after `Session(Ulid),`:

```rust
    /// Show the window on this session's pane, else its workspace: a toast's `mkrew://` link
    /// (ADR 0078).
    Notification { session: Ulid, project: Ulid },
```

  and replace `raise_running_instance`:

```rust
/// Asks another running Mkrew of the same exe and daemon to show its window, on `jump`'s
/// session when a toast link started this launch. `true`: it was asked, this launch should end.
pub fn raise_running_instance(socket: &str, jump: Option<(Ulid, Ulid)>) -> bool {
    let raised = os::raise(&current_key(socket), jump);
    if raised {
        tracing::info!(
            "Mkrew is already running on {socket}: asked it to show its window, exiting"
        );
    }
    raised
}
```

  In `none.rs`: add `use ulid::Ulid;` and make it `pub fn raise(_key: &str, _jump: Option<(Ulid, Ulid)>) -> bool { false }`.

- [ ] **Step 4: `win32.rs`** — imports: add `use ulid::Ulid;`, `use windows::Win32::System::DataExchange::COPYDATASTRUCT;`, and to the second `WindowsAndMessaging` list `AllowSetForegroundWindow, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_COPYDATA`. After `const WM_OPEN` add:

```rust
/// `dwData` of the `WM_COPYDATA` a link launch sends ("MKJP"); `lpData` is [`encode_jump`].
const COPYDATA_JUMP: usize = 0x4D4B_4A50;
/// How long a link launch waits for the running UI before giving up (a hung UI).
const COPYDATA_TIMEOUT_MS: u32 = 2000;

/// Session then project ULID, 16 bytes each.
fn encode_jump((session, project): (Ulid, Ulid)) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&session.to_bytes());
    out[16..].copy_from_slice(&project.to_bytes());
    out
}

fn decode_jump(bytes: &[u8]) -> Option<(Ulid, Ulid)> {
    let bytes: &[u8; 32] = bytes.try_into().ok()?;
    let (session, project) = bytes.split_at(16);
    Some((
        Ulid::from_bytes(session.try_into().ok()?),
        Ulid::from_bytes(project.try_into().ok()?),
    ))
}
```

  In `window_proc`, before the `other if other == state.taskbar_created` arm:

```rust
        WM_COPYDATA => {
            // SAFETY: for WM_COPYDATA `lParam` points to a COPYDATASTRUCT, and `lpData` to its
            // `cbData` bytes, both valid for this call.
            let jump = unsafe {
                let data = &*(lparam.0 as *const COPYDATASTRUCT);
                (data.dwData == COPYDATA_JUMP && !data.lpData.is_null())
                    .then(|| {
                        std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize)
                    })
                    .and_then(decode_jump)
            };
            let Some((session, project)) = jump else {
                return LRESULT(0);
            };
            state.send(TrayEvent::Notification { session, project });
            return LRESULT(1);
        }
```

  Replace `raise`:

```rust
/// Another running Mkrew of the same exe was asked to show its window, on `jump`'s session
/// when given.
pub fn raise(key: &str, jump: Option<(Ulid, Ulid)>) -> bool {
    let class = class_name(key);
    // SAFETY: `class` is NUL-terminated and outlives `FindWindowW`; `bytes` and `data` outlive
    // the synchronous `SendMessageTimeoutW`, which copies them into the other process.
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) else {
            return false;
        };
        let Some(jump) = jump else {
            return PostMessageW(Some(hwnd), WM_OPEN, WPARAM(0), LPARAM(0)).is_ok();
        };
        // The toast click let this launch come to the front; pass that right to the running UI.
        let mut pid = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) != 0 {
            let _ = AllowSetForegroundWindow(pid);
        }
        let bytes = encode_jump(jump);
        let data = COPYDATASTRUCT {
            dwData: COPYDATA_JUMP,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut c_void,
        };
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&raw const data as isize),
            SMTO_ABORTIFHUNG,
            COPYDATA_TIMEOUT_MS,
            Some(&mut result),
        );
        sent.0 != 0 && result == 1
    }
}
```

  Check the 0.62 signatures of `SendMessageTimeoutW`, `GetWindowThreadProcessId`, `AllowSetForegroundWindow` on docs.rs and adjust argument wrapping only.

- [ ] **Step 5: Shell and `main`** — `attention.rs` `tray_event` match gains:

```rust
            TrayEvent::Notification { session, project } => {
                self.notification_clicked(session, project, window, cx);
            }
```

  `main.rs` lines 168-173 become:

```rust
    // Started by a toast click (`mkrew://` link, ADR 0078): show that session's pane.
    let jump = std::env::args()
        .nth(1)
        .and_then(|arg| mkrew_core::link::parse_session_link(&arg));
    // A second launch of the same exe on the same daemon shows the running window (it may be
    // hidden in the tray) instead of opening another one; a dev build at another path, or on
    // another `MKREW_SOCKET`, starts on its own.
    if mkrew_ui::desktop::raise_running_instance(&paths.socket, jump) {
        return;
    }
```

- [ ] **Step 6: Run the tests** — Run: `cargo test -p mkrew-ui desktop::` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings` · Expected: all `desktop` tests pass (incl. the 2 new ones); clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/Cargo.toml crates/mkrew-ui/src/desktop.rs crates/mkrew-ui/src/desktop/win32.rs crates/mkrew-ui/src/desktop/none.rs crates/mkrew-ui/src/shell/attention.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): a mkrew:// launch shows the session in the running window"
```

---

### Task 5: A link launch with no UI running opens the pane

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`struct Shell` field after `reveal`, line ~347; `Shell::new` call, line 224)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::new` signature lines 6-13 and field init near line 446; `refresh` list branch lines 500-522)

**Interfaces:**
- Consumes: the `jump` local from Task 4 in `main`; `Shell::notification_clicked` (existing).
- Produces: field `pending_jump: Option<(Ulid, Ulid)>`; `Shell::new(launched, paths, keymap, keymap_errors, jump: Option<(Ulid, Ulid)>, window, cx)`.

- [ ] **Step 1: Field** — in `struct Shell` after `reveal: Option<Ulid>,`:

```rust
    /// The session a `mkrew://` link asked for when it started this UI (ADR 0078); shown
    /// after the first session list.
    pending_jump: Option<(Ulid, Ulid)>,
```

- [ ] **Step 2: Constructor** — `Shell::new` gains `jump: Option<(Ulid, Ulid)>,` after `keymap_errors: &[KeymapError],`; the struct literal gains `pending_jump: jump,` (next to `toasts: Toasts::default(),`). In `main.rs` the call becomes `Shell::new(launched, paths, keymap, &keymap_errors, jump, window, cx)`.

- [ ] **Step 3: Show it after the first list** — in `refresh`, inside `Ok((projects, sessions)) => { … }`, after the `match current.or_else(|| shell.initial_workspace()) { … }` block:

```rust
                        // After the default workspace, so the pane a toast asked for wins.
                        if let Some((session, project)) = shell.pending_jump.take() {
                            shell.notification_clicked(session, project, window, cx);
                        }
```

- [ ] **Step 4: Build** — Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings` and `cargo test -p mkrew-ui` · Expected: clean, all pass. (GPUI startup wiring; proven by the smoke in Task 6, not a unit test.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): a mkrew:// launch with no UI open starts on the session"
```

---

### Task 6: ADR, SPEC, verify, smoke

**Files:**
- Create: `docs/decisions/0078-toast-click-opens-a-mkrew-link.md`
- Modify: `docs/SPEC.md` (§3 table line ~170; §7 event table line ~608; §8 "Notifications" lines ~687-690; §20A item 2 line ~1860)

- [ ] **Step 1: ADR** — check `ls docs/decisions | tail -3` first; write:

```markdown
# 0078 — A Windows toast click opens a `mkrew://` link

- Date: 2026-10-25
- Status: accepted (SPEC §3, §7, §8, §20A)
- Milestone: none (bug report 2026-10-25)

## Context

Toasts were shown through `notify-rust` under PowerShell's AUMID, and a click was read from the
in-process `Activated` event. Windows raises it only while the banner is on screen (about 6 s):
a click in the Notification Center, or on a toast Windows sent there without a banner (Do Not
Disturb, full screen, a rarely clicked app), never reached Mkrew. `wait_for_response` also ended
at the banner's timeout.

## Decision

- Windows toasts are built by the daemon (WinRT through `windows` 0.62) with
  `activationType="protocol"` and `launch="mkrew://session/<session>?project=<project>"`, still
  under PowerShell's AUMID. Nothing waits for a click; `notify-rust` stays for Linux and macOS.
- At start, the user's daemon (OS notifier on, no `MKREW_SOCKET`, `mkrew.exe` beside it) writes
  `HKCU\Software\Classes\mkrew` → `"<dir>\mkrew.exe" "%1"`.
- `mkrew.exe <link>`: when a UI of the same exe and daemon runs, it grants it the foreground
  (`AllowSetForegroundWindow`) and sends both ids by `WM_COPYDATA` to its tray window, which
  calls the existing `notification_clicked`; otherwise the new UI opens the pane after its first
  session list.

## Consequences

- A click works from the banner and the Notification Center, with the UI visible, hidden to the
  tray or closed.
- Any program or web page can open a `mkrew://` link; the most it does is show a session.
- An installed Mkrew and a dev build both on the default pipe overwrite the link; the last
  daemon started owns it. Uninstalling leaves the key (it then points at a missing exe).
- `notification.clicked` is now emitted on Linux only.
- Not done: an own AUMID (needs a Start Menu shortcut with a toast activator CLSID).
```

- [ ] **Step 2: SPEC** —
  - §3 table row: `` | OS notifications | `notify-rust` (Linux, macOS); WinRT toasts through `windows` (Windows, ADR 0078) | | ``
  - §7 events row: `` | `notification.clicked` | user clicked an OS notification on Linux (UI focuses that pane; Windows uses a `mkrew://` link, §8) | ``
  - §8 "Notifications": replace `` Clicking it emits `notification.clicked`. `` with `` Clicking it shows that session's pane: on Windows the toast opens `mkrew://session/<id>?project=<id>`, which the user's daemon registers under HKCU for the `mkrew.exe` beside it (the running UI shows the pane, or a new UI starts on it, ADR 0078); on Linux it emits `notification.clicked`. ``
  - §20A item 2: replace `` (notify-rust `sound_name`, Windows/macOS; Linux hint) `` with `` (notify-rust `sound_name` on macOS, a hint on Linux; the toast's `<audio>` on Windows) ``.

- [ ] **Step 3: Verify** — Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` · Expected: fmt check, clippy `-D warnings` and all tests pass. If an exe is locked, rename it to `*.locked-old.exe`; never kill the daemon, `mkrew` or `omp`.

- [ ] **Step 4: Smoke (needs the user to click)** — never stop the installed daemon. Use an isolated dev setup and point the link at it by hand (the daemon's own registration is covered by Task 3's tests and skips on an own pipe). Drive it from Python `subprocess` with an explicit `env` (the omp `bash` mangles `\\.\pipe`, and `export` in its persistent shell leaks into later commands):
  1. `cargo build -p mkrew-daemon -p mkrew-ui -p mkrew-cli`.
  2. Dirs `%TEMP%\mksmoke\data`, `%TEMP%\mksmoke\cfg\agents`; `cfg\agents\zzprobe.toml`:
     ```toml
     name = "zzprobe"
     kind = "shell"
     prompt_mode = "none"

     [state]
     mode = "screen"
     idle_after_ms = 1000
     needs_input_patterns = ['PROBE_ASK']
     ```
  3. Hand-register the link for the dev UI on the smoke pipe: `HKCU\Software\Classes\mkrew` `(default)`=`URL:Mkrew`, `URL Protocol`=`""`, `shell\open\command\(default)`=
     `cmd /c "set MKREW_DATA_DIR=<TEMP>\mksmoke\data&& set MKREW_CONFIG_DIR=<TEMP>\mksmoke\cfg&& set MKREW_SOCKET=\\.\pipe\mkrew-smoke&& start "" "<repo>\target\debug\mkrew.exe" "%1""`.
  4. With env `MKREW_DATA_DIR`, `MKREW_CONFIG_DIR`, `MKREW_SOCKET=\\.\pipe\mkrew-smoke`: start `target\debug\mkrew-daemon.exe` and `target\debug\mkrew.exe`; `target\debug\mk.exe project add <repo>`; per toast `mk session spawn --agent zzprobe mkrew` then `mk send <id> "echo PROBE_ASK"`.
  5. Ask the user, one case per toast, to let the banner time out and then click the toast in Win+N: (a) UI open behind another window, (b) UI hidden to the tray (✕), (c) UI quit (tray → Quit UI). Expected each time: the Mkrew window comes to the front on the probe session's pane. A new probe session per case (30 s throttle per session).
  6. Clean up: stop the smoke daemon and UI (only the processes this step started), end the probe sessions, `reg delete "HKCU\Software\Classes\mkrew" /f`, delete `%TEMP%\mksmoke`.

- [ ] **Step 5: Commit** (smoke results in the body)

```bash
git add docs/decisions/0078-toast-click-opens-a-mkrew-link.md docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0078 toast click opens a mkrew:// link; SPEC §3 §7 §8 §20A"
```

- [ ] **Step 6: Tell the user** the fix reaches the installed app only after a new release / installer (`mkrew-release` / `mkrew-build-setup` skills, both need their approval); the new daemon's first start registers `mkrew://`.
