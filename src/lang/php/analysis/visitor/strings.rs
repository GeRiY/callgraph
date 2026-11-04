//! String-literálok (`strings`) összegyűjtése.

use mago_syntax::{cst, walker};

use super::FileVisitor;
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::model::{FileGraphBuilder, StringLit};

#[derive(Debug, Default, Clone, Copy)]
pub struct StringsVisitor;

#[derive(Default)]
struct State {
    container: Option<String>,
    caller: Option<String>,
    snapshots: Vec<(Option<String>, Option<String>)>,
    strings: Vec<StringLit>,
    closure_depth: usize,
}

struct Collector<'a> {
    parsed: &'a ParsedFile<'a>,
}

impl<'a, 'arena> walker::Walker<'a, 'arena, State> for Collector<'a> {
    fn walk_in_instantiation(&self, node: &cst::Instantiation<'arena>, state: &mut State) {
        if matches!(node.class, cst::Expression::AnonymousClass(_)) {
            state.container = None;
            state.caller = None;
        }
    }

    fn walk_in_class(&self, node: &cst::Class<'arena>, state: &mut State) {
        enter_container(state, String::from_utf8_lossy(node.name.value).into_owned());
    }
    fn walk_out_class(&self, _: &cst::Class<'arena>, state: &mut State) {
        restore(state);
    }
    fn walk_in_interface(&self, node: &cst::Interface<'arena>, state: &mut State) {
        enter_container(state, String::from_utf8_lossy(node.name.value).into_owned());
    }
    fn walk_out_interface(&self, _: &cst::Interface<'arena>, state: &mut State) {
        restore(state);
    }
    fn walk_in_trait(&self, node: &cst::Trait<'arena>, state: &mut State) {
        enter_container(state, String::from_utf8_lossy(node.name.value).into_owned());
    }
    fn walk_out_trait(&self, _: &cst::Trait<'arena>, state: &mut State) {
        restore(state);
    }
    fn walk_in_enum(&self, node: &cst::Enum<'arena>, state: &mut State) {
        enter_container(state, String::from_utf8_lossy(node.name.value).into_owned());
    }
    fn walk_out_enum(&self, _: &cst::Enum<'arena>, state: &mut State) {
        restore(state);
    }
    fn walk_in_anonymous_class(&self, _: &cst::AnonymousClass<'arena>, state: &mut State) {
        enter_container(state, "<anonymous>".into());
    }
    fn walk_out_anonymous_class(&self, _: &cst::AnonymousClass<'arena>, state: &mut State) {
        restore(state);
        state.container = None;
        state.caller = None;
    }
    fn walk_in_method(&self, node: &cst::Method<'arena>, state: &mut State) {
        state
            .snapshots
            .push((state.container.clone(), state.caller.take()));
        let name = String::from_utf8_lossy(node.name.value);
        state.caller = Some(if state.container.as_deref() == Some("<anonymous>") {
            caller_name(&name, state.closure_depth)
        } else {
            name.into_owned()
        });
    }
    fn walk_out_method(&self, _: &cst::Method<'arena>, state: &mut State) {
        restore(state);
    }
    fn walk_in_closure(&self, _: &cst::Closure<'arena>, state: &mut State) {
        state
            .snapshots
            .push((state.container.clone(), state.caller.clone()));
        state.closure_depth += 1;
        if state.container.as_deref() == Some("<anonymous>")
            && let Some(caller) = state.caller.clone()
            && !caller.contains(".<closure>")
        {
            state.caller = Some(caller_name(&caller, state.closure_depth));
        }
    }
    fn walk_out_closure(&self, _: &cst::Closure<'arena>, state: &mut State) {
        state.closure_depth = state.closure_depth.saturating_sub(1);
        restore(state);
    }
    fn walk_in_arrow_function(&self, _: &cst::ArrowFunction<'arena>, state: &mut State) {
        state
            .snapshots
            .push((state.container.clone(), state.caller.clone()));
        state.closure_depth += 1;
        if state.container.as_deref() == Some("<anonymous>")
            && let Some(caller) = state.caller.clone()
            && !caller.contains(".<closure>")
        {
            state.caller = Some(caller_name(&caller, state.closure_depth));
        }
    }
    fn walk_out_arrow_function(&self, _: &cst::ArrowFunction<'arena>, state: &mut State) {
        state.closure_depth = state.closure_depth.saturating_sub(1);
        restore(state);
    }
    fn walk_in_literal_string(&self, node: &cst::LiteralString<'arena>, state: &mut State) {
        add(self.parsed, node.value.or(Some(node.raw)), node, state);
    }
    fn walk_in_document_string(&self, node: &cst::DocumentString<'arena>, state: &mut State) {
        let mut value = Vec::new();
        for part in &node.parts {
            if let cst::StringPart::Literal(part) = part {
                value.extend_from_slice(part.value.unwrap_or(part.raw));
            }
        }
        add(self.parsed, Some(&value), node, state);
    }
}

fn enter_container(state: &mut State, name: String) {
    state
        .snapshots
        .push((state.container.take(), state.caller.take()));
    state.container = Some(name);
}

fn restore(state: &mut State) {
    if let Some((container, caller)) = state.snapshots.pop() {
        if state.container.is_some() {
            state.container = container;
        }
        state.caller = caller;
    }
}

fn add(
    parsed: &ParsedFile<'_>,
    value: Option<&[u8]>,
    node: &impl mago_span::HasSpan,
    state: &mut State,
) {
    let Some(value) = value else { return };
    let value = String::from_utf8_lossy(value).into_owned();
    if value.chars().count() < 3 {
        return;
    }
    let truncated = value.chars().count() > 120;
    let mut value: String = value.chars().take(120).collect();
    if truncated {
        value.push('…');
    }
    state.strings.push(StringLit {
        container: state.container.clone(),
        caller: Some(state.caller.as_deref().unwrap_or("__top__").to_string()),
        line: parsed.lines_of(node).0,
        value,
        end_line: None,
    });
}

fn caller_name(base: &str, depth: usize) -> String {
    if depth == 0 {
        base.to_string()
    } else {
        format!("{base}.<closure>{depth}")
    }
}

impl FileVisitor for StringsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut state = State::default();
        let collector = Collector { parsed };
        walker::walk_program(&collector, parsed.program, &mut state);
        for string in state.strings {
            builder.string(string);
        }
    }
}
