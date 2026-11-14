//! A preset PHP-oldala: a FormRequest-osztály, a controller-metódusok (új fájlhoz és beszúráshoz) és a route-sorok
//! szövege. Tiszta szövegépítés (I/O nélkül); az ellenőrzést és az írást a hívó végzi.

use super::spec::{Field, FieldType, Names, Op};

const IND: &str = "    ";

/// A mező validációs tokenjei: `str` → `required|string|max:255`, a `bool` sosem kötelező, az opcionális `nullable`.
pub fn rules(field: &Field) -> Vec<&'static str> {
    let presence = if field.optional {
        "nullable"
    } else {
        "required"
    };
    match field.ty {
        FieldType::Bool => vec!["boolean"],
        FieldType::Str => vec![presence, "string", "max:255"],
        FieldType::Text => vec![presence, "string"],
        FieldType::Int => vec![presence, "integer"],
        FieldType::Float => vec![presence, "numeric"],
        FieldType::Date | FieldType::DateTime => vec![presence, "date"],
        FieldType::Email => vec![presence, "email"],
    }
}

/// A `<?php` fejléc: `declare(strict_types=1)` (ha kell), névtér, rendezett importok; záró üres sorral.
fn header(strict: bool, namespace: &str, imports: &[String]) -> Vec<String> {
    let mut lines = vec!["<?php".to_owned(), String::new()];
    if strict {
        lines.push("declare(strict_types=1);".to_owned());
        lines.push(String::new());
    }
    if !namespace.is_empty() {
        lines.push(format!("namespace {namespace};"));
        lines.push(String::new());
    }
    let mut sorted: Vec<&String> = imports.iter().collect();
    sorted.sort_by_key(|i| i.to_ascii_lowercase());
    sorted.dedup();
    for import in &sorted {
        lines.push(format!("use {import};"));
    }
    if !sorted.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Az osztály-névből és a névtérből a teljes név.
pub fn qualify(namespace: &str, class: &str) -> String {
    if namespace.is_empty() {
        class.to_owned()
    } else {
        format!("{namespace}\\{class}")
    }
}

/// A teljes FormRequest-fájl. `pipe`: a szabályok `'required|string'` alakban (a projekt kérései szerint).
pub fn request_file(
    strict: bool,
    namespace: &str,
    class: &str,
    fields: &[Field],
    pipe: bool,
) -> String {
    let mut lines = header(
        strict,
        namespace,
        &["Illuminate\\Foundation\\Http\\FormRequest".to_owned()],
    );
    lines.push(format!("class {class} extends FormRequest"));
    lines.push("{".to_owned());
    lines.push(format!("{IND}public function authorize(): bool"));
    lines.push(format!("{IND}{{"));
    lines.push(format!("{IND}{IND}return true;"));
    lines.push(format!("{IND}}}"));
    lines.push(String::new());
    lines.push(format!("{IND}/**"));
    lines.push(format!(
        "{IND} * @return array<string, array<int, string>|string>"
    ));
    lines.push(format!("{IND} */"));
    lines.push(format!("{IND}public function rules(): array"));
    lines.push(format!("{IND}{{"));
    lines.push(format!("{IND}{IND}return ["));
    for field in fields {
        let tokens = rules(field);
        let value = if pipe {
            format!("'{}'", tokens.join("|"))
        } else {
            let quoted: Vec<String> = tokens.iter().map(|t| format!("'{t}'")).collect();
            format!("[{}]", quoted.join(", "))
        };
        lines.push(format!("{IND}{IND}{IND}'{}' => {value},", field.name));
    }
    lines.push(format!("{IND}{IND}];"));
    lines.push(format!("{IND}}}"));
    lines.push("}".to_owned());
    lines.push(String::new());
    lines.join("\n")
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

    /// A művelet metódusa (4 szóközös behúzással).
    pub fn method(&self, op: Op) -> Vec<String> {
        let n = self.names;
        let (model, var) = (&n.model, &n.var);
        let m = |text: String| format!("{IND}{text}");
        let b = |text: String| format!("{IND}{IND}{text}");
        let mut lines: Vec<String> = Vec::new();
        match op {
            Op::Index => {
                lines.push(m("public function index(): Response".to_owned()));
                lines.push(m("{".to_owned()));
                lines.push(b(format!(
                    "return Inertia::render('{}', [",
                    n.page("Index")
                )));
                lines.push(format!(
                    "{IND}{IND}{IND}'{}' => {model}::latest()->paginate(20),",
                    n.vars
                ));
                lines.push(b("]);".to_owned()));
            }
            Op::Create => {
                lines.push(m("public function create(): Response".to_owned()));
                lines.push(m("{".to_owned()));
                lines.push(b(format!(
                    "return Inertia::render('{}');",
                    n.page("Create")
                )));
            }
            Op::Store => {
                lines.push(m(format!(
                    "public function store({} $request): RedirectResponse",
                    self.store_request()
                )));
                lines.push(m("{".to_owned()));
                lines.push(b(format!("{model}::create($request->validated());")));
                lines.push(String::new());
                lines.push(b(format!("return {};", self.redirect())));
            }
            Op::Show | Op::Edit => {
                let (word, page) = if op == Op::Show {
                    ("show", "Show")
                } else {
                    ("edit", "Edit")
                };
                lines.push(m(format!(
                    "public function {word}({model} ${var}): Response"
                )));
                lines.push(m("{".to_owned()));
                lines.push(b(format!("return Inertia::render('{}', [", n.page(page))));
                lines.push(format!("{IND}{IND}{IND}'{var}' => ${var},"));
                lines.push(b("]);".to_owned()));
            }
            Op::Update => {
                lines.push(m(format!(
                    "public function update({} $request, {model} ${var}): RedirectResponse",
                    self.update_request()
                )));
                lines.push(m("{".to_owned()));
                lines.push(b(format!("${var}->update($request->validated());")));
                lines.push(String::new());
                lines.push(b(format!("return {};", self.redirect())));
            }
            Op::Destroy => {
                lines.push(m(format!(
                    "public function destroy({model} ${var}): RedirectResponse"
                )));
                lines.push(m("{".to_owned()));
                lines.push(b(format!("${var}->delete();")));
                lines.push(String::new());
                lines.push(b(format!("return {};", self.redirect())));
            }
        }
        lines.push(m("}".to_owned()));
        lines
    }

    /// A metódusok egymás után, üres sorral elválasztva.
    pub fn methods(&self, ops: &[Op]) -> Vec<String> {
        let mut block: Vec<String> = Vec::new();
        for op in ops {
            if !block.is_empty() {
                block.push(String::new());
            }
            block.extend(self.method(*op));
        }
        block
    }

    /// A teljes új controller-fájl.
    pub fn file(&self, strict: bool, namespace: &str, ops: &[Op]) -> String {
        let imports: Vec<String> = ops.iter().flat_map(|op| self.imports(*op)).collect();
        let mut lines = header(strict, namespace, &imports);
        lines.push(format!(
            "class {} extends Controller",
            self.names.controller
        ));
        lines.push("{".to_owned());
        lines.extend(self.methods(ops));
        lines.push("}".to_owned());
        lines.push(String::new());
        lines.join("\n")
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
