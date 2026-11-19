//! The React twin of `ui`: the project's own form components (`.tsx` / `.jsx`) found by name and by the props the
//! React analysis reads (`lang::js::react`: destructured parameter, `Props` type, `propTypes`), or by
//! `.callgraph/gen.toml` `[components]`; the layout; and whether the project translates (a page that calls the
//! `t` of a hook such as `useTranslation()`). The language files are never written.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::page::{Comp, Parts};
use super::ui::{
    COMPONENT_DIRS, Config, LAYOUT_DIRS, MAX_FILES, NOT_FORM, Role, Ui, is_core, php_side_i18n,
    rank,
};
use crate::core::SourceFile;
use crate::generate::fs::Env;
use crate::generate::vue::VueProject;
use crate::lang::Language;
use crate::lang::js::JsFrontend;
use crate::lang::js::react::{COMP_KIND, FLAG_DEFAULT, FLAG_EXPORT, FLAG_OPEN, PROP_KIND};

/// One component file: its main component and the props the analysis reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Found {
    /// `true`: `export function X` / `export const X` (a named import), `false`: `export default`.
    named: bool,
    props: Vec<String>,
    /// Props declared as required (no `?`, no default).
    required: Vec<String>,
    /// The prop set may be larger than listed (`...rest`, HTML attributes, unresolved type).
    open: bool,
    text: String,
}

impl Found {
    fn has(&self, prop: &str) -> bool {
        self.props.iter().any(|p| p == prop)
    }

    /// Declared, or the set is open (an HTML-attributes component accepts it).
    fn accepts(&self, prop: &str) -> bool {
        self.has(prop) || self.open
    }

    fn error_prop(&self) -> Option<&'static str> {
        ["message", "error", "errors"]
            .into_iter()
            .find(|p| self.has(p))
    }

    fn label_prop(&self) -> Option<&'static str> {
        ["value", "label"].into_iter().find(|p| self.has(p))
    }
}

/// The main component of a file: the one named like the file, else the default export, else the only one.
fn analyze(frontend: &JsFrontend, path: &Path, stem: &str, text: &str) -> Option<Found> {
    let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
    let graph = frontend.analyze(&file).ok()?.graph;
    let comps: Vec<_> = graph
        .constants
        .iter()
        .filter(|c| c.kind == COMP_KIND)
        .collect();
    let flags = |c: &crate::model::Constant| -> Vec<String> {
        c.value
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::to_owned)
            .collect()
    };
    let comp = comps
        .iter()
        .find(|c| c.name == stem)
        .or_else(|| {
            comps
                .iter()
                .find(|c| flags(c).iter().any(|f| f == FLAG_DEFAULT))
        })
        .or_else(|| comps.first().filter(|_| comps.len() == 1))?;
    let flags = flags(comp);
    let prefix = format!("{}.", comp.name);
    let mut found = Found {
        named: !flags.iter().any(|f| f == FLAG_DEFAULT)
            && flags
                .iter()
                .any(|f| f.strip_prefix(FLAG_EXPORT) == Some(comp.name.as_str())),
        open: flags.iter().any(|f| f == FLAG_OPEN),
        text: text.to_owned(),
        ..Found::default()
    };
    for c in graph.constants.iter().filter(|c| c.kind == PROP_KIND) {
        if let Some(name) = c.name.strip_prefix(&prefix) {
            found.props.push(name.to_owned());
            if c.value.is_some() {
                found.required.push(name.to_owned());
            }
        }
    }
    Some(found)
}

/// The components of the directories by file stem (the first directory wins).
fn candidates(
    env: Env<'_>,
    project: &VueProject<'_>,
    frontend: &JsFrontend,
    dirs: &[&str],
) -> BTreeMap<String, Found> {
    let mut out = BTreeMap::new();
    for dir in dirs {
        for file in project.jsx_files(dir, MAX_FILES) {
            let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if out.contains_key(stem) {
                continue;
            }
            let Some(text) = project.text(&file) else {
                continue;
            };
            let path = env.root.join(&file);
            if let Some(found) = analyze(frontend, &path, stem, &text) {
                out.insert(stem.to_owned(), found);
            }
        }
    }
    out
}

