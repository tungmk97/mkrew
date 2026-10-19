//! The few HTTP requests `mk web` answers (ADR 0052): the page, a token check and the
//! WebSocket upgrade. GET, and POST for the push routes; every answer closes the connection.

use super::token::Token;

#[derive(Debug)]
pub struct Asset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub body: &'static [u8],
}

const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const WOFF2: &str = "font/woff2";
const PNG: &str = "image/png";

/// The page (Task 2 of the mobile-web plan), embedded in `mk`.
pub const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        content_type: HTML,
        body: include_bytes!("../../web/index.html"),
    },
    Asset {
        path: "/app.css",
        content_type: CSS,
        body: include_bytes!("../../web/app.css"),
    },
    Asset {
        path: "/app.mjs",
        content_type: JS,
        body: include_bytes!("../../web/app.mjs"),
    },
    Asset {
        path: "/model.mjs",
        content_type: JS,
        body: include_bytes!("../../web/model.mjs"),
    },
    Asset {
        path: "/vendor/xterm.mjs",
        content_type: JS,
        body: include_bytes!("../../web/vendor/xterm.mjs"),
    },
    Asset {
        path: "/vendor/xterm.css",
        content_type: CSS,
        body: include_bytes!("../../web/vendor/xterm.css"),
    },
    Asset {
        path: "/vendor/SymbolsNerdFontMono-Regular.woff2",
        content_type: WOFF2,
        body: include_bytes!("../../web/vendor/SymbolsNerdFontMono-Regular.woff2"),
    },
    Asset {
        path: "/apple-touch-icon.png",
        content_type: PNG,
        body: include_bytes!("../../web/apple-touch-icon.png"),
    },
    Asset {
        path: "/sw.js",
        content_type: JS,
        body: include_bytes!("../../web/sw.js"),
    },
];

/// What `mk web` reads from a request head.
#[derive(Debug, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    /// The `t` query parameter.
    pub token: Option<String>,
    /// `Upgrade: websocket` was sent.
    pub upgrade: bool,
    pub websocket_key: Option<String>,
    /// `Content-Length`, 0 when absent or not a number.
    pub content_length: usize,
}

/// Length of the head including its blank line, once the buffer holds all of it.
pub fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

pub fn parse_head(raw: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let target = first.next()?;
    first.next()?.strip_prefix("HTTP/")?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let token = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("t="))
        .map(str::to_owned);
    let mut upgrade = false;
    let mut websocket_key = None;
    let mut content_length = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("upgrade") {
            upgrade = value.eq_ignore_ascii_case("websocket");
        } else if name.eq_ignore_ascii_case("sec-websocket-key") {
            websocket_key = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().unwrap_or(0);
        }
    }
    Some(Head {
        method,
        path: path.to_owned(),
        token,
        upgrade,
        websocket_key,
        content_length,
    })
}

#[derive(Debug)]
pub enum Route {
    Asset(&'static Asset),
    /// `/check` with the right token: the page may open the WebSocket.
    Check,
    /// `/ws` with the right token: answered by [`switching_protocols`], then relayed.
    Upgrade {
        key: String,
    },
    Unauthorized,
    NotFound,
    BadRequest,
    MethodNotAllowed,
    /// `GET /push/key` with the right token: answered with the VAPID public key (or 404 while
    /// `[web] push` is off) by `web.rs`.
    PushKey,
    /// `POST /push/subscribe` / `/push/unsubscribe` with the right token: the body is read by `web.rs`.
    PushSubscribe,
    PushUnsubscribe,
    /// `/manifest.webmanifest` with the right token: the Home Screen app's manifest.
    Manifest(String),
}

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
        "/ws" => match (&head.websocket_key, head.upgrade) {
            (Some(key), true) if authorized => Route::Upgrade { key: key.clone() },
            (Some(_), true) => Route::Unauthorized,
            _ => Route::BadRequest,
        },
        "/index.html" => Route::Asset(&ASSETS[0]),
        path => ASSETS
            .iter()
            .find(|asset| asset.path == path)
            .map_or(Route::NotFound, Route::Asset),
    }
}

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

