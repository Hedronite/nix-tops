//! `nixtops gate`: gate 1 (source), gate 2 (lint), and gate 3 (eval), and the report that says
//! what ran.
//!
//! The order is fixed: the config is checked first (`check::evaluate`), then each layer in the
//! order of the report. A layer that is switched off is `skip` with its reason; one that is on
//! and not built in this version is `not-run`, which fails the gate.

pub mod diag;
pub mod eval;
pub mod layer;
pub mod ledger;
pub mod lint;
pub mod run;
pub mod source;

pub use run::{Evaluator, Seams, Settings, run};
