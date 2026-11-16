//! A név-alapú szabályok táblái (adat): mappanév (A jel) és fájlnév-végződés (B jel), ökoszisztémánként.
//! Új szabály: egy sor; a szerep-név a `role.rs` táblájából való (a tesztek ellenőrzik, hogy mind létezik).

use super::role::Confidence::{self, High, Medium};

/// A fájl ökoszisztémája a kiterjesztéséből (a szabályok ezek szerint szűrnek; a réteg ezzel minősül, ha egy szerep
/// több ökoszisztémában is előfordul: `controller (php)` / `controller (js)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Eco {
    Php,
    Js,
    Swift,
    Rust,
    Kotlin,
}

impl Eco {
    /// `.php` → PHP, `.js/.ts/.vue…` → JS, `.swift`, `.rs`, `.kt` / `.kts`; más (CSS, JSON, MD…) nem kód-réteg: `None`.
    pub(crate) fn of_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "php" => Some(Self::Php),
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "vue" => Some(Self::Js),
            "swift" => Some(Self::Swift),
            "rs" => Some(Self::Rust),
            "kt" | "kts" => Some(Self::Kotlin),
            _ => None,
        }
    }

    /// A kiírt név (a réteg-minősítőben és a `--layer controller:js` szűrőben).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Js => "js",
            Self::Swift => "swift",
            Self::Rust => "rust",
            Self::Kotlin => "kotlin",
        }
    }

    /// Felhasználói bemenet (`php`, `js` / `ts` / `vue`, `swift`, `rust` / `rs`), kisbetűsítve.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "php" => Some(Self::Php),
            "js" | "ts" | "vue" | "javascript" | "typescript" => Some(Self::Js),
            "swift" => Some(Self::Swift),
            "rust" | "rs" => Some(Self::Rust),
            "kotlin" | "kt" | "android" => Some(Self::Kotlin),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Php => 1,
            Self::Js => 2,
            Self::Swift => 4,
            Self::Rust => 8,
            Self::Kotlin => 16,
        }
    }
}

/// Ökoszisztémák halmaza (bitmaszk).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ecos(u8);

impl Ecos {
    pub(crate) fn contains(self, eco: Eco) -> bool {
        self.0 & eco.bit() != 0
    }
}

pub(crate) const ANY: Ecos = Ecos(0b11111);
pub(crate) const PHP: Ecos = Ecos(1);
pub(crate) const JS: Ecos = Ecos(2);
pub(crate) const RUST: Ecos = Ecos(8);
pub(crate) const KOTLIN: Ecos = Ecos(16);
const SWIFT_KOTLIN: Ecos = Ecos(4 | 16);
const PHP_SWIFT_RUST: Ecos = Ecos(1 | 4 | 8);
const NOT_PHP: Ecos = Ecos(2 | 4 | 8 | 16);
const JS_SWIFT_KOTLIN: Ecos = Ecos(2 | 4 | 16);

/// Mappanév-szabály: a normalizált (kisbetűs, `-`/`_` nélküli) mappanevek bármelyike; `parent`: a közvetlen
/// szülő-mappa neve is kell (`Http/Requests`).
#[derive(Debug)]
pub(crate) struct FolderRule {
    pub names: &'static [&'static str],
    pub parent: Option<&'static str>,
    pub role: &'static str,
    pub confidence: Confidence,
    pub ecos: Ecos,
}

const fn folder(
    names: &'static [&'static str],
    parent: Option<&'static str>,
    role: &'static str,
    confidence: Confidence,
    ecos: Ecos,
) -> FolderRule {
    FolderRule {
        names,
        parent,
        role,
        confidence,
        ecos,
    }
}

