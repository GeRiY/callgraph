//! CSS- és SCSS-szkennerek közös, forráshelyes tokenizáló segédei.

mod helpers;

pub(crate) use helpers::{clean_header, is_ident, is_ident_start, line, normalize, strip_comments};
