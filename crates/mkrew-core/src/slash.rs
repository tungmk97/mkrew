//! The `/` commands an agent CLI shows (spec 2026-11-04 §4, §5): a built-in table per CLI plus
//! the skill, command and prompt files that CLI scans, read from the agent's own machine.
//!
//! The CLI is never started. Every scan reads small files from `roots`, stops at the repo root
//! and keeps what it cannot read as one line in [`SlashList::errors`].

pub mod front_matter;
