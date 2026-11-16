//! A metódushívás fogadójának típusa (TS): a típusos paraméter / változó / osztály-property, a `new Típus()`
//! értékadás és az importált vagy a fájlban deklarált típusnév. A `x.m()` / `this.x.m()` így `Típus::m`-re
//! oldódik — de csak akkor, ha a név minden kötése a függvényben (az osztályban) ugyanazt a típust adja.
//! Generikus paraméter, `any` / `unknown`, unió és a nem ismert (se nem importált, se nem a fájlban deklarált)
//! típus feloldatlan marad; JS-ben nincs `?::` jelölés (a tömb- / DOM-metódusok miatt túl zajos lenne).

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BindingIdentifier, BindingPattern, Class, ClassElement, Declaration, Expression,
    FormalParameters, Function, MethodDefinitionKind, Program, Statement, TSType, TSTypeAnnotation,
    TSTypeName, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use super::key_name;
use crate::lang::js::imports::ImportBinding;

/// Egy feloldott fogadó-típus: a `resolved` típusneve és a hívás kategóriája.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub name: String,
    pub category: &'static str,
    /// A nem helyi import forrása, ha a típus csomagból jön (`Call::from`).
    pub from: Option<String>,
}

/// A fájlban ismert típusnevek: a top-level osztályok / interfészek és az importált nevek.
#[derive(Debug, Default)]
pub struct TypeNames {
    local: HashSet<String>,
    /// Helyi név → az importált (exportált) név és az import kategóriája.
    imported: HashMap<String, TypeRef>,
    /// `import * as ns` → az import kategóriája és nem helyi forrása (`ns.Repo`).
    namespaces: HashMap<String, (&'static str, Option<String>)>,
}

impl TypeNames {
    pub fn new(program: &Program<'_>, imports: &[ImportBinding]) -> Self {
        let mut names = Self::default();
        for b in imports {
            if b.import_value.ends_with("::*") {
                names
                    .namespaces
                    .insert(b.local.clone(), (b.category, b.package.clone()));
                continue;
            }
            // `<forrás>::<importált név>` (default importnál a helyi név)
            let name = b.resolved.rsplit("::").next().unwrap_or(b.local.as_str());
            names.imported.insert(
                b.local.clone(),
                TypeRef {
                    name: name.to_string(),
                    category: b.category,
                    from: b.package.clone(),
                },
            );
        }
        for stmt in &program.body {
            let decl = match stmt {
                Statement::ExportDeclaration(e) => Some(&e.declaration),
                Statement::ExportDefaultDeclaration(e) => {
                    if let oxc_ast::ast::ExportDefaultDeclarationKind::ClassDeclaration(c) =
                        &e.declaration
                        && let Some(id) = &c.id
                    {
                        names.local.insert(id.name.to_string());
                    }
                    None
                }
                other => other.as_declaration(),
            };
            match decl {
                Some(Declaration::ClassDeclaration(c)) => {
                    if let Some(id) = &c.id {
                        names.local.insert(id.name.to_string());
                    }
                }
                Some(Declaration::TSInterfaceDeclaration(i)) => {
                    names.local.insert(i.id.name.to_string());
                }
                _ => {}
            }
        }
        names
    }

    fn lookup(&self, name: &str) -> Option<TypeRef> {
        if self.local.contains(name) {
            return Some(TypeRef {
                name: name.to_string(),
                category: "internal_helper",
                from: None,
            });
        }
        self.imported.get(name).cloned()
    }

    /// `ns.Repo` (névtér-import) → `Repo`.
    fn qualified(&self, ns: &str, name: &str) -> Option<TypeRef> {
        self.namespaces.get(ns).map(|(category, from)| TypeRef {
            name: name.to_string(),
            category,
            from: from.clone(),
        })
    }

    /// A típus-annotáció típusa: csak egy ismert név (`Repo`, `Repo<User>`, `ns.Repo`).
    pub fn of_annotation(&self, a: &TSTypeAnnotation<'_>) -> Option<TypeRef> {
        let TSType::TSTypeReference(r) = &a.type_annotation else {
            return None;
        };
        match &r.type_name {
            TSTypeName::IdentifierReference(i) => self.lookup(i.name.as_str()),
            TSTypeName::QualifiedName(q) => match &q.left {
                TSTypeName::IdentifierReference(ns) => {
                    self.qualified(ns.name.as_str(), q.right.name.as_str())
                }
                _ => None,
            },
            TSTypeName::ThisExpression(_) => None,
        }
    }

