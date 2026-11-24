//! `mkrew-fakeagent [--scenario <file|name>] [--resume <id>] [--listen] [--tsp <fixture.jsonl> [--record <file>]] [<prompt>]`: runs a
//! scenario (SPEC §16).
//!
//! The scenario comes from `--scenario`, else `MKREW_FAKE_SCENARIO`, else the built-in `ask`.
//! `--resume <id>` (a profile's `resume` command) is printed first as `Resumed: <id>`, then a
//! `<prompt>` (a task prompt passed with `prompt_mode = "arg"`) as `Task: <prompt>`.
//! `--listen` binds `127.0.0.1:0`, prints `Listening on http://127.0.0.1:<port>/` and waits until killed (a dev server for the port tests).
//! `--tsp <fixture.jsonl>` plays omp's native-surface protocol (TSP, spec 2026-11-23-omp-tsp):
//! raw console input, a hello query, a wait of up to 10 s for the terminal's hello reply, then
//! every `out` line of the fixture as an APC message. `--record <file>` receives every byte read
//! from stdin (the terminal's reply, acks and events). `--hello-delay-ms <n>` prints a line and waits `n` ms before the hello.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use mkrew_core::tsp::wire;
use mkrew_fakeagent::{DEFAULT_SCENARIO, SCENARIO_ENV, load, run};

const USAGE: &str = "usage: mkrew-fakeagent [--scenario <file|name>] [--resume <id>] [--listen] [--tsp <fixture.jsonl> [--record <file>] [--hello-delay-ms <n>]] [<prompt>]";

/// What `--tsp` sends first; omp sends this (then a DA1 query) when it starts.
const TSP_HELLO: &[u8] =
    b"\x1b_tsp;q;{\"q\":\"hello\",\"v\":[1],\"app\":\"fake\",\"features\":[]}\x1b\\";
/// The start of the terminal's hello reply (`ESC _ tsp ; r ;`).
const TSP_REPLY: &[u8] = b"\x1b_tsp;r;";
/// How long `--tsp` waits for the hello reply before it stays a plain program.
const TSP_REPLY_WAIT: Duration = Duration::from_secs(10);

fn main() {
    // VT processing for colors when attached to a Windows console; no-op elsewhere.
    let _ = anstyle_query::windows::enable_ansi_colors();
    match real_main() {
        Ok(code) => process::exit(code),
        Err(err) => {
            eprintln!("mkrew-fakeagent: {err:#}");
            process::exit(2);
        }
    }
}

fn real_main() -> Result<i32> {
    let args = parse_args(std::env::args().skip(1))?;
    if args.listen {
        return listen();
    }
    if let Some(fixture) = &args.tsp {
        return tsp(
            Path::new(fixture),
            args.record.as_deref().map(Path::new),
            args.hello_delay,
        );
    }
    let spec = match args.scenario {
        Some(spec) => spec,
        None => std::env::var(SCENARIO_ENV).unwrap_or_else(|_| DEFAULT_SCENARIO.to_owned()),
    };
    let scenario = load(&spec)?;
    let mut out = BufWriter::new(io::stdout().lock());
    if let Some(id) = args.resume {
        writeln!(out, "Resumed: {id}")?;
    }
    if let Some(prompt) = args.prompt {
        writeln!(out, "Task: {prompt}")?;
    }
    Ok(run(&scenario, &mut io::stdin().lock(), &mut out)?)
}

/// `--listen`: a stand-in dev server for the daemon's port tests; never returns.
fn listen() -> Result<i32> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "Listening on http://127.0.0.1:{}/",
        listener.local_addr()?.port()
    )?;
    out.flush()?;
    loop {
        // Connections are dropped at once: only the open port matters.
        let _ = listener.accept();
    }
}

/// `--tsp`: prints the hello query, records what the terminal writes back, and once the
/// hello reply is in, replays the fixture's `out` messages; never returns.
fn tsp(fixture: &Path, record: Option<&Path>, hello_delay: Option<Duration>) -> Result<i32> {
    let text = std::fs::read_to_string(fixture)
        .with_context(|| format!("read fixture {}", fixture.display()))?;
    let messages = fixture_messages(&text)?;
    let mut record = record
        .map(|path| File::create(path).with_context(|| format!("create {}", path.display())))
        .transpose()?;
    raw_console::enable();

    // Every stdin byte, for the reply check here and (through `record`) for the test.
    let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader_seen = Arc::clone(&seen);
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 {
                break;
            }
            if let Some(file) = &mut record {
                let _ = file.write_all(&buf[..n]).and_then(|()| file.flush());
            }
            reader_seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(&buf[..n]);
        }
    });

    let mut out = io::stdout().lock();
    if let Some(delay) = hello_delay {
        // Output first, so the daemon's hello wait has started; omp loading a session does the same.
        out.write_all(b"fake tsp: starting\r\n")?;
        out.flush()?;
        thread::sleep(delay);
    }
    out.write_all(TSP_HELLO)?;
    out.flush()?;
    let deadline = Instant::now() + TSP_REPLY_WAIT;
    let replied = loop {
        let got = seen.lock().unwrap_or_else(PoisonError::into_inner);
        if got.windows(TSP_REPLY.len()).any(|w| w == TSP_REPLY) {
            break true;
        }
        drop(got);
        if Instant::now() >= deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if replied {
        for message in &messages {
            out.write_all(message)?;
            out.flush()?;
        }
        out.write_all(b"fake tsp done\r\n")?;
    } else {
        out.write_all(b"fake tsp: no reply\r\n")?;
    }
    out.flush()?;
    drop(out);
    loop {
        thread::park();
    }
}