/// The role of a component by its file name and props (not a form element: `None`).
fn classify(stem: &str, c: &Found) -> Option<Role> {
    let low = stem.to_ascii_lowercase();
    let is = |w: &str| low.contains(w);
    if low.ends_with("button") {
        let other = [
            "danger",
            "secondary",
            "dropdown",
            "nav",
            "responsive",
            "icon",
            "link",
        ];
        return (!other.iter().any(|&w| is(w))).then_some(Role::Button);
    }
    if NOT_FORM.iter().any(|&w| is(w)) {
        return None;
    }
    if is("field") && c.has("label") && c.error_prop().is_some() && c.accepts("children") {
        return Some(Role::Field);
    }
    if is("error") || is("validation") {
        return (c.error_prop().is_some() || c.open || c.has("children")).then_some(Role::Error);
    }
    if is("label") {
        return (c.label_prop().is_some() || c.accepts("children") || c.accepts("htmlFor"))
            .then_some(Role::Label);
    }
    let controlled = c.accepts("value") && c.accepts("onChange");
    if is("textarea") {
        return controlled.then_some(Role::Textarea);
    }
    if is("checkbox") {
        return (c.accepts("checked") || c.accepts("onChange")).then_some(Role::Checkbox);
    }
    let other = ["radio", "select", "switch", "file"];
    if (is("input") || is("text")) && controlled && !other.iter().any(|&w| is(w)) {
        return Some(Role::Input);
    }
    None
}

/// `(hook, source)` of the translator in an existing page: `const { t } = useTranslation()` plus its import.
fn translator_in(text: &str) -> Option<(String, String)> {
    static HOOK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"const\s*\{\s*t\s*[,}][^=]*=\s*(use[A-Za-z0-9_]+)\(").expect("valid regex")
    });
    let hook = HOOK.captures(text)?[1].to_owned();
    let import = Regex::new(&format!(
        r#"import\s*\{{[^}}]*\b{hook}\b[^}}]*\}}\s*from\s*['"]([^'"]+)['"]"#
    ))
    .ok()?;
    let source = import.captures(text)?[1].to_owned();
    Some((hook, source))
}

/// The translator of the project: the hook an existing page calls (never the package alone, "only if existing pages
/// do"); `gen.toml` `[i18n] enabled = false` turns it off, `true` falls back to the installed package's hook.
fn detect_translator(
    project: &VueProject<'_>,
    config: &Config,
    npm: &BTreeSet<String>,
) -> Option<(String, String)> {
    match config.i18n {
        Some(false) => None,
        Some(true) => project.jsx_find(translator_in).or_else(|| {
            [
                ("react-i18next", "useTranslation"),
                ("laravel-react-i18n", "useLaravelReactI18n"),
            ]
            .into_iter()
            .find(|(package, _)| npm.contains(*package))
            .map(|(package, hook)| (hook.to_owned(), package.to_owned()))
        }),
        None => project.jsx_find(translator_in),
    }
}

