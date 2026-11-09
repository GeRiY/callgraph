//! CSS frontend: Raffia-validálás és CSS → `FileGraph` leképezés.

mod frontend;
mod rules;
mod scanner;

pub use frontend::CssFrontend;

#[cfg(test)]
mod tests {
    use super::CssFrontend;
    use crate::core::SourceFile;
    use crate::lang::{Language, LanguageFrontend};

    #[test]
    fn analyzes_stylesheet_and_resolves_import() {
        let f = CssFrontend::new();
        let a = f
            .analyze(&SourceFile::new(
                "/tmp/css/theme.css",
                br#"@import url("print.css");
.card { color: var(--brand); background: rgba(0, 0, 0, .2); }
"#
                .to_vec(),
                None,
            ))
            .unwrap();
        assert_eq!(a.graph.containers[0].name, "theme");
        assert_eq!(a.graph.methods[0].body, None);
        assert_eq!(a.graph.imports.get("print.css"), Some("print.css"));
        assert!(a.graph.calls.iter().any(|c| c.target == "var"));
        assert_eq!(
            f.resolve_module("print.css", std::path::Path::new("/tmp/css/theme.css")),
            Some(std::path::PathBuf::from("/tmp/css/print.css"))
        );
    }

    /// A szabályok neve, fajtája és sortartománya (`.p-toast [rule] L29-31`), forrás-sorrendben.
    fn rules(source: &str) -> Vec<String> {
        let file = SourceFile::new("/tmp/css/app.css", source.as_bytes().to_vec(), None);
        let mut methods = CssFrontend::new().analyze(&file).unwrap().graph.methods;
        methods.sort_by_key(|m| m.line);
        methods
            .iter()
            .map(|m| {
                let kind = m.kind.as_deref().unwrap_or_default();
                let end = m.end_line.unwrap_or(m.line);
                format!("{} [{kind}] L{}-{end}", m.name, m.line)
            })
            .collect()
    }

    /// A felhasználói `resources/css/app.css` alakja: a hosszú megjegyzés utáni szabály sem marad ki, a szabály
    /// sora a fejlécé (nem az előző szabály záró `}`-jéé), és az at-szabályok is a listában vannak.
    #[test]
    fn laravel_tailwind_app_css_lists_every_rule_with_its_own_lines() {
        let source = "@import 'tailwindcss';
@source '../../vendor/laravel/framework/src/Illuminate/Pagination/resources/views/*.blade.php';
@source '../../storage/framework/views/*.php';
@source '../**/*.blade.php';
@source '../**/*.js';
@source '../**/*.vue';

@plugin '@tailwindcss/forms';

@theme {
    --font-sans: 'Instrument Sans', ui-sans-serif, system-ui, sans-serif;
}

@tailwind base;
@tailwind components;
@tailwind utilities;

/*
 * PrimeVue toast:
 * a toast a fejléc fölött legyen
 * 3
 * 4
 * 5
 * 6
 * 7
 * 8
 * 9
 */
.p-toast {
    z-index: 9999;
}

.p-toast .p-toast-message {
    width: 100%;
}

/* datepicker */
.p-datepicker-panel {
    min-width: 18rem;
    z-index: 1100;
}

@utility text-link {
    @apply text-blue-600 underline;
}
";
        assert_eq!(
            rules(source),
            [
                "@import 'tailwindcss' [at-rule] L1-1",
                "@source '../../vendor/laravel/framework/src/Illuminate/Pagination/resources/views/*.blade.php' [at-rule] L2-2",
                "@source '../../storage/framework/views/*.php' [at-rule] L3-3",
                "@source '../**/*.blade.php' [at-rule] L4-4",
                "@source '../**/*.js' [at-rule] L5-5",
                "@source '../**/*.vue' [at-rule] L6-6",
                "@plugin '@tailwindcss/forms' [at-rule] L8-8",
                "@theme [at-rule] L10-12",
                "@tailwind base [at-rule] L14-14",
                "@tailwind components [at-rule] L15-15",
                "@tailwind utilities [at-rule] L16-16",
                // a 11 soros megjegyzés (L18-28) utáni szabály is, a saját sorával
                ".p-toast [rule] L29-31",
                ".p-toast .p-toast-message [rule] L33-35",
                ".p-datepicker-panel [rule] L38-41",
                "@utility text-link [at-rule] L43-45",
            ]
        );
    }

    /// A Tailwind v4 és a szabványos at-szabályok (blokkos és `;`-re végződő is), a beágyazott szabályokkal; az
    /// `@apply` nem szabály.
    #[test]
    fn tailwind_v4_and_standard_at_rules_are_outlined() {
        let source = "@charset \"UTF-8\";
@config './tailwind.config.js';
@custom-variant dark (&:where(.dark, .dark *));
@custom-variant pointer-fine {
  @media (pointer: fine) { @slot; }
}
@layer theme, base, components;
@layer components {
  .btn { @apply px-4; }
}
@media (min-width: 640px) {
  .card { padding: 1rem; }
}
@supports (display: grid) { .grid { display: grid; } }
@keyframes spin {
  to { transform: rotate(360deg); }
}
@font-face {
  font-family: Inter;
}
.link {
  @variant hover { color: red; }
}
";
        assert_eq!(
            rules(source),
            [
                "@charset \"UTF-8\" [at-rule] L1-1",
                "@config './tailwind.config.js' [at-rule] L2-2",
                "@custom-variant dark (&:where(.dark, .dark *)) [at-rule] L3-3",
                "@custom-variant pointer-fine [at-rule] L4-6",
                "@media (pointer: fine) [at-rule] L5-5",
                "@layer theme, base, components [at-rule] L7-7",
                "@layer components [at-rule] L8-10",
                ".btn [rule] L9-9",
                "@media (min-width: 640px) [at-rule] L11-13",
                ".card [rule] L12-12",
                "@supports (display: grid) [at-rule] L14-14",
                ".grid [rule] L14-14",
                "@keyframes spin [at-rule] L15-17",
                "to [rule] L16-16",
                "@font-face [at-rule] L18-20",
                ".link [rule] L21-23",
                "@variant hover [at-rule] L22-22",
            ]
        );
    }
}
