//! The `mk web` access token (ADR 0051): 32 random bytes as 64 lowercase hex characters in
//! `<config>/web_token`, required by `/check` and `/ws`.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, anyhow};

pub struct Token(String);

impl Token {
    /// A token as stored and typed: exactly 64 lowercase hex characters.
    pub fn parse(text: &str) -> Option<Self> {
        let valid =
            text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares in time independent of where the first difference is.
    pub fn matches(&self, candidate: Option<&str>) -> bool {
        let Some(candidate) = candidate else {
            return false;
        };
        let (ours, theirs) = (self.0.as_bytes(), candidate.as_bytes());
        ours.len() == theirs.len()
            && ours
                .iter()
                .zip(theirs)
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

/// The stored token, or a new one when `renew`, the file is missing or holds no valid token.
pub fn load_or_create(file: &Path, renew: bool) -> anyhow::Result<Token> {
    if !renew
        && let Some(token) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| Token::parse(text.trim()))
    {
        return Ok(token);
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness from the OS: {e}"))?;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    write_private(file, &text).with_context(|| format!("write {}", file.display()))?;
    Ok(Token(text))
}

/// Owner-only on Unix; the per-user config folder already is on Windows.
fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(file)?
            .write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(file, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_lowercase_hex() {
        assert!(Token::parse(&"a1".repeat(32)).is_some());
        assert!(Token::parse(&"A1".repeat(32)).is_none());
        assert!(Token::parse(&"a1".repeat(31)).is_none());
        assert!(Token::parse(&format!("{}g", "a".repeat(63))).is_none());
    }

    #[test]
    fn matches_only_the_same_token() {
        let token = Token::parse(&"0f".repeat(32)).unwrap();
        assert!(token.matches(Some(&"0f".repeat(32))));
        assert!(!token.matches(Some(&"0e".repeat(32))));
        assert!(!token.matches(Some("0f")));
        assert!(!token.matches(None));
    }

    #[test]
    fn load_or_create_keeps_the_token_until_renewed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config").join("web_token");
        let first = load_or_create(&file, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), first.as_str());
        assert_eq!(
            load_or_create(&file, false).unwrap().as_str(),
            first.as_str()
        );
        let renewed = load_or_create(&file, true).unwrap();
        assert_ne!(renewed.as_str(), first.as_str());
        // A hand-broken file is replaced instead of being used.
        std::fs::write(&file, "not a token").unwrap();
        let replaced = load_or_create(&file, false).unwrap();
        assert!(Token::parse(replaced.as_str()).is_some());
    }

    #[test]
    fn created_tokens_are_lowercase_hex() {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..8 {
            let token = load_or_create(&dir.path().join("web_token"), true).unwrap();
            let text = token.as_str();
            assert_eq!(text.len(), 64, "{text}");
            assert!(
                text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                "{text}"
            );
        }
    }
}
