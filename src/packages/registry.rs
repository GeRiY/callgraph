//! Registry / Factory: melyik ökoszisztémához melyik manifest és melyik lockok tartoznak (a jelen lévő fájl választja
//! ki a stratégiát; a lockok közül az első létező nyer), és melyik csomag melyik framework-nevet adja. Egy új
//! ökoszisztéma (Python, Go) itt egy sor + két stratégia.

use super::model::Ecosystem;
use super::sources::{
    BunLock, Composer, ComposerLock, LockSource, ManifestSource, Npm, NpmLock, PnpmLock, YarnLock,
};

/// Egy ökoszisztéma stratégiái.
pub(crate) struct EcosystemSpec {
    pub id: Ecosystem,
    pub manifest: &'static dyn ManifestSource,
    /// A lockok elsőbbségi sorrendben (az első létező nyer).
    pub locks: &'static [&'static dyn LockSource],
}

/// Az ismert ökoszisztémák, a részek sorrendjében (egy mappán belül előbb a PHP, utána a Node).
pub(crate) const ECOSYSTEMS: &[EcosystemSpec] = &[
    EcosystemSpec {
        id: Ecosystem::Php,
        manifest: &Composer,
        locks: &[&ComposerLock],
    },
    EcosystemSpec {
        id: Ecosystem::Node,
        manifest: &Npm,
        locks: &[&NpmLock, &PnpmLock, &YarnLock, &BunLock],
    },
];

/// Csomag → framework-név (a profil architektúra-felismerése és a PHP framework-detektor ugyanezeket a neveket
/// használja; az `inertia` név a lekérdezés-idejű framework-kötéseket kapcsolja be: `frameworks/`); ugyanazon a
/// részen belül ebben a sorrendben.
pub(crate) const FRAMEWORKS: &[(Ecosystem, &str, &str)] = &[
    (Ecosystem::Php, "laravel/framework", "laravel"),
    (Ecosystem::Php, "yiisoft/yii2", "yii"),
    (Ecosystem::Php, "inertiajs/inertia-laravel", "inertia"),
    (Ecosystem::Node, "nuxt", "nuxt"),
    (Ecosystem::Node, "vue", "vue"),
    (Ecosystem::Node, "next", "next"),
    (Ecosystem::Node, "react", "react"),
    (Ecosystem::Node, "svelte", "svelte"),
    (Ecosystem::Node, "@angular/core", "angular"),
    (Ecosystem::Node, "@inertiajs/vue3", "inertia"),
    (Ecosystem::Node, "@inertiajs/react", "inertia"),
    (Ecosystem::Node, "@inertiajs/svelte", "inertia"),
];
