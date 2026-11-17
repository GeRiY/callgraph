//! Fordítási kulcsok ↔ használatuk: a nyelvfájlok (`lang/hu.json`, `resources/js/lang/en.json`, `lang/en/auth.php`)
//! nyelvenként, és a forrásokban a `$t('a.b')` / `__('x')` hívások. Egy nyelvi **halmaz** egy mappa nyelvei
//! (`lang/`); a nyelvek egymáshoz képest hiányoznak, a használatok a halmazokhoz képest. A dinamikus kulcs
//! (`` `x.${y}` ``, összefűzés) soha nem „hiányzó”: csak előtagként számít használatnak.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::i18n_scan::{Found, Leaf, Usage};

/// A nyelvi könyvtárak nevei (a nyelvfájl valamelyik szülőmappája).
const LANG_DIRS: [&str; 7] = [
    "lang",
    "langs",
    "locales",
    "locale",
    "i18n",
    "translations",
    "l10n",
];

/// A keretrendszer által fogyasztott / szállított PHP-fájlok (a kulcsaik nem „nem használtak” és nem „hiányzók”).
const FRAMEWORK_FILES: [&str; 4] = ["validation", "auth", "pagination", "passwords"];

/// Egy kulcs definíciója.
#[derive(Debug, Clone)]
pub(crate) struct KeyDef {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) value: String,
}

/// Egy nyelv kulcsai egy halmazban.
#[derive(Debug, Default)]
pub(crate) struct LocaleData {
    pub(crate) keys: BTreeMap<String, KeyDef>,
    /// A levelek és minden pontozott előtagjuk (egy objektum-kulcs is „létezik”).
    nodes: HashSet<String>,
    pub(crate) files: Vec<String>,
}

impl LocaleData {
    fn insert(&mut self, key: &str, def: KeyDef) {
        let mut at = 0;
        while let Some(dot) = key[at..].find('.') {
            self.nodes.insert(key[..at + dot].to_owned());
            at += dot + 1;
        }
        self.nodes.insert(key.to_owned());
        self.keys.entry(key.to_owned()).or_insert(def);
    }

    fn has(&self, key: &str) -> bool {
        self.nodes.contains(key)
    }
}

/// Egy mappa nyelvei (`lang/`: `en`, `hu`).
#[derive(Debug, Default)]
pub(crate) struct LangSet {
    pub(crate) dir: String,
    pub(crate) locales: BTreeMap<String, LocaleData>,
}

impl LangSet {
    /// A nyelv rövid címkéje a figyelmeztetésben: az egyetlen JSON-fájl neve (`hu.json`), különben a nyelv kódja.
    fn label(&self, locale: &str) -> String {
        match self.locales.get(locale).map(|data| data.files.as_slice()) {
            Some([file]) if file.ends_with(".json") => {
                file.rsplit('/').next().unwrap_or(file).to_owned()
            }
            _ => locale.to_owned(),
        }
    }
}

/// Egy kulcs állapota a nyelvfájlokhoz képest.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KeyState {
    /// Minden nyelvben megvan.
    Ok,
    /// Van olyan nyelv, ahol hiányzik (címkék).
    Missing(Vec<String>),
    /// Egyik nyelvfájlban sincs.
    Unknown,
    /// Nem ellenőrizhető (névtér, keretrendszer-kulcs, nincs nyelvfájl).
    Skip,
}

/// Egy nyelvfájl jelentése a többi nyelvhez és a használatokhoz képest.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FileReport {
    /// A fájl kulcsai, amelyek egy másik nyelvben hiányoznak: (nyelv, összes, minta).
    pub(crate) missing_in: Vec<(String, usize, Vec<String>)>,
    /// Más nyelvek kulcsai, amelyek ebben hiányoznak: (nyelv, összes, minta).
    pub(crate) lacks: Vec<(String, usize, Vec<String>)>,
    /// A fájl nem használt kulcsai: (összes, minta).
    pub(crate) unused: (usize, Vec<String>),
}

impl FileReport {
    pub(crate) fn is_empty(&self) -> bool {
        self.missing_in.is_empty() && self.lacks.is_empty() && self.unused.0 == 0
    }
}

/// A nyelvfájlok és a használatok.
#[derive(Debug, Default)]
pub(crate) struct I18n {
    pub(crate) sets: Vec<LangSet>,
    file_index: HashMap<String, (usize, String)>,
    pub(crate) found: Found,
    used: HashSet<String>,
    scanned: bool,
}

