//! The gate report (`report.json`, protocol section 6.7): the model, its JSON, and the text form.

mod model;
mod text;

pub use model::{
    ALL_LAYERS, AckEntry, Acks, BaselineIdentity, Counts, Layer, LayerId, LintReport,
    LintToolReport, OutputEntry, Parts, Report, Verdict, WarningEntry, WarningSource,
};
pub use text::render_text;
