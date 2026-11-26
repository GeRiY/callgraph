//! `outline` formázó (a callgraph alapnézete).

use crate::i18n::Lang;
use crate::model::{Call, FileGraph, Method};
use crate::notes::NoteMarks;
use crate::query::QueryResult;
use crate::query::snippet::{self, SourceLines};
use std::fmt::Write as _;

use super::Formatter;

pub struct Outline;
impl Formatter for Outline {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        render(rows, lang, None, None, false)
    }
}

/// A `--grep` outline: mint az `outline`, plusz metódusonként a találat-hívások helye és forrássora
/// (`L25 │ $this->run($x)`, interfészen át illesztettnél `L25 via Resolver │ …`).
pub struct GrepOutline;
impl Formatter for GrepOutline {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        grep_outline(rows, lang, &SourceLines::default(), None)
    }
}

/// A `--grep` outline a megadott forrássor-tárral (a projekt-gráf tárolt hívás-soraival előtöltve is lehet);
/// `marks`: a jegyzet-jelölők (`  📝 N`) a `FILE` és a metódus-sorok végén.
pub(crate) fn grep_outline(
    rows: &[QueryResult],
    lang: Lang,
    source: &SourceLines,
    marks: Option<&NoteMarks>,
) -> String {
    render(rows, lang, Some(source), marks, false)
}

fn render(
    rows: &[QueryResult],
    lang: Lang,
    source: Option<&SourceLines>,
    marks: Option<&NoteMarks>,
    compact: bool,
) -> String {
    // the `FILE` header delimits the files: no separator, no closing tip
    rows.iter()
        .map(|row| outline(row, lang, source, marks, compact))
        .collect::<Vec<_>>()
        .concat()
}

/// Ennyi teszt-függvény fölött a csupa tesztes konténer tagjai egy `N tests` sorba vonódnak.
const COLLAPSE_TESTS: usize = 3;

