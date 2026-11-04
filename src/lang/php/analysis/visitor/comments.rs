//! Sima kommentek gyűjtése a trivia-listából (a docblockok nélkül).

use super::FileVisitor;
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::model::FileGraphBuilder;
use mago_span::HasSpan;
use mago_syntax::cst::{IfBody, TriviaKind};
use mago_syntax::{cst, walker};

/// A `comments` mezőt tölti a jelzett vagy informatív sima kommentekkel.
#[derive(Debug, Default, Clone, Copy)]
pub struct CommentsVisitor;

impl FileVisitor for CommentsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut containers = Vec::new();
        let mut methods = Vec::new();
        collect_scopes(parsed.program, &mut containers, &mut methods);
        let mut state = CommentState::default();
        walker::walk_program(&CommentCollector, parsed.program, &mut state);
        for trivia in parsed.program.trivia.iter() {
            if matches!(
                trivia.kind,
                TriviaKind::SingleLineComment
                    | TriviaKind::MultiLineComment
                    | TriviaKind::HashComment
            ) && !state.condition_gaps.iter().any(|span| {
                contains(*span, trivia.span)
                    && parsed.line_of(span.start.offset) == parsed.line_of(trivia.span.start.offset)
            }) && !is_expression_continuation_comment(parsed, trivia.span)
                && let Some(text) = clean_plain_comment(&String::from_utf8_lossy(trivia.value))
            {
                let upper = text.to_ascii_uppercase();
                let tag = ["FIXME", "TODO", "XXX", "HACK"]
                    .iter()
                    .find(|tag| {
                        upper
                            .split(|c: char| !c.is_ascii_alphanumeric())
                            .any(|word| word == **tag)
                    })
                    .map(|tag| (*tag).to_string());
                if tag.is_some() || text.chars().count() >= 15 {
                    let method = methods.iter().rev().find(|(span, _)| {
                        span.start.offset <= trivia.span.start.offset
                            && trivia.span.end.offset <= span.end.offset
                    });
                    let container = containers.iter().rev().find(|(span, _)| {
                        span.start.offset <= trivia.span.start.offset
                            && trivia.span.end.offset <= span.end.offset
                    });
                    let text = if text.chars().count() > 150 {
                        text.chars().take(150).collect::<String>() + "…"
                    } else {
                        text
                    };
                    builder.comment(crate::model::Comment {
                        container: container.map(|(_, n)| n.clone()),
                        caller: Some(
                            method.map_or_else(|| "__top__".to_string(), |(_, n)| n.clone()),
                        ),
                        line: parsed.line_of(trivia.span.start.offset),
                        tag,
                        text,
                    });
                }
            }
        }
    }
}

#[derive(Default)]
struct CommentState {
    condition_gaps: Vec<mago_span::Span>,
}

struct CommentCollector;

