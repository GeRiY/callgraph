//! A PHP render-helyek: `Inertia::render('X', [...])`, `inertia('X', [...])`, `Inertia::render(component: 'X')` és a
//! route-fájl `Route::inertia('/uri', 'X', [...])` hívása. A page-név csak sima string-literál (dinamikus név:
//! kimarad); a props a tömb-literál (vagy `compact(...)`) kulcsai. Az argumentumok a hívás tárolt `args`-ából (a
//! teljes gráfban, 60 karakterre vágva), vagy a forrásból (a szűkített tényekben nincs argumentum; a forrás a teljes
//! szöveget adja, így a props-kulcsok sem vágódnak le).

use crate::core::srcscan::{self, Dialect};
use crate::model::{Call, FileGraph, FrameworkFacts, RenderFact};

/// A forrásbeli argumentum-keresés felső korlátja (bájt): egy lezáratlan hívás se járja be a fájl végéig.
const MAX_SCAN: usize = 20_000;

/// A render-hívás fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderKind {
    /// `Inertia::render('X', $props)` (`Inertia\Inertia`).
    Render,
    /// `inertia('X', $props)`.
    Helper,
    /// `Route::inertia('/uri', 'X', $props)`.
    Route,
}

impl RenderKind {
    /// A hívás render-hívás-e (a feloldott, különben a nyers cél szerint, kis- / nagybetűtől függetlenül).
    pub(crate) fn of(call: &Call) -> Option<Self> {
        let target = call.resolved.as_deref().unwrap_or(&call.target);
        let target = target.trim_start_matches('\\');
        match call.kind.as_str() {
            "func_call" => {
                let short = target.rsplit('\\').next().unwrap_or(target);
                short
                    .eq_ignore_ascii_case("inertia")
                    .then_some(Self::Helper)
            }
            "static_call" => {
                let (class, method) = target.rsplit_once("::")?;
                let short = class.rsplit('\\').next().unwrap_or(class);
                let is = |want_method: &str, want_class: &str| {
                    method.eq_ignore_ascii_case(want_method)
                        && short.eq_ignore_ascii_case(want_class)
                };
                if is("render", "inertia") {
                    Some(Self::Render)
                } else if is("inertia", "route") {
                    Some(Self::Route)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// A hívott név a forrásban (a hívás megtalálásához a sorában).
    fn name(self) -> &'static str {
        match self {
            Self::Render => "render",
            Self::Helper | Self::Route => "inertia",
        }
    }

    /// A page-név és a props pozicionális helye.
    fn slots(self) -> (usize, usize) {
        match self {
            Self::Render | Self::Helper => (0, 1),
            Self::Route => (1, 2),
        }
    }
}

/// Egy oldal renderelési helye a PHP-ban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderSite {
    /// A hívó fájl a gyökérhez relatívan, `/`-rel.
    pub file: String,
    pub container: Option<String>,
    pub caller: Option<String>,
    pub line: u32,
    /// A page-név (`Users/Index`).
    pub page: String,
    /// A props tömb-literál kulcsai; `None` = nem statikus (változó, kifejezés, levágott argumentum).
    pub props: Option<Vec<String>>,
}

impl RenderSite {
    /// A hívó: `BoardController::show`, függvénynél a neve, a felső szinten (route-fájl) a fájl útja.
    pub(crate) fn caller_label(&self) -> String {
        match (self.container.as_deref(), self.caller.as_deref()) {
            (_, None | Some("__top__")) => self.file.clone(),
            (Some(container), Some(caller)) => format!("{container}::{caller}"),
            (None, Some(caller)) => caller.to_owned(),
        }
    }

    /// `BoardController::show L23`.
    pub(crate) fn label(&self) -> String {
        format!("{} L{}", self.caller_label(), self.line)
    }
}

/// Egy PHP-fájl render-helyei: az argumentumok a forrásból (`text`), ha van, különben a hívás tárolt `args`-ából.
pub(crate) fn sites_in(rel: &str, graph: &FileGraph, text: Option<&str>) -> Vec<RenderSite> {
    graph
        .calls
        .iter()
        .filter_map(|call| {
            let kind = RenderKind::of(call)?;
            let args = text
                .and_then(|text| source_args(text, call.line, kind))
                .or_else(|| call.args.clone())?;
            Some(RenderSite {
                file: rel.to_owned(),
                container: call.container.clone(),
                caller: call.caller.clone(),
                line: call.line,
                page: page_of(kind, &args)?,
                props: props_of(kind, &args),
            })
        })
        .collect()
}

/// A render-hívások sorai, amelyeknek nincs literál page-nevű helye (`sites`: a hívások találatai): dinamikus render.
pub(crate) fn dynamic_lines(graph: &FileGraph, sites: &[RenderSite]) -> Vec<u32> {
    graph
        .calls
        .iter()
        .filter(|call| RenderKind::of(call).is_some())
        .filter(|call| !sites.iter().any(|site| site.line == call.line))
        .map(|call| call.line)
        .collect()
}

/// A render-hely tényként.
pub(crate) fn fact_of_site(site: RenderSite) -> RenderFact {
    RenderFact {
        page: site.page,
        prop_keys: site.props,
        line: site.line,
        container: site.container,
        caller: site.caller,
    }
}

/// A fájl tényei render-helyekként (a `rel` a gyökérhez relatív út).
pub(crate) fn sites_of_facts(rel: &str, facts: &FrameworkFacts) -> Vec<RenderSite> {
    facts
        .render_sites
        .iter()
        .map(|fact| RenderSite {
            file: rel.to_owned(),
            container: fact.container.clone(),
            caller: fact.caller.clone(),
            line: fact.line,
            page: fact.page.clone(),
            props: fact.prop_keys.clone(),
        })
        .collect()
}

/// A PHP-fájl framework-tényei (R4): a render-helyek és a megosztott props az elemzéskor, a forrásból (`text` a
/// teljes fájl). Nincs render-hívás és nincs `share` metódus: `None`.
pub(crate) fn facts_of(graph: &FileGraph, text: &str) -> Option<FrameworkFacts> {
    let renders = graph
        .calls
        .iter()
        .any(|call| RenderKind::of(call).is_some());
    let shared = super::shared::shared_in(graph, text);
    if !renders && shared.is_none() {
        return None;
    }
    let sites = sites_in("", graph, Some(text));
    let facts = FrameworkFacts {
        dynamic_lines: dynamic_lines(graph, &sites),
        render_sites: sites.into_iter().map(fact_of_site).collect(),
        shared,
    };
    (!facts.is_empty()).then_some(facts)
}

/// A hívás page-neve a tárolt `args`-ból (az outline / json nyila); nem render-hívásnál, argumentum nélkül vagy
/// dinamikus névnél `None`.
pub(crate) fn page_of_call(call: &Call) -> Option<String> {
    let kind = RenderKind::of(call)?;
    page_of(kind, call.args.as_deref()?)
}

/// A page-név: a `component:` nevesített argumentum, különben a pozicionális hely; csak sima string-literál.
pub(crate) fn page_of(kind: RenderKind, args: &[String]) -> Option<String> {
    literal(argument(args, "component", kind.slots().0)?)
}

/// A props kulcsai: a `props:` nevesített argumentum, különben a pozicionális hely; nincs ilyen argumentum: üres
/// lista (nem ad át props-ot).
pub(crate) fn props_of(kind: RenderKind, args: &[String]) -> Option<Vec<String>> {
    match argument(args, "props", kind.slots().1) {
        Some(value) => prop_keys(value),
        None => Some(Vec::new()),
    }
}

/// A `name:` nevesített argumentum értéke, különben a `slot`-adik (nem nevesített) argumentum.
fn argument<'a>(args: &'a [String], name: &str, slot: usize) -> Option<&'a str> {
    let split: Vec<(Option<&'a str>, &'a str)> =
        args.iter().map(|arg| split_named(arg.as_str())).collect();
    if let Some(&(_, value)) = split.iter().find(|(named, _)| *named == Some(name)) {
        return Some(value);
    }
    match split.get(slot) {
        Some(&(None, value)) => Some(value),
        _ => None,
    }
}

