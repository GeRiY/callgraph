//! PHP tömb-literál statikus beolvasása: egy `return [ … ];` fájl (config, nyelvfájl) kulcs → érték fája. A route-ciklus
//! (`foreach (config('a.b') as $k => $v)`) forrása és a konfig-fájl outline-ja ugyanezt használja. Ami nem literál
//! (összefűzés, függvényhívás, változó), `Value::Expr` — soha nem hiba.

use super::lines::LineIndex;
use super::srcscan::{self, Dialect};
use super::text::collapse_ws;

/// Egy érték a tömbben.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
    /// `X::class` — a név úgy, ahogy a forrásban áll (a vezető `\` nélkül).
    Class(String),
    Array(Vec<Entry>),
    /// Minden más (sorokra tördelés nélkül, egy sorra vonva).
    Expr(String),
}

/// Egy tömbelem: a kulcs (a kulcs nélküli elem sorszáma szövegként), az érték és a sora a fájlban.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: Value,
    pub line: u32,
}

impl Value {
    /// A tömb elemei, ha tömb.
    pub fn entries(&self) -> Option<&[Entry]> {
        match self {
            Self::Array(entries) => Some(entries),
            _ => None,
        }
    }

    /// A sima szöveg-érték (`Str`).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// A tömb `key` kulcsú eleme.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries()?
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| &entry.value)
    }

    /// PHP-forrás-alak (route-argumentumként újraolvasható): a szöveg `'…'`, az osztály `\Név::class`; a `resolve` az
    /// osztálynevet teljessé teszi (a fájl `use`-aival).
    pub fn to_php(&self, resolve: &dyn Fn(&str) -> String) -> String {
        match self {
            Self::Str(text) => format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'")),
            Self::Int(n) => n.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Null => "null".to_owned(),
            Self::Class(name) => format!("\\{}::class", resolve(name)),
            Self::Expr(text) => text.clone(),
            Self::Array(entries) => {
                let items: Vec<String> = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "'{}' => {}",
                            entry.key.replace('\\', "\\\\").replace('\'', "\\'"),
                            entry.value.to_php(resolve)
                        )
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
    }
}

/// A fájl első `return [ … ];` / `return array( … );` tömbje (a szöveg PHP-forrás, megjegyzésekkel); nincs ilyen: `None`.
pub fn parse_return(text: &str) -> Option<Vec<Entry>> {
    let clean = srcscan::strip_comments(text, Dialect::Php);
    let (from, to) = return_array(&clean)?;
    let index = LineIndex::new(&clean);
    Some(entries(&clean, from, to, &index))
}

/// A `return` utáni tömb tartalmának bájt-határai (a zárójelek nélkül).
fn return_array(clean: &str) -> Option<(usize, usize)> {
    let bytes = clean.as_bytes();
    for (at, _) in clean.match_indices("return") {
        let before_ok = at == 0 || !super::text::is_ident_byte(bytes[at - 1]);
        let after = at + "return".len();
        if !before_ok
            || bytes
                .get(after)
                .is_some_and(|b| super::text::is_ident_byte(*b))
            || brace_depth(&clean[..at]) != 0
        {
            continue;
        }
        let mut open = skip_ws(bytes, after);
        let rest = clean.get(open..)?;
        if starts_array(rest) {
            let paren = skip_ws(bytes, open + 5);
            if bytes.get(paren) == Some(&b'(') {
                open = paren;
            } else {
                continue;
            }
        } else if bytes.get(open) != Some(&b'[') {
            continue;
        }
        let close = srcscan::matching(clean, open, clean.len(), Dialect::Php)?;
        return Some((open + 1, close));
    }
    None
}

