//! Mkrew core: models, agent profile parsing and the session state machine.
//!
//! No async runtime, GPUI or SQLite here (SPEC §4); the only I/O is reading profile files.

pub mod profile;
pub mod state;

pub use profile::{
    AgentProfile, ProfileError, ProfileKind, ProfileSet, PromptMode, StateConfig, StateMode,
    load_dir, parse_profile,
};
pub use state::{HookEvent, SendQueue, StateMachine, can_deliver};
