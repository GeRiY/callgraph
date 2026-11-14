//! A preset Inertia / Vue oldalai (`Create`, `Edit`, `Index`, `Show`): a `<script setup>` a projekt stílusában
//! (TS / JS, pontosvessző, behúzás), a template a projekt meglévő komponenseivel (`InputLabel`, `TextInput`,
//! `InputError`, `PrimaryButton`, az `AuthenticatedLayout`), és sima HTML-lel, ahol nincs ilyen komponens.
//! Tiszta szövegépítés; az ellenőrzést (oxc / tag-egyensúly) a hívó végzi.

use std::collections::BTreeSet;

use super::spec::{Field, FieldType, Names, Op, humanize, humanize_model};
use crate::generate::vue::ProjectFacts;

const INPUT_CLASS: &str = "mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-indigo-500 focus:ring-indigo-500";

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

/// Behúzással író sor-gyűjtő.
struct Out {
    unit: usize,
    lines: Vec<String>,
}

impl Out {
    fn push(&mut self, level: usize, text: impl AsRef<str>) {
        let text = text.as_ref();
        if text.is_empty() {
            self.lines.push(String::new());
        } else {
            self.lines
                .push(format!("{}{text}", " ".repeat(self.unit * level)));
        }
    }
}

fn is_textlike(ty: FieldType) -> bool {
    matches!(
        ty,
        FieldType::Str | FieldType::Email | FieldType::Date | FieldType::DateTime
    )
}

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

