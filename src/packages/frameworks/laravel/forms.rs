//! Űrlap-szerződés: az Inertia `useForm({..})` mezői ↔ a küldés célja (route) ↔ a controller-akció validációja.
//! A Vue / JS oldalon a `const form = useForm({..})` objektum-literáljának felső szintű kulcsai és a
//! `form.post|put|patch|delete(<cél>)` küldések (a forrásból — a szűkített gráfban nincs argumentum); a cél
//! `route('név')` vagy `'/uri'` literál. A PHP oldalon a controller-metódus FormRequest-paramétere és inline
//! validációi (`rules.rs`). Az összevetés: `unvalidated` (küldött, de nincs szabálya) és `missing_required`
//! (kötelező szabály, amit az űrlap nem küld). Kimenetek: az outline `form:` sora a küldő függvény alatt és a
//! controller-akció alatt, a json `form` mezője és a `forms` nézet.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::super::files::ProjectFiles;
use super::super::inertia::render::{literal, source_args_named, split_top};
use super::routes::{Action, Route, short_class, strip_comments};
use super::rules::{self, Validation};
use super::{Laravel, has_verb};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};
use std::path::Path;

/// A `useForm` hívás feloldott neve a gráfban.
const USE_FORM: &str = "@inertiajs/vue3::useForm";
/// Egy csoportban legfeljebb ennyi név (a többi `+N`).
const MAX_NAMES: usize = 5;
/// A controller-akció `form:` sorában legfeljebb ennyi page-fájl (a többi `+N`).
const MAX_FORMS: usize = 3;
/// A page-fájl rövid alakjának levágott előtagja.
const JS_ROOT: &str = "resources/js/";

/// Egy ellenőrzött űrlap-küldés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Form {
    /// A küldő fájl a gyökérhez relatívan, a küldő hívás sora, célja (`form.post`), függvénye és konténere.
    pub file: String,
    pub line: u32,
    pub target: String,
    pub caller: Option<String>,
    pub container: Option<String>,
    /// A küldés igéje (`POST`) és a route uri-ja.
    pub verb: String,
    pub uri: String,
    /// A controller teljes neve és a metódus.
    pub class: String,
    pub method: String,
    /// Az űrlap mezői (a `useForm` literáljának felső szintű kulcsai).
    pub fields: Vec<String>,
    /// A szabályok felső szintű kulcsai; üres és `validated == false`: nincs szabály-forrás.
    pub rules: Vec<String>,
    pub validated: bool,
    pub unvalidated: Vec<String>,
    pub missing_required: Vec<String>,
}

impl Form {
    /// `Teljes\Névtér\FooController@store` (a json-hoz).
    pub(crate) fn action(&self) -> String {
        format!("{}@{}", self.class, self.method)
    }

    fn mismatched(&self) -> bool {
        !self.unvalidated.is_empty() || !self.missing_required.is_empty()
    }

    /// A kimenet tagjai a cél után: a figyelmeztetések (`prefix` + szöveg), nincs szabálynál a jelzés, tisztánál
    /// a mezőszám.
    fn findings(&self, lang: Lang, prefix: &str) -> Vec<String> {
        if !self.validated {
            return vec![
                lang.pick("no validation found", "nincs validáció")
                    .to_owned(),
            ];
        }
        let mut out = Vec::new();
        if !self.unvalidated.is_empty() {
            out.push(format!(
                "{prefix}{} {}",
                lang.pick("unvalidated:", "validálatlan:"),
                names(&self.unvalidated)
            ));
        }
        if !self.missing_required.is_empty() {
            out.push(format!(
                "{prefix}{} {}",
                lang.pick("required not sent:", "kötelező, de nem küldi:"),
                names(&self.missing_required)
            ));
        }
        if out.is_empty() {
            out.push(format!(
                "{} {} ✓",
                self.fields.len(),
                lang.pick("fields", "mező")
            ));
        }
        out
    }

    /// Az outline sora a küldő függvény alatt: `form: POST /register · RegisteredUserController@store · 4 fields ✓`.
    fn line_text(&self, lang: Lang) -> String {
        let mut parts = vec![format!("{} {}", self.verb, self.uri), self.short_action()];
        parts.extend(self.findings(lang, "⚠ "));
        format!("form: {}", parts.join(" · "))
    }

    fn short_action(&self) -> String {
        format!("{}@{}", short_class(&self.class), self.method)
    }
}

