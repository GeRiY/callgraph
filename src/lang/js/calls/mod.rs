//! A script-blokkok hívásai (`calls`): kategorizálás, kötések, `caller` a metódus-passz szerint.

mod callee;
mod scan;
mod walk;

use oxc_ast::ast::{Program, PropertyKey};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::imports::import_bindings;
use super::methods::{Docs, method_index};
use super::resolve::Resolver;
use callee::{Binding, Scope};

/// A kulcs neve (azonosító vagy string).
pub(crate) fn key_name(k: &PropertyKey<'_>) -> Option<String> {
    match k {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::Identifier(i) => Some(i.name.to_string()),
        PropertyKey::StringLiteral(s) => s.value.as_str().map(str::to_string),
        _ => None,
    }
}

/// A hívásokat gyűjtő passz (a template-hívások nem ide tartoznak).
#[derive(Default)]
pub struct CallsPass {
    resolver: Resolver,
}

impl JsPass for CallsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        let index = method_index(ctx, program);
        let mut scope = Scope {
            names: index.names.clone(),
            ..Scope::default()
        };
        for b in import_bindings(program, ctx, &self.resolver) {
            scope.bindings.insert(
                b.local,
                Binding {
                    resolved: b.resolved,
                    category: b.category,
                },
            );
        }
        scan::scan_functions(program, &mut scope);
        let docs = Docs {
            source: ctx.source,
            comments: &program.comments,
        };
        let top = if ctx.setup { "setup" } else { "__module__" };
        let mut w = walk::Walker::new(ctx, &docs, &index, scope, top);
        w.run(program);
        for call in w.calls {
            out.builder.call(call);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::SourceFile;
    use crate::lang::{Language, js::JsFrontend};

    fn calls(src: &str) -> Vec<(String, String, String, Option<String>)> {
        let a = JsFrontend::with_defaults()
            .analyze(&SourceFile::new("/p/x.ts", src.as_bytes().to_vec(), None))
            .unwrap();
        let c = a.graph.calls;
        c.into_iter()
            .map(|c| (c.caller.unwrap(), c.target, c.category, c.resolved))
            .collect()
    }

    #[test]
    fn categorizes_and_binds() {
        let c = calls(
            "import { ref } from 'vue';\nconst emit = defineEmits(['a']);\nfunction go() { emit('a'); ref(1); Math.max(1); foo(); }\n",
        );
        assert!(c.contains(&(
            "go".into(),
            "emit".into(),
            "vue_emit".into(),
            Some("vue::emit:a".into())
        )));
        assert!(c.iter().any(|x| x.0 == "go" && x.2 == "vue_api"));
        assert!(c.iter().any(|x| x.1 == "Math.max" && x.2 == "native_js"));
        assert!(c.iter().any(|x| x.1 == "foo" && x.2 == "global_helper"));
        assert!(
            c.iter()
                .any(|x| x.0 == "__module__" && x.1 == "defineEmits")
        );
    }
}
