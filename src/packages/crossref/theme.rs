//! Tailwind 4 `@theme` tokens: a `--<namespace>-<value>: …;` custom property inside an `@theme` block defines the
//! utility classes of its namespace (`--color-brand-500` -> `bg-brand-500`, `text-brand-500`, `border-brand-500` …).
//! Only the well-known namespaces are mapped. The unknown-token check is deliberately narrow: a used class is
//! reported only when the project defines tokens with the same prefix and first name segment (`brand-…`) but not
//! this exact value, and the segment is not a core Tailwind color name.

use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;

const COLOR: &[&str] = &[
    "text",
    "bg",
    "border",
    "border-x",
    "border-y",
    "border-t",
    "border-r",
    "border-b",
    "border-l",
    "border-s",
    "border-e",
    "ring",
    "ring-offset",
    "inset-ring",
    "outline",
    "fill",
    "stroke",
    "shadow",
    "inset-shadow",
    "divide",
    "from",
    "via",
    "to",
    "decoration",
    "accent",
    "caret",
    "placeholder",
];
const SPACING: &[&str] = &[
    "p", "px", "py", "pt", "pr", "pb", "pl", "ps", "pe", "m", "mx", "my", "mt", "mr", "mb", "ml",
    "ms", "me", "gap", "gap-x", "gap-y", "space-x", "space-y", "w", "h", "size", "min-w", "min-h",
    "max-w", "max-h", "inset", "inset-x", "inset-y", "top", "right", "bottom", "left", "basis",
    "scroll-m", "scroll-p",
];
const RADIUS: &[&str] = &[
    "rounded",
    "rounded-t",
    "rounded-r",
    "rounded-b",
    "rounded-l",
    "rounded-s",
    "rounded-e",
    "rounded-tl",
    "rounded-tr",
    "rounded-br",
    "rounded-bl",
];

/// Token namespace -> class prefixes; the longer namespace names come first (`font-weight` before `font`).
const NAMESPACES: &[(&str, &[&str])] = &[
    ("font-weight", &["font"]),
    ("inset-shadow", &["inset-shadow"]),
    ("drop-shadow", &["drop-shadow"]),
    ("color", COLOR),
    ("font", &["font"]),
    ("text", &["text"]),
    ("leading", &["leading"]),
    ("tracking", &["tracking"]),
    ("spacing", SPACING),
    ("radius", RADIUS),
    ("shadow", &["shadow"]),
    ("container", &["max-w", "w", "min-w", "basis"]),
    ("ease", &["ease"]),
    ("animate", &["animate"]),
    ("blur", &["blur"]),
    ("aspect", &["aspect"]),
];

/// Core Tailwind color names (and keywords): a value starting with these is never reported.
const CORE_STEMS: &[&str] = &[
    "slate",
    "gray",
    "zinc",
    "neutral",
    "stone",
    "red",
    "orange",
    "amber",
    "yellow",
    "lime",
    "green",
    "emerald",
    "teal",
    "cyan",
    "sky",
    "blue",
    "indigo",
    "violet",
    "purple",
    "fuchsia",
    "pink",
    "rose",
    "black",
    "white",
    "transparent",
    "current",
    "inherit",
];

/// One theme token and the utility classes it defines.
#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) sheet: String,
    pub(crate) classes: Vec<String>,
    /// The class prefixes of the namespace and the value after it (`brand-500`).
    prefixes: &'static [&'static str],
    value: String,
}

/// The project's `@theme` tokens.
#[derive(Debug, Default)]
pub(crate) struct Theme {
    pub(crate) tokens: Vec<Token>,
}

fn block_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"@theme\b[^{;]*\{").expect("valid regex"));
    &RE
}

fn decl_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:^|[;{\s])(--[A-Za-z0-9_-]+)\s*:").expect("valid regex"));
    &RE
}

fn comment_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").expect("valid regex"));
    &RE
}