/// The messages omp sent to the terminal in a TSP record (JSONL of `{t, dir, verb, params,
/// body}`): the `out` lines, in order, as `ESC _ tsp ; <verb> ; <json> ESC \`. `in` lines (the
/// terminal's own acks) and blank lines are skipped.
// ponytail: verbs o t f x only; a `b` message is base64, not JSON, and the fixture has none.
fn fixture_messages(text: &str) -> Result<Vec<Vec<u8>>> {
    let mut messages = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let at = index + 1;
        let record: serde_json::Value =
            serde_json::from_str(line).with_context(|| format!("fixture line {at}"))?;
        if record["dir"] != "out" {
            continue;
        }
        let verb = match record["verb"].as_str().map(str::as_bytes) {
            Some([verb @ (b'o' | b't' | b'f' | b'x')]) => *verb,
            other => bail!("fixture line {at}: verb {other:?} is not one of o t f x"),
        };
        let message = wire::encode(verb, &record["body"], wire::MAX_MESSAGE)
            .with_context(|| format!("fixture line {at}"))?;
        messages.push(message);
    }
    Ok(messages)
}

/// Raw console input, so the terminal's APC replies arrive byte for byte and unechoed.
#[cfg(windows)]
#[allow(unsafe_code)]
mod raw_console {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
        ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, SetConsoleMode,
    };

    /// No line editing, no echo, VT input; a no-op when stdin is not a console.
    pub fn enable() {
        let stdin = std::io::stdin().as_raw_handle();
        let mut mode: CONSOLE_MODE = 0;
        // SAFETY: `stdin` is this process's live std handle and `mode` a valid out pointer.
        if unsafe { GetConsoleMode(stdin, &mut mode) } == 0 {
            return;
        }
        let raw = (mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        // SAFETY: same handle; a failure keeps the old mode.
        unsafe { SetConsoleMode(stdin, raw) };
    }
}

/// The PTY's line discipline would hold input back until a newline: `stty` switches it off
/// (stdin is the PTY slave, inherited).
#[cfg(not(windows))]
mod raw_console {
    pub fn enable() {
        let _ = std::process::Command::new("stty")
            .args(["raw", "-echo"])
            .status();
    }
}