fn outline(
    row: &QueryResult,
    lang: Lang,
    source: Option<&SourceLines>,
    marks: Option<&NoteMarks>,
    compact: bool,
) -> String {
    let g = &row.graph;
    let notes: &[crate::notes::Note] = marks.map_or(&[], |marks| marks.of(&g.file));
    let mut s = String::new();
    writeln!(
        s,
        "FILE  {}{}{}{}",
        g.file,
        g.namespace
            .as_ref()
            .map(|n| format!("  NS:{n}"))
            .unwrap_or_default(),
        crate::notes::mark(crate::notes::file_count(notes)),
        crate::io::protect_mark(&g.file)
    )
    .unwrap();
    // szintaktikai hiba: közvetlenül a FILE sor alatt (hiba nélkül nincs sor)
    s.push_str(&super::parse_error_lines(g, lang));
    // Inertia-oldal: egy sor a render-helyekkel (kötések / render-hely nélkül nincs sor)
    if let Some(by) = row.inertia.as_ref().and_then(|i| i.rendered_by(&g.file)) {
        writeln!(s, "{}{by}", lang.pick("rendered by: ", "renderelő: ")).unwrap();
    }
    // nyelvfájl / stíluslap: a kapcsolatok összegzése (nincs mit mondani: nincs sor)
    if let Some(text) = row.crossref_header(lang) {
        writeln!(s, "{text}").unwrap();
    }
    // Livewire: a component class's template and uses, a template's class and `wire:` findings
    for text in row.livewire_header(lang) {
        writeln!(s, "{text}").unwrap();
    }
    // jegyzetek (PILOT): fájl-jegyzetek, és a nem listázott (konténer nélküli) függvényeiké
    crate::notes::file_lines(&mut s, &row.notes);
    crate::notes::unlisted_lines(&mut s, &row.notes, g);
    if g.containers.is_empty() {
        super::outline_procedural::procedural(row, &mut s, lang, source);
        return s;
    }
    // a Vue `defineProps<X>()` interfésze nem külön konténer (a komponens props-listája ugyanezt mondja)
    let visible = g
        .containers
        .iter()
        .filter(|c| !super::outline_members::is_props_duplicate(g, c));
    for c in visible {
        let extends = c
            .extends
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" extends {}", e.join(", ")))
            .unwrap_or_default();
        let implements = c
            .implements
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" implements {}", e.join(", ")))
            .unwrap_or_default();
        let traits = c
            .traits
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" uses {}", e.join(", ")))
            .unwrap_or_default();
        let mut methods: Vec<_> = g
            .methods
            .iter()
            .filter(|m| g.owns(c, m.container.as_deref(), m.line))
            .filter(|m| row.methods.as_ref().is_none_or(|ms| ms.contains(&m.name)))
            .collect();
        methods.sort_by_key(|m| m.line);
        // csupa teszt-függvényű konténer (Rust `mod tests`): `[tests]` jelölés; kijelölés nélkül, ha sok a teszt,
        // a tagok egy `N tests` sorba vonva
        let test_module =
            !methods.is_empty() && methods.iter().all(|m| m.kind.as_deref() == Some("test"));
        let collapsed = test_module && row.methods.is_none() && methods.len() >= COLLAPSE_TESTS;
        let tests_mark = if test_module { " [tests]" } else { "" };
        writeln!(
            s,
            "{} {}{}{}{}{tests_mark}  L{}-{}",
            c.kind, c.name, extends, implements, traits, c.line, c.end_line
        )
        .unwrap();
        // Eloquent-modell: a tábla oszlopai a migrációkból, fillable / casts, figyelmeztetések (nem modellnél nincs sor)
        if let Some(laravel) = row.laravel.as_ref() {
            for line in laravel.model_lines(g.namespace.as_deref(), &c.name, lang) {
                writeln!(s, "  {line}").unwrap();
            }
        }
        // package.json / composer.json: a függőség sorának végén a lockban zárolt verzió
        let locks = manifest_locks(row);
        super::outline_members::constants(&mut s, row, c, locks.as_ref(), compact);
        super::outline_members::properties(&mut s, g, c, compact);
        if collapsed {
            if let (Some(first), Some(last)) = (methods.first(), methods.last()) {
                let n = methods.len();
                writeln!(
                    s,
                    "  {}  L{}-{}",
                    ltf!(lang, "{n} tests", "{n} teszt"),
                    first.line,
                    last.end_line.unwrap_or(last.line)
                )
                .unwrap();
            }
            continue;
        }
        for m in methods {
            method_line(&mut s, m);
            // React: a custom hook is marked, a function component lists its props
            if super::outline_members::is_react_hook(g, m) {
                s.pop();
                s.push_str(" [hook]\n");
            }
            if let Some(props) = super::outline_members::component_props(g, m) {
                writeln!(s, "    {props}").unwrap();
            }
            if !notes.is_empty() {
                let n = crate::notes::method_count(notes, m.container.as_deref(), &m.name);
                if n > 0 {
                    // a metódus-sor végére (a sorvége elé)
                    s.pop();
                    s.push_str(&crate::notes::mark(n));
                    s.push('\n');
                }
            }
            crate::notes::method_lines(&mut s, &row.notes, m.container.as_deref(), &m.name, "    ");
            // Laravel route-akció: egy sor a route-okkal (route-tábla / route nélkül nincs sor)
            if let Some(route) = row.laravel.as_ref().and_then(|laravel| {
                laravel.route_line(g.namespace.as_deref(), m.container.as_deref()?, &m.name)
            }) {
                writeln!(s, "    {route}").unwrap();
            }
            // Inertia-űrlapok: a controller-akció alatt a küldő page-fájlok, a küldő függvény alatt az
            // űrlap-szerződés egy sora (kötések / űrlap nélkül nincs sor)
            if let Some(laravel) = row.laravel.as_ref() {
                if let Some(form) = m
                    .container
                    .as_deref()
                    .and_then(|c| laravel.form_line(g.namespace.as_deref(), c, &m.name))
                {
                    writeln!(s, "    {form}").unwrap();
                }
                for form in laravel.form_lines(&g.file, m.container.as_deref(), &m.name, lang) {
                    writeln!(s, "    {form}").unwrap();
                }
            }
            for (category, list) in categories(row, m, lang) {
                let list = if compact { cap_list(&list) } else { list };
                // Eloquent-reláció: a cél az `orm_relation` sor végén (`belongsToMany → Role`)
                let target = row
                    .laravel
                    .as_ref()
                    .filter(|_| category == "orm_relation")
                    .and_then(|laravel| {
                        laravel.relation_target(
                            g.namespace.as_deref(),
                            m.container.as_deref()?,
                            &m.name,
                        )
                    });
                match target {
                    Some(target) => writeln!(s, "    {category} {list} → {target}"),
                    None => writeln!(s, "    {category} {list}"),
                }
                .unwrap();
            }
            if let Some(lines) = source {
                call_lines(&mut s, row, &method_calls(g, m), lines, lang, "    ");
            }
            // kompakt alakban a megjegyzések (💬) elmaradnak
            for comment in g
                .comments
                .iter()
                .filter(|_| !compact)
                .filter(|comment| comment.caller.as_deref() == Some(&m.name))
                .filter(|comment| comment.container.as_deref() == m.container.as_deref())
            {
                match &comment.tag {
                    Some(tag) => writeln!(s, "    💬 [{tag}] {}", comment.text),
                    None => writeln!(s, "    💬 {}", comment.text),
                }
                .unwrap();
            }
        }
    }
    s
}
/// A kompakt alakban egy hívás-lista legfeljebb ennyi célt mutat (a többi `… +N`).
const COMPACT_TARGETS: usize = 8;