/// `component: 'X'` → (`component`, `'X'`); a `::` (statikus elérés) nem név-elválasztó.
fn split_named(arg: &str) -> (Option<&str>, &str) {
    let arg = arg.trim();
    let end = arg
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map_or(arg.len(), |(i, _)| i);
    if end == 0 || arg.as_bytes()[0].is_ascii_digit() {
        return (None, arg);
    }
    let rest = arg[end..].trim_start();
    match rest.strip_prefix(':') {
        Some(value) if !value.starts_with(':') => (Some(&arg[..end]), value.trim()),
        _ => (None, arg),
    }
}

/// Sima string-literál értéke (`'X'` / `"X"`, változó-behelyettesítés és levágás nélkül); más: `None`.
pub(crate) fn literal(arg: &str) -> Option<String> {
    let t = arg.trim();
    let quote = t.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
    let plain = !inner.is_empty()
        && !inner.contains(quote)
        && !inner.contains('\\')
        && !(quote == '"' && inner.contains('$'));
    plain.then(|| inner.to_owned())
}

/// Egy props-argumentum kulcsai: `['a' => …, "b" => …]` / `array(…)` / `compact('a', 'b')`; ha egy elem nem
/// literál kulcsú (spread, kulcs nélküli, kifejezés-kulcs), vagy az argumentum nem ilyen alak: `None`.
pub(crate) fn prop_keys(arg: &str) -> Option<Vec<String>> {
    let t = arg.trim();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("compact(") {
        let inner = t.get("compact(".len()..)?.strip_suffix(')')?;
        return split_top(inner, ",")
            .into_iter()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(literal)
            .collect();
    }
    let inner = if let Some(rest) = t.strip_prefix('[') {
        rest.strip_suffix(']')?
    } else if lower.starts_with("array(") {
        t.get("array(".len()..)?.strip_suffix(')')?
    } else {
        return None;
    };
    let mut keys = Vec::new();
    for item in split_top(inner, ",") {
        if item.trim().is_empty() {
            continue;
        }
        let parts = split_top(item, "=>");
        if parts.len() < 2 {
            return None;
        }
        keys.push(literal(parts[0])?);
    }
    Some(keys)
}