    /// `new Repo(…)` / `new ns.Repo(…)` típusa.
    pub fn of_new(&self, e: &Expression<'_>) -> Option<TypeRef> {
        let Expression::NewExpression(n) = e.without_parentheses() else {
            return None;
        };
        match n.callee.without_parentheses() {
            Expression::Identifier(i) => self.lookup(i.name.as_str()),
            Expression::StaticMemberExpression(m) => match m.object.without_parentheses() {
                Expression::Identifier(ns) => {
                    self.qualified(ns.name.as_str(), m.property.name.as_str())
                }
                _ => None,
            },
            _ => None,
        }
    }
}

/// Egy név típusa a kötések egyesítésével: eltérő (vagy ismeretlen) kötés → `None`.
fn merge(map: &mut HashMap<String, Option<TypeRef>>, name: &str, ty: Option<TypeRef>) {
    match map.get_mut(name) {
        Some(current) => {
            if *current != ty {
                *current = None;
            }
        }
        None => {
            map.insert(name.to_string(), ty);
        }
    }
}

/// A végső típus: az annotáció dönt (a TS kikényszeríti), különben a `new` értékadások egyezése. A `declared`
/// (ha van) a keret saját nevei: a kívülről látott névre tett értékadás itt nem számít, a típus nélkül
/// deklarált név pedig `None`-nal árnyékolja a külső keret azonos nevét.
fn settle(
    annotated: HashMap<String, Option<TypeRef>>,
    inferred: HashMap<String, Option<TypeRef>>,
    declared: Option<&HashSet<String>>,
) -> HashMap<String, Option<TypeRef>> {
    let mut out = annotated;
    for (name, ty) in inferred {
        if declared.is_none_or(|d| d.contains(&name)) {
            out.entry(name).or_insert(ty);
        }
    }
    for name in declared.into_iter().flatten() {
        out.entry(name.clone()).or_insert(None);
    }
    out
}

/// Egy függvény (vagy a program) helyi neveinek típusa; a beágyazott függvényekbe / osztályokba nem lép.
pub type Frame = HashMap<String, Option<TypeRef>>;

struct FrameScan<'t> {
    types: &'t TypeNames,
    annotated: HashMap<String, Option<TypeRef>>,
    inferred: HashMap<String, Option<TypeRef>>,
    declared: HashSet<String>,
}

impl<'t> FrameScan<'t> {
    fn new(types: &'t TypeNames) -> Self {
        Self {
            types,
            annotated: HashMap::new(),
            inferred: HashMap::new(),
            declared: HashSet::new(),
        }
    }

    /// A paraméterek: a típusos egyszerű paraméter annotált, a `p = new X()` alapérték következtetett.
    fn params(&mut self, params: &FormalParameters<'_>) {
        self.visit_formal_parameters(params);
        for p in &params.items {
            let BindingPattern::BindingIdentifier(id) = &p.pattern else {
                continue;
            };
            if let Some(a) = &p.type_annotation {
                merge(&mut self.annotated, &id.name, self.types.of_annotation(a));
            } else if let Some(init) = &p.initializer {
                merge(&mut self.inferred, &id.name, self.types.of_new(init));
            }
        }
    }

    fn finish(self) -> Frame {
        settle(self.annotated, self.inferred, Some(&self.declared))
    }
}

impl<'a> Visit<'a> for FrameScan<'_> {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.declared.insert(it.name.to_string());
    }

    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        if let Some(id) = &it.id {
            self.declared.insert(id.name.to_string());
        }
    }

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id {
            self.declared.insert(id.name.to_string());
        }
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.id {
            if let Some(a) = &it.type_annotation {
                merge(&mut self.annotated, &id.name, self.types.of_annotation(a));
            } else if let Some(init) = &it.init {
                merge(&mut self.inferred, &id.name, self.types.of_new(init));
            }
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.left {
            let ty = (it.operator == AssignmentOperator::Assign)
                .then(|| self.types.of_new(&it.right))
                .flatten();
            merge(&mut self.inferred, &id.name, ty);
        }
        walk::walk_assignment_expression(self, it);
    }
}

