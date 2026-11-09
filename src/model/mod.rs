//! A JSON-séma serde-structokként, a `golden/ref.jsonl` `graph` objektumával 1:1.

mod builder;
mod call;
mod comment;
mod constant;
mod container;
mod control_flow;
mod file_graph;
mod method;
mod parse_error;
mod property;
mod string_lit;
mod write;

pub use builder::FileGraphBuilder;
pub use call::Call;
pub use comment::Comment;
pub use constant::Constant;
pub use container::Container;
pub use control_flow::{Branch, ControlFlow};
pub use file_graph::{FileGraph, Imports};
pub use method::Method;
pub use parse_error::{MAX_PARSE_ERRORS, ParseError};
pub use property::Property;
pub use string_lit::StringLit;
pub use write::Write;
