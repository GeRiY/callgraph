//! react-router in JSX: `<Route path element>` trees (nested, `index`) and the literal client links
//! (`<Link to>`, `<NavLink to>`, `<Navigate to>`, `navigate('/x')` of `const navigate = useNavigate()`). The route
//! tables written as arrays (`createBrowserRouter([...])`, `useRoutes([...])`) are read by the text scanner of the
//! parent module.

use std::collections::HashSet;

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};

use super::{Ctx, component_text, join_child};
use crate::lang::js::{parse_program, source_type_for};
use crate::packages::webroutes::{ClientRoute, FileFacts, PathUse};

/// The route tree and the links of one file.
pub(super) fn scan(raw: &str, ctx: &Ctx<'_>, facts: &mut FileFacts) {
    let wanted = ["<Route", "<Link", "<NavLink", "<Navigate", "useNavigate"];
    if !wanted.iter().any(|needle| raw.contains(needle)) {
        return;
    }
    let allocator = oxc_allocator::Allocator::default();
    let parsed = parse_program(
        &allocator,
        raw,
        source_type_for(std::path::Path::new(ctx.rel), None),
    );
    let mut visitor = Scan {
        ctx,
        facts,
        parents: Vec::new(),
        navigate: HashSet::new(),
    };
    visitor.visit_program(&parsed.program);
}

struct Scan<'c, 'f> {
    ctx: &'c Ctx<'c>,
    facts: &'f mut FileFacts,
    /// The joined paths of the enclosing `<Route>`s.
    parents: Vec<String>,
    /// Variables bound to `useNavigate()`.
    navigate: HashSet<String>,
}

impl Scan<'_, '_> {
    fn line(&self, offset: u32) -> u32 {
        self.ctx.lines.line_of(offset as usize)
    }

    /// A `<Route>`: its path joined with the parents', then the children under it.
    fn route(&mut self, it: &JSXElement<'_>) {
        let open = &it.opening_element;
        let mut path: Option<String> = None;
        let mut index = false;
        let mut target: Option<String> = None;
        for item in &open.attributes {
            let JSXAttributeItem::Attribute(attr) = item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "path" => {
                    path = attr_string(attr);
                    if path.is_none() {
                        self.ctx.incomplete.set(true);
                    }
                }
                "index" => index = true,
                "element" => target = self.element_target(attr),
                "Component" | "component" => target = self.component_target(attr),
                _ => {}
            }
        }
        let parent = self.parents.last().cloned().unwrap_or_default();
        let full = if index {
            parent
        } else {
            join_child(&parent, path.as_deref().unwrap_or(""))
        };
        let shown = if full.is_empty() {
            "/".to_owned()
        } else {
            full.clone()
        };
        if path.is_some() || index {
            self.ctx.paths.borrow_mut().push(shown.clone());
        }
        if target.is_some() {
            self.facts.client.push(ClientRoute {
                file: self.ctx.rel.to_owned(),
                line: self.line(open.span.start),
                path: shown,
                name: None,
                target,
            });
        }
        self.parents.push(full);
        walk::walk_jsx_element(self, it);
        self.parents.pop();
    }

    /// `element={<Home />}`: the component file (or the tag as written).
    fn element_target(&self, attr: &JSXAttribute<'_>) -> Option<String> {
        let Some(JSXAttributeValue::ExpressionContainer(c)) = &attr.value else {
            return None;
        };
        let Some(Expression::JSXElement(el)) = c.expression.as_expression() else {
            return None;
        };
        let JSXElementName::IdentifierReference(id) = &el.opening_element.name else {
            return None;
        };
        component_text(id.name.as_str(), self.ctx)
    }

    /// `Component={Home}`.
    fn component_target(&self, attr: &JSXAttribute<'_>) -> Option<String> {
        let Some(JSXAttributeValue::ExpressionContainer(c)) = &attr.value else {
            return None;
        };
        let Some(Expression::Identifier(id)) = c.expression.as_expression() else {
            return None;
        };
        component_text(id.name.as_str(), self.ctx)
    }

    /// A literal absolute client path (`/x`); a query string / hash is cut off.
    fn link(&mut self, offset: u32, path: String) {
        if let Some(path) = client_path(&path) {
            self.facts.links.push(PathUse {
                file: self.ctx.rel.to_owned(),
                line: self.line(offset),
                path,
            });
        }
    }
}

/// `/x?a=1#h` → `/x`; relative, external and empty paths: `None`.
fn client_path(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = if path.len() > 1 {
        path.trim_end_matches('/')
    } else {
        path
    };
    (!path.is_empty()).then(|| path.to_owned())
}

/// The value of a JSX attribute when it is a plain string: `path="/x"`, `path={'/x'}`.
fn attr_string(attr: &JSXAttribute<'_>) -> Option<String> {
    match attr.value.as_ref()? {
        JSXAttributeValue::StringLiteral(s) => Some(s.value.display().to_string()),
        JSXAttributeValue::ExpressionContainer(c) => expr_string(c.expression.as_expression()?),
        _ => None,
    }
}

