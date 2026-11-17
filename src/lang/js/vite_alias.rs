//! A `vite.config.*` / `vitest.config.*` `resolve.alias` olvasása (a tsconfig `paths` mellett ez a második
//! alias-forrás): `{ '@kit': fileURLToPath(new URL('./src/kit', import.meta.url)) }`, `path.resolve(__dirname, 'src')`,
//! `[{ find: '@', replacement: '/src' }]`. Csak a statikusan kiolvasható alakok; reguláris kifejezés és számított
//! érték kimarad.

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, ObjectExpression, ObjectProperty, ObjectPropertyKind};
use oxc_ast_visit::{Visit, walk};

use super::calls::key_name;
use super::parse::{parse_program, source_type_for};
use super::resolve::normalize;

/// A config-fájlok nevei a csomag-gyökérben.
const CONFIG_NAMES: [&str; 8] = [
    "vite.config.ts",
    "vite.config.mts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.js",
];

/// A csomag-gyökér Vite aliasai: (név, abszolút cél), deklarációs sorrendben.
pub(super) fn vite_aliases(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for name in CONFIG_NAMES {
        let file = dir.join(name);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let allocator = Allocator::default();
        let parsed = parse_program(&allocator, &text, source_type_for(&file, None));
        let mut finder = Finder {
            dir,
            found: Vec::new(),
        };
        finder.visit_program(&parsed.program);
        out.extend(finder.found);
    }
    out
}

struct Finder<'d> {
    dir: &'d Path,
    found: Vec<(String, PathBuf)>,
}

impl Finder<'_> {
    fn add(&mut self, find: String, replacement: &Expression<'_>) {
        if let Some(path) = eval_path(replacement) {
            // a `/src` a Vite-projekt gyökeréhez viszonyul, a `./src` és a `src` a confighoz
            let rel = path.trim_start_matches('/');
            self.found.push((find, normalize(&self.dir.join(rel))));
        }
    }

    fn entries(&mut self, object: &ObjectExpression<'_>) {
        for prop in &object.properties {
            if let ObjectPropertyKind::ObjectProperty(p) = prop
                && let Some(find) = key_name(&p.key)
            {
                self.add(find, &p.value);
            }
        }
    }

    fn list_entry(&mut self, object: &ObjectExpression<'_>) {
        let mut find = None;
        let mut replacement = None;
        for prop in &object.properties {
            if let ObjectPropertyKind::ObjectProperty(p) = prop {
                match key_name(&p.key).as_deref() {
                    Some("find") => find = string_value(&p.value),
                    Some("replacement") => replacement = Some(&p.value),
                    _ => {}
                }
            }
        }
        if let (Some(find), Some(replacement)) = (find, replacement) {
            self.add(find, replacement);
        }
    }
}

impl<'a> Visit<'a> for Finder<'_> {
    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if key_name(&it.key).as_deref() == Some("alias") {
            match it.value.get_inner_expression() {
                Expression::ObjectExpression(object) => self.entries(object),
                Expression::ArrayExpression(list) => {
                    for el in &list.elements {
                        if let Some(Expression::ObjectExpression(object)) =
                            el.as_expression().map(Expression::get_inner_expression)
                        {
                            self.list_entry(object);
                        }
                    }
                }
                _ => {}
            }
        }
        walk::walk_object_property(self, it);
    }
}

fn string_value(e: &Expression<'_>) -> Option<String> {
    match e.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.display().to_string()),
        _ => None,
    }
}

/// A csomag-gyökérhez viszonyított útvonal-kifejezés: string, `fileURLToPath(new URL('./x', import.meta.url))`,
/// `path.resolve(__dirname, 'x')` / `join(...)` (a nem string argumentumok - `__dirname`, `process.cwd()` - kimaradnak).
fn eval_path(e: &Expression<'_>) -> Option<String> {
    match e.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.display().to_string()),
        Expression::CallExpression(call) => {
            let name = match call.callee.get_inner_expression() {
                Expression::Identifier(i) => i.name.as_str(),
                Expression::StaticMemberExpression(m) => m.property.name.as_str(),
                _ => return None,
            };
            match name {
                "fileURLToPath" => {
                    let arg = call.arguments.first()?.as_expression()?;
                    let Expression::NewExpression(new) = arg.get_inner_expression() else {
                        return None;
                    };
                    string_value(new.arguments.first()?.as_expression()?)
                }
                "resolve" | "join" => {
                    let parts: Vec<String> = call
                        .arguments
                        .iter()
                        .filter_map(|a| a.as_expression().and_then(string_value))
                        .collect();
                    (!parts.is_empty()).then(|| parts.join("/"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(source: &str) -> Vec<(String, String)> {
        let dir = std::env::temp_dir().join(format!("rpg_vite_alias_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vite.config.ts"), source).unwrap();
        let found = vite_aliases(&dir)
            .into_iter()
            .map(|(find, target)| {
                let rel = target.strip_prefix(&dir).unwrap().to_string_lossy();
                (find, rel.replace('\\', "/"))
            })
            .collect();
        let _ = std::fs::remove_dir_all(dir);
        found
    }

    #[test]
    fn reads_object_array_and_call_forms() {
        let got = aliases(
            "export default defineConfig({ resolve: { alias: { '@': '/src', '@kit': fileURLToPath(new URL('./src/kit', import.meta.url)), lib: path.resolve(__dirname, './src/lib'), dyn: compute() } } });",
        );
        assert_eq!(
            got,
            vec![
                ("@".to_string(), "src".to_string()),
                ("@kit".to_string(), "src/kit".to_string()),
                ("lib".to_string(), "src/lib".to_string()),
            ]
        );
        let list = aliases(
            "export default { resolve: { alias: [{ find: '~', replacement: './app' }, { find: /^x/, replacement: './y' }] } };",
        );
        assert_eq!(list, vec![("~".to_string(), "app".to_string())]);
    }
}
