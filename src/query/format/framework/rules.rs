//! A framework-nézetek gyűjtői (a callgraph.sh azonos nevű jq-ágainak mása, szekciónként azonos sorrendben).

use super::{at_line, call_site, quoted_line, re, resolved_or_target, string_site};
use crate::model::{Call, FileGraph};

/// A nézetek hívásai: a hivatkozás (`handler`, `X::class`, függvény értékként) nem hívás — nem dispatch, nem
/// regisztráció (`belongsTo(Event::class)` ne legyen `dispatch`).
fn calls(g: &FileGraph) -> impl Iterator<Item = &Call> {
    g.calls.iter().filter(|c| !c.is_reference())
}

pub(super) fn events(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let arrow = |label: &str, pattern: &regex::Regex, out: &mut Vec<String>| {
        for c in calls(g).filter(|c| pattern.is_match(&c.target)) {
            out.push(format!(
                "{label}{}  -->  {}  L{}",
                call_site(ns, c),
                c.target,
                c.line
            ));
        }
    };
    // A `Call.target` nem tartalmaz zárójelet; a minták a metódusnévre horgonyoznak (a `(?i)on[A-Z]` korábban
    // minden `config` / `response` hívásra illett).
    arrow(
        "dispatch  ",
        re!(r"(?i)Event::dispatch|(?:^|::|->|\.)(?:event|dispatch)$"),
        out,
    );
    arrow(
        "listen    ",
        re!(r"(?i)Event::listen|attachListener|(?:^|::|->|\.)on$"),
        out,
    );
    arrow("event     ", re!(r"(?:^|::|->|\.)on[A-Z]"), out);
    arrow("off       ", re!(r"(?:^|::|->|\.)off[A-Z]"), out);
    arrow(
        "handler   ",
        re!(r"(?i)attachEventHandler|detachEventHandler|hasEventHandler"),
        out,
    );
    for c in calls(g).filter(|c| c.target == "trigger") {
        out.push(format!(
            "trigger   {}  -->  trigger()  L{}",
            call_site(ns, c),
            c.line
        ));
    }
    let pattern = re!(r"(?i)Event|event|Listener|listener|EVENT_");
    for s in g.strings.iter().filter(|s| pattern.is_match(&s.value)) {
        out.push(format!(
            "string    {}  \"{}\"  L{}",
            string_site(ns, s),
            s.value,
            s.line
        ));
    }
}

pub(super) fn routes(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    super::routes::collect(g, ns, out);
    let (start, path) = (re!(r"^/|^[A-Z]"), re!(r"(?i)/[a-zA-Z]|api|route"));
    for s in g
        .strings
        .iter()
        .filter(|s| start.is_match(&s.value) && path.is_match(&s.value))
    {
        out.push(quoted_line("path      ", ns, s));
    }
}

pub(super) fn middleware(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    // a metódusnévre horgonyozva: `middleware`, `prependMiddleware`, `pushMiddleware`, `aliasMiddleware`, …
    let call = re!(r"(?i)middleware$");
    for c in calls(g).filter(|c| call.is_match(&c.target)) {
        out.push(at_line("middleware  ", &c.target, ns, c));
    }
    // osztály-sor: csak a `…Middleware` nevű osztály hívásai (a névtér-szegmens nem számít)
    let class_call = re!(r"(?i)(?:^|\\)\w*Middleware::");
    for c in calls(g).filter(|c| class_call.is_match(resolved_or_target(c))) {
        out.push(at_line("class     ", resolved_or_target(c), ns, c));
    }
    let class = re!(r"(?i)Middleware|middleware");
    for s in g.strings.iter().filter(|s| class.is_match(&s.value)) {
        out.push(quoted_line("string    ", ns, s));
    }
}

pub(super) fn validation(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    // metódusnévre horgonyozva (az `invalidate` nem `validate`)
    let call = re!(
        r"(?i)Validator::make|(?:^|::|->|\.)(?:validate\w*|rules|withValidator|setRules|addError)$"
    );
    for c in calls(g).filter(|c| call.is_match(&c.target)) {
        out.push(at_line("validate  ", &c.target, ns, c));
    }
    for s in g.strings.iter().filter(|s| is_rule_string(&s.value)) {
        out.push(quoted_line("rule      ", ns, s));
    }
}

/// Paraméter nélküli validációs szabályok (`'required'`, `'nullable'`, …).
const PLAIN_RULES: [&str; 28] = [
    "required",
    "nullable",
    "sometimes",
    "present",
    "filled",
    "email",
    "string",
    "integer",
    "numeric",
    "boolean",
    "array",
    "date",
    "url",
    "uuid",
    "json",
    "file",
    "image",
    "confirmed",
    "accepted",
    "alpha",
    "alpha_num",
    "alpha_dash",
    "timezone",
    "ip",
    "bail",
    "safe",
    "unique",
    "exists",
];

/// Paraméteres szabályok (`'max:120'`, `'in:a,b'`, …): a `:` kötelező.
const ARG_RULES: [&str; 22] = [
    "min",
    "max",
    "size",
    "between",
    "digits",
    "in",
    "not_in",
    "exists",
    "unique",
    "regex",
    "after",
    "before",
    "after_or_equal",
    "before_or_equal",
    "date_format",
    "same",
    "different",
    "mimes",
    "mimetypes",
    "required_if",
    "required_with",
    "gt",
];

/// A string érvényes szabály-lista-e: a `|`-jelekkel tagolt részek mindegyike ismert szabály (a `passwords.updated`,
/// az artisan-szignatúra és a mondatok nem azok).
fn is_rule_string(value: &str) -> bool {
    !value.is_empty()
        && value.split('|').all(|part| match part.split_once(':') {
            Some((name, _)) => ARG_RULES.contains(&name),
            None => PLAIN_RULES.contains(&part),
        })
}