/// Legfeljebb `MAX_NAMES` név, a többi `, +N`.
fn names(list: &[String]) -> String {
    let mut text = list
        .iter()
        .take(MAX_NAMES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if list.len() > MAX_NAMES {
        text.push_str(&format!(", +{}", list.len() - MAX_NAMES));
    }
    text
}

/// A küldő fájl rövid alakja: a `resources/js/` nélkül (`Pages/Auth/Register.vue`).
fn short_file(file: &str) -> &str {
    file.strip_prefix(JS_ROOT).unwrap_or(file)
}

/// Egy `useForm` küldés a JS / Vue fájlban (még a route-ra feloldás előtt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub file: String,
    pub line: u32,
    pub target: String,
    pub caller: Option<String>,
    pub container: Option<String>,
    /// `POST` / `PUT` / `PATCH` / `DELETE`.
    pub verb: &'static str,
    pub fields: Vec<String>,
    /// A küldés első argumentuma (a cél): `route('register')` / `'/register'` / bármi más.
    pub arg: String,
}

/// A fájl `useForm` küldései. `text`: a fájl forrása; `calls`: a gráf hívásai (a `useForm` hívás a `resolved`-ből).
/// Nem literál objektumú (`useForm(data)`, spread, számított kulcs) űrlap kimarad.
pub(super) fn extract(rel: &str, text: &str, calls: &[Call]) -> Vec<Site> {
    static DECL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?useForm\b")
            .expect("valid regex")
    });
    let decls: Vec<(u32, String, Vec<String>)> = calls
        .iter()
        .filter(|call| call.resolved.as_deref() == Some(USE_FORM))
        .filter_map(|call| {
            let index = usize::try_from(call.line).ok()?.checked_sub(1)?;
            let var = DECL.captures(text.lines().nth(index)?)?[1].to_owned();
            let args = source_args_named(text, call.line, "useform")?;
            // `useForm('rememberKey', {..})`: az objektum a második argumentum
            let object = match args.as_slice() {
                [key, object, ..] if literal(key).is_some() => object,
                [object, ..] => object,
                [] => return None,
            };
            Some((call.line, var, object_keys(object)?))
        })
        .collect();
    if decls.is_empty() {
        return Vec::new();
    }
    let mut sites: Vec<Site> = Vec::new();
    for call in calls {
        // a scriptben a `resolved` jelöli; a template-ben (`@submit.prevent="form.patch(..)"`) nincs típus, ott a
        // változó neve köti a deklarációhoz
        if call
            .resolved
            .as_deref()
            .is_some_and(|resolved| !resolved.starts_with("@inertiajs/vue3::useForm()."))
        {
            continue;
        }
        let Some((var, method)) = call.target.rsplit_once('.') else {
            continue;
        };
        let verb = match method {
            "post" => "POST",
            "put" => "PUT",
            "patch" => "PATCH",
            "delete" => "DELETE",
            _ => continue,
        };
        // a küldés a hozzá legközelebbi, előtte álló azonos nevű deklarációé; a script előtti template-ben az
        // utolsó azonos nevűé; mező nélküli űrlapnál (`useForm({})`) nincs mit ellenőrizni
        let Some(decl) = decls
            .iter()
            .rfind(|decl| decl.1 == var && decl.0 <= call.line)
            .or_else(|| decls.iter().rfind(|decl| decl.1 == var))
            .filter(|decl| !decl.2.is_empty())
        else {
            continue;
        };
        let Some(arg) = source_args_named(text, call.line, &call.target.to_ascii_lowercase())
            .and_then(|args| args.into_iter().next())
        else {
            continue;
        };
        if sites
            .iter()
            .any(|site| site.line == call.line && site.target == call.target)
        {
            continue;
        }
        sites.push(Site {
            file: rel.to_owned(),
            line: call.line,
            target: call.target.clone(),
            caller: call.caller.clone(),
            container: call.container.clone(),
            verb,
            fields: decl.2.clone(),
            arg,
        });
    }
    sites
}