/// A hívás argumentumai a forrásból: a `line`. sorban kezdődő hívás-láncban az első `<név>(` (a render-hívás neve),
/// onnan a záró `)`-ig, a legfelső szintű vesszők mentén. Nem található / lezáratlan hívás: `None`.
pub(crate) fn source_args(text: &str, line: u32, kind: RenderKind) -> Option<Vec<String>> {
    source_args_named(text, line, kind.name(), Dialect::Php)
}

/// A hívás argumentumai a forrásból, tetszőleges hívott névvel (kisbetűs `name`): a `line`. sorban kezdődő hívás-láncban
/// az első `<name>(` — a Laravel-kötések a `route(` / `to_route(` hívásokhoz is ezt használják. A többsoros láncolt
/// hívás (`form⏎.transform(..)⏎.post(..)`) hívás-sora a lánc kezdősora, ezért a keresés a lánc teljes tartományára
/// kiterjed ([`srcscan::chain_end`]), nem csak az első sorra.
pub(crate) fn source_args_named(
    text: &str,
    line: u32,
    name: &str,
    dialect: Dialect,
) -> Option<Vec<String>> {
    let start = line_start(text, line)?;
    let rest = &text[start..];
    let first_line = rest.find('\n').unwrap_or(rest.len());
    let region_end = srcscan::chain_end(text, start, dialect, MAX_SCAN).max(start + first_line);
    let lower = rest[..region_end - start].to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        let after = at + name.len();
        let before_ok = at == 0 || {
            let b = lower.as_bytes()[at - 1];
            !(b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        };
        let tail = &lower[after..];
        let trimmed = tail.trim_start();
        if before_ok && trimmed.starts_with('(') {
            let open = after + (tail.len() - trimmed.len());
            return srcscan::call_args(&rest[open + 1..], dialect, MAX_SCAN);
        }
        from = after;
    }
    None
}

/// A `line`. (1-től számozott) sor kezdő bájtja.
fn line_start(text: &str, line: u32) -> Option<usize> {
    let skip = usize::try_from(line).ok()?.checked_sub(1)?;
    if skip == 0 {
        return Some(0);
    }
    text.match_indices('\n').nth(skip - 1).map(|(i, _)| i + 1)
}

/// A szöveg darabjai a legfelső szintű (stringeken és zárójeleken kívüli) `sep` mentén (PHP-forrás).
pub(crate) fn split_top<'a>(text: &'a str, sep: &str) -> Vec<&'a str> {
    srcscan::split_top(text, sep, Dialect::Php)
}
