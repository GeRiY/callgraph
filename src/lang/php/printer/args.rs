use mago_span::HasSpan;
use mago_syntax::cst::*;

use super::{Printer, text};

impl Printer<'_, '_> {
    pub(super) fn variable(&mut self, v: &Variable<'_>) -> String {
        match v {
            Variable::Direct(x) => text::lossy(x.name),
            Variable::Indirect(x) => format!("${{{}}}", self.expr(x.expression)),
            Variable::Nested(x) => format!("${}", self.variable(x.variable)),
        }
    }

    pub(crate) fn args(&mut self, l: &ArgumentList<'_>) -> String {
        let items: Vec<_> = l.arguments.iter().collect();
        let mut previous = l.left_parenthesis.end.offset;
        let comments: Vec<_> = items
            .iter()
            .map(|item| {
                let c = self.comments_between(previous, item.span().start.offset);
                previous = item.span().end.offset;
                c
            })
            .collect();
        if comments.iter().all(Vec::is_empty) {
            return items
                .iter()
                .map(|a| self.argument(a))
                .collect::<Vec<_>>()
                .join(", ");
        }
        self.indent += 1;
        let mut out = String::new();
        for (index, item) in items.iter().enumerate() {
            if !comments[index].is_empty() {
                out.push_str(&self.nl());
                out.push_str(&comments[index].join("\n").replace('\n', &self.nl()));
            }
            out.push_str(&self.nl());
            out.push_str(&self.argument(item));
            if index + 1 != items.len() {
                out.push(',');
            }
        }
        self.indent -= 1;
        out.push_str(&self.nl());
        out
    }

    fn argument(&mut self, a: &Argument<'_>) -> String {
        match a {
            Argument::Positional(x) => format!(
                "{}{}",
                if x.ellipsis.is_some() { "..." } else { "" },
                self.expr(x.value)
            ),
            Argument::Named(x) => format!("{}: {}", text::lossy(x.name.value), self.expr(x.value)),
        }
    }
}
