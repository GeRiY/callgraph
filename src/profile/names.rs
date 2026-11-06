//! B jel: a fájlnév (kiterjesztés nélkül) — CamelCase-végződés (`UserService`), JS / TS pontozott név
//! (`user.store`) és `use*` előtag, Rust-modulnév (`handlers`, `user_repo`); a `rules.rs` táblái szerint.

use super::classify::normalize;
use super::role::{Confidence, Kind, Role};
use super::rules::{DOTTED_RULES, Eco, FOLDER_RULES, SUFFIX_RULES, USE_PREFIX};

/// Egy illeszkedő név: szerep, bizonyosság, a kiírt alak és (CamelCase-végződésnél) a minta-példa végződése.
pub(crate) struct NameHit {
    pub role: Role,
    pub confidence: Confidence,
    pub shown: String,
    pub suffix: Option<&'static str>,
}

/// A fájlnév alapján (kiterjesztés nélkül): JS pontozott név és `use*`; Rust-modulnév (`handlers`, `user_repo`);
/// egyébként CamelCase-végződés.
pub(crate) fn name_hit(stem: &str, eco: Eco) -> Option<NameHit> {
    match eco {
        Eco::Js => js_name(stem).or_else(|| camel_suffix(stem.split('.').next()?, eco)),
        Eco::Rust => rust_module(stem),
        Eco::Php | Eco::Swift => camel_suffix(stem.split('.').next()?, eco),
    }
}

fn js_name(stem: &str) -> Option<NameHit> {
    if let Some((_, last)) = stem.rsplit_once('.') {
        let last = last.to_ascii_lowercase();
        if let Some((_, role, confidence)) = DOTTED_RULES.iter().find(|(name, ..)| *name == last) {
            return Some(NameHit {
                role: Role::named(role)?,
                confidence: *confidence,
                shown: format!("*.{last}"),
                suffix: None,
            });
        }
    }
    let rest = stem.strip_prefix("use")?;
    if rest.starts_with(|c: char| c.is_ascii_uppercase()) || rest.starts_with('-') {
        let (role, confidence) = USE_PREFIX;
        return Some(NameHit {
            role: Role::named(role)?,
            confidence,
            shown: "use*".into(),
            suffix: None,
        });
    }
    None
}

fn camel_suffix(stem: &str, eco: Eco) -> Option<NameHit> {
    SUFFIX_RULES
        .iter()
        .filter(|rule| rule.ecos.contains(eco) && stem.ends_with(rule.suffix))
        .find_map(|rule| {
            Some(NameHit {
                role: Role::named(rule.role)?,
                confidence: rule.confidence,
                shown: format!("*{}", rule.suffix),
                suffix: Some(rule.suffix),
            })
        })
}

/// Rust: a modul neve maga a réteg (`handlers.rs`, `routes.rs`), vagy az utolsó `_` utáni része (`user_repo.rs`).
fn rust_module(stem: &str) -> Option<NameHit> {
    let whole = normalize(stem);
    let last = stem.rsplit('_').next().map(normalize).unwrap_or_default();
    [whole, last].into_iter().find_map(|name| {
        let rule = FOLDER_RULES.iter().find(|rule| {
            rule.parent.is_none()
                && rule.ecos.contains(Eco::Rust)
                && rule.names.contains(&name.as_str())
        })?;
        let role = Role::named(rule.role)?;
        // a kontextus-nevek (domain…) fájlnévként nem mondanak elég sokat
        (role.kind() != Kind::Context).then(|| NameHit {
            role,
            confidence: rule.confidence,
            shown: format!("{stem}.rs"),
            suffix: None,
        })
    })
}