/// Egy objektum-literál felső szintű kulcsai: `{ name: '', 'e-mail': '', password }`; ha az argumentum nem
/// literál, vagy egy elem spread / számított kulcs: `None`.
fn object_keys(arg: &str) -> Option<Vec<String>> {
    let inner = arg.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut keys = Vec::new();
    for entry in split_top(inner, ",") {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if entry.starts_with("...") {
            return None;
        }
        let head = split_top(entry, ":")[0].trim();
        let key = match head.chars().next()? {
            '\'' | '"' => literal(head)?,
            _ if head
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$') =>
            {
                head.to_owned()
            }
            _ => return None,
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Some(keys)
}

/// Egy route uri-ja illeszkedik-e az URL szegmenseire (`{param}` bármely szegmenst elfogad).
fn uri_matches(uri: &str, segments: &[&str]) -> bool {
    let route: Vec<&str> = uri.trim_matches('/').split('/').collect();
    route.len() == segments.len()
        && route
            .iter()
            .zip(segments)
            .all(|(r, u)| r == u || (r.starts_with('{') && r.ends_with('}')))
}

/// Az összevetés: (validálatlan mezők, kötelező, de nem küldött kulcsok).
fn compare(fields: &[String], validation: &Validation) -> (Vec<String>, Vec<String>) {
    let unvalidated = fields
        .iter()
        .filter(|field| {
            let field = field.as_str();
            !matches!(field, "_method" | "_token")
                && !validation.rules.contains_key(field)
                && !validation.reads.contains(field)
                && !field.strip_suffix("_confirmation").is_some_and(|base| {
                    validation
                        .rules
                        .get(base)
                        .is_some_and(|flags| flags.confirmed)
                })
        })
        .cloned()
        .collect();
    let missing = validation
        .rules
        .iter()
        .filter(|(key, flags)| {
            flags.required
                && !flags.sometimes
                && !flags.nullable
                && !fields.iter().any(|field| field == *key)
        })
        .map(|(key, _)| key.clone())
        .collect();
    (unvalidated, missing)
}

impl Laravel {
    /// Az űrlap-küldések ellenőrzése a projektre (a `scan` után, a route-táblával): a JS / Vue fájlok `useForm`
    /// küldései a route-ra, a controller-akcióra és a validációjára oldva. Ami nem oldható fel (dinamikus cél,
    /// closure-route, ismeretlen controller / metódus), kimarad.
    pub(crate) fn scan_forms<'g>(
        &mut self,
        graphs: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        files: &dyn ProjectFiles,
    ) {
        self.forms.clear();
        if self.routes.is_empty() {
            return;
        }
        let mut classes: HashMap<String, String> = HashMap::new();
        let mut sites: Vec<Site> = Vec::new();
        for (path, graph) in graphs {
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            if rel.ends_with(".php") {
                if let Some(namespace) = graph.namespace.as_deref() {
                    for container in &graph.containers {
                        classes.insert(
                            format!("{}\\{}", namespace.trim_matches('\\'), container.name),
                            rel.clone(),
                        );
                    }
                }
            } else if graph.module.is_some()
                && graph
                    .calls
                    .iter()
                    .any(|call| call.resolved.as_deref() == Some(USE_FORM))
                && let Some(text) = files.read(path)
            {
                sites.extend(extract(&rel, &text, &graph.calls));
            }
        }
        let mut cache: HashMap<(String, String), Option<Validation>> = HashMap::new();
        let mut forms = Vec::new();
        for site in sites {
            let Some(route) = self.form_route(&site.arg, site.verb) else {
                continue;
            };
            let Action::Controller { class, method } = &route.action else {
                continue;
            };
            let validation = cache
                .entry((class.clone(), method.clone()))
                .or_insert_with(|| self.validation_of(class, method, &classes, files))
                .clone();
            let Some(validation) = validation else {
                continue;
            };
            let (unvalidated, missing_required) = if validation.found {
                compare(&site.fields, &validation)
            } else {
                (Vec::new(), Vec::new())
            };
            forms.push(Form {
                file: site.file,
                line: site.line,
                target: site.target,
                caller: site.caller,
                container: site.container,
                verb: site.verb.to_owned(),
                uri: route.uri.clone(),
                class: class.clone(),
                method: method.clone(),
                fields: site.fields,
                rules: validation.rules.keys().cloned().collect(),
                validated: validation.found,
                unvalidated,
                missing_required,
            });
        }
        self.forms = forms;
    }

    /// A küldés célja a route-táblában: `route('név')` (a küldés igéjével egyező route) vagy `'/uri'` literál
    /// (`{param}` bármely szegmens); más cél (sablon-literál, változó): `None`.
    fn form_route(&self, arg: &str, verb: &str) -> Option<&Route> {
        let arg = arg.trim();
        if let Some(inner) = arg
            .strip_prefix("route(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let name = literal(split_top(inner, ",").first()?)?;
            let route = &self.routes[*self.by_name.get(&name)?];
            let route = self.for_verb(route, verb);
            return has_verb(&route.verb, verb).then_some(route);
        }
        let url = literal(arg)?;
        let path = url.split(['?', '#']).next()?;
        if !path.starts_with('/') {
            return None;
        }
        let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
        let candidates = || {
            self.routes
                .iter()
                .filter(|route| has_verb(&route.verb, verb) && uri_matches(&route.uri, &segments))
        };
        candidates()
            .find(|route| !route.uri.contains('{'))
            .or_else(|| candidates().next())
    }

    /// Egy osztály forrása: a gráfokból gyűjtött térkép, különben a PSR-4 (`App\X\Y` → `app/X/Y.php`).
    fn class_text(
        &self,
        class: &str,
        classes: &HashMap<String, String>,
        files: &dyn ProjectFiles,
    ) -> Option<String> {
        let mapped = classes.get(class).cloned();
        let psr4 = class
            .strip_prefix("App\\")
            .map(|rest| format!("app/{}.php", rest.replace('\\', "/")));
        mapped
            .into_iter()
            .chain(psr4)
            .find_map(|rel| files.read(&self.root.join(rel)))
    }

    /// Az akció validációja: az inline szabályok a törzsből + a FormRequest-típusú paraméterek `rules()`-a.
    /// `None`: a controller vagy a metódus nem található.
    fn validation_of(
        &self,
        class: &str,
        method: &str,
        classes: &HashMap<String, String>,
        files: &dyn ProjectFiles,
    ) -> Option<Validation> {
        let text = self.class_text(class, classes, files)?;
        let clean = strip_comments(&text);
        let (params, body) = rules::method_parts(&clean, method)?;
        let (imports, namespace) = rules::imports_of(&clean);
        let mut validation = rules::body_validation(&body);
        for ty in rules::param_types(&params) {
            let Some(full) = rules::qualify(&ty, &imports, namespace.as_deref()) else {
                continue;
            };
            let Some(request) = self.class_text(&full, classes, files) else {
                continue;
            };
            let from_request = rules::request_validation(&request);
            if from_request.found {
                validation.merge(from_request);
            }
        }
        Some(validation)
    }

    /// A küldő függvény alatti `form:` sorok (a fájl a sorok útja, a gyökérhez képest bármilyen alakban).
    pub(crate) fn form_lines(
        &self,
        file: &str,
        container: Option<&str>,
        caller: &str,
        lang: Lang,
    ) -> Vec<String> {
        if self.forms.is_empty() {
            return Vec::new();
        }
        let Some(rel) = self.rel(file) else {
            return Vec::new();
        };
        self.forms
            .iter()
            .filter(|form| {
                form.file == rel
                    && form.caller.as_deref() == Some(caller)
                    && form.container.as_deref() == container
            })
            .map(|form| form.line_text(lang))
            .collect()
    }

    /// A controller-akció `form:` sora: `form: Pages/Auth/Register.vue, Pages/X.vue, +1` (legfeljebb 3 fájl).
    /// `namespace` + `container`: a controller teljes neve. Nincs űrlap: `None`.
    pub(crate) fn form_line(
        &self,
        namespace: Option<&str>,
        container: &str,
        method: &str,
    ) -> Option<String> {
        if self.forms.is_empty() {
            return None;
        }
        let full = match namespace.filter(|ns| !ns.is_empty()) {
            Some(ns) => format!("{}\\{container}", ns.trim_matches('\\')),
            None => container.to_owned(),
        };
        let mut files: Vec<&str> = Vec::new();
        for form in &self.forms {
            let class_ok =
                form.class == full || (!form.class.contains('\\') && form.class == container);
            if class_ok && form.method == method && !files.contains(&short_file(&form.file)) {
                files.push(short_file(&form.file));
            }
        }
        if files.is_empty() {
            return None;
        }
        let mut line = format!(
            "form: {}",
            files
                .iter()
                .take(MAX_FORMS)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        );
        if files.len() > MAX_FORMS {
            line.push_str(&format!(", +{}", files.len() - MAX_FORMS));
        }
        Some(line)
    }

    /// A (teljes gráfbeli) hívás űrlapja, ha az egy ellenőrzött `useForm` küldés.
    pub(crate) fn form_of(&self, file: &str, call: &Call) -> Option<&Form> {
        if self.forms.is_empty() {
            return None;
        }
        let rel = self.rel(file)?;
        self.forms
            .iter()
            .find(|form| form.file == rel && form.line == call.line && form.target == call.target)
    }

    /// A `forms` nézet sorai a hatókörre (`scope`: a lekérdezett fájlok a gyökérhez relatívan): a problémás és a
    /// validáció nélküli űrlapok soronként, a végén az összegzés. Üres: nincs ellenőrzött űrlap.
    pub(crate) fn form_view_lines(&self, scope: &[String], lang: Lang) -> Vec<String> {
        let mut forms: Vec<&Form> = self
            .forms
            .iter()
            .filter(|form| scope.contains(&form.file))
            .collect();
        if forms.is_empty() {
            return Vec::new();
        }
        forms.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        let mut lines: Vec<String> = forms
            .iter()
            .filter(|form| form.mismatched() || !form.validated)
            .map(|form| {
                let mut parts = vec![format!("{} {}", form.verb, form.uri), form.short_action()];
                parts.extend(form.findings(lang, ""));
                format!("⚠ {} L{} → {}", form.file, form.line, parts.join(" · "))
            })
            .collect();
        let issues = forms.iter().filter(|form| form.mismatched()).count();
        let bare = forms.iter().filter(|form| !form.validated).count();
        lines.push(match lang {
            Lang::En => format!(
                "forms: {} checked, {issues} with issues, {bare} without validation",
                forms.len()
            ),
            Lang::Hu => format!(
                "űrlapok: {} ellenőrizve, {issues} problémás, {bare} validáció nélkül",
                forms.len()
            ),
        });
        lines
    }
}
