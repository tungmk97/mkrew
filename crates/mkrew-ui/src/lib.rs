//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client and connect-or-spawn.

pub mod client;
pub mod connect;
pub mod theme;
