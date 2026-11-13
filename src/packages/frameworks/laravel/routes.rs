//! A route-tábla a `routes/*.php` fájlok forrásából. A szűkített gráfban nincs hívás-argumentum, ezért a láncokat a
//! (megjegyzésektől megtisztított) szövegből olvassa: egy `Route::x(..)->y(..)->z(..)` lánc szegmensei, a
//! csoportok (`prefix` / `name` / `controller` + `group(function () { .. })`, és a `Route::group([..], fn)` alak)
//! törzse rekurzívan, a `resource` / `apiResource` a hét / öt szabványos route-tá bontva. Ami nem értelmezhető,
//! kimarad — soha nem hiba.

use std::sync::LazyLock;

use regex::Regex;

use super::super::inertia::render::split_top;
use crate::model::Imports;

/// A csoport-beágyazás felső korlátja (egy kóbor, önmagát hívó szöveg se pörögjön).
const MAX_DEPTH: usize = 8;

/// Egy route kezelője.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// `[X::class, 'm']` / `'X@m'` / resource-metódus; az invokálható `X::class`: `__invoke`.
    Controller {
        class: String,
        method: String,
    },
    /// `Route::inertia(..)` vagy `Inertia::render('X')`-et visszaadó closure: a page-név.
    Page(String),
    /// `Route::view('/uri', 'nézet')`.
    View(String),
    /// `Route::redirect('/uri', '/cél')`.
    Redirect(String),
    Closure,
}

impl Action {
    /// A kezelő rövid, kiírt alakja: `RegisteredUserController@store`, `view welcome`, `redirect /home`, `closure`
    /// (a `Page` nevét a hívó oldja fel fájlra, ezért itt `page X`).
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Controller { class, method } => {
                format!("{}@{method}", short_class(class))
            }
            Self::Page(page) => format!("page {page}"),
            Self::View(view) => format!("view {view}"),
            Self::Redirect(to) => format!("redirect {to}"),
            Self::Closure => "closure".to_owned(),
        }
    }

    /// A teljes (névtérrel minősített) alak a json-hoz.
    pub(crate) fn full(&self) -> String {
        match self {
            Self::Controller { class, method } => format!("{class}@{method}"),
            other => other.label(),
        }
    }
}

/// Egy route a táblában.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub name: Option<String>,
    /// `GET`, `POST`, `PUT|PATCH`, `ANY` …
    pub verb: String,
    /// Mindig `/`-rel kezdődik (`/register`, a gyökér: `/`).
    pub uri: String,
    pub action: Action,
    /// A route-fájl a gyökérhez relatívan, és a route sora (a tábla helye; most a tesztek olvassák).
    #[allow(dead_code)]
    pub file: String,
    #[allow(dead_code)]
    pub line: u32,
}

/// Az osztály rövid neve (névtér nélkül).
pub(crate) fn short_class(class: &str) -> &str {
    class.rsplit('\\').next().unwrap_or(class)
}

/// A csoportokból örökölt környezet.
#[derive(Debug, Clone, Default)]
struct Ctx {
    prefix: String,
    name: String,
    controller: Option<String>,
}

/// Egy lánc szegmense: a (kisbetűs) metódusnév és az argumentumok bájt-tartományai a megtisztított szövegben.
struct Segment {
    name: String,
    args: Vec<(usize, usize)>,
}

struct Chain {
    segments: Vec<Segment>,
    start: usize,
    end: usize,
}

/// Egy route-fájl route-jai. `rel`: a fájl a gyökérhez relatívan; `imports`: a fájl `use`-ai (a controller-osztályok
/// teljes nevéhez).
pub(super) fn parse_file(rel: &str, text: &str, imports: &Imports) -> Vec<Route> {
    let clean = strip_comments(text);
    let mut starts = vec![0usize];
    starts.extend(clean.match_indices('\n').map(|(i, _)| i + 1));
    let mut parser = Parser {
        text: &clean,
        starts,
        imports,
        rel,
        out: Vec::new(),
    };
    parser.block(0, clean.len(), &Ctx::default(), 0);
    parser.out
}

struct Parser<'a> {
    text: &'a str,
    starts: Vec<usize>,
    imports: &'a Imports,
    rel: &'a str,
    out: Vec<Route>,
}

