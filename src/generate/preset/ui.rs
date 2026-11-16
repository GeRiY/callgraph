//! A preset projekt-ismerete az oldalakhoz: a projekt saját űrlap-komponensei (név + props alapján besorolva, vagy a
//! `.callgraph/gen.toml` szerint), és hogy a projekt használ-e fordítást (`$t()`): akkor az oldalak kulcsokat írnak
//! felirat helyett, a hiányzó kulcsokat a záró jegyzet sorolja fel — nyelvi fájlt a generátor soha nem ír.
//! Minden olvasás a generátor fájlrendszerén (`Env`) megy, a Vue-fájlok a `VueProject` memóján át (egy fájl egyszer).
//!
//! # `.callgraph/gen.toml`
//! ```toml
//! [components]          # a heurisztika helyett: a komponens neve (Név.vue a projektben)
//! input = "TextInput"   # label, input, error, button, layout
//! [i18n]
//! enabled = false       # a fordítás kikapcsolása (true: bekapcsolás, ha a heurisztika nem találta meg)
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::page::{Comp, Parts};
use crate::generate::fs::{Env, walk_php};
use crate::generate::vue::{self, VueProject};

/// A projekt generátor-beállításai.
const CONFIG: &str = ".callgraph/gen.toml";

/// A komponens-mappák, ahol a heurisztika keres.
const COMPONENT_DIRS: [&str; 4] = [
    "resources/js/Components",
    "resources/js/components",
    "src/components",
    "src/Components",
];

/// A layout-mappák.
const LAYOUT_DIRS: [&str; 4] = [
    "resources/js/Layouts",
    "resources/js/layouts",
    "src/layouts",
    "src/Layouts",
];

/// A nyelvi JSON-fájlok lehetséges mappái.
const LANG_JSON_DIRS: [&str; 6] = [
    "lang",
    "resources/lang",
    "resources/js/lang",
    "resources/js/locales",
    "src/locales",
    "src/lang",
];

/// A Laravel PHP-s nyelvi mappái (`lang/<nyelv>/<csoport>.php`).
const LANG_PHP_DIRS: [&str; 2] = ["lang", "resources/lang"];

/// Hány fájlt / komponenst vizsgálunk legfeljebb.
const MAX_FILES: usize = 60;

/// A `.callgraph/gen.toml` tartalma (csak, amit a preset használ).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `[components]`: szerep (`input` …) → komponens-név.
    pub components: BTreeMap<String, String>,
    /// `[i18n] enabled`.
    pub i18n: Option<bool>,
}

/// A sor megjegyzés nélkül (a `#` idézőjelen belül nem megjegyzés).
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return line.get(..i).unwrap_or(line),
            _ => {}
        }
    }
    line
}

impl Config {
    /// A TOML-részhalmaz (`[szakasz]`, `kulcs = "érték"` / `true` / `false`) értelmezése; ami nem érthető, kimarad.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut section = String::new();
        for raw in text.lines() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim().trim_matches(['"', '\'']));
            match (section.as_str(), key) {
                ("components", _) if !value.is_empty() => {
                    out.components.insert(key.to_owned(), value.to_owned());
                }
                ("i18n", "enabled") => {
                    out.i18n = match value {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    };
                }
                _ => {}
            }
        }
        out
    }

    /// A projekt `.callgraph/gen.toml`-ja; hiányzó fájlra az alapértelmezés.
    pub fn load(env: Env<'_>) -> Self {
        env.fs
            .read(&env.root.join(CONFIG))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }
}

/// A komponens szerepe az űrlapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Label,
    Input,
    Error,
    Button,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// A szó egész szóként szerepel-e a szövegben.
