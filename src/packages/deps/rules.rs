//! Specification: a `deps` vizsgálat kivételei egy-egy kis szabály-típus (`PackageRule`), amelyek egy csomagra és a
//! használatára (`Usage`) mondanak igent / nemet; az `AnyOf` kombinátor sorban kérdezi őket, és megmondja, melyik
//! illett. Új kivétel = új típus + egy sor a listában, a vizsgálat (`super`) nem változik.
//!
//! - fölösleges? (nincs hívás / hivatkozás) kivételei: `ToolingPackage`, `FrameworkAutoload`, `ComposerPlugin`,
//!   `CssOrAssetOnly`, `NoNamespace`, `UserIgnore`;
//! - hiányzik (használt, de nem deklarált) kivételei: `BuiltinModule`, `FrameworkBundled`, `WorkspacePackage`,
//!   `UserIgnore` (a `peerDependencies` a manifest-olvasóban eleve deklaráltnak számít);
//! - csak dev (dev-függőség az alkalmazás-kódban) kivételei: `BundledFrontend`, `UserIgnore`.

use super::Usage;
use crate::packages::model::{Ecosystem, Package};
use crate::packages::project::{EcosystemPackages, ProjectPackages};

/// Egy szabály: illik-e a csomagra (a használatával együtt).
pub(crate) trait PackageRule {
    /// A szabály neve (a tesztek és a hibakeresés számára).
    fn name(&self) -> &'static str;
    fn matches(&self, package: &Package, usage: &Usage) -> bool;
}

/// Kombinátor: a szabályok közül az első illő dönt.
pub(crate) struct AnyOf<'r>(pub &'r [&'r dyn PackageRule]);

impl AnyOf<'_> {
    /// Az első illő szabály neve; ha egyik sem illik, `None`.
    pub(crate) fn first_match(&self, package: &Package, usage: &Usage) -> Option<&'static str> {
        self.0
            .iter()
            .find(|rule| rule.matches(package, usage))
            .map(|rule| rule.name())
    }
}

impl PackageRule for AnyOf<'_> {
    fn name(&self) -> &'static str {
        "any-of"
    }

    fn matches(&self, package: &Package, usage: &Usage) -> bool {
        self.first_match(package, usage).is_some()
    }
}

/// Egy név illik-e egy mintára: pontos név, vagy `*`-ra végződő előtag (`phpstan/*`, `eslint*`); kis- /
/// nagybetű-független.
pub(crate) fn name_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix)),
        None => pattern.eq_ignore_ascii_case(name),
    }
}

fn any_matches(patterns: &[&str], name: &str) -> bool {
    patterns.iter().any(|pattern| name_matches(pattern, name))
}

/// CLI- / build- / lint- / teszt-eszközök: a kód nem hivatkozik rájuk.
const TOOLING_PHP: &[&str] = &[
    "laravel/pint",
    "laravel/sail",
    "laravel/pail",
    "laravel/tinker",
    "laravel/breeze",
    "phpstan/*",
    "larastan/*",
    "nunomaduro/larastan",
    "vimeo/psalm",
    "friendsofphp/php-cs-fixer",
    "squizlabs/php_codesniffer",
    "rector/rector",
    "carthage-software/mago",
    "phpunit/phpunit",
    "pestphp/*",
    "mockery/mockery",
    "nunomaduro/collision",
    "fakerphp/faker",
    "spatie/laravel-ignition",
    "barryvdh/laravel-ide-helper",
    "roave/security-advisories",
];

/// Ugyanez npm-ben.
const TOOLING_NODE: &[&str] = &[
    "typescript",
    "vite",
    "@vitejs/*",
    "laravel-vite-plugin",
    "eslint*",
    "@eslint/*",
    "prettier*",
    "vitest",
    "jest",
    "@types/*",
    "tailwindcss",
    "@tailwindcss/*",
    "postcss*",
    "autoprefixer",
    "sass",
    "concurrently",
    "vue-tsc",
    "@vue/tsconfig",
    "cross-env",
    "husky",
    "lint-staged",
    "wrangler",
    "playwright",
    "@playwright/*",
];