fn expr_string(e: &Expression<'_>) -> Option<String> {
    match e.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.display().to_string()),
        Expression::TemplateLiteral(t) if t.is_no_substitution_template() => {
            t.quasis.first().map(|q| {
                q.value
                    .cooked
                    .as_ref()
                    .map_or_else(|| q.value.raw.to_string(), |c| c.display().to_string())
            })
        }
        _ => None,
    }
}

impl<'a> Visit<'a> for Scan<'_, '_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        let open = &it.opening_element;
        if let JSXElementName::IdentifierReference(id) = &open.name {
            match id.name.as_str() {
                "Route" => {
                    self.route(it);
                    return;
                }
                "Link" | "NavLink" | "Navigate" => {
                    for item in &open.attributes {
                        if let JSXAttributeItem::Attribute(attr) = item
                            && let JSXAttributeName::Identifier(name) = &attr.name
                            && name.name.as_str() == "to"
                            && let Some(path) = attr_string(attr)
                        {
                            self.link(open.span.start, path);
                        }
                    }
                }
                _ => {}
            }
        }
        walk::walk_jsx_element(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init)
            && let Expression::CallExpression(call) = init.without_parentheses()
            && matches!(&call.callee, Expression::Identifier(c) if c.name == "useNavigate")
        {
            self.navigate.insert(id.name.to_string());
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(callee) = &it.callee
            && self.navigate.contains(callee.name.as_str())
            && let Some(first) = it.arguments.first().and_then(Argument::as_expression)
            && let Some(path) = expr_string(first)
        {
            self.link(it.span.start, path);
        }
        walk::walk_call_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::super::scan_file;
    use std::collections::HashSet;

    fn known(files: &[&str]) -> HashSet<String> {
        files.iter().map(|f| (*f).to_owned()).collect()
    }

    #[test]
    fn nested_jsx_routes_join_paths_and_resolve_elements() {
        let src = "import { Routes, Route } from 'react-router-dom';\nimport Home from './pages/Home';\nimport { Show } from './pages/Show';\nexport const App = () => (\n  <Routes>\n    <Route path=\"/\" element={<Home />} />\n    <Route path=\"/events\">\n      <Route index element={<Home />} />\n      <Route path=\":id\" element={<Show />} />\n    </Route>\n  </Routes>\n);\n";
        let files = known(&["src/pages/Home.tsx", "src/pages/Show.tsx"]);
        let facts = scan_file("src/App.tsx", src, &files);
        let shown: Vec<String> = facts
            .client
            .iter()
            .map(|r| format!("{} {}", r.path, r.target.as_deref().unwrap_or("-")))
            .collect();
        assert_eq!(
            shown,
            [
                "/ src/pages/Home.tsx",
                "/events src/pages/Home.tsx",
                "/events/:id src/pages/Show.tsx"
            ]
        );
        assert!(
            facts.paths.contains(&"/events".to_string()),
            "{:?}",
            facts.paths
        );
        assert!(!facts.incomplete);
    }

    #[test]
    fn router_arrays_use_element_component_and_lazy() {
        let src = "import { createBrowserRouter } from 'react-router-dom';\nimport Home from './Home';\nexport const router = createBrowserRouter([\n  { path: '/', element: <Home />, children: [\n    { path: 'about', Component: About },\n    { path: 'docs', lazy: () => import('./Docs') },\n  ] },\n]);\n";
        let files = known(&["src/Home.tsx", "src/Docs.tsx"]);
        let facts = scan_file("src/router.tsx", src, &files);
        let shown: Vec<String> = facts
            .client
            .iter()
            .map(|r| format!("{} {}", r.path, r.target.as_deref().unwrap_or("-")))
            .collect();
        assert_eq!(
            shown,
            ["/ src/Home.tsx", "/about About", "/docs src/Docs.tsx"]
        );
    }

    #[test]
    fn literal_links_and_navigations_are_collected() {
        let src = "import { Link, useNavigate } from 'react-router-dom';\nexport function Nav() {\n  const go = useNavigate();\n  go('/events/1?x=1');\n  go(-1);\n  go(`/b`);\n  return <nav><Link to=\"/a\">a</Link><Link to={path}>x</Link><Link to=\"rel\">r</Link></nav>;\n}\n";
        let facts = scan_file("src/Nav.tsx", src, &known(&[]));
        let paths: Vec<&str> = facts.links.iter().map(|l| l.path.as_str()).collect();
        assert_eq!(paths, ["/events/1", "/b", "/a"]);
    }

    #[test]
    fn a_dynamic_route_path_marks_the_table_incomplete() {
        let src = "import { Route } from 'react-router-dom';\nexport const R = () => <Route path={base} element={<X />} />;\n";
        let facts = scan_file("src/R.tsx", src, &known(&[]));
        assert!(facts.incomplete);
    }
}
