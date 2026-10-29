//! Mkrew desktop app (SPEC §10). The binary `mkrew` is the GPUI shell; the library holds the
//! parts that are tested headlessly: design tokens, the IPC client, connect-or-spawn and the
//! terminal emulator/layout (plus the terminal pane view), the sidebar view-model and the
//! terminal tabs / split-tree models, the task board and its task, undo and follow-up
//! controls, conflict warnings, the Inbox, the session history dialog, the pane lifecycle
//! decisions, the keymap, the workspace menu, the "Open in Zed" command, the Docs view-model
//! and its review comments (with the "Send to agent" and "View changes" dialogs), the Markdown
//! render model of the Docs preview and its GPUI elements, and the Files view-model with its
//! editor buffer, its tree actions (new file / folder, add to `.gitignore`), its
//! syntax highlighting and quick open, the Git view-model and its base menu, the command
//! palette, the turn summaries, the peek popover, the broadcast and "Ask AI" dialogs, the in-app confirmation
//! dialog, the Files / Git worktree target, fan-out and token
//! usage, platform text input (IME, Unikey) for the fields and terminals, the Run button, voice
//! input (microphone and speech to text), the phone-access link, token and QR code and its
//! dialog model, the toast queue, the scroll bar of the Files editor and the Markdown preview, and
//! the taskbar / tray attention model, and the in-app update (GitHub releases, install in place).

pub mod ask;
pub mod attention;
pub mod base_dialog;
pub mod board;
pub mod broadcast;
pub mod client;
pub mod clone_dialog;
pub mod comments;
pub mod composer;
pub mod confirm;
pub mod conflicts;
pub mod connect;
pub mod context_menu;
pub mod desktop;
pub mod diagram;
pub mod dialog_focus;
pub mod doc_dialog;
pub mod docs;
pub mod editor;
pub mod fanout;
pub mod field;
pub mod files_view;
pub mod follow;
pub mod follow_up;
pub mod git_actions;
pub mod git_graph;
pub mod git_history;
pub mod git_summary;
pub mod git_view;
pub mod git_viewed;
pub mod glyph;
pub mod gutter;
pub mod highlight;
pub mod history;
pub mod icons;
pub mod ime;
pub mod inbox;
pub mod inspector;
pub mod jira;
pub mod jira_board;
pub mod keymap;
pub mod lifecycle;
pub mod markdown;
pub mod menu;
pub mod palette;
pub mod pane_text;
pub mod peek;
pub mod phone;
pub mod phone_dialog;
pub mod pick;
pub mod plan;
pub mod preview;
pub mod preview_select;
pub mod prompt_picker;
pub mod quick_open;
pub mod redraw;
pub mod router;
pub mod run;
pub mod run_setup;
pub mod scrollbar;
pub mod settings_dialog;
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
pub mod thread_view;
pub mod toast;
pub mod turn;
pub mod turns_dialog;
pub mod undo_dialog;
pub mod update;
pub mod usage;
pub mod voice;
pub mod watches;
pub mod workspace_menu;
pub mod worktree_dialog;
pub mod worktree_target;
pub mod zed;