fn ts_type(field: &Field) -> String {
    let base = match field.ty {
        FieldType::Int | FieldType::Float => "number",
        FieldType::Bool => "boolean",
        _ => "string",
    };
    if field.optional {
        format!("{base} | null")
    } else {
        base.to_owned()
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

    fn inertia_import(&self) -> String {
        let mut names = vec!["Head"];
        match self.kind {
            Kind::Create | Kind::Edit => names.push("useForm"),
            Kind::Index => {
                if self.row_links() {
                    names.push("Link");
                }
                if self.known(Op::Destroy).is_some() {
                    names.push("router");
                }
            }
            Kind::Show => {}
        }
        names.sort_by_key(|n| n.to_ascii_lowercase());
        format!(
            "import {{ {} }} from '@inertiajs/vue3'{}",
            names.join(", "),
            self.semi()
        )
    }

    /// Kell-e a `Link` az index-oldalon (új / szerkesztés / megnézés link).
    fn row_links(&self) -> bool {
        self.known(Op::Create).is_some()
            || self.known(Op::Edit).is_some()
            || self.known(Op::Show).is_some()
    }

    fn script(&self, out: &mut Out) {
        let e = self.semi();
        out.push(
            0,
            if self.facts.ts {
                "<script setup lang=\"ts\">"
            } else {
                "<script setup>"
            },
        );
        for (name, path) in self.components() {
            out.push(0, format!("import {name} from '{path}'{e}"));
        }
        out.push(0, self.inertia_import());
        out.push(0, "");
        match self.kind {
            Kind::Create => {
                self.form_decl(out, None);
                out.push(0, "");
                self.submit_fn(out, "post", &self.names.route_name(Op::Store), None);
            }
            Kind::Edit => {
                self.props_decl(out, true);
                out.push(0, "");
                self.form_decl(out, Some(self.names.var.as_str()));
                out.push(0, "");
                let id = format!("props.{}.id", self.names.var);
                self.submit_fn(
                    out,
                    "put",
                    &self.names.route_name(Op::Update),
                    Some(id.as_str()),
                );
            }
            Kind::Index => {
                self.props_decl(out, false);
                if let Some(route) = self.known(Op::Destroy) {
                    out.push(0, "");
                    let arg = if self.facts.ts { "id: number" } else { "id" };
                    out.push(0, format!("const destroy = ({arg}) => {{"));
                    out.push(1, "if (confirm('Delete this item?')) {");
                    out.push(2, format!("router.delete(route('{route}', id)){e}"));
                    out.push(1, "}");
                    out.push(0, format!("}}{e}"));
                }
            }
            Kind::Show => self.props_decl(out, false),
        }
        out.push(0, "</script>");
    }

    /// `const form = useForm({ … })`; `prop`: az Edit-nél a prop neve, ahonnan az értékek jönnek.
    fn form_decl(&self, out: &mut Out, prop: Option<&str>) {
        out.push(0, "const form = useForm({");
        for field in self.fields {
            let value = match prop {
                Some(var) => format!("props.{var}.{}", field.name),
                None => default_value(field, self.facts.ts),
            };
            out.push(1, format!("{}: {value},", field.name));
        }
        out.push(0, format!("}}){}", self.semi()));
    }

    fn submit_fn(&self, out: &mut Out, verb: &str, route: &str, id: Option<&str>) {
        let e = self.semi();
        out.push(0, "const submit = () => {");
        match id {
            Some(id) => out.push(1, format!("form.{verb}(route('{route}', {id})){e}")),
            None => out.push(1, format!("form.{verb}(route('{route}')){e}")),
        }
        out.push(0, format!("}}{e}"));
    }

    /// A TS-típus mezői (`id` + a mezők), a megadott szinttől.
    fn ts_members(&self, out: &mut Out, level: usize) {
        let sep = if self.facts.semi { ";" } else { "" };
        out.push(level, format!("id: number{sep}"));
        for field in self.fields {
            out.push(level, format!("{}: {}{sep}", field.name, ts_type(field)));
        }
    }

    /// A `defineProps`: Edit / Show: az `item` objektum (`const props =` csak az Edit-nél, ott a script használja);
    /// Index: a lapozott lista.
    fn props_decl(&self, out: &mut Out, bind: bool) {
        let e = self.semi();
        let sep = if self.facts.semi { ";" } else { "" };
        let prefix = if bind { "const props = " } else { "" };
        let single = matches!(self.kind, Kind::Edit | Kind::Show);
        let key = if single {
            &self.names.var
        } else {
            &self.names.vars
        };
        if self.facts.ts {
            out.push(0, format!("{prefix}defineProps<{{"));
            out.push(1, format!("{key}: {{"));
            if single {
                self.ts_members(out, 2);
            } else {
                out.push(2, "data: {");
                self.ts_members(out, 3);
                out.push(2, format!("}}[]{sep}"));
            }
            out.push(1, format!("}}{sep}"));
            out.push(0, format!("}}>(){e}"));
        } else {
            out.push(0, format!("{prefix}defineProps({{"));
            out.push(1, format!("{key}: {{"));
            out.push(2, "type: Object,");
            out.push(2, "required: true,");
            out.push(1, "},");
            out.push(0, format!("}}){e}"));
        }
    }

    // ---- template ---------------------------------------------------------------------------------------------

    fn label_lines(&self, out: &mut Out, level: usize, field: &Field) {
        let (name, text) = (&field.name, humanize(&field.name));
        if self.parts.label.is_some() {
            out.push(
                level,
                format!("<InputLabel for=\"{name}\" value=\"{text}\" />"),
            );
        } else {
            out.push(
                level,
                format!(
                    "<label for=\"{name}\" class=\"block text-sm font-medium text-gray-700\">{text}</label>"
                ),
            );
        }
    }

    fn error_lines(&self, out: &mut Out, level: usize, field: &Field) {
        let name = &field.name;
        if self.parts.error.is_some() {
            out.push(
                level,
                format!("<InputError class=\"mt-2\" :message=\"form.errors.{name}\" />"),
            );
        } else {
            out.push(
                level,
                format!(
                    "<p v-if=\"form.errors.{name}\" class=\"mt-2 text-sm text-red-600\">{{{{ form.errors.{name} }}}}</p>"
                ),
            );
        }
    }

    /// Egy mező blokkja: címke, beviteli elem, hibaüzenet.
    fn field_block(&self, out: &mut Out, level: usize, field: &Field) {
        let name = &field.name;
        let required = if field.optional || field.ty == FieldType::Bool {
            ""
        } else {
            " required"
        };
        let model = format!("v-model=\"form.{name}\"");
        out.push(level, "<div>");
        let inner = level + 1;
        match field.ty {
            FieldType::Bool => {
                out.push(
                    inner,
                    "<label class=\"flex items-center gap-2 text-sm text-gray-700\">",
                );
                out.push(
                    inner + 1,
                    format!(
                        "<input id=\"{name}\" type=\"checkbox\" class=\"rounded border-gray-300 text-indigo-600 shadow-sm focus:ring-indigo-500\" {model} />"
                    ),
                );
                out.push(inner + 1, format!("<span>{}</span>", humanize(name)));
                out.push(inner, "</label>");
            }
            FieldType::Text => {
                self.label_lines(out, inner, field);
                out.push(
                    inner,
                    format!(
                        "<textarea id=\"{name}\" rows=\"4\" class=\"{INPUT_CLASS}\" {model}{required}></textarea>"
                    ),
                );
            }
            FieldType::Int | FieldType::Float => {
                self.label_lines(out, inner, field);
                let step = if field.ty == FieldType::Float {
                    " step=\"any\""
                } else {
                    ""
                };
                out.push(
                    inner,
                    format!(
                        "<input id=\"{name}\" type=\"number\"{step} class=\"{INPUT_CLASS}\" {model}{required} />"
                    ),
                );
            }
            FieldType::Str | FieldType::Email | FieldType::Date | FieldType::DateTime => {
                let kind = match field.ty {
                    FieldType::Email => "email",
                    FieldType::Date => "date",
                    FieldType::DateTime => "datetime-local",
                    _ => "text",
                };
                self.label_lines(out, inner, field);
                if self.parts.input.is_some() {
                    out.push(
                        inner,
                        format!(
                            "<TextInput id=\"{name}\" type=\"{kind}\" class=\"mt-1 block w-full\" {model}{required} />"
                        ),
                    );
                } else {
                    out.push(
                        inner,
                        format!(
                            "<input id=\"{name}\" type=\"{kind}\" class=\"{INPUT_CLASS}\" {model}{required} />"
                        ),
                    );
                }
            }
        }
        self.error_lines(out, inner, field);
        out.push(level, "</div>");
    }

    fn form_body(&self, out: &mut Out, level: usize) {
        out.push(
            level,
            "<form @submit.prevent=\"submit\" class=\"space-y-6 bg-white p-6 shadow sm:rounded-lg\">",
        );
        for field in self.fields {
            self.field_block(out, level + 1, field);
        }
        out.push(level + 1, "<div class=\"flex items-center gap-4\">");
        if self.parts.button.is_some() {
            out.push(
                level + 2,
                "<PrimaryButton :disabled=\"form.processing\">Save</PrimaryButton>",
            );
        } else {
            out.push(
                level + 2,
                "<button type=\"submit\" :disabled=\"form.processing\" class=\"rounded-md bg-gray-800 px-4 py-2 text-xs font-semibold uppercase tracking-widest text-white disabled:opacity-25\">Save</button>",
            );
        }
        out.push(level + 1, "</div>");
        out.push(level, "</form>");
    }

    /// A `{{ item.x }}` cella-kifejezés.
    fn cell(field: &Field) -> String {
        if field.ty == FieldType::Bool {
            format!("{{{{ item.{} ? 'Yes' : 'No' }}}}", field.name)
        } else {
            format!("{{{{ item.{} }}}}", field.name)
        }
    }

    fn index_body(&self, out: &mut Out, level: usize) {
        let vars = &self.names.vars;
        if let Some(route) = self.known(Op::Create) {
            out.push(level, "<div class=\"mb-4 flex justify-end\">");
            out.push(
                level + 1,
                format!(
                    "<Link :href=\"route('{route}')\" class=\"rounded-md bg-gray-800 px-4 py-2 text-xs font-semibold uppercase tracking-widest text-white\">New {}</Link>",
                    humanize_model(&self.names.model)
                ),
            );
            out.push(level, "</div>");
        }
        let (edit, show, destroy) = (
            self.known(Op::Edit),
            self.known(Op::Show),
            self.known(Op::Destroy),
        );
        let actions = edit.is_some() || show.is_some() || destroy.is_some();
        let th = "class=\"px-4 py-3 text-left text-xs font-medium uppercase text-gray-500\"";
        out.push(
            level,
            "<div class=\"overflow-hidden bg-white shadow sm:rounded-lg\">",
        );
        out.push(
            level + 1,
            "<table class=\"min-w-full divide-y divide-gray-200\">",
        );
        out.push(level + 2, "<thead class=\"bg-gray-50\">");
        out.push(level + 3, "<tr>");
        out.push(level + 4, format!("<th {th}>ID</th>"));
        for field in self.fields {
            out.push(
                level + 4,
                format!("<th {th}>{}</th>", humanize(&field.name)),
            );
        }
        if actions {
            out.push(level + 4, "<th class=\"px-4 py-3\"></th>");
        }
        out.push(level + 3, "</tr>");
        out.push(level + 2, "</thead>");
        out.push(level + 2, "<tbody class=\"divide-y divide-gray-200\">");
        out.push(
            level + 3,
            format!("<tr v-for=\"item in {vars}.data\" :key=\"item.id\">"),
        );
        let td = "class=\"px-4 py-3 text-sm text-gray-900\"";
        out.push(level + 4, format!("<td {td}>{{{{ item.id }}}}</td>"));
        for field in self.fields {
            out.push(level + 4, format!("<td {td}>{}</td>", Self::cell(field)));
        }
        if actions {
            out.push(level + 4, "<td class=\"px-4 py-3 text-right text-sm\">");
            let link = "class=\"ml-3 text-indigo-600 hover:text-indigo-900\"";
            if let Some(route) = show {
                out.push(
                    level + 5,
                    format!("<Link :href=\"route('{route}', item.id)\" {link}>Show</Link>"),
                );
            }
            if let Some(route) = edit {
                out.push(
                    level + 5,
                    format!("<Link :href=\"route('{route}', item.id)\" {link}>Edit</Link>"),
                );
            }
            if destroy.is_some() {
                out.push(
                    level + 5,
                    "<button type=\"button\" class=\"ml-3 text-red-600 hover:text-red-900\" @click=\"destroy(item.id)\">Delete</button>",
                );
            }
            out.push(level + 4, "</td>");
        }
        out.push(level + 3, "</tr>");
        out.push(level + 2, "</tbody>");
        out.push(level + 1, "</table>");
        out.push(level, "</div>");
    }

    fn show_body(&self, out: &mut Out, level: usize) {
        let var = &self.names.var;
        out.push(
            level,
            "<dl class=\"divide-y divide-gray-200 bg-white px-6 shadow sm:rounded-lg\">",
        );
        for field in self.fields {
            out.push(level + 1, "<div class=\"py-4\">");
            out.push(
                level + 2,
                format!(
                    "<dt class=\"text-sm font-medium text-gray-500\">{}</dt>",
                    humanize(&field.name)
                ),
            );
            let value = if field.ty == FieldType::Bool {
                format!("{{{{ {var}.{} ? 'Yes' : 'No' }}}}", field.name)
            } else {
                format!("{{{{ {var}.{} }}}}", field.name)
            };
            out.push(
                level + 2,
                format!("<dd class=\"mt-1 text-sm text-gray-900\">{value}</dd>"),
            );
            out.push(level + 1, "</div>");
        }
        out.push(level, "</dl>");
    }

    fn template(&self, out: &mut Out) {
        let title = self.title();
        out.push(0, "<template>");
        out.push(1, format!("<Head title=\"{title}\" />"));
        out.push(0, "");
        let mut level = 1;
        let mut header_slot = false;
        if let Some((name, _)) = &self.parts.layout {
            out.push(level, format!("<{name}>"));
            level += 1;
            if name == "AuthenticatedLayout" {
                header_slot = true;
                out.push(level, "<template #header>");
                out.push(
                    level + 1,
                    "<h2 class=\"text-xl font-semibold leading-tight text-gray-800\">",
                );
                out.push(level + 2, &title);
                out.push(level + 1, "</h2>");
                out.push(level, "</template>");
                out.push(0, "");
            }
        }
        out.push(level, "<div class=\"py-12\">");
        out.push(
            level + 1,
            "<div class=\"mx-auto max-w-7xl sm:px-6 lg:px-8\">",
        );
        let body = level + 2;
        if !header_slot {
            out.push(
                body,
                format!("<h1 class=\"mb-4 text-xl font-semibold text-gray-800\">{title}</h1>"),
            );
        }
        match self.kind {
            Kind::Create | Kind::Edit => self.form_body(out, body),
            Kind::Index => self.index_body(out, body),
            Kind::Show => self.show_body(out, body),
        }
        out.push(level + 1, "</div>");
        out.push(level, "</div>");
        if let Some((name, _)) = &self.parts.layout {
            out.push(1, format!("</{name}>"));
        }
        out.push(0, "</template>");
    }

    /// A teljes `.vue` fájl, záró újsorral.
    pub fn render(&self) -> String {
        let mut out = Out {
            unit: self.facts.unit,
            lines: Vec::new(),
        };
        self.script(&mut out);
        out.push(0, "");
        self.template(&mut out);
        let mut text = out.lines.join("\n");
        text.push('\n');
        text
    }
}