impl<'ast, 'arena> walker::Walker<'ast, 'arena, CommentState> for CommentCollector {
    fn walk_in_if(&self, node: &cst::If<'arena>, state: &mut CommentState) {
        match &node.body {
            IfBody::Statement(body) => {
                add_condition_gap(node.condition, body.statement, state);
                for clause in body.else_if_clauses.iter() {
                    add_condition_gap(clause.condition, clause.statement, state);
                }
                if let Some(clause) = &body.else_clause
                    && matches!(clause.statement, cst::Statement::Block(_))
                {
                    add_gap(clause.r#else.span(), clause.statement, state);
                }
            }
            IfBody::ColonDelimited(body) => {
                if let Some(statement) = body.statements.first() {
                    add_condition_gap(node.condition, statement, state);
                }
                for clause in body.else_if_clauses.iter() {
                    if let Some(statement) = clause.statements.first() {
                        add_condition_gap(clause.condition, statement, state);
                    }
                }
                if let Some(clause) = &body.else_clause
                    && let Some(statement) = clause.statements.first()
                {
                    add_gap(clause.r#else.span(), statement, state);
                }
            }
        }
    }
}

fn add_condition_gap(
    condition: &cst::Expression<'_>,
    statement: &cst::Statement<'_>,
    state: &mut CommentState,
) {
    if !matches!(statement, cst::Statement::Block(_)) {
        return;
    }
    add_gap(condition.span(), statement, state);
}

fn add_gap(preceding: mago_span::Span, statement: &cst::Statement<'_>, state: &mut CommentState) {
    let start = preceding.end.offset;
    let end = statement.span().start.offset;
    if start < end {
        let mut gap = preceding;
        gap.start = preceding.end;
        gap.end = statement.span().start;
        state.condition_gaps.push(gap);
    }
}

fn contains(outer: mago_span::Span, inner: mago_span::Span) -> bool {
    outer.start.offset <= inner.start.offset && inner.end.offset <= outer.end.offset
}

fn is_expression_continuation_comment(parsed: &ParsedFile<'_>, span: mago_span::Span) -> bool {
    let source = parsed.full_lines(span.start.offset.saturating_sub(512), span.end.offset);
    let lines: Vec<_> = source.lines().collect();
    let Some(current) = lines.last() else {
        return false;
    };
    let before = current.split("//").next().unwrap_or_default().trim_end();
    let previous = lines
        .iter()
        .rev()
        .skip(1)
        .find(|line| !line.trim_start().starts_with("//"))
        .copied()
        .unwrap_or_default();
    let trailing_expression = !before.is_empty()
        && (before.ends_with(')')
            || before.ends_with(']')
            || before.ends_with(',')
            || before.ends_with("else"));
    let continued_chain = before.is_empty()
        && (previous.contains("->") || previous.contains("::"))
        && (previous.trim_end().ends_with(')') || previous.trim_end().ends_with("]"));
    let next = parsed.full_lines(span.end.offset, span.end.offset.saturating_add(512));
    let next_token = next.lines().nth(1).map(str::trim).unwrap_or_default();
    let closes_expression = next_token.is_empty() || next_token.starts_with(']');
    (trailing_expression && closes_expression) || continued_chain
}

fn clean_plain_comment(raw: &str) -> Option<String> {
    let lines: Vec<_> = raw
        .lines()
        .map(|line| {
            let mut line = line.trim();
            line = line.trim_start_matches("/*");
            line = line.trim_end_matches("*/");
            line = line
                .strip_prefix("//")
                .map_or(line, |rest| rest.strip_prefix(' ').unwrap_or(rest));
            line = line
                .strip_prefix('*')
                .map_or(line, |rest| rest.strip_prefix(' ').unwrap_or(rest));
            line.trim()
        })
        .filter(|line| !line.is_empty())
        .collect();
    let text = lines.join(" ");
    (!text.is_empty()).then_some(text)
}

fn collect_scopes(
    program: &mago_syntax::cst::Program<'_>,
    containers: &mut Vec<(mago_span::Span, String)>,
    methods: &mut Vec<(mago_span::Span, String)>,
) {
    use mago_syntax::cst::{ClassLikeMember, Statement};
    fn statements<'a>(
        ss: impl IntoIterator<Item = &'a Statement<'a>>,
        c: &mut Vec<(mago_span::Span, String)>,
        m: &mut Vec<(mago_span::Span, String)>,
    ) {
        for s in ss {
            match s {
                Statement::Class(x) => members(x.span(), x.name.value, &x.members, c, m),
                Statement::Interface(x) => members(x.span(), x.name.value, &x.members, c, m),
                Statement::Trait(x) => members(x.span(), x.name.value, &x.members, c, m),
                Statement::Enum(x) => members(x.span(), x.name.value, &x.members, c, m),
                Statement::Namespace(x) => statements(x.statements(), c, m),
                _ => {}
            }
        }
    }
    fn members<'a>(
        span: mago_span::Span,
        name: &[u8],
        ms: &mago_syntax::cst::Sequence<'a, ClassLikeMember<'a>>,
        c: &mut Vec<(mago_span::Span, String)>,
        m: &mut Vec<(mago_span::Span, String)>,
    ) {
        c.push((span, String::from_utf8_lossy(name).into_owned()));
        for member in ms {
            if let ClassLikeMember::Method(method) = member {
                m.push((
                    method.span(),
                    String::from_utf8_lossy(method.name.value).into_owned(),
                ));
            }
        }
    }
    statements(program.statements.iter(), containers, methods);
}
