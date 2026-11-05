//! Rust framework-felismerés (Strategy + Factory): a `Cargo.toml` függőségei → ismert framework.

mod cargo_toml;
mod detector;
mod known;
mod strategy;

pub(crate) use detector::FrameworkDetector;
pub(crate) use known::is_framework_crate;
pub(crate) use strategy::FrameworkStrategy;
