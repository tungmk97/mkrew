# Phone access from the Mkrew UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn phone access on/off from the Mkrew UI, set up `tailscale serve` with one click when it is missing, and get the link onto the iPhone by scanning a QR code (or copying it), with `mk web` kept running by the daemon across reboots.

**Architecture:** A `[web] enabled` switch in `settings.toml` is written by the UI; the daemon's existing settings watcher makes a new supervisor start or kill `mk web` as a windowless child (restart with backoff, last stderr line kept as the error) and reports `web: { enabled, running, error }` in `daemon.status` (protocol 40). The UI's Phone access dialog (palette command + title-bar icon) polls that status and the Tailscale state, and shows a QR code of `https://<MagicDNS>/#t=<token>` built from `<config>/web_token`.

**Tech Stack:** Rust 2024, tokio 1 (daemon), GPUI (UI), `qrcodegen` 1.x (new), `serde_json`, Tailscale CLI (`tailscale status --json`, `tailscale serve status --json`, `tailscale serve --bg 8787`).

**Spec:** `docs/superpowers/specs/2026-10-12-phone-access-ui-design.md`. Builds on `docs/superpowers/specs/2026-10-10-mobile-web-design.md` (`mk web`, ADR 0052), already on this branch.

## Global Constraints

- Work only in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/mobile-web` (branch `feat/mobile-web`).
- Disk is nearly full: never create a second `target/`. Every cargo command runs with `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` and `-j 4`.
- Shell: run `cargo`, `git`, `node` directly; Git Bash scripts with `sh`; in the omp shell `bash` is WSL — never use it.
- If a build fails because an exe in `target/` is locked, rename it to `*.locked-old.exe` (add a suffix if taken). Never kill a running mkrew daemon or UI: they belong to the user.
- No test calls a real agent, Tailscale or the network. Tailscale JSON parsing is pure functions tested on literal JSON.
- No `unwrap`/`expect` outside tests (SPEC §17); `thiserror` in libraries, `anyhow` with context in binaries; `tracing` for logs. The link and the token are never logged and never shown in a tooltip.
- Windows: every child process the daemon or the UI starts (`mk web`, `tailscale`) uses `CREATE_NO_WINDOW` (`creation_flags(0x0800_0000)`, the pattern of `crates/mkrew-daemon/src/git.rs`).
- UI: every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (skill `mkrew-ui-design`); status always icon + text; dark and light both work. UI copy is the spec's "Dialog" section, verbatim.
- `PROTOCOL_VERSION` goes up by exactly one (39 → 40 at the time of writing; re-check `crates/mkrew-protocol/src/lib.rs`).
- ADR number 0053 (`docs/decisions/0053-phone-access-ui.md`); 0051 (Lucide icons) and 0052 (mobile web) are taken. Re-check `docs/decisions` before writing it.
- `[web] enabled` is the only new setting (default `false`). No port or listen-address setting.
- Before daemon or CLI integration tests: `cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4` (with `CARGO_TARGET_DIR`).
- `sh scripts/verify.sh` passes at the end of every task.
- Commits: after each task, commit only that task's files (`git add <paths>`, never `-A` or `.`). Conventional Commits. Never push.

## Review Focus

1. **The user clicks Stop while the phone page is open.** Expected: the `mk web` child is killed within about a second, the phone shows "Can't reach Mkrew…", and the child is not restarted. Pinned by Task 2's integration test (disabled → child gone, `running` false) and a HUMAN_CHECKS line.
2. **Port 8787 is already taken (e.g. a hand-started `mk web` from Task Scheduler).** Expected: the dialog shows `✕ Not running: <the bind error>. Retrying…` and the daemon backs off instead of spinning. Pinned by Task 2's "fake exits at once with a stderr line → error equals it" test and the backoff unit test.
3. **`mk.exe` is not next to the daemon and not on PATH.** Expected: a clear `✕ Not running: mk.exe not found next to mkrew-daemon. Retrying…`, looked up again once a minute (not a spin), and installing `mk` later starts it without a click. Pinned by Task 2's `missing mk` integration test.
4. **`settings.toml` has the user's own comments and other sections.** Expected: Start/Stop changes only `[web] enabled`, everything else byte-for-byte the same; a missing `[web]` section is created. Pinned by Task 1's writer tests.
5. **Tailscale missing, logged out, or serving another port.** Expected: the Tailscale row says "Install and sign in to Tailscale first" or offers Set up; never "set up" for a proxy to a different port. Pinned by Task 1's `serve_proxies_port` tests (8787, other port, empty config).

---

### Task 1: Core `[web]` setting, shared Tailscale helpers, protocol `WebStatus`, `mk web` cutover

Below, `T` = `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target`.

**Current state (investigated):**
- `crates/mkrew-core/src/settings.rs`: `Settings` (:16-29, `#[serde(default, deny_unknown_fields)]`, last field `features: FeatureSettings`); `FeatureSettings` (:31-39) is the model for a small bool section (`Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize`). `set_ui_value` (:256-299) is the text-level writer, hard-wired to `[ui]` only through the closure `is_ui_header` (:265-268) and the literal `"[ui]"` (:278). `write_ui_value` (:301-319) reads the file (the template when missing), `parse`s it, calls `set_ui_value`, `parse`s again, writes; returns `Result<(), SettingsError>`. `template()` (:340-397) ends with `[features]` / `# docs = false` / `# turns = false`. Tests live in `mod tests` (:435-883); `set_ui_value` is called at :440, :710, :718, :833, :843, :847, :854; `write_ui_value` at :444, :737, :741, :867, :870; `full_file_parses` (:503-566) builds an exhaustive `Settings { … }` literal (:533-565) — the only exhaustive `Settings` literal in the workspace. The template test `the_template_parses_to_the_defaults_commented_or_not` (:650-669) strips `# ` from every line containing ` = `, so any comment I add to the template must not contain ` = `.
- `write_ui_value` callers (stay as they are): `crates/mkrew-ui/src/shell/inspector.rs:37`, `shell/workspace_actions.rs:404,434,450`. `set_ui_value` has no caller outside settings.rs.
- `crates/mkrew-core/Cargo.toml:18`: `serde_json = "1"` is already a normal dependency (registry has 1.0.151). No new crate, `Cargo.lock` does not change. `mkrew-core` spawns no process today (no `Command` in `crates/mkrew-core/src`); `lib.rs:3-5` doc says its only I/O is files, `lookup` and `paths`; SPEC §4 line 192 says "NO I/O beyond files" (Task 4 rewords it, see Contract notes).
- `crates/mkrew-cli/src/web/tailscale.rs` (47 lines): `dns_name()` (`Command::new("tailscale").args(["status","--json"]).output()`, no `CREATE_NO_WINDOW`), `parse_dns_name`, 2 tests. Only user: `crates/mkrew-cli/src/web.rs:7` (`mod tailscale;`) and `:37` (`args.host.or_else(tailscale::dns_name)`). `mkrew-cli` already depends on `mkrew-core` (Cargo.toml:29) and keeps `serde_json` for other modules. CLI integration test `tests/cli.rs:967-970` passes `--host test.invalid`, so it never runs tailscale.
- `CREATE_NO_WINDOW` pattern: `crates/mkrew-daemon/src/git.rs:935-941` (`#[cfg(windows)]`, `use std::os::windows::process::CommandExt`, `const CREATE_NO_WINDOW: u32 = 0x0800_0000`).
- Protocol: `PROTOCOL_VERSION` = 39 at `crates/mkrew-protocol/src/lib.rs:16`. `DaemonStatusResult` at `crates/mkrew-protocol/src/message.rs:1297-1309` (last field `settings_error` with `#[serde(default, skip_serializing_if = "Option::is_none")]`). Its only struct literal in the workspace: `crates/mkrew-daemon/src/server.rs:470-477` (`Daemon::status`); every other user decodes it (cli `daemon.rs:32,52,87,112`, daemon tests, ui tests). `server.rs:11-21` imports from `mkrew_protocol`. `message/tests.rs` starts with `use super::*; use serde_json::json;` and has helper `round_trip(&Message)`. No test pins the number 39.
- Real Tailscale shape (checked in Tailscale source `ipn/serve.go`, `cmd/tailscale/cli/serve_legacy.go` `runServeStatus`, `serve_v2.go`, and a real output pasted in tailscale/tailscale#19508): `tailscale serve status --json` prints `json.MarshalIndent(ServeConfig, "", "  ")` of the **background** config only (foreground `tailscale serve <port>` sessions are not shown). `ServeConfig` has `TCP map[uint16]*TCPPortHandler` (JSON object keyed by the port as a string; `{"HTTPS": true}` for an HTTPS site), `Web map[HostPort]*WebServerConfig` keyed `"<dnsname>:<port>"` (no implicit 443) with `Handlers map[string]*HTTPHandler` keyed by mount point (`"/"`), and `HTTPHandler.Proxy string` (`Path`, `Text`, `Redirect` are the other, mutually exclusive, kinds). All fields are `omitempty`, so nothing configured prints `{}`. `tailscale serve --bg 8787` stores `Proxy` as `ipn.ExpandProxyTargetValue` returns it: `http://127.0.0.1:8787`; `localhost:8787` becomes `http://localhost:8787`; a bare `8787` is expanded too, but the struct comment lists `localhost:3030, 3030` as accepted spellings, so the reader accepts them. `TCPForward` (a raw TCP forwarder, `"127.0.0.1:8787"`) is not a web handler and does not serve the phone link.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`WebSettings`, `Settings.web`, generic `set_value`/`write_value`, `write_ui_value` thin, `write_web_enabled`, template, tests)
- Create: `crates/mkrew-core/src/tailscale.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (`pub mod tailscale;`, module doc)
- Modify: `crates/mkrew-protocol/src/message.rs` (`WebStatus`, `DaemonStatusResult.web`), `crates/mkrew-protocol/src/message/tests.rs`, `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-daemon/src/server.rs` (`web: WebStatus::default()`)
- Modify: `crates/mkrew-cli/src/web.rs`; Delete: `crates/mkrew-cli/src/web/tailscale.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces (binding names, used by Tasks 2 and 4):
  - `mkrew_core::settings::WebSettings { pub enabled: bool }`, `Settings.web: WebSettings`
  - `mkrew_core::settings::write_web_enabled(settings_file: &Path, enabled: bool) -> Result<(), SettingsError>`
  - `mkrew_core::tailscale::{WEB_PORT: u16 = 8787, ServeCheck { Ready, Missing }, parse_dns_name(&str) -> Option<String>, serve_proxies_port(&str, u16) -> bool, dns_name() -> Option<String>, serve_status() -> Result<ServeCheck, String>, serve_setup(u16) -> Result<(), String>}`
  - `mkrew_protocol::WebStatus { enabled: bool, running: bool, error: Option<String> }` (`Default`), `DaemonStatusResult.web: WebStatus` (`#[serde(default)]`), `PROTOCOL_VERSION = 40`.

- [ ] **Step 1: Write the failing settings tests**

In `crates/mkrew-core/src/settings.rs`, inside `mod tests`:

(a) After `write_theme` (:443-445) add the helper that keeps the seven existing `set_ui_value` calls unchanged (the public function becomes the private generic `set_value` in Step 2):

```rust
    fn set_ui_value(src: &str, key: &str, value: &str) -> String {
        set_value(src, "ui", key, value)
    }
```

(b) In `missing_file_is_defaults` (:454), after `assert!(settings.awake.keep_awake);` add:

```rust
        assert!(!settings.web.enabled, "phone access is off until the user turns it on");
```

(c) In `full_file_parses` (:503-566): in `src` replace the tail

```
[features]
docs = true
"#;
```

with

```
[features]
docs = true

[web]
enabled = true
"#;
```

and in the expected literal replace

```rust
                features: FeatureSettings {
                    docs: true,
                    turns: false,
                },
            }
```

with

```rust
                features: FeatureSettings {
                    docs: true,
                    turns: false,
                },
                web: WebSettings { enabled: true },
            }
```

(d) Before `unreadable_path_is_an_error` (:875) add:

```rust
    #[test]
    fn web_section_defaults_off_and_rejects_other_keys() {
        let web = |src: &str| parse(Path::new("s.toml"), src).unwrap().web;
        assert!(!web("").enabled);
        assert!(!web("[web]").enabled);
        assert!(web("[web]\nenabled = true").enabled);
        assert!(parse_err("[web]\nenabled = \"yes\"").contains("invalid type"));
        // The port is fixed at 8787 (ADR 0053), so it is not a setting.
        assert!(parse_err("[web]\nport = 8787").contains("unknown field `port`"));
    }

    #[test]
    fn set_value_stays_inside_its_own_section() {
        let src = "[ui]\nfont_size = 15\n\n[web]\nenabled = false\n";
        // Replaces the key in [web]; [ui] is untouched.
        assert_eq!(
            set_value(src, "web", "enabled", "true"),
            "[ui]\nfont_size = 15\n\n[web]\nenabled = true\n"
        );
        // A key of the same name in another section is not replaced: it is added to the
        // section asked for.
        assert_eq!(
            set_value(src, "ui", "enabled", "true"),
            "[ui]\nenabled = true\nfont_size = 15\n\n[web]\nenabled = false\n"
        );
        // No such section: appended, never put into the last one.
        assert_eq!(
            set_value("[ui]\nfont_size = 15\n", "web", "enabled", "true"),
            "[ui]\nfont_size = 15\n\n[web]\nenabled = true\n"
        );
    }

    #[test]
    fn write_web_enabled_keeps_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let read = || std::fs::read_to_string(&path).unwrap();

        // No file: the commented template plus the switch; the second write flips it in place.
        write_web_enabled(&path, true).unwrap();
        assert!(load(&path).unwrap().web.enabled);
        write_web_enabled(&path, false).unwrap();
        assert!(!load(&path).unwrap().web.enabled);
        let text = read();
        assert_eq!(text.matches("\nenabled = ").count(), 1, "one key, not two: {text}");
        assert!(text.contains("# docs = false"), "the template stays: {text}");

        // The user's file without [web]: everything stays, [web] is appended.
        let mine = "# mine\n[ui]\ntheme = \"dark\"\n\n[features]\ndocs = true\n";
        std::fs::write(&path, mine).unwrap();
        write_web_enabled(&path, true).unwrap();
        assert_eq!(read(), format!("{mine}\n[web]\nenabled = true\n"));

        // An existing [web] in the middle: only its line changes.
        let middle = "[web]\nenabled = false\n\n[features]\ndocs = true\n";
        std::fs::write(&path, middle).unwrap();
        write_web_enabled(&path, true).unwrap();
        assert_eq!(read(), "[web]\nenabled = true\n\n[features]\ndocs = true\n");
    }

    #[test]
    fn write_web_enabled_never_turns_a_good_file_bad() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Broken TOML; an unknown [web] key; a dotted `web.enabled` (appending `[web]` would
        // declare the table twice): each is refused and the file is left alone.
        for bad in ["[web\n", "[web]\nport = 1\n", "web.enabled = false\n"] {
            std::fs::write(&path, bad).unwrap();
            assert!(write_web_enabled(&path, true).is_err(), "{bad:?}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
        }
    }
```

- [ ] **Step 2: Run them, expect a compile failure**

Run: `T cargo test -p mkrew-core --lib settings -j 4`
Expected: FAIL to compile: `cannot find function set_value`, `cannot find struct WebSettings`, `cannot find function write_web_enabled`, `no field web on type Settings`.

- [ ] **Step 3: Implement the setting and the generalised writer**

In `crates/mkrew-core/src/settings.rs`:

(a) In `Settings` (:16-29) add after `features`:

```rust
    /// `[web]`: phone access (ADR 0053).
    pub web: WebSettings,
```

(b) After `FeatureSettings` (:31-39) add:

```rust
/// `[web]` (ADR 0053): the daemon runs `mk web` while `enabled` is true. The Phone access
/// dialog writes it; the port is fixed (`mkrew_core::tailscale::WEB_PORT`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebSettings {
    pub enabled: bool,
}
```

(c) Replace `set_ui_value` and `write_ui_value` (:256-319, from `/// `src` with `[ui] <key>…` to the closing `}` of `write_ui_value`) with:

```rust
/// `src` with `[<section>] <key> = <value>` (`value` is a TOML literal such as `"dark"`, `320`
/// or `true`): replaces an uncommented `<key> =` line inside the section, else inserts one right
/// after the section header, else appends the section. Everything else (comments, order, other
/// keys, CRLF line ends) is kept byte for byte. Forms this does not recognise (`ui.theme = …`,
/// an inline `ui = {…}`) can yield invalid TOML: callers parse the result before writing it.
fn set_value(src: &str, section: &str, key: &str, value: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("{key} = {value}");
    let header_text = format!("[{section}]");
    // `[ui]`, `[ ui ]` and `[ui] # note` are the same table header.
    let is_header = |l: &&str| {
        let code = l.split('#').next().unwrap_or_default();
        code.split_whitespace().collect::<String>() == header_text
    };
    let mut lines: Vec<&str> = src.lines().collect();
    let Some(header) = lines.iter().position(is_header) else {
        let sep = if src.is_empty() || src.ends_with(&format!("{eol}{eol}")) {
            ""
        } else if src.ends_with('\n') {
            eol
        } else {
            "\n\n"
        };
        return format!("{src}{sep}{header_text}{eol}{line}{eol}");
    };
    let end = lines[header + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| header + 1 + i);
    // `key` followed by `=`: `theme_x = …` is another key.
    let is_key = |l: &&str| {
        l.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    match lines[header + 1..end].iter().position(is_key) {
        Some(i) => lines[header + 1 + i] = &line,
        None => lines.insert(header + 1, &line),
    }
    let mut out = lines.join(eol);
    if src.ends_with('\n') {
        out.push_str(eol);
    }
    out
}

/// Saves `[<section>] <key> = <value>` in `path` (the template when it is missing) with
/// [`set_value`]. Writes nothing when the file does not parse or the edit would not parse
/// (an unusual section form, a value of the wrong type), so a good file never turns bad.
fn write_value(path: &Path, section: &str, key: &str, value: &str) -> Result<(), SettingsError> {
    let io_error = |e: io::Error| SettingsError {
        file: path.to_path_buf(),
        line_col: None,
        message: e.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == io::ErrorKind::NotFound => template().to_owned(),
        Err(e) => return Err(io_error(e)),
    };
    parse(path, &src)?;
    let out = set_value(&src, section, key, value);
    parse(path, &out)?;
    std::fs::write(path, out).map_err(io_error)
}

/// Saves `[ui] <key> = <value>` (`value` is a TOML literal: `"split"` with quotes, or `320`);
/// see [`write_value`].
pub fn write_ui_value(path: &Path, key: &str, value: &str) -> Result<(), SettingsError> {
    write_value(path, "ui", key, value)
}

/// Saves `[web] enabled` (the Phone access dialog's Start / Stop); see [`write_value`]. The
/// daemon applies it through its settings watcher.
pub fn write_web_enabled(path: &Path, enabled: bool) -> Result<(), SettingsError> {
    write_value(path, "web", "enabled", &enabled.to_string())
}
```

(d) In `template()` replace the last two lines

```rust
     # docs = false\n\
     # turns = false\n"
```

with

```rust
     # docs = false\n\
     # turns = false\n\
     \n\
     # Phone access (ADR 0053): while enabled is true the daemon runs `mk web`, so a phone on your\n\
     # tailnet can reach Mkrew. The phone icon in the title bar writes it; the port is fixed (8787).\n\
     [web]\n\
     # enabled = false\n"
```

- [ ] **Step 4: Run the settings tests, expect pass**

Run: `T cargo test -p mkrew-core --lib settings -j 4`
Expected: PASS (all existing settings tests plus `web_section_defaults_off_and_rejects_other_keys`, `set_value_stays_inside_its_own_section`, `write_web_enabled_keeps_the_rest_of_the_file`, `write_web_enabled_never_turns_a_good_file_bad`; `the_template_parses_to_the_defaults_commented_or_not` still passes with the new `[web]` block).

- [ ] **Step 5: Write the failing `tailscale` tests**

Create `crates/mkrew-core/src/tailscale.rs` containing only the tests (the implementation comes in Step 7), and in `crates/mkrew-core/src/lib.rs` add `pub mod tailscale;` after `pub mod task;` (alphabetical: `settings`, `state`, `tailscale`, `task`).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// What `tailscale serve --bg 8787` followed by `tailscale serve status --json` prints
    /// (Tailscale `ipn.ServeConfig`: `TCP map[uint16]*TCPPortHandler`, `Web map[HostPort]
    /// *WebServerConfig` with `Handlers map[string]*HTTPHandler`).
    const SERVED: &str = r#"{
  "TCP": {
    "443": {
      "HTTPS": true
    }
  },
  "Web": {
    "studio-pc.tail1234.ts.net:443": {
      "Handlers": {
        "/": {
          "Proxy": "http://127.0.0.1:8787"
        }
      }
    }
  }
}
"#;

    fn proxying(proxy: &str) -> String {
        SERVED.replace("http://127.0.0.1:8787", proxy)
    }

    #[test]
    fn reads_the_magicdns_name_without_the_trailing_dot() {
        let json = r#"{"Self":{"HostName":"studio-pc","DNSName":"studio-pc.tail1234.ts.net."}}"#;
        assert_eq!(
            parse_dns_name(json).as_deref(),
            Some("studio-pc.tail1234.ts.net")
        );
    }

    #[test]
    fn no_name_without_magicdns() {
        assert_eq!(parse_dns_name(r#"{"Self":{"DNSName":""}}"#), None);
        assert_eq!(parse_dns_name(r#"{"BackendState":"Stopped"}"#), None);
        assert_eq!(parse_dns_name("not json"), None);
    }

    #[test]
    fn recognises_what_serve_bg_8787_writes() {
        assert!(serve_proxies_port(SERVED, 8787));
        // Another port, and ports that only look like it, are not Mkrew.
        assert!(!serve_proxies_port(SERVED, 3000));
        assert!(!serve_proxies_port(SERVED, 87));
        assert!(!serve_proxies_port(&proxying("http://127.0.0.1:18787"), 8787));
    }

    #[test]
    fn accepts_the_other_spellings_of_the_same_target() {
        for proxy in [
            "http://localhost:8787",
            "http://127.0.0.1:8787/",
            "localhost:8787",
            "127.0.0.1:8787",
            "8787",
        ] {
            assert!(serve_proxies_port(&proxying(proxy), 8787), "{proxy}");
        }
    }

    #[test]
    fn rejects_targets_that_do_not_reach_mk_web() {
        for proxy in [
            "https://127.0.0.1:8787",      // `mk web` speaks plain HTTP
            "http://127.0.0.1:8787/api",   // the page would be served from a sub-path
            "http://192.168.1.5:8787",     // another machine
            "http://127.0.0.1",            // port 80
            "unix:/tmp/mkrew.sock",
        ] {
            assert!(!serve_proxies_port(&proxying(proxy), 8787), "{proxy}");
        }
    }

    #[test]
    fn the_link_needs_the_root_mount_over_https_on_443() {
        // `https://<name>/` is port 443: another port, a sub-path mount or a listener that is
        // not HTTPS does not make the link work.
        let on_8443 = SERVED.replace("ts.net:443", "ts.net:8443");
        let sub_path = SERVED.replace(r#""/": {"#, r#""/phone": {"#);
        let no_https = SERVED.replace(r#""HTTPS": true"#, r#""HTTPS": false"#);
        for (what, json) in [("8443", on_8443), ("sub-path", sub_path), ("no https", no_https)] {
            assert!(!serve_proxies_port(&json, 8787), "{what}");
        }
    }

    #[test]
    fn a_handler_that_is_not_a_proxy_does_not_count() {
        let path = SERVED.replace(r#""Proxy": "http://127.0.0.1:8787""#, r#""Path": "C:\\site""#);
        assert!(!serve_proxies_port(&path, 8787));
        let text = SERVED.replace(r#""Proxy": "http://127.0.0.1:8787""#, r#""Text": "8787""#);
        assert!(!serve_proxies_port(&text, 8787));
    }

    #[test]
    fn finds_the_proxy_among_several_sites() {
        let json = r#"{
  "TCP": { "443": { "HTTPS": true } },
  "Web": {
    "a.tail1234.ts.net:443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:3000" } } },
    "studio-pc.tail1234.ts.net:443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:8787" } } }
  }
}"#;
        assert!(serve_proxies_port(json, 8787));
        assert!(serve_proxies_port(json, 3000));
        assert!(!serve_proxies_port(json, 8080));
    }

    #[test]
    fn nothing_published_is_not_set_up() {
        for json in [
            "{}", // what `serve status --json` prints with no config
            "null",
            "",
            "not json",
            r#"{"Web": []}"#,
            // A raw TCP forwarder is not the HTTPS page.
            r#"{"TCP":{"443":{"TCPForward":"127.0.0.1:8787"}}}"#,
            // A foreground `tailscale serve 8787` ends with its terminal: not a setup.
            r#"{"Foreground":{"session-1":{"TCP":{"443":{"HTTPS":true}},"Web":{"h.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:8787"}}}}}}}"#,
        ] {
            assert!(!serve_proxies_port(json, 8787), "{json}");
        }
    }

    #[test]
    fn the_message_is_what_tailscale_said() {
        assert_eq!(said(b"  denied: not logged in\r\n", b"ignored"), "denied: not logged in");
        // Some versions print their complaint on stdout.
        assert_eq!(said(b" \n", b"Serve is not enabled on your tailnet.\n"), "Serve is not enabled on your tailnet.");
        assert_eq!(said(b"", b"\n"), "");
    }
}
```

- [ ] **Step 6: Run them, expect a compile failure**

Run: `T cargo test -p mkrew-core --lib tailscale -j 4`
Expected: FAIL to compile: `cannot find function parse_dns_name`, `serve_proxies_port`, `said` in this scope.

- [ ] **Step 7: Implement `mkrew_core::tailscale`**

Put this at the top of `crates/mkrew-core/src/tailscale.rs`, above `#[cfg(test)]`:

```rust
//! The `tailscale` CLI as the phone page needs it (ADR 0052, ADR 0053): this PC's MagicDNS name
//! and whether `tailscale serve` already publishes `mk web`. The JSON readers are pure; the
//! runners start `tailscale` without a console window (the daemon and the UI have none).

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The port `mk web` listens on by default and `tailscale serve` is pointed at (ADR 0052).
pub const WEB_PORT: u16 = 8787;

/// `tailscale serve --bg` can wait for the user (e.g. "Serve is not enabled on your tailnet"
/// with a link to approve it); [`serve_setup`] gives up after this.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether `tailscale serve` publishes `mk web` on this PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeCheck {
    Ready,
    Missing,
}

/// `tailscale status --json` → `Self.DNSName` without the trailing dot; `None` when Tailscale
/// is missing, stopped or has no MagicDNS name.
pub fn parse_dns_name(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    let name = value
        .get("Self")?
        .get("DNSName")?
        .as_str()?
        .trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}

/// `tailscale serve status --json` (Tailscale's `ipn.ServeConfig` of the background config):
/// true when `https://<this PC>/` (port 443, mount `/`) proxies to `mk web` on `port`.
/// The proxy may be spelled `http://127.0.0.1:<port>` (what `tailscale serve --bg <port>`
/// stores), `http://localhost:<port>`, with or without `http://` and a trailing `/`, or just the
/// port. Anything unreadable, empty (`{}`), a file or text handler, a TCP forwarder or a
/// foreground-only config is false.
pub fn serve_proxies_port(json: &str, port: u16) -> bool {
    let Ok(config) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    // tailscaled answers HTTPS on 443 only when `TCP["443"].HTTPS` is on.
    if config.pointer("/TCP/443/HTTPS").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    let Some(sites) = config.get("Web").and_then(Value::as_object) else {
        return false;
    };
    // `~1` is `/` in a JSON pointer: the handler mounted at "/".
    sites.iter().any(|(host_port, site)| {
        host_port.ends_with(":443")
            && site
                .pointer("/Handlers/~1/Proxy")
                .and_then(Value::as_str)
                .is_some_and(|proxy| is_local_target(proxy, port))
    })
}

fn is_local_target(proxy: &str, port: u16) -> bool {
    let target = proxy.strip_prefix("http://").unwrap_or(proxy);
    let target = target.strip_suffix('/').unwrap_or(target);
    let port = port.to_string();
    target == port
        || target
            .strip_prefix("127.0.0.1:")
            .or_else(|| target.strip_prefix("localhost:"))
            == Some(port.as_str())
}

/// This PC's MagicDNS name (`tailscale status --json`); `None` when Tailscale is missing,
/// stopped or has no name.
pub fn dns_name() -> Option<String> {
    parse_dns_name(&run(&["status", "--json"]).ok()?)
}

/// Does `tailscale serve` already publish `mk web` on [`WEB_PORT`]? `Err` (a message for the UI)
/// when `tailscale` cannot be started or exits with an error, i.e. it is not installed or its
/// service is not reachable.
pub fn serve_status() -> Result<ServeCheck, String> {
    let json = run(&["serve", "status", "--json"])?;
    Ok(if serve_proxies_port(&json, WEB_PORT) {
        ServeCheck::Ready
    } else {
        ServeCheck::Missing
    })
}

/// `tailscale serve --bg <port>`: publishes `http://127.0.0.1:<port>` as HTTPS on this PC's
/// name inside the tailnet. `Err` is what Tailscale said (trimmed), or why it did not finish.
pub fn serve_setup(port: u16) -> Result<(), String> {
    let mut child = tailscale(&["serve", "--bg", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run tailscale: {e}"))?;
    let deadline = Instant::now() + SETUP_TIMEOUT;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) if Instant::now() >= deadline => {
                // The child may have exited since `try_wait`; there is nothing to do then.
                let _ = child.kill();
                break true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("cannot wait for tailscale: {e}")),
        }
    };
    let output = child
        .wait_with_output()
        .map_err(|e| format!("cannot read tailscale's answer: {e}"))?;
    if timed_out {
        let detail = said(&output.stderr, &output.stdout);
        return Err(format!(
            "tailscale serve did not finish in {} s. {detail}",
            SETUP_TIMEOUT.as_secs()
        )
        .trim_end()
        .to_owned());
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(failure(&output))
    }
}

