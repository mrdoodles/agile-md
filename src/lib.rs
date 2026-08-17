//! agile-md — a tiny filesystem Kanban in markdown.
//!
//! Tasks are markdown files that move between `todo/`, `doing/` and `done/` in
//! `<repo-root>/tasks`. Status is the folder; `git mv` is the audit trail; the
//! ticket format is a MiniJinja template you can edit.
//!
//! The crate is a library so that the `amd` CLI and the terminal UI share one
//! implementation: creating a ticket, moving it and rendering the board all
//! live here, and the front ends only decide how to ask and how to draw.

pub mod board;
pub mod branch;
pub mod create;
pub mod git;
pub mod registry;
pub mod render;
pub mod task;
pub mod templates;

#[cfg(feature = "gui")]
pub mod gui;
#[cfg(feature = "tui")]
pub mod tui;
