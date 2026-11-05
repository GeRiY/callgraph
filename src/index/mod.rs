//! Projekt-szintű szimbólum- és fordított hívó-index.

mod query;
mod record;
mod store;
mod uncertain;

pub use query::CallerRow;
pub(crate) use query::{CallerIndex, FileGraphRecord};
pub(crate) use record::FileStamp;
pub(crate) use store::IndexStore;
pub(crate) use uncertain::{UNKNOWN_RECEIVER, uncertain_match};
