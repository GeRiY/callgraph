//! Nyelv-réteg: a `Language` trait-család, a registry és a nyelv-modulok (`php`, később több).

mod backend;
pub mod css;
mod document;
pub mod filetype;
mod frontend;
pub mod go;
pub mod html;
pub mod js;
pub mod json;
mod language;
pub mod markdown;
pub mod php;
pub mod python;
mod registry;
pub mod rs;
pub mod scss;
pub(crate) mod stylesheet;
pub mod swift;

pub use backend::LanguageBackend;
pub use document::LanguageDocument;
pub use frontend::LanguageFrontend;
pub use language::{Analysis, Language, LanguageRole};
pub use registry::LanguageRegistry;
