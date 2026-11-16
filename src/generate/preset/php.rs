//! A preset PHP-oldala: a FormRequest-osztály és a controller-metódusok (új fájlhoz és beszúráshoz) a közös AST-ből
//! (`ast::TypeDecl` / `Member`), a `PhpEmitter` és a `PhpFileBuilder` útján, valamint a route-sorok szövege.
//! I/O nélküli; az ellenőrzést és az írást a hívó végzi.

use super::columns::StrLimit;
use super::spec::{Field, FieldType, Names, Op};
use crate::generate::PhpEmitter;
use crate::generate::ast::{Member, MemberKind, Param, TypeDecl, TypeKind, TypeRef, Visibility};
use crate::generate::emitter::{Emitter, FileSpec};

/// Behúzás egy szinten a nyers törzsekben (a `PhpEmitter` a törzset további két szinttel húzza be).
const IND: &str = "    ";

/// A mező validációs tokenjei: `str` → `required|string|max:255` (a migráció hosszkorlátja szerint: `text` oszlopra
/// `max` nélkül, `string('x', 100)`-ra `max:100`), a `bool` sosem kötelező, az opcionális `nullable`.
pub fn rules(field: &Field, limit: StrLimit) -> Vec<String> {
    let presence = if field.optional {
        "nullable"
    } else {
        "required"
    };
    let base: Vec<&str> = match field.ty {
        FieldType::Bool => vec!["boolean"],
        FieldType::Str | FieldType::Text => vec![presence, "string"],
        FieldType::Int => vec![presence, "integer"],
        FieldType::Float => vec![presence, "numeric"],
        FieldType::Date | FieldType::DateTime => vec![presence, "date"],
        FieldType::Email => vec![presence, "email"],
    };
    let mut tokens: Vec<String> = base.into_iter().map(str::to_owned).collect();
    if field.ty == FieldType::Str {
        match limit {
            StrLimit::Default => tokens.push("max:255".to_owned()),
            StrLimit::Len(n) => tokens.push(format!("max:{n}")),
            StrLimit::Unlimited => {}
        }
    }
    tokens
}

/// Az osztály-névből és a névtérből a teljes név.
pub fn qualify(namespace: &str, class: &str) -> String {
    if namespace.is_empty() {
        class.to_owned()
    } else {
        format!("{namespace}\\{class}")
    }
}

/// A fájl az emitter fejléc-építőjén át: `declare(strict_types=1)` (ha kell), névtér, rendezett importok.
fn file_text(strict: bool, namespace: &str, imports: Vec<String>, decl: &TypeDecl) -> String {
    let imports: Vec<(String, Option<String>)> = imports.into_iter().map(|i| (i, None)).collect();
    let spec = FileSpec {
        namespace: (!namespace.is_empty()).then_some(namespace),
        imports: &imports,
        strict_types: strict,
    };
    PhpEmitter.file(&spec, std::slice::from_ref(decl))
}

fn named(name: &str) -> TypeRef {
    TypeRef::Named(name.to_owned())
}

fn param(name: &str, ty: &str) -> Param {
    Param {
        name: name.to_owned(),
        ty: Some(named(ty)),
        default: None,
        variadic: false,
        by_ref: false,
        promote: None,
        readonly: false,
    }
}

/// Egy publikus metódus a megadott visszatérési típussal és nyers törzzsel.
fn method(name: &str, params: Vec<Param>, ret: TypeRef, body: String) -> Member {
    let mut member = Member::new(MemberKind::Method, name);
    member.visibility = Some(Visibility::Public);
    member.params = params;
    member.ty = Some(ret);
    member.body = Some(body);
    member
}

/// A mező fordítási kulcsa (`fees.fields.name`): az oldalak címkéi és a kérés `attributes()`-e ugyanezt használják.
pub fn field_key(prefix: &str, field: &Field) -> String {
    format!("{prefix}.fields.{}", field.name)
}

/// A teljes FormRequest-fájl. `pipe`: a szabályok `'required|string'` alakban (a projekt kérései szerint);
/// `translate`: a kulcs-előtag (`fees`), ha a projekt fordít: akkor az `attributes()` a mezőnevekhez `__('kulcs')`-ot ad,
/// így a validációs üzenetek a fordított mezőnevet használják.
pub fn request_file(
    strict: bool,
    namespace: &str,
    class: &str,
    fields: &[Field],
    pipe: bool,
    translate: Option<&str>,
    limits: &std::collections::BTreeMap<String, StrLimit>,
) -> String {
    let mut body = vec!["return [".to_owned()];
    for field in fields {
        let tokens = rules(field, limits.get(&field.name).copied().unwrap_or_default());
        let value = if pipe {
            format!("'{}'", tokens.join("|"))
        } else {
            let quoted: Vec<String> = tokens.iter().map(|t| format!("'{t}'")).collect();
            format!("[{}]", quoted.join(", "))
        };
        body.push(format!("{IND}'{}' => {value},", field.name));
    }
    body.push("];".to_owned());
    // a docblock a szabály alakját írja le: csöves szabály `string`, tömbös `list<string>`
    let rule = if pipe {
        TypeRef::Str
    } else {
        TypeRef::Union(vec![TypeRef::List(Box::new(TypeRef::Str)), TypeRef::Str])
    };
    let mut decl = TypeDecl::new(TypeKind::Class, class);
    decl.extends = vec![named("FormRequest")];
    decl.members = vec![
        method(
            "authorize",
            Vec::new(),
            TypeRef::Bool,
            "return true;".to_owned(),
        ),
        method(
            "rules",
            Vec::new(),
            TypeRef::Map(Box::new(TypeRef::Str), Box::new(rule)),
            body.join("\n"),
        ),
    ];
    if let Some(prefix) = translate {
        let mut names = vec!["return [".to_owned()];
        for field in fields {
            names.push(format!(
                "{IND}'{}' => __('{}'),",
                field.name,
                field_key(prefix, field)
            ));
        }
        names.push("];".to_owned());
        decl.members.push(method(
            "attributes",
            Vec::new(),
            TypeRef::Map(Box::new(TypeRef::Str), Box::new(TypeRef::Str)),
            names.join("\n"),
        ));
    }
    file_text(
        strict,
        namespace,
        vec!["Illuminate\\Foundation\\Http\\FormRequest".to_owned()],
        &decl,
    )
}

