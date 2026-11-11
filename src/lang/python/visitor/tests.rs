//! A Python-visitor tesztjei a teljes frontenden át.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::python::PythonFrontend;
use crate::model::{Call, FileGraph, Method};

fn graph(text: &str) -> FileGraph {
    PythonFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/sample.py",
            text.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

fn method<'g>(g: &'g FileGraph, name: &str) -> &'g Method {
    g.methods
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no method {name}: {:?}", g.methods))
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

const SAMPLE: &str = r#""""Sample module."""
import os
import numpy as np
from app.models import User, Order as O
from . import helpers

MAX_SIZE = 10
TIMEOUT: float = 2.5
__all__ = ["Repo", "make"]


@dataclass(frozen=True)
class Repo(Base):
    """Stores users."""

    table = "users"
    LIMIT = 5
    name: str

    def __init__(self, db):
        self.db = db

    @staticmethod
    def make(kind: str = "x", *args, **kw) -> "Repo":
        return Repo(kind)

    @classmethod
    def build(cls):
        return cls.make()

    @property
    def size(self) -> int:
        return len(self.items())

    async def fetch(self, user: User, n: int = 3) -> list[User]:
        """Loads users."""
        rows = await self.db.query("select * from users")
        self.save(rows)
        print(f"got {n} rows")
        return [x for x in rows if x]

    def _hidden(self):
        pass

    def save(self, rows):
        os.path.join("a", "b")
        np.array(rows)
        helpers.run()
        Repo.make()


def make(a: int, b: str = "x") -> bool:
    """Makes it."""
    def inner():
        return helper()
    f = lambda v: transform(v)
    return inner() and f(a)


def helper():
    return User()


if __name__ == "__main__":
    make(1)
"#;

#[test]
fn structure_classes_functions_and_ranges() {
    let g = graph(SAMPLE);
    assert_eq!(g.framework.as_deref(), Some("python"));
    let kinds: Vec<(&str, &str)> = g
        .containers
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str()))
        .collect();
    assert_eq!(kinds, [("module", "sample.py"), ("class", "Repo")]);
    let repo = &g.containers[1];
    // a dekorátor sora a tartomány eleje
    assert_eq!((repo.line, repo.end_line), (12, 49));
    assert_eq!(repo.extends.as_deref(), Some(&["Base".to_string()][..]));
    assert_eq!(
        repo.traits.as_deref(),
        Some(&["@dataclass(frozen=True)".to_string()][..])
    );
    assert_eq!(repo.doc.as_deref(), Some("Stores users."));
    assert_eq!(g.containers[0].doc.as_deref(), Some("Sample module."));
    let names: Vec<&str> = g.methods.iter().map(|m| m.name.as_str()).collect();
    for n in [
        "__init__", "make", "build", "size", "fetch", "_hidden", "save", "helper",
    ] {
        assert!(names.contains(&n), "{n}: {names:?}");
    }
    assert!(
        !names.contains(&"inner") && !names.contains(&"<lambda>"),
        "{names:?}"
    );
}

#[test]
fn method_kinds_flags_and_signatures() {
    let g = graph(SAMPLE);
    let make = g
        .methods
        .iter()
        .find(|m| m.name == "make" && m.container.as_deref() == Some("Repo"))
        .unwrap();
    assert_eq!(make.kind.as_deref(), Some("staticmethod"));
    assert!(make.is_static);
    assert_eq!(make.line, 23, "decorator line starts the range");
    assert_eq!(make.params, ["kind: str = \"x\"", "*args", "**kw"]);
    assert_eq!(make.return_type.as_deref(), Some("\"Repo\""));
    assert_eq!(method(&g, "build").kind.as_deref(), Some("classmethod"));
    assert_eq!(method(&g, "size").kind.as_deref(), Some("property"));
    let fetch = method(&g, "fetch");
    assert_eq!(fetch.kind.as_deref(), Some("async method"));
    // `self` kimarad a szignatúrából
    assert_eq!(fetch.params, ["user: User", "n: int = 3"]);
    assert_eq!(fetch.return_type.as_deref(), Some("list[User]"));
    assert_eq!(fetch.doc.as_deref(), Some("Loads users."));
    assert_eq!(method(&g, "_hidden").visibility.as_deref(), Some("private"));
    assert_eq!(method(&g, "__init__").visibility.as_deref(), Some("public"));
    let free = g
        .methods
        .iter()
        .find(|m| m.name == "make" && m.container.as_deref() == Some("sample.py"))
        .unwrap();
    assert_eq!(free.kind.as_deref(), Some("function"));
    assert_eq!(free.params, ["a: int", "b: str = \"x\""]);
    assert_eq!(free.return_type.as_deref(), Some("bool"));
}

