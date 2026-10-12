//! Tailscale's MagicDNS name of this PC, for the link `mk web` prints (ADR 0051).

use std::process::Command;

/// `tailscale status --json` → `Self.DNSName`; `None` when Tailscale is missing, stopped or
/// has no MagicDNS name.
pub fn dns_name() -> Option<String> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_dns_name(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_dns_name(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let name = value
        .get("Self")?
        .get("DNSName")?
        .as_str()?
        .trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
