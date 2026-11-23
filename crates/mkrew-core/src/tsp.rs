//! The Tern Surface Protocol (TSP) as omp speaks it (spec 2026-11-23-omp-tsp-surface-design).
//!
//! [`wire`] splits PTY output into plain bytes and TSP messages and encodes our replies,
//! [`doc`] holds the node tree the frames build. Both are pure: no I/O, no clock.

pub mod doc;
pub mod wire;

/// The committed omp 18.13.2 startup record, for tests of every layer.
#[cfg(test)]
pub(crate) mod fixture {
    use serde_json::Value;

    const STARTUP: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/superpowers/specs/2026-11-23-omp-tsp/omp-18.13.2-startup.tsp.jsonl"
    ));

    /// Every `out` line (omp to terminal) as (verb byte, body as parsed JSON), in file order.
    pub(crate) fn out_lines() -> Vec<(u8, Value)> {
        STARTUP
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|rec| rec["dir"] == "out")
            .map(|rec| {
                (
                    rec["verb"].as_str().unwrap().as_bytes()[0],
                    rec["body"].clone(),
                )
            })
            .collect()
    }
}
