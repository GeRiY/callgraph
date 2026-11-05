//! A skill-mappa adatai — a régi bash + jq scriptek (projects.sh, feedback.sh, lib.sh) Rust-utódja.
//! A fájlformátum változatlan: `projects.json` (alias → út vagy `{path, …}`), `errors.jsonl`, `feedback.jsonl`.

mod clock;
mod errors;
mod feedback;
mod home;
mod registry;

#[cfg(test)]
mod tests;

pub use errors::{ErrorLog, ErrorRow, SkillFailure, fail, finish};
pub use feedback::FeedbackStore;
pub use home::SkillHome;
pub use registry::ProjectRegistry;