#[test]
fn constants_properties_and_imports() {
    let g = graph(SAMPLE);
    let consts: Vec<(&str, Option<&str>)> = g
        .constants
        .iter()
        .map(|c| (c.name.as_str(), c.value.as_deref()))
        .collect();
    assert!(consts.contains(&("MAX_SIZE", Some("10"))), "{consts:?}");
    assert!(consts.contains(&("TIMEOUT", Some("2.5"))), "{consts:?}");
    assert!(
        consts.contains(&("__all__", Some("[\"Repo\", \"make\"]"))),
        "{consts:?}"
    );
    assert!(consts.contains(&("LIMIT", Some("5"))), "{consts:?}");
    let props: Vec<(&str, Option<&str>)> = g
        .properties
        .iter()
        .map(|p| (p.name.as_str(), p.type_hint.as_deref()))
        .collect();
    assert_eq!(props, [("table", Some("str")), ("name", Some("str"))]);
    for (alias, full) in [
        ("os", "os"),
        ("np", "numpy"),
        ("User", "app.models.User"),
        ("O", "app.models.Order"),
        ("helpers", ".helpers"),
    ] {
        assert_eq!(g.imports.get(alias), Some(full), "{alias}");
    }
}

#[test]
fn calls_are_categorized() {
    let g = graph(SAMPLE);
    assert_eq!(call(&g, "self.save").category, "internal_helper");
    assert_eq!(call(&g, "cls.make").category, "internal_helper");
    assert_eq!(call(&g, "self.db.query").on.as_deref(), Some("self"));
    assert_eq!(call(&g, "len").category, "native_python");
    assert_eq!(call(&g, "print").category, "native_python");
    assert_eq!(call(&g, "os.path.join").category, "native_python");
    assert_eq!(call(&g, "np.array").category, "external_call");
    assert_eq!(call(&g, "helpers.run").category, "external_call");
    assert_eq!(call(&g, "Repo.make").category, "static_call");
    assert_eq!(call(&g, "helper").category, "internal_helper");
    assert_eq!(call(&g, "User").category, "instantiation");
    let repo = call(&g, "Repo");
    assert_eq!(
        (repo.kind.as_str(), repo.category.as_str()),
        ("new", "instantiation")
    );
    // beágyazott függvény és lambda hívása a külső függvényhez tartozik
    assert_eq!(call(&g, "transform").caller.as_deref(), Some("make"));
    assert_eq!(call(&g, "inner").caller.as_deref(), Some("make"));
    // modul-szintű utasítás
    let main = g.calls.iter().rev().find(|c| c.target == "make").unwrap();
    assert_eq!(main.caller.as_deref(), Some("__module__"));
    assert!(g.methods.iter().any(|m| m.name == "__module__"));
}

#[test]
fn strings_skip_docstrings_and_keep_fstring_text() {
    let g = graph(SAMPLE);
    let values: Vec<&str> = g.strings.iter().map(|s| s.value.as_str()).collect();
    assert!(values.contains(&"select * from users"), "{values:?}");
    assert!(values.contains(&"got {…} rows"), "{values:?}");
    assert!(!values.contains(&"Loads users."), "{values:?}");
}

#[test]
fn framework_from_imports() {
    assert_eq!(
        graph("from flask import Flask\n").framework.as_deref(),
        Some("flask")
    );
    assert_eq!(
        graph("import fastapi\n").framework.as_deref(),
        Some("fastapi")
    );
    assert_eq!(graph("import os\n").framework.as_deref(), Some("python"));
}

#[test]
fn conditional_definitions_and_setter_collision() {
    let g = graph(
        "try:\n    import ujson as json\nexcept ImportError:\n    import json\n\n\
         if True:\n    def cond():\n        pass\n\n\
         class A:\n    @property\n    def x(self):\n        return 1\n\n    @x.setter\n    def x(self, v):\n        pass\n",
    );
    assert!(g.methods.iter().any(|m| m.name == "cond"));
    let xs: Vec<&str> = g
        .methods
        .iter()
        .filter(|m| m.name.starts_with('x'))
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(xs, ["x", "x@L15"]);
}
