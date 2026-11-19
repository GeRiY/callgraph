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
    /// A blokk metódus-indexe: a `MethodsPass` tölti, a `CallsPass` olvassa (a blokkonként újra felülírva).
    pub method_index: Option<super::methods::MethodIndex>,
    /// The script blocks' names and bindings of a `.vue` file (the `CallsPass` fills it): the template expressions
    /// are classified in this scope.
    pub template_scope: Option<super::calls::Scope>,
}

impl JsAnalysis {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            builder: FileGraphBuilder::new(file),
            warnings: Vec::new(),
            method_index: None,
            template_scope: None,
        }
    }
}
