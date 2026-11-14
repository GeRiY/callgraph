//! A preset kifejezésének értelmezése: `laravel:Fee[auth,prefix=admin]>create(name:str,paid?:bool)+edit+index`.
//! Nem a cgx-lexert használja (a fej és a mezőlista saját, szűk nyelvtan), és a hibák egysoros, kétnyelvű üzenetek.

use std::collections::BTreeSet;

use crate::generate::error::{GenError, GenResult};
use crate::packages::frameworks::laravel::preset::table_name;

/// A kifejezés feje.
pub const HEAD: &str = "laravel:";

/// A mező (cgx-neutrális) típusa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Str,
    Int,
    Float,
    Bool,
    Text,
    Date,
    DateTime,
    Email,
}

impl FieldType {
    fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "str" | "string" => Self::Str,
            "int" | "integer" => Self::Int,
            "float" | "decimal" | "number" => Self::Float,
            "bool" | "boolean" => Self::Bool,
            "text" => Self::Text,
            "date" => Self::Date,
            "datetime" | "timestamp" => Self::DateTime,
            "email" => Self::Email,
            _ => return None,
        })
    }
}

/// Egy mező: név, típus, `?` = nem kötelező (nullable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
    pub optional: bool,
}

/// A felhasználó által kért akció.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Index,
    Create,
    Edit,
    Show,
    Destroy,
}

impl Action {
    fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "index" => Self::Index,
            "create" => Self::Create,
            "edit" => Self::Edit,
            "show" => Self::Show,
            "destroy" | "delete" => Self::Destroy,
            _ => return None,
        })
    }
}

/// Egy controller-művelet (az akciókból: a `create` = `Create` + `Store`, az `edit` = `Edit` + `Update`). A sorrend
/// a route-ok sorrendje is (a `/fees/create` a `/fees/{fee}` előtt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Op {
    Index,
    Create,
    Store,
    Show,
    Edit,
    Update,
    Destroy,
}

impl Op {
    /// A controller-metódus és a route-név utótagja.
    pub fn method(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Create => "create",
            Self::Store => "store",
            Self::Show => "show",
            Self::Edit => "edit",
            Self::Update => "update",
            Self::Destroy => "destroy",
        }
    }

    /// A route HTTP-igéje.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Index | Self::Create | Self::Show | Self::Edit => "GET",
            Self::Store => "POST",
            Self::Update => "PUT",
            Self::Destroy => "DELETE",
        }
    }
}

/// Egy akció a mezőivel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    pub action: Action,
    pub fields: Vec<Field>,
}

/// A `[..]` kapcsolók.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub auth: bool,
    pub prefix: Option<String>,
}

/// A teljes kifejezés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub model: String,
    pub options: Options,
    pub actions: Vec<ActionSpec>,
}

impl Spec {
    /// Az akció mezői, ha az akció szerepel.
    pub fn fields_of(&self, action: Action) -> Option<&[Field]> {
        self.actions
            .iter()
            .find(|a| a.action == action)
            .map(|a| a.fields.as_slice())
    }

    /// Van-e ilyen akció.
    pub fn has(&self, action: Action) -> bool {
        self.fields_of(action).is_some()
    }

    /// A megjelenítendő mezők (index / show oszlopai): a sajátjuk, különben a `create`, különben az `edit` mezői.
    pub fn display_fields(&self, action: Action) -> Vec<Field> {
        [action, Action::Create, Action::Edit]
            .iter()
            .filter_map(|a| self.fields_of(*a))
            .find(|fields| !fields.is_empty())
            .map(<[Field]>::to_vec)
            .unwrap_or_default()
    }

    /// Az összes ürlap-mező (create + edit, név szerint egyszer): a modell-ellenőrzés bemenete.
    pub fn form_fields(&self) -> Vec<Field> {
        let mut all: Vec<Field> = Vec::new();
        for action in [Action::Create, Action::Edit] {
            for field in self.fields_of(action).unwrap_or_default() {
                if !all.iter().any(|f| f.name == field.name) {
                    all.push(field.clone());
                }
            }
        }
        all
    }

    /// A controller-műveletek, route-sorrendben.
    pub fn ops(&self) -> Vec<Op> {
        let mut ops = BTreeSet::new();
        for spec in &self.actions {
            let own: &[Op] = match spec.action {
                Action::Index => &[Op::Index],
                Action::Create => &[Op::Create, Op::Store],
                Action::Edit => &[Op::Edit, Op::Update],
                Action::Show => &[Op::Show],
                Action::Destroy => &[Op::Destroy],
            };
            ops.extend(own.iter().copied());
        }
        ops.into_iter().collect()
    }
}

fn bad(message: String) -> GenError {
    GenError::new(message)
}