/// `tailscale <args>`: no stdin, no console window.
fn tailscale(args: &[&str]) -> Command {
    let mut command = Command::new("tailscale");
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The daemon and the UI have no console; without this every call would open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Stdout of a successful `tailscale <args>`; else what it said.
fn run(args: &[&str]) -> Result<String, String> {
    let output = tailscale(args)
        .output()
        .map_err(|e| format!("cannot run tailscale: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(failure(&output))
    }
}

fn failure(output: &Output) -> String {
    match said(&output.stderr, &output.stdout) {
        said if said.is_empty() => format!("tailscale failed ({})", output.status),
        said => said,
    }
}

/// Tailscale's complaint: trimmed stderr, else trimmed stdout, else empty.
fn said(stderr: &[u8], stdout: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let text = if stderr.trim().is_empty() { stdout } else { stderr };
    text.trim().to_owned()
}
```

- [ ] **Step 8: Update the `lib.rs` doc**

In `crates/mkrew-core/src/lib.rs` replace lines 3-5

```rust
//! No async runtime, GPUI or SQLite here (SPEC §4); the only I/O is reading profile and
//! settings files and the prompt library, plus seeding it (and looking up the platform
//! directories in [`paths`] and programs on `PATH` in [`lookup`]).
```

with

```rust
//! No async runtime, GPUI or SQLite here (SPEC §4); the only I/O is reading profile and
//! settings files and the prompt library, plus seeding it (and looking up the platform
//! directories in [`paths`] and programs on `PATH` in [`lookup`]), and the one-shot calls to the
//! `tailscale` CLI in [`tailscale`], shared by `mk web`, the daemon and the UI.
```

- [ ] **Step 9: Run the core tests, expect pass**

Run: `T cargo test -p mkrew-core --lib -j 4`
Expected: PASS, including 10 tests in `tailscale::tests` and the settings tests of Step 4. (If `the_message_is_what_tailscale_said` fails on the CRLF case, the trim in `said` is the bug to fix, not the test.)

- [ ] **Step 10: Write the failing protocol test**

Append to `crates/mkrew-protocol/src/message/tests.rs`:

```rust
#[test]
fn daemon_status_carries_the_phone_access_state() {
    let status = DaemonStatusResult {
        daemon_version: "0.1.4".into(),
        protocol_version: crate::PROTOCOL_VERSION,
        pid: 7,
        started_at: 1,
        sessions: 0,
        settings_error: None,
        web: WebStatus {
            enabled: true,
            running: false,
            error: Some("Only one usage of each socket address is normally permitted".into()),
        },
    };
    let value = serde_json::to_value(&status).unwrap();
    assert_eq!(
        value["web"],
        json!({
            "enabled": true,
            "running": false,
            "error": "Only one usage of each socket address is normally permitted"
        })
    );
    assert_eq!(serde_json::from_value::<DaemonStatusResult>(value).unwrap(), status);

    // No error: the key is absent, like `settings_error`.
    assert_eq!(
        serde_json::to_value(WebStatus::default()).unwrap(),
        json!({ "enabled": false, "running": false })
    );

    // A reply without `web` (a daemon that predates it) still decodes, as "off".
    let old = json!({
        "daemon_version": "0.1.3", "protocol_version": 39, "pid": 7, "started_at": 1, "sessions": 0
    });
    let decoded: DaemonStatusResult = serde_json::from_value(old).unwrap();
    assert_eq!(decoded.web, WebStatus::default());
}
```

- [ ] **Step 11: Run it, expect a compile failure**

Run: `T cargo test -p mkrew-protocol --lib daemon_status_carries -j 4`
Expected: FAIL to compile: `cannot find struct WebStatus`, `struct DaemonStatusResult has no field named web`.

- [ ] **Step 12: Implement `WebStatus` and bump the protocol**

In `crates/mkrew-protocol/src/message.rs`, directly above `DaemonStatusResult` (:1297) add:

```rust
/// Phone access (ADR 0053): `[web] enabled` and whether the daemon's `mk web` child runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebStatus {
    /// `[web] enabled` in `settings.toml`.
    pub enabled: bool,
    /// The `mk web` child is alive.
    pub running: bool,
    /// Why it is not running while enabled (the last line `mk web` printed to stderr, or that
    /// `mk` was not found); cleared while it runs and when disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
```

and in `DaemonStatusResult`, after `settings_error`:

```rust
    /// Phone access state (ADR 0053); absent from a daemon that predates it.
    #[serde(default)]
    pub web: WebStatus,
```

In `crates/mkrew-protocol/src/lib.rs:16` change `= 39;` to `= 40;` (re-check the current value first; bump by exactly one).

In `crates/mkrew-daemon/src/server.rs`: in the `use mkrew_protocol::{…}` list (:11-21) change `TerminalTab, WORKSPACE_COLORS, encode,` to `TerminalTab, WORKSPACE_COLORS, WebStatus, encode,` and in `Daemon::status` (:470-477) add after `settings_error: self.settings.error(),`:

```rust
            web: WebStatus::default(),
```

(Task 2 replaces this default with the supervisor's real state.)

- [ ] **Step 13: Cut `mk web` over to the core module**

In `crates/mkrew-cli/src/web.rs`: delete line 7 `mod tailscale;` and change line 37 to

```rust
    let host = args.host.or_else(mkrew_core::tailscale::dns_name);
```

Delete `crates/mkrew-cli/src/web/tailscale.rs` (`git rm` in Step 15 stages it). Nothing else in `mkrew-cli` refers to it.

- [ ] **Step 14: Run the targeted tests, expect pass**

Run: `T cargo test -p mkrew-protocol -j 4`
Expected: PASS, including `daemon_status_carries_the_phone_access_state`.

Run: `T cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4`
Expected: builds with no warnings (no other `DaemonStatusResult` literal exists; `mk web` compiles against `mkrew_core::tailscale`).

Run: `T cargo test -p mkrew-cli --bin mk web:: -j 4`
Expected: PASS (token, http and relay tests; the two tailscale tests now run in `mkrew-core`).

- [ ] **Step 15: Verify and commit**

```bash
cargo fmt --all
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target CARGO_BUILD_JOBS=4 sh scripts/verify.sh
```
Expected: `verify: OK` (fmt check, clippy `-D warnings`, whole workspace tests; the daemon and CLI integration tests run against the binaries built in Step 14).

```bash
git rm -q crates/mkrew-cli/src/web/tailscale.rs
git add crates/mkrew-core/src/settings.rs crates/mkrew-core/src/tailscale.rs crates/mkrew-core/src/lib.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/src/server.rs crates/mkrew-cli/src/web.rs
git commit -m "feat(core): [web] setting, shared tailscale helpers and daemon.status web field (protocol 40, ADR 0053)"
```

**Contract notes:**
- `serve_proxies_port` is stricter than "any Web handler `/`": the site key must end in `:443` and `TCP["443"].HTTPS` must be true, because the link `https://<name>/` has no port. A site on `:8443` reads as not set up, so **Set up** (`tailscale serve --bg 8787`, which adds the 443 mapping) is the right button. A bare `8787` proxy is accepted too (Tailscale's own struct comment lists it).
- `serve_status()` is `Err` only when `tailscale` cannot start or exits non-zero (not installed, `tailscaled` unreachable). A logged-out but running Tailscale answers `{}`/a config and reads `Missing`; Task 4 can show "Install and sign in to Tailscale first" for `Err` or `dns_name() == None`.
- `serve_setup` has a 30 s limit (kills the child): `tailscale serve --bg` can wait for the user to approve "Serve" in the admin console; without the limit the dialog's Set up would hang forever.
- Core now spawns a process, which SPEC §4 line 192 ("NO I/O beyond files") forbids in words; `lib.rs` doc is updated here, Task 4 must reword that SPEC line in its docs step (the SPEC is Task 4's file).
- `set_ui_value` became the private `set_value(src, section, key, value)` and `write_value` is private; `write_ui_value` stays public as a one-line call (3 UI callers, no migration needed). The old `set_ui_value` name survives only as a test helper in `mod tests`.
- The `mk web` hint line `run `tailscale serve --bg {port}`` (web.rs:77-80) is left as is; `--listen` still defaults to the literal `127.0.0.1:8787` in `main.rs`, which equals `WEB_PORT`.

---

### Task 2: Daemon supervises `mk web`

**Current state (investigated):**
- `crates/mkrew-daemon/src/settings.rs`: `SettingsHolder { path, state: Mutex<State> }` (:18-26). `reload()` (:43-59) replaces `state.settings` on success and keeps the last good ones on error; `current()` (:62-64) clones. The only trigger is `watch()` → `watch_file` → `debounce` (:83-128, `QUIET` = 300 ms). **There is no change signal.** `Settings: PartialEq` is already used by `reload` (:48).
- Who reads settings today: `awake.rs` reads `keep_awake` "on every state event" (doc :6-7) and `notify.rs` the same way, per event. **Nothing polls** (the contract's "poll like awake.rs" does not match the code). A 500 ms poll would wake the daemon forever to catch a switch that changes a few times a year, and add up to 500 ms on top of the 300 ms debounce.
- **Mechanism chosen: `tokio::sync::Notify` inside `SettingsHolder`**, fired by `reload()` only when the loaded settings differ from the old ones (`notify_one` keeps one wake-up for a waiter that is not waiting yet, so a change between "read `current()`" and "await" is never lost). `watch` would need a payload (`Settings` clone or a bool) that the one consumer re-reads anyway; `Notify` is 4 lines, needs no runtime to be created (`SettingsHolder::load` runs in sync unit tests, settings.rs:136-168) and has one consumer: the web supervisor. Caveat that the supervisor handles: `load()` calls `reload()` (:38), so a file that already enables the web fires once at construction; the supervisor treats every wake-up as "re-read `current()`", so a spurious one is harmless.
- `crates/mkrew-daemon/src/spawn.rs:10-19`: `daemon_exe()` = `mkrew-daemon{EXE_SUFFIX}` next to `current_exe()` if it is a file, else the bare name for PATH. The name is hard-coded, so `mk` cannot reuse it as is: this task extracts the body into `pub fn sibling_exe(base: &str)` and `daemon_exe()` calls it (callers in mk/UI unchanged). The file has no tests yet (ends at :64).
- `crates/mkrew-daemon/src/git.rs:935-941`: the `CREATE_NO_WINDOW` pattern (`const CREATE_NO_WINDOW: u32 = 0x0800_0000; command.creation_flags(..)`). tokio's own `Command` has the same method on Windows (tokio 1.53.2 in Cargo.lock: `process/mod.rs:675 creation_flags`, `:664 kill_on_drop`, `:1326 Child::kill`, `:1379 Child::wait`; `Child.stderr` is a public `Option<ChildStderr>`). `crates/mkrew-daemon/Cargo.toml:31` already enables `process`, `io-util`, `sync`, `time`, `rt-multi-thread`, `macros`; dev-dependencies add `test-util` (:53). **No Cargo change in this task.**
- `crates/mkrew-daemon/src/server.rs`: `struct Daemon` (:53-63); `Daemon::new` (:86-98, `settings: Arc::new(SettingsHolder::load(..))` at :91); `serve` (:105-132) starts awake at :111-115 and aborts `backups`/`keep_awake` at :126-127 after `accept` returns, then `self.hub.shutdown().await`; `status()` (:469-478) builds `DaemonStatusResult` — after Task 1 it contains `web: WebStatus::default()` (and possibly a `WebStatus` import in the `use mkrew_protocol::{..}` list at :11-21). It is the only `DaemonStatusResult` literal in the daemon crate.
- `crates/mkrew-daemon/src/lib.rs`: module list :6-31 (alphabetical, `mod watch;` then `mod worktree;`), `pub use notify::ENV_NOTIFY;` :38 — the model for a test env hook that integration tests read as `mkrew_daemon::ENV_NOTIFY`.
- `crates/mkrew-daemon/tests/daemon/main.rs`: module list :43-55 (`mod turns; mod workspaces;`); `Env` :60-198 — `spawn()` :164-180 builds the daemon `Command` with fixed env and `Stdio::null()`, `start()` :182-197 waits for `hello`; `Daemon` :201-221 kills the process on drop and has `wait_exit`; `Client::call` :244, `Client::stop` :381; `built_bin(name)` :387-396 returns a workspace bin next to `mkrew-daemon` (what `write_fake_profile_as` uses :132-143); `wait_text(path)` :892 waits for non-blank file text. `basics.rs:35-71` already proves that editing `config/settings.toml` under a running daemon is picked up through the real watcher. `Env` has **no way to pass extra env vars to the daemon** — this task adds `spawn_with`/`start_with`.
- **Fake `mk` chosen: `mkrew-fakeagent`** (built beside the daemon, `built_bin("mkrew-fakeagent")`). Why it fits: `main.rs:52-74` takes the first bare argument as a prompt, so `mkrew-fakeagent web` is accepted (prints `Task: web` to stdout, which the daemon discards); the scenario comes from env `MKREW_FAKE_SCENARIO` (`main.rs:30-33`), which the child inherits from the daemon, so a test only sets two env vars on the daemon; steps `write_file` (absolute path works), `sleep` and `ask` (`lib.rs:88-143`) cover all three cases: (1) alive and provably writing (a heartbeat file rewritten every 100 ms: after Stop/daemon exit the file stops changing, which is observable without OS process APIs), (2) exits at once with a stderr line — `ask` with the daemon's null stdin returns `Error::StdinClosed` (`lib.rs:65-66`) and `main.rs:21-24` prints `mkrew-fakeagent: stdin closed while waiting for an answer` to stderr, exit code 2, (3) missing binary via a path that does not exist. Rejected: a new test bin crate (workspace + `Cargo.lock` churn for 20 lines), a `.cmd`/`.sh` script (platform split, and killing `cmd.exe` leaves its grandchild running, which would make "child gone" unprovable).
- The child inherits the daemon's environment (`MKREW_CONFIG_DIR`, `MKREW_DATA_DIR`, `MKREW_SOCKET` when set), so `mk web` reads the same `web_token` and socket as the daemon without extra arguments.

**Files:**
- Create: `crates/mkrew-daemon/src/web.rs`, `crates/mkrew-daemon/tests/daemon/web.rs`
- Modify: `crates/mkrew-daemon/src/settings.rs` (change signal + test), `crates/mkrew-daemon/src/spawn.rs` (`sibling_exe` + test), `crates/mkrew-daemon/src/lib.rs` (`mod web;`, `pub use web::ENV_MK_EXE;`), `crates/mkrew-daemon/src/server.rs` (field, start/stop, status), `crates/mkrew-daemon/tests/daemon/main.rs` (`mod web;`, `Env::spawn_with`/`start_with`)
- Test: unit tests in `web.rs`, `settings.rs`, `spawn.rs`; integration `crates/mkrew-daemon/tests/daemon/web.rs`

**Interfaces:**
- Consumes (Task 1): `mkrew_core::settings::Settings.web.enabled: bool` (`settings.current().web.enabled`); `mkrew_protocol::WebStatus { enabled: bool, running: bool, error: Option<String> }` with `Default + Clone + PartialEq + Debug`; `DaemonStatusResult.web: WebStatus`; the `[web]` section in `settings.toml` (`enabled = true`).
- Produces:
  - `SettingsHolder::changed(&self) -> impl Future<Output = ()>` (async fn): completes after a reload that changed the settings; one wake-up is remembered; single waiter.
  - `mkrew_daemon::spawn::sibling_exe(base: &str) -> anyhow::Result<PathBuf>`; `daemon_exe()` keeps its signature.
  - `crates/mkrew-daemon/src/web.rs`: `pub struct Web` (`Default`) with `pub fn status(&self) -> WebStatus`; `pub fn start(web: Arc<Web>, settings: Arc<SettingsHolder>) -> JoinHandle<()>` (runs until aborted; aborting kills the child); `pub const ENV_MK_EXE: &str = "MKREW_MK_EXE"`, re-exported as `mkrew_daemon::ENV_MK_EXE`. Pure helpers (private, unit-tested): `backoff(u32) -> Duration`, `after_exit(u32, Duration) -> (Duration, u32)`, `async last_line(impl AsyncBufRead + Unpin) -> Option<String>`, `exit_error(Option<String>, &str) -> String`.
  - Behaviour: enabled → spawn `mk web` (`MKREW_MK_EXE`, else `mk` next to the daemon, else PATH) with stdin null, stdout null, stderr piped, `CREATE_NO_WINDOW`, `kill_on_drop`; status `running = true, error = None`. Exit → `error` = last non-empty stderr line (else `mk web stopped (<exit status>)`), restart after `backoff` (1, 2, 4, 8, 16, 32, 60, 60 … s), a run of ≥ 60 s resets the count. Not found → `error = "mk.exe not found next to mkrew-daemon"` (`mk` without `.exe` off Windows), looked up again every 60 s or at once when the settings change. Disabled → child killed, `running = false`, `error = None`. `status().enabled` is the value the supervisor last saw (it follows `settings.toml` after the 300 ms debounce).

- [ ] **Step 1: Write the failing unit tests (settings change signal, `sibling_exe`)**

In `crates/mkrew-daemon/src/settings.rs`, inside `mod tests` (after `bad_file_at_start_means_defaults_and_an_error`, before `debounce_reloads_once_after_quiet`), add:

```rust
    /// Whether `changed()` completes within a second of (paused) time.
    async fn fires(holder: &SettingsHolder) -> bool {
        tokio::time::timeout(Duration::from_secs(1), holder.changed())
            .await
            .is_ok()
    }

    #[tokio::test(start_paused = true)]
    async fn changed_fires_once_per_real_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let holder = SettingsHolder::load(path.clone());
        assert!(!fires(&holder).await, "nothing changed yet");

        std::fs::write(&path, "[web]\nenabled = true\n").unwrap();
        holder.reload();
        assert!(fires(&holder).await);
        assert!(!fires(&holder).await, "one wake-up per change");

        holder.reload();
        assert!(!fires(&holder).await, "same settings again: no wake-up");

        std::fs::write(&path, "[web]\nenabled = \"yes\"\n").unwrap();
        holder.reload();
        assert!(!fires(&holder).await, "a broken file keeps the settings");

        std::fs::write(&path, "[web]\nenabled = false\n").unwrap();
        holder.reload();
        assert!(fires(&holder).await);
    }
```

In `crates/mkrew-daemon/src/spawn.rs`, append at the end of the file:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_exe_prefers_the_file_next_to_the_running_executable() {
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().unwrap().to_str().unwrap();
        assert_eq!(sibling_exe(stem).unwrap(), exe);

        let missing = format!("no-such-program{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(
            sibling_exe("no-such-program").unwrap(),
            PathBuf::from(missing)
        );
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon --lib -j 4 -- settings:: spawn::`
Expected: FAIL to compile — `no method named changed found for struct SettingsHolder`, `cannot find function sibling_exe in this scope`.

- [ ] **Step 3: Implement the change signal and `sibling_exe`**

`crates/mkrew-daemon/src/settings.rs`:

1. Line 12: `use tokio::sync::mpsc;` → `use tokio::sync::{Notify, mpsc};`
2. Replace the struct (lines 18-21):

```rust
pub struct SettingsHolder {
    path: PathBuf,
    state: Mutex<State>,
    /// Woken by a reload that changed the settings; keeps one wake-up for a late waiter.
    changed: Notify,
}
```

3. In `load` (the `Self { .. }` literal, lines 31-37) add the field after the `state:` entry:

```rust
            state: Mutex::new(State {
                settings: Settings::default(),
                error: None,
            }),
            changed: Notify::new(),
        };
```

4. In `reload`, replace the `Ok(settings) => { .. }` arm (lines 47-53):

```rust
            Ok(settings) => {
                let differs = state.settings != settings;
                if state.error.is_some() || differs {
                    tracing::info!(file = %self.path.display(), "settings loaded");
                }
                state.settings = settings;
                state.error = None;
                if differs {
                    self.changed.notify_one();
                }
            }
```

5. After `current()` (line 64) add:

```rust

    /// Completes after a reload that changed the settings; a change that happened before the
    /// call completes it at once (one is remembered, also the one the first load made).
    /// One waiter only: the `mk web` supervisor ([`crate::web`]).
    pub async fn changed(&self) {
        self.changed.notified().await;
    }
```

`crates/mkrew-daemon/src/spawn.rs`: replace lines 8-19 (`daemon_exe`) with:

```rust
/// `mkrew-daemon` installed next to the running executable (the release layout and
/// `target/<profile>/`), else whatever `PATH` finds.
pub fn daemon_exe() -> anyhow::Result<PathBuf> {
    sibling_exe("mkrew-daemon")
}

/// `base` plus the platform's executable suffix, next to the running executable when it is a
/// file there, else the bare name for `PATH` to find. The daemon also finds `mk` this way.
pub fn sibling_exe(base: &str) -> anyhow::Result<PathBuf> {
    let name = format!("{base}{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().context("locate the running executable")?;
    let sibling = exe.with_file_name(&name);
    Ok(if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(name)
    })
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon --lib -j 4 -- settings:: spawn::`
Expected: PASS — the existing settings tests plus `changed_fires_once_per_real_change` and `sibling_exe_prefers_the_file_next_to_the_running_executable`.

- [ ] **Step 5: Write the failing supervisor unit tests**

Create `crates/mkrew-daemon/src/web.rs` with only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_a_minute_and_stays() {
        let secs: Vec<u64> = (0..9).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(backoff(u32::MAX).as_secs(), 60, "no shift overflow");
    }

    #[test]
    fn a_stable_run_forgives_earlier_failures() {
        let quick = Duration::from_secs(2);
        let mut failures = 0;
        let mut waits = Vec::new();
        for ran in [quick, quick, quick, STABLE, quick] {
            let (wait, next) = after_exit(failures, ran);
            waits.push(wait.as_secs());
            failures = next;
        }
        // 1, 2, 4; the run of a minute starts over at 1; the next quick exit waits 2.
        assert_eq!(waits, [1, 2, 4, 1, 2]);
    }

    #[test]
    fn just_under_a_minute_is_still_a_failure() {
        let (wait, next) = after_exit(3, STABLE - Duration::from_millis(1));
        assert_eq!((wait.as_secs(), next), (8, 4));
    }

    #[tokio::test]
    async fn last_line_is_the_last_one_with_text() {
        let text = b"starting\nError: port 8787 in use\n\n  \r\n";
        let last = last_line(BufReader::new(&text[..])).await;
        assert_eq!(last.as_deref(), Some("Error: port 8787 in use"));

        let no_newline = b"first\n  second \r\nthird";
        let last = last_line(BufReader::new(&no_newline[..])).await;
        assert_eq!(last.as_deref(), Some("third"));
        assert_eq!(last_line(BufReader::new(&b"\n \n"[..])).await, None);
    }

    #[tokio::test]
    async fn last_line_keeps_reading_past_invalid_utf8() {
        // Stopping at the bad line would leave the pipe unread and block `mk web` on a full one.
        let text = b"ok\nbad \xff byte\nfine\n";
        let last = last_line(BufReader::new(&text[..])).await;
        assert_eq!(last.as_deref(), Some("fine"));
    }

    #[test]
    fn exit_error_prefers_the_stderr_line() {
        assert_eq!(exit_error(Some("boom".into()), "exit code: 1"), "boom");
        assert_eq!(
            exit_error(None, "exit code: 3"),
            "mk web stopped (exit code: 3)"
        );
    }
}
```

In `crates/mkrew-daemon/src/lib.rs` add `mod web;` between `mod watch;` and `mod worktree;`.

- [ ] **Step 6: Run them to see them fail**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon --lib -j 4 -- web::`
Expected: FAIL to compile — `cannot find function backoff`, `after_exit`, `last_line`, `exit_error`, `cannot find value STABLE`.

- [ ] **Step 7: Implement `web.rs`**

Insert at the very top of `crates/mkrew-daemon/src/web.rs` (above `#[cfg(test)]`):

```rust
//! Phone access: while `[web] enabled = true` in `settings.toml`, the daemon runs `mk web` as a
//! child and reports it in `daemon.status` (`web`). The daemon itself opens no network socket:
//! the listener lives in the `mk web` process (SPEC §15).
//!
//! [`start`] is one task: it reads the switch, runs the child until it exits or the switch
//! goes off, and starts it again after [`backoff`]. [`SettingsHolder::changed`] wakes it. The
//! child dies with the daemon (`kill_on_drop`, and the task is dropped when the daemon ends).

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use mkrew_protocol::WebStatus;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::settings::SettingsHolder;
use crate::spawn;

/// Names the program to run instead of looking for `mk` (tests point it at the fake agent).
pub const ENV_MK_EXE: &str = "MKREW_MK_EXE";

/// A child that ran this long forgives the failures before it.
const STABLE: Duration = Duration::from_secs(60);
/// The longest wait between two starts.
const MAX_WAIT: Duration = Duration::from_secs(60);
/// How long to wait for the last stderr lines of a child that exited.
const DRAIN: Duration = Duration::from_secs(1);

/// What `daemon.status` reports; the supervisor task is its only writer.
#[derive(Default)]
pub struct Web {
    status: Mutex<WebStatus>,
}

impl Web {
    pub fn status(&self) -> WebStatus {
        self.lock().clone()
    }

    fn set(&self, enabled: bool, running: bool, error: Option<String>) {
        *self.lock() = WebStatus {
            enabled,
            running,
            error,
        };
    }

    fn lock(&self) -> MutexGuard<'_, WebStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Supervises `mk web` per `settings.current().web.enabled` until aborted; aborting (or
/// dropping the runtime) kills the child.
pub fn start(web: Arc<Web>, settings: Arc<SettingsHolder>) -> JoinHandle<()> {
    tokio::spawn(supervise(web, settings))
}

/// How a child run ended.
enum Ended {
    /// The switch went off; the child was killed.
    Disabled,
    /// There is no `mk` to run.
    NotFound,
    /// The child exited, or could not start, after running for `ran`.
    Exited { ran: Duration, error: String },
}

async fn supervise(web: Arc<Web>, settings: Arc<SettingsHolder>) {
    let mut failures = 0u32;
    loop {
        if !settings.current().web.enabled {
            web.set(false, false, None);
            failures = 0;
            settings.changed().await;
            continue;
        }
        match run_child(&web, &settings).await {
            // The next turn of the loop publishes "off".
            Ended::Disabled => {}
            Ended::NotFound => {
                web.set(true, false, Some(not_found_message()));
                // Look again once a minute (or at once when the settings change), so installing
                // `mk` later just works and the dialog's "Retrying…" stays true.
                sleep_while_enabled(&settings, STABLE).await;
            }
            Ended::Exited { ran, error } => {
                let (wait, next) = after_exit(failures, ran);
                failures = next;
                tracing::warn!(?ran, ?wait, "mk web stopped, starting it again");
                web.set(true, false, Some(error));
                sleep_while_enabled(&settings, wait).await;
            }
        }
    }
}

/// Starts `mk web` and returns when it is over. Publishes "running" once it started.
async fn run_child(web: &Web, settings: &SettingsHolder) -> Ended {
    let mut child = match spawn_mk() {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ended::NotFound,
        Err(e) => {
            return Ended::Exited {
                ran: Duration::ZERO,
                error: format!("cannot start mk web: {e}"),
            };
        }
    };
    let started = Instant::now();
    tracing::info!(pid = ?child.id(), "mk web started");
    web.set(true, true, None);
    // Read while it runs: a full stderr pipe would block `mk web`.
    let reader = child
        .stderr
        .take()
        .map(|pipe| tokio::spawn(last_line(BufReader::new(pipe))));
    loop {
        tokio::select! {
            status = child.wait() => {
                let last = match reader {
                    Some(reader) => finish(reader).await,
                    None => None,
                };
                let how = match status {
                    Ok(status) => status.to_string(),
                    Err(e) => e.to_string(),
                };
                return Ended::Exited { ran: started.elapsed(), error: exit_error(last, &how) };
            }
            () = settings.changed() => {
                // Any other setting changing leaves the child alone.
                if !settings.current().web.enabled {
                    if let Err(e) = child.kill().await {
                        tracing::warn!("cannot kill mk web: {e}");
                    }
                    if let Some(reader) = &reader {
                        reader.abort();
                    }
                    tracing::info!("mk web stopped: phone access turned off");
                    return Ended::Disabled;
                }
            }
        }
    }
}

/// Sleeps `wait`, or less when the switch goes off (the caller's loop then publishes "off").
async fn sleep_while_enabled(settings: &SettingsHolder, wait: Duration) {
    let deadline = Instant::now() + wait;
    loop {
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => return,
            () = settings.changed() => {
                if !settings.current().web.enabled {
                    return;
                }
            }
        }
    }
}

/// `mk web`: no console window (the daemon has none), no input, output ignored, stderr piped.
fn spawn_mk() -> io::Result<Child> {
    let mut command = Command::new(mk_exe());
    command
        .arg("web")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // Without this the child would open a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

/// `MKREW_MK_EXE` when set (tests), else `mk` next to the running daemon, else `PATH`.
fn mk_exe() -> PathBuf {
    match std::env::var_os(ENV_MK_EXE).filter(|v| !v.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => spawn::sibling_exe("mk").unwrap_or_else(|_| PathBuf::from("mk")),
    }
}

fn not_found_message() -> String {
    format!(
        "mk{} not found next to mkrew-daemon",
        std::env::consts::EXE_SUFFIX
    )
}

/// The wait before the next start, 1, 2, 4 … s up to a minute, for the `failures`-th failure in
/// a row (0 = the first).
fn backoff(failures: u32) -> Duration {
    // 1 << 6 = 64 s is already over the cap; the `min` also keeps the shift in range.
    Duration::from_secs(1u64 << failures.min(6)).min(MAX_WAIT)
}

/// The wait before the next start and the new failure count after a child ran for `ran`: a run
/// of [`STABLE`] or more starts the count over.
fn after_exit(failures: u32, ran: Duration) -> (Duration, u32) {
    let failures = if ran >= STABLE { 0 } else { failures };
    (backoff(failures), failures.saturating_add(1))
}

/// Reads `reader` to its end; the last line with text, trimmed. Bytes that are not UTF-8 are
/// replaced, never a reason to stop reading.
async fn last_line(mut reader: impl AsyncBufRead + Unpin) -> Option<String> {
    let mut last = None;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => return last,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim();
                if !line.is_empty() {
                    last = Some(line.to_owned());
                }
            }
        }
    }
}

/// The stderr reader's result once the child is gone. Bounded: a grandchild that keeps the pipe
/// open must not hold up the restart.
async fn finish(mut reader: JoinHandle<Option<String>>) -> Option<String> {
    let last = tokio::time::timeout(DRAIN, &mut reader).await;
    reader.abort();
    last.ok()?.ok()?
}

/// What `daemon.status` shows for an exited child: its last stderr line, else its exit status.
fn exit_error(last_line: Option<String>, how: &str) -> String {
    last_line.unwrap_or_else(|| format!("mk web stopped ({how})"))
}
```

- [ ] **Step 8: Run the unit tests to see them pass**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon --lib -j 4 -- web:: settings:: spawn::`
Expected: PASS — 6 `web::` tests (`backoff_doubles_to_a_minute_and_stays`, `a_stable_run_forgives_earlier_failures`, `just_under_a_minute_is_still_a_failure`, `last_line_is_the_last_one_with_text`, `last_line_keeps_reading_past_invalid_utf8`, `exit_error_prefers_the_stderr_line`), plus the settings and spawn tests from Step 4. (`dead_code` warnings for `start`/`Web::status`/`ENV_MK_EXE` are expected until Step 11.)

- [ ] **Step 9: Write the failing integration tests**

`crates/mkrew-daemon/tests/daemon/main.rs`:

1. Add `mod web;` between `mod turns;` and `mod workspaces;` (lines 53-54).
2. Replace line 164 (`    fn spawn(&self) -> Daemon {`) with:

```rust
    fn spawn(&self) -> Daemon {
        self.spawn_with(&[])
    }

    /// Like [`Env::spawn`], with extra environment variables for the daemon.
    fn spawn_with(&self, vars: &[(&str, &Path)]) -> Daemon {
```

3. After the `.env("CODEX_HOME", self.dir.path().join("codex"))` line (173) add:

```rust
            .envs(vars.iter().copied())
```

4. Replace lines 182-184 (`/// Spawns a daemon and waits until it answers `hello`.`, `fn start(&self) -> (Daemon, Client) {`, `let mut daemon = self.spawn();`) with:

```rust
    /// Spawns a daemon and waits until it answers `hello`.
    fn start(&self) -> (Daemon, Client) {
        self.start_with(&[])
    }

    /// Like [`Env::start`], with extra environment variables for the daemon.
    fn start_with(&self, vars: &[(&str, &Path)]) -> (Daemon, Client) {
        let mut daemon = self.spawn_with(vars);
```

Create `crates/mkrew-daemon/tests/daemon/web.rs`:

```rust
//! Phone access: while `[web] enabled`, the daemon runs `mk web` as a child and reports it in
//! `daemon.status`. The `mk` here is the fake agent (`MKREW_MK_EXE`): it takes `web` as its
//! prompt and plays the scenario named by `MKREW_FAKE_SCENARIO`.

use mkrew_protocol::WebStatus;

use super::*;

/// The fake agent's `ask` with the daemon's null stdin ends at once, printing this to stderr.
const STDIN_CLOSED: &str = "mkrew-fakeagent: stdin closed while waiting for an answer";

impl Env {
    /// `settings.toml` with `[web] enabled = <on>`.
    fn set_web(&self, on: bool) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), format!("[web]\nenabled = {on}\n")).unwrap();
    }

    /// A daemon whose `mk` is the fake agent playing `scenario` (TOML text).
    fn start_with_mk(&self, scenario: &str) -> (Daemon, Client) {
        let file = self.dir.path().join("mk-scenario.toml");
        std::fs::write(&file, scenario).unwrap();
        let fake = built_bin("mkrew-fakeagent");
        self.start_with(&[
            (mkrew_daemon::ENV_MK_EXE, fake.as_path()),
            ("MKREW_FAKE_SCENARIO", file.as_path()),
        ])
    }
}

/// A `mk web` that lives 15 s and rewrites `beat` with a counter every 100 ms.
fn heartbeat(beat: &Path) -> String {
    (0..150)
        .map(|n| {
            format!(
                "[[step]]\naction = \"write_file\"\npath = '{}'\ncontent = \"{n}\"\n\n\
                 [[step]]\naction = \"sleep\"\nms = 100\n\n",
                beat.display()
            )
        })
        .collect()
}

/// True when `beat` did not change for 600 ms: nothing writes it any more.
fn is_frozen(beat: &Path) -> bool {
    let before = std::fs::read_to_string(beat).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(600));
    std::fs::read_to_string(beat).unwrap_or_default() == before
}

/// Polls `daemon.status` until `web` satisfies `wanted`; returns it.
fn wait_web(client: &mut Client, wanted: impl Fn(&WebStatus) -> bool) -> WebStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
        if wanted(&status.web) {
            return status.web;
        }
        assert!(
            Instant::now() < deadline,
            "web status stayed {:?}",
            status.web
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn mk_web_runs_while_enabled_and_dies_when_disabled() {
    let env = Env::new();
    let beat = env.dir.path().join("beat.txt");
    env.set_web(true);
    let (_daemon, mut client) = env.start_with_mk(&heartbeat(&beat));

    let on = wait_web(&mut client, |w| w.running);
    assert_eq!(
        on,
        WebStatus {
            enabled: true,
            running: true,
            error: None
        }
    );
    wait_text(&beat); // the child really runs, not only reported

    env.set_web(false);
    // `enabled` only turns false after the child was killed and reaped.
    let off = wait_web(&mut client, |w| !w.enabled);
    assert_eq!(off, WebStatus::default());
    assert!(is_frozen(&beat), "mk web still runs after it was turned off");

    env.set_web(true);
    wait_web(&mut client, |w| w.running);
    assert!(!is_frozen(&beat), "turning it on again starts a new child");
    client.stop();
}

#[test]
fn mk_web_dies_with_the_daemon() {
    let env = Env::new();
    let beat = env.dir.path().join("beat.txt");
    env.set_web(true);
    let (mut daemon, mut client) = env.start_with_mk(&heartbeat(&beat));
    wait_web(&mut client, |w| w.running);
    wait_text(&beat);

    client.stop();
    assert!(daemon.wait_exit().success());
    assert!(is_frozen(&beat), "mk web outlived the daemon");
}

#[test]
fn a_mk_web_that_exits_at_once_reports_its_last_stderr_line() {
    let env = Env::new();
    env.set_web(true);
    let (_daemon, mut client) = env.start_with_mk("[[step]]\naction = \"ask\"\n");

    let down = wait_web(&mut client, |w| !w.running && w.error.is_some());
    assert_eq!(
        down,
        WebStatus {
            enabled: true,
            running: false,
            error: Some(STDIN_CLOSED.into())
        }
    );

    env.set_web(false);
    let off = wait_web(&mut client, |w| !w.enabled);
    assert_eq!(off, WebStatus::default(), "turning it off clears the error");
    client.stop();
}

#[test]
fn a_missing_mk_is_reported() {
    let env = Env::new();
    env.set_web(true);
    let missing = env.dir.path().join("no-such-mk");
    let (_daemon, mut client) = env.start_with(&[(mkrew_daemon::ENV_MK_EXE, missing.as_path())]);

    let web = wait_web(&mut client, |w| w.error.is_some());
    assert_eq!(
        web,
        WebStatus {
            enabled: true,
            running: false,
            error: Some(format!(
                "mk{} not found next to mkrew-daemon",
                std::env::consts::EXE_SUFFIX
            ))
        }
    );
    client.stop();
}
```

- [ ] **Step 10: Run them to see them fail**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4 && CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon --test daemon -j 4 web::`
Expected: builds and the 4 tests FAIL after ~30 s each with `web status stayed WebStatus { enabled: false, running: false, error: None }` (Task 1 left `web: WebStatus::default()` in `status()`; `ENV_MK_EXE` is not exported yet, so if the build says `could not find ENV_MK_EXE in mkrew_daemon`, that is the failure for now — Step 11 adds the export).

- [ ] **Step 11: Wire the supervisor into the daemon**

`crates/mkrew-daemon/src/lib.rs`: after `pub use notify::ENV_NOTIFY;` add:

```rust
pub use web::ENV_MK_EXE;
```

`crates/mkrew-daemon/src/server.rs`:

1. Imports: `use crate::{awake, backup, plan, session_log, undo, worktree};` → `use crate::{awake, backup, plan, session_log, undo, web, worktree};`
2. `struct Daemon`: after `settings: Arc<SettingsHolder>,` add:

```rust
    /// What `mk web` is doing (phone access), kept by the task `serve` starts.
    web: Arc<web::Web>,
```

3. `Daemon::new`: after `settings: Arc::new(SettingsHolder::load(paths.settings_file())),` add:

```rust
            web: Arc::default(),
```

4. `serve`: in the doc comment, change "starts keep awake and daily DB backups" to "starts keep awake, `mk web` (phone access, while `[web] enabled`) and daily DB backups". After the `let keep_awake = awake::start(..);` statement add:

```rust
        let web_task = web::start(Arc::clone(&self.web), Arc::clone(&self.settings));
```

and after `keep_awake.abort();` add:

```rust
        // Awaited: dropping the task kills `mk web`, and that must have happened before the
        // daemon returns.
        web_task.abort();
        let _ = web_task.await;
```

5. `status()`: replace Task 1's `web: WebStatus::default(),` with:

```rust
            web: self.web.status(),
```

If Task 1 added `WebStatus` to the `use mkrew_protocol::{..}` list in this file and nothing else uses it, remove it there (unused import fails clippy).

- [ ] **Step 12: Run the daemon tests to see them pass**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo build -p mkrew-daemon -p mkrew-fakeagent -p mkrew-cli --bins -j 4 && CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-daemon -j 4`
Expected: PASS — lib unit tests (incl. 6 `web::`, `settings::changed_fires_once_per_real_change`, `spawn::sibling_exe_…`) and the `daemon` integration binary incl. `web::mk_web_runs_while_enabled_and_dies_when_disabled`, `web::mk_web_dies_with_the_daemon`, `web::a_mk_web_that_exits_at_once_reports_its_last_stderr_line`, `web::a_missing_mk_is_reported`. If `mk_web_dies_with_the_daemon` fails, the child outlived `serve`: that is the real bug (the `web_task.await` in `serve`), do not add sleeps.

- [ ] **Step 13: Full verification**

Run: `sh scripts/verify.sh`
Expected: `verify: OK`. If `cargo fmt --check` complains, run `cargo fmt --all` and re-run (the `tests/daemon/web.rs` chained calls and the `format!` string are the likely spots).

- [ ] **Step 14: Commit**

```
git add crates/mkrew-daemon/src/web.rs crates/mkrew-daemon/src/settings.rs crates/mkrew-daemon/src/spawn.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon/main.rs crates/mkrew-daemon/tests/daemon/web.rs
git commit -m "feat(daemon): run mk web while [web] enabled and report it in daemon.status"
```

**Contract notes:**
- The contract's "poll `current()` every 500 ms like awake.rs" does not match the code: `awake.rs`/`notify.rs` read settings per hub event and never poll. Chosen instead: `SettingsHolder::changed()` backed by `tokio::sync::Notify`, fired by `reload()` only on a real change (justified in Current state). Single waiter by design.
- Exports are `web::Web` + `web::start(web, settings)` (shape of `awake::start`) rather than one `status()` on a supervisor object; `Daemon.web: Arc<web::Web>` is created in `Daemon::new`, so `status()` works before `serve` runs. `pub use web::ENV_MK_EXE` mirrors `ENV_NOTIFY`.
- `spawn::sibling_exe(base)` is new public API in `mkrew_daemon::spawn` (`daemon_exe` now calls it); nothing else in the workspace changes.
- `WebStatus.enabled` is written by the supervisor, not read live from the settings, so `running`/`error` can never contradict it; it follows `settings.toml` after the 300 ms debounce.
- Not-found message uses `EXE_SUFFIX`: exactly `mk.exe not found next to mkrew-daemon` on Windows, `mk not found …` elsewhere. Controller ruling: a missing `mk` is looked up again every `STABLE` (60 s) or at once on a settings change — a stat per minute, not a spin — so the dialog's "Retrying…" is true and installing `mk` later needs no click (spec updated). Not asserted by an integration test (the missing path is fixed per daemon process).
- The stderr line is never logged (only `ran`/`wait`); `MKREW_MK_EXE` is a test hook and is not documented in SPEC (like `MKREW_NOTIFY`, which SPEC does not list either). Task 4's SPEC §8/§15 text may say "the daemon starts `mk web` next to `mkrew-daemon`".

---

### Task 3: UI pure pieces — `qrcodegen`, `phone.rs` (link / read_token / qr_svg), QR theme tokens, `theme::hex`

Pure, headless pieces the Phone access dialog (Task 4) draws: the link a phone opens, the token the UI reads from `<config>/web_token`, and the QR code of the link as an SVG string. No GPUI rendering here; everything is unit-tested without a window, Tailscale or the network.

**Current state (investigated):**

- `crates/mkrew-ui/Cargo.toml:15-57` — `[dependencies]` is alphabetical, each non-obvious crate has a one-line `#` comment above it (e.g. `:37-38` `pulldown-cmark`, `:32-33` `mermaid-rs-renderer`). `qrcodegen` goes between `pulldown-cmark` (`:38`) and `serde` (`:39`). `tempfile = "3"` is already a dev-dependency (`:67`). `edition = "2024"` (workspace), lints: `unwrap_used` / `expect_used` warn, allowed in tests (`clippy.toml`).
- `Cargo.lock` has no `qrcodegen` entry and `~/.cargo/registry/src/*/` has no copy: the first cargo command after the edit downloads it and adds the lock entry.
- **qrcodegen API, latest is 1.8.0** (checked on docs.rs rustdoc for `qrcodegen 1.8.0`, source `src/lib.rs`; no dependencies, MIT):
  - `QrCode::encode_text(text: &str, ecl: QrCodeEcc) -> Result<QrCode, DataTooLong>` — **`Result` since 1.8** (1.3 returned `Option`; do not copy older snippets). It picks numeric / alphanumeric / byte mode itself and searches versions 1..=40.
  - `enum QrCodeEcc { Low, Medium, Quartile, High }` (`Medium` ≈ 15 % recoverable); `enum DataTooLong { SegmentTooLong, DataOverCapacity }`, implements `Debug + Clone + Error + Display`.
  - `QrCode::size(&self) -> i32` (21..=177, modules per side), `QrCode::version()`, `QrCode::get_module(&self, x: i32, y: i32) -> bool` (`true` = dark; `(0,0)` is the top-left; **out-of-bounds coordinates return `false`**, so `get_module(x - 4, y - 4)` is a free quiet zone).
  - Capacity: version 40 at ECC Medium holds 2331 bytes in byte mode (ISO/IEC 18004), so 2331 lowercase ASCII chars fit and 2332 give `DataTooLong`. Lowercase letters are not in the alphanumeric set, so `"a".repeat(n)` is encoded in byte mode.
- `crates/mkrew-ui/src/theme.rs` — colors are plain `u32` `0xRRGGBB` (RGBA tokens such as `scrim` are `0xRRGGBBAA`); `pub struct Theme` (`:453-554`) has one `pub` field per token with a doc comment; `Theme::dark()` (`:600-662`) and `Theme::light()` (`:664-727`) list their own values and end with `..Self::shared()`; `Theme::shared()` (`:729-794`) holds values that are **the same in both appearances** (e.g. `scrim: 0x0000_0059` at `:754`, with a comment `// Black at ~35 %, in both appearances.`) and zero placeholders for the rest. The QR tokens are the same in both appearances, so they go in `shared()`, like `scrim`. `pub fn contrast(a: u32, b: u32) -> f32` (`:878-893`) is the existing WCAG helper used by the tests (`:990-1007`).
- **No color → string helper exists in `theme.rs`.** `crates/mkrew-ui/src/diagram.rs:69` has a private closure `let hex = |color: u32| format!("#{color:06X}");` used ~40 times in `colors()` (`:68-116`); its tests format `#{:06X}` by hand (`:132`, `:165`). The new `theme::hex` replaces that closure (clean cutover, one definition); `diagram.rs:15` already imports `crate::theme::{self, Theme}`.
- `crates/mkrew-ui/src/lib.rs:17-80` — `pub mod …;` list, alphabetical; `peek` is `:53`, `plan` is `:54`, so `phone` goes between them. The crate doc (`:1-15`) enumerates the headless parts; it ends `…voice\n//! input (microphone and speech to text), and\n//! the scroll bar of the Files editor and the Markdown preview.`
- How an SVG becomes an image in GPUI: `diagram.rs:52` `Image::from_bytes(ImageFormat::Svg, svg.into_bytes())`. GPUI rasterizes an SVG at its **intrinsic size × 2** (`gpui-pre-0.3.7/src/svg_renderer.rs:81,195-207`, `platform.rs:3034-3037`), and the element then scales that bitmap. A QR whose SVG is only `viewBox="0 0 53 53"` (no `width`/`height`) would be a 106 px bitmap stretched to ~224 px: soft module edges. So the SVG carries `width`/`height` = `side × MODULE_PX` (8 px per module, as Task 4 asked) plus `shape-rendering="crispEdges"`.
- Token rule to mirror: `crates/mkrew-cli/src/web/token.rs:13-17` `Token::parse` — exactly 64 chars, each `b'0'..=b'9' | b'a'..=b'f'`; callers trim first (`:43`). The file is `<config_dir>/web_token` (`crates/mkrew-cli/src/web.rs:46`). The CLI crate is a binary, so the UI cannot reuse `Token`; the 3-line rule is repeated.
- Test conventions: tests live in `#[cfg(test)] mod tests { use super::*; … }` at the bottom of the file; `tempfile::tempdir()` for files (`theme.rs:944`); `.unwrap()` is fine in tests.

**Files:**

- Modify: `crates/mkrew-ui/Cargo.toml` (+ `Cargo.lock`, written by cargo)
- Create: `crates/mkrew-ui/src/phone.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (register `pub mod phone;`, crate doc)
- Modify: `crates/mkrew-ui/src/theme.rs` (`qr_dark` / `qr_light` fields and values, `pub fn hex`, tests)
- Modify: `crates/mkrew-ui/src/diagram.rs` (use `theme::hex` instead of its private closure)
- Test: `crates/mkrew-ui/src/phone.rs` (`mod tests`), `crates/mkrew-ui/src/theme.rs` (`mod tests`)

**Interfaces:**

Consumes: nothing from the other tasks.

Produces (names are binding, Task 4 calls them):

```rust
// crates/mkrew-ui/src/phone.rs
pub fn link(host: &str, token: &str) -> String;                          // "https://{host}/#t={token}"
pub fn read_token(config_dir: &std::path::Path) -> Option<String>;       // <config_dir>/web_token, trimmed, 64 lowercase hex
pub fn qr_svg(text: &str, dark: &str, light: &str) -> Option<String>;    // None: text too long for any QR version

// crates/mkrew-ui/src/theme.rs
pub struct Theme { /* … */ pub qr_dark: u32, pub qr_light: u32 /* … */ }   // same in dark() and light()
pub fn hex(color: u32) -> String;                                        // 0xRRGGBB -> "#RRGGBB"
```

Task 4 builds the SVG with `phone::qr_svg(&text, &theme::hex(theme.qr_dark), &theme::hex(theme.qr_light))` and wraps the bytes in `Image::from_bytes(ImageFormat::Svg, svg.into_bytes())`.

- [ ] **Step 1: Add the dependency**

In `crates/mkrew-ui/Cargo.toml`, after the `pulldown-cmark` line (`:38`) and before `serde` (`:39`), insert:

```toml
# QR code of the phone link (spec 2026-10-12-phone-access-ui-design.md): pure Rust, no
# dependencies; `phone.rs` draws the matrix as an SVG.
qrcodegen = "1.8.0"
```

Run (downloads `qrcodegen v1.8.0` and writes the lock entry; this is a dependency download, not a test):

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo check -p mkrew-ui --lib -j 4
```

Expected: `Downloaded qrcodegen v1.8.0`, then `Finished`. `git diff Cargo.lock` shows a new `[[package]] name = "qrcodegen" version = "1.8.0"` block (with `source` and `checksum`) and `"qrcodegen",` added to the `mkrew-ui` dependency list.

- [ ] **Step 2: Write the failing theme tests**

In `crates/mkrew-ui/src/theme.rs`, inside `mod tests`, directly after `body_text_contrast_is_at_least_4_5` (ends `:1007`, before `mode_cycles_system_light_dark`), insert:

```rust
    #[test]
    fn hex_is_a_padded_css_color() {
        assert_eq!(hex(0x17181C), "#17181C");
        assert_eq!(hex(0x00000A), "#00000A", "leading zeros stay");
        assert_eq!(hex(0xFFFFFF), "#FFFFFF");
    }

    #[test]
    fn the_qr_code_is_dark_on_light_in_both_appearances() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_eq!(
            (dark.qr_dark, dark.qr_light),
            (light.qr_dark, light.qr_light),
            "phone cameras read dark modules on a light panel in either theme"
        );
        assert!(
            contrast(light.qr_dark, light.qr_light) >= 7.0,
            "modules stand out from the panel"
        );
        assert!(
            contrast(light.qr_light, 0xFFFFFF) < contrast(light.qr_dark, 0xFFFFFF),
            "the panel is the light one, the modules the dark one (not inverted)"
        );
    }
```

- [ ] **Step 3: Run the theme tests, expect a compile failure**

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib -j 4 -- theme::tests::hex theme::tests::the_qr_code
```

Expected: `error[E0425]: cannot find function `hex` in this scope` and `error[E0609]: no field `qr_dark` on type `Theme`` (and `qr_light`).

- [ ] **Step 4: Add the tokens and `hex`**

Edit `crates/mkrew-ui/src/theme.rs` (line numbers are those before this step; apply bottom-up so they stay valid):

1. After `contrast` (the `}` at `:893`, before the blank line and `#[cfg(test)]`), insert:

```rust

/// `0xRRGGBB` as the CSS / SVG color `#RRGGBB` (diagram and QR code SVGs).
pub fn hex(color: u32) -> String {
    format!("#{color:06X}")
}
```

2. In `Theme::shared()`, after `scrim: 0x0000_0059,` (`:754`), insert:

```rust
            // The QR code of Phone access: near-black modules on white, in both appearances.
            qr_dark: 0x101114,
            qr_light: 0xFFFFFF,
```

3. In `pub struct Theme`, after `pub scrim: u32,` (`:496`), insert:

```rust
    /// The QR code of Phone access (SPEC §14): dark modules on a light panel in both
    /// appearances, because phone cameras read dark-on-light only; so both are set in
    /// `shared()`, not in `dark()` / `light()`.
    pub qr_dark: u32,
    pub qr_light: u32,
```

- [ ] **Step 5: Make `diagram.rs` use `theme::hex`**

In `crates/mkrew-ui/src/diagram.rs`: change the import at `:15` to

```rust
use crate::theme::{self, Theme, hex};
```

and delete the private closure at `:69`:

```rust
    let hex = |color: u32| format!("#{color:06X}");
```

(All ~40 `hex(theme.…)` calls in `colors()` now call `theme::hex`, same output. The test helper `fn parse(hex: &str)` at `:172` has a parameter named `hex`, which just shadows the import inside that function.)

- [ ] **Step 6: Run the theme and diagram tests, expect pass**

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib -j 4 -- theme:: diagram::
```

Expected: all `theme::tests::*` and `diagram::tests::*` pass, including `hex_is_a_padded_css_color` and `the_qr_code_is_dark_on_light_in_both_appearances` (diagram tests unchanged and green: they format the same `#{:06X}`).

- [ ] **Step 7: Write the failing `phone.rs` tests and register the module**

In `crates/mkrew-ui/src/lib.rs`, after `pub mod peek;` (`:53`) insert `pub mod phone;`, and change the crate doc lines `:14-15` from

```rust
//! input (microphone and speech to text), and
//! the scroll bar of the Files editor and the Markdown preview.
```

to

```rust
//! input (microphone and speech to text), the phone-access link, token and QR code, and
//! the scroll bar of the Files editor and the Markdown preview.
```

Create `crates/mkrew-ui/src/phone.rs` with only the tests for now (the implementation goes above them in Step 9):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// A config folder, with a `web_token` file holding `token_file` when given.
    fn config_with(token_file: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = token_file {
            std::fs::write(dir.path().join("web_token"), text).unwrap();
        }
        dir
    }

    /// ~110 chars, like the real link of a Tailscale name.
    fn typical_link() -> String {
        link("desktop-nguyentung.tail1234.ts.net", TOKEN)
    }

    /// The dark cells the SVG's path draws, as a `side` × `side` grid (quiet zone included).
    fn drawn_cells(svg: &str, side: usize) -> Vec<Vec<bool>> {
        let path = svg
            .split("<path d=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a path");
        let mut grid = vec![vec![false; side]; side];
        for run in path.split('M').filter(|run| !run.is_empty()) {
            // One run of dark cells in a row: "x,yhWv1h-Wz".
            let (x, rest) = run.split_once(',').unwrap();
            let (y, rest) = rest.split_once('h').unwrap();
            let (width, rest) = rest.split_once('v').unwrap();
            assert_eq!(rest, format!("1h-{width}z"), "run {run}");
            let (x, y, width): (usize, usize, usize) = (
                x.parse().unwrap(),
                y.parse().unwrap(),
                width.parse().unwrap(),
            );
            for cell in &mut grid[y][x..x + width] {
                assert!(!*cell, "cell drawn twice in run {run}");
                *cell = true;
            }
        }
        grid
    }

    #[test]
    fn the_link_carries_the_token_in_the_fragment() {
        assert_eq!(
            link("pc.tail1234.ts.net", TOKEN),
            format!("https://pc.tail1234.ts.net/#t={TOKEN}")
        );
    }

    #[test]
    fn read_token_trims_and_accepts_only_64_lowercase_hex() {
        let read = |text: &str| read_token(config_with(Some(text)).path());
        assert_eq!(read(TOKEN).as_deref(), Some(TOKEN));
        assert_eq!(
            read(&format!("{TOKEN}\r\n")).as_deref(),
            Some(TOKEN),
            "an editor or `echo` adds a newline"
        );
        assert_eq!(read(&format!("  {TOKEN}\n")).as_deref(), Some(TOKEN));
        assert_eq!(read(&TOKEN.to_uppercase()), None, "mk web writes lowercase");
        assert_eq!(read(&TOKEN[..63]), None, "too short");
        assert_eq!(read(&format!("{TOKEN}0")), None, "too long");
        assert_eq!(read(&format!("g{}", &TOKEN[1..])), None, "not hex");
        assert_eq!(read(""), None);
        assert_eq!(
            read(&format!("{TOKEN}\n{TOKEN}\n")),
            None,
            "two tokens are not one"
        );
    }

    #[test]
    fn read_token_is_none_before_the_first_start() {
        let dir = config_with(None);
        assert_eq!(read_token(dir.path()), None, "no web_token file yet");
        assert_eq!(
            read_token(&dir.path().join("no-such-folder")),
            None,
            "no config folder yet"
        );
    }

    #[test]
    fn the_svg_draws_the_symbol_inside_a_four_module_quiet_zone() {
        let text = typical_link();
        let svg = qr_svg(&text, "#101114", "#ffffff").expect("a link fits a QR code");
        let qr = QrCode::encode_text(&text, QrCodeEcc::Medium).unwrap();
        let side = qr.size() + 8;
        assert!(svg.starts_with("<svg "), "{svg}");
        assert!(
            svg.contains(&format!("viewBox=\"0 0 {side} {side}\"")),
            "symbol + 4 modules each side: {side}"
        );
        assert!(
            svg.contains(&format!("width=\"{0}\" height=\"{0}\"", side * 8)),
            "8 px per module: GPUI rasterizes at twice the intrinsic size"
        );
        let cells = drawn_cells(&svg, side as usize);
        for y in 0..side {
            for x in 0..side {
                assert_eq!(
                    cells[y as usize][x as usize],
                    qr.get_module(x - 4, y - 4),
                    "module {x},{y} (quiet zone is light, finder patterns are where the spec puts them)"
                );
            }
        }
    }

    #[test]
    fn the_svg_is_dark_modules_on_a_light_background() {
        let svg = qr_svg(&typical_link(), "#101114", "#ffffff").unwrap();
        let element = |tag: &str| {
            svg.split(tag)
                .nth(1)
                .and_then(|rest| rest.split("/>").next())
                .unwrap_or_default()
                .to_owned()
        };
        let (background, modules) = (element("<rect "), element("<path "));
        assert!(background.contains("#ffffff"), "{background}");
        assert!(!background.contains("#101114"), "{background}");
        assert!(modules.contains("#101114"), "{modules}");
        assert!(!modules.contains("#ffffff"), "{modules}");
        assert!(svg.contains("crispEdges"), "no blurred module edges");
    }

    #[test]
    fn the_svg_is_none_when_the_text_does_not_fit_any_version() {
        // Version 40 at ECC Medium holds 2331 bytes; lowercase ASCII is encoded as bytes.
        assert!(qr_svg(&"a".repeat(2331), "#000000", "#ffffff").is_some());
        assert_eq!(qr_svg(&"a".repeat(2332), "#000000", "#ffffff"), None);
    }
}
```

- [ ] **Step 8: Run the `phone` tests, expect a compile failure**

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib -j 4 -- phone::
```

Expected: `error[E0425]: cannot find function `link` in this scope`, the same for `read_token` and `qr_svg`, and `error[E0433]`/`E0412` for `QrCode` / `QrCodeEcc` (not imported yet).

- [ ] **Step 9: Implement `phone.rs`**

Insert at the very top of `crates/mkrew-ui/src/phone.rs`, above `#[cfg(test)]`:

```rust
//! Phone access, the pure pieces (spec 2026-10-12-phone-access-ui-design.md): the link the
//! phone opens, the access token `mk web` stored, and the QR code of the link as an SVG.
//! No GPUI here; the Phone access dialog shows the SVG with `gpui::img`. The link carries the
//! token: never log it.

use std::fmt::Write as _;
use std::path::Path;

use qrcodegen::{QrCode, QrCodeEcc};

/// Light border around the symbol, in modules (ISO/IEC 18004 asks for 4).
const QUIET_ZONE: i32 = 4;

/// Pixels per module in the SVG's own size. GPUI rasterizes an SVG at that size × 2 and then
/// scales the bitmap, so 8 px per module gives 16 bitmap px per module, crisp when shown smaller.
const MODULE_PX: i32 = 8;

/// What the phone opens: the token travels in the fragment, which the browser never sends.
pub fn link(host: &str, token: &str) -> String {
    format!("https://{host}/#t={token}")
}

/// The token `mk web` keeps in `<config_dir>/web_token`; `None` before its first start or when
/// the file does not hold a token (exactly 64 lowercase hex characters, like `mk web`'s rule).
pub fn read_token(config_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(config_dir.join("web_token")).ok()?;
    let token = text.trim();
    let valid = token.len() == 64 && token.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    valid.then(|| token.to_owned())
}

/// `text` as a QR code (ECC Medium) drawn as an SVG: `dark` modules on a `light` panel, both
/// `#rrggbb` ([`crate::theme::hex`] of `qr_dark` / `qr_light`). `None` when the text is too
/// long for the largest QR code.
pub fn qr_svg(text: &str, dark: &str, light: &str) -> Option<String> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    let size = qr.size();
    let side = size + 2 * QUIET_ZONE;
    // One rectangle per run of dark modules in a row: far fewer elements than one per module.
    let mut path = String::new();
    for y in 0..size {
        let mut x = 0;
        while x < size {
            if !qr.get_module(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < size && qr.get_module(x, y) {
                x += 1;
            }
            let width = x - start;
            // Writing to a String cannot fail.
            let _ = write!(
                path,
                "M{},{}h{width}v1h-{width}z",
                start + QUIET_ZONE,
                y + QUIET_ZONE
            );
        }
    }
    let pixels = side * MODULE_PX;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{pixels}\" height=\"{pixels}\" \
         viewBox=\"0 0 {side} {side}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{side}\" height=\"{side}\" fill=\"{light}\"/>\
         <path d=\"{path}\" fill=\"{dark}\"/></svg>"
    ))
}

```

(The file now continues with the `#[cfg(test)] mod tests` from Step 7.)

- [ ] **Step 10: Run the `phone` tests, expect pass**

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib -j 4 -- phone::
```

Expected: `6 passed` (`the_link_carries_the_token_in_the_fragment`, `read_token_trims_and_accepts_only_64_lowercase_hex`, `read_token_is_none_before_the_first_start`, `the_svg_draws_the_symbol_inside_a_four_module_quiet_zone`, `the_svg_is_dark_modules_on_a_light_background`, `the_svg_is_none_when_the_text_does_not_fit_any_version`).

If `the_svg_is_none_when_…` fails on the 2331 / 2332 boundary, the ECC level is not `Medium` (Low would accept 2332): fix the code, not the numbers.

- [ ] **Step 11: Format, verify, commit**

```
cargo fmt --all
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target sh scripts/verify.sh
```

(`cargo fmt` also settles the import order in `diagram.rs` and the `write!` layout. `scripts/verify.sh` runs `cargo fmt --check`, `clippy --workspace --all-targets -D warnings` and `cargo test --workspace`; use `CARGO_BUILD_JOBS=4` in the environment so the `-j 4` limit also applies there.) Expected: `verify: OK`.

```
git add crates/mkrew-ui/Cargo.toml Cargo.lock crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/phone.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/diagram.rs
git commit -m "feat(ui): phone link, token reader and QR code SVG for phone access"
```

**Contract notes:**

- `qr_dark` / `qr_light` are set once in `Theme::shared()` (next to `scrim`), not repeated in `dark()` and `light()`; the contract's "same value in dark and light" holds and cannot drift. SPEC §14's token table entry is Task 4's doc work.
- `theme::hex(color: u32) -> String` is new (uppercase `#RRGGBB`, matching what `diagram.rs` already produced) and `diagram.rs`'s private `hex` closure is deleted in favor of it (clean cutover, one definition). This touches `diagram.rs`, which the contract does not list.
- The SVG has `width`/`height` attributes (`side × 8`) in addition to the `viewBox`, because GPUI rasterizes SVG images at their intrinsic size × 2; without them the QR would be a blurry upscale. The `viewBox` is still exactly `0 0 {size+8} {size+8}`. Task 4 should give the `gpui::img` element a fixed square size with the default fit.
- qrcodegen 1.8.0's `encode_text` returns `Result<_, DataTooLong>`, not the `Option` of 1.3; `qr_svg` maps both failure kinds to `None` as the contract says (the dialog shows no QR then).
- `Cargo.lock` is committed with this task (new `qrcodegen` entry only).

---

### Task 4: UI dialog, title-bar icon, palette command, background polling, docs and smoke

**Current state (investigated):**
- Dialog pattern: `crates/mkrew-ui/src/shell/history.rs:61-113` (`open_turns`: `close`/clear other dialogs, `dialog_serial += 1`, `window.focus(&self.dialog_focus, cx)`, `cx.notify()`, `connection.call::<T>(Request::…)` then `cx.spawn_in(window, async move |this, cx| { … this.update(cx, |shell, cx| …) })`); close = `focus_pane(window, cx)`. A dialog with no text field and Esc-only keys: `shell/git_dialogs.rs:25-41` (`git_discard_key_down` → `cx.stop_propagation()` + `undo_dialog::cancels(&event.keystroke)`, `undo_dialog.rs:283`). `base_dialog::key` (the contract's example) is the Git base menu's text-field key map (Enter saves, edits text): wrong for this dialog, so `undo_dialog::cancels` is used.
- Dialog render: `shell/git_dialogs.rs:372-474` (`render_git_base`): `div().id(..).w(px(theme::DIALOG_WIDTH)).flex().flex_col().gap(px(space::M)).p(px(space::XL)).rounded(px(radius::LARGE)).border(px(theme::BORDER_WIDTH)).border_color(rgb(theme.border)).bg(rgb(theme.surface)).shadow_md().font_family(theme.ui_font_family.clone()).track_focus(&self.dialog_focus).on_key_down(cx.listener(..))`, title `font_size::CARD_TITLE` + `weight::SEMIBOLD`, error row = `✕` in `theme.status_exited` + text in `theme.text`, footer `justify_end` buttons, wrapped by `Self::modal_shade(id, panel, cx)` (`shell/focus.rs:190`). Rendered from `main.rs:674-678` (`.children(self.git_base.as_ref().map(..))`); `Shell::modal_open` (`shell/focus.rs:7-22`) lists every open dialog and the title bar's controls go inert while it is true (`chrome.rs:82,128,164`).
- Widgets (`shell/widgets.rs`): `button(id, label, theme, primary)` :100, `disabled_button` :161, `meta_line(theme, SharedString)` :511, `tooltip(theme, text, keys)` :799, `icon_button(id, Icon, theme)` :260-278 (icon always `theme.text_muted`; no way to recolor), `icon_svg(Icon, size, color)` (`icons.rs:113`). SVG bytes → image: `widgets.rs:720-737` (`Arc<gpui::Image>` from `gpui::Image::from_bytes(gpui::ImageFormat::Svg, bytes)`, `gpui::img(Arc::clone(..)).size(theme::rems_px(..))`; default `ObjectFit::Contain`). gpui-pre 0.3.7 rasterizes an SVG image at 2× its intrinsic `width`/`height` (`svg_renderer.rs:81,200-207`, `platform.rs:3034-3037`) — Task 3's `qr_svg` therefore sets `width`/`height` = 8 px per module so the ~216 px QR is crisp.
- Clipboard: `cx.write_to_clipboard(ClipboardItem::new_string(text))` (`shell/files.rs:636`, `ClipboardItem` is in `use crate::*`).
- Title bar: `shell/chrome.rs:131-147` builds `right` (theme toggle `icon_button("toggle-theme", …)` closes with `);` at :147), then `if let Some(project_id) = workspace` adds History / New / Run (:148-163); `right.when(modal, …inert_layer())` at :164.
- Icons (ADR 0051, lucide-static **1.55.0**, ISC, license in `icon/lucide/LICENSE`): 48 SVGs in `icon/lucide/`, one `Variant => "file-stem",` per icon in the `icons!` list (`icons.rs:40-89`, alphabetical: `Share2` :77, `Sparkles` :78), embedded by `include_bytes!("../../../icon/lucide/<stem>.svg")`; test `every_icon_is_served_and_is_a_lucide_svg` (`icons.rs:127`) covers every new variant. `https://unpkg.com/lucide-static@1.55.0/icons/smartphone.svg` exists (`<rect width="14" height="20" x="5" y="2" rx="2" ry="2" />` + `<path d="M12 18h.01" />`, same header as `play.svg`). ADR 0051 :14 says "48 SVGs".
- Palette: `palette.rs:53-69` `enum Extra`, `:72-88` `ALL: [Self; 15]`, `:90-108` `title`, `:111-137` `available` (`OpenSettings => true`). Dispatch `shell/navigation.rs:479` (`Extra::OpenSettings => self.open_settings(cx)` before the workspace-required arm) and `:544` (`Extra::OpenSettings` in the inner match, comment "Handled above: it needs no workspace"). Tests `palette/tests.rs:301-311` (`extras()` filters `RunSetup | OpenSettings` out because they are always listed), `:292` (`extras(&none) == []`), `:378-383` (titles unique, none equals a command title).
- Config / settings paths: `Shell::new(launched, paths: Paths, …)` (`shell/connection.rs:6`) keeps `settings_file: PathBuf` (`main.rs:348`, `paths.settings_file()`), not `config_dir`; `paths` is moved into the connect task at `connection.rs:126-130`, so a clone must be taken before (:24 area). `Paths { data_dir, config_dir, socket }` (`mkrew-core/src/paths.rs:16`); `web_token` lives in `config_dir` (`mk web`). The UI writes settings inline on the UI thread (`write_ui_value` calls in `shell/workspace_actions.rs:403-454`); `SettingsError` implements `Display` (`.to_string()`).
- Connection: `Rc<Connection>` in `Shell.connection: Option<..>`; `connection.call::<T>(Request)` returns a `Send + 'static` future (`client.rs:187-193`). Nothing in the UI calls `Request::DaemonStatus` yet. Disconnect reset list: `connection.rs:182-229`; connect hook `connection.rs:147-150` (`shell.connection = Some(connection); shell.refresh(..)`).
- Background work: `cx.background_executor().spawn(async move { blocking })` for blocking calls (`history.rs:44-54`, `connection.rs:128-131`); periodic loop = `cx.spawn_in(window, async move |this, cx| loop { cx.background_executor().timer(..).await; this.update_in(cx, …) })` (`connection.rs:47-109`, `theme::STATUS_TICK`, `theme.rs:67`).
- **No toast mechanism exists**: `theme::TOAST_DURATION` (`theme.rs:63`) is unused and `grep -ri toast crates/mkrew-ui/src` finds nothing else. Existing dialogs show failures as an inline error row inside the dialog. This task does the same (contract note).
- Theme tokens used below (all exist): `theme.{bg,surface,surface_raised,text,text_muted,accent,status_exited,border}`, `space::{XS,S,M,XL}`, `radius::{MEDIUM,LARGE}`, `font_size::{META,BODY,CARD_TITLE}`, `weight::SEMIBOLD`, `theme::{DIALOG_WIDTH 520, BORDER_WIDTH, ICON_SIZE, rems_px}`. From Task 3: `theme::hex(u32) -> String`, `Theme.qr_dark` (0x101114) / `Theme.qr_light` (0xFFFFFF) set in `Theme::shared()`, `phone::{link, read_token, qr_svg}`. From Task 1: `mkrew_core::settings::write_web_enabled`, `mkrew_core::tailscale::{WEB_PORT, ServeCheck, dns_name, serve_status, serve_setup}`, `mkrew_protocol::{WebStatus, DaemonStatusResult.web}`.
- Docs anchors (verified): SPEC `docs/SPEC.md` :192 (§4 tree, `mkrew-core … NO I/O beyond files.`), :558 (`daemon.status/stop` row), :609 (end of §7), :753-754 (title bar bullet), :1249 (last token row `diff-hunk-header`), :1276-1277 (end of the Icons bullet / `- Density`), :1313 (§15 Security), :1617-1619 (`[features]` in the §20.0 template), :1620-1622 (end of the template / `**Turn.**`). `README.md` :175-192 (`## Use Mkrew from your phone`, steps 1-4 and the closing paragraph). `docs/HUMAN_CHECKS.md` :18 (newest `### To verify — 2026-10-12 Mkrew web on a phone …` block). `docs/PROGRESS.md` :31-33 (`## Notes for the next session`, first bullet = Mkrew web). ADRs 0051 and 0052 exist; 0053 is free.

**Files:**
- Create: `icon/lucide/smartphone.svg`, `crates/mkrew-ui/src/phone_dialog.rs`, `crates/mkrew-ui/src/shell/phone_access.rs`, `docs/decisions/0053-phone-access-ui.md`
- Create (if the screenshot works): `docs/screenshots/2026-10-12-phone-access-dark.png`, `docs/screenshots/2026-10-12-phone-access-light.png`
- Modify: `crates/mkrew-ui/src/icons.rs`, `crates/mkrew-ui/src/lib.rs`, `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/theme.rs`, `crates/mkrew-ui/src/main.rs`, `crates/mkrew-ui/src/shell.rs`, `crates/mkrew-ui/src/shell/chrome.rs`, `crates/mkrew-ui/src/shell/connection.rs`, `crates/mkrew-ui/src/shell/focus.rs`, `crates/mkrew-ui/src/shell/navigation.rs`, `crates/mkrew-ui/src/shell/widgets.rs`
- Modify (docs): `docs/SPEC.md`, `docs/decisions/0051-lucide-icons.md` (count 48 → 49), `README.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`
- Test: unit tests in `phone_dialog.rs` (model, rows, polling bookkeeping, link/QR cache) and `palette/tests.rs`; `icons.rs` test covers the new icon; smoke in Step 12.

**Interfaces:**
- Consumes (names exactly as in the contract and Task 1/3 results): `mkrew_core::settings::write_web_enabled(&Path, bool) -> Result<(), SettingsError>`; `mkrew_core::tailscale::{WEB_PORT: u16, ServeCheck {Ready, Missing}, dns_name() -> Option<String>, serve_status() -> Result<ServeCheck, String>, serve_setup(u16) -> Result<(), String>}` (blocking; `serve_setup` has a 30 s kill timeout; a logged-out Tailscale reads as `Missing` from `serve_status`, so "Unavailable" is also derived from `dns_name() == None`); `mkrew_protocol::{WebStatus {enabled, running, error}, DaemonStatusResult {…, web}}`; `crate::phone::{link(&str,&str)->String, read_token(&Path)->Option<String>, qr_svg(&str,&str,&str)->Option<String>}`; `crate::theme::hex(u32)->String`, `Theme.qr_dark/qr_light`.
- Produces (crate-internal unless `pub` in the lib):
  - lib `mkrew_ui::phone_dialog` (pure, tested): `Daemon {Loading, Known(WebStatus), Failed(String)}`, `Serve {Checking, Ready, Missing, Unavailable(Option<String>)}`, `Setup {Idle, Running, Failed(String)}`, `Mark`, `Switch {Start, Stop}`, `StatusRow`, `ServeRow`, `SetUp {Ready, Busy}`, `LinkBlock {Hidden, Starting, Ready}`, `Link {text, image}`, `Local {serve, host, token}` with blocking `Local::read(&Path)`, `Phone` (`Default`, `running()`, `pending()`, `opened()`, `due(now, open)`, `begin_poll(now)`, `polled(web, local, now, colors)`, `written(enabled, now)`, `write_failed(msg)`, `begin_setup()`, `setup_done(result)`, `link()`, `link_block()`, `copy_link()`), fns `status_row`, `serve_row`, `serve_state`.
  - `Shell` (bin): fields `config_dir: PathBuf`, `phone: Phone`, `phone_open: bool`; methods `open_phone_access`, `close_phone_access`, `phone_access_key_down`, `poll_phone`, `set_phone_enabled`, `set_up_tailscale`, `copy_phone_link`, `render_phone_access`, `render_phone_button`.
  - `Extra::PhoneAccess` (title `Global: Phone access…`), `Icon::Smartphone`, `widgets::icon_button_in(id, glyph, color, theme)`, `theme::{PHONE_QR_SIZE, PHONE_TICK, PHONE_POLL_IDLE}`.

---

- [ ] **Step 1: Vendor the Lucide `smartphone` icon and register it**

Run (from the worktree root; the same URL pattern ADR 0051 documents):

```
curl -fsSL https://unpkg.com/lucide-static@1.55.0/icons/smartphone.svg -o icon/lucide/smartphone.svg
```

`icon/lucide/smartphone.svg` must read exactly:

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-smartphone"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <rect width="14" height="20" x="5" y="2" rx="2" ry="2" />
  <path d="M12 18h.01" />
</svg>
```

(If `curl` is unavailable, write the file with that content: it is byte-for-byte what lucide-static 1.55.0 serves.) In `crates/mkrew-ui/src/icons.rs`, in the `icons!` list, after `Share2 => "share-2",` (line 77) add:

```rust
    Smartphone => "smartphone",
```

In `docs/decisions/0051-lucide-icons.md` line 14 change `48 SVGs vendored in` to `49 SVGs vendored in`.

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib icons -j 4`
Expected: `every_icon_is_served_and_is_a_lucide_svg` passes (it now also covers `Smartphone`; a missing or non-24 px file would fail it).

- [ ] **Step 2: Failing palette test**

In `crates/mkrew-ui/src/palette/tests.rs` change the `extras` helper filter (line 305) from
`Action::Extra(extra) if !matches!(extra, Extra::RunSetup | Extra::OpenSettings) => {`
to
`Action::Extra(extra) if !matches!(extra, Extra::RunSetup | Extra::OpenSettings | Extra::PhoneAccess) => {`
and add, after the test that ends at line 299 (`availability_follows_the_view`):

```rust
#[test]
fn phone_access_is_listed_in_every_view_and_without_a_workspace() {
    for view in [View::Terminals, View::Board, View::Docs, View::Files, View::Git] {
        assert!(Extra::PhoneAccess.available(&context(view)), "{view:?}");
    }
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::PhoneAccess))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Global: Phone access…");
    assert_eq!(entry.shortcut, "", "palette-only: no key is bound");
}
```

Run: `CARGO_TARGET_DIR=… cargo test -p mkrew-ui --lib palette -j 4`
Expected: compile error `no variant or associated item named `PhoneAccess` found for enum `Extra``.

- [ ] **Step 3: `Extra::PhoneAccess`**

In `crates/mkrew-ui/src/palette.rs`: add the variant after `OpenSettings,` (line 68): `PhoneAccess,`; change `pub const ALL: [Self; 15]` to `[Self; 16]` and append `Self::PhoneAccess,` after `Self::OpenSettings,` (line 87); in `title` add after the `OpenSettings` arm (line 106): `Self::PhoneAccess => "Global: Phone access…",`; in `available` change `Self::OpenSettings => true,` (line 135) to `Self::OpenSettings | Self::PhoneAccess => true,`.

In `crates/mkrew-ui/src/shell/navigation.rs` add after line 479 (`palette::Action::Extra(Extra::OpenSettings) => self.open_settings(cx),`):

```rust
            palette::Action::Extra(Extra::PhoneAccess) => self.open_phone_access(window, cx),
```

and in the inner match, after `Extra::OpenSettings => self.open_settings(cx),` (line 544) add:

```rust
                    Extra::PhoneAccess => self.open_phone_access(window, cx),
```

(`open_phone_access` is written in Step 6; the crate compiles again at Step 8.) The first arm handles it with no workspace; the inner arm only keeps the match exhaustive, like `OpenSettings`.

- [ ] **Step 4: Failing model tests**

Add `pub mod phone_dialog;` to `crates/mkrew-ui/src/lib.rs` right after `pub mod phone;` (Task 3; alphabetical, before `pub mod plan;`). Create `crates/mkrew-ui/src/phone_dialog.rs` with only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: (&str, &str) = ("#101114", "#FFFFFF");

    fn web(enabled: bool, running: bool, error: Option<&str>) -> WebStatus {
        WebStatus {
            enabled,
            running,
            error: error.map(str::to_owned),
        }
    }

    fn known(enabled: bool, running: bool, error: Option<&str>) -> Daemon {
        Daemon::Known(web(enabled, running, error))
    }

    fn token(pair: &str) -> String {
        pair.repeat(32)
    }

    fn local(host: Option<&str>, token: Option<String>) -> Local {
        Local {
            serve: serve_state(Ok(ServeCheck::Ready), host),
            host: host.map(str::to_owned),
            token,
        }
    }

    /// A poll while the dialog is open: `mk web` runs, Tailscale answered.
    fn polled_on(phone: &mut Phone, now: Instant, host: Option<&str>, token: Option<String>) {
        phone.polled(Ok(web(true, true, None)), Some(local(host, token)), now, COLORS);
    }

    #[test]
    fn the_status_row_follows_the_daemon() {
        let row = status_row(&known(false, false, None), None);
        assert_eq!((row.mark, row.text.as_str(), row.switch), (Mark::Off, "Off", Some(Switch::Start)));
        let row = status_row(&known(true, true, None), None);
        assert_eq!((row.mark, row.text.as_str(), row.switch), (Mark::On, "On", Some(Switch::Stop)));
        // The daemon restarts a failed child with backoff: Stop stays, the reason shows, and an
        // error that already ends with a period does not get a second one.
        let row = status_row(&known(true, false, Some("address already in use.")), None);
        assert_eq!(row.mark, Mark::Down);
        assert_eq!(row.text, "Not running: address already in use. Retrying…");
        assert_eq!(row.switch, Some(Switch::Stop));
        // Enabled but no error yet: the daemon is still starting the child.
        let row = status_row(&known(true, false, None), None);
        assert_eq!((row.text.as_str(), row.switch), ("Starting…", Some(Switch::Stop)));
        let row = status_row(&Daemon::Loading, None);
        assert_eq!((row.text.as_str(), row.switch), ("Checking…", None));
        let row = status_row(&Daemon::Failed("closed".into()), None);
        assert_eq!(row.mark, Mark::Down);
        assert_eq!(row.text, "Cannot read the daemon status: closed");
        assert_eq!(row.switch, None);
    }

    #[test]
    fn a_click_shows_progress_without_a_button_until_the_daemon_agrees() {
        let row = status_row(&known(false, false, None), Some(true));
        assert_eq!((row.mark, row.text.as_str(), row.switch), (Mark::Wait, "Starting…", None));
        let row = status_row(&known(true, true, None), Some(false));
        assert_eq!((row.mark, row.text.as_str(), row.switch), (Mark::Wait, "Stopping…", None));
    }

    #[test]
    fn the_wait_ends_when_the_daemon_agrees_or_after_five_seconds() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        phone.written(true, t0);
        assert_eq!(phone.pending(), Some(true));
        // The daemon has not re-read settings.toml yet.
        phone.polled(Ok(web(false, false, None)), None, t0 + Duration::from_secs(1), COLORS);
        assert_eq!(phone.pending(), Some(true));
        phone.polled(Ok(web(true, false, None)), None, t0 + Duration::from_secs(2), COLORS);
        assert_eq!(phone.pending(), None);
        // A daemon that never agrees (settings.toml it cannot read) does not leave the row stuck.
        phone.written(false, t0);
        phone.polled(Ok(web(true, true, None)), None, t0 + Duration::from_secs(4), COLORS);
        assert_eq!(phone.pending(), Some(false));
        phone.polled(Ok(web(true, true, None)), None, t0 + Duration::from_secs(5), COLORS);
        assert_eq!(phone.pending(), None);
    }

    #[test]
    fn a_failed_status_call_says_why_and_the_next_one_recovers() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(!phone.running());
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.running());
        phone.polled(Err("pipe closed".into()), None, t0, COLORS);
        assert!(!phone.running(), "no icon accent for a status that could not be read");
        assert_eq!(phone.daemon, Daemon::Failed("pipe closed".into()));
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.running());
    }

    #[test]
    fn the_link_shows_only_while_on_with_a_name() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
        // On, a name, but `mk web` has not written web_token yet (first start).
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), None);
        assert_eq!(phone.link_block(), LinkBlock::Starting);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert_eq!(phone.link_block(), LinkBlock::Ready);
        assert_eq!(
            phone.link().map(|link| link.text.clone()),
            Some(format!("https://pc.tail1.ts.net/#t={}", token("a1")))
        );
        // No MagicDNS name: nothing to link to.
        polled_on(&mut phone, t0, None, Some(token("a1")));
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
        assert!(phone.link().is_none());
        // Off: the link is hidden even with a name and a token.
        phone.polled(
            Ok(web(false, false, None)),
            Some(local(Some("pc.tail1.ts.net"), Some(token("a1")))),
            t0,
            COLORS,
        );
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
    }

    #[test]
    fn the_qr_is_rebuilt_only_when_the_link_changes() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        let first = phone.link().and_then(|link| link.image.clone()).expect("a QR for a normal link");
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        let same = phone.link().and_then(|link| link.image.clone()).expect("still there");
        assert!(Arc::ptr_eq(&first, &same), "a poll every second must not re-render the QR");
        // `mk web --new-token`: the old QR must not stay on screen.
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("b2")));
        let next = phone.link().expect("link");
        assert_eq!(next.text, format!("https://pc.tail1.ts.net/#t={}", token("b2")));
        assert!(!Arc::ptr_eq(&first, next.image.as_ref().expect("QR")));
        // The token file goes away: no link, no stale QR.
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), None);
        assert!(phone.link().is_none());
    }

    #[test]
    fn a_poll_without_tailscale_data_keeps_what_the_dialog_knew() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        // Dialog closed: only `daemon.status` is read.
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert_eq!(phone.serve, Serve::Ready);
        assert!(phone.link().is_some());
    }

    #[test]
    fn copying_returns_the_link_and_the_label_resets_on_the_next_poll() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert_eq!(phone.copy_link(), None);
        assert!(!phone.copied);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert_eq!(phone.copy_link(), Some(format!("https://pc.tail1.ts.net/#t={}", token("a1"))));
        assert!(phone.copied);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert!(!phone.copied);
    }

    #[test]
    fn serve_state_tells_missing_from_unavailable() {
        assert_eq!(serve_state(Ok(ServeCheck::Ready), Some("pc.ts.net")), Serve::Ready);
        assert_eq!(serve_state(Ok(ServeCheck::Missing), Some("pc.ts.net")), Serve::Missing);
        // Logged out: `tailscale serve status` answers, but there is no MagicDNS name.
        assert_eq!(serve_state(Ok(ServeCheck::Missing), None), Serve::Unavailable(None));
        assert_eq!(serve_state(Ok(ServeCheck::Ready), None), Serve::Unavailable(None));
        assert_eq!(
            serve_state(Err("program not found".into()), None),
            Serve::Unavailable(Some("program not found".into()))
        );
    }

    #[test]
    fn the_tailscale_row_offers_set_up_only_when_it_can_work() {
        let row = serve_row(&Serve::Checking, &Setup::Idle);
        assert_eq!((row.text, row.button), ("Checking Tailscale…", None));
        let row = serve_row(&Serve::Ready, &Setup::Idle);
        assert_eq!((row.mark, row.text, row.button), (Mark::Done, "Tailscale serve is set up", None));
        let row = serve_row(&Serve::Missing, &Setup::Idle);
        assert_eq!(row.text, "Tailscale doesn't publish Mkrew yet");
        assert_eq!((row.button, row.error), (Some(SetUp::Ready), None));
        let row = serve_row(&Serve::Missing, &Setup::Running);
        assert_eq!(row.button, Some(SetUp::Busy));
        let row = serve_row(&Serve::Missing, &Setup::Failed("access denied".into()));
        assert_eq!(row.button, Some(SetUp::Ready), "Set up can be pressed again");
        assert_eq!(row.error.as_deref(), Some("access denied"));
        let row = serve_row(&Serve::Unavailable(Some("not found".into())), &Setup::Idle);
        assert_eq!((row.mark, row.text, row.button), (Mark::Down, "Install and sign in to Tailscale first", None));
        assert_eq!(row.error.as_deref(), Some("not found"));
        let row = serve_row(&Serve::Unavailable(None), &Setup::Idle);
        assert_eq!((row.button, row.error), (None, None));
    }

    #[test]
    fn set_up_runs_once_and_can_be_pressed_again_after_a_failure() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(!phone.begin_setup(), "nothing to set up before Tailscale was checked");
        phone.polled(
            Ok(web(true, true, None)),
            Some(Local {
                serve: Serve::Missing,
                host: Some("pc.ts.net".into()),
                token: None,
            }),
            t0,
            COLORS,
        );
        assert!(phone.begin_setup());
        assert!(!phone.begin_setup(), "a second click while it runs does nothing");
        phone.setup_done(Err("access denied".into()));
        assert_eq!(phone.setup, Setup::Failed("access denied".into()));
        assert!(phone.begin_setup());
        phone.setup_done(Ok(()));
        assert_eq!((phone.setup.clone(), phone.serve.clone()), (Setup::Idle, Serve::Ready));
        assert!(!phone.begin_setup(), "already set up");
    }

    #[test]
    fn opening_forgets_the_last_check_but_not_the_daemon_state() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.ts.net"), Some(token("a1")));
        phone.write_failed("disk full".into());
        phone.opened();
        assert_eq!(phone.serve, Serve::Checking);
        assert_eq!(phone.setup, Setup::Idle);
        assert_eq!(phone.notice, None);
        assert!(phone.running());
    }

    #[test]
    fn a_failed_write_is_reported_and_clears_with_the_next_attempt() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        phone.write_failed("access denied".into());
        assert_eq!(phone.notice.as_deref(), Some("Cannot write settings.toml: access denied"));
        phone.written(true, t0);
        assert_eq!(phone.notice, None);
    }

    #[test]
    fn polls_do_not_overlap_and_a_closed_dialog_polls_slowly() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(phone.due(t0, false), "never polled");
        assert!(phone.begin_poll(t0));
        assert!(!phone.begin_poll(t0), "one request in flight at a time");
        phone.polled(Ok(web(false, false, None)), None, t0, COLORS);
        assert!(phone.begin_poll(t0));
        phone.polled(Ok(web(false, false, None)), None, t0, COLORS);
        assert!(!phone.due(t0 + Duration::from_secs(4), false));
        assert!(phone.due(t0 + theme::PHONE_POLL_IDLE, false));
        assert!(phone.due(t0, true), "open: every tick");
    }
}
```

Run: `CARGO_TARGET_DIR=… cargo test -p mkrew-ui --lib phone_dialog -j 4`
Expected: compile errors, e.g. ``cannot find type `WebStatus` in this scope``, ``cannot find function `status_row` in this scope``, ``cannot find type `Phone` in this scope``.

- [ ] **Step 5: Implement the model**

Theme constants first. In `crates/mkrew-ui/src/theme.rs` after `pub const DIALOG_WIDTH: f32 = 520.0;` (line 213) add:

```rust

