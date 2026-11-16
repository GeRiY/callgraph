//! Fájl- és típus-szint: importok, containerek (class / data class / sealed / enum / object / companion object /
//! interface), a fájl-szintű container és a tagok szétosztása.

use tree_sitter::Node;

use super::super::known::KNOWN_INTERFACES;
use super::super::parser::{end_line, line, named_children};
use super::scan::{ctor_params, import_path, is_property_param};
use super::util::{base_type, child_of, has_token, is_comment, name_and_type, name_node};
use super::{KotlinVisitor, Scope};
use crate::model::{Container, FileGraphBuilder, Method, Property};

impl KotlinVisitor<'_, '_> {
    /// A fájl-szintű függvények / változók / utasítások containere (`<fájlnév>`, `type: module`).
    pub(super) fn module_container(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let name = self.file_name.clone();
        let end = end_line(root).max(1);
        b.container(Container {
            kind: "module".to_string(),
            name: name.clone(),
            line: 1,
            end_line: end,
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(Vec::new()),
            implements: Some(Vec::new()),
            traits: Some(Vec::new()),
            doc: None,
        });
        self.containers.push((name.clone(), 1, end));
        self.module = Some(name);
    }

    /// A gyökér közvetlen gyerekei: import, típus, fájl-szintű tag vagy utasítás.
    pub(super) fn visit_top_level(&mut self, root: Node<'_>, b: &mut FileGraphBuilder) {
        let mut statements: Vec<Node<'_>> = Vec::new();
        self.top_level_nodes(root, &mut statements, b);
        self.top_level_statements(&statements, b);
    }

