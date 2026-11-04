//! Precedencia-függő operátor-nyomtatás (nikic `pInfixOp` / `pPrefixOp` / `pPostfixOp`).

use mago_syntax::cst::*;

use super::Printer;
use super::text::{self, binary_op, prec, strip};

pub(super) const MAX: i32 = 1000;

impl Printer<'_, '_> {
    /// Precedencia-érzékeny kifejezés; `None`, ha a kifejezés nem operátor.
    pub(super) fn operator(&mut self, e: &Expression<'_>, p: i32, lhs: i32) -> Option<String> {
        let e = strip(e);
        let (op, l, r) = prec(e).or(match e {
            Expression::Binary(_) | Expression::UnaryPrefix(_) => Some((MAX, MAX, MAX)),
            _ => None,
        })?;
        Some(match e {
            Expression::Binary(b) if matches!(b.operator, BinaryOperator::Instanceof(_)) => {
                let tail = format!(" instanceof {}", self.expr(b.rhs));
                self.postfix(op, b.lhs, &tail, p, lhs)
            }
            Expression::Binary(b) => {
                let sep = format!(" {} ", binary_op(&b.operator));
                self.infix((op, l, r), (b.lhs, &sep, b.rhs), p, lhs)
            }
            Expression::Conditional(c) => {
                let sep = match c.then {
                    Some(t) => format!(" ? {} : ", self.expr(t)),
                    None => " ?: ".to_string(),
                };
                self.infix((op, l, r), (c.condition, &sep, c.r#else), p, lhs)
            }
            Expression::UnaryPrefix(u) => {
                let o = text::lossy(u.operator.as_bytes());
                if u.operator.is_reference() {
                    return Some(format!("&{}", self.expr(u.operand)));
                }
                if prec(e).is_none() {
                    return Some(format!("{o}{}", self.expr(u.operand)));
                }
                let o = if u.operator.is_cast() {
                    format!("{o} ")
                } else {
                    o
                };
                self.prefix(op, &o, u.operand, p, lhs)
            }
            Expression::Assignment(a) => {
                let head = format!("{} {} ", self.expr(a.lhs), a.operator.as_str());
                self.prefix(op, &head, a.rhs, p, lhs)
            }
            Expression::Throw(t) => self.prefix(op, "throw ", t.exception, p, lhs),
            Expression::Clone(c) => self.prefix(op, "clone ", c.object, p, lhs),
            Expression::ArrowFunction(f) => {
                let head = self.arrow_head(f);
                self.prefix(op, &head, f.expression, p, lhs)
            }
            _ => return None,
        })
    }

    fn infix(
        &mut self,
        (op, l, r): (i32, i32, i32),
        (left, sep, right): (&Expression<'_>, &str, &Expression<'_>),
        p: i32,
        mut lhs: i32,
    ) -> String {
        let wrap = op >= p;
        if wrap {
            lhs = MAX;
        }
        let s = format!("{}{}{}", self.p(left, l, l), sep, self.p(right, r, lhs));
        if wrap { format!("({s})") } else { s }
    }

    pub(super) fn prefix(
        &mut self,
        op: i32,
        head: &str,
        operand: &Expression<'_>,
        _p: i32,
        mut lhs: i32,
    ) -> String {
        let wrap = op >= lhs;
        if wrap {
            lhs = MAX;
        }
        let mut arg = self.p(operand, op, lhs);
        if (head == "+" && arg.starts_with('+')) || (head == "-" && arg.starts_with('-')) {
            arg = format!("({arg})");
        }
        let s = format!("{head}{arg}");
        if wrap { format!("({s})") } else { s }
    }

    fn postfix(
        &mut self,
        op: i32,
        operand: &Expression<'_>,
        tail: &str,
        p: i32,
        mut lhs: i32,
    ) -> String {
        let wrap = op >= p;
        if wrap {
            lhs = MAX;
        }
        let lhs = lhs.min(op);
        let s = format!("{}{tail}", self.p(operand, op, lhs));
        if wrap { format!("({s})") } else { s }
    }
}