/// Side of the Phone access QR code (ADR 0053): px at the default 13 px body, scaled with
/// `[ui] font_size` ([`rems_px`]). Big enough for a camera across a desk.
pub const PHONE_QR_SIZE: f32 = 216.0;
```

and after `STATUS_TICK` (line 67):

```rust

/// The Phone access poll clock (ADR 0053): while its dialog is open `daemon.status` and
/// Tailscale are read on every tick; closed, `daemon.status` alone every [`PHONE_POLL_IDLE`]
/// (it keeps the title bar icon true).
pub const PHONE_TICK: Duration = Duration::from_secs(1);
pub const PHONE_POLL_IDLE: Duration = Duration::from_secs(5);
```

Now put this at the top of `crates/mkrew-ui/src/phone_dialog.rs`, above the `#[cfg(test)]` block:

```rust
//! The "Phone access" dialog's model (ADR 0053): what the daemon and Tailscale last said, the
//! rows and the link they make, and the poll bookkeeping. Pure and tested headlessly;
//! `shell/phone_access.rs` renders it and runs the blocking calls off the UI thread.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mkrew_core::tailscale::{self, ServeCheck};
use mkrew_protocol::WebStatus;

use crate::{phone, theme};

/// A click on Start / Stop waits at most this long for the daemon to re-read `settings.toml`
/// (it debounces changes); then the row shows what the daemon really reports.
const PENDING_TIMEOUT: Duration = Duration::from_secs(5);

/// What `daemon.status` last said about `mk web`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    Loading,
    Known(WebStatus),
    Failed(String),
}

/// What Tailscale last said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Serve {
    Checking,
    /// `tailscale serve` already proxies Mkrew's port.
    Ready,
    Missing,
    /// Not installed, not running or logged out; the message, if any, is the CLI's.
    Unavailable(Option<String>),
}

/// The Set up button's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setup {
    Idle,
    Running,
    Failed(String),
}

/// `serve_status()` plus the MagicDNS name: a logged-out Tailscale answers "not served", but
/// without a name there is nothing to set up or to link to.
pub fn serve_state(check: Result<ServeCheck, String>, host: Option<&str>) -> Serve {
    match check {
        Err(message) => Serve::Unavailable(Some(message)),
        Ok(_) if host.is_none() => Serve::Unavailable(None),
        Ok(ServeCheck::Ready) => Serve::Ready,
        Ok(ServeCheck::Missing) => Serve::Missing,
    }
}

/// The glyph before a row's text (status is never color alone, SPEC §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    On,
    Off,
    Done,
    Down,
    Wait,
}

impl Mark {
    pub fn glyph(self) -> &'static str {
        match self {
            Mark::On => "●",
            Mark::Off | Mark::Wait => "○",
            Mark::Done => "✓",
            Mark::Down => "✕",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Start,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    pub mark: Mark,
    pub text: String,
    pub switch: Option<Switch>,
}

/// The Status row. `pending` is a Start (`true`) / Stop (`false`) the daemon has not reported
/// yet.
pub fn status_row(daemon: &Daemon, pending: Option<bool>) -> StatusRow {
    let row = |mark, text: &str, switch| StatusRow {
        mark,
        text: text.to_owned(),
        switch,
    };
    match daemon {
        Daemon::Loading => row(Mark::Wait, "Checking…", None),
        Daemon::Failed(error) => StatusRow {
            mark: Mark::Down,
            text: format!("Cannot read the daemon status: {error}"),
            switch: None,
        },
        Daemon::Known(web) => match pending {
            Some(true) => row(Mark::Wait, "Starting…", None),
            Some(false) => row(Mark::Wait, "Stopping…", None),
            None if !web.enabled => row(Mark::Off, "Off", Some(Switch::Start)),
            None if web.running => row(Mark::On, "On", Some(Switch::Stop)),
            None => match &web.error {
                Some(error) => StatusRow {
                    mark: Mark::Down,
                    text: format!("Not running: {}. Retrying…", error.trim_end_matches('.')),
                    switch: Some(Switch::Stop),
                },
                None => row(Mark::Wait, "Starting…", Some(Switch::Stop)),
            },
        },
    }
}

/// The Set up button: pressable, or showing that `tailscale serve` is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetUp {
    Ready,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeRow {
    pub mark: Mark,
    pub text: &'static str,
    pub button: Option<SetUp>,
    /// Shown under the row: the Set up failure, or why Tailscale is unavailable.
    pub error: Option<String>,
}

pub fn serve_row(serve: &Serve, setup: &Setup) -> ServeRow {
    let row = |mark, text, button, error| ServeRow {
        mark,
        text,
        button,
        error,
    };
    match serve {
        Serve::Checking => row(Mark::Wait, "Checking Tailscale…", None, None),
        Serve::Ready => row(Mark::Done, "Tailscale serve is set up", None, None),
        Serve::Missing => row(
            Mark::Off,
            "Tailscale doesn't publish Mkrew yet",
            Some(if *setup == Setup::Running {
                SetUp::Busy
            } else {
                SetUp::Ready
            }),
            match setup {
                Setup::Failed(error) => Some(error.clone()),
                _ => None,
            },
        ),
        Serve::Unavailable(message) => row(
            Mark::Down,
            "Install and sign in to Tailscale first",
            None,
            message.clone(),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkBlock {
    Hidden,
    /// On, name known, `mk web` has not written `web_token` yet (first start).
    Starting,
    Ready,
}

/// The phone link and its QR code.
#[derive(Clone)]
pub struct Link {
    pub text: String,
    /// `None` when the link is too long for any QR code.
    pub image: Option<Arc<gpui::Image>>,
}

/// What the dialog reads from the machine: blocking (two `tailscale` runs and a file), so call
/// it off the UI thread.
pub struct Local {
    pub serve: Serve,
    pub host: Option<String>,
    pub token: Option<String>,
}

impl Local {
    pub fn read(config_dir: &Path) -> Local {
        let check = tailscale::serve_status();
        // No second process when the first one already failed.
        let host = if check.is_ok() {
            tailscale::dns_name()
        } else {
            None
        };
        Local {
            serve: serve_state(check, host.as_deref()),
            host,
            token: phone::read_token(config_dir),
        }
    }
}

/// Phone access as the UI knows it: the title bar icon and the dialog read this.
pub struct Phone {
    pub daemon: Daemon,
    pub serve: Serve,
    pub setup: Setup,
    /// A failed Start / Stop (`settings.toml` could not be written), shown in the dialog.
    pub notice: Option<String>,
    /// "Copy link" was just pressed (the label says so until the next poll).
    pub copied: bool,
    pending: Option<(bool, Instant)>,
    polling: bool,
    last_poll: Option<Instant>,
    host: Option<String>,
    token: Option<String>,
    link: Option<Link>,
}

impl Default for Phone {
    fn default() -> Self {
        Phone {
            daemon: Daemon::Loading,
            serve: Serve::Checking,
            setup: Setup::Idle,
            notice: None,
            copied: false,
            pending: None,
            polling: false,
            last_poll: None,
            host: None,
            token: None,
            link: None,
        }
    }
}

impl Phone {
    /// `mk web` runs: the title bar icon turns accent.
    pub fn running(&self) -> bool {
        matches!(&self.daemon, Daemon::Known(web) if web.running)
    }

    pub fn pending(&self) -> Option<bool> {
        self.pending
    }

    pub fn link(&self) -> Option<&Link> {
        self.link.as_ref()
    }

    pub fn link_block(&self) -> LinkBlock {
        let on = matches!(&self.daemon, Daemon::Known(web) if web.enabled && web.running);
        match (on, &self.host, &self.token) {
            (true, Some(_), Some(_)) => LinkBlock::Ready,
            (true, Some(_), None) => LinkBlock::Starting,
            _ => LinkBlock::Hidden,
        }
    }

    /// The dialog opens: Tailscale is asked again, an earlier Set up failure is forgotten.
    pub fn opened(&mut self) {
        self.serve = Serve::Checking;
        self.setup = Setup::Idle;
        self.notice = None;
        self.copied = false;
    }

    /// The loop's tick: the dialog polls on every tick, a closed one every
    /// [`theme::PHONE_POLL_IDLE`].
    pub fn due(&self, now: Instant, open: bool) -> bool {
        open || self
            .last_poll
            .is_none_or(|last| now.duration_since(last) >= theme::PHONE_POLL_IDLE)
    }

    /// `false` while the previous poll has not answered (answers must not arrive out of order).
    pub fn begin_poll(&mut self, now: Instant) -> bool {
        if self.polling {
            return false;
        }
        self.polling = true;
        self.last_poll = Some(now);
        true
    }

    /// A poll answered: `web` from `daemon.status`, `local` only when the dialog is open.
    /// `colors` = `#rrggbb` of `qr_dark` / `qr_light`.
    pub fn polled(
        &mut self,
        web: Result<WebStatus, String>,
        local: Option<Local>,
        now: Instant,
        colors: (&str, &str),
    ) {
        self.polling = false;
        self.copied = false;
        match web {
            Ok(web) => {
                if let Some((wanted, since)) = self.pending
                    && (web.enabled == wanted || now.duration_since(since) >= PENDING_TIMEOUT)
                {
                    self.pending = None;
                }
                self.daemon = Daemon::Known(web);
            }
            Err(error) => self.daemon = Daemon::Failed(error),
        }
        let Some(local) = local else {
            return;
        };
        self.serve = local.serve;
        self.host = local.host;
        self.token = local.token;
        self.link = match (&self.host, &self.token) {
            (Some(host), Some(token)) => {
                let text = phone::link(host, token);
                // Every poll lands here: render the QR again only for another link.
                if self.link.as_ref().is_some_and(|link| link.text == text) {
                    self.link.take()
                } else {
                    let image = phone::qr_svg(&text, colors.0, colors.1).map(|svg| {
                        Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Svg,
                            svg.into_bytes(),
                        ))
                    });
                    Some(Link { text, image })
                }
            }
            _ => None,
        };
    }

    /// Start / Stop was written to `settings.toml`; the row waits for the daemon to agree.
    pub fn written(&mut self, enabled: bool, now: Instant) {
        self.pending = Some((enabled, now));
        self.notice = None;
    }

    pub fn write_failed(&mut self, error: String) {
        self.notice = Some(format!("Cannot write settings.toml: {error}"));
    }

    /// `true` when a Set up should start now (Tailscale lacks the serve entry, none runs).
    pub fn begin_setup(&mut self) -> bool {
        if self.serve != Serve::Missing || self.setup == Setup::Running {
            return false;
        }
        self.setup = Setup::Running;
        true
    }

    pub fn setup_done(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.setup = Setup::Idle;
                // The command succeeded; the next poll confirms.
                self.serve = Serve::Ready;
            }
            Err(error) => self.setup = Setup::Failed(error),
        }
    }

    /// The text for the clipboard; `None` without a link.
    pub fn copy_link(&mut self) -> Option<String> {
        let text = self.link.as_ref()?.text.clone();
        self.copied = true;
        Some(text)
    }
}
```

Run: `CARGO_TARGET_DIR=… cargo test -p mkrew-ui --lib phone_dialog -j 4`
Expected: all `phone_dialog` tests pass (13). (`Phone` has no `Debug`: `gpui::Image` has none, and no test formats a `Phone`.)

- [ ] **Step 6: Shell state, polling, actions (`shell/phone_access.rs`)**

In `crates/mkrew-ui/src/shell.rs` add `mod phone_access;` after `mod peek;`.

In `crates/mkrew-ui/src/main.rs`:
- After line 78 (`use mkrew_ui::peek::{self, Peek};`) add `use mkrew_ui::phone_dialog::{self, LinkBlock, Local, Mark, Phone, SetUp, Switch};`.
- After line 354 (`prompts_dir: PathBuf,`) add:

```rust
    /// `<config>/mkrew`: `mk web` keeps `web_token` there (Phone access link, ADR 0053).
    config_dir: PathBuf,
    /// Phone access (ADR 0053): what `daemon.status` and Tailscale last said; the title bar
    /// icon and the dialog read it.
    phone: Phone,
    /// The "Phone access" dialog is open (modal).
    phone_open: bool,
