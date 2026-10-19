//! `impl Shell` split by area; `struct Shell`, `main` and `Render` stay in `main.rs`.
//! Free widget fns are `pub(crate)` and re-exported at the crate root (`use shell::x::*`),
//! so every module reaches crate-root items and widgets through `use crate::*`.

pub(crate) mod cards;
mod chrome;
pub(crate) mod inspector;
pub(crate) mod menus;
pub(crate) mod pane_render;
pub(crate) mod reorder;
pub(crate) mod sidebar;
pub(crate) mod sidebar_view;
pub(crate) mod widgets;

mod comments;
pub(crate) mod confirm_prompt;
pub(crate) mod docs;
mod files;
pub(crate) mod files_render;
pub(crate) mod git;
mod git_dialogs;
pub(crate) mod git_diff;
pub(crate) mod git_menus;
mod git_render;
pub(crate) mod git_rows;
mod send_dialog;

mod board;
mod composer;
mod connection;
mod dialogs;
mod focus;
mod history;
mod navigation;
mod peek;
mod phone_access;
mod run;
mod sessions;
mod tabs;
mod toasts;
mod task_dialog;
pub(crate) mod thread_panel;
mod voice_input;
mod workspace;
mod workspace_actions;
mod worktrees;
