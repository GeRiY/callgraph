//! `controlFlow`: `if` / `else if` / `else` láncok (az `if let` is) és `match`-karok.

use syn::visit::{self, Visit};
use syn::{Expr, ExprIf, ExprMatch, Macro};

use super::body::{BodyVisit, Location, walk_bodies};
use super::macros::visit_body;
use super::{FileVisitor, Scopes};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::ParsedFile;
use crate::model::{Branch, ControlFlow, FileGraphBuilder};

/// A `controlFlow` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ControlFlowVisitor;

impl FileVisitor for ControlFlowVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        _ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut collector = Collector {
            parsed,
            location: None,
            rows: Vec::new(),
        };
        walk_bodies(scopes, &mut collector);
        for row in collector.rows {
            builder.control_flow(row);
        }
    }
}

struct Collector<'p, 'a> {
    parsed: &'p ParsedFile<'a>,
    location: Option<Location>,
    rows: Vec<ControlFlow>,
}

impl Collector<'_, '_> {
    fn branch(&self, label: String, (start_line, end_line): (u32, u32)) -> Branch {
        Branch {
            label,
            start_line,
            end_line,
            fall_through: None,
        }
    }

    fn push(&mut self, kind: &str, line: u32, cond: String, branches: Vec<Branch>) {
        let Some(location) = &self.location else {
            return;
        };
        self.rows.push(ControlFlow {
            container: Some(location.container.clone()),
            caller: Some(location.caller.clone()),
            kind: kind.to_owned(),
            line,
            cond: Some(cond),
            branches,
        });
    }

    /// Az `if` lánc ágai; a belső `else if`-ek nem kapnak külön sort, de a törzsük bejárásra kerül.
    fn if_chain(&mut self, node: &ExprIf) -> Vec<Branch> {
        let mut branches = Vec::new();
        let mut current = node;
        let mut head = "if";
        loop {
            let cond = self.parsed.short(&*current.cond, 80);
            let lines = self.parsed.lines(&current.then_branch);
            branches.push(self.branch(format!("{head} ({cond})"), lines));
            self.visit_expr(&current.cond);
            self.visit_block(&current.then_branch);
            match current.else_branch.as_ref().map(|(_, e)| &**e) {
                Some(Expr::If(next)) => {
                    current = next;
                    head = "elseif";
                }
                Some(other) => {
                    branches.push(self.branch("else".to_owned(), self.parsed.lines(other)));
                    self.visit_expr(other);
                    break;
                }
                None => break,
            }
        }
        branches
    }
}

impl BodyVisit for Collector<'_, '_> {
    fn enter(&mut self, location: Location) {
        self.location = Some(location);
    }
}

impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        let line = self.parsed.line(node.if_token.span);
        let cond = self.parsed.short(&*node.cond, 80);
        let index = self.rows.len();
        let branches = self.if_chain(node);
        // A sorrend a forrás szerinti: a külső `if` a belső elágazások elé kerül.
        let Some(location) = &self.location else {
            return;
        };
        self.rows.insert(
            index,
            ControlFlow {
                container: Some(location.container.clone()),
                caller: Some(location.caller.clone()),
                kind: "if".to_owned(),
                line,
                cond: Some(cond),
                branches,
            },
        );
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        let line = self.parsed.line(node.match_token.span);
        let cond = self.parsed.short(&*node.expr, 80);
        let branches = node
            .arms
            .iter()
            .map(|arm| {
                let mut label = self.parsed.short(&arm.pat, 80);
                if let Some((_, guard)) = &arm.guard {
                    label = format!("{label} if {}", self.parsed.short(&**guard, 60));
                }
                self.branch(label, self.parsed.lines(&*arm.body))
            })
            .collect();
        self.push("match", line, cond, branches);
        visit::visit_expr_match(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        visit_body(self, node);
    }
}
