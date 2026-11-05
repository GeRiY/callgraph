//! Az ismert Rust frameworkök (adatvezérelt stratégiák), felismerési prioritás szerint:
//! előbb az alkalmazás-keretrendszerek, utána az ORM-ek, a CLI, a runtime, végül a szerializáció.

use super::FrameworkStrategy;

/// Egy ismert framework (a `FrameworkStrategy` adatvezérelt megvalósítása).
#[derive(Debug)]
pub(crate) struct KnownFramework {
    name: &'static str,
    package: &'static str,
    crates: &'static [&'static str],
}

impl FrameworkStrategy for KnownFramework {
    fn name(&self) -> &'static str {
        self.name
    }

    fn package(&self) -> &'static str {
        self.package
    }

    fn crates(&self) -> &'static [&'static str] {
        self.crates
    }
}

const fn fw(
    name: &'static str,
    package: &'static str,
    crates: &'static [&'static str],
) -> KnownFramework {
    KnownFramework {
        name,
        package,
        crates,
    }
}

/// Prioritás szerint: az első, a `Cargo.toml`-ban szereplő csomag adja a fájl `framework` mezőjét.
pub(crate) static KNOWN_FRAMEWORKS: &[KnownFramework] = &[
    fw("bevy", "bevy", &["bevy"]),
    fw("tauri", "tauri", &["tauri"]),
    fw(
        "leptos",
        "leptos",
        &["leptos", "leptos_router", "leptos_meta"],
    ),
    fw("dioxus", "dioxus", &["dioxus"]),
    fw("yew", "yew", &["yew"]),
    fw(
        "axum",
        "axum",
        &["axum", "axum_extra", "tower", "tower_http", "hyper"],
    ),
    fw(
        "actix-web",
        "actix-web",
        &["actix_web", "actix_rt", "actix"],
    ),
    fw("rocket", "rocket", &["rocket"]),
    fw("warp", "warp", &["warp"]),
    fw("poem", "poem", &["poem"]),
    fw("diesel", "diesel", &["diesel"]),
    fw("sqlx", "sqlx", &["sqlx"]),
    fw("sea-orm", "sea-orm", &["sea_orm"]),
    fw("clap", "clap", &["clap"]),
    fw("tokio", "tokio", &["tokio"]),
    fw("async-std", "async-std", &["async_std"]),
    fw("serde", "serde", &["serde", "serde_json", "serde_yaml"]),
];

/// Az út első szegmense valamelyik ismert framework crate-je-e (felismeréstől függetlenül).
pub(crate) fn is_framework_crate(root: &str) -> bool {
    KNOWN_FRAMEWORKS.iter().any(|f| f.owns_crate(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_crates_are_known() {
        assert!(is_framework_crate("actix_web"));
        assert!(is_framework_crate("tower_http"));
        assert!(!is_framework_crate("regex"));
    }

    #[test]
    fn names_and_packages_are_unique() {
        let mut names: Vec<_> = KNOWN_FRAMEWORKS.iter().map(|f| f.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), KNOWN_FRAMEWORKS.len());
    }
}
