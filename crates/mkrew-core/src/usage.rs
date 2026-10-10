//! Token usage from agent transcripts (SPEC §20E.3), read incrementally.
//!
//! Pure: the daemon reads the bytes appended to a transcript since [`UsageReader::offset`] and
//! hands them to [`UsageReader::feed`]. A partial last line is kept until its newline arrives.
//! Lines that are not JSON are skipped silently; JSON that looks like a usage line but lacks the
//! token fields gives one warning (format changed).

use std::collections::HashMap;

use serde_json::Value;

use crate::profile::UsageFormat;

/// Token totals of one agent session. The four counts add up to every token of the
/// conversation (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage {
    /// Fresh input tokens over the whole session: the cache is NOT in here.
    pub input: u64,
    /// Input tokens served from the prompt cache.
    pub cache_read: u64,
    /// Input tokens written to the prompt cache (Codex has none).
    pub cache_write: u64,
    /// Output tokens over the whole session.
    pub output: u64,
    /// Context size of the newest message.
    pub context: u64,
    /// The model of the newest message; `None` while no line named one.
    pub model: Option<String>,
}

impl Usage {
    /// `context` as a whole percentage of `window` (rounded down); `None` without a window.
    pub fn context_percent(&self, window: Option<u64>) -> Option<u64> {
        window
            .filter(|w| *w > 0)
            .map(|w| self.context.saturating_mul(100) / w)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Claude,
    Codex,
    Omp,
}

/// Incremental reader for one transcript file.
#[derive(Debug, Clone)]
pub struct UsageReader {
    format: Format,
    /// Bytes of the file consumed so far, including `partial`.
    offset: u64,
    partial: Vec<u8>,
    usage: Option<Usage>,
    /// The model of the newest message that names one (Codex: the newest `turn_context`).
    /// Not in `usage`: Codex names it on another line than the counts. [`Self::usage`] joins them.
    model: Option<String>,
    /// Claude repeats one API message's usage on every content-block line: the counts last
    /// added per `message.id` + `requestId` (`[input, cache write, cache read, output]`),
    /// replaced (not added) on a repeat. Kept for the whole session, not just the last key,
    /// since sidechain lines can interleave.
    seen: HashMap<String, [u64; 4]>,
    warning: Option<String>,
    warned: bool,
}

impl UsageReader {
    /// A reader for `format`; an unknown format gives the one warning to log instead.
    pub fn new(format: &UsageFormat) -> Result<Self, String> {
        let format = match format {
            UsageFormat::ClaudeJsonl => Format::Claude,
            UsageFormat::CodexJsonl => Format::Codex,
            UsageFormat::OmpJsonl => Format::Omp,
            UsageFormat::Unknown(name) => {
                return Err(format!("unknown usage format `{name}`: no usage shown"));
            }
        };
        Ok(Self {
            format,
            offset: 0,
            partial: Vec::new(),
            usage: None,
            model: None,
            seen: HashMap::new(),
            warning: None,
            warned: false,
        })
    }

    /// Where the next read of the file starts.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The totals so far; `None` until a usage line was seen.
    pub fn usage(&self) -> Option<Usage> {
        let usage = self.usage.clone()?;
        Some(Usage {
            model: self.model.clone(),
            ..usage
        })
    }

    /// The warning about an unrecognised line, at most once per reader.
    pub fn take_warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    /// Consumes bytes appended to the file at [`Self::offset`] and returns the totals so far.
    pub fn feed(&mut self, appended: &[u8]) -> Option<Usage> {
        self.offset += appended.len() as u64;
        self.partial.extend_from_slice(appended);
        let Some(end) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return self.usage();
        };
        let rest = self.partial.split_off(end + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        for line in complete.split(|b| *b == b'\n') {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            let parsed = match self.format {
                Format::Claude => self.claude_line(&value),
                Format::Codex => self.codex_line(&value),
                Format::Omp => self.omp_line(&value),
            };
            match parsed {
                Line::Other => {}
                Line::Usage(usage) => self.usage = Some(usage),
                Line::Broken(what) => self.warn(what),
            }
        }
        self.usage()
    }

    /// Remembers `model` as the newest one (no allocation while it is unchanged).
    fn set_model(&mut self, model: Option<&str>) {
        if let Some(model) = model.filter(|m| !m.is_empty() && self.model.as_deref() != Some(m)) {
            self.model = Some(model.to_owned());
        }
    }