impl Parser<'_> {
    /// A (1-től számozott) sor a bájt-helyen.
    fn line_of(&self, at: usize) -> u32 {
        u32::try_from(self.starts.partition_point(|&start| start <= at)).unwrap_or(u32::MAX)
    }

    /// Az argumentum szövege (levágva).
    fn slice(&self, (from, to): (usize, usize)) -> &str {
        self.text.get(from..to).unwrap_or("").trim()
    }

    /// A szegmens argumentumai szövegként.
    fn texts(&self, segment: &Segment) -> Vec<String> {
        segment
            .args
            .iter()
            .map(|&range| self.slice(range).to_owned())
            .collect()
    }

    /// A `from..to` tartomány `Route::` láncai, egymás után (a csoportok törzse rekurzívan).
    fn block(&mut self, from: usize, to: usize, ctx: &Ctx, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let mut at = from;
        while let Some(found) = next_route(self.text, at, to) {
            if let Some(chain) = self.chain(found, to) {
                at = chain.end;
                self.apply(&chain, ctx, depth);
            } else {
                at = found + "Route::".len();
            }
        }
    }

    /// A `Route::x(..)->y(..)` lánc a `at` helyen; ha az első szegmens nem hívás (`Route::currentRouteName`
    /// zárójel nélkül), `None`.
    fn chain(&self, at: usize, to: usize) -> Option<Chain> {
        let mut pos = at + "Route::".len();
        let (first, next) = self.segment(pos, to)?;
        let mut segments = vec![first];
        pos = next;
        loop {
            let arrow = skip_ws(self.text, pos);
            if !self.text.get(arrow..).is_some_and(|t| t.starts_with("->")) {
                break;
            }
            let name_at = skip_ws(self.text, arrow + 2);
            let Some((segment, next)) = self.segment(name_at, to) else {
                break;
            };
            segments.push(segment);
            pos = next;
        }
        Some(Chain {
            segments,
            start: at,
            end: pos,
        })
    }

    /// Egy `név(argumentumok)` szegmens a `pos` helyen, és a záró `)` utáni hely.
    fn segment(&self, pos: usize, to: usize) -> Option<(Segment, usize)> {
        let bytes = self.text.as_bytes();
        let mut end = pos;
        while end < to && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end == pos {
            return None;
        }
        let name = self.text[pos..end].to_ascii_lowercase();
        let open = skip_ws(self.text, end);
        if open >= to || bytes[open] != b'(' {
            return None;
        }
        let close = matching(self.text, open, to)?;
        Some((
            Segment {
                name,
                args: split_args(self.text, open + 1, close),
            },
            close + 1,
        ))
    }

    /// Egy lánc hatása: csoport → a törzs a módosított környezettel; route / resource → a tábla bővül.
    fn apply(&mut self, chain: &Chain, ctx: &Ctx, depth: usize) {
        let Some(at) = chain.segments.iter().position(|s| is_terminal(&s.name)) else {
            return;
        };
        let term = &chain.segments[at];
        let mods: Vec<&Segment> = chain
            .segments
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, segment)| segment)
            .collect();
        let first_literal = |segment: &Segment| {
            segment
                .args
                .first()
                .and_then(|&range| unquote(self.slice(range)))
        };
        let names: String = mods
            .iter()
            .filter(|s| matches!(s.name.as_str(), "name" | "as"))
            .filter_map(|s| first_literal(s))
            .collect();
        let prefixes: Vec<String> = mods
            .iter()
            .filter(|s| s.name == "prefix")
            .filter_map(|s| first_literal(s))
            .collect();
        let controller = mods
            .iter()
            .filter(|s| s.name == "controller")
            .find_map(|s| s.args.first().and_then(|&r| self.class_of(self.slice(r))));
        let mut own = ctx.clone();
        own.prefix = join_uri(
            std::iter::once(ctx.prefix.as_str()).chain(prefixes.iter().map(String::as_str)),
        );
        own.name.push_str(&names);
        if controller.is_some() {
            own.controller = controller;
        }
        let line = self.line_of(chain.start);
        match term.name.as_str() {
            "group" => self.group(term, own, depth),
            "resource" | "apiresource" => self.resource(term, &mods, &own, line),
            _ => self.route(term, &own, !names.is_empty(), line),
        }
    }

    /// `->group(function () { .. })` / `Route::group([attr], function () { .. })`.
    fn group(&mut self, term: &Segment, mut ctx: Ctx, depth: usize) {
        let Some(&(from, to)) = term.args.last() else {
            return;
        };
        if term.args.len() >= 2 {
            let attributes = self.slice(term.args[0]).to_owned();
            self.attributes(&attributes, &mut ctx);
        }
        self.block(from, to, &ctx, depth + 1);
    }

    /// A `['prefix' => 'x', 'as' => 'y.', 'controller' => X::class]` tömb a környezetbe.
    fn attributes(&self, array: &str, ctx: &mut Ctx) {
        let Some(inner) = array.strip_prefix('[').and_then(|a| a.strip_suffix(']')) else {
            return;
        };
        for item in split_top(inner, ",") {
            let parts = split_top(item, "=>");
            let [key, value] = parts.as_slice() else {
                continue;
            };
            let Some(key) = unquote(key) else {
                continue;
            };
            let value = value.trim();
            match key.as_str() {
                "prefix" => {
                    if let Some(prefix) = unquote(value) {
                        ctx.prefix = join_uri([ctx.prefix.as_str(), prefix.as_str()]);
                    }
                }
                "as" => {
                    if let Some(name) = unquote(value) {
                        ctx.name.push_str(&name);
                    }
                }
                "controller" => {
                    if let Some(class) = self.class_of(value) {
                        ctx.controller = Some(class);
                    }
                }
                _ => {}
            }
        }
    }

    /// Egy HTTP-igés / `match` / `view` / `redirect` / `inertia` route a táblába.
    fn route(&mut self, term: &Segment, ctx: &Ctx, named: bool, line: u32) {
        let args = self.texts(term);
        let arg = |n: usize| args.get(n).map(String::as_str);
        let literal_action = |n: usize, make: fn(String) -> Action| {
            arg(n).and_then(unquote).map_or(Action::Closure, make)
        };
        let action_at = |n: usize| arg(n).map_or(Action::Closure, |a| self.action(a, ctx));
        let (verb, uri_at, action) = match term.name.as_str() {
            "get" | "post" | "put" | "patch" | "delete" | "options" | "any" => {
                (term.name.to_ascii_uppercase(), 0, action_at(1))
            }
            "match" => (arg(0).map(methods_of).unwrap_or_default(), 1, action_at(2)),
            "view" => ("GET".to_owned(), 0, literal_action(1, Action::View)),
            "redirect" | "permanentredirect" => {
                ("ANY".to_owned(), 0, literal_action(1, Action::Redirect))
            }
            "inertia" => ("GET".to_owned(), 0, literal_action(1, Action::Page)),
            _ => return,
        };
        // a nem literál uri (változó, összefűzés) `?`: a route neve így is ismert marad
        let own = arg(uri_at).and_then(unquote);
        let uri = join_uri([ctx.prefix.as_str(), own.as_deref().unwrap_or("?")]);
        let route = Route {
            name: named.then(|| ctx.name.clone()),
            verb,
            uri,
            action,
            file: self.rel.to_owned(),
            line,
        };
        self.out.push(route);
    }

    /// A route kezelője: `[X::class, 'm']`, `X::class` (invokálható), `'X@m'`, `'m'` (a csoport controllerén),
    /// closure (benne `Inertia::render('P')` → page); más: closure.
    fn action(&self, arg: &str, ctx: &Ctx) -> Action {
        let t = arg.trim();
        if let Some(c) = array_re().captures(t) {
            return Action::Controller {
                class: self.resolve(&c[1]),
                method: c[2].to_owned(),
            };
        }
        if let Some(c) = class_re().captures(t) {
            return Action::Controller {
                class: self.resolve(&c[1]),
                method: "__invoke".to_owned(),
            };
        }
        if let Some(text) = unquote(t) {
            return match text.split_once('@') {
                Some((class, method)) => Action::Controller {
                    class: self.resolve(class),
                    method: method.to_owned(),
                },
                None => match &ctx.controller {
                    Some(class) => Action::Controller {
                        class: class.clone(),
                        method: text,
                    },
                    None => Action::Controller {
                        class: self.resolve(&text),
                        method: "__invoke".to_owned(),
                    },
                },
            };
        }
        if closure_re().is_match(t)
            && let Some(c) = page_re().captures(t)
        {
            return Action::Page(c[1].to_owned());
        }
        Action::Closure
    }

    /// `X::class` → a teljes osztálynév.
    fn class_of(&self, arg: &str) -> Option<String> {
        let c = class_re().captures(arg.trim())?;
        Some(self.resolve(&c[1]))
    }

    /// Az osztálynév teljes alakja a fájl `use`-aiból (a vezető `\` = már teljes).
    fn resolve(&self, raw: &str) -> String {
        let raw = raw.trim();
        if let Some(full) = raw.strip_prefix('\\') {
            return full.to_owned();
        }
        let (head, rest) = match raw.split_once('\\') {
            Some((head, rest)) => (head, Some(rest)),
            None => (raw, None),
        };
        match (self.imports.get(head), rest) {
            (Some(full), Some(rest)) => format!("{full}\\{rest}"),
            (Some(full), None) => full.to_owned(),
            (None, _) => raw.to_owned(),
        }
    }

    /// `Route::resource('photos', PhotoController::class)` → 7 route; `apiResource` → 5 (`create` / `edit` nélkül);
    /// a láncolt `->only([..])` / `->except([..])` szűri. A beágyazott név (`photos.comments`) a szülő paraméterén át.
    fn resource(&mut self, term: &Segment, mods: &[&Segment], ctx: &Ctx, line: u32) {
        let args = self.texts(term);
        let Some(resource) = args.first().and_then(|a| unquote(a)) else {
            return;
        };
        let class = args
            .get(1)
            .and_then(|a| {
                self.class_of(a)
                    .or_else(|| unquote(a).map(|text| self.resolve(&text)))
            })
            .or_else(|| ctx.controller.clone());
        let Some(class) = class else {
            return;
        };
        let words = |name: &str| -> Vec<String> {
            mods.iter()
                .filter(|s| s.name == name)
                .flat_map(|s| s.args.iter())
                .flat_map(|&range| {
                    word_re()
                        .captures_iter(self.slice(range))
                        .map(|c| c[1].to_owned())
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        let (only, except) = (words("only"), words("except"));
        let parts: Vec<&str> = resource
            .split(['.', '/'])
            .filter(|part| !part.is_empty())
            .collect();
        let Some((last, parents)) = parts.split_last() else {
            return;
        };
        let mut base = String::new();
        for parent in parents {
            base.push_str(&format!("{parent}/{{{}}}/", singular(parent)));
        }
        base.push_str(last);
        let param = singular(last).replace('-', "_");
        let name_base = parts.join(".");
        let api = term.name == "apiresource";
        for (action, verb, suffix) in RESOURCE {
            if api && matches!(action, "create" | "edit") {
                continue;
            }
            let listed = only.iter().any(|w| w == action);
            if (!only.is_empty() && !listed) || except.iter().any(|w| w == action) {
                continue;
            }
            let tail = suffix.replace("{p}", &format!("{{{param}}}"));
            self.out.push(Route {
                name: Some(format!("{}{name_base}.{action}", ctx.name)),
                verb: verb.to_owned(),
                uri: join_uri([ctx.prefix.as_str(), base.as_str(), tail.as_str()]),
                action: Action::Controller {
                    class: class.clone(),
                    method: action.to_owned(),
                },
                file: self.rel.to_owned(),
                line,
            });
        }
    }
}

/// A resource szabványos route-jai: (metódus, ige, uri-vég) — a Laravel sorrendjében.
const RESOURCE: [(&str, &str, &str); 7] = [
    ("index", "GET", ""),
    ("create", "GET", "create"),
    ("store", "POST", ""),
    ("show", "GET", "{p}"),
    ("edit", "GET", "{p}/edit"),
    ("update", "PUT|PATCH", "{p}"),
    ("destroy", "DELETE", "{p}"),
];

/// Közelítő egyes szám (a Laravel `Str::singular`-ja helyett): `photos` → `photo`, `categories` → `category`.
fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        format!("{stem}y")
    } else if ["sses", "xes", "ches", "shes"]
        .iter()
        .any(|end| word.ends_with(end))
    {
        word.strip_suffix("es").unwrap_or(word).to_owned()
    } else if word.ends_with("ss") {
        word.to_owned()
    } else {
        word.strip_suffix('s').unwrap_or(word).to_owned()
    }
}

