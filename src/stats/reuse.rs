//! „Az előző kimenetből jött-e a következő hívás?” (v0.9.5): egy mért hívás célfájlja és a kért szimbóluma benne
//! volt-e ugyanabban a munkamenetben az ELŐZŐ hívás kimenetében. Kifelé csak a sáv megy: `use_file` = `hit1` (az
//! első felsorolt fájl), `hit5` (2.–5.), `hit` (6.+), `miss` (nem volt benne), `-` (nincs fájl-cél, nincs előző
//! hívás, vagy az előző hibás volt / nem volt kimenete); `use_sym` = `hit` / `miss` / `-`.
//!
//! A kimenet elemei (fájl-utak első előfordulási sorrendben, azonosítók) CSAK a memóriában élnek, a kimenet első
//! 64 KiB-jából; a helyi `session.json`-ba kulcsolt, rövidített hash-ként kerülnek (`HMAC-SHA256` a
//! telepítésenkénti kulccsal, `hmac.rs`, az első 16 hexa jegy), és kizárólag a fenti sávok kiszámításához kellenek.
//! Út, név, keresési kifejezés vagy ezek hash-e a napi összesítőbe, az eseménynaplóba és a beküldésbe SOHA nem kerül.

use std::collections::HashSet;

use super::event::{Client, CommandKind, Outcome};
use super::hmac::keyed_hex;
use super::session::SessionState;
use super::target::{Call, Target};

/// Ennyi bájtot vizsgál legfeljebb egy kimenet elejéből (a CLI is csak ennyit tart meg, `Meter`).
pub(crate) const SAMPLE_BYTES: usize = 64 * 1024;
/// Legfeljebb ennyi különböző fájl-elemet tart egy kimenetből.
pub(crate) const MAX_FILE_ITEMS: usize = 200;
/// Legfeljebb ennyi különböző azonosítót tart egy kimenetből.
pub(crate) const MAX_SYMBOL_ITEMS: usize = 1500;
/// Egy hívás legfeljebb ennyi kért szimbólumát veti össze.
pub(crate) const MAX_QUERY_SYMBOLS: usize = 16;
/// Az út-utótag legfeljebb ennyi komponensből áll (a tárolt fájl-elem is ennyire rövidül).
const MAX_SUFFIX: usize = 8;
/// A tárolt hash hossza (hexa jegy).
const SHORT_HEX: usize = 16;
/// Egy azonosító legalább ennyi karakter.
const MIN_IDENT: usize = 3;

/// A fájl-sávok a riport sorrendjében (`-` nélkül).
pub(crate) const FILE_BANDS: [&str; 4] = ["hit1", "hit5", "hit", "miss"];
/// A szimbólum-sávok a riport sorrendjében (`-` nélkül).
pub(crate) const SYM_BANDS: [&str; 2] = ["hit", "miss"];
/// Minden sáv-név (a tesztek és az adatvédelmi ellenőrzés szótára).
#[cfg(test)]
pub(crate) const USE_BANDS: [&str; 5] = ["hit1", "hit5", "hit", "miss", "-"];

/// Egy kimenet elemei (csak a memóriában): fájl-utak első előfordulási sorrendben és azonosítók.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OutputItems {
    pub(crate) files: Vec<String>,
    pub(crate) symbols: Vec<String>,
}

impl OutputItems {
    /// A kimenet szövegéből (legfeljebb az első `SAMPLE_BYTES` bájtból); üres kimenetnél `None`.
    pub(crate) fn from_text(text: &str) -> Option<Self> {
        let text = prefix(text, SAMPLE_BYTES);
        if text.trim().is_empty() {
            return None;
        }
        Some(Self {
            files: file_items(text),
            symbols: symbol_items(text, MAX_SYMBOL_ITEMS),
        })
    }
}

/// A szöveg legfeljebb `max` bájtos eleje, karakterhatáron.
fn prefix(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A célfájl helye az előző kimenetben.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileUse {
    Hit1,
    Hit5,
    Hit,
    Miss,
    None,
}

impl FileUse {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Hit1 => "hit1",
            Self::Hit5 => "hit5",
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::None => "-",
        }
    }

    /// A 0-tól számolt hely sávja.
    fn from_position(index: usize) -> Self {
        match index {
            0 => Self::Hit1,
            1..=4 => Self::Hit5,
            _ => Self::Hit,
        }
    }
}

