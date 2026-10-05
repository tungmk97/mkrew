//! `mkrew-fakeagent [--scenario <file|name>] [--resume <id>] [<prompt>]` — runs a scenario
//! (SPEC §16).
//!
//! The scenario comes from `--scenario`, else `MKREW_FAKE_SCENARIO`, else the built-in `ask`.
//! `--resume <id>` (a profile's `resume` command) is printed first as `Resumed: <id>`, then a
//! `<prompt>` (a task prompt passed with `prompt_mode = "arg"`) as `Task: <prompt>`.

use std::io::{self, BufWriter, Write};
use std::process;

use anyhow::{Result, bail};
use mkrew_fakeagent::{DEFAULT_SCENARIO, SCENARIO_ENV, load, run};

const USAGE: &str = "usage: mkrew-fakeagent [--scenario <file|name>] [--resume <id>] [<prompt>]";

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

#[derive(Debug, Default, PartialEq)]
struct Args {
    scenario: Option<String>,
    resume: Option<String>,
    prompt: Option<String>,
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
        } else if arg.starts_with("--") || parsed.prompt.is_some() {
            bail!("unexpected argument `{arg}`\n{USAGE}");
        } else {
            parsed.prompt = Some(arg);
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
