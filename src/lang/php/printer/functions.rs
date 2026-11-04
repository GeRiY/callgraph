//! Closure- és arrow-function kifejezések nyomtatása.

use mago_span::HasSpan;
use mago_syntax::cst::*;

use super::Printer;
use super::text::lossy;

impl Printer<'_, '_> {
    pub(super) fn nl(&self) -> String {
        format!("\n{}", "    ".repeat(self.indent))
    }

    pub(super) fn comments_between(&self, from: u32, to: u32) -> Vec<String> {
        let trivia = &self.parsed.program.trivia.nodes;
        let start = trivia.partition_point(|t| t.span.start.offset < from);
        trivia
            .iter()
            .skip(start)
            .take_while(|t| t.span.end.offset <= to)
            .filter(|t| !matches!(t.kind, TriviaKind::WhiteSpace))
            .map(|t| lossy(t.value).trim().to_string())
            .collect()
    }

    fn hint_text(&self, node: &impl HasSpan) -> String {
        self.src(node)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    fn params(&mut self, list: &FunctionLikeParameterList<'_>) -> String {
        list.parameters
            .iter()
            .map(|p| {
                let mut s = String::new();
                for m in p.modifiers.iter() {
                    s.push_str(&self.src(m).to_ascii_lowercase());
                    s.push(' ');
                }
                if let Some(h) = &p.hint {
                    s.push_str(&self.hint_text(h));
                    s.push(' ');
                }
                if p.ampersand.is_some() {
                    s.push('&');
                }
                if p.ellipsis.is_some() {
                    s.push_str("...");
                }
                s.push_str(&lossy(p.variable.name));
                if let Some(d) = &p.default_value {
                    s.push_str(" = ");
                    s.push_str(&self.expr(d.value));
                }
                s
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn return_type(&self, r: &Option<FunctionLikeReturnTypeHint<'_>>) -> String {
        r.as_ref()
            .map(|r| format!(": {}", self.hint_text(&r.hint)))
            .unwrap_or_default()
    }

    pub(super) fn closure(&mut self, c: &Closure<'_>) -> String {
        let mut s = String::new();
        if c.r#static.is_some() {
            s.push_str("static ");
        }
        s.push_str("function ");
        if c.ampersand.is_some() {
            s.push('&');
        }
        s.push_str(&format!("({})", self.params(&c.parameter_list)));
        if let Some(u) = &c.use_clause {
            let vars = u
                .variables
                .iter()
                .map(|v| {
                    format!(
                        "{}{}",
                        if v.ampersand.is_some() { "&" } else { "" },
                        lossy(v.variable.name)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            s.push_str(&format!(" use ({vars})"));
        }
        s.push_str(&self.return_type(&c.return_type_hint));
        s.push_str(" {");
        s.push_str(&self.block_body(c.body.left_brace.end.offset, &c.body.statements));
        s.push_str(&self.nl());
        s.push('}');
        s
    }

    pub(super) fn arrow_head(&mut self, f: &ArrowFunction<'_>) -> String {
        let st = if f.r#static.is_some() { "static " } else { "" };
        let amp = if f.ampersand.is_some() { "&" } else { "" };
        format!(
            "{st}fn{amp}({}){} => ",
            self.params(&f.parameter_list),
            self.return_type(&f.return_type_hint),
        )
    }

    fn block_body(&mut self, mut prev: u32, stmts: &Sequence<'_, Statement<'_>>) -> String {
        self.indent += 1;
        let mut out = String::new();
        for st in stmts.iter() {
            for c in self.comments_between(prev, st.span().start.offset) {
                out.push_str(&self.nl());
                out.push_str(&c.replace('\n', &self.nl()));
            }
            out.push_str(&self.nl());
            out.push_str(&self.stmt(st));
            prev = st.span().end.offset;
        }
        self.indent -= 1;
        out
    }

    fn stmt(&mut self, st: &Statement<'_>) -> String {
        match st {
            Statement::Expression(e) => format!("{};", self.expr(e.expression)),
            Statement::Return(r) => r
                .value
                .map_or_else(|| "return;".into(), |v| format!("return {};", self.expr(v))),
            Statement::If(i) => self.if_stmt(i),
            Statement::Block(b) => {
                let body = self.block_body(b.left_brace.end.offset, &b.statements);
                format!("{{{body}{}}}", self.nl())
            }
            other => self.src(other),
        }
    }

    fn braced(&mut self, st: &Statement<'_>) -> String {
        let (start, list): (u32, Vec<&Statement<'_>>) = match st {
            Statement::Block(b) => (b.left_brace.end.offset, b.statements.iter().collect()),
            other => (other.span().start.offset, vec![other]),
        };
        self.indent += 1;
        let mut out = String::new();
        let mut prev = start;
        for statement in list {
            for comment in self.comments_between(prev, statement.span().start.offset) {
                out.push_str(&self.nl());
                out.push_str(&comment.replace('\n', &self.nl()));
            }
            out.push_str(&self.nl());
            out.push_str(&self.stmt(statement));
            prev = statement.span().end.offset;
        }
        self.indent -= 1;
        format!("{{{out}{}}}", self.nl())
    }

    fn if_stmt(&mut self, i: &If<'_>) -> String {
        let cond = self.expr(i.condition);
        let IfBody::Statement(body) = &i.body else {
            return self.src(i);
        };
        let mut out = format!("if ({cond}) {}", self.braced(body.statement));
        for clause in body.else_if_clauses.iter() {
            out.push_str(&format!(
                " elseif ({}) {}",
                self.expr(clause.condition),
                self.braced(clause.statement)
            ));
        }
        if let Some(clause) = &body.else_clause {
            out.push_str(&format!(" else {}", self.braced(clause.statement)));
        }
        out
    }
}
