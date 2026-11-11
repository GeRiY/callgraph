//! „ReAct-barát” helyreállító üzenetek: üres vagy hibás eredménynél megmondják az agentnek a következő lépést
//! (hasonló nevek, javasolt minta, mit érdemes másképp kérni). Csak a szöveges kimenetbe kerülnek (outline, grep,
//! locate, hibaüzenet); a gépi formátumokat (json, mermaid, dot, stats) és a `next` javaslat-sorokat nem érintik.
//!
//! Minden generátor tiszta függvény, a nyelvet explicit kapja (mint a formázók), így mindkét nyelv tesztelhető.

use crate::i18n::Lang;
use crate::model::FileGraph;
use regex::Regex;
use std::collections::BTreeSet;

/// Legfeljebb ennyi „erre gondoltál” javaslat.
pub(crate) const MAX_SUGGESTIONS: usize = 3;

/// A név szerinti hívó-minta eleje (a skill ajánlott alakja): `(^|::|\.|->)név$`.
const NAME_PATTERN_PREFIX: &str = r"(^|::|\.|->)";

/// Két szöveg Levenshtein-távolsága (karakterenként: beszúrás, törlés, csere = 1).
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.iter().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(i + 1);
        for (j, right) in b.iter().enumerate() {
            let cost = usize::from(left != right);
            let best = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
            current.push(best);
        }
        previous = current;
    }
    previous[b.len()]
}

/// A megengedett edit-távolság: rövid (legfeljebb 3 karakteres) névnél 1, különben 2 (a 2 karakteres névnél a
/// 2-es távolság már „bármi” lenne).
fn max_distance(chars: usize) -> usize {
    if chars <= 3 { 1 } else { 2 }
}

/// Előtag-egyezés valamelyik irányban; legalább 3 karakteres rövidebb oldallal (a „fi” ne illeszkedjen mindenre).
fn is_prefix_pair(a: &str, b: &str) -> bool {
    a.chars().count().min(b.chars().count()) >= 3 && (a.starts_with(b) || b.starts_with(a))
}

/// A `target`-hez leghasonlóbb elemek (legfeljebb `limit`). Az elem `(kulcs, kiírt alak)`: a kulcshoz hasonlítunk,
/// a kiírt alakot adjuk vissza. Sorrend: csak kis-/nagybetűben eltérő, majd edit-távolság szerint (1, 2), majd
/// előtag-egyezés; azonos rangon a hosszkülönbség, végül az ábécé dönt. A teljesen azonos kulcs kimarad.
pub(crate) fn closest<'a>(
    target: &str,
    items: impl IntoIterator<Item = (&'a str, &'a str)>,
    limit: usize,
) -> Vec<String> {
    let wanted = target.to_lowercase();
    let wanted_len = wanted.chars().count();
    let max = max_distance(wanted_len);
    let mut scored: Vec<(usize, usize, &str)> = Vec::new();
    for (key, shown) in items {
        if key == target {
            continue;
        }
        let lower = key.to_lowercase();
        let gap = lower.chars().count().abs_diff(wanted_len);
        let distance = if lower == wanted {
            0
        } else if gap <= max {
            edit_distance(&lower, &wanted)
        } else {
            usize::MAX
        };
        let rank = if distance <= max {
            distance
        } else if is_prefix_pair(&lower, &wanted) {
            max + 1
        } else {
            continue;
        };
        scored.push((rank, gap, shown));
    }
    scored.sort_unstable();
    let mut out: Vec<String> = Vec::new();
    for (_, _, shown) in scored {
        if out.len() == limit {
            break;
        }
        if !out.iter().any(|seen| seen == shown) {
            out.push(shown.to_owned());
        }
    }
    out
}

/// Egyszerű javaslat-lista egy szavas nevekhez (metódus-, kategória-nevek): a kulcs maga a kiírt alak.
pub(crate) fn closest_names<'a>(
    target: &str,
    names: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<String> {
    closest(target, names.into_iter().map(|name| (name, name)), limit)
}

/// `Did you mean: a, b` sor (üres listánál üres szöveg, sortörés nélkül).
pub(crate) fn did_you_mean(lang: Lang, suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        return String::new();
    }
    let list = suggestions.join(", ");
    ltf!(
        lang,
        "Did you mean: {list}",
        "Esetleg erre gondoltál: {list}"
    )
}

