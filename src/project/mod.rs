//! Projekt-réteg: a projekt-gyökér megkeresése és a projekten belüli `.callgraph/` tároló.
//! Az eszköz semmit sem tárol a projekten kívül.

mod aliases;
mod entity;
mod locator;
mod store;

#[cfg(test)]
mod tests;

pub use aliases::{AliasSource, NoAliases, ProjectsJsonAliases};
pub use entity::Project;
pub use locator::{ProjectGroup, ProjectLocator};
pub use store::{ProjectStore, STORE_DIR};