/// Eltérés a callgraph-tól: ott a `later\\(` minta bezáratlan zárójelet ad, a jq hibával kilép, és mindig „nincs találat” lesz;
/// itt a szándékolt `later\(` szerepel.
pub(super) fn jobs(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let call = re!(r"(?i)Queue::push|(?:^|::|->|\.)(?:dispatch\w*|later|pushOn)$");
    for c in calls(g).filter(|c| call.is_match(&c.target)) {
        out.push(at_line("dispatch  ", &c.target, ns, c));
    }
    // a `Handle` nem minta: az `EnsureReaderCard::handle` nem job
    let class = re!(r"(?i)\\Jobs\\|(?:^|\\)\w*(?:Job|Queue)\w*::");
    for c in calls(g).filter(|c| class.is_match(resolved_or_target(c))) {
        out.push(at_line("job       ", resolved_or_target(c), ns, c));
    }
    let string = re!(r"(?i)queue|Queue|job|Job");
    for s in g.strings.iter().filter(|s| string.is_match(&s.value)) {
        out.push(quoted_line("string    ", ns, s));
    }
}

pub(super) fn providers(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let call = re!(
        r"(?i)(?:^|::|->|\.)(?:singleton|scoped|bind|instance|register|boot|loadMigrationsFrom|loadRoutesFrom|publishes|publish|mergeConfigFrom|loadTranslationsFrom|loadViewsFrom)$"
    );
    // Laravelben a regisztráló hívás (`bind`, `boot`, `register` …) csak a service provider osztályokban számít: máshol
    // (egy modell `boot`-ja, egy lekérdezés `bind`-ja) nem szolgáltatás-regisztráció
    let laravel = g.framework.as_deref() == Some("laravel");
    let in_provider = |c: &Call| {
        !laravel
            || g.file.contains("Providers")
            || g.file.ends_with("bootstrap/app.php")
            || c.container.as_deref().is_some_and(|name| {
                name.ends_with("Provider")
                    || g.containers.iter().any(|container| {
                        container.name == name
                            && container
                                .extends
                                .iter()
                                .flatten()
                                .any(|parent| parent.ends_with("ServiceProvider"))
                    })
            })
    };
    for c in calls(g).filter(|c| call.is_match(&c.target) && in_provider(c)) {
        out.push(at_line("provider  ", &c.target, ns, c));
    }
    let class = re!(r"(?i)(?:^|\\)\w*Provider::");
    for c in calls(g).filter(|c| class.is_match(resolved_or_target(c))) {
        out.push(at_line("class     ", resolved_or_target(c), ns, c));
    }
}

pub(super) fn magic(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let (name, delegates) = (re!(r"^__[a-z]"), re!(r"forwardCallTo|__callStatic"));
    for m in g.methods.iter().filter(|m| name.is_match(&m.name)) {
        let tag = match &m.body {
            Some(body) if delegates.is_match(body) => "  [delegates]",
            _ => "",
        };
        let container = m.container.as_deref().unwrap_or("");
        out.push(format!(
            "magic     {}  in  {ns}{container}  L{}{tag}",
            m.name, m.line
        ));
    }
}

pub(super) fn di(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let caller = |c: &crate::model::Call| c.caller.clone().unwrap_or_default();
    let component = re!(r"(?i)components|behaviors|container");
    for c in calls(g).filter(|c| component.is_match(&c.target)) {
        out.push(at_line("di        ", &c.target, ns, c));
    }
    let (bind, in_provider) = (
        re!(r"(?i)(?:^|::|->|\.)(?:singleton|scoped|bind|extend|alias)$"),
        re!(r"(?i)^(?:register|boot)"),
    );
    for c in calls(g).filter(|c| bind.is_match(&c.target) && in_provider.is_match(&caller(c))) {
        out.push(at_line("di        ", &c.target, ns, c));
    }
    let factory = re!(r"(?i)^(?:register|boot|create|resolve|get)");
    for c in calls(g).filter(|c| c.kind == "new" && factory.is_match(&caller(c))) {
        out.push(at_line("new       ", &c.target, ns, c));
    }
}

pub(super) fn views(g: &FileGraph, _ns: &str, out: &mut Vec<String>) {
    if !re!(r"(?i)views|template|view|blade|widget").is_match(&g.file) {
        return;
    }
    let line = |label: &str, text: &str, at: u32| format!("{label}{text}  @  {}  L{at}", g.file);
    // Blade-sablon: a nézet-használat (`@extends`, `@include`, `<x-…>`) a valódi gráfból
    if g.file.to_ascii_lowercase().ends_with(".blade.php") {
        for c in calls(g).filter(|c| matches!(c.category.as_str(), "view_use" | "component_use")) {
            out.push(line(
                "view      ",
                &format!("{} {}", c.kind, c.target),
                c.line,
            ));
        }
        return;
    }
    for c in calls(g).filter(|c| c.kind == "method_call" || c.kind == "static_call") {
        out.push(line("view      ", &c.target, c.line));
    }
    let widget = re!(r"(?i)Yii::|Html::|ActiveForm|GridView|DetailView|LinkPager");
    for c in calls(g).filter(|c| widget.is_match(&c.target)) {
        out.push(line("widget    ", &c.target, c.line));
    }
    let variable = re!(r"\$[a-z]");
    for c in calls(g)
        .filter(|c| c.kind == "method_call" && variable.is_match(c.on.as_deref().unwrap_or("")))
    {
        let on = c.on.as_deref().unwrap_or("");
        out.push(line("property  ", &format!("{on}->{}", c.target), c.line));
    }
}