fn usage() -> String {
    crate::t!(
        "laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index (actions: index create edit show destroy; types: str int float bool text date datetime email; `?` = optional)",
        "laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index (akciók: index create edit show destroy; típusok: str int float bool text date datetime email; `?` = nem kötelező)"
    )
    .to_owned()
}

/// A szöveg felosztása a `sep` mentén a zárójelen kívül.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        } else if c == sep && depth == 0 {
            parts.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&text[start..]);
    parts
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_field(text: &str) -> GenResult<Field> {
    let (raw_name, raw_type) = match text.split_once(':') {
        Some((n, t)) => (n.trim(), t.trim()),
        None => (text.trim(), "str"),
    };
    let mut optional = false;
    let mut name = raw_name;
    if let Some(stripped) = name.strip_suffix('?') {
        optional = true;
        name = stripped.trim_end();
    }
    let mut ty_word = raw_type;
    if let Some(stripped) = ty_word.strip_prefix('?') {
        optional = true;
        ty_word = stripped.trim_start();
    }
    if let Some(stripped) = ty_word.strip_suffix('?') {
        optional = true;
        ty_word = stripped.trim_end();
    }
    if !is_ident(name) || name == "id" {
        return Err(bad(crate::tf!(
            "bad field name `{}` (letters, digits, `_`; `id` is implicit). {}",
            "hibás mezőnév: `{}` (betű, számjegy, `_`; az `id` magától értetődő). {}",
            name,
            usage()
        )));
    }
    let ty = FieldType::parse(ty_word).ok_or_else(|| {
        bad(crate::tf!(
            "unknown field type `{}` for `{}`. {}",
            "ismeretlen mezőtípus: `{}` ennél: `{}`. {}",
            ty_word,
            name,
            usage()
        ))
    })?;
    Ok(Field {
        name: name.to_owned(),
        ty,
        optional,
    })
}

fn parse_options(text: &str) -> GenResult<Options> {
    let mut options = Options::default();
    for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match item.split_once('=') {
            None if item == "auth" => options.auth = true,
            Some(("prefix", value)) => {
                let value = value.trim().trim_matches('/');
                let ok = !value.is_empty()
                    && value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/'));
                if !ok {
                    return Err(bad(crate::tf!(
                        "bad prefix `{}` (letters, digits, `-`, `_`, `/`)",
                        "hibás prefix: `{}` (betű, számjegy, `-`, `_`, `/`)",
                        value
                    )));
                }
                options.prefix = Some(value.to_owned());
            }
            _ => {
                return Err(bad(crate::tf!(
                    "unknown option `{}` (known: auth, prefix=<path>)",
                    "ismeretlen kapcsoló: `{}` (van: auth, prefix=<út>)",
                    item
                )));
            }
        }
    }
    Ok(options)
}

fn parse_action(piece: &str) -> GenResult<ActionSpec> {
    let piece = piece.trim();
    let (word, args) = match piece.split_once('(') {
        Some((word, rest)) => {
            let inner = rest.trim_end().strip_suffix(')').ok_or_else(|| {
                bad(crate::tf!(
                    "missing `)` in `{}`. {}",
                    "hiányzó `)` itt: `{}`. {}",
                    piece,
                    usage()
                ))
            })?;
            (word.trim(), Some(inner))
        }
        None => (piece, None),
    };
    let action = Action::parse(word).ok_or_else(|| {
        bad(crate::tf!(
            "unknown action `{}`. {}",
            "ismeretlen akció: `{}`. {}",
            word,
            usage()
        ))
    })?;
    let mut fields: Vec<Field> = Vec::new();
    for item in args
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let field = parse_field(item)?;
        if fields.iter().any(|f| f.name == field.name) {
            return Err(bad(crate::tf!(
                "field `{}` is listed twice in `{}`",
                "a(z) `{}` mező kétszer szerepel itt: `{}`",
                field.name,
                word
            )));
        }
        fields.push(field);
    }
    Ok(ActionSpec { action, fields })
}

