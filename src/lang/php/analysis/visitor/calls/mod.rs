//! Hívások gyűjtése és kategorizálása.

mod dynamic;
mod helpers;
mod state;
#[cfg(test)]
mod tests;
mod walker;

use super::FileVisitor;
use crate::{
    lang::php::{analysis::AnalysisContext, framework::FrameworkDetector, parser::ParsedFile},
    model::FileGraphBuilder,
};
use helpers::Collector;
use mago_syntax::walker as ast_walker;
use state::{State, categorize};
use std::path::Path;

pub(crate) use crate::lang::php::printer::{lossy, strip_expr as strip};

/// A `calls` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct CallsVisitor;

impl FileVisitor for CallsVisitor {
    fn visit(&self, p: &ParsedFile<'_>, ctx: &mut AnalysisContext, b: &mut FileGraphBuilder) {
        let mut detector = FrameworkDetector::new();
        let dir = Path::new(&ctx.path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let collector = Collector { parsed: p };
        let (resolver, imports) = collector.source_resolver();
        let mut state = State {
            resolver,
            imports,
            framework: detector.detect(dir),
            ..State::default()
        };
        ast_walker::walk_program(&collector, p.program, &mut state);
        let calls = std::mem::take(&mut state.calls);
        for mut call in calls {
            categorize(&mut call, &state);
            b.call(call);
        }
    }
}
