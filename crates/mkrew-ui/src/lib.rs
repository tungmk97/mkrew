//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view), the sidebar view-model and the
//! terminal tabs / split-tree models, the task board and its task, undo and follow-up
//! controls, conflict warnings, the Inbox, the session history dialog, the pane lifecycle
//! decisions, the keymap, the workspace menu, the "Open in Zed" command, the Docs view-model
//! and its review comments (with the "Send to agent" and "View changes" dialogs), the Markdown
//! render model of the Docs preview and its GPUI elements, and the Files view-model with its
//! editor buffer, its
//! syntax highlighting and quick open, the Git view-model and its base menu, the command
//! palette, the turn summaries, the
//! peek popover, the broadcast dialog, the Files / Git worktree target, fan-out and token
//! usage, platform text input (IME, Unikey) for the fields and terminals, the Run button, voice
//! input (microphone and speech to text), and
//! the scroll bar of the Files editor and the Markdown preview.

pub mod base_dialog;
pub mod board;
pub mod broadcast;
pub mod client;
pub mod comments;
pub mod composer;
pub mod conflicts;
pub mod connect;
pub mod context_menu;
pub mod diagram;
pub mod doc_dialog;
pub mod docs;
pub mod editor;
pub mod fanout;
pub mod files_view;
pub mod follow;
pub mod follow_up;
pub mod git_actions;
pub mod git_history;
pub mod git_summary;
pub mod git_view;
pub mod highlight;
pub mod history;
pub mod ime;
pub mod inbox;
pub mod inspector;
pub mod keymap;
pub mod lifecycle;
pub mod markdown;
pub mod palette;
pub mod pane_text;
pub mod peek;
pub mod plan;
pub mod preview;
pub mod prompt_picker;
pub mod quick_open;
pub mod router;
pub mod run;
pub mod run_setup;
pub mod scrollbar;
pub mod sidebar;
pub mod split;
pub mod status;
pub mod stt;
pub mod tabs;
pub mod task_dialog;
pub mod terminal;
pub mod text_box;
pub mod text_find;
pub mod theme;
pub mod turn;
pub mod turns_dialog;
pub mod undo_dialog;
pub mod usage;
pub mod voice;
pub mod workspace_menu;
pub mod worktree_dialog;
pub mod worktree_target;
pub mod zed;
