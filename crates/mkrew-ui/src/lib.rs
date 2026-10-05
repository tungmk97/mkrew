//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view), the sidebar view-model and the
//! terminal tabs / split-tree models and the pane lifecycle decisions.

pub mod client;
pub mod connect;
pub mod lifecycle;
pub mod router;
pub mod sidebar;
pub mod split;
pub mod tabs;
pub mod terminal;
pub mod theme;
