//! Névfeloldás egy fájlon belül: importok (`use`), a fájl saját típusai és függvényei.
//! A hívás-kategorizálás ezt kérdezi (`Resolution`), syn-típus nélkül.

use std::collections::{HashMap, HashSet};

use super::module_path::{ModulePath, short_key};

/// Egy leírt út feloldása.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// A fájlban definiált elem; az érték a container-kulcs (`Típus::név`, `modul::fn`).
    Local(String),
    /// A crate saját eleme (`crate::…` kanonikus út).
    Crate(Vec<String>),
    /// Másik crate (std / core / alloc is) eleme, teljes úttal.
    External(Vec<String>),
    /// Egyetlen, nem feloldható név (glob-import, szülő-modul, prelude-jelölt).
    Unknown(Vec<String>),
}

/// A fájl névtábla: importok, saját típusok, szabad függvények és `self`-es metódusok.
#[derive(Debug, Default)]
pub(crate) struct Resolver {
    module: ModulePath,
    imports: Vec<(String, Vec<String>)>,
    types: HashSet<String>,
    free_fns: HashMap<String, Vec<String>>,
    methods: HashMap<String, Vec<String>>,
}

impl Resolver {
    pub(crate) fn new(module: ModulePath) -> Self {
        Self {
            module,
            ..Self::default()
        }
    }

    pub(crate) fn module(&self) -> &ModulePath {
        &self.module
    }

    /// Import felvétele (alias → kanonikus út); azonos aliasnál az első marad (a fájl-szintű `use`).
    pub(crate) fn add_import(&mut self, alias: String, canonical: Vec<String>) {
        if !self.imports.iter().any(|(a, _)| *a == alias) {
            self.imports.push((alias, canonical));
        }
    }

    /// Saját típus (struct / enum / union / trait) a minősített nevével (`tests::Stub`).
    pub(crate) fn add_type(&mut self, qualified: String) {
        self.types.insert(qualified);
    }

    pub(crate) fn add_free_fn(&mut self, name: &str, container: &str) {
        self.free_fns
            .entry(name.to_owned())
            .or_default()
            .push(container.to_owned());
    }

    pub(crate) fn add_method(&mut self, name: &str, container: &str) {
        let containers = self.methods.entry(name.to_owned()).or_default();
        if !containers.iter().any(|c| c == container) {
            containers.push(container.to_owned());
        }
    }

    /// Van-e `name` nevű `self`-es metódus a containerben.
    pub(crate) fn has_method(&self, container: &str, name: &str) -> bool {
        self.methods
            .get(name)
            .is_some_and(|cs| cs.iter().any(|c| c == container))
    }

    /// Az egyetlen container, amelyben `name` nevű `self`-es metódus van.
    pub(crate) fn unique_method(&self, name: &str) -> Option<&str> {
        match self.methods.get(name).map(Vec::as_slice) {
            Some([only]) => Some(only),
            _ => None,
        }
    }

    /// Saját típus keresése a beágyazott modulokból kifelé.
    fn local_type(&self, inline: &[String], name: &str) -> Option<String> {
        (0..=inline.len()).rev().find_map(|depth| {
            let qualified = qualify(&inline[..depth], name);
            self.types.contains(&qualified).then_some(qualified)
        })
    }

    /// Egy hívás-út feloldása a hívó containere (`Self`) és a beágyazott modulok szerint.
    pub(crate) fn resolve(
        &self,
        path: &[String],
        container: &str,
        inline: &[String],
    ) -> Resolution {
        let Some(first) = path.first() else {
            return Resolution::Unknown(Vec::new());
        };
        let rest = &path[1..];
        if first == "Self" {
            return Resolution::Local(join(container, rest));
        }
        if let Some(ty) = self.local_type(inline, first) {
            return Resolution::Local(join(&ty, rest));
        }
        if rest.is_empty()
            && let Some(containers) = self.free_fns.get(first)
        {
            let own = qualify_container(&self.module, inline);
            let at = containers
                .iter()
                .find(|c| **c == own)
                .unwrap_or(&containers[0]);
            return Resolution::Local(format!("{at}::{first}"));
        }
        let full = match self.imports.iter().find(|(alias, _)| alias == first) {
            Some((_, target)) => target.iter().chain(rest).cloned().collect(),
            None if matches!(first.as_str(), "crate" | "self" | "super") => {
                self.module.canonical(inline, path)
            }
            None if rest.is_empty() || starts_upper(first) => {
                return Resolution::Unknown(path.to_vec());
            }
            None => return Resolution::External(path.to_vec()),
        };
        self.classify(full)
    }

