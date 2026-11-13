//! A PHP célnyelv: `PhpEmitter` (Strategy), `PhpFileBuilder` (Builder) és a típus-leképezés.

mod builder;
mod emitter;
pub mod types;

pub use builder::PhpFileBuilder;
pub use emitter::PhpEmitter;
