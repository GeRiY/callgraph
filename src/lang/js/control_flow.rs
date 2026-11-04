//! A `controlFlow` mező script-részre: `if` / `else if` lánc és `switch` (ágak sortartománnyal).

use std::collections::HashSet;

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::scope::text::src_of;
use super::scope::{Scopes, scope_hooks};
use crate::model::{Branch, ControlFlow};

/// Az elágazások gyűjtése.
pub struct ControlFlowPass;

impl JsPass for ControlFlowPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let mut v = FlowVisitor {
            ctx,
            scopes: Scopes::build(program, ctx),
            chained: HashSet::new(),
            found: Vec::new(),
        };
        v.visit_program(program);
        for c in v.found {
            out.builder.control_flow(c);
        }
    }
}

struct FlowVisitor<'c> {
    ctx: &'c PassContext<'c>,
    scopes: Scopes,
    /// Az `else if` láncba tartozó belső `if`-ek (a lánc a külső `if`-nél van).
    chained: HashSet<u32>,
    found: Vec<ControlFlow>,
}

fn unparen<'b, 'a>(mut e: &'b Expression<'a>) -> &'b Expression<'a> {
    while let Expression::ParenthesizedExpression(p) = e {
        e = &p.expression;
    }
    e
}

impl FlowVisitor<'_> {
    fn src(&self, e: &Expression<'_>, max: usize) -> String {
        let s = unparen(e).span();
        src_of(self.ctx.source, s.start, s.end, max)
    }

    fn line(&self, offset: u32) -> u32 {
        self.ctx.lines.line_of(offset as usize)
    }

    fn branch(&self, label: String, span: Span, fall_through: Option<bool>) -> Branch {
        Branch {
            label,
            start_line: self.line(span.start),
            end_line: self.line(span.end),
            fall_through,
        }
    }

    fn emit(&mut self, kind: &str, line: u32, cond: String, branches: Vec<Branch>) {
        self.found.push(ControlFlow {
            container: Some(self.scopes.container().to_string()),
            caller: Some(self.scopes.caller().to_string()),
            kind: kind.to_string(),
            line,
            cond: Some(cond),
            branches,
        });
    }
}

impl<'a> Visit<'a> for FlowVisitor<'_> {
    scope_hooks!();

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        if let Some(Statement::IfStatement(alt)) = &it.alternate {
            self.chained.insert(alt.span.start);
        }
        if !self.chained.contains(&it.span.start) {
            let mut branches = Vec::new();
            let mut cur = Some(it);
            let mut first = true;
            while let Some(c) = cur {
                let head = if first { "if" } else { "elseif" };
                let label = format!("{head} ({})", self.src(&c.test, 80));
                branches.push(self.branch(label, c.consequent.span(), None));
                first = false;
                cur = match &c.alternate {
                    Some(Statement::IfStatement(next)) => Some(next),
                    Some(other) => {
                        branches.push(self.branch("else".into(), other.span(), None));
                        None
                    }
                    None => None,
                };
            }
            let cond = self.src(&it.test, 100);
            self.emit("if", self.line(it.span.start), cond, branches);
        }
        walk::walk_if_statement(self, it);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        let last = it.cases.len().saturating_sub(1);
        let branches = it
            .cases
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let label = c.test.as_ref().map_or("default".to_string(), |t| {
                    format!("case {}", self.src(t, 60))
                });
                self.branch(label, c.span, Some(c.consequent.is_empty() && i < last))
            })
            .collect();
        let cond = self.src(&it.discriminant, 100);
        self.emit("switch", self.line(it.span.start), cond, branches);
        walk::walk_switch_statement(self, it);
    }
}

#[cfg(test)]
mod tests {
    use crate::core::SourceFile;
    use crate::lang::Language;
    use crate::lang::js::JsFrontend;

    #[test]
    fn if_chain_and_switch_fallthrough() {
        let src = "function f(a) {\n if (a) {\n x();\n } else if (b) {\n y();\n } else {\n z();\n }\n switch (a) {\n case 1:\n case 2: break;\n default: break;\n }\n}\n";
        let f = SourceFile::new("/p/m.js", src.as_bytes().to_vec(), None);
        let g = JsFrontend::with_defaults().analyze(&f).unwrap().graph;
        assert_eq!(g.control_flow.len(), 2);
        let labels: Vec<_> = g.control_flow[0]
            .branches
            .iter()
            .map(|b| b.label.as_str())
            .collect();
        assert_eq!(labels, ["if (a)", "elseif (b)", "else"]);
        assert_eq!(g.control_flow[1].branches[0].fall_through, Some(true));
        assert_eq!(g.control_flow[1].branches[2].fall_through, Some(false));
    }
}