/// A hívás-lista (`a, b, c`) első `COMPACT_TARGETS` eleme, a többi `… +N`.
fn cap_list(list: &str) -> String {
    let parts: Vec<&str> = list.split(", ").collect();
    if parts.len() <= COMPACT_TARGETS + 2 {
        return list.to_owned();
    }
    format!(
        "{}, … +{}",
        parts[..COMPACT_TARGETS].join(", "),
        parts.len() - COMPACT_TARGETS
    )
}

/// Az `outline` kompakt alakja (a `--limit` fölötti kimenet első lépcsője, a metódus-index előtt): megjegyzések
/// nélkül, a hosszú tag-listák tördelve, a hívás-listák legfeljebb nyolc céllal (`… +N`). Minden tag megmarad.
pub(crate) fn compact_outline(rows: &[QueryResult], lang: Lang) -> String {
    render(rows, lang, None, None, true)
}

/// A Markdown-címsor szintje (a szakasz `visibility`-je `h1` … `h6`); más tagnál `None`.
fn heading_level(m: &Method) -> Option<usize> {
    let digits = m.visibility.as_deref()?.strip_prefix('h')?;
    let level = digits.parse::<usize>().ok()?;
    (1..=6).contains(&level).then_some(level)
}

/// A tag neve után `()`, ha hívható; a CSS-szabály / at-szabály, a JSON-kulcs, a Markdown- / HTML-címsor és a
/// HTML `<title>` nem az.
pub(crate) fn parens(m: &Method) -> &'static str {
    let plain = matches!(
        m.kind.as_deref(),
        Some("rule" | "at-rule" | "key" | "title" | "import")
    ) || m.kind.as_deref() == Some(crate::lang::blade::BLOCK_KIND)
        || heading_level(m).is_some();
    if plain { "" } else { "()" }
}