/// Eszköz-csomag (`laravel/pint`, `eslint-plugin-vue`, `@types/node`).
pub(crate) struct ToolingPackage;

impl PackageRule for ToolingPackage {
    fn name(&self) -> &'static str {
        "tooling"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        let list = match package.ecosystem {
            Ecosystem::Php => TOOLING_PHP,
            Ecosystem::Node => TOOLING_NODE,
        };
        any_matches(list, &package.name)
    }
}

/// A framework tölti be kód-hivatkozás nélkül (a nevükben `laravel` nélküliek; a többit a név adja).
const AUTOLOADED_PHP: &[&str] = &["barryvdh/*", "tightenco/ziggy", "spatie/laravel-*"];

/// PHP-csomag, amelyet a framework tölt be: a Laravel package auto-discovery (`extra.laravel.providers` a lockban,
/// vagy a nevében `laravel`: `laravel/sanctum`, `laravel/horizon`, `rakutentech/laravel-request-docs`).
pub(crate) struct FrameworkAutoload;

impl PackageRule for FrameworkAutoload {
    fn name(&self) -> &'static str {
        "framework-autoload"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        package.ecosystem == Ecosystem::Php
            && (package.auto_discovered
                || package.name.to_ascii_lowercase().contains("laravel")
                || any_matches(AUTOLOADED_PHP, &package.name))
    }
}

/// PHP-csomag psr-4 / psr-0 névtér nélkül (`classmap` / `files` autoload, metapackage, lock nélküli rész): a kód
/// hívásait nem lehet hozzá rendelni, így a „nincs hivatkozás” nem eldönthető.
pub(crate) struct NoNamespace;

impl PackageRule for NoNamespace {
    fn name(&self) -> &'static str {
        "no-namespace"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        package.ecosystem == Ecosystem::Php && package.namespaces.is_empty()
    }
}

/// Composer-plugin (`type: composer-plugin` a lockban) és a `php-http/discovery`.
pub(crate) struct ComposerPlugin;

impl PackageRule for ComposerPlugin {
    fn name(&self) -> &'static str {
        "composer-plugin"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        package.ecosystem == Ecosystem::Php
            && (package.plugin || package.name.eq_ignore_ascii_case("php-http/discovery"))
    }
}

/// CSS-ből / HTML-ből használt npm-csomagok (betűkészlet, ikon, CSS-keretrendszer).
const ASSET_NODE: &[&str] = &[
    "@fontsource/*",
    "@fontsource-variable/*",
    "primeicons",
    "@fortawesome/fontawesome-free",
    "bootstrap",
    "bootstrap-icons",
    "normalize.css",
];

/// Csak CSS-ből / HTML-ből használt npm-csomag: a listán van, vagy egy `.css` / `.scss` `@import` / `@use` hivatkozik
/// rá (`Usage::css`).
pub(crate) struct CssOrAssetOnly;

impl PackageRule for CssOrAssetOnly {
    fn name(&self) -> &'static str {
        "css-or-asset"
    }

    fn matches(&self, package: &Package, usage: &Usage) -> bool {
        package.ecosystem == Ecosystem::Node
            && (usage.css > 0 || any_matches(ASSET_NODE, &package.name))
    }
}

/// A `.callgraphignore` `deps-ignore <név>` sorai (pontos név vagy `vendor/*` minta).
pub(crate) struct UserIgnore {
    patterns: Vec<String>,
}

impl UserIgnore {
    pub(crate) fn new(patterns: Vec<String>) -> Self {
        Self { patterns }
    }
}

impl PackageRule for UserIgnore {
    fn name(&self) -> &'static str {
        "user-ignore"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        self.patterns
            .iter()
            .any(|pattern| name_matches(pattern, &package.name))
    }
}

/// Nem csomag: a Node beépített modulja (`fs`, `node:path`), PHP-ban a platform (`php`, `ext-*`, `lib-*`).
pub(crate) struct BuiltinModule;