/// A mappanév-szabályok; az első illeszkedő dönt, ezért a szülős (szűkebb) szabály áll elöl.
pub(crate) static FOLDER_RULES: &[FolderRule] = &[
    folder(&["requests"], Some("http"), "request", High, PHP),
    folder(&["resources"], Some("http"), "dto", High, PHP),
    folder(&["commands"], Some("console"), "command", High, PHP),
    folder(&["factories"], Some("database"), "factory", High, PHP),
    folder(
        &["controllers", "controller"],
        None,
        "controller",
        High,
        ANY,
    ),
    folder(&["requests"], None, "request", Medium, PHP),
    folder(
        &["middleware", "middlewares"],
        None,
        "middleware",
        High,
        ANY,
    ),
    folder(&["policies", "policy"], None, "policy", High, PHP),
    folder(&["services", "service"], None, "service", High, ANY),
    folder(&["actions"], None, "service", Medium, PHP),
    folder(&["usecases", "usecase"], None, "service", Medium, ANY),
    folder(
        &["repositories", "repository", "repos", "repo"],
        None,
        "repository",
        High,
        ANY,
    ),
    folder(
        &["db", "database", "persistence"],
        None,
        "repository",
        Medium,
        RUST,
    ),
    folder(
        &["models", "model", "entities", "entity"],
        None,
        "model",
        High,
        ANY,
    ),
    folder(&["jobs", "job"], None, "job", High, ANY),
    folder(&["events", "event"], None, "event", High, ANY),
    folder(
        &["listeners", "listener", "subscribers"],
        None,
        "listener",
        High,
        ANY,
    ),
    folder(&["observers", "observer"], None, "observer", High, ANY),
    folder(&["mail", "mails", "mailables"], None, "mail", High, PHP),
    folder(&["notifications"], None, "notification", High, PHP),
    folder(&["enums", "enum"], None, "enum", High, ANY),
    folder(&["commands", "command"], None, "command", Medium, ANY),
    folder(&["handlers", "handler"], None, "handler", High, ANY),
    folder(&["providers", "provider"], None, "provider", High, PHP),
    folder(
        &["providers", "provider"],
        None,
        "provider",
        Medium,
        NOT_PHP,
    ),
    folder(&["factories", "factory"], None, "factory", High, ANY),
    folder(&["builders", "builder"], None, "builder", High, ANY),
    folder(&["strategies", "strategy"], None, "strategy", High, ANY),
    folder(&["adapters", "adapter"], None, "adapter", High, ANY),
    folder(&["decorators", "decorator"], None, "decorator", High, ANY),
    folder(
        &["dto", "dtos", "datatransferobjects"],
        None,
        "dto",
        High,
        ANY,
    ),
    folder(&["transformers"], None, "dto", Medium, PHP),
    folder(&["views", "view"], None, "view", High, PHP_SWIFT_RUST),
    folder(&["views"], None, "page", Medium, JS),
    folder(
        &["templates", "template", "layouts"],
        None,
        "view",
        High,
        PHP,
    ),
    folder(&["templates", "layouts"], None, "view", Medium, JS),
    folder(&["screens"], None, "page", Medium, JS),
    folder(&["screens"], None, "view", Medium, SWIFT_KOTLIN),
    folder(&["components", "component"], None, "component", High, JS),
    folder(&["components"], None, "view", Medium, SWIFT_KOTLIN),
    folder(&["pages", "page"], None, "page", High, JS),
    folder(
        &["composables", "composable", "hooks", "hook"],
        None,
        "composable",
        High,
        JS,
    ),
    folder(&["stores", "store"], None, "store", High, JS),
    folder(&["stores", "store"], None, "store", Medium, SWIFT_KOTLIN),
    folder(&["api", "apis"], None, "api", High, JS),
    folder(
        &["viewmodels", "viewmodel", "vms"],
        None,
        "viewmodel",
        High,
        ANY,
    ),
    folder(
        &["routes", "route", "routers", "router", "routing"],
        None,
        "route",
        High,
        ANY,
    ),
    folder(&["migrations", "migration"], None, "migration", High, ANY),
    folder(&["seeders", "seeds"], None, "migration", Medium, PHP),
    folder(
        &[
            "tests",
            "test",
            "__tests__",
            "spec",
            "specs",
            "e2e",
            "androidtest",
        ],
        None,
        "test",
        High,
        ANY,
    ),
    // minta- / fixture-mappák (golden/samples/*, tests/fixtures/*): a bennük lévő, akár más nyelvű mintakód nem a
    // projekt architektúrája
    folder(
        &["golden", "fixtures", "fixture", "testdata"],
        None,
        "test",
        High,
        ANY,
    ),
    folder(&["benches", "benchmarks"], None, "bench", High, RUST),
    folder(&["examples", "example"], None, "example", High, RUST),
    folder(
        &["config", "configs", "configuration"],
        None,
        "config",
        High,
        ANY,
    ),
    folder(&["domain"], None, "domain", High, ANY),
    folder(&["application"], None, "application", Medium, ANY),
    folder(
        &["infrastructure", "infra"],
        None,
        "infrastructure",
        High,
        ANY,
    ),
    folder(&["presentation"], None, "presentation", Medium, ANY),
    folder(&["ports", "port"], None, "port", High, ANY),
    // Swift / Kotlin: a tipikus SwiftUI- / Android-projekt mappaszerkezete (`Features/Chat/Content`, `Core/Networking`, `Shared/UI`)
    folder(
        &[
            "networking",
            "network",
            "api",
            "apiclient",
            "webservices",
            "remote",
        ],
        None,
        "api",
        Medium,
        SWIFT_KOTLIN,
    ),
    folder(
        &["persistence", "storage", "database", "db"],
        None,
        "repository",
        Medium,
        SWIFT_KOTLIN,
    ),
    folder(
        &[
            "ui",
            "uikit",
            "designsystem",
            "widgets",
            "controls",
            "cells",
            "content",
        ],
        None,
        "view",
        Medium,
        SWIFT_KOTLIN,
    ),
    folder(
        &["features", "feature", "modules", "scenes", "flows"],
        None,
        "feature",
        Medium,
        SWIFT_KOTLIN,
    ),
    folder(
        &[
            "shared",
            "common",
            "core",
            "utils",
            "util",
            "utilities",
            "helpers",
            "extensions",
            "support",
        ],
        None,
        "util",
        Medium,
        SWIFT_KOTLIN,
    ),
    // Kotlin / Android: `data/`, `di/`, `viewmodel/`, `navigation/`, `dao/`, `entity/`
    folder(
        &["data", "datasource", "datasources", "dao"],
        None,
        "repository",
        Medium,
        KOTLIN,
    ),
    folder(&["entity", "entities"], None, "model", Medium, KOTLIN),
    folder(
        &["viewmodel", "viewmodels"],
        None,
        "viewmodel",
        Medium,
        KOTLIN,
    ),
    folder(
        &["di", "inject", "injection", "hilt", "koin", "dagger"],
        None,
        "provider",
        Medium,
        KOTLIN,
    ),
    folder(&["navigation", "nav"], None, "route", Medium, KOTLIN),
    folder(&["activities", "fragments"], None, "view", Medium, KOTLIN),
];

