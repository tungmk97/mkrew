//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view), the sidebar view-model and the
//! terminal tabs / split-tree models, the task board and its task, undo and follow-up
//! controls, conflict warnings, the Inbox, the session history dialog, the pane lifecycle
//! decisions, the keymap, the workspace menu, the "Open in Zed" command, the Docs view-model
//! and its review comments, the Markdown render model of the Docs preview and its GPUI elements.

pub mod board;
pub mod client;
pub mod comments;
pub mod conflicts;
pub mod connect;
pub mod docs;
pub mod follow_up;
pub mod history;
pub mod inbox;
pub mod keymap;
pub mod lifecycle;
pub mod markdown;
pub mod preview;
pub mod router;
pub mod sidebar;
pub mod split;
pub mod status;
pub mod tabs;
pub mod task_dialog;
pub mod terminal;
pub mod theme;
pub mod undo_dialog;
pub mod workspace_menu;
pub mod zed;