/// A kifejezés értelmezése.
pub fn parse(expr: &str) -> GenResult<Spec> {
    let text = expr.trim();
    let rest = text.strip_prefix(HEAD).ok_or_else(|| bad(usage()))?;
    let (head, action_text) = match rest.split_once('>') {
        Some((head, actions)) => (head.trim(), actions.trim()),
        None => (rest.trim(), ""),
    };
    let (model, options) = match head.split_once('[') {
        Some((name, opts)) => {
            let inner = opts.trim_end().strip_suffix(']').ok_or_else(|| {
                bad(crate::tf!(
                    "missing `]` in `{}`. {}",
                    "hiányzó `]` itt: `{}`. {}",
                    head,
                    usage()
                ))
            })?;
            (name.trim(), parse_options(inner)?)
        }
        None => (head, Options::default()),
    };
    if !is_ident(model) || !model.starts_with(|c: char| c.is_ascii_uppercase()) {
        return Err(bad(crate::tf!(
            "the resource must be a StudlyCase model name (e.g. `Fee`), got `{}`. {}",
            "az erőforrás StudlyCase modellnév legyen (pl. `Fee`), ez jött: `{}`. {}",
            model,
            usage()
        )));
    }
    if action_text.is_empty() {
        return Err(bad(crate::tf!(
            "name at least one action after `>`. {}",
            "adj meg legalább egy akciót a `>` után. {}",
            usage()
        )));
    }
    let mut actions: Vec<ActionSpec> = Vec::new();
    for piece in split_top(action_text, '+') {
        let spec = parse_action(piece)?;
        if actions.iter().any(|a| a.action == spec.action) {
            return Err(bad(crate::tf!(
                "action `{}` is listed twice",
                "az akció kétszer szerepel: `{}`",
                piece.trim()
            )));
        }
        actions.push(spec);
    }
    let mut spec = Spec {
        model: model.to_owned(),
        options,
        actions,
    };
    share_fields(&mut spec)?;
    Ok(spec)
}

/// A `create` és az `edit` űrlap-mezői: ha az egyik üres, a másikét örökli; ha mindkettő üres, hiba.
fn share_fields(spec: &mut Spec) -> GenResult<()> {
    let create = spec.fields_of(Action::Create).map(<[Field]>::to_vec);
    let edit = spec.fields_of(Action::Edit).map(<[Field]>::to_vec);
    let shared = [create.clone(), edit.clone()]
        .into_iter()
        .flatten()
        .find(|f| !f.is_empty());
    for (action, own) in [(Action::Create, create), (Action::Edit, edit)] {
        if own.as_ref().is_some_and(Vec::is_empty) {
            let Some(fields) = shared.clone() else {
                let word = if action == Action::Create {
                    "create"
                } else {
                    "edit"
                };
                return Err(bad(crate::tf!(
                    "`{}` needs fields, e.g. `{}(name:str,amount:int)`",
                    "a(z) `{}` mezőket vár, pl. `{}(name:str,amount:int)`",
                    word,
                    word
                )));
            };
            if let Some(slot) = spec.actions.iter_mut().find(|a| a.action == action) {
                slot.fields = fields;
            }
        }
    }
    Ok(())
}

/// A névkészlet, amelyből minden generált név származik.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    /// `Fee`
    pub model: String,
    /// `fee` (a route-paraméter és a változó)
    pub var: String,
    /// `fees` (az Inertia prop)
    pub vars: String,
    /// `Fees` (az oldal-mappa)
    pub pages: String,
    /// `FeeController`
    pub controller: String,
    /// `/admin/fees`
    pub uri: String,
    /// `admin.fees` (a route-nevek előtagja)
    pub route: String,
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn lower_first(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().collect::<String>() + chars.as_str()
    })
}

impl Names {
    pub fn of(spec: &Spec) -> Self {
        let table = table_name(&spec.model);
        let pascal: String = table.split('_').map(capitalize).collect();
        let camel = lower_first(&pascal);
        let slug = table.replace('_', "-");
        let (uri, route) = match &spec.options.prefix {
            Some(prefix) => (
                format!("/{prefix}/{slug}"),
                format!("{}.{slug}", prefix.replace('/', ".")),
            ),
            None => (format!("/{slug}"), slug),
        };
        Self {
            model: spec.model.clone(),
            var: lower_first(&spec.model),
            vars: camel,
            pages: pascal,
            controller: format!("{}Controller", spec.model),
            uri,
            route,
        }
    }

    /// Az Inertia-oldal neve (`Fees/Create`).
    pub fn page(&self, name: &str) -> String {
        format!("{}/{name}", self.pages)
    }

    /// A művelet route-neve (`fees.create`).
    pub fn route_name(&self, op: Op) -> String {
        format!("{}.{}", self.route, op.method())
    }

    /// A művelet URI-ja (`/fees/{fee}/edit`).
    pub fn route_uri(&self, op: Op) -> String {
        match op {
            Op::Index | Op::Store => self.uri.clone(),
            Op::Create => format!("{}/create", self.uri),
            Op::Show | Op::Update | Op::Destroy => format!("{}/{{{}}}", self.uri, self.var),
            Op::Edit => format!("{}/{{{}}}/edit", self.uri, self.var),
        }
    }
}

/// Az ember-olvasható szó egy mezőnévből: `paid_at` → `Paid at`.
pub fn humanize(name: &str) -> String {
    capitalize(&name.replace('_', " "))
}

/// A modellnévből ember-olvasható szó: `ServiceJob` → `Service job`.
pub fn humanize_model(model: &str) -> String {
    let mut out = String::new();
    for (i, c) in model.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
