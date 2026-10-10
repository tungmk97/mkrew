//! `impl Shell` split by area; `struct Shell`, `main` and `Render` stay in `main.rs`.
//! Free widget fns are `pub(crate)` and re-exported at the crate root (`use shell::x::*`),
//! so every module reaches crate-root items and widgets through `use crate::*`.

pub(crate) mod cards;
mod chrome;
pub(crate) mod inspector;
pub(crate) mod menus;
pub(crate) mod pane_render;
pub(crate) mod sidebar;
pub(crate) mod widgets;
