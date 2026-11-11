//! A publikus szerződés és a docblock-kivonat formázója.

use crate::i18n::Lang;
use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Api;

impl Formatter for Api {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let mut out = String::new();
        for row in rows {
            writeln!(
                out,
                "FILE  {}{}{}{}",
                row.graph.file,
                row.graph
                    .namespace
                    .as_ref()
                    .map(|n| format!("  NS:{n}"))
                    .unwrap_or_default(),
                crate::notes::mark(crate::notes::file_count(&row.notes)),
                crate::io::protect_mark(&row.graph.file)
            )
            .unwrap();
            out.push_str(&super::parse_error_lines(&row.graph, lang));
            out.push('\n');
            if row.graph.containers.is_empty() {
                out.push_str(lang.pick(
                    "(procedural file — no class/interface/trait; --format outline shows the top-level calls)\n\n",
                    "(procedurális fájl — nincs class/interface/trait; --format outline mutatja a top-level hívásokat)\n\n",
                ));
                let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
                for call in row
                    .graph
                    .calls
                    .iter()
                    .filter(|call| call.caller.as_deref() == Some("__top__"))
                {
                    groups
                        .entry(call.category.clone())
                        .or_default()
                        .push(call.resolved.clone().unwrap_or_else(|| call.target.clone()));
                }
                let mut groups: Vec<_> = groups.into_iter().collect();
                groups.sort_by_key(|(category, _)| crate::model::category_rank(category));
                for (category, mut targets) in groups {
                    targets.sort();
                    targets.dedup();
                    writeln!(out, "  {category:<18}{}", targets.join(", ")).unwrap();
                }
                out.push_str("---\n");
                continue;
            }
            for container in &row.graph.containers {
                write_container(&mut out, row, container, lang);
            }
        }
        out.push_str(lang.pick(
            "💡 for the method bodies' logic: Read from the L<n> line above — not the whole file. For the call graph: --format outline.\n",
            "💡 a metódusok törzs-logikájához: Read a fenti L<n> sortól — ne a teljes fájlt. Hívás-gráfhoz: --format outline.\n",
        ));
        out
    }
}

fn write_container(
    out: &mut String,
    row: &QueryResult,
    container: &crate::model::Container,
    lang: Lang,
) {
    let extends = join_non_empty(&container.extends, "extends");
    let implements = join_non_empty(&container.implements, "implements");
    let traits = join_non_empty(&container.traits, "uses");
    writeln!(
        out,
        "{} {}{}{}{}  L{}-{}",
        container.kind,
        container.name,
        extends,
        implements,
        traits,
        container.line,
        container.end_line
    )
    .unwrap();
    if let Some(doc) = &container.doc {
        writeln!(out, "  {doc}").unwrap();
    }
    out.push('\n');
    let constants: Vec<_> = row
        .graph
        .constants
        .iter()
        .filter(|c| {
            row.graph.owns(container, c.container.as_deref(), c.line)
                && is_public(c.visibility.as_deref())
        })
        .collect();
    if !constants.is_empty() {
        let n = constants.len();
        out.push_str(&ltf!(
            lang,
            "  public constants ({n}):\n",
            "  publikus konstansok ({n}):\n"
        ));
        for constant in constants {
            writeln!(
                out,
                "    {}{}",
                constant.name,
                constant
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default()
            )
            .unwrap();
        }
        out.push('\n');
    }
    let properties: Vec<_> = row
        .graph
        .properties
        .iter()
        // a JS/Vue-propnak a régi motorban nincs visibility-je, így a jq `== "public"` kihagyja
        .filter(|p| {
            !p.js
                && row.graph.owns(container, p.container.as_deref(), p.line)
                && is_public(Some(p.visibility.as_str()))
        })
        .collect();
    if !properties.is_empty() {
        let n = properties.len();
        out.push_str(&ltf!(
            lang,
            "  public properties ({n}):\n",
            "  publikus property-k ({n}):\n"
        ));
        for property in properties {
            writeln!(
                out,
                "    {} ${}{}",
                property.type_hint.as_deref().unwrap_or("mixed"),
                property.name,
                if property.readonly { " [readonly]" } else { "" }
            )
            .unwrap();
        }
        out.push('\n');
    }
    write_vue_contract(out, row, container, lang);
    let mut methods: Vec<_> = row
        .graph
        .methods
        .iter()
        .filter(|m| {
            row.graph.owns(container, m.container.as_deref(), m.line)
                && is_public(m.visibility.as_deref())
        })
        .collect();
    if methods.is_empty() {
        out.push_str(lang.pick("  (no public methods)\n", "  (nincs publikus metódus)\n"));
    } else {
        let n = methods.len();
        out.push_str(&ltf!(
            lang,
            "  public API ({n}):\n",
            "  publikus felület ({n}):\n"
        ));
        methods.sort_by_key(|m| m.line);
        for method in methods {
            write!(out, "    {}({})", method.name, method.params.join(", ")).unwrap();
            if let Some(return_type) = &method.return_type {
                write!(out, ": {return_type}").unwrap();
            }
            write!(out, "  L{}", method.line).unwrap();
            if method.is_static {
                out.push_str(" [static]");
            }
            let notes =
                crate::notes::method_count(&row.notes, method.container.as_deref(), &method.name);
            out.push_str(&crate::notes::mark(notes));
            out.push_str(&super::signature::doc_types_suffix(method));
            out.push('\n');
            if let Some(doc) = &method.doc {
                writeln!(out, "      {doc}").unwrap();
            }
        }
    }
    out.push_str("---\n");
}