pub(super) fn method_line(s: &mut String, m: &Method) {
    // HTML belépő-import (`<script src>`, `<link href>`): `import /src/main.ts  L5-5`
    if m.kind.as_deref() == Some(crate::lang::html::IMPORT_KIND) {
        writeln!(
            s,
            "  import {}  L{}-{}",
            m.name,
            m.line,
            m.end_line.unwrap_or(m.line)
        )
        .unwrap();
        return;
    }
    // a HTML `<title>`: a címsorok előtt, saját jelöléssel (`<title> Tervdokumentum  L4-4`)
    if m.kind.as_deref() == Some(crate::lang::html::TITLE_KIND) {
        let end = m.end_line.unwrap_or(m.line);
        writeln!(s, "  <title> {}  L{}-{end}", m.name, m.line).unwrap();
        return;
    }
    // a címsor a szintje szerint behúzva, `#`-jelekkel (`## Docker / build`)
    if let Some(level) = heading_level(m) {
        let indent = "  ".repeat(level - 1);
        let hashes = "#".repeat(level);
        let end = m.end_line.unwrap_or(m.line);
        writeln!(s, "  {indent}{hashes} {}  L{}-{end}", m.name, m.line).unwrap();
        return;
    }
    // a modul-szintű pszeudo-metódus nem hívható cél: rövid címke, `()` nélkül (a hívásai megmaradnak a gráfban)
    if m.name == "__module__" {
        writeln!(
            s,
            "  module-level code  L{}-{}",
            m.line,
            m.end_line.unwrap_or(m.line)
        )
        .unwrap();
        return;
    }
    // `--signatures`: a hívható tagnál `name(params): ret` a `name()` helyett (ha nincs adat, ugyanaz marad)
    let label = if crate::cli::signatures::enabled() && !parens(m).is_empty() {
        super::signature::signature(m)
    } else {
        format!("{}{}", m.name, parens(m))
    };
    let test = if m.kind.as_deref() == Some("test") {
        " [test]"
    } else {
        ""
    };
    writeln!(
        s,
        "  {label}  L{}-{}{}{}{test}",
        m.line,
        m.end_line.unwrap_or(m.line),
        if m.is_static { " [static]" } else { "" },
        if m.is_abstract { " [abstract]" } else { "" }
    )
    .unwrap();
}
/// A metódus hívásai (a `categories` szűrőjével).
pub(crate) fn method_calls<'g>(g: &'g FileGraph, m: &Method) -> Vec<&'g Call> {
    g.calls
        .iter()
        .filter(|c| c.caller.as_deref() == Some(&m.name))
        .filter(|c| c.container.as_deref() == m.container.as_deref())
        .collect()
}

/// A hívás kiírt sora: a metódus- / statikus hívás láncban a `->név(` sora (az azonos kezdősorú, azonos nevű hívások
/// sorban egymás utáni előfordulást kapnak); minden más hívásé a tárolt sor.
pub(crate) fn shown_line(source: &SourceLines, file: &str, calls: &[&Call], call: &Call) -> u32 {
    if !matches!(
        call.kind.as_str(),
        "method_call" | "nullsafe_method_call" | "static_call"
    ) {
        return call.line;
    }
    let name = call
        .target
        .rsplit(['>', ':', '.'])
        .next()
        .unwrap_or(&call.target);
    let nth = calls
        .iter()
        .take_while(|c| !std::ptr::eq(**c, call))
        .filter(|c| c.line == call.line && c.kind == call.kind && c.target == call.target)
        .count();
    source.call_line(file, call.line, name, nth)
}

/// A hívások helye soronként egyszer (`L25 [via X] │ kód`); az azonos (és azonos `via`-jú) sorok egy sorba vonva
/// (`L12, L40 │ kód`), legfeljebb `snippet::MAX_PER_METHOD` sor, a többi `… +N more: L55, L61`.
pub(crate) fn call_lines(
    s: &mut String,
    row: &QueryResult,
    calls: &[&Call],
    source: &SourceLines,
    lang: Lang,
    indent: &str,
) {
    // a láncolt hívás sora a hívás tagjáé, nem a lánc kezdősoráé
    let located: Vec<(u32, &Call)> = calls
        .iter()
        .map(|&c| (shown_line(source, &row.graph.file, calls, c), c))
        .collect();
    let mut lines: Vec<u32> = located.iter().map(|(line, _)| *line).collect();
    lines.sort_unstable();
    lines.dedup();
    let sites = snippet::merge(lines.iter().map(|&line| {
        let at = || located.iter().filter(|(l, _)| *l == line).map(|(_, c)| *c);
        let via = at().find_map(|c| row.via_of(c));
        // csak hivatkozásokat tartalmazó sor `(ref)`: a sor többi hívása hívás marad
        let reference = at().all(Call::is_reference);
        (line, (via, reference), source.get(&row.graph.file, line))
    }));
    let (shown, rest) = snippet::cap(sites);
    for site in &shown {
        writeln!(
            s,
            "{indent}{}",
            snippet::sites_line_ref(
                lang,
                &site.lines,
                site.key.1,
                site.key.0,
                site.code.as_deref()
            )
        )
        .unwrap();
    }
    if !rest.is_empty() {
        writeln!(s, "{indent}{}", snippet::more(lang, &rest)).unwrap();
    }
}

