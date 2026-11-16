//! A stack-sor: a kiemelt csomagok (frameworkök és a kódírás módját meghatározó könyvtárak) rögzített, stabil
//! sorrendben, a lock pontos verziójával — lock nélkül a deklarált tartománnyal, jelölve. Egy sor; 10 fölött `+N`.
//!
//! `stack:   laravel/framework 11.31.0 · livewire/livewire 3.5.2 · vue ^3.4 (range)   (composer.lock)`

use std::fmt::Write as _;

use super::model::Ecosystem;
use super::project::ProjectPackages;
use crate::i18n::Lang;

/// A kiemelt csomagok a kiírás sorrendjében.
const NOTABLE: &[(Ecosystem, &str)] = &[
    (Ecosystem::Php, "laravel/framework"),
    (Ecosystem::Php, "symfony/framework-bundle"),
    (Ecosystem::Php, "symfony/symfony"),
    (Ecosystem::Php, "yiisoft/yii2"),
    (Ecosystem::Php, "livewire/livewire"),
    (Ecosystem::Php, "filament/filament"),
    (Ecosystem::Php, "inertiajs/inertia-laravel"),
    (Ecosystem::Php, "doctrine/orm"),
    (Ecosystem::Php, "pestphp/pest"),
    (Ecosystem::Php, "phpunit/phpunit"),
    (Ecosystem::Node, "nuxt"),
    (Ecosystem::Node, "next"),
    (Ecosystem::Node, "vue"),
    (Ecosystem::Node, "react"),
    (Ecosystem::Node, "svelte"),
    (Ecosystem::Node, "@angular/core"),
    (Ecosystem::Node, "@nestjs/core"),
    (Ecosystem::Node, "express"),
    (Ecosystem::Node, "@inertiajs/vue3"),
    (Ecosystem::Node, "@inertiajs/react"),
    (Ecosystem::Node, "pinia"),
    (Ecosystem::Node, "vuex"),
    // UI-komponenskönyvtárak: a sablonok és az importok formáját határozzák meg
    (Ecosystem::Node, "primevue"),
    (Ecosystem::Node, "vuetify"),
    (Ecosystem::Node, "element-plus"),
    (Ecosystem::Node, "prisma"),
    (Ecosystem::Node, "tailwindcss"),
    (Ecosystem::Node, "typescript"),
    (Ecosystem::Node, "vite"),
    (Ecosystem::Node, "vitest"),
    (Ecosystem::Node, "jest"),
];

/// Teszt- és build-eszközök: a tömör (MCP-indító) sorból kimaradnak, mert a kód írásának módját nem határozzák meg.
const TOOLING: &[&str] = &[
    "pestphp/pest",
    "phpunit/phpunit",
    "typescript",
    "vite",
    "vitest",
    "jest",
];

/// A tömör sor rövid nevei (a többi csomag a saját nevével).
fn short_name(name: &str) -> &str {
    match name {
        "laravel/framework" => "laravel",
        "symfony/framework-bundle" | "symfony/symfony" => "symfony",
        "yiisoft/yii2" => "yii2",
        "livewire/livewire" => "livewire",
        "filament/filament" => "filament",
        "inertiajs/inertia-laravel" => "inertia",
        "doctrine/orm" => "doctrine",
        "@angular/core" => "angular",
        "@nestjs/core" => "nest",
        "@inertiajs/vue3" => "inertia-vue",
        "@inertiajs/react" => "inertia-react",
        other => other,
    }
}

/// A főverzió egy verzióból vagy tartományból (`^11.31` → `11`, `v3.5.2` → `3`, `0.462.0` → `0.462`).
fn major(version: &str) -> Option<String> {
    let start = version.find(|c: char| c.is_ascii_digit())?;
    let mut parts = version[start..]
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty());
    let first = parts.next()?;
    Some(if first == "0" {
        parts
            .next()
            .map_or_else(|| "0".to_owned(), |minor| format!("0.{minor}"))
    } else {
        first.to_owned()
    })
}

/// Legfeljebb ennyi csomag a sorban; a többi `+N`.
const SHOWN: usize = 10;

/// Egy kiemelt csomag a stack-sorban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackEntry {
    pub name: &'static str,
    /// A lock pontos verziója, lock nélkül a deklarált tartomány.
    pub version: Option<String>,
    pub from_lock: bool,
}

/// A projekt kiemelt csomagjai és a felhasznált lock-fájlok.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Stack {
    pub entries: Vec<StackEntry>,
    pub locks: Vec<String>,
}

impl Stack {
    /// A kiemelt csomagok a részek közvetlen függőségeiből (több részben is meglévőnél az első részé).
    pub(crate) fn of(map: &ProjectPackages) -> Self {
        let mut entries = Vec::new();
        let mut used: Vec<usize> = Vec::new();
        for &(ecosystem, name) in NOTABLE {
            let found = map
                .parts
                .iter()
                .enumerate()
                .filter(|(_, part)| part.ecosystem == ecosystem)
                .find_map(|(at, part)| {
                    part.packages
                        .iter()
                        .find(|package| package.name == name)
                        .map(|package| (at, package))
                });
            let Some((at, package)) = found else {
                continue;
            };
            let from_lock = package.installed.is_some();
            if from_lock && !used.contains(&at) {
                used.push(at);
            }
            entries.push(StackEntry {
                name,
                version: package
                    .installed
                    .clone()
                    .or_else(|| package.declared.clone()),
                from_lock,
            });
        }
        used.sort_unstable();
        let mut locks: Vec<String> = Vec::new();
        for lock in used.iter().filter_map(|&at| map.parts[at].lock.as_ref()) {
            if !locks.contains(lock) {
                locks.push(lock.clone());
            }
        }
        Self { entries, locks }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A tömör stack-sor az MCP-kapcsolódáskor (minden munkamenet elején megjelenik, ezért a legrövidebb forma):
    /// rövid nevek, csak a főverzió (0.x-nél fő.al), a teszt- és build-eszközök nélkül, tartomány-jelölés nélkül.
    /// `stack: laravel 12 · inertia 2 · vue 3 · tailwindcss 4`. Üres, ha nincs mit mutatni.
    pub(crate) fn compact_line(&self) -> Option<String> {
        let items: Vec<String> = self
            .entries
            .iter()
            .filter(|entry| !TOOLING.contains(&entry.name))
            .take(SHOWN)
            .map(|entry| match entry.version.as_deref().and_then(major) {
                Some(version) => format!("{} {version}", short_name(entry.name)),
                None => short_name(entry.name).to_owned(),
            })
            .collect();
        (!items.is_empty()).then(|| format!("stack: {}", items.join(" · ")))
    }

    /// A stack-sor (sorvége nélkül).
    pub(crate) fn line(&self, lang: Lang) -> String {
        let range = lang.pick("(range)", "(tartomány)");
        let mut items: Vec<String> = self
            .entries
            .iter()
            .take(SHOWN)
            .map(|entry| match (&entry.version, entry.from_lock) {
                (Some(version), true) => format!("{} {version}", entry.name),
                (Some(version), false) => format!("{} {version} {range}", entry.name),
                (None, _) => entry.name.to_owned(),
            })
            .collect();
        let rest = self.entries.len().saturating_sub(SHOWN);
        if rest > 0 {
            items.push(format!("+{rest}"));
        }
        let mut line = format!("stack:   {}", items.join(" · "));
        if !self.locks.is_empty() {
            let _ = write!(line, "   ({})", self.locks.join(", "));
        }
        line
    }
}
