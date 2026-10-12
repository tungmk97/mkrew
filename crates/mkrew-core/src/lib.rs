//! Mkrew core: models, agent profile parsing and the session state machine.
//!
//! No async runtime, GPUI or SQLite here (SPEC §4); the only I/O is reading profile and
//! settings files and the prompt library, plus seeding it (and looking up the platform
//! directories in [`paths`] and programs on `PATH` in [`lookup`]), and the one-shot calls to the
//! `tailscale` CLI in [`tailscale`], shared by `mk web`, the daemon and the UI.

pub mod conflict;
pub mod git;
pub mod hook;
pub mod log_tail;
pub mod lookup;
pub mod paths;
pub mod plan;
pub mod profile;
pub mod prompt;
pub mod review;
pub mod run;
pub mod settings;
pub mod state;
pub mod tailscale;
pub mod task;
pub mod turn;
pub mod usage;

pub use hook::HookAgent;
pub use paths::Paths;
pub use profile::{
    AgentProfile, ProfileError, ProfileKind, ProfileSet, PromptMode, StateConfig, StateMode,
    UsageConfig, UsageFormat, load_dir, parse_profile,
};
pub use settings::{Settings, SettingsError};
pub use state::{HookEvent, SendQueue, StateMachine, can_deliver};
