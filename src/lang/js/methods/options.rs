//! Options API (`export default {…}` / `export default call({…})`): `setup`, `methods`, `computed`, `watch`.

use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind};

use super::collector::{Collector, FnRef, Opts};
use super::top::key_name;

/// A tulajdonság értéke függvény-e (a `foo() {}` rövid alak is értékként él).
fn prop_fn<'a, 'b>(p: &'b oxc_ast::ast::ObjectProperty<'a>) -> Option<FnRef<'a, 'b>> {
    FnRef::of_expr(&p.value)
}

impl Collector<'_> {
    pub fn options_api(&mut self, obj: &ObjectExpression<'_>) {
        for p in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else {
                continue;
            };
            let Some(k) = key_name(&p.key) else { continue };
            if k == "props" || k == "emits" {
                continue;
            }
            if let Some(f) = prop_fn(p) {
                let kind = if k == "setup" { "setup" } else { "option" };
                let name = self.add(&k, f, p.span, Opts::new("public", kind));
                if k == "setup" {
                    self.nested(&name, f);
                }
            } else if matches!(k.as_str(), "methods" | "computed" | "watch")
                && let Expression::ObjectExpression(inner) = &p.value
            {
                self.option_group(&k, inner);
            }
        }
    }

    fn option_group(&mut self, group: &str, inner: &ObjectExpression<'_>) {
        for q in &inner.properties {
            let ObjectPropertyKind::ObjectProperty(q) = q else {
                continue;
            };
            let Some(qk) = key_name(&q.key) else { continue };
            let f = prop_fn(q).or_else(|| match &q.value {
                Expression::ObjectExpression(o) => o.properties.iter().find_map(|r| match r {
                    ObjectPropertyKind::ObjectProperty(r) => prop_fn(r),
                    _ => None,
                }),
                _ => None,
            });
            if let Some(f) = f {
                let name = if group == "methods" {
                    qk
                } else {
                    format!("{group}.{qk}")
                };
                self.add(&name, f, q.span, Opts::new("public", group));
            }
        }
    }
}