/// Az `a` / `b` (fájl-)csoport: a PHP-nál a fájlnév (`php:auth`), a JSON-nál közös.
fn group(file: &str) -> String {
    if file.ends_with(".php") {
        let name = file.rsplit('/').next().unwrap_or(file);
        format!("php:{name}")
    } else if is_script(file) {
        "script".to_owned()
    } else {
        "json".to_owned()
    }
}

/// Forrásmondat-szerű kulcs (`Hello!`, `Regards,`, `Dear :name`): a keretrendszer / vendor nézetei használhatják,
/// ezért sosem „nem használt” (a többi nyelvből továbbra is hiányozhat).
fn sentence_like(key: &str) -> bool {
    key.contains(char::is_whitespace)
        || key.ends_with(['!', '?', '.', ',', ':'])
        || key
            .match_indices(':')
            .any(|(at, _)| key[at + 1..].starts_with(|c: char| c.is_alphabetic()))
}

fn locale_like(text: &str) -> bool {
    let (head, tail) = match text.split_once(['-', '_']) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    };
    (2..=3).contains(&head.len())
        && head.bytes().all(|c| c.is_ascii_lowercase())
        && tail.is_none_or(|tail| {
            (2..=4).contains(&tail.len()) && tail.bytes().all(|c| c.is_ascii_alphabetic())
        })
}

/// TS / JS szótár-modul-e a fájl (kiterjesztés szerint).
fn is_script(file: &str) -> bool {
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| matches!(ext, "ts" | "js" | "mjs"))
}

fn is_lang_dir(name: &str) -> bool {
    LANG_DIRS.contains(&name)
}

/// A nyelvfájl halmaza (a mappája) és nyelve az útból; `None`, ha nem nyelvfájl-alakú az út.
pub(super) fn lang_target(rel: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = rel.split('/').collect();
    let (file, dirs) = parts.split_last()?;
    let (stem, ext) = file.rsplit_once('.')?;
    let last = dirs.last().copied();
    match ext {
        "json" | "ts" | "js" | "mjs" => {
            if locale_like(stem) && last.is_some_and(is_lang_dir) {
                return Some((dirs.join("/"), stem.to_owned()));
            }
            let parent = last?;
            let grand = dirs.len().checked_sub(2).map(|i| dirs[i])?;
            (locale_like(parent) && is_lang_dir(grand))
                .then(|| (dirs[..dirs.len() - 1].join("/"), parent.to_owned()))
        }
        "php" if !stem.ends_with(".blade") => {
            let parent = last?;
            let grand = dirs.len().checked_sub(2).map(|i| dirs[i])?;
            (locale_like(parent) && is_lang_dir(grand))
                .then(|| (dirs[..dirs.len() - 1].join("/"), parent.to_owned()))
        }
        _ => None,
    }
}

/// TS / JS szótár-modul alakú nyelvfájl-e az út.
pub(crate) fn is_script_path(rel: &str) -> bool {
    is_script(rel) && lang_target(rel).is_some()
}

/// Nyelvfájl-alakú-e az út.
pub(crate) fn is_lang_path(rel: &str) -> bool {
    lang_target(rel).is_some()
}

