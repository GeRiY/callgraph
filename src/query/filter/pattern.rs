//! A `--grep` / `callers` minta előfeldolgozása: a PHP névtér-elválasztó (`App\Svc\Real::m`) a regexben escape-nek
//! számítana (`\S` = nem-szóköz, `\R`, `\B`…), ezért a névtér-alakú `\` literálissá (`\\`) válik.

use std::borrow::Cow;

use crate::error::CallgraphError;

/// Azonosító-karakter: a névtér-szegmens vége.
fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A plain identifier pattern: only `[A-Za-z0-9_]` (a leading `$` is allowed, as in JS `$el` / PHP names); no regex
/// metacharacter, no `::`, no `.`. Such a pattern matches whole names (see `namespace_pattern`).
pub(crate) fn is_plain_identifier(pattern: &str) -> bool {
    let name = pattern.strip_prefix('$').unwrap_or(pattern);
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A névtér-elválasztó alakú `\`-ek kétszerezése: egy `\`, amelyet azonosító-karakter előz meg és NAGYBETŰ követ
/// (`App\Svc`, `o\B`, `p\S`), literális `\\` lesz. Változatlan marad a már kétszerezett `\\`, a szándékos
/// escape (`\.`, `\d`, `\w`, `\s`, `\b`; kisbetű után kisbetű: `a\d`), a nem azonosító-karakter utáni `\S`
/// (`\s\S`, `[\S]`, `^\S`), és az olyan nagybetűs osztály, amelyet kvantor követ (`foo\S+`, `\P{L}`) — az nem
/// névtér-szegmens. Idempotens.
pub(crate) fn namespace_pattern(pattern: &str) -> Cow<'_, str> {
    if is_plain_identifier(pattern) {
        // whole-word match: the name is a separate segment of the target (`x.name`, `T::name`, `?::name`,
        // `App\Name::m`, `name()`), never a substring of a longer name. `.*name.*` keeps the substring search.
        let escaped = pattern.replace('$', "\\$");
        // case-sensitive (`(?-i:…)`): `store` is a method name, `Store` a class — they must not match each other
        return Cow::Owned(format!(
            "(?-i:(?:^|[^A-Za-z0-9_$]){escaped}(?:$|[^A-Za-z0-9_$]))"
        ));
    }
    if !pattern.contains('\\') {
        return Cow::Borrowed(pattern);
    }
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 4);
    let mut changed = false;
    // az előző karakter szó szerinti azonosító-karakter volt (nem escape-sorozat része)
    let mut after_ident = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' {
            out.push(c);
            after_ident = is_ident(c);
            i += 1;
            continue;
        }
        let Some(&next) = chars.get(i + 1) else {
            // záró magányos `\`: a regex úgyis hibás, a hibaüzenet a javaslattal együtt jön
            out.push(c);
            break;
        };
        let quantified = matches!(chars.get(i + 2), Some('+' | '*' | '?' | '{'));
        if after_ident && next.is_ascii_uppercase() && !quantified {
            out.push_str("\\\\");
            changed = true;
            i += 1;
        } else {
            // escape-pár (`\\`, `\.`, `\d`…): egyben marad
            out.push(c);
            out.push(next);
            i += 2;
        }
        after_ident = false;
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(pattern)
    }
}

/// A „hibás minta” hiba; ha a minta `\`-t tartalmaz, a névtér-elválasztóról szóló javaslattal.
pub(crate) fn invalid_pattern(error: &regex::Error, pattern: &str) -> CallgraphError {
    let mut message = crate::tf!("invalid pattern: {error}", "hibás minta: {error}");
    if pattern.contains('\\') {
        message.push_str(crate::t!(
            "\n💡 `\\` is a regex escape: double the namespace separator (`App\\\\Svc\\\\Real::m`) or use the `Real::m` form.",
            "\n💡 a `\\` regex-escape: a névtér-elválasztót kétszerezd (`App\\\\Svc\\\\Real::m`), vagy használd a `Real::m` alakot."
        ));
    }
    CallgraphError::Usage(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_separators_become_literal() {
        assert_eq!(namespace_pattern(r"App\Svc\Real::m"), r"App\\Svc\\Real::m");
        assert_eq!(namespace_pattern(r"o\B"), r"o\\B");
        assert_eq!(namespace_pattern(r"p\S"), r"p\\S");
        assert_eq!(namespace_pattern(r"c\R"), r"c\\R");
        assert_eq!(
            namespace_pattern(r"Illuminate\Support\Facades\DB::raw"),
            r"Illuminate\\Support\\Facades\\DB::raw"
        );
    }

    #[test]
    fn deliberate_escapes_and_doubled_separators_stay() {
        for same in [
            r"foo\.bar",
            r"a\d+",
            r"\w+\s\b",
            r"app\svc",
            r"App\\Svc\\Real::m",
            r"\Svc",
            r"x\S+",
            r"a\P{L}",
            r"plain::name",
            r"\\Svc",
            "a\\",
        ] {
            assert_eq!(namespace_pattern(same), same, "{same}");
        }
    }

    #[test]
    fn is_idempotent_and_matches_php_targets() {
        let once = namespace_pattern(r"App\Svc\Real::m").into_owned();
        assert_eq!(namespace_pattern(&once), once);
        let re = regex::RegexBuilder::new(&once)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(re.is_match(r"App\Svc\Real::m"));
        assert!(!re.is_match("AppSvcReal::m"));
        assert!(!re.is_match("App Svc Real::m"));
    }

    fn matches(pattern: &str, target: &str) -> bool {
        regex::RegexBuilder::new(&namespace_pattern(pattern))
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match(target)
    }

    #[test]
    fn plain_identifier_matches_whole_names_not_substrings() {
        for hit in [
            "execute",
            "x.execute",
            "?::execute",
            "Type::execute",
            "App\\Svc::execute",
            "a->execute",
        ] {
            assert!(matches("execute", hit), "{hit}");
        }
        for miss in [
            "executeCommand",
            "executeC",
            "myexecute",
            "execute_all",
            "x.executeC",
        ] {
            assert!(!matches("execute", miss), "{miss}");
        }
        // a class name still finds its qualified static calls and instantiations
        assert!(matches(
            "SongService",
            "App\\Services\\SongService::deleteSongs"
        ));
        assert!(matches("Repo", "Repo::find"));
        assert!(!matches("Repo", "RepoFactory::make"));
        // the case decides between a method name and a class name
        assert!(matches("store", "Cache::store"));
        assert!(!matches("store", "Store::get"));
        assert!(!matches("execute", "cmd.go::Execute"));
        assert!(matches("Store", "Store::get"));
        assert!(!matches("Store", "Cache::store"));
        assert!(matches("$el", "this.$el"));
        assert!(!matches("$el", "this.$elx"));
    }

    #[test]
    fn explicit_regex_keeps_the_substring_behaviour() {
        assert!(matches(".*execute.*", "executeCommand"));
        assert!(matches("execute.*", "executeCommand"));
        assert!(matches("exec|run", "runner"));
        assert!(matches("execut[e]", "executeCommand"));
        assert!(matches("^execute", "executeCommand"));
    }

    #[test]
    fn scoped_and_dotted_patterns_are_unchanged() {
        for same in ["Store::fetch", "a.b", "Store::fetch$", "route::orders.show"] {
            assert!(!is_plain_identifier(same), "{same}");
            assert_eq!(namespace_pattern(same), same, "{same}");
        }
        assert!(matches("Store::fetch", "Store::fetch_all"));
        assert!(matches("a.b", "xa.bx"));
    }

    #[test]
    fn invalid_pattern_error_carries_the_namespace_hint() {
        let bad = String::from(r"app\models");
        let error = regex::Regex::new(&bad).unwrap_err();
        let text = invalid_pattern(&error, &bad).to_string();
        assert!(text.contains("💡") && text.contains("Real::m"), "{text}");
        let open = String::from("(");
        let error = regex::Regex::new(&open).unwrap_err();
        let text = invalid_pattern(&error, &open).to_string();
        assert!(!text.contains("💡"), "{text}");
    }
}