/// A metódus hívásai kategóriánként (`external_call`, `a, b`); a csomaghoz rendelt külső hívások külön sorban, a
/// csomag rövid címkéjével a célok előtt (`guzzle: GuzzleHttp\Client::request` — `package_label`).
/// Egy hívás-csoport kulcsa: (kategória, a csomag rövid címkéje).
type CategoryKey = (String, Option<String>);

pub(super) fn categories(row: &QueryResult, m: &Method, lang: Lang) -> Vec<(String, String)> {
    let g = &row.graph;
    let mut grouped: std::collections::BTreeMap<CategoryKey, Vec<String>> =
        std::collections::BTreeMap::new();
    for c in g
        .calls
        .iter()
        .filter(|c| c.caller.as_deref() == Some(&m.name))
        .filter(|c| c.container.as_deref() == m.container.as_deref())
        // a HTML belépő-import sora már a metódus-sor (`import /src/main.ts`)
        .filter(|c| c.kind != crate::lang::html::IMPORT_KIND)
    {
        // wire / inject / nav: a hívás-fajta a kategória-oszlopban (`wire RealtimeSocket::onFrame`)
        let column = if matches!(c.kind.as_str(), "wire" | "inject" | "nav") {
            c.kind.clone()
        } else if c.kind == "const_fetch" && c.category == "external_call" {
            // az enum-eset / osztály-konstans olvasása nem hívás: külön sor (a json kategóriája változatlan)
            "const_use".to_owned()
        } else {
            c.category.clone()
        };
        let mut target = shown_target(row, c, lang);
        if c.is_reference() {
            target.push_str(snippet::REF_MARK);
        }
        grouped
            .entry((column, package_label(row, c)))
            .or_default()
            .push(target);
    }
    let mut ordered: Vec<(CategoryKey, Vec<String>)> = grouped.into_iter().collect();
    // a `dynamic_call` a lista végére (a rendezés stabil: a többi marad ábécében)
    ordered.sort_by_key(|((category, _), _)| crate::model::category_rank(category));
    ordered
        .into_iter()
        .map(|((category, label), targets)| {
            // az azonos cél egyszer; dokumentum-linknél a darabszámmal (`docs/install.md ×3`)
            let mut unique: Vec<(String, usize)> = Vec::new();
            for target in targets {
                match unique.iter_mut().find(|(seen, _)| *seen == target) {
                    Some((_, count)) => *count += 1,
                    None => unique.push((target, 1)),
                }
            }
            unique.sort();
            let counted = matches!(
                category.as_str(),
                "anchor" | "external_url" | "internal_file" | "image"
            );
            let items: Vec<String> = unique
                .iter()
                .map(|(target, count)| {
                    if counted && *count > 1 {
                        format!("{target} ×{count}")
                    } else {
                        target.clone()
                    }
                })
                .collect();
            let list = if category == "component_use" {
                factor_paths(&items)
            } else {
                items.join(", ")
            };
            let list = match label {
                Some(label) => format!("{label}: {list}"),
                None => list,
            };
            (category, list)
        })
        .collect()
}