impl I18n {
    /// Egy nyelvfájl felvétele (a szöveg a fájl tartalma); ami nem értelmezhető, kimarad.
    pub(super) fn add_lang(&mut self, rel: &str, text: &str) {
        let Some((dir, locale)) = lang_target(rel) else {
            return;
        };
        let leaves: Vec<Leaf> = if is_script(rel) {
            super::i18n_script::script_leaves(text)
        } else if rel.ends_with(".json") {
            let Some(all) = crate::lang::json::leaves(text) else {
                return;
            };
            let strings: Vec<Leaf> = all
                .into_iter()
                .filter(|(_, _, value)| value.is_some())
                .collect();
            if strings.is_empty() {
                return;
            }
            strings
        } else {
            let stem = rel
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".php"))
                .unwrap_or("");
            super::i18n_scan::php_leaves(text, stem)
        };
        if leaves.is_empty() {
            return;
        }
        let index = match self.sets.iter().position(|set| set.dir == dir) {
            Some(index) => index,
            None => {
                self.sets.push(LangSet {
                    dir,
                    locales: BTreeMap::new(),
                });
                self.sets.len() - 1
            }
        };
        let data = self.sets[index].locales.entry(locale.clone()).or_default();
        data.files.push(rel.to_owned());
        for (key, line, value) in leaves {
            let def = KeyDef {
                file: rel.to_owned(),
                line,
                value: value.unwrap_or_default(),
            };
            data.insert(&key, def);
        }
        self.file_index.insert(rel.to_owned(), (index, locale));
    }

    /// Egy forrásfájl fordítási hívásai.
    pub(super) fn add_source(&mut self, rel: &str, ext: &str, text: &str) {
        super::i18n_scan::scan(rel, ext, text, &mut self.found);
        self.scanned = true;
    }

    /// A gyűjtés vége: a használt kulcsok halmaza.
    pub(super) fn finish(&mut self) {
        self.used = self
            .found
            .usages
            .iter()
            .map(|usage| usage.key.clone())
            .collect();
    }

    /// Van-e nyelvfájl.
    pub(crate) fn has_sets(&self) -> bool {
        !self.sets.is_empty()
    }

    /// Nyelvfájl-e a fájl (a halmaza indexe és a nyelve).
    pub(crate) fn lang_file(&self, rel: &str) -> Option<(usize, &str)> {
        let (index, locale) = self.file_index.get(rel)?;
        Some((*index, locale.as_str()))
    }

    fn framework_key(key: &str) -> bool {
        key.split_once('.')
            .is_some_and(|(head, _)| FRAMEWORK_FILES.contains(&head))
    }

    /// Egy kulcs állapota (a dinamikus kulcsot a hívó nem adja ide).
    pub(crate) fn check(&self, key: &str) -> KeyState {
        if self.sets.is_empty() || key.contains(':') {
            return KeyState::Skip;
        }
        let mut found = false;
        let mut missing: Vec<String> = Vec::new();
        for set in &self.sets {
            if !set.locales.values().any(|data| data.has(key)) {
                continue;
            }
            found = true;
            for (locale, data) in &set.locales {
                let label = set.label(locale);
                if !data.has(key) && !missing.contains(&label) {
                    missing.push(label);
                }
            }
        }
        if !found {
            return if Self::framework_key(key) {
                KeyState::Skip
            } else {
                KeyState::Unknown
            };
        }
        if missing.is_empty() {
            KeyState::Ok
        } else {
            KeyState::Missing(missing)
        }
    }

    /// A figyelmeztető szöveg egy hívás-kulcshoz (` ⚠ missing in hu.json` / ` ⚠ no key`), ha van mit mondani.
    pub(crate) fn warning(&self, key: &str, lang: crate::i18n::Lang) -> Option<String> {
        match self.check(key) {
            KeyState::Missing(labels) => Some(format!(
                " ⚠ {} {}",
                lang.pick("missing in", "hiányzik:"),
                labels.join(", ")
            )),
            KeyState::Unknown => Some(format!(" ⚠ {}", lang.pick("no key", "nincs ilyen kulcs"))),
            _ => None,
        }
    }

    /// Használt-e a kulcs: hívás, hívott szülő-objektum, dinamikus előtag vagy idézőjeles hivatkozás.
    pub(crate) fn is_used(&self, key: &str) -> bool {
        if Self::framework_key(key) || sentence_like(key) {
            return true;
        }
        let mut end = 0;
        loop {
            let part = match key[end..].find('.') {
                Some(dot) => &key[..end + dot],
                None => key,
            };
            if self.used.contains(part) || self.found.literals.contains(part) {
                return true;
            }
            if part.len() == key.len() {
                break;
            }
            end = part.len() + 1;
        }
        self.found
            .prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix.as_str()))
    }

    /// A kulcs literál használati helyei (fájl, sor).
    pub(crate) fn usages_of(&self, key: &str) -> Vec<&Usage> {
        self.found
            .usages
            .iter()
            .filter(|usage| usage.key == key)
            .collect()
    }

    /// Egy nyelvfájl jelentése; a `limit` a kulcs-minták hossza. `None`, ha nem nyelvfájl.
    pub(crate) fn file_report(&self, rel: &str, limit: usize) -> Option<FileReport> {
        let (index, locale) = self.lang_file(rel)?;
        let set = &self.sets[index];
        let mine = set.locales.get(locale)?;
        let own = group(rel);
        let own_keys: Vec<&String> = mine
            .keys
            .iter()
            .filter(|(_, def)| def.file == rel)
            .map(|(key, _)| key)
            .collect();
        let mut report = FileReport::default();
        for (other, data) in &set.locales {
            if other == locale || !data.files.iter().any(|file| group(file) == own) {
                continue;
            }
            let absent: Vec<&String> = own_keys
                .iter()
                .copied()
                .filter(|key| !data.has(key))
                .collect();
            if !absent.is_empty() {
                report.missing_in.push((
                    other.clone(),
                    absent.len(),
                    absent
                        .iter()
                        .take(limit)
                        .map(|key| (*key).clone())
                        .collect(),
                ));
            }
            let lacking: Vec<&String> = data
                .keys
                .iter()
                .filter(|(key, def)| group(&def.file) == own && !mine.has(key))
                .map(|(key, _)| key)
                .collect();
            if !lacking.is_empty() {
                report.lacks.push((
                    other.clone(),
                    lacking.len(),
                    lacking
                        .iter()
                        .take(limit)
                        .map(|key| (*key).clone())
                        .collect(),
                ));
            }
        }
        if self.scanned {
            let unused: Vec<&String> = own_keys
                .iter()
                .copied()
                .filter(|key| !self.is_used(key))
                .collect();
            report.unused = (
                unused.len(),
                unused
                    .iter()
                    .take(limit)
                    .map(|key| (*key).clone())
                    .collect(),
            );
        }
        Some(report)
    }

    /// A kulcs (vagy a pontozott előtagja alatti levelek) értékei nyelvenként a fájl halmazában: `(kulcs, [(címke,
    /// sor, érték)])`; a hiányzó nyelv `None`. Legfeljebb `limit` kulcs.
    pub(crate) fn key_rows(&self, rel: &str, name: &str, limit: usize) -> Vec<KeyRow> {
        let Some((index, _)) = self.lang_file(rel) else {
            return Vec::new();
        };
        let set = &self.sets[index];
        let prefix = format!("{name}.");
        let mut keys: Vec<&String> = set
            .locales
            .values()
            .flat_map(|data| data.keys.keys())
            .filter(|key| key.as_str() == name || key.starts_with(&prefix))
            .collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .take(limit)
            .map(|key| KeyRow {
                key: key.clone(),
                values: set
                    .locales
                    .iter()
                    .map(|(locale, data)| (set.label(locale), data.keys.get(key).cloned()))
                    .collect(),
            })
            .collect()
    }
}