/// A Vue-komponens szerződése: a `props:` (név, ismert típus, kötelező) és az `emits:` lista; a JS-prop / emit
/// csak a JS-frontendből jön, így más nyelv kimenete nem változik.
fn write_vue_contract(
    out: &mut String,
    row: &QueryResult,
    container: &crate::model::Container,
    lang: Lang,
) {
    let props: Vec<_> = row
        .graph
        .properties
        .iter()
        .filter(|p| p.js && row.graph.owns(container, p.container.as_deref(), p.line))
        .collect();
    if !props.is_empty() {
        let n = props.len();
        writeln!(out, "  props ({n}):").unwrap();
        for p in props {
            let ty = p.type_hint.as_deref().filter(|t| !t.is_empty() && *t != "mixed");
            writeln!(
                out,
                "    {}{}{}",
                p.name,
                ty.map(|t| format!(": {t}")).unwrap_or_default(),
                if p.readonly { " [required]" } else { "" }
            )
            .unwrap();
        }
        out.push('\n');
    }
    let emits: Vec<_> = row
        .graph
        .constants
        .iter()
        .filter(|c| c.js && c.kind == "emit")
        .filter(|c| row.graph.owns(container, c.container.as_deref(), c.line))
        .collect();
    if !emits.is_empty() {
        let n = emits.len();
        writeln!(out, "  emits ({n}):").unwrap();
        for c in emits {
            writeln!(out, "    {}", c.name.trim_start_matches("emit:")).unwrap();
        }
        out.push('\n');
    }
}

/// Publikus elem: `public` (PHP / JS / Swift) vagy Rust `pub` (a `pub(crate)` / `pub(super)` nem).
/// A Rust trait-implementációk metódusai is `pub`-ok (a trait-en át hívhatók), így ezek is listázódnak.
fn is_public(visibility: Option<&str>) -> bool {
    matches!(visibility, Some("public" | "pub"))
}

fn join_non_empty(values: &Option<Vec<String>>, label: &str) -> String {
    values
        .as_ref()
        .filter(|v| !v.is_empty())
        .map(|v| format!(" {label} {}", v.join(", ")))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::FormatterRegistry;
    use crate::core::SourceFile;
    use crate::i18n::Lang;
    use crate::lang::LanguageRegistry;
    use crate::query::QueryResult;

    fn api(path: &str, text: &str) -> String {
        let language = LanguageRegistry::with_defaults();
        let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
        let analyzer = language.for_path(std::path::Path::new(path)).unwrap();
        let rows = [QueryResult::new(
            analyzer.analyze(&file).unwrap().graph,
            None,
        )];
        FormatterRegistry::with_defaults()
            .in_lang(Lang::En)
            .render_with("api", &rows, None, None)
            .unwrap()
    }

    #[test]
    fn vue_api_lists_props_and_emits() {
        let sfc = "<script setup lang=\"ts\">\nconst props = defineProps<{ title: string; size?: number }>();\nconst emit = defineEmits<{ (e: 'save', v: number): void; (e: 'close'): void }>();\n</script>\n<template><div>{{ props.title }}</div></template>\n";
        let out = api("src/Comp.vue", sfc);
        assert!(out.contains("  props (2):\n"), "{out}");
        assert!(out.contains("    title: string [required]\n"), "{out}");
        assert!(out.contains("    size: number\n"), "{out}");
        assert!(out.contains("  emits (2):\n    close\n    save\n"), "{out}");
    }

    #[test]
    fn vue_options_api_lists_array_props_and_emits() {
        let sfc = "<script>\nexport default { props: ['a', 'b'], emits: ['go'] };\n</script>\n";
        let out = api("src/Opt.vue", sfc);
        assert!(out.contains("  props (2):\n    a\n    b\n"), "{out}");
        assert!(out.contains("  emits (1):\n    go\n"), "{out}");
    }

    #[test]
    fn components_without_props_and_other_languages_have_no_such_lists() {
        let out = api("src/Plain.vue", "<template><p>hi</p></template>\n");
        assert!(!out.contains("props (") && !out.contains("emits ("), "{out}");
        let out = api(
            "src/a.ts",
            "export class A { public go(x: number): void {} }\n",
        );
        assert!(!out.contains("props (") && !out.contains("emits ("), "{out}");
        assert!(out.contains("go(") && out.contains("public API"), "{out}");
    }
}
