//! Projekt-szintű fordított hívó-index és típus-hierarchia (a tárolásuk: `crate::graph::ProjectGraph`).

mod hierarchy;
mod query;
mod uncertain;
mod wiring;

pub(crate) use hierarchy::{TypeHierarchy, ViaMatcher};
// a projekt-profil (`crate::profile`) a típusnevek és hívás-célok összevetéséhez
pub(crate) use hierarchy::normalize as normalize_type;
pub(crate) use query::CallerIndex;
pub use query::CallerRow;
pub(crate) use uncertain::{UNKNOWN_RECEIVER, pattern_name, target_matches, uncertain_match};
pub(crate) use wiring::{LinkKind, PhpWiring};
