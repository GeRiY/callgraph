//! Feloldatlan fogadójú metódushívás: a frontend `?::név` célt ad (Rust: `x.név()`, ha `x` típusa nem
//! következtethető). A `Típus::név` alakú hívó-keresés ezekre név szerint illeszkedik, bizonytalanként (`?`).

use regex::Regex;

/// A feloldatlan fogadó jelölése a cél elején.
pub(crate) const UNKNOWN_RECEIVER: &str = "?::";

/// Feloldatlan fogadójú (`?::név`) cél-e.
pub(crate) fn is_uncertain(target: &str) -> bool {
    target.starts_with(UNKNOWN_RECEIVER)
}

/// A cél puszta neve: `?::név` → `név`, más célnál a cél maga.
pub(crate) fn bare_name(target: &str) -> &str {
    target.strip_prefix(UNKNOWN_RECEIVER).unwrap_or(target)
}

/// A minta utolsó `::` utáni azonosítója (`Store::fetch`, `Store::fetch$`, `Store::fetch\(` → `fetch`).
pub(crate) fn pattern_name(pattern: &str) -> Option<&str> {
    let (_, tail) = pattern.rsplit_once("::")?;
    let tail = tail.trim_end_matches('$');
    let tail = tail.strip_suffix("\\b").unwrap_or(tail);
    let tail = tail.strip_suffix("\\(").unwrap_or(tail);
    (!tail.is_empty() && tail.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(tail)
}

/// Egy `?::név` cél név szerinti illeszkedése: a puszta névre illeszkedik a minta, vagy a minta
/// `Típus::név` alakú, és a neve egyezik (kis-/nagybetű-függetlenül, mint a keresés).
pub(crate) fn uncertain_match(regex: &Regex, target: &str) -> bool {
    let Some(name) = target.strip_prefix(UNKNOWN_RECEIVER) else {
        return false;
    };
    regex.is_match(name)
        || pattern_name(regex.as_str()).is_some_and(|wanted| wanted.eq_ignore_ascii_case(name))
}

/// Illeszkedik-e a minta a hívás-célra. A kicsomagolt használat (`const { site } = usePageChrome(); site.x()` → cél
/// `usePageChrome().site.x`) a hívás eredményének tagját hívja, nem a `usePageChrome`-ot: ilyen célnál a minta csak
/// a `().` utáni részre illik (maga a `usePageChrome()` hívás külön él).
pub(crate) fn target_matches(regex: &Regex, target: &str) -> bool {
    match target.split_once("().") {
        Some((_, rest)) => regex.is_match(rest),
        None => regex.is_match(target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructured_use_does_not_match_the_composable() {
        let re = regex("usePageChrome");
        assert!(target_matches(&re, "chrome.ts::usePageChrome"));
        assert!(!target_matches(&re, "chrome.ts::usePageChrome().site.x"));
        assert!(target_matches(&regex("push"), "useRouter().push"));
        assert!(!target_matches(&re, "other"));
    }

    fn regex(pattern: &str) -> Regex {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .unwrap()
    }

    #[test]
    fn type_method_pattern_matches_unknown_receiver_by_name() {
        assert!(uncertain_match(&regex("Store::fetch"), "?::fetch"));
        assert!(uncertain_match(&regex("Store::fetch$"), "?::fetch"));
        assert!(uncertain_match(&regex("^fetch$"), "?::fetch"));
        assert!(!uncertain_match(&regex("Store::fetch"), "?::fetch_all"));
        assert!(!uncertain_match(&regex("Store::fetch"), "fetch"));
        assert!(!uncertain_match(&regex("Store::save"), "?::fetch"));
    }

    #[test]
    fn bare_names() {
        assert!(is_uncertain("?::run"));
        assert!(!is_uncertain("Runner::run"));
        assert_eq!(bare_name("?::run"), "run");
        assert_eq!(bare_name("run"), "run");
    }
}