/// A hívás-cél rövid alakja a hasonlításhoz: a névtér és a hosszú út nélkül, az utolsó két tag (`Típus::metódus`);
/// a `->` és a `.` is `::`-nak számít, a `?` (feloldatlan fogadó) tag kimarad.
pub(crate) fn short(symbol: &str) -> String {
    let unified = symbol.replace("->", "::").replace('.', "::");
    let parts: Vec<&str> = unified
        .split("::")
        .map(|part| part.rsplit('\\').next().unwrap_or(part))
        .filter(|part| !part.is_empty() && *part != "?")
        .collect();
    match parts.as_slice() {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [.., owner, name] => format!("{owner}::{name}"),
    }
}

/// A `--grep` minta egyszerű (nem reguláris) azonosítóként, ha az: a név szerinti minta elejét (`(^|::|\.|->)`),
/// a `^` / `$` horgonyokat és a `\.` / `\\` escape-eket levéve csak betű, szám, `_`, `:`, `.`, `\`, `-`, `>`
/// maradhat. Bármi más (karakterosztály, csoport, számláló…) → `None`.
pub(crate) fn plain_symbol(pattern: &str) -> Option<String> {
    let mut text = pattern.trim();
    text = text.strip_prefix(NAME_PATTERN_PREFIX).unwrap_or(text);
    text = text.strip_prefix('^').unwrap_or(text);
    text = text.strip_suffix('$').unwrap_or(text);
    let text = text.replace(r"\\", "\\").replace(r"\.", ".");
    if text.is_empty()
        || !text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.' | '\\' | '-' | '>'))
    {
        return None;
    }
    Some(short(&text)).filter(|plain| !plain.is_empty())
}

/// Név szerinti minta az azonosító utolsó tagjából (`Usr::find` → `(^|::|\.|->)find$`); `None`, ha a név nem
/// egyszerű azonosító.
pub(crate) fn name_pattern(plain: &str) -> Option<String> {
    let name = plain.rsplit("::").next()?;
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then(|| format!("{NAME_PATTERN_PREFIX}{name}$"))
}

/// Egy hívás-cél alkalmas-e javaslatnak (rövid, azonosító-szerű; a kifejezés-szövegek kimaradnak).
fn plausible(target: &str) -> bool {
    !target.is_empty()
        && target.chars().count() <= 80
        && target
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.' | '\\' | '>' | '-' | '?'))
}

/// A gráfban szereplő nevek: a definíciók (típus, metódus, függvény) és a hívás-célok, a „van-e ilyen” és a
/// „erre gondoltál” kérdésekhez.
#[derive(Debug, Default)]
pub(crate) struct Catalog {
    /// A definíciók minden alakban: `név`, `Típus::név`, `Névtér\Típus::név`, `Típus`.
    defined: BTreeSet<String>,
    /// A hívás-célok, ahogy a gráfban állnak (`resolved` vagy `target`).
    called: BTreeSet<String>,
    /// A javaslatokhoz: a definíciók és a hívás-célok rövid alakja (`Típus::metódus`).
    symbols: BTreeSet<String>,
}

/// Egy név jelenléte a gráfban (a `--depth` üzenetéhez).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    /// Sem definíció, sem hívás-cél.
    Missing,
    /// Definiált, de a vizsgált hatókörben senki sem hívja, máshol sem.
    DefinedOnly,
    /// Máshol (a hatókörön kívül) hívják.
    CalledElsewhere,
}