/// A token name (`--color-brand-500`) split into its namespace's class prefixes and the value (`brand-500`).
fn split(name: &str) -> Option<(&'static [&'static str], &str)> {
    let rest = name.strip_prefix("--")?;
    for (namespace, prefixes) in NAMESPACES {
        if let Some(value) = rest
            .strip_prefix(namespace)
            .and_then(|tail| tail.strip_prefix('-'))
        {
            // `--text-xl--line-height` is a modifier of `--text-xl`, not a token of its own
            if value.is_empty() || value.contains("--") || value.starts_with('-') {
                return None;
            }
            return Some((prefixes, value));
        }
    }
    None
}

impl Theme {
    pub(crate) fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The `@theme { … }` declarations of a stylesheet; comments are blanked (lines stay).
    pub(super) fn add_sheet(&mut self, rel: &str, text: &str) {
        if !text.contains("@theme") {
            return;
        }
        let blanked = comment_re().replace_all(text, |caps: &regex::Captures<'_>| {
            caps[0]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' })
                .collect::<String>()
        });
        for open in block_re().find_iter(&blanked) {
            let body_start = open.end();
            let mut depth = 1usize;
            let mut end = blanked.len();
            for (at, byte) in blanked.bytes().enumerate().skip(body_start) {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = at;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &blanked[body_start..end];
            for caps in decl_re().captures_iter(body) {
                let Some(name) = caps.get(1) else { continue };
                let Some((prefixes, value)) = split(name.as_str()) else {
                    continue;
                };
                self.tokens.push(Token {
                    sheet: rel.to_owned(),
                    classes: prefixes
                        .iter()
                        .map(|prefix| format!("{prefix}-{value}"))
                        .collect(),
                    prefixes,
                    value: value.to_owned(),
                });
            }
        }
    }

    /// Every utility class the tokens define.
    pub(crate) fn class_names(&self) -> impl Iterator<Item = &str> {
        self.tokens
            .iter()
            .flat_map(|token| token.classes.iter().map(String::as_str))
    }

    pub(crate) fn defines(&self, class: &str) -> bool {
        self.tokens
            .iter()
            .any(|token| token.classes.iter().any(|c| c == class))
    }

    /// Tokens of one stylesheet.
    pub(crate) fn in_sheet<'t>(&'t self, sheet: &'t str) -> impl Iterator<Item = &'t Token> {
        self.tokens.iter().filter(move |token| token.sheet == sheet)
    }

    /// The probe heads (`text-brand-`): a used word starting with one of them is a candidate for the unknown check.
    /// Only tokens with a multi-segment value (`brand-500`) take part.
    pub(crate) fn heads(&self) -> HashSet<String> {
        let mut heads = HashSet::new();
        for token in &self.tokens {
            let Some((stem, _)) = token.value.split_once('-') else {
                continue;
            };
            if CORE_STEMS.contains(&stem) {
                continue;
            }
            for prefix in token.prefixes {
                heads.insert(format!("{prefix}-{stem}-"));
            }
        }
        heads
    }

    /// The defined sibling classes (`text-brand-500`, …) when `word` looks like a theme-token utility (it starts
    /// with one of the `heads`) whose token does not exist; `None` for everything else.
    pub(crate) fn unknown(&self, heads: &HashSet<String>, word: &str) -> Option<Vec<String>> {
        if self.defines(word) {
            return None;
        }
        let head = word
            .match_indices('-')
            .map(|(at, _)| &word[..=at])
            .find(|head| head.len() < word.len() && heads.contains(*head))?;
        let siblings: BTreeSet<&str> = self.class_names().filter(|c| c.starts_with(head)).collect();
        // the nearest shades first (`450` -> `500`, `400`), at most two
        let shade = |class: &str| {
            class
                .rsplit('-')
                .next()
                .and_then(|tail| tail.parse::<i64>().ok())
        };
        let wanted = shade(word);
        let mut ranked: Vec<&str> = siblings.into_iter().collect();
        ranked.sort_by_key(|class| match (wanted, shade(class)) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => i64::MAX,
        });
        Some(ranked.into_iter().take(2).map(str::to_owned).collect())
    }
}
