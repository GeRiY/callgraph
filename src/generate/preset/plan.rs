//! A preset terve: minden érintett fájl teljes új szövege, a memóriában (a lemezhez nem nyúl). A tervet a hívó
//! előbb ellenőrzi (minden fájl), és csak ezután írja ki egyetlen tranzakcióban.

use std::collections::BTreeSet;

use super::page::{Kind, Page, Parts};
use super::php::{self, ControllerCtx};
use super::spec::{Action, Field, Names, Op, Spec};
use crate::generate::edit::{add_imports, insert_at_end, join_lines, split_lines};
use crate::generate::emitter::Emitter;
use crate::generate::error::{GenError, GenResult};
use crate::generate::fs::{Env, walk_php};
use crate::generate::project::{Psr4, detect_strict_types, relative};
use crate::generate::vue;
use crate::generate::{PhpEmitter, generator::pick_container};
use crate::packages::frameworks::laravel::preset as laravel;

/// A route-fájl, amelybe a preset a route-okat írja.
const ROUTES_FILE: &str = "routes/web.php";

/// Egy érintett fájl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub rel: String,
    /// A teljes új szöveg.
    pub text: String,
    /// A `--dry` kimenete: új fájlnál a teljes szöveg, módosításnál a beszúrt sorok.
    pub shown: Vec<String>,
    /// Az egysoros jelentés.
    pub summary: String,
}

impl Planned {
    fn new_file(rel: String, text: String) -> Self {
        let lines = text.lines().count();
        Self {
            summary: crate::tf!("wrote {} ({} lines)", "írtam: {} ({} sor)", rel, lines),
            shown: text.lines().map(str::to_owned).collect(),
            rel,
            text,
        }
    }
}

/// A teljes terv.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub files: Vec<Planned>,
    /// A kihagyott route-ok és hasonló megjegyzések (egy-egy sor).
    pub notes: Vec<String>,
    /// Az űrlap-oldalak és a kérés-osztályaik szövege `(oldal, kérés)` az önellenőrzéshez.
    pub forms: Vec<(String, String)>,
    /// A route-nevek a terv után (a meglévők és az újak): ezekre linkel a generált kód.
    pub route_names: BTreeSet<String>,
    /// Az önellenőrzéshez: a meglévő route-nevek és a *kiírandó* route-fájl újraolvasott nevei.
    pub verified_routes: BTreeSet<String>,
    /// A meglévő modell-fájl, ha van.
    pub model_file: Option<String>,
    /// A modell teljes osztályneve (a névtérből).
    pub model_class: String,
}

fn read(env: Env<'_>, rel: &str) -> GenResult<String> {
    env.fs.read(&env.root.join(rel)).map_err(|e| {
        GenError::new(crate::tf!(
            "cannot read {}: {}",
            "nem olvasható: {}: {}",
            rel,
            e
        ))
    })
}

fn ensure_new(env: Env<'_>, rel: &str) -> GenResult<()> {
    if env.fs.exists(&env.root.join(rel)) {
        return Err(GenError::new(crate::tf!(
            "{} already exists — the preset never overwrites a file (nothing written)",
            "{} már létezik — a preset soha nem ír felül fájlt (nem írtam semmit)",
            rel
        )));
    }
    Ok(())
}

/// A projekt kéréseiben a szabályok `'a|b'` (true) vagy `['a', 'b']` (false) alakban vannak-e többségben.
fn pipe_style(env: Env<'_>) -> bool {
    let (mut pipe, mut array) = (0usize, 0usize);
    for path in walk_php(env.fs, env.root.join("app/Http/Requests")).take(30) {
        let Ok(text) = env.fs.read(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if !(line.starts_with('\'') || line.starts_with('"')) {
                continue;
            }
            let Some((_, value)) = line.split_once("=>") else {
                continue;
            };
            let value = value.trim();
            if value.starts_with('[') {
                array += 1;
            } else if (value.starts_with('\'') || value.starts_with('"')) && value.contains('|') {
                pipe += 1;
            }
        }
    }
    pipe > array
}

