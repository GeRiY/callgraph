//! Vezérlés (`controlFlow`): match-karok és switch-case-ek (fall-through jelöléssel).

use mago_span::HasSpan;
use mago_syntax::cst::{Expression, Match, MatchArm, Statement, Switch, SwitchBody, SwitchCase};
use mago_syntax::{cst, walker};

use super::{FileVisitor, context::Ctx};
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::lang::php::printer::print_expr;
use crate::model::{Branch, ControlFlow, FileGraphBuilder};

/// A `controlFlow` match- és switch-sorait tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct MatchSwitchVisitor;

#[derive(Default)]
struct State {
    ctx: Ctx,
    rows: Vec<ControlFlow>,
}

struct Collector<'a> {
    parsed: &'a ParsedFile<'a>,
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

    fn walk_in_match(&self, node: &Match<'arena>, state: &mut State) {
        let branches = node.arms.iter().map(|arm| self.match_branch(arm)).collect();
        self.push(state, "match", node.span(), node.expression, branches);
    }
    fn walk_in_switch(&self, node: &Switch<'arena>, state: &mut State) {
        let cases = match &node.body {
            SwitchBody::BraceDelimited(b) => &b.cases,
            SwitchBody::ColonDelimited(b) => &b.cases,
        };
        let total = cases.len();
        let branches = cases
            .iter()
            .enumerate()
            .map(|(idx, case)| self.switch_branch(case, idx + 1 < total))
            .collect();
        self.push(state, "switch", node.span(), node.expression, branches);
    }
}

impl Collector<'_> {
    fn push(
        &self,
        state: &mut State,
        kind: &str,
        span: mago_span::Span,
        cond: &Expression<'_>,
        branches: Vec<Branch>,
    ) {
        state.rows.push(ControlFlow {
            container: state.ctx.container.clone(),
            caller: Some(state.ctx.caller_name()),
            kind: kind.to_string(),
            line: self.parsed.line_of(span.start.offset),
            cond: Some(self.short(cond)),
            branches,
        });
    }

    /// Kifejezés rövid forrás-szövege (80 karakterre vágva, `…`-tal).
    fn short(&self, expr: &Expression<'_>) -> String {
        let text = print_expr(self.parsed, expr);
        if text.chars().count() > 80 {
            let mut cut: String = text.chars().take(80).collect();
            cut.push('…');
            cut
        } else {
            text
        }
    }

    fn match_branch(&self, arm: &MatchArm<'_>) -> Branch {
        let (label, body) = match arm {
            MatchArm::Default(d) => ("default".to_string(), d.expression),
            MatchArm::Expression(e) => {
                let conds: Vec<String> = e.conditions.iter().map(|c| self.short(c)).collect();
                (conds.join(", "), e.expression)
            }
        };
        let (start_line, end_line) = self.parsed.lines_of(body);
        Branch {
            label,
            start_line,
            end_line,
            fall_through: None,
        }
    }

    fn switch_branch(&self, case: &SwitchCase<'_>, has_next: bool) -> Branch {
        let (label, stmts, case_line) = match case {
            SwitchCase::Default(d) => (
                "default".to_string(),
                &d.statements,
                self.parsed.line_of(d.span().start.offset),
            ),
            SwitchCase::Expression(e) => (
                format!("case {}", self.short(e.expression)),
                &e.statements,
                self.parsed.line_of(e.span().start.offset),
            ),
        };
        let (start_line, end_line) = match (stmts.first(), stmts.last()) {
            (Some(first), Some(last)) => {
                (self.parsed.lines_of(first).0, self.parsed.lines_of(last).1)
            }
            _ => (case_line, case_line),
        };
        // A nikic-referencia csak Break/Return/Continue utasítást tekint lezárásnak.
        let terminated = matches!(
            stmts.last(),
            Some(Statement::Break(_) | Statement::Return(_) | Statement::Continue(_))
        );
        Branch {
            label,
            start_line,
            end_line,
            fall_through: (has_next && !stmts.is_empty() && !terminated).then_some(true),
        }
    }
}

impl FileVisitor for MatchSwitchVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut state = State::default();
        let collector = Collector { parsed };
        walker::walk_program(&collector, parsed.program, &mut state);
        for row in state.rows {
            builder.control_flow(row);
        }
    }
}