    fn warn(&mut self, what: &str) {
        if !self.warned {
            self.warned = true;
            self.warning = Some(format!("usage transcript format changed? {what}"));
        }
    }

    /// `type: "assistant"` lines carry `message.usage`. `input` counts `input_tokens` only;
    /// cache creation and cache read are `cache_write` and `cache_read`. Context is the sum of
    /// the three on the newest non-sidechain message (without its output, unlike Codex's), so it
    /// is what it was before the split. The model is `message.model` of that same message.
    fn claude_line(&mut self, value: &Value) -> Line {
        if value["type"] != "assistant" {
            return Line::Other;
        }
        let message = &value["message"];
        if message["model"] == "<synthetic>" {
            return Line::Other;
        }
        let usage = &message["usage"];
        let (Some(fresh), Some(output)) = (
            usage["input_tokens"].as_u64(),
            usage["output_tokens"].as_u64(),
        ) else {
            return Line::Broken("assistant line without message.usage token counts");
        };
        let cache = |key: &str| usage[key].as_u64().unwrap_or(0);
        // [input, cache write, cache read, output]
        let counts = [
            fresh,
            cache("cache_creation_input_tokens"),
            cache("cache_read_input_tokens"),
            output,
        ];

        // `model` of `self.usage` is always `None` (see `UsageReader::model`): this clone
        // allocates nothing.
        let mut total = self.usage.clone().unwrap_or_default();
        let key = format!(
            "{}\n{}",
            message["id"].as_str().unwrap_or_default(),
            value["requestId"].as_str().unwrap_or_default()
        );
        // A line without ids cannot be matched to its siblings: count it on its own.
        let key = (key != "\n").then_some(key);
        if let Some(old) = key.as_ref().and_then(|k| self.seen.get(k)).copied() {
            total.input -= old[0];
            total.cache_write -= old[1];
            total.cache_read -= old[2];
            total.output -= old[3];
        }
        if let Some(key) = key {
            self.seen.insert(key, counts);
        }
        total.input += counts[0];
        total.cache_write += counts[1];
        total.cache_read += counts[2];
        total.output += counts[3];
        if value["isSidechain"] != true {
            total.context = counts[0] + counts[1] + counts[2];
            self.set_model(message["model"].as_str());
        }
        Line::Usage(total)
    }

