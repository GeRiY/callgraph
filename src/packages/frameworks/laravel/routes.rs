//! A route-tábla a `routes/*.php` fájlok forrásából. A szűkített gráfban nincs hívás-argumentum, ezért a láncokat a
//! (megjegyzésektől megtisztított) szövegből olvassa: egy `Route::x(..)->y(..)->z(..)` lánc szegmensei, a
//! csoportok (`prefix` / `name` / `controller` + `group(function () { .. })`, és a `Route::group([..], fn)` alak)
//! törzse rekurzívan, a `resource` / `apiResource` a hét / öt szabványos route-tá bontva. Ami nem értelmezhető,
//! kimarad — soha nem hiba.

use std::sync::LazyLock;

use regex::Regex;

use super::super::inertia::render::split_top;
use super::route_cfg::{Configs, Env, Loop, resolve_name};
use crate::core::srcscan::{self, Dialect};
use crate::model::Imports;

/// A csoport-beágyazás felső korlátja (egy kóbor, önmagát hívó szöveg se pörögjön).
const MAX_DEPTH: usize = 8;
/// Egy konfig-ciklusból legfeljebb ennyi elem bomlik ki route-okra.
const MAX_LOOP: usize = 200;

/// Egy route kezelője.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Route {
    pub name: Option<String>,
    /// `GET`, `POST`, `PUT|PATCH`, `ANY` …
    pub verb: String,
    /// Mindig `/`-rel kezdődik (`/register`, a gyökér: `/`).
    pub uri: String,
    pub action: Action,
    /// A route-fájl a gyökérhez relatívan, és a route sora (a tábla helye).
    pub file: String,
    pub line: u32,
    /// A csoportokból örökölt és a saját `->middleware(..)` nevei, a Laravel sorrendjében (osztály: teljes név).
    #[serde(default)]
    pub middleware: Vec<String>,
    /// A konfigból generált route Inertia-komponense (`'component' => 'Welcome'` a config-tömbben), ha van.
    #[serde(default)]
    pub page: Option<String>,
}

/// Az osztály rövid neve (névtér nélkül).
pub(crate) fn short_class(class: &str) -> &str {
    class.rsplit('\\').next().unwrap_or(class)
}

/// A csoportokból örökölt környezet.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Ctx {
    prefix: String,
    name: String,
    controller: Option<String>,
    /// A csoportok `->middleware(..)` nevei (a beágyazás sorrendjében összefűzve).
    middleware: Vec<String>,
}

impl Ctx {
    /// Üres-e (nincs örökölt előtag, név-előtag, controller). A puszta middleware nem számít: az nem változtatja meg a
    /// betöltött fájl route-jait (uri, név, kezelő), csak a middleware-oszlopot.
    pub(super) fn is_empty(&self) -> bool {
        self.prefix.is_empty() && self.name.is_empty() && self.controller.is_none()
    }
}

/// Egy másik route-fájl, amelyet egy csoport tölt be (`->group(base_path('routes/x.php'))`,
/// `->group(__DIR__.'/x.php')`, vagy `require __DIR__.'/x.php'` egy csoport törzsében): a route-jait a csoport
/// környezetével (uri-előtag, név-előtag, controller) kell beolvasni.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Include {
    /// A fájl a gyökérhez relatívan (`routes/admin.php`).
    pub file: String,
    pub ctx: Ctx,
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
/// Egy route-fájl route-jai a legfelső szinten (a csoportokból betöltött fájlok nélkül).
pub(super) fn parse_file(rel: &str, text: &str, imports: &Imports) -> Vec<Route> {
    parse_file_in(rel, text, imports, &Ctx::default()).0
}

/// Egy route-fájl route-jai a `ctx` környezetben (a fájlt egy csoport töltötte be; a legfelső szint: üres), és a
/// csoportokból betöltött további fájlok (a csoportjuk környezetével).
pub(super) fn parse_file_in(
    rel: &str,
    text: &str,
    imports: &Imports,
    ctx: &Ctx,
) -> (Vec<Route>, Vec<Include>) {
    let parsed = parse_file_cfg(rel, text, imports, ctx, &Configs::default());
    (parsed.routes, parsed.includes)
}

/// Egy route-fájl beolvasott tartalma.
pub(super) struct Parsed {
    pub routes: Vec<Route>,
    pub includes: Vec<Include>,
    /// A konfig-ciklussal kibontott láncok sor-tartományai (kezdő, záró sor): az ott álló `->name($név)` nem dinamikus.
    pub expanded: Vec<(u32, u32)>,
}