#[derive(Debug, Default, PartialEq)]
struct Args {
    scenario: Option<String>,
    resume: Option<String>,
    prompt: Option<String>,
    listen: bool,
    /// `--tsp <fixture.jsonl>`.
    tsp: Option<String>,
    /// `--record <file>` (with `--tsp`): everything read from stdin.
    record: Option<String>,
    /// `--hello-delay-ms <n>` (with `--tsp`): a line of text, then a pause before the hello.
    hello_delay: Option<Duration>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args> {
    let mut parsed = Args::default();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--scenario=") {
            parsed.scenario = Some(value.to_owned());
        } else if arg == "--scenario" {
            match args.next() {
                Some(value) => parsed.scenario = Some(value),
                None => bail!("--scenario needs a value\n{USAGE}"),
            }
        } else if arg == "--resume" {
            match args.next() {
                Some(value) => parsed.resume = Some(value),
                None => bail!("--resume needs a value\n{USAGE}"),
            }
        } else if arg == "--listen" {
            parsed.listen = true;
        } else if arg == "--tsp" {
            match args.next() {
                Some(value) => parsed.tsp = Some(value),
                None => bail!("--tsp needs a value\n{USAGE}"),
            }
        } else if arg == "--hello-delay-ms" {
            let ms = args.next().and_then(|v| v.parse::<u64>().ok());
            match ms {
                Some(ms) => parsed.hello_delay = Some(Duration::from_millis(ms)),
                None => bail!("--hello-delay-ms needs a number\n{USAGE}"),
            }
        } else if arg == "--record" {
            match args.next() {
                Some(value) => parsed.record = Some(value),
                None => bail!("--record needs a value\n{USAGE}"),
            }
        } else if arg.starts_with("--") || parsed.prompt.is_some() {
            bail!("unexpected argument `{arg}`\n{USAGE}");
        } else {
            parsed.prompt = Some(arg);
        }
    }
    if (parsed.record.is_some() || parsed.hello_delay.is_some()) && parsed.tsp.is_none() {
        bail!("--record and --hello-delay-ms need --tsp\n{USAGE}");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The startup record of a real omp 18.13.2 (spec 2026-11-23-omp-tsp): 12 `out` and 10 `in` lines.
    const FIXTURE: &str = include_str!(
        "../../../docs/superpowers/specs/2026-11-23-omp-tsp/omp-18.13.2-startup.tsp.jsonl"
    );

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn scenario_arg(list: &[&str]) -> Option<String> {
        parse_args(args(list)).unwrap().scenario
    }

    #[test]
    fn scenario_arg_forms() {
        assert_eq!(scenario_arg(&[]), None);
        assert_eq!(
            scenario_arg(&["--scenario", "flood"]).as_deref(),
            Some("flood")
        );
        assert_eq!(
            scenario_arg(&["--scenario=a.toml"]).as_deref(),
            Some("a.toml")
        );
        assert!(parse_args(args(&["--scenario"])).is_err());
        assert!(parse_args(args(&["--bogus"])).is_err());
    }

    #[test]
    fn one_positional_arg_is_the_prompt() {
        let parsed = parse_args(args(&["--scenario", "ask", "fix the bug & ship"])).unwrap();
        assert_eq!(parsed.prompt.as_deref(), Some("fix the bug & ship"));
        assert_eq!(parsed.scenario.as_deref(), Some("ask"));
        assert!(parse_args(args(&["one", "two"])).is_err());
    }

    #[test]
    fn resume_takes_the_native_id() {
        let parsed = parse_args(args(&["--resume", "native-1", "go"])).unwrap();
        assert_eq!(parsed.resume.as_deref(), Some("native-1"));
        assert_eq!(parsed.prompt.as_deref(), Some("go"));
        assert!(parse_args(args(&["--resume"])).is_err());
    }

    #[test]
    fn listen_is_a_flag() {
        assert!(parse_args(args(&["--listen"])).unwrap().listen);
        assert!(!parse_args(args(&[])).unwrap().listen);
    }

    #[test]
    fn tsp_takes_a_fixture_and_a_record_file() {
        let parsed = parse_args(args(&["--tsp", "f.jsonl", "--record", "r.bin"])).unwrap();
        assert_eq!(parsed.tsp.as_deref(), Some("f.jsonl"));
        assert_eq!(parsed.record.as_deref(), Some("r.bin"));
        // The record file is optional, the fixture is not.
        let bare = parse_args(args(&["--tsp", "f.jsonl"])).unwrap();
        assert_eq!(bare.record, None);
        assert!(parse_args(args(&["--tsp"])).is_err());
        assert!(parse_args(args(&["--tsp", "f.jsonl", "--record"])).is_err());
        assert!(parse_args(args(&["--record", "r.bin"])).is_err());
        assert_eq!(parse_args(args(&[])).unwrap().tsp, None);
    }

    #[test]
    fn only_out_lines_are_replayed_in_order_with_their_verbs() {
        let text = [
            r#"{"t":1,"dir":"out","verb":"o","body":{"id":"s:1","mode":"inline"}}"#,
            r#"{"t":2,"dir":"in","verb":"e","params":{},"body":{"ev":"ack","sf":"s:1","s":1}}"#,
            "",
            r#"{"t":3,"dir":"out","verb":"f","body":{"sf":"s:1","s":1,"ops":[]}}"#,
            r#"{"t":4,"dir":"out","verb":"x","body":{"id":"s:1","keep":false}}"#,
        ]
        .join("\n");
        let messages = fixture_messages(&text).unwrap();
        assert_eq!(
            messages.len(),
            3,
            "the in line and the blank line are skipped"
        );
        for (message, verb) in messages.iter().zip(["o", "f", "x"]) {
            let head = format!("\x1b_tsp;{verb};");
            assert!(message.starts_with(head.as_bytes()), "{message:?}");
            assert!(message.ends_with(b"\x1b\\"), "{message:?}");
        }
        let head = b"\x1b_tsp;x;".len();
        let body = &messages[2][head..messages[2].len() - 2];
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(body).unwrap(),
            json!({"id": "s:1", "keep": false})
        );
    }

    #[test]
    fn the_committed_fixture_replays_its_twelve_out_messages() {
        let messages = fixture_messages(FIXTURE).unwrap();
        // `ESC _ t s p ;` is 6 bytes, so byte 6 is the verb.
        let verbs: String = messages.iter().map(|m| char::from(m[6])).collect();
        assert_eq!(verbs, format!("ot{}", "f".repeat(10)));
    }

    #[test]
    fn a_bad_fixture_line_names_its_line_number() {
        let text = "{\"dir\":\"out\",\"verb\":\"o\",\"body\":{}}\nnot json";
        let err = fixture_messages(text).unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err:#}");
        // `b` carries base64, which `wire::encode` cannot write.
        let blob = r#"{"dir":"out","verb":"b","body":{}}"#;
        assert!(fixture_messages(blob).is_err());
        let long = r#"{"dir":"out","verb":"ff","body":{}}"#;
        assert!(fixture_messages(long).is_err());
    }
}
