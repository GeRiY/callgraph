//! A CSS és az SCSS közös szkennere: egy motor, a nyelvi eltérések a `Dialect`-ben (`scanner`), a tiszta segédek
//! a `helpers` / `rules` / `scss_parts` modulokban.

mod helpers;
mod rules;
mod scanner;
mod scss_parts;

pub(crate) use scanner::{Dialect, StylesheetScanner};
