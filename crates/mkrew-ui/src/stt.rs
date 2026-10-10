//! Speech to text for voice input (SPEC §20G): one `POST {base_url}/audio/transcriptions`
//! (OpenAI-compatible: Groq, OpenAI, local servers) with the WAV as multipart form data.

use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use mkrew_core::settings::VoiceSettings;

/// The whole call (upload and model) gives up after this.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The API key from the environment variable `api_key_env` names (`var` reads it); `None` when
/// the name is empty (a local server without keys).
pub fn api_key(
    settings: &VoiceSettings,
    var: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>> {
    let name = settings.api_key_env.trim();
    if name.is_empty() {
        return Ok(None);
    }
    match var(name).map(|key| key.trim().to_owned()) {
        Some(key) if !key.is_empty() => Ok(Some(key)),
        _ => bail!("set the {name} environment variable to your API key, then restart Mkrew"),
    }
}

pub fn url(base_url: &str) -> String {
    format!("{}/audio/transcriptions", base_url.trim_end_matches('/'))
}

/// The multipart form: model, json answer, temperature 0, language and prompt when set, then
/// the WAV as `file`.
pub fn body(settings: &VoiceSettings, wav: &[u8], boundary: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(wav.len() + 1024);
    let mut field = |name: &str, value: &str| {
        if !value.is_empty() {
            out.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
    };
    field("model", settings.model.trim());
    field("response_format", "json");
    field("temperature", "0");
    field("language", settings.language.trim());
    field("prompt", settings.prompt.trim());
    out.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(wav);
    out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    out
}

/// The text of a 2xx answer, or the API's reason for any other status.
pub fn parse(status: u16, body: &str) -> Result<String> {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    if (200..300).contains(&status) {
        let text = json
            .as_ref()
            .and_then(|json| json.get("text"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("unexpected answer: {}", short(body)))?;
        return Ok(clean(text));
    }
    let message = json
        .as_ref()
        .and_then(|json| json.pointer("/error/message"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| short(body), str::to_owned);
    bail!("HTTP {status}: {message}")
}

/// The first 200 chars of `body`, trimmed.
fn short(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &body[..end]),
        None => body.to_owned(),
    }
}

/// Whisper's text as one line with single spaces: a line break typed into an agent would be
/// Enter (send).
pub fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sends `wav` for speech to text. Blocking: run it off the UI thread.
pub fn transcribe(settings: &VoiceSettings, api_key: Option<&str>, wav: &[u8]) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // 4xx / 5xx bodies carry the reason (bad key, rate limit): read them.
        .http_status_as_error(false)
        .build()
        .into();
    let boundary = format!("mkrew-{}", ulid::Ulid::new());
    let url = url(&settings.base_url);
    let mut request = agent.post(url.as_str()).header(
        "Content-Type",
        format!("multipart/form-data; boundary={boundary}"),
    );
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let mut response = request
        .send(&body(settings, wav, &boundary)[..])
        .with_context(|| format!("cannot reach {url}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("cannot read the answer")?;
    parse(status, &text)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};

    use super::*;

    #[test]
    fn api_key_comes_from_the_named_variable() {
        let settings = VoiceSettings::default();
        let key = api_key(&settings, |name| {
            (name == "GROQ_API_KEY").then(|| " gsk_1 \n".to_owned())
        });
        assert_eq!(key.unwrap(), Some("gsk_1".to_owned()));
        let missing = api_key(&settings, |_| None).unwrap_err().to_string();
        assert!(missing.contains("GROQ_API_KEY"), "{missing}");
        let blank = api_key(&settings, |_| Some("  ".into()))
            .unwrap_err()
            .to_string();
        assert!(blank.contains("GROQ_API_KEY"), "{blank}");
        let local = VoiceSettings {
            api_key_env: String::new(),
            ..VoiceSettings::default()
        };
        assert_eq!(
            api_key(&local, |_| panic!("no variable is read")).unwrap(),
            None
        );
    }

    #[test]
    fn url_joins_with_or_without_a_trailing_slash() {
        assert_eq!(
            url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            url("http://127.0.0.1:8000/v1/"),
            "http://127.0.0.1:8000/v1/audio/transcriptions"
        );
    }

    #[test]
    fn body_has_the_set_fields_then_the_wav() {
        let settings = VoiceSettings {
            prompt: "Mkrew".into(),
            ..VoiceSettings::default()
        };
        let text = String::from_utf8(body(&settings, b"WAVDATA", "B")).unwrap();
        assert_eq!(
            text,
            "--B\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n\
             --B\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\njson\r\n\
             --B\r\nContent-Disposition: form-data; name=\"temperature\"\r\n\r\n0\r\n\
             --B\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nMkrew\r\n\
             --B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\nWAVDATA\r\n--B--\r\n"
        );
        let forced = VoiceSettings {
            language: "vi".into(),
            ..VoiceSettings::default()
        };
        let text = String::from_utf8(body(&forced, b"", "B")).unwrap();
        assert!(text.contains("name=\"language\"\r\n\r\nvi\r\n"), "{text}");
    }

    #[test]
    fn answers_become_text_or_the_apis_reason() {
        assert_eq!(
            parse(200, r#"{"text":" Xin chào,\nrun the tests. "}"#).unwrap(),
            "Xin chào, run the tests."
        );
        assert_eq!(parse(200, r#"{"text":""}"#).unwrap(), "");
        let bad_key = parse(
            401,
            r#"{"error":{"message":"Invalid API Key","type":"invalid_request_error"}}"#,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(bad_key, "HTTP 401: Invalid API Key");
        let gateway = parse(502, " <html>Bad Gateway</html>\n")
            .unwrap_err()
            .to_string();
        assert_eq!(gateway, "HTTP 502: <html>Bad Gateway</html>");
        let odd = parse(200, "{}").unwrap_err().to_string();
        assert!(odd.starts_with("unexpected answer"), "{odd}");
        let long = parse(500, &"x".repeat(500)).unwrap_err().to_string();
        assert_eq!(long, format!("HTTP 500: {}…", "x".repeat(200)));
    }

    /// Loopback only (`127.0.0.1`, SPEC §16): the exact request a real provider receives.
    #[test]
    fn transcribe_posts_multipart_with_the_key_and_reads_the_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let line = line.to_ascii_lowercase();
                    let value = line.strip_prefix("content-length:")?.trim().to_owned();
                    value.parse().ok()
                })
                .unwrap();
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let answer = r#"{"text":" Sửa lỗi này, then run the tests."}"#;
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{answer}",
                answer.len()
            )
            .unwrap();
            (head, body)
        });
        let settings = VoiceSettings {
            base_url: format!("http://127.0.0.1:{port}/v1/"),
            ..VoiceSettings::default()
        };
        let text = transcribe(&settings, Some("gsk_test"), b"RIFFfake").unwrap();
        assert_eq!(text, "Sửa lỗi này, then run the tests.");
        let (head, body) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(
            head.starts_with("post /v1/audio/transcriptions http/1.1\r\n"),
            "{head}"
        );
        assert!(
            head.contains("authorization: bearer gsk_test\r\n"),
            "{head}"
        );
        assert!(
            head.contains("content-type: multipart/form-data; boundary=mkrew-"),
            "{head}"
        );
        let body = String::from_utf8_lossy(&body);
        assert!(
            body.contains("name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n"),
            "{body}"
        );
        assert!(
            body.contains("filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFfake\r\n"),
            "{body}"
        );
    }
}
