use mago_span::HasSpan;
use mago_syntax::cst::*;

use super::Printer;

impl Printer<'_, '_> {
    pub(super) fn elements(
        &mut self,
        open_end: u32,
        e: &TokenSeparatedSequence<'_, ArrayElement<'_>>,
    ) -> String {
        let items: Vec<_> = e.iter().collect();
        let mut previous = open_end;
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
                .map(|x| match x {
                    ArrayElement::KeyValue(x) => {
                        format!("{} => {}", self.expr(x.key), self.expr(x.value))
                    }
                    ArrayElement::Value(x) => self.expr(x.value),
                    ArrayElement::Variadic(x) => format!("...{}", self.expr(x.value)),
                    ArrayElement::Missing(_) => String::new(),
                })
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
            out.push_str(&match item {
                ArrayElement::KeyValue(x) => {
                    format!("{} => {}", self.expr(x.key), self.expr(x.value))
                }
                ArrayElement::Value(x) => self.expr(x.value),
                ArrayElement::Variadic(x) => format!("...{}", self.expr(x.value)),
                ArrayElement::Missing(_) => String::new(),
            });
            if index + 1 != items.len() {
                out.push(',');
            }
        }
        self.indent -= 1;
        out.push_str(&self.nl());
        out
    }
}