/// A szöveg végén nyitott `{` blokkok száma (a stringeken belüli kapcsos zárójel nem számít): a fájl legfelső szintű
/// `return`-je 0 mélységen áll, egy closure-é nem.
fn brace_depth(text: &str) -> usize {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for &b in text.as_bytes() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'"' => quote = Some(b),
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// `array` kulcsszóval kezdődik-e (kis- / nagybetűtől függetlenül).
fn starts_array(text: &str) -> bool {
    text.as_bytes()
        .get(..5)
        .is_some_and(|head| head.eq_ignore_ascii_case(b"array"))
}

fn skip_ws(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// A `from..to` tömbtartalom elemei.
fn entries(text: &str, from: usize, to: usize, index: &LineIndex) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut next: i64 = 0;
    for (a, b) in srcscan::split_args(text, from, to, Dialect::Php) {
        let Some(item) = text.get(a..b) else {
            continue;
        };
        if item.starts_with("...") {
            continue;
        }
        let parts = srcscan::split_top(item, "=>", Dialect::Php);
        let (key, value_from) = if parts.len() >= 2 {
            let key = key_of(parts[0].trim());
            if let Ok(n) = key.parse::<i64>() {
                next = next.max(n + 1);
            }
            (key, a + parts[0].len() + 2)
        } else {
            let key = next.to_string();
            next += 1;
            (key, a)
        };
        out.push(Entry {
            key,
            value: value(text, value_from, b, index),
            line: index.line_of(a),
        });
    }
    out
}

/// A kulcs szövege: literál string / szám / `X::class`, más: a kifejezés egy sorra vonva.
fn key_of(raw: &str) -> String {
    match literal(raw) {
        Some(Value::Str(text)) => text,
        Some(Value::Int(n)) => n.to_string(),
        Some(Value::Class(name)) => name,
        _ => collapse_ws(raw),
    }
}

/// Egy érték a `from..to` tartományból.
fn value(text: &str, from: usize, to: usize, index: &LineIndex) -> Value {
    let Some(raw) = text.get(from..to) else {
        return Value::Null;
    };
    let t = raw.trim();
    let start = from + (raw.len() - raw.trim_start().len());
    let end = start + t.len();
    let array_open = if t.starts_with('[') {
        Some(start)
    } else if starts_array(t) {
        let paren = skip_ws(text.as_bytes(), start + 5);
        (text.as_bytes().get(paren) == Some(&b'(')).then_some(paren)
    } else {
        None
    };
    if let Some(open) = array_open
        && srcscan::matching(text, open, end, Dialect::Php) == Some(end - 1)
    {
        return Value::Array(entries(text, open + 1, end - 1, index));
    }
    literal(t).unwrap_or_else(|| Value::Expr(collapse_ws(t)))
}

/// Skalár literál: string (interpoláció nélkül), egész, `true` / `false` / `null`, `X::class`.
fn literal(t: &str) -> Option<Value> {
    let t = t.trim();
    if let Some(quote) = t.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
        if quote == '"' && (inner.contains('$') || inner.contains('{')) {
            return None;
        }
        // egy idézőjel a szövegen belül csak védve (`\'`) lehet; a `'a' . 'b'` összefűzés nem literál
        let mut text = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(n) if n == quote || n == '\\' => text.push(n),
                    Some(n) => {
                        text.push('\\');
                        text.push(n);
                    }
                    None => text.push('\\'),
                }
            } else if c == quote {
                return None;
            } else {
                text.push(c);
            }
        }
        return Some(Value::Str(text));
    }
    match t.to_ascii_lowercase().as_str() {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        "null" => return Some(Value::Null),
        _ => {}
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some(Value::Int(n));
    }
    let name = t.strip_suffix("::class")?;
    let name = name.trim_start_matches('\\');
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '\\'))
    .then(|| Value::Class(name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_literals_with_lines() {
        let text = "<?php\n// c\nreturn [\n  'a' => ['x' => 1, 'y' => 'b'],\n  'c' => Foo\\Bar::class,\n  'd',\n  'e' => 'p' . $q,\n];\n";
        let entries = parse_return(text).unwrap();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["a", "c", "0", "e"]);
        assert_eq!(entries[0].line, 4);
        assert_eq!(entries[0].value.get("y").unwrap().as_str(), Some("b"));
        assert_eq!(entries[1].value, Value::Class("Foo\\Bar".to_owned()));
        assert!(matches!(entries[3].value, Value::Expr(_)));
    }

    #[test]
    fn php_text_round_trips_through_to_php() {
        let entries =
            parse_return("<?php return ['k' => ['u' => '/x', 'c' => A::class]];").unwrap();
        let text = entries[0].value.to_php(&|name| format!("App\\{name}"));
        assert_eq!(text, "['u' => '/x', 'c' => \\App\\A::class]");
    }

    #[test]
    fn no_return_array_is_none() {
        assert!(parse_return("<?php echo 1;").is_none());
        assert!(parse_return("<?php return $x;").is_none());
    }
}
