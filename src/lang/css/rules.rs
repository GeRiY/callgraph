//! A CSS-szabályok outline-fajtája és fejléc-helye (a szkenner segédei).

/// A `;`-re végződő at-szabályok, amik az outline-ba kerülnek (a Tailwind v4-éi is); az `@apply` nem ilyen.
const STATEMENT_AT_RULES: &[&str] = &[
    "import",
    "tailwind",
    "plugin",
    "source",
    "config",
    "custom-variant",
    "layer",
    "charset",
    "namespace",
    "reference",
    "use",
    "forward",
];

/// A szabály fajtája az outline-hoz: `at-rule` (`@media`, `@theme` …) vagy `rule` (szelektor).
pub(super) fn rule_kind(header: &str) -> &'static str {
    if header.starts_with('@') {
        "at-rule"
    } else {
        "rule"
    }
}

/// Outline-ba kerülő, `;`-re végződő at-szabály-e a (megjegyzés nélküli, egysoros) fejléc.
pub(super) fn is_statement_at_rule(header: &str) -> bool {
    let Some(name) = header.strip_prefix('@') else {
        return false;
    };
    let keyword = name
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | '"' | '\''))
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    STATEMENT_AT_RULES.contains(&keyword.as_str())
}

/// A fejléc első (nem üres, nem megjegyzés) karakterének bájt-eltolása a szegmensben.
pub(super) fn header_offset(raw: &str) -> usize {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i = raw[i + 2..]
                .find("*/")
                .map_or(bytes.len(), |end| i + 2 + end + 2);
        } else {
            return i;
        }
    }
    0
}
