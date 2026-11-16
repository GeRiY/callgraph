//! A preset Inertia / Vue oldalai (`Create`, `Edit`, `Index`, `Show`): a `vue::model::Program` (script-tagok és
//! template-elemek) a Vue-generátor SFC-útján épül fel — `ScriptEmitter`, template-emitter, `SfcBuilder` —, a
//! projekt stílusában (TS / JS, pontosvessző, behúzás), a projekt meglévő komponenseivel (`InputLabel`, `TextInput`,
//! `InputError`, `PrimaryButton`, az `AuthenticatedLayout`), és sima HTML-lel, ahol nincs ilyen komponens.
//! Az ellenőrzést (oxc / tag-egyensúly) a hívó végzi.

use std::collections::BTreeSet;

use super::spec::{Field, FieldType, Names, Op, humanize, humanize_model};
use crate::generate::ast::TypeRef;
use crate::generate::vue::model::{
    Attr, Element, FormField, Import, Kind as ProgramKind, Member, Program, Prop, Script,
    TypeField, VParam,
};
use crate::generate::vue::types::object_ts;
use crate::generate::vue::{CodeStyle, ProjectFacts, VueGenerator};

const INPUT_CLASS: &str = "mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-indigo-500 focus:ring-indigo-500";
const BUTTON_CLASS: &str =
    "rounded-md bg-gray-800 px-4 py-2 text-xs font-semibold uppercase tracking-widest text-white";

/// Az oldal fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Create,
    Edit,
    Index,
    Show,
}

impl Kind {
    /// A fájlnév (`Create` → `Create.vue`).
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Edit => "Edit",
            Self::Index => "Index",
            Self::Show => "Show",
        }
    }
}

/// A projektben talált komponensek: `(név, import-út)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    pub layout: Option<(String, String)>,
    pub label: Option<String>,
    pub input: Option<String>,
    pub error: Option<String>,
    pub button: Option<String>,
    /// Van-e Ziggy (`tightenco/ziggy` / `ziggy-js`): `route('név')`; különben sima URL-ek (`'/fees'`).
    pub ziggy: bool,
}

/// Egy oldal bemenete.
pub struct Page<'a> {
    pub kind: Kind,
    pub names: &'a Names,
    pub fields: &'a [Field],
    pub facts: ProjectFacts,
    pub parts: &'a Parts,
    /// A tudott route-nevek (a linkekhez: csak létező route-ra linkelünk).
    pub routes: &'a BTreeSet<String>,
}

fn is_textlike(ty: FieldType) -> bool {
    matches!(
        ty,
        FieldType::Str | FieldType::Email | FieldType::Date | FieldType::DateTime
    )
}

/// A mező `useForm` alapértéke (JS-kifejezés): a `bool` `false`, a szám `null` (TS-ben típus-állítással), a többi `''`.
fn default_value(field: &Field, ts: bool) -> String {
    match field.ty {
        FieldType::Bool => "false".to_owned(),
        FieldType::Int | FieldType::Float => {
            if ts {
                "null as number | null".to_owned()
            } else {
                "null".to_owned()
            }
        }
        _ => "''".to_owned(),
    }
}

/// A mező nyelvfüggetlen típusa (az opcionális `opt<…>`): a TS-típust a közös `vue::types::ts_type` adja.
fn field_type(field: &Field) -> TypeRef {
    let base = match field.ty {
        FieldType::Int => TypeRef::Int,
        FieldType::Float => TypeRef::Float,
        FieldType::Bool => TypeRef::Bool,
        _ => TypeRef::Str,
    };
    if field.optional {
        TypeRef::Opt(Box::new(base))
    } else {
        base
    }
}

/// Template-elem építő: `el("div").attr("class", "x").text("…")`.
trait Build: Sized {
    fn attr(self, name: &str, value: impl Into<String>) -> Self;
    fn flag(self, name: &str) -> Self;
    fn text(self, text: impl Into<String>) -> Self;
    fn kid(self, child: Element) -> Self;
    fn kids(self, children: Vec<Element>) -> Self;
}

fn el(tag: &str) -> Element {
    Element {
        tag: tag.to_owned(),
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
        text: None,
        children: Vec::new(),
    }
}

