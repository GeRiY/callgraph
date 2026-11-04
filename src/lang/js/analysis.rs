//! A JS-elemzés közös építője és a bővítési pont: a következő lépések `JsPass`-ként kapcsolódnak.

use std::path::Path;

use oxc_ast::ast::Program;

use super::lines::LineIndex;
use crate::model::FileGraphBuilder;

/// Egy elemzendő script-blokk kontextusa.
pub struct PassContext<'a> {
    /// Az elemzett fájl útja (a feloldóhoz, a `module` mezőhöz).
    pub path: &'a Path,
    pub source: &'a str,
    pub lines: &'a LineIndex,
    /// `true`, ha a blokk `<script setup>`.
    pub setup: bool,
}

/// Egy elemzési lépés (Strategy): a programból tölti a `JsAnalysis` mezőit (structure, calls, strings, ...).
pub trait JsPass: Send + Sync {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis);
}

/// A fájlgráf építője a figyelmeztetésekkel; a passzok ebbe gyűjtenek.
pub struct JsAnalysis {
    pub builder: FileGraphBuilder,
    pub warnings: Vec<String>,
}

impl JsAnalysis {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            builder: FileGraphBuilder::new(file),
            warnings: Vec::new(),
        }
    }
}
