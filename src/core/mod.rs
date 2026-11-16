//! Nyelvfüggetlen építőkövek: a generikus lánc, a forrásfájl, a sorindex és a részleges elemzés vágási pontjai.

mod chain;
mod lines;
mod recovery;
mod source;

pub use chain::{Chain, Rule};
pub use lines::LineIndex;
pub use recovery::{clamp_to_content, prefix_cuts, recover_segments};
pub use source::SourceFile;
