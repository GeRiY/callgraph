//! A JSON-értékek outline-alakja és a kulcs-út (`scripts.build`, `files.0`) forrásbeli sortartománya.

use jsonc_parser::{CollectOptions, ParseOptions, ast::Value, common::Ranged, parse_to_ast};

/// Ennél hosszabb string-érték az outline-ban `…`-ra vágva (karakterben).
const MAX_SHOWN: usize = 60;

/// Az outline-ban mutatott érték: a rövid skalár maga (a string idézőjelben, `MAX_SHOWN` karakter fölött `…`-ra
/// vágva), az objektum és a tömb a fajtája és az elemszáma (`object (3)`, `array (2)`).
pub(super) fn display(value: &Value<'_>) -> String {
    match value {
        Value::StringLit(text) => {
            let text = text.value.as_ref();
            if text.chars().count() > MAX_SHOWN {
                let cut: String = text.chars().take(MAX_SHOWN).collect();
                format!("\"{cut}…\"")
            } else {
                format!("\"{text}\"")
            }
        }
        Value::NumberLit(number) => number.value.to_owned(),
        Value::BooleanLit(boolean) => boolean.value.to_string(),
        Value::NullKeyword(_) => "null".into(),
        Value::Object(object) => format!("object ({})", object.properties.len()),
        Value::Array(array) => format!("array ({})", array.elements.len()),
    }
}

/// A kulcs-út (`a.b.c`; tömb-elem az indexével: `files.0`) sortartománya a forrásban: a kulcs sorától az értéke
/// végéig (tömb-elemnél az elem soraival). A pontot tartalmazó kulcs is illeszkedik (`exports../src`), a hosszabb
/// egyezés nyer. Hibás JSON-nál vagy ismeretlen útnál `None`.
pub(crate) fn key_lines(text: &str, path: &str) -> Option<(u32, u32)> {
    let options = (CollectOptions::default(), ParseOptions::default());
    let parsed = parse_to_ast(text, &options.0, &options.1).ok()?;
    let (start, end) = find(parsed.value.as_ref()?, path)?;
    let last = end.saturating_sub(1).max(start);
    Some((line(text, start), line(text, last)))
}

/// A kulcs-út bájt-tartománya (a kulcs elejétől az érték végéig).
fn find(value: &Value<'_>, path: &str) -> Option<(usize, usize)> {
    match value {
        Value::Object(object) => {
            let mut props: Vec<_> = object.properties.iter().collect();
            // a hosszabb kulcs előbb: a pontot tartalmazó kulcs ne essen szét
            props.sort_by_key(|prop| std::cmp::Reverse(prop.name.as_str().len()));
            props.into_iter().find_map(|prop| {
                let key = prop.name.as_str();
                if path == key {
                    return Some((prop.start(), prop.end()));
                }
                let rest = path.strip_prefix(key)?.strip_prefix('.')?;
                find(&prop.value, rest)
            })
        }
        Value::Array(array) => {
            let (index, rest) = match path.split_once('.') {
                Some((index, rest)) => (index, Some(rest)),
                None => (path, None),
            };
            let element = array.elements.get(index.parse::<usize>().ok()?)?;
            match rest {
                None => Some((element.start(), element.end())),
                Some(rest) => find(element, rest),
            }
        }
        _ => None,
    }
}

fn line(text: &str, offset: usize) -> u32 {
    let before = &text.as_bytes()[..offset.min(text.len())];
    (before.iter().filter(|&&b| b == b'\n').count() + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::key_lines;
    use crate::core::SourceFile;
    use crate::lang::Language;
    use crate::lang::json::JsonDocument;

    const PACKAGE: &str = r#"{
    "private": true,
    "scripts": {
        "build": "vite build",
        "dev": "vite"
    },
    "devDependencies": {
        "laravel-vite-plugin": "^1.0"
    },
    "files": [
        "dist",
        { "name": "x" }
    ],
    "exports": { "./src": "./src/index.js" },
    "description": "A very long description string that goes on and on beyond sixty characters for sure",
    "version": 3,
    "nothing": null
}
"#;

    fn values(text: &str) -> Vec<(String, String)> {
        let file = SourceFile::new("package.json", text.as_bytes().to_vec(), None);
        JsonDocument
            .analyze(&file)
            .unwrap()
            .graph
            .constants
            .into_iter()
            .map(|c| (c.name, c.value.unwrap_or_default()))
            .collect()
    }

    #[test]
    fn short_scalars_are_shown_long_strings_cut_containers_counted() {
        let values = values(PACKAGE);
        let value = |name: &str| {
            values
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
                .unwrap_or_else(|| panic!("{name}: {values:?}"))
        };
        assert_eq!(value("private"), "true");
        assert_eq!(value("scripts"), "object (2)");
        assert_eq!(value("scripts.build"), "\"vite build\"");
        assert_eq!(value("devDependencies.laravel-vite-plugin"), "\"^1.0\"");
        assert_eq!(value("files"), "array (2)");
        assert_eq!(value("version"), "3");
        assert_eq!(value("nothing"), "null");
        assert_eq!(
            value("description"),
            "\"A very long description string that goes on and on beyond si…\""
        );
    }

    #[test]
    fn key_path_resolves_to_the_exact_source_lines() {
        assert_eq!(key_lines(PACKAGE, "scripts"), Some((3, 6)));
        assert_eq!(key_lines(PACKAGE, "scripts.build"), Some((4, 4)));
        assert_eq!(key_lines(PACKAGE, "devDependencies"), Some((7, 9)));
        assert_eq!(key_lines(PACKAGE, "files.0"), Some((11, 11)));
        assert_eq!(key_lines(PACKAGE, "files.1"), Some((12, 12)));
        assert_eq!(key_lines(PACKAGE, "files.1.name"), Some((12, 12)));
        // a pontot tartalmazó kulcs is
        assert_eq!(key_lines(PACKAGE, "exports../src"), Some((14, 14)));
        assert_eq!(key_lines(PACKAGE, "scripts.nope"), None);
        assert_eq!(key_lines(PACKAGE, "files.9"), None);
        assert_eq!(key_lines("{ broken", "a"), None);
    }
}