    fn top_level_nodes<'t>(
        &mut self,
        parent: Node<'t>,
        statements: &mut Vec<Node<'t>>,
        b: &mut FileGraphBuilder,
    ) {
        for node in named_children(parent) {
            match node.kind() {
                "import" | "import_header" => {
                    let full = import_path(self.text(node));
                    if !full.ends_with('*') && !full.is_empty() {
                        let alias = self.import_alias(node, &full);
                        b.import(alias, full);
                    }
                }
                "import_list" | "ERROR" => self.top_level_nodes(node, statements, b),
                "package_header" | "file_annotation" | "annotation" | "shebang_line"
                | "type_alias" => {}
                "class_declaration" | "object_declaration" => self.type_decl(node, None, b),
                _ if is_comment(node) => {}
                _ => {
                    let container = self.module.clone();
                    if !self.member(node, container.as_deref(), false, b) {
                        statements.push(node);
                    }
                }
            }
        }
    }

    /// `import a.b.C` → `C`; `import a.b.C as D` → `D`.
    fn import_alias(&self, node: Node<'_>, full: &str) -> String {
        let text = self.text(node);
        if let Some((_, alias)) = text.split_once(" as ") {
            return alias.trim().trim_end_matches(';').trim().to_string();
        }
        full.rsplit('.').next().unwrap_or(full).to_string()
    }

    /// Egy típus-deklaráció containerként, majd a tagjai (beágyazott típusnál `Külső.Belső`).
    pub(super) fn type_decl(
        &mut self,
        node: Node<'_>,
        outer: Option<&str>,
        b: &mut FileGraphBuilder,
    ) {
        let companion = node.kind() == "companion_object";
        let base = match name_node(node) {
            Some(n) => self.text(n).to_string(),
            None if companion => "Companion".to_string(),
            None => return,
        };
        let qualified = match outer {
            Some(o) => format!("{o}.{base}"),
            None => base.clone(),
        };
        let words = self.modifier_words(node);
        let has = |w: &str| words.iter().any(|x| x == w);
        let interface = has_token(node, "interface");
        let kind = match node.kind() {
            "object_declaration" => "object",
            "companion_object" => "companion object",
            _ if interface && has("sealed") => "sealed interface",
            _ if interface => "interface",
            _ if has("enum") => "enum",
            _ if has("data") => "data class",
            _ if has("sealed") => "sealed class",
            _ if has("annotation") => "annotation class",
            _ if has("value") => "value class",
            _ => "class",
        };
        let (extends, implements) = self.split_supertypes(node, interface);
        let is_abstract = interface || has("abstract") || has("sealed");
        let is_final = !interface
            && node.kind() == "class_declaration"
            && !has("open")
            && !is_abstract
            && !has("enum");
        b.container(Container {
            kind: kind.to_string(),
            name: qualified.clone(),
            line: line(node),
            end_line: end_line(node),
            is_abstract: Some(is_abstract),
            is_final: Some(is_final),
            extends: Some(extends),
            implements: Some(implements),
            traits: Some(Vec::new()),
            doc: self.doc_of(node),
        });
        self.containers
            .push((qualified.clone(), line(node), end_line(node)));
        self.annotation_calls(node, Some(&qualified), "__top__", b);
        self.primary_constructor(node, &qualified, b);
        let body = child_of(node, "class_body").or_else(|| child_of(node, "enum_class_body"));
        if let Some(body) = body {
            self.type_members(body, &qualified, interface, b);
        }
    }

    /// A primary constructor: `val` / `var` paraméterei property-k, maga a konstruktor `init` metódus
    /// (`@Inject constructor(..)` annotációjával együtt).
    fn primary_constructor(&mut self, node: Node<'_>, container: &str, b: &mut FileGraphBuilder) {
        let Some(ctor) = child_of(node, "primary_constructor") else {
            return;
        };
        let params = ctor_params(node);
        for param in &params {
            if !is_property_param(self.parsed, *param) {
                continue;
            }
            let (Some(name), declared) = name_and_type(self.parsed, *param) else {
                continue;
            };
            let mut parts = self.annotations(*param);
            parts.extend(declared);
            let words = self.modifier_words(*param);
            let readonly = super::util::binding_kind(self.parsed, *param) == Some("val");
            b.property(Property {
                container: Some(container.to_string()),
                name,
                line: line(*param),
                type_hint: (!parts.is_empty()).then(|| parts.join(" ")),
                visibility: words
                    .iter()
                    .find(|w| matches!(w.as_str(), "private" | "protected" | "internal" | "public"))
                    .cloned()
                    .unwrap_or_else(|| "public".to_string()),
                is_static: false,
                readonly,
                doc: None,
                default: None,
                js: false,
            });
        }
        if params.is_empty() && self.annotations(ctor).is_empty() {
            return;
        }
        let container_opt = Some(container.to_string());
        let name = "constructor".to_string();
        b.method(Method {
            container: container_opt.clone(),
            name: name.clone(),
            line: line(ctor),
            end_line: Some(end_line(ctor)),
            visibility: Some(self.visibility(ctor)),
            is_static: false,
            is_abstract: false,
            kind: Some("init".to_string()),
            params: self.params(node),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        });
        self.add_method_span(&container_opt, &name, line(ctor), end_line(ctor));
        self.annotation_calls(ctor, Some(container), &name, b);
        let scope = Scope {
            container: container_opt,
            caller: name,
            in_compose: false,
        };
        for param in params {
            if let Some(value) = super::util::value_of(param) {
                self.walk_body(value, &scope, b);
            }
        }
    }

    /// A típus-törzs tagjai; a hibás (`ERROR`) és csomagoló részek belsejében is keres deklarációt.
    fn type_members(
        &mut self,
        body: Node<'_>,
        qualified: &str,
        interface: bool,
        b: &mut FileGraphBuilder,
    ) {
        for member in named_children(body) {
            match member.kind() {
                "ERROR" | "class_member_declarations" | "enum_entries" => {
                    self.type_members(member, qualified, interface, b)
                }
                "class_declaration" | "object_declaration" | "companion_object" => {
                    self.type_decl(member, Some(qualified), b)
                }
                "enum_entry" => self.enum_entry(member, qualified, b),
                _ if is_comment(member) => {}
                _ => {
                    self.member(member, Some(qualified), interface, b);
                }
            }
        }
    }

    /// `extends` / `implements`: a konstruktor-hívásos (`: Base()`) szupertípus az ős, a többi interfész;
    /// interfésznél minden szupertípus `extends`.
    fn split_supertypes(&self, node: Node<'_>, interface: bool) -> (Vec<String>, Vec<String>) {
        let mut specs = Vec::new();
        for child in named_children(node) {
            match child.kind() {
                "delegation_specifiers" => specs.extend(
                    named_children(child)
                        .into_iter()
                        .filter(|c| !is_comment(*c)),
                ),
                "delegation_specifier" => specs.push(child),
                _ => {}
            }
        }
        let (mut extends, mut implements) = (Vec::new(), Vec::new());
        for spec in specs {
            let text = self.text(spec);
            let head = text.split(" by ").next().unwrap_or(text);
            let invoked = head.contains('(');
            let name = head
                .split('(')
                .next()
                .unwrap_or(head)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("");
            if name.is_empty() {
                continue;
            }
            let known_interface = KNOWN_INTERFACES.contains(&name.as_str())
                || base_type(&name).is_some_and(|t| KNOWN_INTERFACES.contains(&t.as_str()));
            if interface || (invoked && !known_interface) {
                extends.push(name);
            } else {
                implements.push(name);
            }
        }
        (extends, implements)
    }

    /// Fájl-szintű utasítások (`.kts` szkript): `__module__` pszeudo-metódus a hívásaikkal.
    fn top_level_statements(&mut self, statements: &[Node<'_>], b: &mut FileGraphBuilder) {
        let (Some(first), Some(last)) = (statements.first(), statements.last()) else {
            return;
        };
        let scope = Scope {
            container: self.module.clone(),
            caller: "__module__".to_string(),
            in_compose: false,
        };
        let (start, end) = (line(*first), end_line(*last));
        self.add_method_span(&scope.container, "__module__", start, end);
        b.method(Method {
            container: scope.container.clone(),
            name: "__module__".to_string(),
            line: start,
            end_line: Some(end),
            visibility: Some("public".to_string()),
            is_static: false,
            is_abstract: false,
            kind: Some("top-level".to_string()),
            params: Vec::new(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        });
        self.locals.clear();
        for node in statements {
            self.walk_body(*node, &scope, b);
        }
    }
}