/// A kért szimbólum benne volt-e az előző kimenetben.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SymUse {
    Hit,
    Miss,
    None,
}

impl SymUse {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::None => "-",
        }
    }
}

/// Egy hívás két sávja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Uses {
    pub(crate) file: FileUse,
    pub(crate) sym: SymUse,
}

impl Uses {
    /// Nincs mivel összevetni.
    pub(crate) const NONE: Self = Self {
        file: FileUse::None,
        sym: SymUse::None,
    };
}

/// A napi `uses` kulcsa: `{előző}[!]>{ez}|f:{sáv}|s:{sáv}` (az előző neve úgy, mint az átmenet-éleknél).
pub(crate) fn use_key(from: &str, to: CommandKind, uses: Uses) -> String {
    format!(
        "{from}>{}|f:{}|s:{}",
        to.name(),
        uses.file.name(),
        uses.sym.name()
    )
}

/// A fájl-elemek: útnak látszó szavak (kiterjesztéssel: `.` után 1–6 betű/szám, legalább egy betűvel), a záró
/// `:12` / `:L12` / `::metódus` és írásjelek nélkül, `/` elválasztóval, `./` előtag nélkül, legfeljebb az utolsó 8
/// komponensre rövidítve; különbözők, első előfordulási sorrendben, legfeljebb `MAX_FILE_ITEMS`.
pub(crate) fn file_items(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for token in text.split(is_file_separator) {
        if items.len() >= MAX_FILE_ITEMS {
            break;
        }
        if let Some(item) = file_item(token)
            && seen.insert(item.clone())
        {
            items.push(item);
        }
    }
    items
}

/// A szavak határa (a `<` / `>` nem: a `->` tag-hozzáférést a szó egészében kell felismerni).
fn is_file_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '|' | '='
        )
}

/// Egy szó, ha útnak látszik (egységes alakban).
fn file_item(token: &str) -> Option<String> {
    let token = token.split_once("::").map_or(token, |(path, _)| path);
    let path = token.replace('\\', "/");
    let bytes = path.as_bytes();
    // `C:/…`: a meghajtó-betű kettőspontja még az út része, a többi (`:12`, `:L12`) már nem
    let skip = if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        2
    } else {
        0
    };
    let end = path[skip..].find(':').map_or(path.len(), |i| i + skip);
    let path = path[..end].trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    if path.contains("->") {
        return None;
    }
    let name = path.rsplit('/').next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    let ext_ok = (1..=6).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        && ext.bytes().any(|b| b.is_ascii_alphabetic());
    if !ext_ok || !stem.chars().any(char::is_alphanumeric) {
        return None;
    }
    let tail = tail(&components(path));
    (!tail.is_empty()).then_some(tail)
}

/// Egy út komponensei (`/` vagy `\` elválasztó; az üres, `.` és `..` komponensek nélkül).
fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect()
}

/// Az utolsó `MAX_SUFFIX` komponens `/`-rel összefűzve.
fn tail(parts: &[&str]) -> String {
    parts[parts.len().saturating_sub(MAX_SUFFIX)..].join("/")
}

/// Az azonosítók (`[A-Za-z_][A-Za-z0-9_]{2,}`; a `$`, `::`, `.`, `->` elválaszt, így a `Class::method` és az
/// `$obj->prop` mindkét része külön elem): különbözők, első előfordulási sorrendben, legfeljebb `cap`.
pub(crate) fn symbol_items(text: &str, cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for word in identifiers(text) {
        if items.len() >= cap {
            break;
        }
        if seen.insert(word) {
            items.push(word.to_owned());
        }
    }
    items
}

fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|w| w.trim_start_matches(|c: char| c.is_ascii_digit()))
        .filter(|w| w.len() >= MIN_IDENT)
}