/// Fájlnév-végződés (a kiterjesztés nélküli név vége, kis-/nagybetű érzékenyen: `UserService`).
#[derive(Debug)]
pub(crate) struct SuffixRule {
    pub suffix: &'static str,
    pub role: &'static str,
    pub confidence: Confidence,
    pub ecos: Ecos,
}

const fn suffix(
    suffix: &'static str,
    role: &'static str,
    confidence: Confidence,
    ecos: Ecos,
) -> SuffixRule {
    SuffixRule {
        suffix,
        role,
        confidence,
        ecos,
    }
}

/// A végződés-szabályok; az első illeszkedő dönt, ezért a hosszabb (`ServiceProvider`) áll a rövidebb előtt.
pub(crate) static SUFFIX_RULES: &[SuffixRule] = &[
    suffix("ServiceProvider", "provider", High, PHP),
    suffix("RepositoryInterface", "repository", High, ANY),
    suffix("RepositoryContract", "repository", High, ANY),
    suffix("Repository", "repository", High, ANY),
    suffix("Repo", "repository", Medium, ANY),
    suffix("Controller", "controller", High, ANY),
    suffix("Middleware", "middleware", High, ANY),
    suffix("Policy", "policy", High, PHP),
    suffix("Request", "request", Medium, PHP),
    suffix("ViewModel", "viewmodel", High, ANY),
    suffix("Service", "service", High, ANY),
    suffix("Factory", "factory", High, ANY),
    suffix("Builder", "builder", High, ANY),
    suffix("Observer", "observer", High, ANY),
    suffix("Listener", "listener", High, ANY),
    suffix("Subscriber", "listener", Medium, ANY),
    suffix("Handler", "handler", High, ANY),
    suffix("Strategy", "strategy", High, ANY),
    suffix("Adapter", "adapter", High, ANY),
    suffix("Decorator", "decorator", High, ANY),
    suffix("Resource", "dto", Medium, PHP),
    suffix("DTO", "dto", High, ANY),
    suffix("Dto", "dto", High, ANY),
    suffix("Job", "job", Medium, PHP),
    suffix("Event", "event", Medium, PHP),
    suffix("Command", "command", Medium, ANY),
    suffix("Provider", "provider", Medium, ANY),
    suffix("Screen", "view", Medium, SWIFT_KOTLIN),
    suffix("Screen", "page", Medium, JS),
    suffix("View", "view", Medium, SWIFT_KOTLIN),
    suffix("Page", "page", Medium, JS),
    suffix("Store", "store", Medium, JS_SWIFT_KOTLIN),
    suffix("Component", "component", Medium, JS),
    suffix("Migration", "migration", Medium, ANY),
    suffix("Seeder", "migration", Medium, PHP),
    suffix("Tests", "test", High, ANY),
    suffix("Test", "test", High, ANY),
    suffix("Spec", "test", High, ANY),
    suffix("Router", "route", Medium, ANY),
    suffix("Routes", "route", Medium, ANY),
    suffix("Model", "model", Medium, ANY),
    suffix("Client", "api", Medium, SWIFT_KOTLIN),
    suffix("Manager", "service", Medium, SWIFT_KOTLIN),
    suffix("Row", "view", Medium, SWIFT_KOTLIN),
    suffix("Cell", "view", Medium, SWIFT_KOTLIN),
    suffix("Sheet", "view", Medium, SWIFT_KOTLIN),
    suffix("Activity", "view", Medium, KOTLIN),
    suffix("Fragment", "view", Medium, KOTLIN),
    suffix("UseCase", "service", Medium, KOTLIN),
    suffix("Interactor", "service", Medium, KOTLIN),
    suffix("Dao", "repository", High, KOTLIN),
    suffix("RepositoryImpl", "repository", High, KOTLIN),
    suffix("DataSource", "repository", Medium, KOTLIN),
    suffix("Entity", "model", Medium, KOTLIN),
    suffix("Api", "api", Medium, KOTLIN),
    suffix("Module", "provider", Medium, KOTLIN),
];

/// JS / TS pontozott fájlnév (`user.store.ts`, `api.service.ts`, `a.spec.ts`): az utolsó pont utáni rész.
pub(crate) static DOTTED_RULES: &[(&str, &str, Confidence)] = &[
    ("store", "store", High),
    ("service", "service", High),
    ("controller", "controller", High),
    ("repository", "repository", High),
    ("model", "model", High),
    ("dto", "dto", High),
    ("api", "api", High),
    ("middleware", "middleware", High),
    ("routes", "route", High),
    ("router", "route", High),
    ("test", "test", High),
    ("spec", "test", High),
    ("config", "config", High),
    ("composable", "composable", High),
    ("page", "page", Medium),
    ("component", "component", Medium),
];

/// Ha semmi más nem dönt: a `.vue` fájl maga egy komponens (single-file component).
pub(crate) const VUE_FALLBACK: (&str, Confidence) = ("component", Medium);

/// `useAuth.ts` / `use-auth.ts` (JS): composable / hook.
pub(crate) const USE_PREFIX: (&str, Confidence) = ("composable", High);
