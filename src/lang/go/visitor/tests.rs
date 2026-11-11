//! A Go-visitor tesztjei a teljes frontenden át.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::go::GoFrontend;
use crate::model::{Call, FileGraph, Method};

fn graph(text: &str) -> FileGraph {
    GoFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/sample.go",
            text.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

fn method_in<'g>(g: &'g FileGraph, container: &str, name: &str) -> &'g Method {
    g.methods
        .iter()
        .find(|m| m.name == name && m.container.as_deref() == Some(container))
        .unwrap_or_else(|| panic!("no method {container}.{name}: {:?}", g.methods))
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

const SAMPLE: &str = r#"// Package shop contains the shop services.
package shop

import (
	"errors"
	"fmt"
	"net/http"
	mylog "example.com/app/log"
	_ "github.com/lib/pq"
	"github.com/gin-gonic/gin"
)

const MaxSize = 10

const (
	modeA = iota
	modeB
)

var ErrNotFound = errors.New("not found")

// Repo stores users.
type Repo struct {
	db   *sql.DB
	Name string `json:"name"`
	Base
	*Logger
}

type Finder interface {
	Fetcher
	Find(id int) (*User, error)
	close()
}

type ID int

type Alias = Repo

type Stack[T any] struct {
	items []T
}

// NewRepo builds a repository.
func NewRepo(db *sql.DB) *Repo {
	return &Repo{db: db}
}

// Find loads one user.
func (r *Repo) Find(id int) (*User, error) {
	r.check(id)
	fmt.Println("find", id)
	rows := make([]int, 0)
	rows = append(rows, id)
	defer r.db.Close()
	go func() {
		mylog.Info("async")
	}()
	resp, _ := http.Get("http://x")
	_ = gin.Default()
	return nil, errors.New("x")
}

func (r *Repo) check(id int) {}

func (s *Stack[T]) Push(v T) { s.items = append(s.items, v) }

func Map[T, U any](xs []T, f func(T) U) []U {
	return nil
}

func (u User) String() string { return "" }

func init() {}

func init() {}
"#;

#[test]
fn structure_types_and_ranges() {
    let g = graph(SAMPLE);
    assert_eq!(g.framework.as_deref(), Some("gin"));
    assert_eq!(g.namespace.as_deref(), Some("shop"));
    let kinds: Vec<(&str, &str)> = g
        .containers
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("module", "sample.go"),
            ("extension", "User"),
            ("class", "Repo"),
            ("interface", "Finder"),
            ("type", "ID"),
            ("alias", "Alias"),
            ("class", "Stack"),
        ]
    );
    assert_eq!(
        g.containers[0].doc.as_deref(),
        Some("Package shop contains the shop services.")
    );
    let repo = &g.containers[2];
    assert_eq!((repo.line, repo.end_line), (23, 28));
    assert_eq!(repo.doc.as_deref(), Some("Repo stores users."));
    // a beágyazott típusok (pointerrel is) az „ősök”
    assert_eq!(
        repo.extends.as_deref(),
        Some(&["Base".to_string(), "Logger".to_string()][..])
    );
    let finder = &g.containers[3];
    assert_eq!(
        finder.extends.as_deref(),
        Some(&["Fetcher".to_string()][..])
    );
    assert_eq!(finder.is_abstract, Some(true));
    assert_eq!(
        g.containers[4].extends.as_deref(),
        Some(&["int".to_string()][..])
    );
    assert_eq!(
        g.containers[5].extends.as_deref(),
        Some(&["Repo".to_string()][..])
    );
    assert_eq!(
        g.containers[6].traits.as_deref(),
        Some(&["[T any]".to_string()][..])
    );
}

#[test]
fn methods_receivers_and_signatures() {
    let g = graph(SAMPLE);
    let find = method_in(&g, "Repo", "Find");
    assert_eq!(find.kind.as_deref(), Some("method"));
    assert_eq!(find.line, 50);
    assert_eq!(find.params, ["id int"]);
    assert_eq!(find.return_type.as_deref(), Some("(*User, error)"));
    assert_eq!(find.doc.as_deref(), Some("Find loads one user."));
    assert_eq!(find.visibility.as_deref(), Some("public"));
    assert_eq!(
        method_in(&g, "Repo", "check").visibility.as_deref(),
        Some("private")
    );
    // generikus típus receivere: a típusparaméter nem része a container nevének
    assert_eq!(method_in(&g, "Stack", "Push").params, ["v T"]);
    // a típus másik fájlban van: a metódus az `extension` containerhez tartozik
    assert_eq!(
        method_in(&g, "User", "String").return_type.as_deref(),
        Some("string")
    );
    let new_repo = method_in(&g, "sample.go", "NewRepo");
    assert_eq!(new_repo.kind.as_deref(), Some("function"));
    assert_eq!(new_repo.params, ["db *sql.DB"]);
    assert_eq!(new_repo.return_type.as_deref(), Some("*Repo"));
    assert_eq!(
        new_repo.doc.as_deref(),
        Some("NewRepo builds a repository.")
    );
    let map = method_in(&g, "sample.go", "Map");
    assert_eq!(map.params, ["xs []T", "f func(T) U"]);
    assert_eq!(map.return_type.as_deref(), Some("[]U"));
    // interfész-metódusok
    let iface = method_in(&g, "Finder", "Find");
    assert!(iface.is_abstract);
    assert_eq!(iface.return_type.as_deref(), Some("(*User, error)"));
    assert_eq!(
        method_in(&g, "Finder", "close").visibility.as_deref(),
        Some("private")
    );
    // többszörös `init`
    let inits: Vec<&str> = g
        .methods
        .iter()
        .filter(|m| m.name.starts_with("init"))
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(inits, ["init", "init@L76"]);
}