/// The components, the translation and the notes for a React project.
pub fn discover(
    env: Env<'_>,
    project: &VueProject<'_>,
    config: &Config,
    npm: &BTreeSet<String>,
    ziggy: bool,
) -> Ui {
    let frontend = JsFrontend::with_defaults();
    let texts = candidates(env, project, &frontend, &COMPONENT_DIRS);
    let mut bad: Vec<String> = Vec::new();
    let roles: [(&str, Role, &[&str]); 7] = [
        ("field", Role::Field, &[]),
        ("label", Role::Label, &["InputLabel"]),
        ("input", Role::Input, &["TextInput"]),
        ("textarea", Role::Textarea, &[]),
        ("checkbox", Role::Checkbox, &[]),
        ("error", Role::Error, &["InputError"]),
        ("button", Role::Button, &["PrimaryButton"]),
    ];

    // gen.toml, then the usual name, then the heuristic (only a missing role)
    let mut from_config: Vec<String> = Vec::new();
    let mut guessed: Vec<String> = Vec::new();
    let mut picks: Vec<Option<(String, String)>> = Vec::new();
    for (key, role, defaults) in &roles {
        let exists = |name: &str| {
            project
                .find_jsx_component(name)
                .map(|p| (name.to_owned(), p))
        };
        let mut pick = None;
        if let Some(name) = config.components.get(*key) {
            match exists(name) {
                Some(found) => {
                    from_config.push(format!("{key}={name}"));
                    pick = Some(found);
                }
                None => bad.push(name.clone()),
            }
        }
        if pick.is_none() {
            pick = defaults.iter().find_map(|name| exists(name));
        }
        if pick.is_none() && is_core(*role) {
            let best = texts
                .iter()
                .filter(|(stem, c)| classify(stem, c) == Some(*role))
                .min_by_key(|(stem, _)| rank(stem, *role))
                .and_then(|(stem, _)| exists(stem));
            if let Some(found) = best {
                guessed.push(format!("{key}={}", found.0));
                pick = Some(found);
            }
        }
        picks.push(pick);
    }
    let mut comps = picks
        .into_iter()
        .zip(roles.iter())
        .map(|(pick, (_, role, _))| {
            let (name, path) = pick?;
            let found = texts.get(&name);
            let prop = match role {
                Role::Field => Some(
                    found
                        .and_then(Found::error_prop)
                        .unwrap_or("error")
                        .to_owned(),
                ),
                Role::Label => found
                    .map_or(Some("value"), Found::label_prop)
                    .map(str::to_owned),
                Role::Error => Some(
                    found
                        .and_then(Found::error_prop)
                        .unwrap_or("message")
                        .to_owned(),
                ),
                _ => None,
            };
            Some((
                Comp {
                    id: *role == Role::Field && found.is_some_and(|f| f.has("id")),
                    name,
                    path,
                    prop,
                },
                found.is_some_and(|f| f.named),
            ))
        });
    let mut named: BTreeSet<String> = BTreeSet::new();
    let mut next = || {
        let (comp, is_named) = comps.next().flatten()?;
        if is_named {
            named.insert(comp.name.clone());
        }
        Some(comp)
    };
    let (field, label, input, textarea, checkbox, error, button) =
        (next(), next(), next(), next(), next(), next(), next());

    // the layout
    let layouts = candidates(env, project, &frontend, &LAYOUT_DIRS);
    let mut layout = None;
    if let Some(name) = config.components.get("layout") {
        match project.find_jsx_component(name) {
            Some(path) => {
                from_config.push(format!("layout={name}"));
                layout = Some((name.clone(), path));
            }
            None => bad.push(name.clone()),
        }
    }
    if layout.is_none() {
        layout = ["AuthenticatedLayout", "AppLayout"]
            .into_iter()
            .find_map(|n| project.find_jsx_component(n).map(|p| (n.to_owned(), p)));
    }
    if layout.is_none() {
        let best = layouts
            .keys()
            .filter(|stem| stem.ends_with("Layout") && !stem.contains("Guest"))
            .min_by_key(|stem| {
                let preferred = ["Authenticated", "App", "Main"]
                    .iter()
                    .any(|w| stem.contains(*w));
                (u8::from(!preferred), (*stem).clone())
            });
        if let Some(stem) = best
            && let Some(path) = project.find_jsx_component(stem)
        {
            guessed.push(format!("layout={stem}"));
            layout = Some((stem.clone(), path));
        }
    }
    let layout_info = layout
        .as_ref()
        .and_then(|(name, _)| layouts.get(name).or_else(|| texts.get(name)));
    let layout_header = layout_info.is_some_and(|l| l.has("header") || l.text.contains("header"));
    if let Some((name, _)) = &layout
        && let Some(info) = layout_info
        && info.named
    {
        named.insert(name.clone());
    }

    let mut notes = Vec::new();
    if !bad.is_empty() {
        notes.push(crate::tf!(
            "⚠ gen.toml: component not found: {}",
            "⚠ gen.toml: nincs ilyen komponens: {}",
            bad.join(", ")
        ));
    }
    if !from_config.is_empty() {
        notes.push(crate::tf!(
            "components (gen.toml): {}",
            "komponensek (gen.toml): {}",
            from_config.join(", ")
        ));
    }
    if !guessed.is_empty() {
        notes.push(crate::tf!(
            "components guessed: {}",
            "komponensek (tippelt): {}",
            guessed.join(", ")
        ));
    }
    if let Some(info) = layout_info {
        let needs: Vec<&str> = info
            .required
            .iter()
            .map(String::as_str)
            .filter(|p| !matches!(*p, "header" | "children"))
            .collect();
        if !needs.is_empty() {
            notes.push(crate::tf!(
                "⚠ the layout requires prop(s) the pages do not pass: {}",
                "⚠ a layout kötelező propjait az oldalak nem adják át: {}",
                needs.join(", ")
            ));
        }
    }

    let i18n_hook = detect_translator(project, config, npm);
    let php_i18n = i18n_hook.is_some() && php_side_i18n(env);
    Ui {
        parts: Parts {
            layout,
            field,
            label,
            input,
            textarea,
            checkbox,
            error,
            button,
            i18n: i18n_hook.is_some(),
            ziggy,
            react: true,
            i18n_hook,
            named,
            layout_header,
        },
        php_i18n,
        notes,
    }
}
