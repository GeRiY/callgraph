//! A framework-nézetek gyűjtői (a callgraph.sh azonos nevű jq-ágainak mása, szekciónként azonos sorrendben).

use super::{at_line, call_site, quoted_line, re, resolved_or_target, string_site};
use crate::model::FileGraph;

pub(super) fn events(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let arrow = |label: &str, pattern: &regex::Regex, out: &mut Vec<String>| {
        for c in g.calls.iter().filter(|c| pattern.is_match(&c.target)) {
            out.push(format!(
                "{label}{}  -->  {}  L{}",
                call_site(ns, c),
                c.target,
                c.line
            ));
        }
    };
    arrow(
        "dispatch  ",
        re!(r"(?i)Event::dispatch|event\(|dispatch\("),
        out,
    );
    arrow(
        "listen    ",
        re!(r"(?i)Event::listen|attachListener|on\("),
        out,
    );
    arrow("event     ", re!(r"(?i)on[A-Z]"), out);
    arrow("off       ", re!(r"(?i)off[A-Z]"), out);
    arrow(
        "handler   ",
        re!(r"(?i)attachEventHandler|detachEventHandler|hasEventHandler"),
        out,
    );
    for c in g.calls.iter().filter(|c| c.target == "trigger") {
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
    let call = re!(r"(?i)middleware|prependMiddleware|pushMiddleware|terminateMiddleware");
    for c in g.calls.iter().filter(|c| call.is_match(&c.target)) {
        out.push(at_line("middleware", &c.target, ns, c));
    }
    let class = re!(r"(?i)Middleware|middleware");
    for c in g
        .calls
        .iter()
        .filter(|c| class.is_match(resolved_or_target(c)))
    {
        out.push(at_line("class     ", resolved_or_target(c), ns, c));
    }
    for s in g.strings.iter().filter(|s| class.is_match(&s.value)) {
        out.push(quoted_line("string    ", ns, s));
    }
}

pub(super) fn validation(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let call = re!(r"(?i)validate|rules|Validator::make|withValidator|setRules|addError");
    for c in g.calls.iter().filter(|c| call.is_match(&c.target)) {
        out.push(at_line("validate  ", &c.target, ns, c));
    }
    let rule = re!(
        r"(?i)required|email|unique|min:|max:|integer|numeric|boolean|array|exists:|in:|not_in:|regex:|date|after:|before:"
    );
    for s in g.strings.iter().filter(|s| rule.is_match(&s.value)) {
        out.push(quoted_line("rule      ", ns, s));
    }
}

/// Eltérés a callgraph-tól: ott a `later\\(` minta bezáratlan zárójelet ad, a jq hibával kilép, és mindig „nincs találat” lesz;
/// itt a szándékolt `later\(` szerepel.
pub(super) fn jobs(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let call = re!(
        r"(?i)dispatch|Queue::push|Bus::dispatch|later\(|pushOn|dispatchSync|dispatchAfterResponse"
    );
    for c in g.calls.iter().filter(|c| call.is_match(&c.target)) {
        out.push(at_line("dispatch  ", &c.target, ns, c));
    }
    let class = re!(r"(?i)Job|Queue|Queueable|ShouldQueue|Handle");
    for c in g
        .calls
        .iter()
        .filter(|c| class.is_match(resolved_or_target(c)))
    {
        out.push(at_line("job       ", resolved_or_target(c), ns, c));
    }
    let string = re!(r"(?i)queue|Queue|job|Job");
    for s in g.strings.iter().filter(|s| string.is_match(&s.value)) {
        out.push(quoted_line("string    ", ns, s));
    }
}

pub(super) fn providers(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let call = re!(
        r"(?i)singleton|bind|register|boot|loadMigrationsFrom|publish|mergeConfigFrom|loadTranslationsFrom|loadViewsFrom"
    );
    for c in g.calls.iter().filter(|c| call.is_match(&c.target)) {
        out.push(at_line("provider  ", &c.target, ns, c));
    }
    let class = re!(r"(?i)Provider|ServiceProvider");
    for c in g
        .calls
        .iter()
        .filter(|c| class.is_match(resolved_or_target(c)))
    {
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
    for c in g.calls.iter().filter(|c| component.is_match(&c.target)) {
        out.push(at_line("di        ", &c.target, ns, c));
    }
    let (bind, in_provider) = (
        re!(r"(?i)singleton|bind|extend|alias"),
        re!(r"(?i)register|boot"),
    );
    for c in g
        .calls
        .iter()
        .filter(|c| bind.is_match(&c.target) && in_provider.is_match(&caller(c)))
    {
        out.push(at_line("di        ", &c.target, ns, c));
    }
    let factory = re!(r"(?i)register|boot|create|resolve|get");
    for c in g
        .calls
        .iter()
        .filter(|c| c.kind == "new" && factory.is_match(&caller(c)))
    {
        out.push(at_line("new       ", &c.target, ns, c));
    }
}

pub(super) fn views(g: &FileGraph, _ns: &str, out: &mut Vec<String>) {
    if !re!(r"(?i)views|template|view|blade|widget").is_match(&g.file) {
        return;
    }
    let line = |label: &str, text: &str, at: u32| format!("{label}{text}  @  {}  L{at}", g.file);
    for c in g
        .calls
        .iter()
        .filter(|c| c.kind == "method_call" || c.kind == "static_call")
    {
        out.push(line("view      ", &c.target, c.line));
    }
    let widget = re!(r"(?i)Yii::|Html::|ActiveForm|GridView|DetailView|LinkPager");
    for c in g.calls.iter().filter(|c| widget.is_match(&c.target)) {
        out.push(line("widget    ", &c.target, c.line));
    }
    let variable = re!(r"\$[a-z]");
    for c in g
        .calls
        .iter()
        .filter(|c| c.kind == "method_call" && variable.is_match(c.on.as_deref().unwrap_or("")))
    {
        let on = c.on.as_deref().unwrap_or("");
        out.push(line("property  ", &format!("{on}->{}", c.target), c.line));
    }
}
