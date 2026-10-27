//! Terminal panes (SPEC §10): a UI-side emulator fed from the daemon's attach snapshot and
//! output frames, the pure grid → styled-runs layout, key → bytes encoding, the find bar, and
//! the GPUI view.

pub mod copy_mode;
pub mod emulator;
pub mod find;
pub mod hints;
pub mod input;
pub mod layout;
pub mod links;
pub mod view;