    /// `event_msg` lines with `payload.type: "token_count"` carry cumulative totals in
    /// `info.total_token_usage` (`info` is null before the first answer); context is
    /// `info.last_token_usage.total_tokens`, what Codex itself counts as in the window. Its
    /// `input_tokens` INCLUDES `cached_input_tokens`: fresh input is the difference. The model
    /// is `payload.model` of the newest `turn_context` line.
    fn codex_line(&mut self, value: &Value) -> Line {
        let payload = &value["payload"];
        if value["type"] == "turn_context" {
            self.set_model(payload["model"].as_str());
            return Line::Other;
        }
        if value["type"] != "event_msg" || payload["type"] != "token_count" {
            return Line::Other;
        }
        let info = &payload["info"];
        if info.is_null() {
            return Line::Other;
        }
        let total = &info["total_token_usage"];
        match (
            total["input_tokens"].as_u64(),
            total["output_tokens"].as_u64(),
            info["last_token_usage"]["total_tokens"].as_u64(),
        ) {
            (Some(input), Some(output), Some(context)) => {
                let cached = total["cached_input_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(input);
                Line::Usage(Usage {
                    input: input - cached,
                    cache_read: cached,
                    output,
                    context,
                    ..Usage::default()
                })
            }
            _ => Line::Broken("token_count line without total/last token usage"),
        }
    }

    /// `type: "message"` lines with `message.role: "assistant"` carry that API call's own
    /// `message.usage`; the calls are added up. `input` is already without the cache
    /// (`cacheRead` / `cacheWrite`), `cost` is not read. Context is input + both cache counts of
    /// the newest call (without its output, like Claude). A call with no tokens at all (aborted,
    /// failed) is not a reading: it keeps the context and the model. `model_change` lines are
    /// ignored: after a fallback switch the next call can still name the old model, so only
    /// `message.model` of a counted call says what answered.
    fn omp_line(&mut self, value: &Value) -> Line {
        let message = &value["message"];
        if value["type"] != "message" || message["role"] != "assistant" {
            return Line::Other;
        }
        let usage = &message["usage"];
        let (Some(input), Some(output)) = (usage["input"].as_u64(), usage["output"].as_u64())
        else {
            return Line::Broken("assistant line without message.usage token counts");
        };
        let cache = |key: &str| usage[key].as_u64().unwrap_or(0);
        let (cache_read, cache_write) = (cache("cacheRead"), cache("cacheWrite"));
        if input + output + cache_read + cache_write == 0 {
            return Line::Other;
        }
        let mut total = self.usage.clone().unwrap_or_default();
        total.input += input;
        total.output += output;
        total.cache_read += cache_read;
        total.cache_write += cache_write;
        total.context = input + cache_read + cache_write;
        self.set_model(message["model"].as_str());
        Line::Usage(total)
    }
}

enum Line {
    /// Not a usage line.
    Other,
    /// New totals.
    Usage(Usage),
    /// Looks like a usage line but the fields are not where they should be.
    Broken(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../tests/fixtures/usage/claude.jsonl");
    const CODEX: &[u8] = include_bytes!("../tests/fixtures/usage/codex.jsonl");
    const OMP: &[u8] = include_bytes!("../tests/fixtures/usage/omp.jsonl");

    /// Two content-block lines of one message counted once, the sidechain line in the totals
    /// but not the context, the synthetic line ignored. `input` is fresh input only; the cache
    /// is split out. Context is unchanged: input + cache write + cache read of the newest
    /// main-thread message (so the ctx % did not move with the split).
    fn claude_usage() -> Usage {
        Usage {
            input: 2 + 3 + 1,
            cache_read: 48215 + 9000 + 48903,
            cache_write: 688 + 1200,
            output: 206 + 40 + 512,
            context: 1 + 1200 + 48903,
            // The sidechain line (haiku) is between the two opus messages and never counts.
            model: Some("claude-opus-5".into()),
        }
    }

    /// The newest cumulative totals, not their sum. Codex's `input_tokens` includes the cached
    /// ones: fresh = 51122 - 26880. Context = last `total_tokens`; model = `turn_context`.
    fn codex_usage() -> Usage {
        Usage {
            input: 51122 - 26880,
            cache_read: 26880,
            cache_write: 0,
            output: 1561,
            context: 35579,
            model: Some("gpt-5.5-codex".into()),
        }
    }

    /// Per-call usage is added up; `input` is already fresh in omp. Context = input + cache
    /// read + cache write of the newest counted call (without its output, like Claude). The
    /// aborted zero-usage call at the end, and the `model_change` line, change nothing.
    fn omp_usage() -> Usage {
        Usage {
            input: 2 + 2 + 1,
            cache_read: 45333 + 72843 + 74000,
            cache_write: 2002 + 4748 + 120,
            output: 94 + 77 + 310,
            context: 1 + 74000 + 120,
            model: Some("claude-sonnet-5-5".into()),
        }
    }

    fn fixtures() -> [(UsageFormat, &'static [u8], Usage); 3] {
        [
            (UsageFormat::ClaudeJsonl, CLAUDE, claude_usage()),
            (UsageFormat::CodexJsonl, CODEX, codex_usage()),
            (UsageFormat::OmpJsonl, OMP, omp_usage()),
        ]
    }

    fn reader(format: &UsageFormat) -> UsageReader {
        UsageReader::new(format).unwrap()
    }

    #[test]
    fn fixtures_give_the_expected_totals() {
        for (format, bytes, expected) in fixtures() {
            let mut r = reader(&format);
            assert_eq!(r.feed(bytes), Some(expected), "{format:?}");
            assert_eq!(r.offset(), bytes.len() as u64);
            assert_eq!(r.take_warning(), None, "{format:?}");
        }
    }

    #[test]
    fn reading_in_any_chunks_gives_the_same_totals() {
        for (format, bytes, expected) in fixtures() {
            for size in [1, 7, 100, 1000] {
                let mut r = reader(&format);
                let mut fed = 0;
                for chunk in bytes.chunks(size) {
                    assert_eq!(r.offset(), fed);
                    r.feed(chunk);
                    fed += chunk.len() as u64;
                }
                assert_eq!(
                    r.usage(),
                    Some(expected.clone()),
                    "{format:?} in chunks of {size}"
                );
                assert_eq!(r.offset(), bytes.len() as u64);
            }
        }
    }

    #[test]
    fn partial_last_line_waits_for_its_newline() {
        let line = br#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5,"output_tokens":2},"last_token_usage":{"total_tokens":7}}}}"#;
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert_eq!(r.feed(line), None);
        assert_eq!(r.offset(), line.len() as u64);
        let usage = Usage {
            input: 5,
            output: 2,
            context: 7,
            ..Usage::default()
        };
        assert_eq!(r.feed(b"\r\n"), Some(usage.clone()));
        assert_eq!(r.feed(b"{\"type\":"), Some(usage));
    }

    #[test]
    fn turns_appended_later_add_up() {
        let (first, second) = CLAUDE.split_at(CLAUDE.len() / 2);
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        let early = r.feed(first).unwrap();
        assert!(early.output < claude_usage().output, "{early:?}");
        assert_eq!(r.feed(second), Some(claude_usage()));
    }

    /// The model is the newest main-thread one: a sidechain (sub-agent) line and a synthetic
    /// line after it do not replace it.
    #[test]
    fn claude_model_is_the_newest_main_thread_one() {
        let line = |side: bool, model: &str, id: &str| {
            format!(
                "{{\"type\":\"assistant\",\"isSidechain\":{side},\"requestId\":\"{id}\",\"message\":{{\"id\":\"{id}\",\"model\":\"{model}\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n"
            )
        };
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        r.feed(line(false, "claude-opus-5", "a").as_bytes());
        r.feed(line(true, "claude-haiku-4-5", "b").as_bytes());
        r.feed(line(false, "<synthetic>", "c").as_bytes());
        let usage = r.usage().unwrap();
        assert_eq!(usage.model.as_deref(), Some("claude-opus-5"));
        // Both non-synthetic lines are in the totals; the synthetic one is not.
        assert_eq!((usage.input, usage.output), (2, 2));
        r.feed(line(false, "claude-sonnet-5", "d").as_bytes());
        assert_eq!(r.usage().unwrap().model.as_deref(), Some("claude-sonnet-5"));
    }

    /// Codex names the model on `turn_context`, before the turn's counts: the newest one wins,
    /// a model alone is no usage, and cached tokens leave `input`.
    #[test]
    fn codex_model_is_the_newest_turn_context_and_cached_tokens_leave_input() {
        let turn = |model: &str| {
            format!("{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"{model}\"}}}}\n")
        };
        let count = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":60,\"output_tokens\":5},\"last_token_usage\":{\"total_tokens\":105}}}}\n";
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert_eq!(r.feed(turn("gpt-a").as_bytes()), None);
        r.feed(count.as_bytes());
        let usage = r.feed(turn("gpt-b").as_bytes()).unwrap();
        assert_eq!(
            (
                usage.input,
                usage.cache_read,
                usage.cache_write,
                usage.model.as_deref()
            ),
            (40, 60, 0, Some("gpt-b"))
        );
    }

    #[test]
    fn unknown_format_is_no_reader_and_one_warning() {
        let err = UsageReader::new(&UsageFormat::Unknown("gemini".into())).unwrap_err();
        assert!(err.contains("gemini"), "{err}");
    }

    /// An aborted or failed call has zero tokens: it is not a reading (no ctx 0 %, no model).
    #[test]
    fn omp_calls_that_used_no_tokens_are_not_usage() {
        let aborted = br#"{"type":"message","message":{"role":"assistant","model":"claude-opus-5-5","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0},"stopReason":"aborted"}}"#;
        let mut r = reader(&UsageFormat::OmpJsonl);
        assert_eq!(r.feed(&[aborted.as_slice(), b"\n"].concat()), None);
        assert_eq!(r.take_warning(), None);
    }

    #[test]
    fn changed_format_warns_once() {
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        let broken = b"{\"type\":\"assistant\",\"message\":{\"tokens\":3}}\n";
        assert_eq!(r.feed(broken), None);
        assert!(r.take_warning().unwrap().contains("message.usage"));
        r.feed(broken);
        assert_eq!(r.take_warning(), None);

        let mut r = reader(&UsageFormat::CodexJsonl);
        r.feed(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{}}}\nnot json\n");
        assert!(r.take_warning().unwrap().contains("token_count"));
        assert_eq!(r.usage(), None);

        let mut r = reader(&UsageFormat::OmpJsonl);
        r.feed(b"{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"model\":\"m\"}}\n");
        assert!(r.take_warning().unwrap().contains("message.usage"));
        assert_eq!(r.usage(), None);
    }

    #[test]
    fn context_percent_needs_a_window() {
        let usage = Usage {
            context: 124_000,
            ..Usage::default()
        };
        assert_eq!(usage.context_percent(Some(200_000)), Some(62));
        assert_eq!(usage.context_percent(None), None);
        assert_eq!(usage.context_percent(Some(0)), None);
    }
}