fn contains_word(hay: &str, word: &str) -> bool {
    hay.match_indices(word).any(|(i, _)| {
        let before = hay.get(..i).and_then(|h| h.chars().next_back());
        let after = hay.get(i + word.len()..).and_then(|h| h.chars().next());
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// A `defineProps(…)` / `props:` környéke (nincs ilyen: az egész szöveg).
fn props_region(text: &str) -> &str {
    let Some(at) = text.find("defineProps").or_else(|| text.find("props:")) else {
        return text;
    };
    text.get(at..(at + 800).min(text.len()))
        .or_else(|| text.get(at..))
        .unwrap_or(text)
}

fn has_prop(text: &str, name: &str) -> bool {
    contains_word(props_region(text), name)
}

/// Egy komponens szerepe a fájlnév és a props alapján (nem űrlap-elem: `None`).
fn classify(stem: &str, text: &str) -> Option<Role> {
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
    if is("error") || is("validation") {
        return Some(Role::Error);
    }
    if is("label") {
        return Some(Role::Label);
    }
    let other = ["checkbox", "radio", "select", "switch", "textarea", "file"];
    if has_prop(text, "modelValue")
        && (is("input") || is("text") || text.contains("<input"))
        && !other.iter().any(|&w| is(w))
    {
        return Some(Role::Input);
    }
    None
}

/// A találatok rangsora egy szerepen belül: a megszokott név, majd a jellemző szó, majd a névsor.
fn rank(stem: &str, role: Role) -> (u8, String) {
    let default = match role {
        Role::Label => "InputLabel",
        Role::Input => "TextInput",
        Role::Error => "InputError",
        Role::Button => "PrimaryButton",
    };
    let low = stem.to_ascii_lowercase();
    let typical = match role {
        Role::Button => low.contains("primary") || low.contains("submit"),
        Role::Input => low.contains("text"),
        _ => false,
    };
    let score = if stem == default {
        0
    } else if typical {
        1
    } else {
        2
    };
    (score, stem.to_owned())
}

/// A címke-komponens szövegének bemenete: `value` / `label` prop, különben a slot (`None`).
fn label_prop(text: &str) -> Option<String> {
    ["value", "label"]
        .into_iter()
        .find(|p| has_prop(text, p))
        .map(str::to_owned)
}

/// A hiba-komponens prop-ja (`message`, `error`, `errors`; alapérték `message`).
fn error_prop(text: &str) -> String {
    ["message", "error", "errors"]
        .into_iter()
        .find(|p| has_prop(text, p))
        .unwrap_or("message")
        .to_owned()
}

/// A mappák `.vue` fájljai `(név, szöveg)` párokban, névsorban (a szöveg a közös memóból).
fn candidates(project: &VueProject<'_>, dirs: &[&str]) -> BTreeMap<String, Rc<String>> {
    let mut out = BTreeMap::new();
    for dir in dirs {
        for file in project.vue_files(dir, MAX_FILES) {
            let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if out.contains_key(stem) {
                continue;
            }
            if let Some(text) = project.text(&file) {
                out.insert(stem.to_owned(), text);
            }
        }
    }
    out
}

/// Alias mögötti komponensek: fájlnév-törzs → (import-út, a fájl szövege).
type AliasComponents = BTreeMap<String, (String, Rc<String>)>;

/// Az alias mögötti (külön UI-csomag) komponensek: `stem → (import-út, szöveg)`. Az oldalak által már importált
/// komponensek is ide kerülnek (akkor is, ha a mappájuk nem komponens-mappa), utána az alias-mappák `components`
/// fájljai; a második érték az oldalak importjainak fájlnév-törzsei.
fn alias_candidates(project: &VueProject<'_>) -> (AliasComponents, BTreeSet<String>) {
    let mut out = BTreeMap::new();
    let mut used = BTreeSet::new();
    let stem_of = |path: &str| {
        path.rsplit('/')
            .next()
            .and_then(|f| f.strip_suffix(".vue"))
            .map(str::to_owned)
    };
    for source in project.sample_vue_imports() {
        let Some(stem) = stem_of(&source) else {
            continue;
        };
        used.insert(stem.clone());
        for (prefix, dir) in project.aliases() {
            if let Some(rest) = source
                .strip_prefix(prefix.as_str())
                .and_then(|r| r.strip_prefix('/'))
                && let Some(text) = project.text(&dir.join(rest))
            {
                out.entry(stem.clone()).or_insert((source.clone(), text));
            }
        }
    }
    for (path, file) in project.alias_components(MAX_FILES * 3) {
        let Some(stem) = stem_of(&path) else {
            continue;
        };
        if !out.contains_key(&stem)
            && let Some(text) = project.text(&file)
        {
            out.insert(stem, (path, text));
        }
    }
    (out, used)
}

/// Honnan van a választás.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Config,
    Default,
    Guess,
}

/// Egy szerep választása: név, import-út, a komponens szövege (ha ismert), forrás.
struct Pick {
    name: String,
    path: String,
    text: Option<Rc<String>>,
    source: Source,
}

/// A szerep feloldása: `gen.toml` → megszokott név → (hívó) heurisztika. A hibás `gen.toml`-nevet a `bad` kapja.
fn resolve(
    project: &VueProject<'_>,
    config: &Config,
    key: &str,
    defaults: &[&str],
    bad: &mut Vec<String>,
) -> Option<Pick> {
    let found = |name: &str, source: Source| {
        vue::component_path(project, name).map(|path| Pick {
            name: name.to_owned(),
            path,
            text: None,
            source,
        })
    };
    if let Some(name) = config.components.get(key) {
        match found(name, Source::Config) {
            Some(pick) => return Some(pick),
            None => bad.push(name.clone()),
        }
    }
    defaults
        .iter()
        .find_map(|name| found(name, Source::Default))
}

/// A projekt-ismeret az oldalaknak.
pub struct Ui {
    pub parts: Parts,
    /// A PHP-oldalon (FormRequest `attributes()`) is fordítunk-e.
    pub php_i18n: bool,
    /// A döntések egy-egy sorban (mit választott a heurisztika, mit a `gen.toml`).
    pub notes: Vec<String>,
}

fn to_comp(pick: Pick, texts: &BTreeMap<String, Rc<String>>, role: Role) -> Comp {
    let text = pick.text.or_else(|| texts.get(&pick.name).cloned());
    let prop = match (role, text) {
        (Role::Label, Some(text)) if pick.source != Source::Default => label_prop(&text),
        (Role::Label, _) => Some("value".to_owned()),
        (Role::Error, Some(text)) if pick.source != Source::Default => Some(error_prop(&text)),
        (Role::Error, _) => Some("message".to_owned()),
        _ => None,
    };
    Comp {
        name: pick.name,
        path: pick.path,
        prop,
    }
}

/// A komponensek, a fordítás és a jegyzetek. `npm`: a `package.json` függőségei; `ziggy`: van-e Ziggy.
pub fn discover(
    env: Env<'_>,
    project: &VueProject<'_>,
    config: &Config,
    npm: &BTreeSet<String>,
    ziggy: bool,
) -> Ui {
    let mut bad: Vec<String> = Vec::new();
    let roles = [
        ("label", Role::Label, "InputLabel"),
        ("input", Role::Input, "TextInput"),
        ("error", Role::Error, "InputError"),
        ("button", Role::Button, "PrimaryButton"),
    ];
    let mut picks: Vec<Option<Pick>> = roles
        .iter()
        .map(|(key, _, default)| resolve(project, config, key, &[*default], &mut bad))
        .collect();
    let mut layout = resolve(
        project,
        config,
        "layout",
        &["AuthenticatedLayout", "AppLayout"],
        &mut bad,
    );

    // a heurisztika csak a hiányzó szerepekre fut (a komponens-fájlokat egyszer olvassa)
    let configured = picks.iter().zip(roles.iter()).any(|(pick, (_, role, _))| {
        matches!(role, Role::Label | Role::Error)
            && pick.as_ref().is_some_and(|p| p.source == Source::Config)
    });
    let mut texts = BTreeMap::new();
    let mut aliased: BTreeMap<String, String> = BTreeMap::new();
    let mut imported: BTreeSet<String> = BTreeSet::new();
    if picks.iter().any(Option::is_none) || configured {
        texts = candidates(project, &COMPONENT_DIRS);
        // külön UI-csomag (`@ui/…` alias): a helyi komponens-mappák nyernek, az alias-mögöttiek kiegészítik
        let (found, used) = alias_candidates(project);
        for (stem, (path, text)) in found {
            if let std::collections::btree_map::Entry::Vacant(slot) = texts.entry(stem.clone()) {
                slot.insert(text);
                aliased.insert(stem, path);
            }
        }
        imported = used;
    }
    for (pick, (_, role, _)) in picks.iter_mut().zip(roles.iter()) {
        if pick.is_some() {
            continue;
        }
        let best = texts
            .iter()
            .filter(|(stem, text)| classify(stem, text) == Some(*role))
            // az oldalak által már importált komponens előbbre való
            .min_by_key(|(stem, _)| (u8::from(!imported.contains(*stem)), rank(stem, *role)));
        if let Some((stem, text)) = best
            && let Some(path) = aliased
                .get(stem)
                .cloned()
                .or_else(|| vue::component_path(project, stem))
        {
            *pick = Some(Pick {
                name: stem.clone(),
                path,
                text: Some(Rc::clone(text)),
                source: Source::Guess,
            });
        }
    }
    if layout.is_none() {
        let layouts = candidates(project, &LAYOUT_DIRS);
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
            && let Some(path) = vue::component_path(project, stem)
        {
            layout = Some(Pick {
                name: stem.clone(),
                path,
                text: None,
                source: Source::Guess,
            });
        }
    }

    let mut notes = Vec::new();
    if !bad.is_empty() {
        notes.push(crate::tf!(
            "⚠ gen.toml: component not found: {}",
            "⚠ gen.toml: nincs ilyen komponens: {}",
            bad.join(", ")
        ));
    }
    let mut from_config: Vec<String> = Vec::new();
    let mut guessed: Vec<String> = Vec::new();
    let mut note_pick = |key: &str, pick: &Pick| match pick.source {
        Source::Config => from_config.push(format!("{key}={}", pick.name)),
        Source::Guess => guessed.push(format!("{key}={}", pick.name)),
        Source::Default => {}
    };
    for ((key, _, _), pick) in roles.iter().zip(&picks) {
        if let Some(pick) = pick {
            note_pick(key, pick);
        }
    }
    if let Some(pick) = &layout {
        note_pick("layout", pick);
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

    let mut comps = picks
        .into_iter()
        .zip(roles.iter())
        .map(|(pick, (_, role, _))| pick.map(|p| to_comp(p, &texts, *role)));
    let (label, input, error, button) = (
        comps.next().flatten(),
        comps.next().flatten(),
        comps.next().flatten(),
        comps.next().flatten(),
    );
    let (i18n, php_i18n) = detect_i18n(env, project, config, npm);
    Ui {
        parts: Parts {
            layout: layout.map(|p| (p.name, p.path)),
            label,
            input,
            error,
            button,
            ziggy,
            i18n,
        },
        php_i18n,
        notes,
    }
}

/// Az `név(` hívás szerepel-e a szövegben (`$t(`, `trans(`, `__(`).
fn calls(text: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    text.match_indices(&needle).any(|(i, _)| {
        !text
            .get(..i)
            .and_then(|h| h.chars().next_back())
            .is_some_and(is_ident)
    })
}

fn translates(text: &str) -> bool {
    ["$t", "trans", "__"].iter().any(|f| calls(text, f))
}

/// `(Vue-oldali, PHP-oldali)` fordítás: a Vue-oldal a `vue-i18n` / `laravel-vue-i18n` csomag, vagy a meglévő
/// `.vue` fájlokban `$t(` / `trans(` / `__(` hívás; a PHP-oldal (csak ha a Vue-oldal be van kapcsolva): van
/// `lang/` mappa, vagy a Blade-nézetek fordítanak. A `gen.toml` `[i18n] enabled` felülírja.
fn detect_i18n(
    env: Env<'_>,
    project: &VueProject<'_>,
    config: &Config,
    npm: &BTreeSet<String>,
) -> (bool, bool) {
    let vue = match config.i18n {
        Some(on) => on,
        None => {
            npm.contains("vue-i18n")
                || npm.contains("laravel-vue-i18n")
                || project.sample_any(translates)
        }
    };
    if !vue {
        return (false, false);
    }
    let lang_dir = LANG_PHP_DIRS
        .iter()
        .any(|d| env.fs.list(&env.root.join(d)).is_ok());
    let php = lang_dir
        || walk_php(env.fs, env.root.join("resources/views"))
            .take(20)
            .filter_map(|p| env.fs.read(&p).ok())
            .any(|text| translates(&text) || text.contains("@lang("));
    (true, php)
}

/// Egy nyelvi forrás: út és a kulcs-keresés.
struct LangFile {
    rel: String,
    /// JSON: a beolvasott érték; PHP: a csoport szövege (hiányzó fájl: üres szöveg).
    content: Lang,
}

enum Lang {
    Json(serde_json::Value),
    Php(String),
}

impl LangFile {
    fn has(&self, key: &str) -> bool {
        match &self.content {
            Lang::Json(doc) => {
                doc.get(key).is_some()
                    || key
                        .split('.')
                        .try_fold(doc, |node, part| node.get(part))
                        .is_some()
            }
            Lang::Php(text) => key
                .rsplit('.')
                .next()
                .is_some_and(|last| text.contains(&format!("'{last}'"))),
        }
    }
}

/// A nyelvi fájlok (legfeljebb 3): a JSON-ok, ha nincs, a `lang/<nyelv>/<csoport>.php` fájlok.
fn lang_files(env: Env<'_>, group: &str) -> Vec<LangFile> {
    let mut out: Vec<LangFile> = Vec::new();
    for dir in LANG_JSON_DIRS {
        let Ok(mut entries) = env.fs.list(&env.root.join(dir)) else {
            continue;
        };
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        for entry in entries {
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if entry.is_dir || !name.ends_with(".json") || out.len() >= 3 {
                continue;
            }
            let Ok(text) = env.fs.read(&entry.path) else {
                continue;
            };
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) {
                out.push(LangFile {
                    rel: format!("{dir}/{name}"),
                    content: Lang::Json(doc),
                });
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    for dir in LANG_PHP_DIRS {
        let Ok(mut entries) = env.fs.list(&env.root.join(dir)) else {
            continue;
        };
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        for entry in entries.into_iter().filter(|e| e.is_dir).take(3) {
            let Some(locale) = entry.path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let rel = format!("{dir}/{locale}/{group}.php");
            let text = env.fs.read(&env.root.join(&rel)).unwrap_or_default();
            out.push(LangFile {
                rel,
                content: Lang::Php(text),
            });
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// A kulcsok tömören: az első néhány, majd `… +N`.
fn compact(keys: &[&String]) -> String {
    const SHOWN: usize = 4;
    let head: Vec<&str> = keys.iter().take(SHOWN).map(|k| k.as_str()).collect();
    if keys.len() > SHOWN {
        format!("{} … +{}", head.join(", "), keys.len() - SHOWN)
    } else {
        head.join(", ")
    }
}

/// A fordítási jegyzet-sorok: nyelvi fájlonként a hiányzó (új) kulcsok; nyelvi fájl nélkül a teljes kulcslista.
pub fn i18n_notes(env: Env<'_>, keys: &BTreeSet<String>) -> Vec<String> {
    let Some(first) = keys.iter().next() else {
        return Vec::new();
    };
    let group = first.split('.').next().unwrap_or("");
    let files = lang_files(env, group);
    if files.is_empty() {
        let all: Vec<&String> = keys.iter().collect();
        return vec![crate::tf!(
            "i18n: no lang files found — add {} keys: {}",
            "i18n: nincs nyelvi fájl — {} kulcsot vegyél fel: {}",
            keys.len(),
            compact(&all)
        )];
    }
    files
        .iter()
        .map(|file| {
            let missing: Vec<&String> = keys.iter().filter(|k| !file.has(k)).collect();
            if missing.is_empty() {
                crate::tf!(
                    "i18n: all {} keys already in {}",
                    "i18n: mind a(z) {} kulcs megvan itt: {}",
                    keys.len(),
                    file.rel
                )
            } else {
                crate::tf!(
                    "i18n: {} keys missing in {}: {}",
                    "i18n: {} kulcs hiányzik itt: {} — {}",
                    missing.len(),
                    file.rel,
                    compact(&missing)
                )
            }
        })
        .collect()
}
