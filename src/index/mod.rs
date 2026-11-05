//! Projekt-szintű fordított hívó-index és típus-hierarchia (a tárolásuk: `crate::graph::ProjectGraph`).

mod hierarchy;
mod query;
mod uncertain;

pub(crate) use hierarchy::{TypeHierarchy, ViaMatcher};
pub(crate) use query::CallerIndex;
pub use query::CallerRow;
pub(crate) use uncertain::{UNKNOWN_RECEIVER, uncertain_match};