/// A komponens-utak közös mappa-előtagja kiemelve: `ui/src/components/{atoms/A, organisms/B}.vue`, ha minden
/// elem `<mappa>/Név/Név.vue` alakú (a kapcsos zárójel utáni `.vue` azt jelenti: minden `X` az `X/X.vue`); egyéb
/// esetben `ui/src/components/{atoms/A/A.vue, b.vue}` (teljes fájlnevek). Három elem alatt, vagy ha nincs közös
/// mappa, a lista változatlan.
fn factor_paths(items: &[String]) -> String {
    let plain = || items.join(", ");
    if items.len() < 3
        || items
            .iter()
            .any(|item| item.contains([' ', '{', '}', ',', '\\']))
    {
        return plain();
    }
    let parts: Vec<Vec<&str>> = items.iter().map(|item| item.split('/').collect()).collect();
    let shortest = parts.iter().map(|p| p.len() - 1).min().unwrap_or(0);
    let common = (0..shortest)
        .take_while(|&i| parts.iter().all(|p| p[i] == parts[0][i]))
        .count();
    if common == 0 {
        return plain();
    }
    let prefix = parts[0][..common].join("/");
    // minden elem `…/Név/Név.ext` és azonos kiterjesztésű?
    let doubled = |p: &Vec<&str>| -> Option<(String, String)> {
        let (stem, ext) = p[p.len() - 1].rsplit_once('.')?;
        let dir_at = p.len().checked_sub(2).filter(|at| *at >= common)?;
        (p[dir_at] == stem).then(|| (p[common..p.len() - 1].join("/"), ext.to_owned()))
    };
    let collapsed: Option<Vec<(String, String)>> = parts.iter().map(doubled).collect();
    if let Some(collapsed) = collapsed
        && collapsed.iter().all(|(_, ext)| *ext == collapsed[0].1)
    {
        let names: Vec<&str> = collapsed.iter().map(|(name, _)| name.as_str()).collect();
        return format!("{prefix}/{{{}}}.{}", names.join(", "), collapsed[0].1);
    }
    let rest: Vec<String> = parts.iter().map(|p| p[common..].join("/")).collect();
    format!("{prefix}/{{{}}}", rest.join(", "))
}

/// Go: a feloldott `csomag\Típus::metódus` cél rövid alakja a kimenetben — a csomag-előtag nélkül (`Command::Flags`),
/// és a hívó saját típusának metódusa (`internal_helper`) csak a metódus neve (`Flags`), mint a PHP `$this->x`-nél.
/// A kulcs (a hívó-keresés) a gráfban a teljes marad.
fn short_go_target(row: &QueryResult, call: &Call) -> Option<String> {
    if !row.graph.file.ends_with(".go") {
        return None;
    }
    let resolved = call.resolved.as_deref()?;
    let rest = match row.graph.namespace.as_deref() {
        Some(ns) if !ns.is_empty() => resolved.strip_prefix(&format!("{ns}\\"))?,
        _ => resolved,
    };
    let (owner, method) = rest.split_once("::")?;
    if call.category == "internal_helper" && call.container.as_deref() == Some(owner) {
        return Some(method.to_owned());
    }
    Some(rest.to_owned())
}

/// A hívás kiírt célja: a feloldott (különben a nyers) cél; Inertia render-hívásnál utána a page-fájl rövid alakja
/// (`Inertia\Inertia::render → Pages/Board.vue`, nem létező fájlnál `(missing)`); Ziggy `route('név')` hívásnál a
/// route linkje (`route::register → POST /register · RegisteredUserController@store`). Kötések nélkül a régi.
pub(super) fn shown_target(row: &QueryResult, call: &Call, lang: Lang) -> String {
    let target = short_go_target(row, call)
        .unwrap_or_else(|| call.resolved.as_deref().unwrap_or(&call.target).to_owned());
    let target = target.as_str();
    // Ziggy `route('név')` hívásnál a Laravel route-tábla linkje (` → POST /uri · X@m`, vagy ` ⚠ no route`)
    if let Some(link) = row.route_link(call, lang) {
        return format!("{target}{link}");
    }
    // `$t('a.b')`: a kulcs hiánya a nyelvfájlokból (` ⚠ missing in hu.json` / ` ⚠ no key`)
    if let Some(link) = row.i18n_link(call, lang) {
        return format!("{target}{link}");
    }
    // dokumentum-link: a nem létező fájl / saját címsor ` ⚠ missing`
    if link_missing(&row.graph, call) {
        return format!("{target}{}", lang.pick(" ⚠ missing", " ⚠ hiányzik"));
    }
    match row.inertia_page(call) {
        Some(page) if page.exists => format!("{target} → {}", page.short),
        Some(page) => format!(
            "{target} → {}{}",
            page.short,
            lang.pick(" (missing)", " (hiányzik)")
        ),
        None => target.to_owned(),
    }
}

