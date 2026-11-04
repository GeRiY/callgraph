//! Property-írások (`writes`): `$x->prop = …` (assign).
//!
//! Az `ast_graph.php` `handleAssign` párja (a `handleAssignOp` a referenciában holt kód, lásd lent). A kontextus (container, caller)
//! a referencia állapotgépét követi: konténer-belépéskor beáll, kilépéskor nullázódik
//! (anonim osztályra is), a metódus-kilépés a callert `__top__`-ra állítja vissza.

use mago_syntax::cst::cst::*;
use mago_syntax::walker::{Walker, walk_program};

use super::FileVisitor;
use crate::lang::php::analysis::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::model::{FileGraphBuilder, Write};

/// A `writes` mezőt tölti.
#[derive(Debug, Default, Clone, Copy)]
pub struct WritesVisitor;

impl FileVisitor for WritesVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        _ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut state = State {
            parsed,
            container: None,
            method: None,
            closure_depth: 0,
            writes: Vec::new(),
        };
        walk_program(&Collector, parsed.program, &mut state);
        for w in state.writes {
            builder.write(w);
        }
    }
}

struct State<'p, 'a> {
    parsed: &'p ParsedFile<'a>,
    container: Option<String>,
    method: Option<String>,
    closure_depth: u32,
    writes: Vec<Write>,
}

impl State<'_, '_> {
    fn enter_container(&mut self, name: &[u8]) {
        self.container = Some(String::from_utf8_lossy(name).into_owned());
    }

    fn leave_container(&mut self) {
        self.container = None;
    }

    fn record(&mut self, assignment: &Assignment<'_>, kind: &str) {
        let Expression::Access(Access::Property(access)) = assignment.lhs.unparenthesized() else {
            return;
        };
        let ClassLikeMemberSelector::Identifier(prop) = &access.property else {
            return;
        };
        let on = match access.object {
            Expression::Variable(Variable::Direct(v)) => {
                String::from_utf8_lossy(v.name).into_owned()
            }
            _ => "<expr>".to_string(),
        };
        self.writes.push(Write {
            container: self.container.clone(),
            caller: self.method.clone().unwrap_or_else(|| "__top__".into()),
            line: self.parsed.lines_of(assignment).0,
            kind: kind.to_string(),
            on,
            property: String::from_utf8_lossy(prop.value).into_owned(),
        });
    }
}

struct Collector;

impl<'ast, 'arena, 'p, 'a> Walker<'ast, 'arena, State<'p, 'a>> for Collector {
    fn walk_in_class(&self, n: &'ast Class<'arena>, s: &mut State<'p, 'a>) {
        s.enter_container(n.name.value);
    }
    fn walk_out_class(&self, _: &'ast Class<'arena>, s: &mut State<'p, 'a>) {
        s.leave_container();
    }
    fn walk_in_interface(&self, n: &'ast Interface<'arena>, s: &mut State<'p, 'a>) {
        s.enter_container(n.name.value);
    }
    fn walk_out_interface(&self, _: &'ast Interface<'arena>, s: &mut State<'p, 'a>) {
        s.leave_container();
    }
    fn walk_in_trait(&self, n: &'ast Trait<'arena>, s: &mut State<'p, 'a>) {
        s.enter_container(n.name.value);
    }
    fn walk_out_trait(&self, _: &'ast Trait<'arena>, s: &mut State<'p, 'a>) {
        s.leave_container();
    }
    fn walk_in_enum(&self, n: &'ast Enum<'arena>, s: &mut State<'p, 'a>) {
        s.enter_container(n.name.value);
    }
    fn walk_out_enum(&self, _: &'ast Enum<'arena>, s: &mut State<'p, 'a>) {
        s.leave_container();
    }
    fn walk_in_anonymous_class(&self, _: &'ast AnonymousClass<'arena>, s: &mut State<'p, 'a>) {
        s.container = Some("<anonymous>".into());
    }
    fn walk_out_anonymous_class(&self, _: &'ast AnonymousClass<'arena>, s: &mut State<'p, 'a>) {
        s.leave_container();
    }

    fn walk_in_method(&self, n: &'ast Method<'arena>, s: &mut State<'p, 'a>) {
        let name = String::from_utf8_lossy(n.name.value).into_owned();
        s.method = Some(if s.closure_depth > 0 {
            format!("{name}.<closure>{}", s.closure_depth)
        } else {
            name
        });
    }
    fn walk_out_method(&self, _: &'ast Method<'arena>, s: &mut State<'p, 'a>) {
        s.method = None;
    }

    // A referencia a closure-mélységet a `Stmt\Expression(Closure)` és a `Closure` belépésnél
    // is növeli (a statement-szintű closure így 2-t számol).
    fn walk_in_statement_expression(
        &self,
        n: &'ast ExpressionStatement<'arena>,
        s: &mut State<'p, 'a>,
    ) {
        if matches!(n.expression, Expression::Closure(_)) {
            s.closure_depth += 1;
        }
    }
    fn walk_out_statement_expression(
        &self,
        n: &'ast ExpressionStatement<'arena>,
        s: &mut State<'p, 'a>,
    ) {
        if matches!(n.expression, Expression::Closure(_)) {
            s.closure_depth = s.closure_depth.saturating_sub(1);
        }
    }
    fn walk_in_closure(&self, _: &'ast Closure<'arena>, s: &mut State<'p, 'a>) {
        s.closure_depth += 1;
    }
    fn walk_out_closure(&self, _: &'ast Closure<'arena>, s: &mut State<'p, 'a>) {
        s.closure_depth = s.closure_depth.saturating_sub(1);
    }
    fn walk_in_arrow_function(&self, _: &'ast ArrowFunction<'arena>, s: &mut State<'p, 'a>) {
        s.closure_depth += 1;
    }
    fn walk_out_arrow_function(&self, _: &'ast ArrowFunction<'arena>, s: &mut State<'p, 'a>) {
        s.closure_depth = s.closure_depth.saturating_sub(1);
    }

    fn walk_in_assignment(&self, n: &'ast Assignment<'arena>, s: &mut State<'p, 'a>) {
        // A referencia dispatch-táblája az absztrakt `AssignOp` osztályra van kulcsolva, amit a
        // `get_class()` sosem ad vissza, így a `handleAssignOp` soha nem fut: compound írás
        // (`+=`, `.=`, `??=` …) a referenciában nincs. A paritás kedvéért itt sem rögzítjük.
        if n.operator.is_assign() {
            s.record(n, "assign");
        }
    }
}