impl Catalog {
    pub(crate) fn from_graphs<'a>(graphs: impl IntoIterator<Item = &'a FileGraph>) -> Self {
        let mut catalog = Self::default();
        for graph in graphs {
            catalog.add(graph);
        }
        catalog
    }

    /// Egy fájl gráfjának nevei.
    pub(crate) fn add(&mut self, graph: &FileGraph) {
        let namespace = graph.namespace.as_deref().filter(|ns| !ns.is_empty());
        for container in &graph.containers {
            self.define(namespace, None, &container.name);
        }
        for method in &graph.methods {
            let owner = method.container.as_deref().filter(|c| !c.is_empty());
            self.define(namespace, owner, &method.name);
        }
        for call in graph.calls.iter().filter(|call| !call.is_dynamic()) {
            let target = call.resolved.as_deref().unwrap_or(&call.target);
            if plausible(target) {
                self.called.insert(target.to_owned());
                let symbol = short(target);
                if !symbol.is_empty() {
                    self.symbols.insert(symbol);
                }
            }
        }
    }

    fn define(&mut self, namespace: Option<&str>, owner: Option<&str>, name: &str) {
        if name.is_empty() {
            return;
        }
        let plain = match owner {
            Some(owner) => format!("{owner}::{name}"),
            None => name.to_owned(),
        };
        if let Some(namespace) = namespace {
            self.defined.insert(format!("{namespace}\\{plain}"));
        }
        if owner.is_some() {
            self.defined.insert(name.to_owned());
        }
        let symbol = short(&plain);
        self.defined.insert(plain);
        if !symbol.is_empty() {
            self.symbols.insert(symbol);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// A mintára illeszkedő név jelenléte: hívják (máshol), csak definiált, vagy sehol.
    pub(crate) fn presence(&self, regex: &Regex) -> Presence {
        if self.called.iter().any(|name| regex.is_match(name)) {
            Presence::CalledElsewhere
        } else if self.defined.iter().any(|name| regex.is_match(name)) {
            Presence::DefinedOnly
        } else {
            Presence::Missing
        }
    }

    /// A `plain` azonosítóhoz hasonló nevek (`Típus::metódus` alakban): minősített azonosítónál a teljes
    /// `Típus::metódus`-hoz, névnél a metódus-/típusnévhez hasonlít.
    pub(crate) fn suggest(&self, plain: &str) -> Vec<String> {
        let wanted = short(plain);
        let qualified = wanted.contains("::");
        closest(
            &wanted,
            self.symbols.iter().map(|symbol| {
                let key = if qualified {
                    symbol.as_str()
                } else {
                    symbol.rsplit("::").next().unwrap_or(symbol)
                };
                (key, symbol.as_str())
            }),
            MAX_SUGGESTIONS,
        )
    }
}

/// `--grep` nulla találatnál: mit jelent a minta, név szerinti minta, hasonló nevek. A generikus „törzs-logika”
/// tipp ilyenkor nem jelenik meg (nincs találat, amire vonatkozna).
pub(crate) fn empty_grep(lang: Lang, pattern: &str, catalog: &Catalog) -> String {
    let mut out = ltf!(
        lang,
        "ℹ no call target matches {pattern}\n",
        "ℹ egyetlen hívás-cél sem illeszkedik a mintára: {pattern}\n"
    );
    out.push_str(lang.pick(
        "💡 the pattern is a regular expression on the call TARGET (case-insensitive): (^|::|\\.|->)name$ finds every call by name; --guess also shows the name-only matches; Type::method finds only the resolved calls\n",
        "💡 a minta reguláris kifejezés a hívás CÉLJÁRA (kis-/nagybetű-független): a (^|::|\\.|->)név$ név szerint minden hívást megtalál; a --guess a csak név szerinti egyezéseket is mutatja; a Típus::metódus csak a feloldott hívásokat\n",
    ));
    let plain = plain_symbol(pattern);
    if let Some(by_name) = plain.as_deref().and_then(name_pattern)
        && by_name != pattern.trim()
    {
        out.push_str(&ltf!(
            lang,
            "💡 by name only: '{by_name}'\n",
            "💡 csak név szerint: '{by_name}'\n"
        ));
    }
    if let Some(plain) = plain.as_deref() {
        let suggestions = catalog.suggest(plain);
        if !suggestions.is_empty() {
            let list = suggestions.join(", ");
            out.push_str(&ltf!(
                lang,
                "💡 did you mean: {list}\n",
                "💡 esetleg erre gondoltál: {list}\n"
            ));
        }
    }
    out
}

/// `--depth`, ha az 1. szinten nincs hívó: létezik-e a cél, és ha nem, mire hasonlít. Soronként két szóköz
/// behúzással, mint a mélységi nézet többi sora.
pub(crate) fn no_caller(
    lang: Lang,
    pattern: &str,
    presence: Presence,
    catalog: &Catalog,
) -> String {
    let plain = plain_symbol(pattern);
    let name = plain.as_deref().unwrap_or(pattern);
    match presence {
        Presence::Missing => {
            let mut out = ltf!(
                lang,
                "  (no method named {name} in the project map — check the spelling)\n",
                "  (nincs {name} nevű metódus a projekt-térképen — ellenőrizd a helyesírást)\n"
            );
            if let Some(plain) = plain.as_deref() {
                let suggestions = catalog.suggest(plain);
                if !suggestions.is_empty() {
                    let list = suggestions.join(", ");
                    out.push_str(&ltf!(
                        lang,
                        "  💡 did you mean: {list}\n",
                        "  💡 esetleg erre gondoltál: {list}\n"
                    ));
                }
            }
            out
        }
        Presence::DefinedOnly => ltf!(
            lang,
            "  ({name} exists but nothing calls it in the project — it may be an entry point, an event/route handler or called dynamically)\n",
            "  ({name} létezik, de a projektben semmi sem hívja — lehet belépési pont, esemény-/útvonalkezelő, vagy dinamikusan hívják)\n"
        ),
        Presence::CalledElsewhere => ltf!(
            lang,
            "  ({name} is not called from this path, but it is called elsewhere in the project — search from the project root)\n",
            "  ({name}-t ebből az útból nem hívják, de a projekt más részén igen — a projekt gyökeréből keress)\n"
        ),
    }
}

/// `--strings` nulla találatnál: a szűrő kis-/nagybetű-független részlet-illesztés (nem regex).
pub(crate) fn empty_strings(lang: Lang) -> &'static str {
    lang.pick(
        "💡 the match is a case-insensitive substring of the string literal (not a regex); try a shorter fragment\n",
        "💡 az illesztés a string-literál kis-/nagybetű-független részlete (nem regex); próbálj rövidebb töredéket\n",
    )
}