```
- In `render`, after the `git_base` children block (line 678, `)` closing `.children(` for `git_base`), add:

```rust
                    .children(
                        self.phone_open
                            .then(|| self.render_phone_access(&theme, cx)),
                    )
```

In `crates/mkrew-ui/src/shell/focus.rs` `modal_open`: add `|| self.phone_open` after `|| self.git_base.is_some()`.

In `crates/mkrew-ui/src/shell/connection.rs`:
- after `let prompts_dir = paths.prompts_dir();` (line 24) add `let config_dir = paths.config_dir.clone();`
- after the status tick loop's `.detach();` (line 109) add:

```rust
        // The title bar's phone icon follows `daemon.status.web`: read every `PHONE_POLL_IDLE`,
        // and on every tick while the Phone access dialog is open (ADR 0053).
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(theme::PHONE_TICK).await;
                let ticked = this.update_in(cx, |shell, window, cx| {
                    if shell.phone.due(Instant::now(), shell.phone_open) {
                        shell.poll_phone(window, cx);
                    }
                });
                if ticked.is_err() {
                    return;
                }
            }
        })
        .detach();
```
- in the connect hook (lines 147-150) after `shell.refresh(window, cx);` add `shell.poll_phone(window, cx);`
- in the disconnect reset after `shell.git_base = None;` (line 209) add `shell.phone_open = false;` and `shell.phone = Phone::default();`
- in the `Self { … }` literal after `prompts_dir,` (line 269) add `config_dir,`, `phone: Phone::default(),`, `phone_open: false,`.

Create `crates/mkrew-ui/src/shell/phone_access.rs`:

```rust
//! The "Phone access" dialog and the title bar's phone icon (ADR 0053): start / stop `mk web`
//! (the daemon supervises it while `[web] enabled`), publish it with `tailscale serve`, and
//! hand the link to the phone as a QR code.