/// Az Inertia-oldalak mappája (`resources/js/Pages`, vagy ha csak a kisbetűs van meg, az).
fn pages_dir(env: Env<'_>) -> &'static str {
    let exists = |rel: &str| env.fs.list(&env.root.join(rel)).is_ok();
    if !exists("resources/js/Pages") && exists("resources/js/pages") {
        "resources/js/pages"
    } else {
        "resources/js/Pages"
    }
}

/// A projekt komponensei, amelyeket az oldalak használnak.
fn parts(env: Env<'_>) -> Parts {
    let path = |name: &str| vue::component_path(env, name);
    Parts {
        layout: ["AuthenticatedLayout", "AppLayout"]
            .iter()
            .find_map(|name| path(name).map(|p| ((*name).to_owned(), p))),
        label: path("InputLabel"),
        input: path("TextInput"),
        error: path("InputError"),
        button: path("PrimaryButton"),
    }
}

/// Az utolsó nem üres sor után, egy üres sorral elválasztva; a záró újsor megmarad.
fn append_block(lines: &mut Vec<String>, block: &[String]) {
    let at = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(lines.len(), |i| i + 1);
    let tail = lines.split_off(at);
    lines.push(String::new());
    lines.extend(block.iter().cloned());
    if tail.is_empty() {
        lines.push(String::new());
    } else {
        lines.extend(tail);
    }
}

/// A route-ok terve.
struct RoutesPlan {
    file: Option<Planned>,
    names: BTreeSet<String>,
    verified: BTreeSet<String>,
    skipped: Vec<String>,
}

fn plan_routes(
    env: Env<'_>,
    spec: &Spec,
    names: &Names,
    ops: &[Op],
    controller_fqcn: &str,
) -> GenResult<RoutesPlan> {
    let mut known: BTreeSet<String> = BTreeSet::new();
    let mut taken: Vec<(String, String)> = Vec::new();
    for path in walk_php(env.fs, env.root.join("routes")) {
        let Ok(text) = env.fs.read(&path) else {
            continue;
        };
        let rel = relative(env.root, &path).unwrap_or_default();
        for route in laravel::routes_of(&rel, &text) {
            if let Some(name) = route.name {
                known.insert(name);
            }
            taken.push((route.verb, route.uri));
        }
    }
    let mut added: Vec<Op> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for op in ops {
        let name = names.route_name(*op);
        let uri = names.route_uri(*op);
        let same_uri = taken.iter().any(|(verb, existing)| {
            *existing == uri && (verb == "ANY" || verb.split('|').any(|v| v == op.verb()))
        });
        if known.contains(&name) || same_uri {
            skipped.push(name);
        } else {
            added.push(*op);
        }
    }
    let mut all = known.clone();
    all.extend(added.iter().map(|op| names.route_name(*op)));
    if added.is_empty() {
        return Ok(RoutesPlan {
            file: None,
            verified: known,
            names: all,
            skipped,
        });
    }

    let path = env.root.join(ROUTES_FILE);
    let existed = env.fs.exists(&path);
    let (mut lines, eol) = if existed {
        split_lines(&read(env, ROUTES_FILE)?)
    } else {
        (vec!["<?php".to_owned(), String::new()], "\n")
    };
    // az importok a route-ok (az első nem import / nem megjegyzés sor) fölé
    let limit = lines
        .iter()
        .position(|l| {
            let t = l.trim();
            !(t.is_empty()
                || t.starts_with("<?php")
                || t.starts_with("use ")
                || t.starts_with("namespace ")
                || t.starts_with("declare(")
                || t.starts_with("//")
                || t.starts_with("/*")
                || t.starts_with('*')
                || t.starts_with('#'))
        })
        .unwrap_or(lines.len());
    let mut imports = vec![(controller_fqcn.to_owned(), None)];
    if !existed {
        imports.push(("Illuminate\\Support\\Facades\\Route".to_owned(), None));
    }
    add_imports(&mut lines, limit, &imports);

    let route_lines: Vec<String> = added.iter().map(|op| php::route_line(names, *op)).collect();
    let indented =
        |lines: &[String]| -> Vec<String> { lines.iter().map(|l| format!("    {l}")).collect() };
    let (block, place) = if spec.options.auth {
        let joined = join_lines(&lines, "\n");
        if let Some(close) = laravel::auth_group_close_line(&joined) {
            let block = indented(&route_lines);
            insert_at_end(&mut lines, close + 1, &block);
            (block, crate::t!(" in auth group", " az auth csoportban"))
        } else {
            let mut group = vec!["Route::middleware('auth')->group(function () {".to_owned()];
            group.extend(indented(&route_lines));
            group.push("});".to_owned());
            append_block(&mut lines, &group);
            (
                group,
                crate::t!(" in new auth group", " új auth csoportban"),
            )
        }
    } else {
        append_block(&mut lines, &route_lines);
        (route_lines, "")
    };
    let text = join_lines(&lines, eol);
    let mut verified = known;
    verified.extend(
        laravel::routes_of(ROUTES_FILE, &text)
            .into_iter()
            .filter_map(|route| route.name),
    );
    let added_names: Vec<String> = added.iter().map(|op| names.route_name(*op)).collect();
    let shown_names = if added_names.len() > 4 {
        format!(
            "{}, +{}",
            added_names[..3].join(", "),
            added_names.len() - 3
        )
    } else {
        added_names.join(", ")
    };
    let summary = crate::tf!(
        "{}: +{} routes ({}){}",
        "{}: +{} route ({}){}",
        ROUTES_FILE,
        added.len(),
        shown_names,
        place
    );
    Ok(RoutesPlan {
        file: Some(Planned {
            rel: ROUTES_FILE.to_owned(),
            text,
            shown: block,
            summary,
        }),
        names: all,
        verified,
        skipped,
    })
}

/// A controller: új fájl, vagy a metódusok beszúrása a meglévőbe (létező metódusra hiba).
fn plan_controller(
    env: Env<'_>,
    rel: &str,
    namespace: &str,
    ctx: &ControllerCtx<'_>,
    ops: &[Op],
    strict: bool,
) -> GenResult<Planned> {
    if !env.fs.exists(&env.root.join(rel)) {
        return Ok(Planned::new_file(
            rel.to_owned(),
            ctx.file(strict, namespace, ops),
        ));
    }
    let text = read(env, rel)?;
    let (mut lines, eol) = split_lines(&text);
    let graph = PhpEmitter.analyze(rel, &text);
    let container = pick_container(&graph, Some(ctx.names.controller.as_str()), rel)?;
    let have: Vec<&str> = graph
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some(container.name.as_str()))
        .map(|m| m.name.as_str())
        .collect();
    let clash: Vec<&str> = ops
        .iter()
        .map(|op| op.method())
        .filter(|m| have.contains(m))
        .collect();
    if !clash.is_empty() {
        return Err(GenError::new(crate::tf!(
            "{} already has: {} — not generating over it (nothing written)",
            "{} már tartalmazza: {} — nem generálok rá (nem írtam semmit)",
            rel,
            clash.join(", ")
        )));
    }
    if container.line >= container.end_line {
        return Err(GenError::new(crate::tf!(
            "{} is declared on one line; reformat it so its `{{` and `}}` are on separate lines",
            "a(z) {} egy sorban van deklarálva; tördeld úgy, hogy a `{{` és a `}}` külön sorban legyen",
            container.name
        )));
    }
    let block = ctx.methods(ops);
    insert_at_end(&mut lines, container.end_line as usize, &block);
    let own_ns = graph.namespace.as_deref();
    let imports: Vec<(String, Option<String>)> = ops
        .iter()
        .flat_map(|op| ctx.imports(*op))
        .filter(|fqcn| fqcn.rsplit_once('\\').map(|(ns, _)| ns) != own_ns)
        .map(|fqcn| (fqcn, None))
        .collect();
    add_imports(
        &mut lines,
        (container.line as usize).saturating_sub(1),
        &imports,
    );
    let methods: Vec<&str> = ops.iter().map(|op| op.method()).collect();
    Ok(Planned {
        rel: rel.to_owned(),
        text: join_lines(&lines, eol),
        shown: block,
        summary: crate::tf!(
            "inserted into {}: {}",
            "beszúrva ide: {}: {}",
            rel,
            methods.join(", ")
        ),
    })
}