/// Mint a [`parse_file_in`], és a `foreach (config('a.b') as $k => $v) Route::…` ciklusok a `configs` literál
/// tömbjéből route-okra bomlanak (a változó-használat a tömb értékeivel behelyettesítve).
pub(super) fn parse_file_cfg(
    rel: &str,
    text: &str,
    imports: &Imports,
    ctx: &Ctx,
    configs: &Configs,
) -> Parsed {
    let clean = strip_comments(text);
    let mut starts = vec![0usize];
    starts.extend(clean.match_indices('\n').map(|(i, _)| i + 1));
    let mut parser = Parser {
        text: &clean,
        starts,
        imports,
        rel,
        configs,
        env: None,
        out: Vec::new(),
        includes: Vec::new(),
        expanded: Vec::new(),
    };
    parser.block(0, clean.len(), ctx, 0);
    Parsed {
        routes: parser.out,
        includes: parser.includes,
        expanded: parser.expanded,
    }
}

struct Parser<'a> {
    text: &'a str,
    starts: Vec<usize>,
    imports: &'a Imports,
    rel: &'a str,
    configs: &'a Configs,
    /// A éppen kibontott konfig-ciklus változói (`None`: nincs ciklus).
    env: Option<Env>,
    out: Vec<Route>,
    includes: Vec<Include>,
    expanded: Vec<(u32, u32)>,
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

    /// Az argumentum szövege; konfig-ciklusban a ciklus változói a tömb értékeire cserélve.
    fn arg(&self, range: (usize, usize)) -> String {
        let raw = self.slice(range);
        match &self.env {
            Some(env) => env.subst(raw),
            None => raw.to_owned(),
        }
    }

    /// A szegmens argumentumai szövegként.
    fn texts(&self, segment: &Segment) -> Vec<String> {
        segment.args.iter().map(|&range| self.arg(range)).collect()
    }

    /// A `from..to` tartomány `Route::` láncai, egymás után (a csoportok törzse rekurzívan).
    fn block(&mut self, from: usize, to: usize, ctx: &Ctx, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let mut at = from;
        // a láncok közötti szöveg (a láncok belsejét a csoportjuk saját `block`-ja nézi): `require` egy csoportban
        let mut gap = from;
        let loops = if self.configs.is_empty() {
            Vec::new()
        } else {
            self.loops(from, to)
        };
        while let Some(found) = next_route(self.text, at, to) {
            if let Some(chain) = self.chain(found, to) {
                self.requires(gap, found, ctx);
                at = chain.end;
                gap = at;
                let around: Vec<&Loop> = loops
                    .iter()
                    .filter(|l| l.body.0 <= found && found < l.body.1)
                    .collect();
                if !around.is_empty() {
                    let (first, last) = (self.line_of(chain.start), self.line_of(chain.end));
                    self.expanded.push((first, last));
                }
                self.run(&chain, ctx, depth, &around);
            } else {
                at = found + "Route::".len();
            }
        }
        self.requires(gap, to, ctx);
    }

    /// A lánc kibontása a körülvevő konfig-ciklusokkal (kívülről befelé); ciklus nélkül egyszer.
    fn run(&mut self, chain: &Chain, ctx: &Ctx, depth: usize, loops: &[&Loop]) {
        let Some((first, rest)) = loops.split_first() else {
            self.apply(chain, ctx, depth);
            return;
        };
        let outer = self.env.clone();
        for env in &first.envs {
            self.env = Some(env.merged(outer.as_ref()));
            self.run(chain, ctx, depth, rest);
        }
        self.env = outer;
    }

    /// A `from..to` tartomány `foreach (config('a.b') as $k => $v)` ciklusai, amelyek forrása literál konfig-tömb.
    fn loops(&self, from: usize, to: usize) -> Vec<Loop> {
        static HEAD: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(
                r#"^\s*(?:config\(\s*['"]([\w.\-]+)['"]\s*(?:,[^)]*)?\)|\$(\w+))\s+as\s+(?:\$(\w+)\s*=>\s*)?\$(\w+)\s*$"#,
            )
            .expect("érvényes regex")
        });
        let bytes = self.text.as_bytes();
        let mut out = Vec::new();
        let mut at = from;
        while let Some(found) = find_word(self.text, "foreach", at, to) {
            at = found + "foreach".len();
            let open = skip_ws(self.text, at);
            if bytes.get(open) != Some(&b'(') {
                continue;
            }
            let Some(close) = matching(self.text, open, to) else {
                continue;
            };
            let Some(caps) = HEAD.captures(&self.text[open + 1..close]) else {
                continue;
            };
            let path = match (caps.get(1), caps.get(2)) {
                (Some(path), _) => path.as_str().to_owned(),
                (None, Some(var)) => match self.assigned_config(var.as_str()) {
                    Some(path) => path,
                    None => continue,
                },
                (None, None) => continue,
            };
            let Some((entries, imports)) = self.configs.lookup(&path) else {
                continue;
            };
            let (key_var, value_var) = (caps.get(3).map(|m| m.as_str()), &caps[4]);
            let envs = entries
                .iter()
                .take(MAX_LOOP)
                .map(|entry| Env::of(key_var, value_var, entry, imports))
                .collect();
            let body_from = skip_ws(self.text, close + 1);
            let body = if bytes.get(body_from) == Some(&b'{') {
                matching(self.text, body_from, to).map(|end| (body_from + 1, end))
            } else {
                self.text
                    .get(body_from..to)
                    .and_then(|rest| rest.find(';'))
                    .map(|end| (body_from, body_from + end + 1))
            };
            if let Some(body) = body {
                out.push(Loop { body, envs });
            }
        }
        out
    }

    /// A `$változó = config('a.b');` értékadás konfig-kulcsa (a `foreach ($változó as ..)` forrása).
    fn assigned_config(&self, var: &str) -> Option<String> {
        let pattern = format!(
            r#"\${}\s*=\s*config\(\s*['"]([\w.\-]+)['"]"#,
            regex::escape(var)
        );
        let caps = Regex::new(&pattern).ok()?.captures(self.text)?;
        Some(caps[1].to_owned())
    }

    /// A `from..to` szöveg `require __DIR__.'/x.php'` / `require base_path('routes/x.php')` sorai, ha egy csoport
    /// törzsében vannak (a legfelső szintű `require`-t a betöltött fájlok listája kezeli).
    fn requires(&mut self, from: usize, to: usize, ctx: &Ctx) {
        if ctx.is_empty() || from >= to {
            return;
        }
        let Some(text) = self.text.get(from..to) else {
            return;
        };
        let found: Vec<String> = require_re()
            .captures_iter(text)
            .filter_map(|caps| file_ref(self.rel, &caps[1]))
            .collect();
        for file in found {
            self.includes.push(Include {
                file,
                ctx: ctx.clone(),
            });
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
        let names: String = mods
            .iter()
            .filter(|s| matches!(s.name.as_str(), "name" | "as"))
            .filter_map(|s| {
                // az interpolált / összefűzött név (`"{$code}.x"`, `$code . '.x'`) is név: a ziggy nézet
                // dinamikus mintaként kezeli
                s.args
                    .first()
                    .and_then(|&range| dynamic_text(&self.arg(range), true))
            })
            .collect();
        let prefixes: Vec<String> = mods
            .iter()
            .filter(|s| s.name == "prefix")
            .filter_map(|s| {
                s.args
                    .first()
                    .and_then(|&range| dynamic_text(&self.arg(range), false))
            })
            .collect();
        let controller = mods
            .iter()
            .filter(|s| s.name == "controller")
            .find_map(|s| s.args.first().and_then(|&r| self.class_of(&self.arg(r))));
        let mut own = ctx.clone();
        own.prefix = join_uri(
            std::iter::once(ctx.prefix.as_str()).chain(prefixes.iter().map(String::as_str)),
        );
        own.name.push_str(&names);
        own.middleware.extend(
            mods.iter()
                .filter(|s| s.name == "middleware")
                .flat_map(|s| {
                    s.args
                        .iter()
                        .flat_map(|&r| self.middleware_names(&self.arg(r)))
                }),
        );
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
            let attributes = self.arg(term.args[0]);
            self.attributes(&attributes, &mut ctx);
        }
        // `->group(base_path('routes/admin.php'))` / `->group(__DIR__.'/admin.php')`: a fájl a csoport környezetével
        if let Some(file) = file_ref(self.rel, self.slice((from, to))) {
            if !ctx.is_empty() {
                self.includes.push(Include { file, ctx });
            }
            return;
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
                    if let Some(prefix) = dynamic_text(value, false) {
                        ctx.prefix = join_uri([ctx.prefix.as_str(), prefix.as_str()]);
                    }
                }
                "as" => {
                    if let Some(name) = dynamic_text(value, true) {
                        ctx.name.push_str(&name);
                    }
                }
                "controller" => {
                    if let Some(class) = self.class_of(value) {
                        ctx.controller = Some(class);
                    }
                }
                "middleware" => ctx.middleware.extend(self.middleware_names(value)),
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
            // a saját `->name()` nélküli route egy nevesített csoportban a csoport (összefűzött) nevét kapja — a
            // Laravel `RouteGroup::merge`-e így adja: `$new['as'] = $old['as'] . ($new['as'] ?? '')`
            name: (named || !ctx.name.is_empty()).then(|| ctx.name.clone()),
            verb,
            uri,
            action,
            file: self.rel.to_owned(),
            line,
            middleware: ctx.middleware.clone(),
            page: self.env.as_ref().and_then(Env::page),
        };
        self.out.push(route);
    }

    /// A `->middleware(..)` argumentum nevei: `'auth'`, `X::class` (teljes név) vagy ezek tömbje; más (változó,
    /// hívás) kimarad.
    fn middleware_names(&self, arg: &str) -> Vec<String> {
        let arg = arg.trim();
        if let Some(inner) = arg.strip_prefix('[').and_then(|a| a.strip_suffix(']')) {
            return split_top(inner, ",")
                .into_iter()
                .flat_map(|item| self.middleware_names(item))
                .collect();
        }
        unquote(arg)
            .or_else(|| self.class_of(arg))
            .into_iter()
            .collect()
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
        resolve_name(self.imports, raw)
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
                middleware: ctx.middleware.clone(),
                page: None,
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

/// Egy fájl-hivatkozás: `base_path('routes/x.php')` (a gyökérhez relatív) vagy `__DIR__.'/x.php'` (a hivatkozó
/// `rel` fájl mappájához relatív), normalizálva (`..` / `.` nélkül); más: `None`.
pub(super) fn file_ref(rel: &str, text: &str) -> Option<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"^(?:base_path\(\s*['"]/?([\w/.-]+\.php)['"]\s*\)|__DIR__\s*\.\s*['"]/?([\w/.-]+\.php)['"])$"#,
        )
        .expect("érvényes regex")
    });
    let caps = RE.captures(text.trim())?;
    let joined = match (caps.get(1), caps.get(2)) {
        (Some(rooted), _) => rooted.as_str().to_owned(),
        (None, Some(relative)) => {
            let dir = rel.rsplit_once('/').map_or("", |(dir, _)| dir);
            format!("{dir}/{}", relative.as_str())
        }
        (None, None) => return None,
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn require_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"\b(?:require|include)(?:_once)?\s*\(?\s*(base_path\(\s*['"][\w/.-]+\.php['"]\s*\)|__DIR__\s*\.\s*['"][\w/.-]+\.php['"])"#,
        )
        .expect("érvényes regex")
    });
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