/// A controller-generálás környezete.
pub struct ControllerCtx<'a> {
    pub names: &'a Names,
    /// A modell és a kérés-osztályok névtere.
    pub model_ns: &'a str,
    pub request_ns: &'a str,
    /// Van-e index route (akkor a mentés után oda irányít, különben `back()`).
    pub has_index: bool,
}

impl ControllerCtx<'_> {
    fn model_fqcn(&self) -> String {
        qualify(self.model_ns, &self.names.model)
    }

    /// A `store` kérés-osztálya.
    pub fn store_request(&self) -> String {
        format!("Store{}Request", self.names.model)
    }

    /// Az `update` kérés-osztálya.
    pub fn update_request(&self) -> String {
        format!("Update{}Request", self.names.model)
    }

    fn redirect(&self) -> String {
        if self.has_index {
            format!("to_route('{}')", self.names.route_name(Op::Index))
        } else {
            "back()".to_owned()
        }
    }

    /// Az importok (teljes osztálynevek), amelyeket a művelet kér.
    pub fn imports(&self, op: Op) -> Vec<String> {
        let mut out = Vec::new();
        match op {
            Op::Index | Op::Create | Op::Show | Op::Edit => {
                out.push("Inertia\\Inertia".to_owned());
                out.push("Inertia\\Response".to_owned());
            }
            Op::Store | Op::Update | Op::Destroy => {
                out.push("Illuminate\\Http\\RedirectResponse".to_owned());
            }
        }
        if op != Op::Create {
            out.push(self.model_fqcn());
        }
        match op {
            Op::Store => out.push(qualify(self.request_ns, &self.store_request())),
            Op::Update => out.push(qualify(self.request_ns, &self.update_request())),
            _ => {}
        }
        out
    }

    /// A művelet metódusa az AST-ben (a törzs nyers PHP, a kiírást a `PhpEmitter` végzi).
    pub fn method(&self, op: Op) -> Member {
        let n = self.names;
        let (model, var) = (&n.model, &n.var);
        match op {
            Op::Index => method(
                "index",
                Vec::new(),
                named("Response"),
                format!(
                    "return Inertia::render('{}', [\n{IND}'{}' => {model}::latest()->paginate(20),\n]);",
                    n.page("Index"),
                    n.vars
                ),
            ),
            Op::Create => method(
                "create",
                Vec::new(),
                named("Response"),
                format!("return Inertia::render('{}');", n.page("Create")),
            ),
            Op::Store => method(
                "store",
                vec![param("request", &self.store_request())],
                named("RedirectResponse"),
                format!(
                    "{model}::create($request->validated());\n\nreturn {};",
                    self.redirect()
                ),
            ),
            Op::Show | Op::Edit => {
                let (word, page) = if op == Op::Show {
                    ("show", "Show")
                } else {
                    ("edit", "Edit")
                };
                method(
                    word,
                    vec![param(var, model)],
                    named("Response"),
                    format!(
                        "return Inertia::render('{}', [\n{IND}'{var}' => ${var},\n]);",
                        n.page(page)
                    ),
                )
            }
            Op::Update => method(
                "update",
                vec![param("request", &self.update_request()), param(var, model)],
                named("RedirectResponse"),
                format!(
                    "${var}->update($request->validated());\n\nreturn {};",
                    self.redirect()
                ),
            ),
            Op::Destroy => method(
                "destroy",
                vec![param(var, model)],
                named("RedirectResponse"),
                format!("${var}->delete();\n\nreturn {};", self.redirect()),
            ),
        }
    }

    /// A metódusok az AST-ben, a műveletek sorrendjében.
    pub fn members(&self, ops: &[Op]) -> Vec<Member> {
        ops.iter().map(|op| self.method(*op)).collect()
    }

    /// A metódusok sorai a `PhpEmitter`-rel (4 szóközös behúzás, üres sorral elválasztva); `kind`: a befogadó típus.
    pub fn methods(&self, kind: TypeKind, ops: &[Op]) -> Vec<String> {
        PhpEmitter.members(kind, &self.members(ops)).collect()
    }

    /// A teljes új controller-fájl.
    pub fn file(&self, strict: bool, namespace: &str, ops: &[Op]) -> String {
        let imports: Vec<String> = ops.iter().flat_map(|op| self.imports(*op)).collect();
        let mut decl = TypeDecl::new(TypeKind::Class, &self.names.controller);
        decl.extends = vec![named("Controller")];
        decl.members = self.members(ops);
        file_text(strict, namespace, imports, &decl)
    }
}

/// Egy route-sor: `Route::get('/fees', [FeeController::class, 'index'])->name('fees.index');`
pub fn route_line(names: &Names, op: Op) -> String {
    format!(
        "Route::{}('{}', [{}::class, '{}'])->name('{}');",
        op.verb().to_ascii_lowercase(),
        names.route_uri(op),
        names.controller,
        op.method(),
        names.route_name(op)
    )
}