/// Egy kért szimbólum-szöveg (metódusnév, `Osztály::metódus`, hívó-minta) azonosítói.
pub(crate) fn query_symbols(text: &str) -> Vec<String> {
    symbol_items(text, MAX_QUERY_SYMBOLS)
}

/// Egy szabad szöveges keresés (`--const`, `search`) csak akkor szimbólum, ha egyetlen azonosító.
pub(crate) fn single_identifier(text: &str) -> Option<String> {
    let text = text.trim();
    let mut words = identifiers(text);
    let word = words.next()?;
    (word == text && words.next().is_none()).then(|| word.to_owned())
}

/// Egy fájl-elem rövidített kulcsolt hash-e.
pub(crate) fn file_hash(key: &[u8], item: &str) -> String {
    short(keyed_hex(key, &format!("o:{item}")))
}

/// Egy azonosító rövidített kulcsolt hash-e.
pub(crate) fn symbol_hash(key: &[u8], item: &str) -> String {
    short(keyed_hex(key, &format!("s:{item}")))
}

fn short(mut hex: String) -> String {
    hex.truncate(SHORT_HEX);
    hex
}

/// Különböző hash-ek az elemek sorrendjében.
fn hashed(items: &[String], hash: impl Fn(&str) -> String) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|item| hash(item.as_str()))
        .filter(|h| seen.insert(h.clone()))
        .collect()
}

/// A cél fájl-útja (a `Target::from_text` célnál a projekt-alias utáni relatív út).
fn target_path(target: &Target) -> Option<&str> {
    let file = target.file.as_deref()?;
    Some(file.split_once('\u{0}').map_or(file, |(_, rel)| rel))
}

/// A célfájl az előző kimenet fájl-elemei között: az útja utolsó 1..=8 komponensének (út-utótag
/// komponens-határon) bármelyike egyezik egy elemmel; a legjobb (legkisebb) hely számít.
pub(crate) fn file_use(key: &[u8], out_files: &[String], target: Option<&Target>) -> FileUse {
    let Some(path) = target.and_then(target_path) else {
        return FileUse::None;
    };
    let parts = components(path);
    (1..=parts.len().min(MAX_SUFFIX))
        .filter_map(|n| {
            let suffix = parts[parts.len() - n..].join("/");
            let hash = file_hash(key, &suffix);
            out_files.iter().position(|f| *f == hash)
        })
        .min()
        .map_or(FileUse::Miss, FileUse::from_position)
}

/// A kért szimbólumok bármelyike benne van-e az előző kimenet azonosítói között.
pub(crate) fn symbol_use(key: &[u8], out_syms: &[String], symbols: &[String]) -> SymUse {
    if symbols.is_empty() {
        return SymUse::None;
    }
    if symbols
        .iter()
        .any(|s| out_syms.contains(&symbol_hash(key, s)))
    {
        SymUse::Hit
    } else {
        SymUse::Miss
    }
}

/// Egy hívás összevetése az előző hívás tárolt kimenet-elemeivel, majd a tárolt elemek cseréje erre a hívásra
/// (hibás vagy üres kimenetnél üresre: a következő hívás `-`-t kap). A munkamenet-határt a `SessionState::advance`
/// már kezelte (új munkamenetben nincs tárolt kimenet). A hook háttér-frissítése nem számít, és a tárolt elemekhez
/// sem nyúl.
pub(crate) fn step(state: &mut SessionState, key: &[u8], call: &Call) -> Uses {
    if call.event.client == Client::Hook {
        return Uses::NONE;
    }
    let uses = if state.has_out {
        Uses {
            file: file_use(key, &state.out_files, call.target.as_ref()),
            sym: symbol_use(key, &state.out_syms, &call.symbols),
        }
    } else {
        Uses::NONE
    };
    let output = call
        .output
        .as_ref()
        .filter(|_| call.event.outcome == Outcome::Ok);
    state.has_out = output.is_some();
    state.out_files = output.map_or_else(Vec::new, |o| hashed(&o.files, |f| file_hash(key, f)));
    state.out_syms = output.map_or_else(Vec::new, |o| hashed(&o.symbols, |s| symbol_hash(key, s)));
    uses
}