/// A terv a kifejezésből. Hibánál (létező kérés / oldal / metódus) semmi nem íródik.
pub fn build(env: Env<'_>, spec: &Spec) -> GenResult<Plan> {
    let names = Names::of(spec);
    let psr4 = Psr4::load(env.fs, env.root);
    let strict = detect_strict_types(env.fs, env.root, &psr4);
    let ops = spec.ops();
    let ns_of = |rel: &str, fallback: &str| {
        psr4.namespace_of(rel)
            .unwrap_or_else(|| fallback.to_owned())
    };
    let controller_rel = format!("app/Http/Controllers/{}.php", names.controller);
    let controller_ns = ns_of(&controller_rel, "App\\Http\\Controllers");
    let request_ns = ns_of("app/Http/Requests/X.php", "App\\Http\\Requests");
    let model_file = [
        format!("app/Models/{}.php", names.model),
        format!("app/{}.php", names.model),
    ]
    .into_iter()
    .find(|rel| env.fs.exists(&env.root.join(rel)));
    let model_probe = model_file
        .clone()
        .unwrap_or_else(|| format!("app/Models/{}.php", names.model));
    let model_ns = ns_of(&model_probe, "App\\Models");
    let model_class = php::qualify(&model_ns, &names.model);

    let routes = plan_routes(
        env,
        spec,
        &names,
        &ops,
        &php::qualify(&controller_ns, &names.controller),
    )?;
    let ctx = ControllerCtx {
        names: &names,
        model_ns: &model_ns,
        request_ns: &request_ns,
        has_index: routes.names.contains(&names.route_name(Op::Index)),
    };

    let mut plan = Plan {
        model_file,
        model_class,
        ..Plan::default()
    };

    // 1. kérés-osztályok
    let pipe = pipe_style(env);
    let mut request_texts: Vec<(Action, String)> = Vec::new();
    for (action, class) in [
        (Action::Create, ctx.store_request()),
        (Action::Edit, ctx.update_request()),
    ] {
        let Some(fields) = spec.fields_of(action) else {
            continue;
        };
        let rel = format!("app/Http/Requests/{class}.php");
        ensure_new(env, &rel)?;
        let text = php::request_file(strict, &request_ns, &class, fields, pipe);
        request_texts.push((action, text.clone()));
        plan.files.push(Planned::new_file(rel, text));
    }

    // 2. controller
    plan.files.push(plan_controller(
        env,
        &controller_rel,
        &controller_ns,
        &ctx,
        &ops,
        strict,
    )?);

    // 3. route-ok
    if !routes.skipped.is_empty() {
        plan.notes.push(crate::tf!(
            "skipped existing route(s): {}",
            "kihagyott, már létező route: {}",
            routes.skipped.join(", ")
        ));
    }
    if let Some(file) = routes.file {
        plan.files.push(file);
    }
    plan.route_names = routes.names;
    plan.verified_routes = routes.verified;

    // 4. oldalak
    let facts = vue::project_facts(env);
    let parts = parts(env);
    let dir = pages_dir(env);
    for (action, kind) in [
        (Action::Index, Kind::Index),
        (Action::Create, Kind::Create),
        (Action::Show, Kind::Show),
        (Action::Edit, Kind::Edit),
    ] {
        if !spec.has(action) {
            continue;
        }
        let fields: Vec<Field> = match action {
            Action::Create | Action::Edit => spec.fields_of(action).unwrap_or_default().to_vec(),
            _ => spec.display_fields(action),
        };
        let rel = format!("{dir}/{}/{}.vue", names.pages, kind.file_stem());
        ensure_new(env, &rel)?;
        let page = Page {
            kind,
            names: &names,
            fields: &fields,
            facts,
            parts: &parts,
            routes: &plan.route_names,
        };
        let text = page.render();
        if let Some((_, request)) = request_texts.iter().find(|(a, _)| *a == action) {
            plan.forms.push((text.clone(), request.clone()));
        }
        plan.files.push(Planned::new_file(rel, text));
    }
    Ok(plan)
}