fn is_terminal(name: &str) -> bool {
    matches!(
        name,
        "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "options"
            | "any"
            | "match"
            | "view"
            | "redirect"
            | "permanentredirect"
            | "inertia"
            | "resource"
            | "apiresource"
            | "group"
    )
}

fn class_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\\?([\w\\]+)::class$").expect("érvényes regex"));
    &RE
}

fn array_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^\[\s*\\?([\w\\]+)::class\s*,\s*['"](\w+)['"]\s*,?\s*\]$"#)
            .expect("érvényes regex")
    });
    &RE
}

fn closure_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:static\s+)?(?:function|fn)\b").expect("érvényes regex"));
    &RE
}

fn page_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:Inertia::render|\binertia)\s*\(\s*['"]([^'"]+)['"]"#)
            .expect("érvényes regex")
    });
    &RE
}

fn word_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"['"](\w+)['"]"#).expect("érvényes regex"));
    &RE
}

/// `['get', 'post']` → `GET|POST`.
fn methods_of(list: &str) -> String {
    let found: Vec<String> = word_re()
        .captures_iter(list)
        .map(|c| c[1].to_ascii_uppercase())
        .collect();
    if found.is_empty() {
        "MATCH".to_owned()
    } else {
        found.join("|")
    }
}

/// Sima string-literál értéke (`'x'` / `"x"`); más (változó, összefűzés, behelyettesítés): `None`.
fn unquote(arg: &str) -> Option<String> {
    let t = arg.trim();
    let quote = t.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
    let plain = !inner.contains(quote) && !(quote == '"' && inner.contains('$'));
    plain.then(|| inner.replace("\\\\", "\\"))
}