/// A `package.json` / `composer.json` fájl függőségeinek zárolt verziója (név → verzió) a csomagtérképből; más
/// fájlnál, vagy csomagtérkép nélkül `None`.
fn manifest_locks(row: &QueryResult) -> Option<std::collections::HashMap<String, String>> {
    let ecosystem = match std::path::Path::new(&row.graph.file)
        .file_name()
        .and_then(|name| name.to_str())?
    {
        "package.json" => "node",
        "composer.json" => "php",
        _ => return None,
    };
    let packages = row.packages.as_ref()?;
    let locks: std::collections::HashMap<String, String> = packages
        .all_direct()
        .into_iter()
        .filter(|package| package.ecosystem.label() == ecosystem)
        .filter_map(|package| Some((package.name.clone(), package.installed.clone()?)))
        .collect();
    (!locks.is_empty()).then_some(locks)
}

/// A függőség-kulcs (`dependencies.vue`, `require.laravel/framework`) zárolt verziója.
pub(super) fn locked_version<'l>(
    locks: &'l std::collections::HashMap<String, String>,
    key: &crate::model::Constant,
) -> Option<&'l str> {
    if key.kind != "key" {
        return None;
    }
    let (section, name) = key.name.split_once('.')?;
    matches!(
        section,
        "dependencies" | "devDependencies" | "peerDependencies" | "require" | "require-dev"
    )
    .then(|| locks.get(name).map(String::as_str))
    .flatten()
}

/// Hiányzik-e a dokumentum-link célja: a relatív fájl nem létezik, vagy a Markdown `#horgony` nem egyezik a fájl
/// egyik címsorának slug-jával sem. Az ellenőrizhetetlen cél (abszolút út, séma, sablon, `%`-kódolás, HTML-id)
/// nem hiányzik.
fn link_missing(g: &FileGraph, call: &Call) -> bool {
    let path = std::path::Path::new(&g.file);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let target = call.target.as_str();
    if target.contains(['%', '{', '<', '$']) {
        return false;
    }
    match call.category.as_str() {
        "internal_file" if matches!(extension, "md" | "html" | "htm") => {
            let file = target.split(['#', '?']).next().unwrap_or_default();
            if file.is_empty() || file.starts_with('/') || file.contains(':') {
                return false;
            }
            path.parent().is_some_and(|dir| !dir.join(file).exists())
        }
        "anchor" if extension == "md" => {
            let wanted = target.trim_start_matches('#').to_lowercase();
            !wanted.is_empty() && !heading_slugs(g).contains(&wanted)
        }
        _ => false,
    }
}

/// A fájl címsorainak GitHub-stílusú horgony-neve (kisbetű, szóköz → `-`, írásjel nélkül; az ismétlődőé `-1`, `-2`…).
fn heading_slugs(g: &FileGraph) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    for m in g.methods.iter().filter(|m| heading_level(m).is_some()) {
        let base: String = m
            .name
            .trim()
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                c if c.is_alphanumeric() || c == '_' || c == '-' => Some(c),
                c if c.is_whitespace() => Some('-'),
                _ => None,
            })
            .collect();
        let same = slugs
            .iter()
            .filter(|s| **s == base || is_numbered(s, &base))
            .count();
        slugs.push(if same == 0 {
            base
        } else {
            format!("{base}-{same}")
        });
    }
    slugs
}

/// A `slug` a `base-<szám>` alak (az ismétlődő címsor sorszámozott horgonya).
fn is_numbered(slug: &str, base: &str) -> bool {
    slug.strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A hívás csomag-címkéje az outline-ban (`guzzle`): csak a csomaghoz rendelt `external_call` / `framework_use`
/// hívásnál, és a `framework_use`-nál nem, ha a csomag maga a framework (`laravel/framework`: a kategória már
/// mondja). Csomagtérkép nélkül mindig `None` (a kimenet a régi).
pub(super) fn package_label(row: &QueryResult, call: &Call) -> Option<String> {
    if !matches!(call.category.as_str(), "external_call" | "framework_use") {
        return None;
    }
    let package = row.package_of(call)?;
    if call.category == "framework_use" && crate::packages::is_framework(&package.name) {
        return None;
    }
    Some(package.label().to_owned())
}
