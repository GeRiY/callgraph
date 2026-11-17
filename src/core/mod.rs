//! Nyelvfüggetlen építőkövek: a generikus lánc, a forrásfájl, a sorindex és a részleges elemzés vágási pontjai.

mod chain;
pub mod diag;
mod lines;
pub mod markup;
pub mod phpval;
mod recovery;
mod source;
pub mod srcscan;
pub mod text;

pub use chain::{Chain, Rule};
pub use lines::LineIndex;
pub use recovery::{clamp_to_content, prefix_cuts, recover_segments};
pub use source::SourceFile;