/// Az uri-darabok `/`-rel összefűzve, üres darabok nélkül; mindig `/`-rel kezdődik.
fn join_uri<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let segments: Vec<&str> = parts
        .into_iter()
        .flat_map(|part| part.split('/'))
        .filter(|segment| !segment.is_empty())
        .collect();
    format!("/{}", segments.join("/"))
}

/// A következő `Route::` (szó elején) a `from..to` tartományban.
fn next_route(text: &str, from: usize, to: usize) -> Option<usize> {
    let mut at = from;
    while at < to {
        let found = at + text.get(at..to)?.find("Route::")?;
        let before_ok = found == 0 || {
            let b = text.as_bytes()[found - 1];
            !(b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        };
        if before_ok {
            return Some(found);
        }
        at = found + "Route::".len();
    }
    None
}

/// A következő nem-szóköz bájt helye.
fn skip_ws(text: &str, from: usize) -> usize {
    let rest = text.as_bytes().get(from..).unwrap_or_default();
    from + rest.iter().take_while(|b| b.is_ascii_whitespace()).count()
}

/// A `open` helyi nyitó zárójel párja (string- és beágyazás-tudóan); lezáratlan: `None`.
fn matching(text: &str, open: usize, to: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in text.as_bytes().iter().enumerate().take(to).skip(open) {
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
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A `from..to` szöveg argumentumai a legfelső szintű vesszők mentén (levágva, az üresek nélkül): bájt-tartományok.
fn split_args(text: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut start = from;
    for (i, &b) in text.as_bytes().iter().enumerate().take(to).skip(from) {
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
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                spans.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    spans.push((start, to));
    spans
        .into_iter()
        .filter_map(|(a, b)| {
            let raw = text.get(a..b)?;
            let trimmed = raw.trim();
            let lead = raw.len() - raw.trim_start().len();
            (!trimmed.is_empty()).then_some((a + lead, a + lead + trimmed.len()))
        })
        .collect()
}

/// Egy karakter helyére szóköz (a sortörés marad): a bájt-helyek így nem tolódnak el.
fn blank(out: &mut String, ch: char) {
    if ch == '\n' {
        out.push('\n');
    } else {
        out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
    }
}

/// A PHP-megjegyzések (`//`, `#`, `/* */`) szóközre cserélve — a bájt-helyek és a sortörések megmaradnak, és a
/// stringeken belüli `//` (url) érintetlen.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        let next = chars.peek().copied();
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            '/' if next == Some('/') => skip_line(&mut chars, &mut out, ch),
            '#' if next != Some('[') => skip_line(&mut chars, &mut out, ch),
            '/' if next == Some('*') => {
                chars.next();
                out.push_str("  ");
                let mut star = false;
                for c in chars.by_ref() {
                    blank(&mut out, c);
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Egy sor-megjegyzés a sor végéig szóközre cserélve (a sortörés megmarad).
fn skip_line(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String, first: char) {
    blank(out, first);
    while let Some(&c) = chars.peek() {
        if c == '\n' {
            break;
        }
        blank(out, c);
        chars.next();
    }
}