/// A program top-level neveinek típusa.
pub fn program_frame(types: &TypeNames, program: &Program<'_>) -> Frame {
    let mut scan = FrameScan::new(types);
    for stmt in &program.body {
        // az importok kötései a `Scope`-ban vannak; itt nem árnyékolnak
        if !matches!(stmt, Statement::ImportDeclaration(_)) {
            scan.visit_statement(stmt);
        }
    }
    scan.finish()
}

/// Egy `function` / metódus neveinek típusa (paraméterek + a törzs saját deklarációi).
pub fn function_frame(types: &TypeNames, f: &Function<'_>) -> Frame {
    let mut scan = FrameScan::new(types);
    scan.params(&f.params);
    if let Some(body) = &f.body {
        scan.visit_function_body(body);
    }
    scan.finish()
}

/// Egy nyíl-függvény neveinek típusa.
pub fn arrow_frame(types: &TypeNames, f: &ArrowFunctionExpression<'_>) -> Frame {
    let mut scan = FrameScan::new(types);
    scan.params(&f.params);
    scan.visit_arrow_function_body(&f.body);
    scan.finish()
}

/// Egy osztály a hívás-feloldáshoz: a neve, a saját metódusai, a property-k típusa.
#[derive(Debug, Default)]
pub struct ClassFrame {
    pub name: Option<String>,
    /// A saját metódusnevek (`this.m()` → `Osztály::m`).
    pub methods: HashSet<String>,
    /// A metódusként definiált függvények kezdő offsetjei (bennük a `this` az osztály).
    pub method_fns: HashSet<u32>,
    pub props: HashMap<String, Option<TypeRef>>,
}

struct ClassScan<'t> {
    types: &'t TypeNames,
    annotated: HashMap<String, Option<TypeRef>>,
    inferred: HashMap<String, Option<TypeRef>>,
}

impl<'a> Visit<'a> for ClassScan<'_> {
    /// A beágyazott osztály `this`-e más.
    fn visit_class(&mut self, _it: &Class<'a>) {}

    /// `this.x = new X()` (más értékadás → a property típusa csak az annotációból jöhet).
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::StaticMemberExpression(m) = &it.left
            && matches!(
                m.object.without_parentheses(),
                Expression::ThisExpression(_)
            )
        {
            let ty = (it.operator == AssignmentOperator::Assign)
                .then(|| self.types.of_new(&it.right))
                .flatten();
            merge(&mut self.inferred, &m.property.name, ty);
        }
        walk::walk_assignment_expression(self, it);
    }
}

/// Az osztály property-típusai: a típusos deklaráció (`private repo: Repo`, `repo!: Repo`), a konstruktor
/// paraméter-property-je (`constructor(private repo: Repo)`), különben a `new` kezdőérték / értékadás.
pub fn class_frame(types: &TypeNames, class: &Class<'_>) -> ClassFrame {
    let mut frame = ClassFrame {
        name: class.id.as_ref().map(|id| id.name.to_string()),
        ..ClassFrame::default()
    };
    let mut scan = ClassScan {
        types,
        annotated: HashMap::new(),
        inferred: HashMap::new(),
    };
    for el in &class.body.body {
        match el {
            ClassElement::PropertyDefinition(p) => {
                let Some(key) = key_name(&p.key) else {
                    continue;
                };
                let value = p.value.as_ref().map(Expression::without_parentheses);
                if let Some(v) = value
                    && matches!(
                        v,
                        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
                    )
                {
                    frame.methods.insert(key.clone());
                    frame.method_fns.insert(v.span().start);
                }
                if let Some(a) = &p.type_annotation {
                    merge(&mut scan.annotated, &key, types.of_annotation(a));
                } else if let Some(v) = value {
                    merge(&mut scan.inferred, &key, types.of_new(v));
                }
            }
            ClassElement::MethodDefinition(m) => {
                frame.method_fns.insert(m.value.span.start);
                if m.kind == MethodDefinitionKind::Constructor {
                    for p in &m.value.params.items {
                        let BindingPattern::BindingIdentifier(id) = &p.pattern else {
                            continue;
                        };
                        if p.accessibility.is_some() || p.readonly {
                            let ty = p
                                .type_annotation
                                .as_ref()
                                .and_then(|a| types.of_annotation(a));
                            merge(&mut scan.annotated, &id.name, ty);
                        }
                    }
                } else if let Some(key) = key_name(&m.key) {
                    frame.methods.insert(key);
                }
            }
            _ => {}
        }
        scan.visit_class_element(el);
    }
    frame.props = settle(scan.annotated, scan.inferred, None);
    frame
}