/// Egy felső szintű kulcs a szótár-modul kulcs-fájában: a név, a levelek száma és a közvetlen gyerekek nevei.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TreeGroup {
    pub(crate) name: String,
    pub(crate) leaves: usize,
    pub(crate) children: Vec<String>,
}

impl I18n {
    /// A szótár-modul (TS / JS nyelvfájl) kulcs-fája: felső szint, darabszám, a következő szint nevei.
    pub(crate) fn key_tree(&self, rel: &str) -> Vec<TreeGroup> {
        let Some((index, locale)) = self.lang_file(rel) else {
            return Vec::new();
        };
        let Some(mine) = self.sets[index].locales.get(locale) else {
            return Vec::new();
        };
        let mut groups: Vec<TreeGroup> = Vec::new();
        for (key, def) in &mine.keys {
            if def.file != rel {
                continue;
            }
            let mut parts = key.splitn(3, '.');
            let top = parts.next().unwrap_or_default();
            let child = parts.next();
            if groups.last().is_none_or(|group| group.name != top) {
                groups.push(TreeGroup {
                    name: top.to_owned(),
                    leaves: 0,
                    children: Vec::new(),
                });
            }
            let Some(group) = groups.last_mut() else {
                continue;
            };
            group.leaves += 1;
            if let Some(child) = child {
                let branch = parts.next().is_some();
                let label = if branch {
                    format!("{child}.")
                } else {
                    child.to_owned()
                };
                let seen = group
                    .children
                    .last()
                    .is_some_and(|last| last == &label || last.trim_end_matches('.') == child);
                if !seen {
                    group.children.push(label);
                }
            }
        }
        groups
    }
}

/// Egy kulcs sora a kulcs-nézethez: nyelvenként a definíció (vagy hiány).
#[derive(Debug)]
pub(crate) struct KeyRow {
    pub(crate) key: String,
    pub(crate) values: Vec<(String, Option<KeyDef>)>,
}