/// `locate` találat nélküli kifejezéseknél: hogyan kérdezz másképp.
pub(crate) fn locate_no_match(lang: Lang) -> &'static str {
    lang.pick(
        "💡 try one part of the name (a camelCase / snake_case piece, singular form); give a non-English word as its English code name (visszatérítés → refund); for UI text quote the phrase (\"…\") or use `show <dir> --strings`\n",
        "💡 próbáld a név egy részét (egy camelCase / snake_case darab, egyes szám); a nem angol szót add meg angol kódnévként (visszatérítés → refund); felületi szöveghez idézd a kifejezést (\"…\") vagy használd: `show <mappa> --strings`\n",
    )
}

/// Ismeretlen `--category`: az ismert nevek listájával (a layer-hibával analóg), és ha van hasonló, azzal.
pub(crate) fn unknown_category(lang: Lang, name: &str, known: &[&str]) -> String {
    let list = known.join(", ");
    let mut out = ltf!(
        lang,
        "Unknown category: {name} (known: {list})",
        "Ismeretlen kategória: {name} (ismertek: {list})"
    );
    let line = did_you_mean(
        lang,
        &closest_names(name, known.iter().copied(), MAX_SUGGESTIONS),
    );
    if !line.is_empty() {
        out.push('\n');
        out.push_str(&line);
    }
    out
}

/// A fájl- / mappanév (utolsó tag) és a szülő (`/` elválasztóval).
fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// Kiterjesztés nélküli név (`User.php` → `User`; a pont nélküli név változatlan).
fn stem(name: &str) -> &str {
    name.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or(name, |(stem, _)| stem)
}

/// Hasonló útvonalak egy nem létező útvonalhoz (`/`-elválasztós, a gyökérhez relatív). Elsőként az azonos nevű
/// (vagy kiterjesztés nélkül azonos nevű) elem más mappában, utána a névre edit-távolsággal (≤2, rövid névnél 1)
/// hasonlók: az azonos mappában lévők előbb, majd távolság, majd ábécé szerint. Legfeljebb `MAX_SUGGESTIONS`.
pub(crate) fn similar_paths(requested: &str, known: &[String]) -> Vec<String> {
    let requested = requested.replace('\\', "/");
    let requested = requested.trim_matches('/');
    let (parent, name) = split_path(requested);
    if name.is_empty() {
        return Vec::new();
    }
    let wanted = name.to_lowercase();
    let max = max_distance(wanted.chars().count());
    let mut same: Vec<&str> = known
        .iter()
        .map(String::as_str)
        .filter(|path| *path != requested)
        .filter(|path| {
            let (_, other) = split_path(path);
            other == name || (!name.contains('.') && stem(other) == name)
        })
        .collect();
    same.sort_unstable_by_key(|path| (path.len().abs_diff(requested.len()), *path));
    let mut near: Vec<(usize, bool, &str)> = Vec::new();
    for path in known.iter().map(String::as_str) {
        let (dir, other) = split_path(path);
        if other == name || same.contains(&path) {
            continue;
        }
        let lower = other.to_lowercase();
        let mut distance = edit_distance(&lower, &wanted);
        // kiterjesztés nélkül kért névnél a törzshöz is mérünk (`Usr` ~ `User.php`)
        if !name.contains('.') {
            distance = distance.min(edit_distance(stem(&lower), &wanted));
        }
        if distance <= max {
            near.push((distance, dir != parent, path));
        }
    }
    near.sort_unstable_by_key(|(distance, other_dir, path)| (*other_dir, *distance, *path));
    same.into_iter()
        .chain(near.into_iter().map(|(_, _, path)| path))
        .take(MAX_SUGGESTIONS)
        .map(str::to_owned)
        .collect()
}

