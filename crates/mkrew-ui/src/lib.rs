//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view), the sidebar view-model and the
//! terminal tabs / split-tree models, the task board, the Inbox, the pane lifecycle decisions,
//! the keymap, the workspace menu and the "Open in Zed" command.

pub mod board;
pub mod client;
pub mod connect;
pub mod inbox;
pub mod keymap;
pub mod lifecycle;
pub mod router;
pub mod sidebar;
pub mod split;
pub mod status;
pub mod tabs;
pub mod terminal;
pub mod theme;
pub mod workspace_menu;
pub mod zed;
