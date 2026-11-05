//! Projekt-szintű szimbólum- és fordított hívó-index.

mod hierarchy;
mod query;
mod record;
mod store;

pub(crate) use hierarchy::{TypeHierarchy, ViaMatcher};
pub use query::CallerRow;
pub(crate) use query::{CallerIndex, FileGraphRecord};
pub(crate) use record::FileStamp;
pub(crate) use store::IndexStore;
