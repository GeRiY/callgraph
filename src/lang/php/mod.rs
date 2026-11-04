//! A PHP nyelv-modul (`PhpBackend`): parser, elemző-visitorok, hívás-kategorizálás, framework-stratégiák.

pub mod analysis;
mod backend;
pub mod categorize;
pub mod framework;
pub mod parser;
pub mod printer;

pub use backend::PhpBackend;
