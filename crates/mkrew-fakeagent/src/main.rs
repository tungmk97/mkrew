//! `mkrew-fakeagent [--scenario <file|name>]` — runs a scenario (SPEC §16).
//!
//! The scenario comes from `--scenario`, else `MKREW_FAKE_SCENARIO`, else the built-in `ask`.

use std::io::{self, BufWriter};
use std::process;

use anyhow::{Result, bail};
use mkrew_fakeagent::{DEFAULT_SCENARIO, SCENARIO_ENV, load, run};

const USAGE: &str = "usage: mkrew-fakeagent [--scenario <file|name>]";

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
    let spec = match scenario_arg(std::env::args().skip(1))? {
        Some(spec) => spec,
        None => std::env::var(SCENARIO_ENV).unwrap_or_else(|_| DEFAULT_SCENARIO.to_owned()),
    };
    let scenario = load(&spec)?;
    let mut out = BufWriter::new(io::stdout().lock());
    Ok(run(&scenario, &mut io::stdin().lock(), &mut out)?)
}

fn scenario_arg(mut args: impl Iterator<Item = String>) -> Result<Option<String>> {
    let mut spec = None;
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--scenario=") {
            spec = Some(value.to_owned());
        } else if arg == "--scenario" {
            match args.next() {
                Some(value) => spec = Some(value),
                None => bail!("--scenario needs a value\n{USAGE}"),
            }
        } else {
            bail!("unexpected argument `{arg}`\n{USAGE}");
        }
    }
    Ok(spec)
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

    #[test]
    fn scenario_arg_forms() {
        assert_eq!(scenario_arg(args(&[])).unwrap(), None);
        assert_eq!(
            scenario_arg(args(&["--scenario", "flood"]))
                .unwrap()
                .as_deref(),
            Some("flood")
        );
        assert_eq!(
            scenario_arg(args(&["--scenario=a.toml"]))
                .unwrap()
                .as_deref(),
            Some("a.toml")
        );
        assert!(scenario_arg(args(&["--scenario"])).is_err());
        assert!(scenario_arg(args(&["--bogus"])).is_err());
    }
}
