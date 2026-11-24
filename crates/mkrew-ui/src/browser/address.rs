//! The browser pane's address field (spec 6.2): what a typed text loads.

/// The URL `input` loads, `None` for blank input. A kept scheme, a local address over http,
/// a host name over https, anything else a Google search.
pub fn normalize(input: &str) -> Option<String> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    if let Some((scheme, _)) = text.split_once(':')
        && ["http", "https", "file", "about"].contains(&scheme.to_ascii_lowercase().as_str())
    {
        return Some(text.to_owned());
    }
    if !text.contains(char::is_whitespace) {
        let host = host_of(text);
        if is_local(host) {
            return Some(format!("http://{text}"));
        }
        if host.contains('.') && !host.contains(['\\', ':']) {
            return Some(format!("https://{text}"));
        }
    }
    Some(format!("https://www.google.com/search?q={}", query(text)))
}

/// `text` up to the first `/`, without its port (a bracketed IPv6 address keeps its brackets).
fn host_of(text: &str) -> &str {
    let authority = text.split('/').next().unwrap_or(text);
    if authority.starts_with('[') {
        return authority.split_inclusive(']').next().unwrap_or(authority);
    }
    authority.split(':').next().unwrap_or(authority)
}

fn is_local(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::Ipv4Addr>().is_ok()
        || host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .is_some_and(|h| h.parse::<std::net::Ipv6Addr>().is_ok())
}

/// `application/x-www-form-urlencoded` of `text`: space as `+`, unreserved bytes kept.
fn query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn the_address_rule() {
        let n = |s| normalize(s);
        assert_eq!(n("   "), None);
        assert_eq!(n("https://docs.rs/wry"), Some("https://docs.rs/wry".into()));
        assert_eq!(n("  HTTP://X.dev "), Some("HTTP://X.dev".into()));
        assert_eq!(n("file:///C:/a.html"), Some("file:///C:/a.html".into()));
        assert_eq!(n("about:blank"), Some("about:blank".into()));
        assert_eq!(n("localhost"), Some("http://localhost".into()));
        assert_eq!(
            n("localhost:5173/path"),
            Some("http://localhost:5173/path".into())
        );
        assert_eq!(n("127.0.0.1:8080"), Some("http://127.0.0.1:8080".into()));
        assert_eq!(n("[::1]:3000"), Some("http://[::1]:3000".into()));
        assert_eq!(n("example.com"), Some("https://example.com".into()));
        assert_eq!(n("docs.rs/wry"), Some("https://docs.rs/wry".into()));
        assert_eq!(
            n("example.com:8080"),
            Some("https://example.com:8080".into())
        );
        let search = |q: &str| Some(format!("https://www.google.com/search?q={q}"));
        assert_eq!(n("foo"), search("foo"));
        assert_eq!(n("two words"), search("two+words"));
        assert_eq!(n("a.b c"), search("a.b+c"));
        assert_eq!(
            n("javascript:alert(1)"),
            search("javascript%3Aalert%281%29")
        );
        assert_eq!(n(r"C:\x.html"), search("C%3A%5Cx.html"));
        assert_eq!(n("c++ & rust?"), search("c%2B%2B+%26+rust%3F"));
    }
}
