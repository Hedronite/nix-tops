//! The Nix-TOPS runner. `main.rs` parses the command line and calls [`cmd::run`].
//!
//! Every command of the protocol's CLI table has its own module under `cmd/`. A command that is
//! not built yet says so and exits 4 ([`cmd::not_built`]). The modules below `cmd` that are
//! empty today are the homes of later steps.

pub mod build_info;
pub mod cmd;
pub mod config;
pub mod exit;
pub mod gate;
pub mod host;
pub mod inventory;
pub mod probe;
pub mod profiles;
pub mod report;
