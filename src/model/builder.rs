//! Builder: a visitorok ebbe gyűjtenek, a végén immutábilis `FileGraph` lesz belőle.

use super::{
    Call, Comment, Constant, Container, ControlFlow, FileGraph, Imports, Method, Property,
    StringLit, Write,
};

/// A `FileGraph` lépésenkénti összeállítója.
#[derive(Debug, Default)]
pub struct FileGraphBuilder {
    file: String,
    module: Option<String>,
    sfc: Option<serde_json::Value>,
    framework: Option<String>,
    namespace: Option<String>,
    imports: Imports,
    containers: Vec<Container>,
    properties: Vec<Property>,
    constants: Vec<Constant>,
    methods: Vec<Method>,
    calls: Vec<Call>,
    comments: Vec<Comment>,
    control_flow: Vec<ControlFlow>,
    strings: Vec<StringLit>,
    writes: Vec<Write>,
}

impl FileGraphBuilder {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            ..Self::default()
        }
    }

    pub fn module(&mut self, module: Option<String>) -> &mut Self {
        self.module = module;
        self
    }

    pub fn sfc(&mut self, sfc: Option<serde_json::Value>) -> &mut Self {
        self.sfc = sfc;
        self
    }

    pub fn framework(&mut self, framework: Option<String>) -> &mut Self {
        self.framework = framework;
        self
    }

    /// A fájl névtere: az első megnevezett `namespace` marad.
    pub fn namespace_if_unset(&mut self, namespace: Option<String>) -> &mut Self {
        if self.namespace.is_none() {
            self.namespace = namespace;
        }
        self
    }

    pub fn import(&mut self, alias: String, full: String) -> &mut Self {
        self.imports.insert(alias, full);
        self
    }

    pub fn imports_as_object(&mut self) -> &mut Self {
        self.imports.serialize_empty_as_object();
        self
    }

    pub fn container(&mut self, c: Container) -> &mut Self {
        self.containers.push(c);
        self
    }

    pub fn property(&mut self, p: Property) -> &mut Self {
        self.properties.push(p);
        self
    }

    pub fn constant(&mut self, c: Constant) -> &mut Self {
        self.constants.push(c);
        self
    }

    pub fn method(&mut self, m: Method) -> &mut Self {
        self.methods.push(m);
        self
    }

    /// Az eddig felvett metódusok száma (a záró sor utólagos beállításához, pl. CSS-blokkoknál).
    pub fn method_count(&self) -> usize {
        self.methods.len()
    }

    /// Egy már felvett metódus záró sorának utólagos beállítása.
    pub fn set_method_end(&mut self, index: usize, end_line: u32) {
        if let Some(method) = self.methods.get_mut(index) {
            method.end_line = Some(end_line);
        }
    }

    pub fn call(&mut self, c: Call) -> &mut Self {
        self.calls.push(c);
        self
    }

    pub fn comment(&mut self, c: Comment) -> &mut Self {
        self.comments.push(c);
        self
    }

    pub fn control_flow(&mut self, c: ControlFlow) -> &mut Self {
        self.control_flow.push(c);
        self
    }

    pub fn string(&mut self, s: StringLit) -> &mut Self {
        self.strings.push(s);
        self
    }

    pub fn write(&mut self, w: Write) -> &mut Self {
        self.writes.push(w);
        self
    }

    /// Lezárja az építést.
    /// Az első container `doc`-ja (a JS-frontend fő containeréhez; csak ha még üres).
    pub fn first_container_doc_is_none(&self) -> bool {
        self.containers.first().is_some_and(|c| c.doc.is_none())
    }

    pub fn set_first_container_doc(&mut self, doc: Option<String>) {
        if let Some(c) = self.containers.first_mut() {
            c.doc = doc;
        }
    }

    /// A metódusok stabil rendezése a kezdősoruk szerint (a JS-frontend több blokkja + a template-metódus miatt).
    pub fn sort_methods_by_line(&mut self) {
        self.methods.sort_by_key(|m| m.line);
    }

    pub fn build(self) -> FileGraph {
        FileGraph {
            file: self.file,
            module: self.module,
            sfc: self.sfc,
            framework: self.framework,
            namespace: self.namespace,
            imports: self.imports,
            containers: self.containers,
            properties: self.properties,
            constants: self.constants,
            methods: self.methods,
            calls: self.calls,
            comments: self.comments,
            control_flow: self.control_flow,
            strings: self.strings,
            writes: self.writes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_builder_gives_empty_graph() {
        let g = FileGraphBuilder::new("a.php").build();
        assert_eq!(g.file, "a.php");
        assert!(g.namespace.is_none() && g.containers.is_empty() && g.imports.is_empty());
    }

    #[test]
    fn first_namespace_wins() {
        let mut b = FileGraphBuilder::new("a.php");
        b.namespace_if_unset(Some("A".into()))
            .namespace_if_unset(Some("B".into()));
        assert_eq!(b.build().namespace.as_deref(), Some("A"));
    }
}