impl Build for Element {
    fn attr(mut self, name: &str, value: impl Into<String>) -> Self {
        self.attrs.push(Attr {
            name: name.to_owned(),
            value: Some(value.into()),
        });
        self
    }

    fn flag(mut self, name: &str) -> Self {
        self.attrs.push(Attr {
            name: name.to_owned(),
            value: None,
        });
        self
    }

    fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    fn kid(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    fn kids(mut self, children: Vec<Element>) -> Self {
        self.children.extend(children);
        self
    }
}

impl Page<'_> {
    fn semi(&self) -> &'static str {
        if self.facts.semi { ";" } else { "" }
    }

    fn title(&self) -> String {
        let model = humanize_model(&self.names.model);
        match self.kind {
            Kind::Create => format!("Create {model}"),
            Kind::Edit => format!("Edit {model}"),
            Kind::Index => humanize_model(&self.names.pages),
            Kind::Show => model,
        }
    }

    fn known(&self, op: Op) -> Option<String> {
        let name = self.names.route_name(op);
        self.routes.contains(&name).then_some(name)
    }

    /// A művelet címe JS-kifejezésként: Ziggy-vel `route('fees.edit', id)`, nélküle `` `/fees/${id}/edit` ``.
    fn href(&self, op: Op, id: Option<&str>) -> String {
        let name = self.names.route_name(op);
        if self.parts.ziggy {
            return match id {
                Some(id) => format!("route('{name}', {id})"),
                None => format!("route('{name}')"),
            };
        }
        let uri = self.names.route_uri(op);
        match id {
            Some(id) => {
                let param = format!("{{{}}}", self.names.var);
                format!("`{}`", uri.replace(&param, &format!("${{{id}}}")))
            }
            None => format!("'{uri}'"),
        }
    }

    fn is_form(&self) -> bool {
        matches!(self.kind, Kind::Create | Kind::Edit)
    }

    // ---- script -----------------------------------------------------------------------------------------------

    /// A használt komponensek `(név, út)` párjai, névsorban.
    fn components(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        if let Some(layout) = &self.parts.layout {
            out.push(layout.clone());
        }
        if self.is_form() {
            let pairs = [
                ("InputError", &self.parts.error, true),
                (
                    "InputLabel",
                    &self.parts.label,
                    self.fields.iter().any(|f| f.ty != FieldType::Bool),
                ),
                ("PrimaryButton", &self.parts.button, true),
                (
                    "TextInput",
                    &self.parts.input,
                    self.fields.iter().any(|f| is_textlike(f.ty)),
                ),
            ];
            for (name, path, used) in pairs {
                if let (Some(path), true) = (path, used) {
                    out.push((name.to_owned(), path.clone()));
                }
            }
        }
        out.sort();
        out
    }

    /// Az Inertia névvel importált tagjai: `Head`, a form-oldalon a `useForm` (a `form` tag igényli, az emitter
    /// adja hozzá), az index-oldalon a `Link` és a `router`.
    fn inertia_names(&self) -> Vec<String> {
        let mut names = vec!["Head"];
        if self.kind == Kind::Index {
            if self.row_links() {
                names.push("Link");
            }
            if self.known(Op::Destroy).is_some() {
                names.push("router");
            }
        }
        names.into_iter().map(str::to_owned).collect()
    }

    /// Kell-e a `Link` az index-oldalon (új / szerkesztés / megnézés link).
    fn row_links(&self) -> bool {
        self.known(Op::Create).is_some()
            || self.known(Op::Edit).is_some()
            || self.known(Op::Show).is_some()
    }

    fn script(&self) -> Script {
        let members = match self.kind {
            Kind::Create => vec![
                self.form_member(None),
                self.submit_member("post", Op::Store, None),
            ],
            Kind::Edit => {
                let id = format!("props.{}.id", self.names.var);
                vec![
                    self.props_member(),
                    self.form_member(Some(self.names.var.as_str())),
                    self.submit_member("put", Op::Update, Some(id.as_str())),
                ]
            }
            Kind::Index => {
                let mut members = vec![self.props_member()];
                if self.known(Op::Destroy).is_some() {
                    members.push(self.destroy_member());
                }
                members
            }
            Kind::Show => vec![self.props_member()],
        };
        let mut imports: Vec<Import> = self
            .components()
            .into_iter()
            .map(|(name, source)| Import {
                source,
                default: Some(name),
                named: Vec::new(),
            })
            .collect();
        imports.push(Import {
            source: "@inertiajs/vue3".to_owned(),
            default: None,
            named: self.inertia_names(),
        });
        Script {
            members,
            imports,
            guess: Vec::new(),
        }
    }

    /// `const form = useForm({ … })`; `prop`: az Edit-nél a prop neve, ahonnan az értékek jönnek.
    fn form_member(&self, prop: Option<&str>) -> Member {
        let fields = self
            .fields
            .iter()
            .map(|field| FormField {
                name: field.name.clone(),
                ty: None,
                default: Some(match prop {
                    Some(var) => format!("props.{var}.{}", field.name),
                    None => default_value(field, self.facts.ts),
                }),
            })
            .collect();
        Member::Form {
            name: "form".to_owned(),
            fields,
        }
    }

    fn func(name: &str, params: Vec<VParam>, body: String) -> Member {
        Member::Func {
            name: name.to_owned(),
            params,
            ret: None,
            body: Some(body),
            is_async: false,
        }
    }

    fn submit_member(&self, verb: &str, op: Op, id: Option<&str>) -> Member {
        let body = format!("form.{verb}({}){}", self.href(op, id), self.semi());
        Self::func("submit", Vec::new(), body)
    }

    fn destroy_member(&self) -> Member {
        let e = self.semi();
        let body = format!(
            "if (confirm('Delete this item?')) {{\n  router.delete({}){e}\n}}",
            self.href(Op::Destroy, Some("id"))
        );
        let id = VParam {
            name: "id".to_owned(),
            ty: Some(TypeRef::Int),
            default: None,
            optional: false,
            rest: false,
        };
        Self::func("destroy", vec![id], body)
    }

    /// A TS-objektumtípus (`{ id: number; name: string }`): `id` + a mezők, a típusok a közös `ts_type`-pal.
    fn row_type(&self) -> String {
        let mut fields = vec![TypeField {
            name: "id".to_owned(),
            optional: false,
            ty: TypeRef::Int,
        }];
        fields.extend(self.fields.iter().map(|f| TypeField {
            name: f.name.clone(),
            optional: false,
            ty: field_type(f),
        }));
        object_ts(&fields, "; ")
    }

    /// A `defineProps`: Edit / Show: az `item` objektum; Index: a lapozott lista (`{ data: item[] }`).
    fn props_member(&self) -> Member {
        let row = self.row_type();
        let (name, ty) = if matches!(self.kind, Kind::Edit | Kind::Show) {
            (self.names.var.clone(), row)
        } else {
            (self.names.vars.clone(), format!("{{ data: {row}[] }}"))
        };
        Member::Props(vec![Prop {
            name,
            ty: Some(TypeRef::Named(ty)),
            required: true,
            default: None,
        }])
    }

    // ---- template ---------------------------------------------------------------------------------------------

    fn label_el(&self, field: &Field) -> Element {
        let (name, text) = (&field.name, humanize(&field.name));
        if self.parts.label.is_some() {
            el("InputLabel").attr("for", name).attr("value", text)
        } else {
            el("label")
                .attr("for", name)
                .attr("class", "block text-sm font-medium text-gray-700")
                .text(text)
        }
    }

    fn error_el(&self, field: &Field) -> Element {
        let name = &field.name;
        if self.parts.error.is_some() {
            el("InputError")
                .attr("class", "mt-2")
                .attr(":message", format!("form.errors.{name}"))
        } else {
            el("p")
                .attr("v-if", format!("form.errors.{name}"))
                .attr("class", "mt-2 text-sm text-red-600")
                .text(format!("{{{{ form.errors.{name} }}}}"))
        }
    }

    /// Egy mező blokkja: címke, beviteli elem, hibaüzenet.
    fn field_block(&self, field: &Field) -> Element {
        let name = &field.name;
        let model = format!("form.{name}");
        let required = !(field.optional || field.ty == FieldType::Bool);
        let mark = |element: Element| {
            if required {
                element.flag("required")
            } else {
                element
            }
        };
        let block = el("div");
        let block = match field.ty {
            FieldType::Bool => block.kid(
                el("label")
                    .attr("class", "flex items-center gap-2 text-sm text-gray-700")
                    .kid(
                        el("input")
                            .attr("id", name)
                            .attr("type", "checkbox")
                            .attr(
                                "class",
                                "rounded border-gray-300 text-indigo-600 shadow-sm focus:ring-indigo-500",
                            )
                            .attr("v-model", &model),
                    )
                    .kid(el("span").text(humanize(name))),
            ),
            FieldType::Text => block.kid(self.label_el(field)).kid(mark(
                el("textarea")
                    .attr("id", name)
                    .attr("rows", "4")
                    .attr("class", INPUT_CLASS)
                    .attr("v-model", &model),
            )),
            FieldType::Int | FieldType::Float => {
                let mut input = el("input").attr("id", name).attr("type", "number");
                if field.ty == FieldType::Float {
                    input = input.attr("step", "any");
                }
                block.kid(self.label_el(field)).kid(mark(
                    input
                        .attr("class", INPUT_CLASS)
                        .attr("v-model", &model),
                ))
            }
            FieldType::Str | FieldType::Email | FieldType::Date | FieldType::DateTime => {
                let kind = match field.ty {
                    FieldType::Email => "email",
                    FieldType::Date => "date",
                    FieldType::DateTime => "datetime-local",
                    _ => "text",
                };
                let (tag, class) = if self.parts.input.is_some() {
                    ("TextInput", "mt-1 block w-full")
                } else {
                    ("input", INPUT_CLASS)
                };
                block.kid(self.label_el(field)).kid(mark(
                    el(tag)
                        .attr("id", name)
                        .attr("type", kind)
                        .attr("class", class)
                        .attr("v-model", &model),
                ))
            }
        };
        block.kid(self.error_el(field))
    }

    fn form_body(&self) -> Element {
        let submit = if self.parts.button.is_some() {
            el("PrimaryButton")
                .attr(":disabled", "form.processing")
                .text("Save")
        } else {
            el("button")
                .attr("type", "submit")
                .attr(":disabled", "form.processing")
                .attr("class", format!("{BUTTON_CLASS} disabled:opacity-25"))
                .text("Save")
        };
        el("form")
            .attr("@submit.prevent", "submit")
            .attr("class", "space-y-6 bg-white p-6 shadow sm:rounded-lg")
            .kids(self.fields.iter().map(|f| self.field_block(f)).collect())
            .kid(
                el("div")
                    .attr("class", "flex items-center gap-4")
                    .kid(submit),
            )
    }

    /// A `{{ item.x }}` cella-kifejezés.
    fn cell(item: &str, field: &Field) -> String {
        if field.ty == FieldType::Bool {
            format!("{{{{ {item}.{} ? 'Yes' : 'No' }}}}", field.name)
        } else {
            format!("{{{{ {item}.{} }}}}", field.name)
        }
    }

    fn index_body(&self) -> Vec<Element> {
        let mut out = Vec::new();
        if self.known(Op::Create).is_some() {
            out.push(
                el("div").attr("class", "mb-4 flex justify-end").kid(
                    el("Link")
                        .attr(":href", self.href(Op::Create, None))
                        .attr("class", BUTTON_CLASS)
                        .text(format!("New {}", humanize_model(&self.names.model))),
                ),
            );
        }
        let (edit, show, destroy) = (
            self.known(Op::Edit),
            self.known(Op::Show),
            self.known(Op::Destroy),
        );
        let actions = edit.is_some() || show.is_some() || destroy.is_some();
        let th = "px-4 py-3 text-left text-xs font-medium uppercase text-gray-500";
        let mut head = el("tr").kid(el("th").attr("class", th).text("ID"));
        for field in self.fields {
            head = head.kid(el("th").attr("class", th).text(humanize(&field.name)));
        }
        if actions {
            head = head.kid(el("th").attr("class", "px-4 py-3"));
        }
        let td = "px-4 py-3 text-sm text-gray-900";
        let mut row = el("tr")
            .attr("v-for", format!("item in {}.data", self.names.vars))
            .attr(":key", "item.id")
            .kid(el("td").attr("class", td).text("{{ item.id }}"));
        for field in self.fields {
            row = row.kid(el("td").attr("class", td).text(Self::cell("item", field)));
        }
        if actions {
            let link = "ml-3 text-indigo-600 hover:text-indigo-900";
            let mut cell = el("td").attr("class", "px-4 py-3 text-right text-sm");
            if show.is_some() {
                cell = cell.kid(
                    el("Link")
                        .attr(":href", self.href(Op::Show, Some("item.id")))
                        .attr("class", link)
                        .text("Show"),
                );
            }
            if edit.is_some() {
                cell = cell.kid(
                    el("Link")
                        .attr(":href", self.href(Op::Edit, Some("item.id")))
                        .attr("class", link)
                        .text("Edit"),
                );
            }
            if destroy.is_some() {
                cell = cell.kid(
                    el("button")
                        .attr("type", "button")
                        .attr("class", "ml-3 text-red-600 hover:text-red-900")
                        .attr("@click", "destroy(item.id)")
                        .text("Delete"),
                );
            }
            row = row.kid(cell);
        }
        out.push(
            el("div")
                .attr("class", "overflow-hidden bg-white shadow sm:rounded-lg")
                .kid(
                    el("table")
                        .attr("class", "min-w-full divide-y divide-gray-200")
                        .kid(el("thead").attr("class", "bg-gray-50").kid(head))
                        .kid(
                            el("tbody")
                                .attr("class", "divide-y divide-gray-200")
                                .kid(row),
                        ),
                ),
        );
        out
    }

    fn show_body(&self) -> Element {
        let var = self.names.var.as_str();
        let mut list = el("dl").attr(
            "class",
            "divide-y divide-gray-200 bg-white px-6 shadow sm:rounded-lg",
        );
        for field in self.fields {
            list = list.kid(
                el("div")
                    .attr("class", "py-4")
                    .kid(
                        el("dt")
                            .attr("class", "text-sm font-medium text-gray-500")
                            .text(humanize(&field.name)),
                    )
                    .kid(
                        el("dd")
                            .attr("class", "mt-1 text-sm text-gray-900")
                            .text(Self::cell(var, field)),
                    ),
            );
        }
        list
    }

    fn template(&self) -> Vec<Element> {
        let title = self.title();
        let header_slot = self
            .parts
            .layout
            .as_ref()
            .is_some_and(|(name, _)| name == "AuthenticatedLayout");
        let mut body = Vec::new();
        if !header_slot {
            body.push(
                el("h1")
                    .attr("class", "mb-4 text-xl font-semibold text-gray-800")
                    .text(&title),
            );
        }
        match self.kind {
            Kind::Create | Kind::Edit => body.push(self.form_body()),
            Kind::Index => body.extend(self.index_body()),
            Kind::Show => body.push(self.show_body()),
        }
        let page = el("div").attr("class", "py-12").kid(
            el("div")
                .attr("class", "mx-auto max-w-7xl sm:px-6 lg:px-8")
                .kids(body),
        );
        let head = el("Head").attr("title", &title);
        let Some((name, _)) = &self.parts.layout else {
            return vec![head, page];
        };
        let mut layout = el(name);
        if header_slot {
            layout = layout.kid(
                el("template").flag("#header").kid(
                    el("h2")
                        .attr("class", "text-xl font-semibold leading-tight text-gray-800")
                        .text(&title),
                ),
            );
        }
        vec![head, layout.kid(page)]
    }

    /// A teljes `.vue` fájl, záró újsorral: a modell a Vue-generátor SFC-útján, a projekt behúzás-egységében.
    pub fn render(&self, sfc: &VueGenerator<'_>) -> String {
        let program = Program {
            kind: ProgramKind::Sfc,
            name: None,
            script: Some(self.script()),
            template: Some(self.template()),
            style: None,
        };
        let style = CodeStyle {
            ts: self.facts.ts,
            semi: self.facts.semi,
        };
        let text = sfc.sfc_page(&program, style, self.facts.unit);
        // a `const props =` kötés csak ott marad, ahol a script használja (`props.<var>.id`): máshol a
        // `noUnusedLocals` (vue-tsc) „declared but never read” hibát adna
        if text.contains("props.") {
            text
        } else {
            text.replacen("const props = ", "", 1)
        }
    }
}
