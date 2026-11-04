//! Vezérlés (`controlFlow`): if / elseif / else ágak (`handleIf`, `stmtsRange`).

use mago_syntax::cst::{IfBody, Sequence, Statement};

mod if_range;
use mago_syntax::{cst, walker};

use mago_span::HasSpan;

use super::{FileVisitor, context::Ctx};
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::lang::php::printer::print_expr;
use crate::model::{Branch, ControlFlow, FileGraphBuilder};

/// A `controlFlow` if-sorait tölti (a match/switch sorait a `MatchSwitchVisitor`).
#[derive(Debug, Default, Clone, Copy)]
pub struct IfElseVisitor;

#[derive(Default)]
struct State {
    ctx: Ctx,
    flows: Vec<ControlFlow>,
}

pub(super) struct Collector<'a> {
    parsed: &'a ParsedFile<'a>,
}

/// A feltétel rövid forrásszövege: 80 karakternél hosszabbat `…`-gal csonkol (`exprToShortString`).
fn short(text: &str) -> String {
    if text.chars().count() > 80 {
        let mut s: String = text.chars().take(80).collect();
        s.push('…');
        s
    } else {
        text.to_string()
    }
}

impl Collector<'_> {
    fn cond(&self, expr: &cst::Expression<'_>) -> String {
        short(&print_expr(self.parsed, expr))
    }

    fn branch(&self, label: String, (start_line, end_line): (u32, u32)) -> Branch {
        Branch {
            label,
            start_line,
            end_line,
            fall_through: None,
        }
    }

    fn colon<'s>(
        &self,
        label: String,
        stmts: &'s Sequence<'s, Statement<'s>>,
        (at, until): (u32, u32),
    ) -> Branch {
        self.branch(label, self.range(stmts.iter(), at, Some(until)))
    }
}

impl<'a, 'arena> walker::Walker<'a, 'arena, State> for Collector<'a> {
    fn walk_in_class(&self, node: &cst::Class<'arena>, state: &mut State) {
        state.ctx.enter_class(node.name.value);
    }
    fn walk_out_class(&self, _: &cst::Class<'arena>, state: &mut State) {
        state.ctx.leave_class();
    }
    fn walk_in_interface(&self, node: &cst::Interface<'arena>, state: &mut State) {
        state.ctx.enter_class(node.name.value);
    }
    fn walk_out_interface(&self, _: &cst::Interface<'arena>, state: &mut State) {
        state.ctx.leave_class();
    }
    fn walk_in_trait(&self, node: &cst::Trait<'arena>, state: &mut State) {
        state.ctx.enter_class(node.name.value);
    }
    fn walk_out_trait(&self, _: &cst::Trait<'arena>, state: &mut State) {
        state.ctx.leave_class();
    }
    fn walk_in_enum(&self, node: &cst::Enum<'arena>, state: &mut State) {
        state.ctx.enter_class(node.name.value);
    }
    fn walk_out_enum(&self, _: &cst::Enum<'arena>, state: &mut State) {
        state.ctx.leave_class();
    }
    fn walk_in_anonymous_class(&self, node: &cst::AnonymousClass<'arena>, state: &mut State) {
        state.ctx.enter_anon(node);
    }
    fn walk_out_anonymous_class(&self, _: &cst::AnonymousClass<'arena>, state: &mut State) {
        state.ctx.leave_anon();
    }
    fn walk_in_method(&self, node: &cst::Method<'arena>, state: &mut State) {
        state.ctx.enter_method(node.name.value);
    }
    fn walk_out_method(&self, _: &cst::Method<'arena>, state: &mut State) {
        state.ctx.leave_method();
    }
    fn walk_in_closure(&self, _: &cst::Closure<'arena>, state: &mut State) {
        state.ctx.enter_closure();
    }
    fn walk_out_closure(&self, _: &cst::Closure<'arena>, state: &mut State) {
        state.ctx.leave_closure();
    }
    fn walk_in_arrow_function(&self, _: &cst::ArrowFunction<'arena>, state: &mut State) {
        state.ctx.enter_closure();
    }
    fn walk_out_arrow_function(&self, _: &cst::ArrowFunction<'arena>, state: &mut State) {
        state.ctx.leave_closure();
    }
    fn walk_in_partial_argument_list(
        &self,
        n: &cst::PartialArgumentList<'arena>,
        state: &mut State,
    ) {
        state.ctx.args_in(n.span().start.offset);
    }
    fn walk_out_partial_argument_list(
        &self,
        n: &cst::PartialArgumentList<'arena>,
        state: &mut State,
    ) {
        state.ctx.args_out(n.span().start.offset);
    }

    fn walk_in_if(&self, node: &cst::If<'arena>, state: &mut State) {
        let p = self.parsed;
        let at = p.lines_of(node).0;
        let cond = self.cond(node.condition);
        let mut branches = Vec::new();
        let label = format!("if ({cond})");
        match &node.body {
            IfBody::Statement(b) => {
                branches.push(self.branch(label, self.stmt_range(b.statement, at)));
                for e in b.else_if_clauses.iter() {
                    let label = format!("elseif ({})", self.cond(e.condition));
                    let fb = p.lines_of(e).0;
                    branches.push(self.branch(label, self.stmt_range(e.statement, fb)));
                }
                if let Some(e) = &b.else_clause {
                    let fb = p.lines_of(e).0;
                    branches.push(self.branch("else".into(), self.stmt_range(e.statement, fb)));
                }
            }
            IfBody::ColonDelimited(b) => {
                let starts: Vec<u32> = b
                    .else_if_clauses
                    .iter()
                    .map(|e| e.span().start.offset)
                    .chain(b.else_clause.iter().map(|e| e.span().start.offset))
                    .chain(std::iter::once(b.endif.span.start.offset))
                    .collect();
                branches.push(self.colon(label, &b.statements, (at, starts[0])));
                for (i, e) in b.else_if_clauses.iter().enumerate() {
                    let label = format!("elseif ({})", self.cond(e.condition));
                    let at = (p.lines_of(e).0, starts[i + 1]);
                    branches.push(self.colon(label, &e.statements, at));
                }
                if let Some(e) = &b.else_clause {
                    let at = (p.lines_of(e).0, b.endif.span.start.offset);
                    branches.push(self.colon("else".into(), &e.statements, at));
                }
            }
        }
        state.flows.push(ControlFlow {
            container: state.ctx.container.clone(),
            caller: Some(state.ctx.caller_name()),
            kind: "if".to_string(),
            line: at,
            cond: Some(cond),
            branches,
        });
    }
}

impl FileVisitor for IfElseVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut state = State::default();
        let collector = Collector { parsed };
        walker::walk_program(&collector, parsed.program, &mut state);
        for flow in state.flows {
            builder.control_flow(flow);
        }
    }
}