impl PackageRule for BuiltinModule {
    fn name(&self) -> &'static str {
        "builtin"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        match package.ecosystem {
            Ecosystem::Node => crate::packages::resolve::is_builtin(&package.name),
            Ecosystem::Php => !package.name.contains('/'),
        }
    }
}

/// A framework saját ökoszisztémája: ha a rész deklarálja a frameworköt, ezeket a csomagokat vele együtt kapja
/// (a közvetlen használatuk nem „hiányzó függőség”). Egy helyen, frameworkönként egy sor.
const FRAMEWORK_BUNDLED: &[(&str, &[&str])] = &[
    (
        "laravel/framework",
        &[
            "symfony/*",
            "illuminate/*",
            "psr/*",
            "nesbot/carbon",
            "league/*",
            "guzzlehttp/*",
            "monolog/*",
            "laravel/*",
            "ramsey/*",
            "doctrine/*",
            "brick/math",
            "vlucas/phpdotenv",
        ],
    ),
    (
        "yiisoft/yii2",
        &["yiisoft/*", "psr/*", "ezyang/htmlpurifier", "cebe/markdown"],
    ),
    (
        "nuxt",
        &[
            "vue",
            "vue-router",
            "@vue/*",
            "@nuxt/*",
            "nitropack",
            "h3",
            "ofetch",
            "@unhead/*",
        ],
    ),
    ("vue", &["@vue/*"]),
    ("next", &["@next/*", "styled-jsx"]),
    ("@angular/core", &["@angular/*", "rxjs", "tslib", "zone.js"]),
];

/// A rész deklarált frameworkjeivel érkező csomagok (`FRAMEWORK_BUNDLED`).
pub(crate) struct FrameworkBundled {
    patterns: Vec<&'static str>,
}

impl FrameworkBundled {
    pub(crate) fn of(part: &EcosystemPackages) -> Self {
        let patterns = FRAMEWORK_BUNDLED
            .iter()
            .filter(|(framework, _)| part.packages.iter().any(|p| p.name == *framework))
            .flat_map(|(_, bundled)| bundled.iter().copied())
            .collect();
        Self { patterns }
    }
}

impl PackageRule for FrameworkBundled {
    fn name(&self) -> &'static str {
        "framework-bundled"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        any_matches(&self.patterns, &package.name)
    }
}

/// A projekt saját (workspace-) csomagja: valamelyik rész manifestjének `name`-je (`@acme/ui`).
pub(crate) struct WorkspacePackage {
    names: Vec<String>,
}

impl WorkspacePackage {
    pub(crate) fn of(map: &ProjectPackages) -> Self {
        let names = map.all().filter_map(|part| part.name.clone()).collect();
        Self { names }
    }
}

impl PackageRule for WorkspacePackage {
    fn name(&self) -> &'static str {
        "workspace"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        self.names.contains(&package.name)
    }
}

/// Bundlerek: ha a Node-rész ezek egyikével épül, a böngészőbe kerülő kód függőségei jogosan lehetnek
/// `devDependencies`-ben (a build beleforgatja őket).
const BUNDLERS: &[&str] = &[
    "vite",
    "webpack",
    "laravel-mix",
    "rollup",
    "esbuild",
    "parcel",
    "@vue/cli-service",
    "react-scripts",
    "@angular-devkit/build-angular",
    "@rspack/core",
    "@rsbuild/core",
    "tsup",
];

/// Bundlerrel épülő frontend-rész Node-csomagja (a „csak dev” nem hiba).
pub(crate) struct BundledFrontend {
    bundled: bool,
}

impl BundledFrontend {
    pub(crate) fn of(part: &EcosystemPackages) -> Self {
        let bundled = part.ecosystem == Ecosystem::Node
            && part
                .packages
                .iter()
                .any(|package| any_matches(BUNDLERS, &package.name));
        Self { bundled }
    }
}

impl PackageRule for BundledFrontend {
    fn name(&self) -> &'static str {
        "bundled-frontend"
    }

    fn matches(&self, package: &Package, _usage: &Usage) -> bool {
        self.bundled && package.ecosystem == Ecosystem::Node
    }
}
