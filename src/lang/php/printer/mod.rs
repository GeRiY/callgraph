//! Közös PHP-kifejezés-nyomtató a hívás- és vezérlés-visitorok számára.

mod args;
mod arrays;
mod constructs;
mod functions;
mod ops;
mod text;

use crate::lang::php::parser::ParsedFile;
use mago_syntax::cst::*;
use text::{escape_dq, escape_heredoc, escape_sq, float_text, int_text, strip};

/// Normalizált PHP-kifejezést állít elő a nikic printer alakjában.
pub struct Printer<'p, 'a> {
    pub parsed: &'p ParsedFile<'a>,
    pub(crate) indent: usize,
}

impl<'p, 'a> Printer<'p, 'a> {
    pub fn new(parsed: &'p ParsedFile<'a>) -> Self {
        Self { parsed, indent: 0 }
    }
    pub fn print(parsed: &'p ParsedFile<'a>, e: &Expression<'a>) -> String {
        Self::new(parsed).expr(e)
    }
    fn src(&self, n: &impl mago_span::HasSpan) -> String {
        self.parsed.text(n.span())
    }
    fn lhs(&mut self, e: &Expression<'_>) -> String {
        let e = strip(e);
        let s = self.expr(e);
        if matches!(
            e,
            Expression::Variable(_)
                | Expression::Identifier(_)
                | Expression::ConstantAccess(_)
                | Expression::ArrayAccess(_)
                | Expression::Access(_)
                | Expression::Call(_)
                | Expression::Array(_)
                | Expression::LegacyArray(_)
                | Expression::Self_(_)
                | Expression::Static(_)
                | Expression::Parent(_)
                | Expression::Literal(Literal::String(_))
        ) {
            s
        } else {
            format!("({s})")
        }
    }
    fn call_lhs(&mut self, e: &Expression<'_>) -> String {
        let e = strip(e);
        let s = self.expr(e);
        if matches!(
            e,
            Expression::Variable(_)
                | Expression::Identifier(_)
                | Expression::ArrayAccess(_)
                | Expression::Call(_)
                | Expression::Array(_)
                | Expression::LegacyArray(_)
        ) {
            s
        } else {
            format!("({s})")
        }
    }
    pub fn expr(&mut self, e: &Expression<'_>) -> String {
        self.p(e, ops::MAX, ops::MAX)
    }
    pub(crate) fn p(&mut self, e: &Expression<'_>, prec: i32, lhs: i32) -> String {
        if let Some(s) = self.operator(e, prec, lhs) {
            return s;
        }
        match strip(e) {
            Expression::UnaryPostfix(u) => {
                let x = self.expr(u.operand);
                let op = match u.operator {
                    UnaryPostfixOperator::PostIncrement(_) => "++",
                    UnaryPostfixOperator::PostDecrement(_) => "--",
                };
                format!("{x}{op}")
            }
            Expression::Literal(Literal::String(s)) => match s.kind {
                LiteralStringKind::SingleQuoted => {
                    format!("'{}'", escape_sq(s.value.unwrap_or(s.raw)))
                }
                LiteralStringKind::DoubleQuoted => {
                    format!("\"{}\"", escape_dq(s.value.unwrap_or(s.raw)))
                }
            },
            Expression::Literal(Literal::Integer(i)) => int_text(i.raw),
            Expression::Literal(Literal::Float(f)) => float_text(f.raw),
            Expression::Literal(Literal::True(k))
            | Expression::Literal(Literal::False(k))
            | Expression::Literal(Literal::Null(k)) => self.src(k),
            Expression::CompositeString(CompositeString::Interpolated(s)) => {
                let parts = s
                    .parts
                    .iter()
                    .map(|part| match part {
                        StringPart::Literal(l) => escape_dq(l.value.unwrap_or(l.raw)),
                        StringPart::Expression(e) => format!("{{{}}}", self.expr(e)),
                        StringPart::BracedExpression(e) => {
                            format!("{{{}}}", self.expr(e.expression))
                        }
                    })
                    .collect::<String>();
                format!("\"{parts}\"")
            }
            Expression::CompositeString(CompositeString::Document(d)) => self.document(d),
            Expression::CompositeString(c) => self.src(c),
            Expression::Array(a) => format!(
                "[{}]",
                self.elements(a.left_bracket.end.offset, &a.elements)
            ),
            Expression::LegacyArray(a) => format!(
                "array({})",
                self.elements(a.left_parenthesis.end.offset, &a.elements)
            ),
            Expression::List(a) => format!(
                "list({})",
                self.elements(a.left_parenthesis.end.offset, &a.elements)
            ),
            Expression::ArrayAccess(a) => format!("{}[{}]", self.lhs(a.array), self.expr(a.index)),
            Expression::ArrayAppend(a) => format!("{}[]", self.lhs(a.array)),
            Expression::Variable(v) => self.variable(v),
            Expression::ConstantAccess(c) => text::lossy(c.name.value()),
            Expression::Identifier(i) => text::lossy(i.value()),
            Expression::Call(c) => self.call(c),
            Expression::Access(a) => self.access(a),
            Expression::Instantiation(i) => self.instantiation(i),
            Expression::Closure(c) => self.closure(c),
            Expression::Match(m) => self.match_expr(m),
            Expression::Construct(c) => self.construct(c),
            Expression::MagicConstant(m) => self.src(m).to_ascii_uppercase(),
            other => self.src(other).trim().to_string(),
        }
    }
    fn document(&mut self, d: &DocumentString<'_>) -> String {
        let label = text::lossy(d.label);
        let quote = match d.kind {
            DocumentKind::Nowdoc => "'",
            DocumentKind::Heredoc => "",
        };
        let mut body = String::new();
        for part in d.parts.iter() {
            match part {
                StringPart::Literal(l) => {
                    let v = l.value.unwrap_or(l.raw);
                    body.push_str(&if quote.is_empty() {
                        escape_heredoc(v)
                    } else {
                        text::lossy(v)
                    });
                }
                StringPart::Expression(e) => body.push_str(&format!("{{{}}}", self.expr(e))),
                StringPart::BracedExpression(e) => {
                    body.push_str(&format!("{{{}}}", self.expr(e.expression)));
                }
            }
        }
        let nl = self.nl();
        let body = body.replace('\n', &nl);
        if body.is_empty() {
            return format!("<<<{quote}{label}{quote}{nl}{label}");
        }
        format!("<<<{quote}{label}{quote}{nl}{body}{nl}{label}")
    }
    fn member(&mut self, m: &ClassLikeMemberSelector<'_>) -> String {
        match m {
            ClassLikeMemberSelector::Identifier(x) => text::lossy(x.value),
            ClassLikeMemberSelector::Variable(x) => format!("{{{}}}", self.variable(x)),
            ClassLikeMemberSelector::Expression(x) => format!("{{{}}}", self.expr(x.expression)),
            ClassLikeMemberSelector::Missing(_) => String::new(),
        }
    }
    fn call(&mut self, c: &Call<'_>) -> String {
        match c {
            Call::Function(x) => format!(
                "{}({})",
                self.call_lhs(x.function),
                self.args(&x.argument_list)
            ),
            Call::Method(x) => format!(
                "{}->{}({})",
                self.lhs(x.object),
                self.member(&x.method),
                self.args(&x.argument_list)
            ),
            Call::NullSafeMethod(x) => format!(
                "{}?->{}({})",
                self.lhs(x.object),
                self.member(&x.method),
                self.args(&x.argument_list)
            ),
            Call::StaticMethod(x) => format!(
                "{}::{}({})",
                self.lhs(x.class),
                self.static_member(&x.method),
                self.args(&x.argument_list)
            ),
        }
    }
    fn static_member(&mut self, m: &ClassLikeMemberSelector<'_>) -> String {
        match m {
            ClassLikeMemberSelector::Variable(v) => self.variable(v),
            _ => self.member(m),
        }
    }
    fn access(&mut self, a: &Access<'_>) -> String {
        match a {
            Access::Property(x) => format!("{}->{}", self.lhs(x.object), self.member(&x.property)),
            Access::NullSafeProperty(x) => {
                format!("{}?->{}", self.lhs(x.object), self.member(&x.property))
            }
            Access::StaticProperty(x) => {
                format!("{}::{}", self.lhs(x.class), self.variable(&x.property))
            }
            Access::ClassConstant(x) => format!(
                "{}::{}",
                self.lhs(x.class),
                match &x.constant {
                    ClassLikeConstantSelector::Identifier(i) => text::lossy(i.value),
                    ClassLikeConstantSelector::Expression(e) =>
                        format!("{{{}}}", self.expr(e.expression)),
                    ClassLikeConstantSelector::Missing(_) => String::new(),
                }
            ),
        }
    }
    fn instantiation(&mut self, i: &Instantiation<'_>) -> String {
        format!(
            "new {}({})",
            self.expr(i.class),
            i.argument_list
                .as_ref()
                .map(|x| self.args(x))
                .unwrap_or_default()
        )
    }
}

pub fn print_expr(parsed: &ParsedFile<'_>, e: &Expression<'_>) -> String {
    Printer::print(parsed, e)
}
pub(crate) use text::lossy;
pub(crate) use text::strip as strip_expr;
