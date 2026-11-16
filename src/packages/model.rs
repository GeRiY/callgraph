//! A csomag-modell: ökoszisztéma, csomag (deklarált tartomány + telepített verzió), a manifest és a lock egy-egy
//! bejegyzése. A forrás-stratégiák (`sources/`) ezekre adaptálják a saját formátumukat.

use serde::{Deserialize, Serialize};

/// A csomagkezelő ökoszisztémája.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Ecosystem {
    /// Composer: `composer.json` + `composer.lock`.
    Php,
    /// npm / pnpm / yarn / bun: `package.json` + a lock.
    Node,
}

impl Ecosystem {
    /// A kiírt rövid név.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Node => "node",
        }
    }
}

/// Egy csomag: a manifest közvetlen függősége, vagy (a `installed_only` listában) csak a lockban szereplő.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Package {
    pub name: String,
    pub ecosystem: Ecosystem,
    /// A manifest verzió-tartománya (`^11.0`); a csak a lockban szereplőnél `None`.
    pub declared: Option<String>,
    /// A lock pontos verziója (vezető `v` nélkül); lock nélkül `None`.
    pub installed: Option<String>,
    pub dev: bool,
    /// PHP: a csomag psr-4 / psr-0 névtér-előtagjai a lockból, `\`-re végződve.
    pub namespaces: Vec<String>,
    /// PHP: a lock-bejegyzés `extra.laravel.providers`-t ad (a Laravel package auto-discovery tölti be, kód-hivatkozás
    /// nélkül is használt).
    #[serde(default)]
    pub auto_discovered: bool,
    /// PHP: a lock-bejegyzés `type: composer-plugin` (a Composer tölti be, nem a kód).
    #[serde(default)]
    pub plugin: bool,
}

/// A manifest egy függősége.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dependency {
    pub name: String,
    /// A verzió-tartomány, ahogy a manifestben áll (`^3.4`).
    pub range: String,
    pub dev: bool,
}

/// Egy manifest (`composer.json`, `package.json`) a csomagtérképhez szükséges része.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Manifest {
    /// A függőségek a manifest sorrendjében (előbb a prod, utána a dev; ismétlés nélkül).
    pub deps: Vec<Dependency>,
    /// PHP: a projekt saját psr-4 / psr-0 névterei (`autoload`, `autoload-dev`), `\`-re végződve.
    pub own_namespaces: Vec<String>,
    /// A csomag saját neve (`acme/shop`, `@acme/web`).
    pub name: Option<String>,
    /// Node: a `workspaces` minták (tömb vagy `{packages: […]}` alak).
    pub workspaces: Vec<String>,
}

/// Egy lock-bejegyzés.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Installed {
    pub name: String,
    /// A pontos verzió (vezető `v` nélkül).
    pub version: String,
    /// PHP: a lock `autoload` psr-4 / psr-0 kulcsai, `\`-re végződve.
    pub namespaces: Vec<String>,
    /// yarn: a tartományok, amelyekre ez a verzió feloldódott (több verziónál ez választ a közvetlen függőséghez).
    pub ranges: Vec<String>,
    /// PHP: `extra.laravel.providers` van a bejegyzésben (Laravel package auto-discovery).
    pub auto_discovered: bool,
    /// PHP: `type: composer-plugin`.
    pub plugin: bool,
}