use crate::*;

impl Shell {
    /// "Phone access…" (palette, title bar icon): opens the dialog and reads the state at once.
    pub(crate) fn open_phone_access(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // The status comes from the daemon: no connection, nothing to show.
        if self.connection.is_none() {
            return;
        }
        self.close_menus();
        self.phone_open = true;
        self.phone.opened();
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        self.poll_phone(window, cx);
    }

    pub(crate) fn close_phone_access(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.phone_open = false;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn phone_access_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_phone_access(window, cx);
        }
    }

    /// Reads `daemon.status` (always) and Tailscale + `web_token` (dialog open). Both are off
    /// the UI thread: the call is a queued request, the `tailscale` runs and the file read go
    /// to the background executor. One poll at a time.
    pub(crate) fn poll_phone(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.phone.begin_poll(Instant::now()) {
            return;
        }
        let open = self.phone_open;
        let config_dir = self.config_dir.clone();
        let status = connection.call::<mkrew_protocol::DaemonStatusResult>(Request::DaemonStatus);
        cx.spawn_in(window, async move |this, cx| {
            let web = status
                .await
                .map(|status| status.web)
                .map_err(|error| format!("{error:#}"));
            let local = if open {
                Some(
                    cx.background_executor()
                        .spawn(async move { Local::read(&config_dir) })
                        .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |shell, cx| {
                let (dark, light) = {
                    let theme = cx.theme();
                    (theme::hex(theme.qr_dark), theme::hex(theme.qr_light))
                };
                shell
                    .phone
                    .polled(web, local, Instant::now(), (&dark, &light));
                cx.notify();
            });
        })
        .detach();
    }

    /// Start / Stop: `[web] enabled` in `settings.toml`; the daemon's settings watcher does the
    /// rest. A failed write shows in the dialog (there is no toast layer).
    pub(crate) fn set_phone_enabled(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match mkrew_core::settings::write_web_enabled(&self.settings_file, enabled) {
            Ok(()) => self.phone.written(enabled, Instant::now()),
            Err(error) => {
                tracing::warn!("cannot write [web] enabled: {error}");
                self.phone.write_failed(error.to_string());
            }
        }
        cx.notify();
        self.poll_phone(window, cx);
    }

    /// "Set up": `tailscale serve --bg 8787` in the background (it can take a while: 30 s at
    /// most); its error shows under the Tailscale row.
    pub(crate) fn set_up_tailscale(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.phone.begin_setup() {
            return;
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async { mkrew_core::tailscale::serve_setup(mkrew_core::tailscale::WEB_PORT) })
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.phone.setup_done(done);
                cx.notify();
            });
        })
        .detach();
    }

    /// "Copy link": the link carries the token; it goes to the clipboard and nowhere else
    /// (not to logs, not to a tooltip).
    pub(crate) fn copy_phone_link(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(text) = self.phone.copy_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
    }

    /// The title bar's phone icon: accent while `mk web` runs, muted otherwise. Hidden without
    /// a daemon connection (the dialog needs it).
    pub(crate) fn render_phone_button(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        self.connection.as_ref()?;
        let (color, text) = if self.phone.running() {
            (theme.accent, "Phone access: on")
        } else {
            (theme.text_muted, "Phone access: off")
        };
        Some(
            icon_button_in("phone-access", Icon::Smartphone, color, theme)
                .tooltip(tooltip(theme, text, ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.open_phone_access(window, cx);
                })),
        )
    }

    /// The dialog: Status, Tailscale, then the link (QR, text, Copy link) while it is on.
    pub(crate) fn render_phone_access(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let phone = &self.phone;
        let glyph = |mark: Mark| {
            div()
                .flex_none()
                .text_color(rgb(match mark {
                    Mark::On | Mark::Done => theme.accent,
                    Mark::Down => theme.status_exited,
                    Mark::Off | Mark::Wait => theme.text_muted,
                }))
                .child(mark.glyph())
        };
        let line = |text: String| {
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(text)
        };
        let error_row = |text: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(text))
        };

        let status = phone_dialog::status_row(&phone.daemon, phone.pending());
        let status_row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(glyph(status.mark))
            .child(line(status.text))
            .children(status.switch.map(|switch| match switch {
                Switch::Start => button("phone-start", "Start", theme, true)
                    .tooltip(tooltip(
                        theme,
                        "Turn phone access on: the daemon runs mk web",
                        "",
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.set_phone_enabled(true, window, cx);
                    })),
                Switch::Stop => button("phone-stop", "Stop", theme, false)
                    .tooltip(tooltip(
                        theme,
                        "Turn phone access off: the phone can no longer reach Mkrew",
                        "",
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.set_phone_enabled(false, window, cx);
                    })),
            }));

        let serve = phone_dialog::serve_row(&phone.serve, &phone.setup);
        let serve_row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(glyph(serve.mark))
            .child(line(serve.text.to_owned()))
            .children(serve.button.map(|state| match state {
                SetUp::Ready => button("phone-setup", "Set up", theme, false)
                    .tooltip(tooltip(
                        theme,
                        "Run tailscale serve so the phone reaches Mkrew over HTTPS",
                        "",
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.set_up_tailscale(window, cx);
                    })),
                SetUp::Busy => disabled_button("phone-setup", "Setting up…", theme, false),
            }));

        let link: Option<gpui::AnyElement> = match phone.link_block() {
            LinkBlock::Hidden => None,
            LinkBlock::Starting => Some(meta_line(theme, "Starting…".into()).into_any_element()),
            LinkBlock::Ready => phone.link().map(|link| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    // Dark modules on the light `qr_light` panel in both themes: phone cameras
                    // read that reliably (the SVG carries its own quiet zone and background).
                    .children(link.image.clone().map(|image| {
                        div().flex().justify_center().child(
                            gpui::img(image)
                                .size(theme::rems_px(theme::PHONE_QR_SIZE))
                                .rounded(px(radius::MEDIUM)),
                        )
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(font_size::META)
                                    .text_color(rgb(theme.text_muted))
                                    .child(link.text.clone()),
                            )
                            .child(
                                button(
                                    "phone-copy",
                                    if phone.copied { "Copied" } else { "Copy link" },
                                    theme,
                                    false,
                                )
                                .tooltip(tooltip(theme, "Copy the link (it carries the access token)", ""))
                                .on_click(cx.listener(|shell, _, _, cx| {
                                    shell.copy_phone_link(cx);
                                })),
                            ),
                    )
                    .child(meta_line(
                        theme,
                        "Scan with the iPhone camera, then Share → Add to Home Screen.".into(),
                    ))
                    .into_any_element()
            }),
        };

        let panel = div()
            .id("phone-access-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::phone_access_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Phone access"),
            )
            .child(status_row)
            .children(phone.notice.clone().map(error_row))
            .child(serve_row)
            .children(serve.error.map(error_row))
            .children(link)
            .child(
                div().flex().items_center().justify_end().pt(px(space::S)).child(
                    button("phone-close", "Close", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_phone_access(window, cx);
                        })),
                ),
            );
        Self::modal_shade("phone-access-shade", panel, cx)
    }
}
```

Next, the recolorable icon button. In `crates/mkrew-ui/src/shell/widgets.rs` replace the whole `icon_button` fn (lines 259-278, from `/// A square ghost button holding one icon (theme toggle, sidebar +).` to its closing `}`) with:

```rust
/// A square ghost button holding one icon (theme toggle, sidebar +).
pub(crate) fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    icon_button_in(id, glyph, theme.text_muted, theme)
}

/// An [`icon_button`] whose icon is drawn in `color`: a state (the phone icon is accent while
/// phone access is on).
pub(crate) fn icon_button_in(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::ICON_BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(icon_svg(glyph, theme::ICON_SIZE, color))
}
```

In `crates/mkrew-ui/src/shell/chrome.rs` change line 147 (the `);` that closes the theme toggle's `.child(`) to:

```rust
            )
            .children(self.render_phone_button(theme, cx));
```

(Leave the `if let Some(project_id) = workspace { right = right.child(history)… }` that follows as is: the phone icon sits right of the theme toggle, left of History, and shows without a workspace.)

Also update the title bar doc comment above `render_title_bar` (chrome.rs:61-65): replace `the theme toggle, History (◷)` with `the theme toggle, the phone icon (Phone access, ADR 0053), History (◷)`.

- [ ] **Step 7: Build, tests, clippy**

Run:

```
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo test -p mkrew-ui --lib -j 4
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
```

Expected: tests pass (palette: `phone_access_is_listed_in_every_view_and_without_a_workspace`, `palette_only_actions_follow_the_view_and_state`; `phone_dialog`; `icons`); clippy clean. If a gpui builder method name differs (e.g. `Img::rounded` or `Stateful::tooltip` generics), fix the call to the form used in the neighbouring files (`widgets.rs:720-737` for `img`, `history.rs:451-459` for `tooltip`); do not change the model.

- [ ] **Step 8: Token and literal check**

Run: `git diff -U0 -- crates/mkrew-ui/src | grep -nE '^\+.*(#[0-9A-Fa-f]{6}|0x[0-9A-Fa-f]{6}|px\([0-9])'`
Expected: only the `#101114` / `#FFFFFF` strings inside the test constant `COLORS` in `phone_dialog.rs` (test data). No other hex colors and no raw `px(<number>)` outside `theme.rs`.

- [ ] **Step 9: Docs — SPEC**

Edit `docs/SPEC.md` (each replacement is an exact string on the cited line):

1. §4 tree, line 192: replace `state machine. NO I/O beyond files.` with `state machine. NO I/O beyond files, except the `tailscale` CLI runs of `mkrew_core::tailscale` (ADR 0053).`
2. §7 table, line 558: replace
   `| `daemon.status/stop` | – | info / ok |`
   with
   `| `daemon.status/stop` | – | info / ok; `status` carries `web` = `{enabled, running, error?}`: the `mk web` child the daemon supervises while `[web] enabled` (§20.0, ADR 0053); `error` = the child's last stderr line |`
3. §7, after line 609 (`v2 adds requests and events in §20; each milestone that changes the protocol bumps `PROTOCOL_VERSION`.`) add a paragraph (blank line before):
   `Protocol 40 (ADR 0053): `daemon.status` gains `web`.`
4. Title bar, lines 753-754: replace `the theme toggle, History (`◷` icon), `+ New ▾`` with `the theme toggle, the phone icon (`smartphone`, §14; hidden without a daemon connection), History (`◷` icon), `+ New ▾``.
5. §14 token table, after line 1249 (`| `diff-hunk-header` | #2C2E37 | #F6F7F9 |`) add:
   `| `qr-dark` | #101114 | #101114 |`
   `| `qr-light` | #FFFFFF | #FFFFFF |`
   and after the table's neighbouring bullet list item at line 1276 (end of the Icons bullet, `stay text. Status icons (§11), marks in messages and the live Run label stay text.`) add before `- Density`:
   ```
   - Phone access (ADR 0053): a Lucide `smartphone` icon button in the title bar, right of the
     theme toggle (accent while `mk web` runs, `text-muted` otherwise; tooltip "Phone access: on" /
     "Phone access: off"), and the palette command "Global: Phone access…", open the "Phone
     access" dialog (`DIALOG_WIDTH`, same frame as the other dialogs, Esc / Close). Rows, each
     with a glyph and text (§11): **Status** `○ Off` + Start, `● On` + Stop, `✕ Not running:
     <error>. Retrying…` + Stop; **Tailscale** `✓ Tailscale serve is set up`, `Tailscale doesn't
     publish Mkrew yet` + Set up (its error below), `Install and sign in to Tailscale first`; **Link**
     (only while On and the MagicDNS name is known): the QR code, the link as one line and Copy
     link, "Scan with the iPhone camera, then Share → Add to Home Screen." (`Starting…` until
     `mk web` has written `web_token`). The QR is always dark modules (`qr-dark`) on a light
     panel (`qr-light`) in both themes, `PHONE_QR_SIZE` px. The dialog re-reads `daemon.status`
     and Tailscale about once a second while open (off the UI thread); the title bar icon reads
     `daemon.status` every 5 s. A failed Start / Stop shows in the dialog. The link carries the
     access token: it is never logged and never in a tooltip.
   ```
6. §15 Security row, line 1313: replace `the daemon has no network listener (`mk web`, started by the user, listens on 127.0.0.1 by default and requires its token on every WebSocket, ADR 0052)` with `the daemon has no network listener (`mk web` — started by the user, or by the daemon as a child while the user turned phone access on, ADR 0053 — listens on 127.0.0.1 by default and requires its token on every WebSocket, ADR 0052)`.
7. §20.0 settings template, after line 1619 (`turns = false               # §20C.1–2 turn snapshots + "Turns…"; read when a session starts`) and before the closing ``` add:
   ```
   [web]                       # ADR 0053; phone access
   enabled = false             # the daemon runs `mk web` while true; the Phone access dialog writes it
   ```
   and after the closing ``` (line 1620) and its blank line, before `**Turn.**`, add:
   ```
   **Phone access** (ADR 0053). While `[web] enabled = true` the daemon supervises `mk web` as a
   windowless child (`mk.exe` next to `mkrew-daemon.exe`, else on PATH; stderr's last line is the
   `error`; a child that exits is started again after 1, 2, 4 … 60 s; `enabled = false` kills it).
   `daemon.status.web` reports `{enabled, running, error?}`. The Phone access dialog (§14) writes the
   key and shows the link; `mk web --new-token` still revokes old links.

   ```
8. In the same file, update the TOC/§9 nothing else.

- [ ] **Step 10: Docs — ADR 0053, README, HUMAN_CHECKS, PROGRESS**

Create `docs/decisions/0053-phone-access-ui.md`:

```markdown
# 0053 — Phone access from the Mkrew UI

- Date: 2026-10-12
- Status: accepted (reverses the "starting `mk web` from the daemon" non-goal and the "keep `mk web`
  running with Task Scheduler" advice of ADR 0052)
- Milestone: none (outside the milestone loop; user request: no typing `mk web` or `tailscale serve`)

## Context

ADR 0052 needs the user to start `mk web` by hand (or with Task Scheduler) and to run
`tailscale serve --bg 8787` once. The user wants to do all of it from the Mkrew UI: see whether phone
access is on, switch it on and off, set up Tailscale when it is missing, and move the link to the
iPhone without typing.

## Options

1. The UI starts `mk web` as its own child: it dies with the UI; phone access would stop when the
   window is closed.
2. A listener inside the daemon: a network surface in the process that owns every agent (rejected
   in ADR 0052).
3. The daemon supervises `mk web` as a child while `[web] enabled = true` in `settings.toml`; the UI
   writes the key and reads the state from `daemon.status`. (Chosen.)

## Decision

- `[web] enabled` (bool, default `false`) is the only setting. The UI writes it
  (`mkrew_core::settings::write_web_enabled`, keeps the other sections and comments); the daemon
  reacts through its settings watcher: it spawns `mk web` (windowless, `kill_on_drop`, stderr read),
  restarts a failed child after 1, 2, 4 … 60 s (a 60 s run resets the wait), kills it when disabled.
  `mk.exe` not found → `error`, looked up again every 60 s. The daemon process still has no
  network listener: the socket is in the `mk web` child (SPEC §15).
- `daemon.status` gains `web: {enabled, running, error?}`; `PROTOCOL_VERSION` 39 → 40.
- The UI runs `tailscale serve status --json` (parsed in `mkrew_core::tailscale`, shared with `mk web`)
  and offers **Set up** (`tailscale serve --bg 8787`) only when the proxy is missing and a MagicDNS name
  exists. `mkrew-core` therefore runs the `tailscale` CLI (every run `CREATE_NO_WINDOW`); SPEC §4's
  "no I/O beyond files" says so.
- UI: palette `Global: Phone access…` and a Lucide `smartphone` title bar icon (accent while running)
  open the Phone access dialog (SPEC §14). It polls `daemon.status` and Tailscale about once a second
  while open (background executor, never in render); the icon reads `daemon.status` every 5 s. The
  link `https://<MagicDNS name>/#t=<token>` (token from `<config>/web_token`) is shown as a QR code
  (`qrcodegen`, SVG drawn with `gpui::img`; dark modules on a light panel in both themes: tokens
  `qr_dark` / `qr_light`) and can be copied. It appears only in this dialog.
- Not in the UI (YAGNI): a new-token button (use `mk web --new-token`), choosing port or listen
  address, starting Tailscale or signing in, notifications, several PCs.

## Consequences

- The README setup becomes: phone icon → Start (→ Set up the first time) → scan the QR. Phone access
  stays on across reboots and with the UI closed, because the daemon owns the child.
- A hand-started `mk web` (or a Task Scheduler entry) still works but competes for port 8787: the
  dialog then shows `Not running: <bind error>. Retrying…`.
- Stop kills the child: open phone sockets drop at once.
- The UI cannot show a toast (none exists); a failed Start / Stop is an error row in the dialog.
```

`README.md`: replace lines 182-192 (from `1. Install Tailscale on this PC and on the phone…` through `and keep that one running: old links stop working.`) with:

```markdown
1. Install Tailscale on this PC and on the phone, signed in to the same account. In the
   Tailscale admin console, under DNS, turn on MagicDNS and HTTPS Certificates.
2. In Mkrew click the phone icon in the title bar (or open the command palette, `Ctrl+Shift+P`,
   and pick "Phone access…"), then click **Start**. The first time, also click **Set up**: it runs
   `tailscale serve --bg 8787`, which Tailscale remembers across reboots.
3. Scan the QR code with the iPhone camera (or click **Copy link** and send the link to yourself,
   e.g. in Notes). On iPhone: Share → Add to Home Screen.

Phone access stays on across reboots, also when the Mkrew window is closed. **Stop** in the same
dialog cuts the phone off at once. The phone never changes a session's size; the terminal font
shrinks to fit (pinch to zoom).
Lost the phone or shared the link by mistake? Click **Stop**, run `mk web --new-token` and stop it
again with `Ctrl+C`, then click **Start**: old links stop working.
```

`docs/HUMAN_CHECKS.md`: insert directly above line 18 (`### To verify — 2026-10-12 Mkrew web on a phone (plan docs/superpowers/plans/2026-10-10-mobile-web.md)`):

```markdown
### To verify — 2026-10-12 Phone access from the UI (plan docs/superpowers/plans/2026-10-12-phone-access-ui.md, ADR 0053)

- [ ] **Dialog, dark and light:** palette → "Global: Phone access…" and the title bar phone icon both
  open "Phone access". Check Off / On / `✕ Not running` rows, the Tailscale row, the QR panel (dark
  modules on white in both themes), the link line and Copy link in dark and light; no clipped text at
  the 520 px width; Esc closes.
- [ ] **Start → icon accent:** Start → the row shows "Starting…" then `● On`, the title bar icon turns
  accent and its tooltip reads "Phone access: on"; Stop → `○ Off`, muted icon, "Phone access: off".
  Restart the PC with phone access on and the Mkrew window closed: after login the icon is accent
  when the UI starts again (the daemon started `mk web`).
- [ ] **Scan:** with Tailscale set up, scan the QR with the iPhone camera → Safari opens the Mkrew
  page and shows Sessions (no token screen). Copy link pastes the same link.
- [ ] **Stop while the phone page is open:** Stop → within a few seconds the phone page shows "Can't
  reach Mkrew on <pc>. Retrying…" and does not come back until Start.
- [ ] **Set up on a PC without serve config:** `tailscale serve reset`, open the dialog → "Tailscale
  doesn't publish Mkrew yet" + Set up; click it → the button reads "Setting up…", then "✓ Tailscale
  serve is set up". Sign out of Tailscale → "Install and sign in to Tailscale first", no Set up.
- [ ] **Port in use:** run `mk web` by hand, then Start → `✕ Not running: <bind error>. Retrying…`;
  stop the hand-started one → the row becomes `● On` within about a minute.

```

`docs/PROGRESS.md`: insert directly under `## Notes for the next session` (after line 32, the blank line), as the first bullet:

```markdown
- 2026-10-12 phone access from the UI (plan docs/superpowers/plans/2026-10-12-phone-access-ui.md, ADR 0053, protocol 39→40): `[web] enabled` (`mkrew_core::settings::write_web_enabled`), `mkrew_core::tailscale` (shared with `mk web`), daemon `web.rs` supervises `mk web` (`daemon.status.web`), UI `phone.rs` (link, token, QR svg), `phone_dialog.rs` (pure model) + `shell/phone_access.rs` (dialog, title bar `smartphone` icon, palette "Global: Phone access…", 1 s poll while open / 5 s for the icon). No toast layer exists, so Start/Stop errors show inside the dialog. Manual checks in HUMAN_CHECKS "2026-10-12 Phone access from the UI". Outside the milestone loop.
```

- [ ] **Step 11: Rebuild bins**

Run: `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins -j 4`
Expected: builds (`target/debug/mkrew.exe`, `mk.exe`, `mkrew-daemon.exe`). If an exe is locked, rename it to `*.locked-old.exe` and re-run. Never kill the user's daemon or UI.

- [ ] **Step 12: Smoke (isolated; never the user's daemon, config or Tailscale)**

Port 8787 is the real port `mk web` binds, and `tailscale` is the real one on this PC: before starting, run `netstat -ano | findstr :8787`. If something listens there (the user's own `mk web`), the dialog will show the "Not running: … Retrying…" row instead of `● On`: that is also a valid check of that row; do not stop the user's process. **Never click Set up in the smoke** (it changes the user's Tailscale config).

Write `$env:TEMP\phone-smoke.ps1`:

```powershell
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int L, T, R, B; }
}
'@
function Shot($path) {
  $h = [W]::GetForegroundWindow(); $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r)
  $bmp = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  $bmp.Save($path); $g.Dispose(); $bmp.Dispose()
}
$root = Join-Path $env:TEMP 'phone-smoke'
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$root\data", "$root\config" | Out-Null
$env:MKREW_DATA_DIR = "$root\data"; $env:MKREW_CONFIG_DIR = "$root\config"
$env:MKREW_SOCKET = '\\.\pipe\mkrew-phone-smoke'; $env:MKREW_NOTIFY = 'off'
$t = 'C:\Users\NguyenTung\Workspace\PersonalProject\mkrew-build-kit\mkrew\target\debug'
$shots = 'C:\Users\NguyenTung\Workspace\PersonalProject\mkrew-build-kit\mkrew\.worktrees\mobile-web\docs\screenshots'
foreach ($theme in 'dark', 'light') {
  Set-Content "$root\config\settings.toml" "[ui]`ntheme = `"$theme`"`n"
  $ui = Start-Process "$t\mkrew.exe" -PassThru
  Start-Sleep 6
  [System.Windows.Forms.SendKeys]::SendWait('^+p'); Start-Sleep 1
  [System.Windows.Forms.SendKeys]::SendWait('Phone access'); Start-Sleep 1
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}'); Start-Sleep 3
  Shot "$shots\2026-10-12-phone-access-$theme-off.png"
  # Start through the same key the dialog writes (the click path is a HUMAN_CHECKS item).
  Add-Content "$root\config\settings.toml" "`n[web]`nenabled = true`n"
  Start-Sleep 6
  Shot "$shots\2026-10-12-phone-access-$theme-on.png"
  Set-Content "$root\config\settings.toml" "[ui]`ntheme = `"$theme`"`n`n[web]`nenabled = false`n"
  Start-Sleep 4
  Shot "$shots\2026-10-12-phone-access-$theme-stopped.png"
  Stop-Process -Id $ui.Id
}
& "$t\mk.exe" daemon stop
Remove-Item -Recurse -Force $root
```

Run: `powershell -NoProfile -File $env:TEMP\phone-smoke.ps1`
Expected, checked by reading the six PNGs (`read` the images): `-off`: `○ Off` + Start, a Tailscale row (the real one on this PC), phone icon muted; `-on`: `● On` (or `✕ Not running: …` if 8787 is taken), phone icon accent, the QR + link + Copy link only when Tailscale gave a MagicDNS name and `mk web` wrote `web_token`; `-stopped`: `○ Off`, muted icon. Dark and light both legible, QR dark-on-light in both. If the PNGs come out cropped or black (DPI scaling / protected window), delete them, skip the screenshots and rely on the HUMAN_CHECKS block. Keep only two of the PNGs if they are good (`…-dark-on.png`, `…-light-on.png`), renamed to `docs/screenshots/2026-10-12-phone-access-dark.png` / `-light.png`; delete the rest. Delete `$env:TEMP\phone-smoke.ps1` and make sure no `mkrew.exe` / `mkrew-daemon.exe` / `mk.exe web` of the smoke is left (`Get-Process mkrew*, mk` — stop only processes whose start time is from this run; never the user's).

- [ ] **Step 13: Full verify and commit**

```
cargo fmt --all
CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target sh scripts/verify.sh
```

Expected: fmt clean, clippy clean, all tests pass.

```
git add icon/lucide/smartphone.svg crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/phone_dialog.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/widgets.rs docs/SPEC.md docs/decisions/0051-lucide-icons.md docs/decisions/0053-phone-access-ui.md README.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git add docs/screenshots/2026-10-12-phone-access-dark.png docs/screenshots/2026-10-12-phone-access-light.png
git commit -m "feat(ui): Phone access dialog, title bar icon and palette command (ADR 0053)"
```

(Skip the second `git add` if Step 12 produced no usable screenshots. `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `crates/mkrew-ui/src/theme.rs` and `widgets.rs` may carry the user's uncommitted work: check `git diff --stat` on them first and stage only this task's hunks with `git add -p` if so.)

**Contract notes:**
- The contract says "error → toast". No toast layer exists in the UI (`TOAST_DURATION` is unused; SPEC §14 describes toasts but nothing renders them), so a failed Start/Stop (and Set up) shows as an error row inside the dialog; building a toast layer is out of scope.
- Esc uses `undo_dialog::cancels` (what the Git discard dialog uses), not `base_dialog::key` (that is the Git base menu's text-field key map).
- Added one lib file beyond the contract, `crates/mkrew-ui/src/phone_dialog.rs` (pure model + tests, same split as `turns_dialog.rs` / `base_dialog.rs` vs their `shell/` renderers), so the status/Tailscale/link rules and poll bookkeeping are unit-tested; and three theme constants (`PHONE_QR_SIZE`, `PHONE_TICK`, `PHONE_POLL_IDLE`), `widgets::icon_button_in` (the existing `icon_button` cannot be recolored), and a `config_dir` field on `Shell` (only `settings_file` was kept).
- The title bar icon needs the running state while the dialog is closed, so `daemon.status` is also polled every 5 s (and once on connect); Tailscale/`web_token` are read only while the dialog is open.
- Per Task 1's note, "Install and sign in to Tailscale first" is also shown when `dns_name()` is `None` (`serve_state`), and Set up is offered only when a name exists and the serve entry is `Missing`; the Set up button shows "Setting up…" while `serve_setup` runs (30 s kill timeout).
- Added wait states the spec does not list: "Starting…" / "Stopping…" after a click until the daemon's `web.enabled` agrees (max 5 s), and "Starting…" for enabled-without-error. The spec's `✕ Not running: <error>. Retrying…` is shown for every error, including `mk.exe not found next to mkrew-daemon` where Task 2 does not retry; telling them apart would need a string match across crates.
- Smoke drives the dialog with the palette (SendKeys) and toggles `[web] enabled` by editing the isolated `settings.toml` (the daemon path); the Start/Stop click and Copy link are HUMAN_CHECKS items, and Set up is never clicked in the smoke (it would change the real Tailscale config).
