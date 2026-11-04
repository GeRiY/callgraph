use mago_syntax::cst::*;
pub(crate) fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}
pub(crate) fn strip<'a, 'b>(mut e: &'a Expression<'b>) -> &'a Expression<'b> {
    while let Expression::Parenthesized(x) = e {
        e = x.expression;
    }
    e
}
/// nikic `precedenceMap`: `(operátor, bal oldal, jobb oldal)`; `None`, ha nincs precedenciája.
pub(super) fn prec(e: &Expression<'_>) -> Option<(i32, i32, i32)> {
    Some(match strip(e) {
        Expression::Binary(b) => match b.operator {
            BinaryOperator::Exponentiation(_) => (0, 0, 1),
            BinaryOperator::Instanceof(_) => (20, -1, -1),
            BinaryOperator::Multiplication(_)
            | BinaryOperator::Division(_)
            | BinaryOperator::Modulo(_) => (40, 41, 40),
            BinaryOperator::Addition(_)
            | BinaryOperator::Subtraction(_)
            | BinaryOperator::StringConcat(_) => (50, 51, 50),
            BinaryOperator::LeftShift(_) | BinaryOperator::RightShift(_) => (60, 61, 60),
            BinaryOperator::LessThan(_)
            | BinaryOperator::LessThanOrEqual(_)
            | BinaryOperator::GreaterThan(_)
            | BinaryOperator::GreaterThanOrEqual(_) => (70, 70, 70),
            BinaryOperator::Equal(_)
            | BinaryOperator::NotEqual(_)
            | BinaryOperator::Identical(_)
            | BinaryOperator::NotIdentical(_)
            | BinaryOperator::AngledNotEqual(_)
            | BinaryOperator::Spaceship(_) => (80, 80, 80),
            BinaryOperator::BitwiseAnd(_) => (90, 91, 90),
            BinaryOperator::BitwiseXor(_) => (100, 101, 100),
            BinaryOperator::BitwiseOr(_) => (110, 111, 110),
            BinaryOperator::And(_) => (120, 121, 120),
            BinaryOperator::Or(_) => (130, 131, 130),
            BinaryOperator::NullCoalesce(_) => (140, 140, 141),
            BinaryOperator::LowAnd(_) => (190, 191, 190),
            BinaryOperator::LowXor(_) => (200, 201, 200),
            BinaryOperator::LowOr(_) => (210, 211, 210),
        },
        Expression::UnaryPrefix(u) => match u.operator {
            UnaryPrefixOperator::Reference(_)
            | UnaryPrefixOperator::PreIncrement(_)
            | UnaryPrefixOperator::PreDecrement(_) => return None,
            _ if u.operator.is_not() => (30, -1, -1),
            _ => (10, -1, -1),
        },
        Expression::Conditional(_) => (150, 150, 150),
        Expression::Assignment(_) => (160, -1, -1),
        Expression::ArrowFunction(_) => (230, -1, -1),
        Expression::Throw(_) => (240, -1, -1),
        Expression::Clone(_) => (-10, 0, 1),
        _ => return None,
    })
}
pub(super) fn binary_op(o: &BinaryOperator<'_>) -> &'static str {
    match o {
        BinaryOperator::Addition(_) => "+",
        BinaryOperator::Subtraction(_) => "-",
        BinaryOperator::Multiplication(_) => "*",
        BinaryOperator::Division(_) => "/",
        BinaryOperator::Modulo(_) => "%",
        BinaryOperator::Exponentiation(_) => "**",
        BinaryOperator::BitwiseAnd(_) => "&",
        BinaryOperator::BitwiseOr(_) => "|",
        BinaryOperator::BitwiseXor(_) => "^",
        BinaryOperator::LeftShift(_) => "<<",
        BinaryOperator::RightShift(_) => ">>",
        BinaryOperator::NullCoalesce(_) => "??",
        BinaryOperator::Equal(_) => "==",
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => "!=",
        BinaryOperator::Identical(_) => "===",
        BinaryOperator::NotIdentical(_) => "!==",
        BinaryOperator::LessThan(_) => "<",
        BinaryOperator::LessThanOrEqual(_) => "<=",
        BinaryOperator::GreaterThan(_) => ">",
        BinaryOperator::GreaterThanOrEqual(_) => ">=",
        BinaryOperator::Spaceship(_) => "<=>",
        BinaryOperator::StringConcat(_) => ".",
        BinaryOperator::Instanceof(_) => "instanceof",
        BinaryOperator::And(_) => "&&",
        BinaryOperator::Or(_) => "||",
        BinaryOperator::LowAnd(_) => "and",
        BinaryOperator::LowOr(_) => "or",
        BinaryOperator::LowXor(_) => "xor",
    }
}
pub(super) fn escape_dq(s: &[u8]) -> String {
    let mut out = String::new();
    for c in lossy(s).chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            '$' | '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 && c != '\x1b' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
pub(super) fn escape_heredoc(s: &[u8]) -> String {
    let text = lossy(s);
    let mut out = String::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\t' => out.push_str("\\t"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\r' if it.peek() != Some(&'\n') => out.push_str("\\r"),
            '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 && c != '\x1b' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
pub(super) fn escape_sq(s: &[u8]) -> String {
    let chars: Vec<char> = lossy(s).chars().collect();
    let mut out = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '\'' => out.push_str("\\'"),
            '\\' => {
                if matches!(chars.get(i + 1), None | Some('\'' | '\\')) {
                    out.push('\\');
                }
                out.push('\\');
            }
            c => out.push(c),
        }
    }
    out
}
pub(super) fn float_text(raw: &[u8]) -> String {
    let t = lossy(raw).replace('_', "");
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && !t.contains(['e', 'E']) => {
            let s = format!("{v}");
            if s.contains('.') { s } else { format!("{s}.0") }
        }
        _ => t,
    }
}
pub(super) fn int_text(raw: &[u8]) -> String {
    let t = lossy(raw).replace('_', "");
    let lower = t.to_ascii_lowercase();
    let parsed = if let Some(h) = lower.strip_prefix("0x") {
        u64::from_str_radix(h, 16).map(|v| format!("0x{v:x}"))
    } else if let Some(b) = lower.strip_prefix("0b") {
        u64::from_str_radix(b, 2).map(|v| format!("0b{v:b}"))
    } else if let Some(o) = lower.strip_prefix("0o") {
        u64::from_str_radix(o, 8).map(|v| format!("0{v:o}"))
    } else {
        return t;
    };
    parsed.unwrap_or(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quote_escapes_only_when_needed() {
        assert_eq!(escape_sq(br"/\d+/"), r"/\d+/");
        assert_eq!(escape_sq(br"a\\b"), r"a\\\b");
        assert_eq!(escape_sq(b"it's"), r"it\'s");
        assert_eq!(escape_sq(br"end\"), r"end\\");
    }

    #[test]
    fn numbers_are_normalized() {
        assert_eq!(int_text(b"1_000"), "1000");
        assert_eq!(int_text(b"0XFF"), "0xff");
        assert_eq!(float_text(b"1.50"), "1.5");
        assert_eq!(float_text(b"2.0"), "2.0");
    }
}
