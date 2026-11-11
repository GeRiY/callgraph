//! A script-blokkok hívásai (`calls`): kategorizálás, kötések, `caller` a metódus-passz szerint.

mod callee;
mod receivers;
mod scan;
#[cfg(test)]
mod tests_dynamic;
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
        let imports = import_bindings(program, ctx, &self.resolver);
        scope.types = receivers::TypeNames::new(program, &imports);
        for b in imports {
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
        calls_in("/p/x.ts", src)
    }

    fn calls_in(path: &str, src: &str) -> Vec<(String, String, String, Option<String>)> {
        let a = JsFrontend::with_defaults()
            .analyze(&SourceFile::new(path, src.as_bytes().to_vec(), None))
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

    fn has(c: &[(String, String, String, Option<String>)], want: (&str, &str, &str, Option<&str>)) {
        assert!(
            c.iter()
                .any(|x| (x.0.as_str(), x.1.as_str(), x.2.as_str(), x.3.as_deref()) == want),
            "{want:?} in {c:?}"
        );
    }

    #[test]
    fn typed_receivers_resolve_to_type_method() {
        let c = calls(concat!(
            "import { Repo } from './repo';\n",
            "import * as m from './models';\n",
            "interface Store { get(): void }\n",
            "class Svc implements Store {\n",
            "  private repo: Repo;\n",
            "  cache = new Map();\n",
            "  constructor(private other: Repo, plain: Repo) { this.repo = plain; }\n",
            "  get() { this.repo.find(); this.other!.save(); this.cache.get(1); this.helper(); }\n",
            "  helper() { const s: Store = this; s.get(); }\n",
            "}\n",
            "function a(r: Repo, t: any, u: Repo | null) { r.find(); t.load(); u.drop(); }\n",
            "function b() { const x = new m.User(); x.load(); let y = new Repo(); y = make(); y.keep(); }\n",
            "function c(r: Repo) { [1].map((r) => r.spin()); }\n",
            "function d<T>(g: T) { g.find(); }\n",
        ));
        has(
            &c,
            ("get", "this.repo.find", "module_use", Some("Repo::find")),
        );
        // `this.other!`: a non-null jelölés mögött is (a cél a régi szabály szerint a puszta név)
        has(&c, ("get", "save", "module_use", Some("Repo::save")));
        has(&c, ("get", "this.cache.get", "external_call", None));
        has(
            &c,
            ("get", "helper", "internal_helper", Some("Svc::helper")),
        );
        has(&c, ("helper", "get", "internal_helper", Some("Store::get")));
        has(&c, ("a", "find", "module_use", Some("Repo::find")));
        // `any` és unió: feloldatlan
        has(&c, ("a", "load", "external_call", None));
        has(&c, ("a", "drop", "external_call", None));
        // névtér-import; a `y` kötései nem egyeznek
        has(&c, ("b", "load", "module_use", Some("User::load")));
        has(&c, ("b", "keep", "external_call", None));
        // a típus nélküli nyíl-paraméter árnyékolja a külső `r: Repo`-t; a generikus `T` feloldatlan
        has(&c, ("c", "spin", "external_call", None));
        has(&c, ("d", "find", "external_call", None));
    }

    #[test]
    fn this_outside_methods_and_plain_js_stay_unresolved() {
        let c = calls(concat!(
            "class A {\n",
            "  repo = new A();\n",
            "  run() { const f = function () { this.repo.run(); }; f(); this.repo.run(); }\n",
            "}\n",
            "export default { methods: { go() { this.repo.run(); } } }\n",
        ));
        has(
            &c,
            ("run", "this.repo.run", "internal_helper", Some("A::run")),
        );
        assert_eq!(
            c.iter()
                .filter(|x| x.1 == "this.repo.run" && x.3.is_none())
                .count(),
            2,
            "{c:?}"
        );
    }

    #[test]
    fn jsx_components_are_component_use_calls() {
        let src = concat!(
            "import { Button } from 'ui-kit';\n",
            "import * as Ui from 'ui-kit';\n",
            "function Local() { return <i />; }\n",
            "const Arrow = () => <b />;\n",
            "export function Page() {\n",
            "  return <section><Button label=\"a\" /><Ui.Card.Head>x</Ui.Card.Head><Local /><Arrow /><Global /><></></section>;\n",
            "}\n",
        );
        for path in ["/p/Page.tsx", "/p/Page.jsx", "/p/Page.js"] {
            let c = calls_in(path, src);
            has(
                &c,
                ("Page", "Button", "component_use", Some("ui-kit::Button")),
            );
            has(
                &c,
                (
                    "Page",
                    "Ui.Card.Head",
                    "component_use",
                    Some("ui-kit::Ui::*.Card.Head"),
                ),
            );
            has(&c, ("Page", "Local", "internal_helper", None));
            has(&c, ("Page", "Arrow", "internal_helper", None));
            has(&c, ("Page", "Global", "component_use", None));
            // `<section>`, `<i>`, `<b>`, a fragment és a záró elem nem hívás
            let n = c.iter().filter(|x| x.0 == "Page").count();
            assert_eq!(n, 5, "{path}: {c:?}");
            assert!(!c.iter().any(|x| x.1 == "section" || x.1 == "i"), "{c:?}");
        }
    }

    #[test]
    fn jsx_lowercase_namespaced_and_this_names_are_not_calls() {
        let c = calls_in(
            "/p/a.tsx",
            "function f() { return <div><svg:rect /><this.Part /><span>a</span></div>; }\n",
        );
        assert!(c.is_empty(), "{c:?}");
    }

    #[test]
    fn jsx_props_still_record_inner_calls() {
        let c = calls_in(
            "/p/a.jsx",
            "import { Row } from 'ui-kit';\nfunction f() { return <Row onClick={() => go()}>{make()}</Row>; }\n",
        );
        has(&c, ("f", "Row", "component_use", Some("ui-kit::Row")));
        assert!(c.iter().any(|x| x.1 == "go"), "{c:?}");
        assert!(c.iter().any(|x| x.1 == "make"), "{c:?}");
    }
}
