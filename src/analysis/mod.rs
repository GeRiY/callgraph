//! Elemző-réteg: az `Analyzer` (Facade) a nyelv-registryn át. A nyelv-specifikus visitorok a `lang::*` alatt vannak.

mod analyzer;

pub use analyzer::Analyzer;
