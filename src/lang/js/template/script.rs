//! Template expressions through the script analyser: an expression is parsed with oxc and walked by the same
//! `Walker` + `Scope` as a `<script>` block, so a template call gets the category and `resolved` of the script call.

use std::path::PathBuf;
use std::sync::OnceLock;

use oxc_allocator::Allocator;
use oxc_ast::ast::BindingIdentifier;
use oxc_ast_visit::Visit;
use oxc_span::SourceType;
use regex::Regex;

use crate::lang::js::analysis::PassContext;
use crate::lang::js::calls::{Scope, Walker};
use crate::lang::js::lines::LineIndex;
use crate::lang::js::methods::{Docs, MethodIndex};
use crate::lang::js::parse::parse_program;
use crate::lang::js::resolve::Resolver;
use crate::model::Call;

/// Walks template expressions in the script's scope.
pub struct ExprAnalyzer {
    path: PathBuf,
    index: MethodIndex,
    resolver: Resolver,
    scope: Scope,
}

impl ExprAnalyzer {
    /// `scope`: the merged script blocks' scope; `methods`: the file's method names (the Options API `methods` /
    /// `computed` too).
    pub fn new(
        path: PathBuf,
        container: &str,
        mut scope: Scope,
        methods: impl IntoIterator<Item = String>,
    ) -> Self {
        scope.template = true;
        scope.names.extend(methods);
        Self {
            path,
            index: MethodIndex {
                names: Default::default(),
                fns: Default::default(),
                main: container.to_string(),
            },
            resolver: Resolver::new(),
            scope,
        }
    }

    /// The calls of one expression (`handler`: an `@x` value, which may also be a statement list like
    /// `a(); b++`); `None` when it does not parse (Vue-specific or malformed syntax): the caller falls back.
    pub fn calls(&mut self, exp: &str, line: u32, handler: bool) -> Option<Vec<Call>> {
        let wrapped = format!("({exp}\n);");
        if let Some(calls) = self.walk(&wrapped, line) {
            return Some(calls);
        }
        if handler {
            return self.walk(exp, line);
        }
        None
    }

    fn walk(&mut self, source: &str, line: u32) -> Option<Vec<Call>> {
        let allocator = Allocator::default();
        let parsed = parse_program(&allocator, source, SourceType::ts());
        if parsed.fatal || !parsed.errors.is_empty() {
            return None;
        }
        let lines = LineIndex::with_base(source, line);
        let ctx = PassContext {
            path: &self.path,
            source,
            lines: &lines,
            setup: false,
        };
        let docs = Docs {
            source,
            comments: &parsed.program.comments,
        };
        let mut walker = Walker::new(
            &ctx,
            &docs,
            &self.index,
            &self.resolver,
            &mut self.scope,
            "template",
        );
        walker.run(&parsed.program);
        Some(walker.calls)
    }
}

/// The names a binding pattern declares (`item`, `(item, i)`, `{ a, b: c }`, `[x, y]`): the `v-for` item part and
/// the `v-slot` / `#name="…"` value.
pub fn pattern_names(pattern: &str) -> Vec<String> {
    let pattern = pattern.trim();
    let inner = pattern
        .strip_prefix('(')
        .and_then(|p| p.strip_suffix(')'))
        .unwrap_or(pattern);
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let source = format!("({inner}) => 0;");
    let allocator = Allocator::default();
    let parsed = parse_program(&allocator, &source, SourceType::ts());
    if parsed.fatal || !parsed.errors.is_empty() {
        // not parseable: every identifier-like word is taken as a local (hiding is cheaper than a false call)
        return identifier()
            .find_iter(inner)
            .map(|m| m.as_str().to_string())
            .collect();
    }
    let mut names = Names::default();
    names.visit_program(&parsed.program);
    names.0
}

/// `item in list` / `(a, b) of list`: (the item part, the source expression).
pub fn for_parts(exp: &str) -> Option<(&str, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r"^([\s\S]*?)\s+(?:in|of)\s+([\s\S]*)$").expect("valid regex"));
    let m = re.captures(exp)?;
    Some((m.get(1)?.as_str(), m.get(2)?.as_str()))
}

fn identifier() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z_$][A-Za-z0-9_$]*").expect("valid regex"))
}

#[derive(Default)]
struct Names(Vec<String>);

impl<'a> Visit<'a> for Names {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.push(it.name.to_string());
    }
}

/// A call whose callee root is a template-local name (a `v-for` item, a slot prop): not a call to a known or
/// unknown function of the component.
pub fn is_local_call(call: &Call, locals: &[String]) -> bool {
    if locals.is_empty() || !matches!(call.kind.as_str(), "func_call" | "method_call") {
        return false;
    }
    let head = call.on.as_deref().unwrap_or(&call.target);
    locals.iter().any(|l| l == head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_names_cover_the_vue_forms() {
        assert_eq!(pattern_names("item"), ["item"]);
        assert_eq!(pattern_names("(item, i)"), ["item", "i"]);
        assert_eq!(pattern_names("{ a, b: c }"), ["a", "c"]);
        assert_eq!(pattern_names("({ id }, idx)"), ["id", "idx"]);
        assert_eq!(pattern_names("[x, y]"), ["x", "y"]);
        assert!(pattern_names("").is_empty());
    }

    #[test]
    fn for_parts_split_on_in_and_of() {
        assert_eq!(for_parts("(a, b) in rows()"), Some(("(a, b)", "rows()")));
        assert_eq!(for_parts("x of xs"), Some(("x", "xs")));
        assert_eq!(for_parts("nothing"), None);
    }
}