impl Route {
    /// The whole answer. `Upgrade` is answered by [`switching_protocols`] instead (`handle`
    /// never calls this for it); it falls back to 400.
    pub fn response(&self) -> Vec<u8> {
        match self {
            Route::Asset(asset) => respond(
                "200 OK",
                &[
                    ("Content-Type", asset.content_type),
                    ("Cache-Control", "no-cache"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
                asset.body,
            ),
            Route::Check => respond("204 No Content", &[("Cache-Control", "no-store")], b""),
            Route::Unauthorized => respond("401 Unauthorized", &[], b"wrong or missing token\n"),
            Route::NotFound => respond("404 Not Found", &[], b"not found\n"),
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
            Route::MethodNotAllowed => respond(
                "405 Method Not Allowed",
                &[("Allow", "GET, POST")],
                b"GET or POST only\n",
            ),
        }
    }
}

fn respond(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    // RFC 7230 §3.3.2: a 204 carries no Content-Length.
    if !status.starts_with("204 ") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

/// The upgrade cannot be served: the browser gets the reason instead of a WebSocket.
pub fn unavailable(reason: &str) -> Vec<u8> {
    respond(
        "503 Service Unavailable",
        &[],
        format!("{reason}\n").as_bytes(),
    )
}

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

pub fn switching_protocols(key: &str) -> Vec<u8> {
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> Token {
        Token::parse(&"ab".repeat(32)).unwrap()
    }

    fn head(text: &str) -> Head {
        parse_head(text.as_bytes()).unwrap()
    }

    #[test]
    fn finds_the_end_of_the_head() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n\r\nrest"), Some(27));
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n"), None);
    }

    #[test]
    fn parses_an_upgrade_request() {
        let text = format!(
            "GET /ws?x=1&t={} HTTP/1.1\r\nHost: pc\r\nUpgrade: WebSocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
            "ab".repeat(32)
        );
        assert_eq!(
            head(&text),
            Head {
                method: "GET".into(),
                path: "/ws".into(),
                token: Some("ab".repeat(32)),
                upgrade: true,
                websocket_key: Some("dGhlIHNhbXBsZSBub25jZQ==".into()),
                content_length: 0,
            }
        );
        assert_eq!(parse_head(b"\xff\xfe"), None);
        assert_eq!(parse_head(b"GARBAGE\r\n\r\n"), None);
    }

    #[test]
    fn push_and_manifest_routes_need_the_token() {
        let good = "ab".repeat(32);
        let get = |path: &str| route(&head(&format!("GET {path} HTTP/1.1\r\n\r\n")), &token());
        let post = |path: &str| {
            route(
                &head(&format!(
                    "POST {path} HTTP/1.1\r\nContent-Length: 2\r\n\r\n"
                )),
                &token(),
            )
        };
        assert!(matches!(
            get(&format!("/push/key?t={good}")),
            Route::PushKey
        ));
        assert!(matches!(get("/push/key"), Route::Unauthorized));
        assert!(matches!(
            post(&format!("/push/subscribe?t={good}")),
            Route::PushSubscribe
        ));
        assert!(matches!(
            post(&format!("/push/unsubscribe?t={good}")),
            Route::PushUnsubscribe
        ));
        assert!(matches!(post("/push/subscribe"), Route::Unauthorized));
        assert!(matches!(
            post(&format!("/push/key?t={good}")),
            Route::MethodNotAllowed
        ));
        assert!(matches!(
            get(&format!("/push/subscribe?t={good}")),
            Route::NotFound
        ));
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
        assert_eq!(
            head("POST /x HTTP/1.1\r\ncontent-length: 12\r\n\r\n").content_length,
            12
        );
        assert_eq!(head("POST /x HTTP/1.1\r\n\r\n").content_length, 0);
        assert_eq!(
            head("POST /x HTTP/1.1\r\nContent-Length: abc\r\n\r\n").content_length,
            0
        );
    }

    #[test]
    fn plain_answers_are_never_cached() {
        let text = String::from_utf8(plain("404 Not Found", "off\n")).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"), "{text}");
        assert!(text.contains("Cache-Control: no-store\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\noff\n"), "{text}");
    }

    #[test]
    fn serves_the_page_and_its_files() {
        for (path, kind) in [
            ("/", "text/html; charset=utf-8"),
            ("/index.html", "text/html; charset=utf-8"),
            ("/app.mjs", "text/javascript; charset=utf-8"),
            ("/vendor/xterm.css", "text/css; charset=utf-8"),
            ("/vendor/SymbolsNerdFontMono-Regular.woff2", "font/woff2"),
        ] {
            match route(&head(&format!("GET {path} HTTP/1.1\r\n\r\n")), &token()) {
                Route::Asset(asset) => assert_eq!(asset.content_type, kind, "{path}"),
                other => panic!("{path}: {other:?}"),
            }
        }
        assert!(matches!(
            route(&head("GET /nope HTTP/1.1\r\n\r\n"), &token()),
            Route::NotFound
        ));
        assert!(matches!(
            route(&head("POST / HTTP/1.1\r\n\r\n"), &token()),
            Route::MethodNotAllowed
        ));
    }

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

    #[test]
    fn routes_check_the_token() {
        let good = "ab".repeat(32);
        let bad = "cd".repeat(32);
        let check = |query: &str| {
            route(
                &head(&format!("GET /check{query} HTTP/1.1\r\n\r\n")),
                &token(),
            )
        };
        assert!(matches!(check(&format!("?t={good}")), Route::Check));
        assert!(matches!(check(&format!("?t={bad}")), Route::Unauthorized));
        assert!(matches!(check(""), Route::Unauthorized));
        let upgrade = |t: &str| {
            route(
                &head(&format!(
                    "GET /ws?t={t} HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k==\r\n\r\n"
                )),
                &token(),
            )
        };
        assert!(matches!(upgrade(&good), Route::Upgrade { key } if key == "k=="));
        assert!(matches!(upgrade(&bad), Route::Unauthorized));
        let plain = route(
            &head(&format!("GET /ws?t={good} HTTP/1.1\r\n\r\n")),
            &token(),
        );
        assert!(matches!(plain, Route::BadRequest));
    }

    #[test]
    fn answers_with_status_length_and_close() {
        let text = String::from_utf8(Route::Unauthorized.response()).unwrap();
        assert!(text.starts_with("HTTP/1.1 401 Unauthorized\r\n"), "{text}");
        assert!(
            text.contains("Content-Length: 23\r\nConnection: close\r\n"),
            "{text}"
        );
        assert!(text.ends_with("\r\n\r\nwrong or missing token\n"), "{text}");
    }

    #[test]
    fn unavailable_says_why() {
        let text = String::from_utf8(unavailable("daemon is down")).unwrap();
        assert!(
            text.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "{text}"
        );
        assert!(text.contains("Content-Length: 15\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\ndaemon is down\n"), "{text}");
    }

    #[test]
    fn no_content_carries_no_length() {
        // RFC 7230 §3.3.2: a 204 has no Content-Length.
        let text = String::from_utf8(Route::Check.response()).unwrap();
        assert!(text.starts_with("HTTP/1.1 204 No Content\r\n"), "{text}");
        assert!(
            !text.to_ascii_lowercase().contains("content-length"),
            "{text}"
        );
        assert!(text.contains("Connection: close\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\n"), "{text}");
    }

    #[test]
    fn accepts_the_upgrade_with_the_rfc_6455_key() {
        let text = String::from_utf8(switching_protocols("dGhlIHNhbXBsZSBub25jZQ==")).unwrap();
        assert!(
            text.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
            "{text}"
        );
        assert!(
            text.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"),
            "{text}"
        );
        assert!(text.ends_with("\r\n\r\n"));
    }
}