    /// Kanonikus út besorolása: a fájl saját eleme, a crate eleme vagy külső.
    fn classify(&self, full: Vec<String>) -> Resolution {
        if full.first().map(String::as_str) != Some("crate") {
            return Resolution::External(full);
        }
        let own = self.module.absolute(&[]);
        if full.len() > own.len() && full[..own.len()] == own[..] {
            let item = &full[own.len()];
            if self.types.contains(item) {
                return Resolution::Local(full[own.len()..].join("::"));
            }
            if full.len() == own.len() + 1 && self.free_fns.contains_key(item) {
                return Resolution::Local(format!("{}::{item}", self.module.container_name()));
            }
        }
        Resolution::Crate(full)
    }

    /// Egy leírt `use`-út kanonizálása (`self::` / `super::` → `crate::…`).
    pub(crate) fn canonical(&self, inline: &[String], written: &[String]) -> Vec<String> {
        self.module.canonical(inline, written)
    }
}

/// A crate-belső út rövid kulcsa (a másik fájl container-nevéhez illeszkedik).
pub(crate) fn crate_key(path: &[String]) -> String {
    short_key(path)
}

/// Név minősítése a beágyazott modulokkal (`["tests"]`, `Stub` → `tests::Stub`).
pub(crate) fn qualify(inline: &[String], name: &str) -> String {
    if inline.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", inline.join("::"))
    }
}

/// A szabad függvények containere: a fájl-modul, beágyazott modulban a modul minősített neve.
pub(crate) fn qualify_container(module: &ModulePath, inline: &[String]) -> String {
    if inline.is_empty() {
        module.container_name()
    } else {
        inline.join("::")
    }
}

fn join(head: &str, rest: &[String]) -> String {
    std::iter::once(head.to_owned())
        .chain(rest.iter().cloned())
        .collect::<Vec<_>>()
        .join("::")
}

fn starts_upper(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(s: &str) -> Vec<String> {
        s.split("::").map(str::to_owned).collect()
    }

    fn resolver() -> Resolver {
        let mut r = Resolver::new(ModulePath::from_file("src/cli/args.rs"));
        r.add_import("PathBuf".into(), segs("std::path::PathBuf"));
        r.add_import("Method".into(), segs("crate::model::Method"));
        r.add_type("Args".into());
        r.add_type("tests::Stub".into());
        r.add_free_fn("usage", "args");
        r
    }

    #[test]
    fn resolves_local_items() {
        let r = resolver();
        let res = |p: &str, inline: &[String]| r.resolve(&segs(p), "Args", inline);
        assert_eq!(res("Self::new", &[]), Resolution::Local("Args::new".into()));
        assert_eq!(
            res("Args::parse", &[]),
            Resolution::Local("Args::parse".into())
        );
        assert_eq!(res("usage", &[]), Resolution::Local("args::usage".into()));
        let tests = ["tests".to_owned()];
        assert_eq!(
            res("Stub::new", &tests),
            Resolution::Local("tests::Stub::new".into())
        );
        assert_eq!(
            res("super::usage", &tests),
            Resolution::Local("args::usage".into())
        );
    }

    #[test]
    fn resolves_imports_and_crates() {
        let r = resolver();
        let res = |p: &str| r.resolve(&segs(p), "Args", &[]);
        assert_eq!(
            res("PathBuf::from"),
            Resolution::External(segs("std::path::PathBuf::from"))
        );
        assert_eq!(
            res("Method::new"),
            Resolution::Crate(segs("crate::model::Method::new"))
        );
        assert_eq!(
            res("regex::Regex::new"),
            Resolution::External(segs("regex::Regex::new"))
        );
        assert_eq!(res("helper"), Resolution::Unknown(segs("helper")));
        assert_eq!(res("Foo::new"), Resolution::Unknown(segs("Foo::new")));
        assert_eq!(crate_key(&segs("crate::model::Method::new")), "Method::new");
    }
}