/// Egy név / előtag argumentum szövege dinamikus mintával: a literál értéke; `"{$code}.x"` / `"$code.x"` változatlanul;
/// az összefűzés (`$code . '.x'`, `'x.' . config('a')`) literál darabokkal és `{$?}`-ként a nem literál darabokkal.
/// `need_literal`: legalább egy literál darab kell (a puszta `$name` nem minta); különben a puszta dinamikus érték
/// is `{$?}`.
fn dynamic_text(raw: &str, need_literal: bool) -> Option<String> {
    let raw = raw.trim();
    if let Some(text) = unquote(raw) {
        return Some(text);
    }
    if let Some(inner) = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .filter(|inner| inner.contains('$') && !inner.contains('"'))
    {
        return Some(inner.to_owned());
    }
    let parts = split_top(raw, ".");
    let mut text = String::new();
    let mut literal = false;
    for part in &parts {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        if let Some(value) = unquote(part) {
            text.push_str(&value);
            literal = true;
        } else if let Some(inner) = part
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .filter(|inner| !inner.contains('"'))
        {
            text.push_str(inner);
            literal = true;
        } else {
            text.push_str("{$?}");
        }
    }
    (literal || !need_literal).then_some(text)
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

/// A `word` kulcsszó következő előfordulása (szóhatárral) a `from..to` tartományban.
fn find_word(text: &str, word: &str, from: usize, to: usize) -> Option<usize> {
    let mut at = from;
    while at < to {
        let found = at + text.get(at..to)?.find(word)?;
        let after = found + word.len();
        let bytes = text.as_bytes();
        let before_ok = found == 0 || !crate::core::text::is_ident_byte(bytes[found - 1]);
        let after_ok = bytes
            .get(after)
            .is_none_or(|b| !crate::core::text::is_ident_byte(*b));
        if before_ok && after_ok {
            return Some(found);
        }
        at = after;
    }
    None
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
pub(super) fn matching(text: &str, open: usize, to: usize) -> Option<usize> {
    srcscan::matching(text, open, to, Dialect::Php)
}

/// A `from..to` szöveg argumentumai a legfelső szintű vesszők mentén (levágva, az üresek nélkül): bájt-tartományok.
fn split_args(text: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    srcscan::split_args(text, from, to, Dialect::Php)
}

/// A PHP-megjegyzések (`//`, `#`, `/* */`) szóközre cserélve — a bájt-helyek és a sortörések megmaradnak, és a
/// stringeken belüli `//` (url) érintetlen.
pub(super) fn strip_comments(text: &str) -> String {
    srcscan::strip_comments(text, Dialect::Php)
}
