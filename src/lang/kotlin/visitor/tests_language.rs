//! Kotlin nyelvi szélső esetek: `when` alanyal és `is` ágakkal, névtelen objektum, `sealed interface` hierarchia,
//! `inline` / `reified`, `typealias`, `operator fun`, infix hívás, scope-függvények, safe call, elvis, `!!`,
//! string-sablonok kifejezéssel.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::kotlin::KotlinFrontend;
use crate::model::{Call, FileGraph};

const SOURCE: &str = r#"package com.example.edge

typealias Handler = (Event) -> Unit

sealed interface Event {
    data class Click(val id: Long) : Event
    data object Back : Event
    class Failure(val cause: Throwable) : Event
}

interface Clock {
    fun now(): Long
}

inline fun <reified T> Gson.parse(json: String): T = fromJson(json, T::class.java)

data class Money(val cents: Long) {
    operator fun plus(other: Money) = Money(cents + other.cents)
    operator fun invoke() = cents
}

class Dispatcher(private val repo: Repo, private val handler: Handler) {
    fun handle(event: Event) {
        when (event) {
            is Event.Click -> repo.open(event.id)
            Event.Back -> repo.close()
            is Event.Failure -> log("failed: ${event.cause.message}")
        }
    }

    fun clock(): Clock = object : Clock {
        override fun now(): Long = System.currentTimeMillis()
    }

    fun scopes() {
        repo.let { it.fetch() }
        with(repo) { fetch() }
        repo.apply { refresh() }
        val pair = "a" to 1
        val name = repo?.label() ?: fallback()
        repo?.close()
        repo.owner!!.save()
        println("total: ${repo.total()} items, ${repo.items.size}")
    }

    private fun log(message: String) {}

    private fun fallback() = "none"
}
"#;

fn graph() -> FileGraph {
    KotlinFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/Edge.kt",
            SOURCE.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

#[test]
fn when_with_subject_and_is_branches() {
    let g = graph();
    let when = g.control_flow.iter().find(|f| f.kind == "when").unwrap();
    assert_eq!(when.cond.as_deref(), Some("event"));
    assert_eq!(when.caller.as_deref(), Some("handle"));
    assert_eq!(when.branches.len(), 3);
    assert!(when.branches[0].label.contains("Event.Click"));
    assert!(when.branches[1].label.contains("Event.Back"));
    let open = call(&g, "repo.open");
    assert_eq!(open.resolved.as_deref(), Some("Repo::open"));
    assert_eq!(open.caller.as_deref(), Some("handle"));
    assert_eq!(call(&g, "log").category, "internal_helper");
}

#[test]
fn sealed_interface_hierarchy_and_typealias() {
    let g = graph();
    let event = g.containers.iter().find(|c| c.name == "Event").unwrap();
    assert!(event.kind.contains("interface"), "{}", event.kind);
    for (name, kind) in [
        ("Event.Click", "class"),
        ("Event.Back", "object"),
        ("Event.Failure", "class"),
    ] {
        let nested = g
            .containers
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no container {name}: {:?}", g.containers));
        assert!(nested.kind.contains(kind), "{name}: {}", nested.kind);
        let parents = nested.extends.iter().chain(nested.implements.iter());
        assert!(
            parents.flatten().any(|p| p == "Event"),
            "{name}: {nested:?}"
        );
    }
    // a typealias nem típus-container, de a fájl többi része épen marad
    assert!(g.containers.iter().all(|c| c.name != "Handler"));
    assert!(g.containers.iter().any(|c| c.name == "Dispatcher"));
}

#[test]
fn anonymous_object_calls_belong_to_the_enclosing_function() {
    let g = graph();
    let now = call(&g, "System.currentTimeMillis");
    assert_eq!(now.caller.as_deref(), Some("clock"));
    assert!(g.methods.iter().any(|m| m.name == "clock"));
}

#[test]
fn inline_reified_and_operator_functions() {
    let g = graph();
    let parse = g.methods.iter().find(|m| m.name == "parse").unwrap();
    assert_eq!(parse.kind.as_deref(), Some("extension"));
    assert_eq!(call(&g, "fromJson").caller.as_deref(), Some("parse"));
    for name in ["plus", "invoke"] {
        assert!(
            g.methods
                .iter()
                .any(|m| m.name == name && m.container.as_deref() == Some("Money")),
            "no operator {name}: {:?}",
            g.methods
        );
    }
    assert_eq!(call(&g, "Money").category, "instantiation");
    assert_eq!(call(&g, "Money").caller.as_deref(), Some("plus"));
}

#[test]
fn scope_functions_keep_their_inner_calls() {
    let g = graph();
    assert_eq!(call(&g, "repo.let").category, "native_kotlin");
    let fetch = call(&g, "it.fetch");
    assert_eq!(
        (fetch.category.as_str(), fetch.caller.as_deref()),
        ("external_call", Some("scopes"))
    );
    assert_eq!(call(&g, "with").category, "native_kotlin");
    // `with(repo) { fetch() }` / `apply { refresh() }`: az implicit-this hívás puszta névvel marad
    assert_eq!(call(&g, "fetch").caller.as_deref(), Some("scopes"));
    assert_eq!(call(&g, "repo.apply").category, "native_kotlin");
    assert_eq!(call(&g, "refresh").caller.as_deref(), Some("scopes"));
}

#[test]
fn safe_call_elvis_and_non_null_keep_the_receiver() {
    let g = graph();
    let label = call(&g, "repo.label");
    assert_eq!(label.resolved.as_deref(), Some("Repo::label"));
    assert_eq!(label.on.as_deref(), Some("repo"));
    assert_eq!(call(&g, "fallback").category, "internal_helper");
    assert_eq!(
        call(&g, "repo.close").resolved.as_deref(),
        Some("Repo::close")
    );
    // `repo.owner!!.save()`: a `!!` nem szakítja meg az utat
    let save = call(&g, "repo.owner.save");
    assert_eq!(save.on.as_deref(), Some("repo"));
    assert_eq!(save.caller.as_deref(), Some("scopes"));
}

#[test]
fn string_templates_expose_their_expressions() {
    let g = graph();
    let total = call(&g, "repo.total");
    assert_eq!(total.resolved.as_deref(), Some("Repo::total"));
    assert_eq!(total.caller.as_deref(), Some("scopes"));
    assert!(
        g.strings.iter().any(|s| s.value.starts_with("total: ${")),
        "{:?}",
        g.strings
    );
    assert!(g.strings.iter().any(|s| s.value.starts_with("failed: ${")));
}

#[test]
fn infix_calls_do_not_break_the_outline() {
    let g = graph();
    let names: Vec<&str> = g
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some("Dispatcher"))
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "constructor",
            "handle",
            "clock",
            "scopes",
            "log",
            "fallback"
        ]
    );
}