#[test]
fn constants_properties_and_imports() {
    let g = graph(SAMPLE);
    let consts: Vec<(&str, &str, Option<&str>)> = g
        .constants
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        consts,
        [
            ("const", "MaxSize", Some("10")),
            ("const", "modeA", Some("iota")),
            ("const", "modeB", None),
            ("static", "ErrNotFound", Some("errors.New(\"not found\")")),
        ]
    );
    assert_eq!(g.constants[0].visibility.as_deref(), Some("public"));
    assert_eq!(g.constants[1].visibility.as_deref(), Some("private"));
    let props: Vec<(&str, &str, Option<&str>)> = g
        .properties
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.visibility.as_str(),
                p.type_hint.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        props,
        [
            ("db", "private", Some("*sql.DB")),
            ("Name", "public", Some("string")),
            ("items", "private", Some("[]T")),
        ]
    );
    for (alias, full) in [
        ("fmt", "fmt"),
        ("http", "net/http"),
        ("mylog", "example.com/app/log"),
        ("gin", "github.com/gin-gonic/gin"),
        ("github.com/lib/pq", "github.com/lib/pq"),
    ] {
        assert_eq!(g.imports.get(alias), Some(full), "{alias}");
    }
}

#[test]
fn calls_are_categorized() {
    let g = graph(SAMPLE);
    let println = call(&g, "fmt.Println");
    assert_eq!(println.category, "native_go");
    assert_eq!(println.kind, "method_call");
    assert_eq!(println.on.as_deref(), Some("fmt"));
    let check = call(&g, "r.check");
    assert_eq!(check.category, "internal_helper");
    assert_eq!(check.on, None);
    assert_eq!(call(&g, "make").category, "native_go");
    assert_eq!(call(&g, "append").category, "native_go");
    // `defer` és `go func() { … }()` a külső metódushoz tartozik
    let close = call(&g, "r.db.Close");
    assert_eq!(close.category, "native_go");
    assert_eq!(close.on.as_deref(), Some("r"));
    assert_eq!(close.caller.as_deref(), Some("Find"));
    let info = call(&g, "mylog.Info");
    assert_eq!(info.category, "external_call");
    assert_eq!(info.caller.as_deref(), Some("Find"));
    assert_eq!(call(&g, "http.Get").category, "native_go");
    assert_eq!(call(&g, "gin.Default").category, "external_call");
    let literal = call(&g, "Repo");
    assert_eq!(
        (literal.kind.as_str(), literal.category.as_str()),
        ("new", "instantiation")
    );
    assert_eq!(literal.caller.as_deref(), Some("NewRepo"));
    // csomag-szintű inicializáló
    assert!(
        g.calls
            .iter()
            .any(|c| c.target == "errors.New" && c.caller.as_deref() == Some("__module__")),
        "{:?}",
        g.calls
    );
    assert!(g.methods.iter().any(|m| m.name == "__module__"));
}

#[test]
fn strings_skip_struct_tags() {
    let g = graph(SAMPLE);
    let values: Vec<&str> = g.strings.iter().map(|s| s.value.as_str()).collect();
    for expected in ["find", "async", "not found", "http://x"] {
        assert!(values.contains(&expected), "{expected}: {values:?}");
    }
    assert!(!values.iter().any(|v| v.contains("json:")), "{values:?}");
}

#[test]
fn framework_from_imports() {
    assert_eq!(
        graph("package a\n\nimport \"github.com/labstack/echo/v4\"\n")
            .framework
            .as_deref(),
        Some("echo")
    );
    assert_eq!(
        graph("package a\n\nimport \"fmt\"\n").framework.as_deref(),
        Some("go")
    );
}

#[test]
fn method_before_its_type_needs_no_extension() {
    let g = graph("package a\n\nfunc (t *T) M() {}\n\ntype T struct{}\n");
    let kinds: Vec<&str> = g.containers.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, ["module", "class"]);
    assert_eq!(method_in(&g, "T", "M").kind.as_deref(), Some("method"));
}

#[test]
fn generic_calls_and_directives() {
    let g = graph(
        "package a\n\n//go:generate stringer -type=K\n// K is a kind.\ntype K int\n\nfunc f(xs []int) {\n\tMap[int](xs)\n}\n",
    );
    assert_eq!(
        g.containers[1].doc.as_deref(),
        Some("K is a kind."),
        "{:?}",
        g.containers
    );
    assert_eq!(call(&g, "Map").kind, "func_call");
}

#[test]
fn grouped_types_and_named_results() {
    let g = graph(
        "package a\n\ntype (\n\tA struct{ X int }\n\tB interface{ Do() }\n)\n\nfunc (a *A) Split(n int) (head, tail string, err error) { return }\n",
    );
    let kinds: Vec<(&str, &str)> = g
        .containers
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [("module", "sample.go"), ("class", "A"), ("interface", "B")]
    );
    assert_eq!((g.containers[1].line, g.containers[1].end_line), (4, 4));
    assert_eq!(
        method_in(&g, "A", "Split").return_type.as_deref(),
        Some("(head, tail string, err error)")
    );
}
