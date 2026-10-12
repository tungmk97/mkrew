//! The `tailscale` CLI as the phone page needs it (ADR 0052, ADR 0053): this PC's MagicDNS name
//! and whether `tailscale serve` already publishes `mk web`. The JSON readers are pure; the
//! runners start `tailscale` without a console window (the daemon and the UI have none).

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The port `mk web` listens on by default and `tailscale serve` is pointed at (ADR 0052).
pub const WEB_PORT: u16 = 8787;

/// `tailscale serve --bg` can wait for the user (e.g. "Serve is not enabled on your tailnet"
/// with a link to approve it); [`serve_setup`] gives up after this.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);

/// `tailscale status` / `serve status` answer at once; [`run`] gives up after this so a stuck
/// service cannot freeze the UI poll or `mk web` (which asks for the name before it binds).
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

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

/// Stdout of a successful `tailscale <args>`; else what it said, or why it did not finish.
fn run(args: &[&str]) -> Result<String, String> {
    let mut child = tailscale(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run tailscale: {e}"))?;
    // Read both pipes while waiting: a full pipe must not look like a hang.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                // The child may have exited since `try_wait`; there is nothing to do then.
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "tailscale did not answer in {} s",
                    QUERY_TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("cannot wait for tailscale: {e}")),
        }
    };
    let output = Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(failure(&output))
    }
}

/// Reads `pipe` to its end on a thread.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            // A read error ends the text early; what was read so far is still what it said.
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
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
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    text.trim().to_owned()
}

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
        assert!(!serve_proxies_port(
            &proxying("http://127.0.0.1:18787"),
            8787
        ));
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
            "https://127.0.0.1:8787",    // `mk web` speaks plain HTTP
            "http://127.0.0.1:8787/api", // the page would be served from a sub-path
            "http://192.168.1.5:8787",   // another machine
            "http://127.0.0.1",          // port 80
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
        for (what, json) in [
            ("8443", on_8443),
            ("sub-path", sub_path),
            ("no https", no_https),
        ] {
            assert!(!serve_proxies_port(&json, 8787), "{what}");
        }
    }

    #[test]
    fn a_handler_that_is_not_a_proxy_does_not_count() {
        let path = SERVED.replace(
            r#""Proxy": "http://127.0.0.1:8787""#,
            r#""Path": "C:\\site""#,
        );
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
        assert_eq!(
            said(b"  denied: not logged in\r\n", b"ignored"),
            "denied: not logged in"
        );
        // Some versions print their complaint on stdout.
        assert_eq!(
            said(b" \n", b"Serve is not enabled on your tailnet.\n"),
            "Serve is not enabled on your tailnet."
        );
        assert_eq!(said(b"", b"\n"), "");
    }
}
