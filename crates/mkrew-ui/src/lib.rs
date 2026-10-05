//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view) and the sidebar view-model.

pub mod client;
pub mod connect;
pub mod sidebar;
pub mod terminal;
pub mod theme;
