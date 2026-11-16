//! A migrációk szöveg-oszlopai a `str` mező hosszkorlátjához: `text` / `mediumText` / `longText` oszlopra nem kell
//! `max:255` (különben a validáció levágná a hosszú szöveget), a `string('x', 100)` pedig `max:100`. Szándékosan
//! egyszerű, regex nélküli szöveg-átvizsgálás: ami nem egyértelmű, az alapértelmezett (`max:255`) marad.

use std::collections::BTreeMap;

/// Egy `str` mező hosszkorlátja a migráció szerint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrLimit {
    /// Nincs adat (vagy `string('x')`): `max:255`.
    #[default]
    Default,
    /// Hosszú szöveg-oszlop (`text` / `mediumText` / `longText`): nincs `max`.
    Unlimited,
    /// `string('x', N)`: `max:N`.
    Len(u32),
}

/// Az oszlopnév → hosszkorlát térkép a tábla migrációiból (a későbbi migráció felülírja a korábbit).
pub fn limits(migrations: &[String], table: &str) -> BTreeMap<String, StrLimit> {
    let mut found = BTreeMap::new();
    for text in migrations {
        for block in schema_blocks(text, table) {
            columns_of(block, &mut found);
        }
    }
    found
}

/// A `Schema::create('<tábla>'` / `Schema::table('<tábla>'` blokkok szövege (a következő `Schema::` hívásig).
fn schema_blocks<'t>(text: &'t str, table: &str) -> Vec<&'t str> {
    let starts: Vec<usize> = text.match_indices("Schema::").map(|(i, _)| i).collect();
    let mut blocks = Vec::new();
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(text.len());
        let block = &text[start..end];
        let Some(open) = block.find('(') else {
            continue;
        };
        if first_string(&block[open + 1..]).is_some_and(|(name, _)| name == table) {
            blocks.push(block);
        }
    }
    blocks
}

/// A szöveg elején álló idézőjeles string és a utána maradó rész.
fn first_string(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let quote = text.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = &text[1..];
    let close = inner.find(quote)?;
    Some((&inner[..close], &inner[close + 1..]))
}

fn columns_of(block: &str, found: &mut BTreeMap<String, StrLimit>) {
    for piece in block.split("->").skip(1) {
        let Some(open) = piece.find('(') else {
            continue;
        };
        let kind = piece[..open].trim();
        let is_long = matches!(kind, "text" | "mediumText" | "longText");
        if !is_long && kind != "string" {
            continue;
        }
        let Some((name, rest)) = first_string(&piece[open + 1..]) else {
            continue;
        };
        let limit = if is_long {
            StrLimit::Unlimited
        } else {
            // `string('x', 100)`: a vessző után álló szám
            rest.trim_start()
                .strip_prefix(',')
                .map(|r| {
                    r.trim_start()
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                })
                .and_then(|digits| digits.parse::<u32>().ok())
                .map_or(StrLimit::Default, StrLimit::Len)
        };
        found.insert(name.to_owned(), limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIGRATION: &str = "Schema::create('posts', function (Blueprint $table) {\n    $table->id();\n    $table->string('title');\n    $table->string('slug', 100);\n    $table->text('body');\n    $table->longText('html')->nullable();\n});\nSchema::create('other', function (Blueprint $table) {\n    $table->text('title');\n});";

    #[test]
    fn text_columns_are_unlimited_and_string_length_is_read() {
        let found = limits(&[MIGRATION.to_owned()], "posts");
        assert_eq!(found.get("body"), Some(&StrLimit::Unlimited));
        assert_eq!(found.get("html"), Some(&StrLimit::Unlimited));
        assert_eq!(found.get("slug"), Some(&StrLimit::Len(100)));
        assert_eq!(found.get("title"), Some(&StrLimit::Default));
        assert!(!found.contains_key("id"));
    }
}
