//! A dinamikus (`dynamic_call`) hívás-jelölés tesztjei: a pozitív minták, a literál (nem dinamikus) esetek és a
//! közönséges hívások változatlansága.

use crate::lang::php::PhpBackend;
use crate::model::Call;

/// Egy osztály-metódus törzse a megadott sorokkal.
fn calls(body: &str) -> Vec<Call> {
    let src = format!(
        "<?php\nclass A {{\n  function f($obj, $class, $fn, $x, $m, $var, $expr) {{\n{body}\n  }}\n}}\n"
    );
    PhpBackend::with_defaults()
        .analyze_source("a.php", src.as_bytes())
        .graph
        .calls
}

/// A `dynamic_call` jelölések céljai, sorrendben.
fn dynamic(body: &str) -> Vec<String> {
    calls(body)
        .into_iter()
        .filter(|c| c.category == "dynamic_call")
        .map(|c| c.target)
        .collect()
}

fn none(body: &str) {
    assert_eq!(dynamic(body), Vec::<String>::new(), "{body}");
}

#[test]
fn variable_method_name() {
    assert_eq!(
        dynamic("$this->$m('favorites', 1);"),
        vec!["$this->$m".to_string()]
    );
    assert_eq!(dynamic("$obj->$m();"), vec!["$obj->$m".to_string()]);
}

#[test]
fn expression_method_name() {
    assert_eq!(dynamic("$obj->{$expr}(1);"), vec!["$obj->{$expr}"]);
    assert_eq!(dynamic("$obj->{'get' . $x}();"), vec!["$obj->{'get' . $x}"]);
}

#[test]
fn nullsafe_variable_method_name() {
    assert_eq!(dynamic("$obj?->$m();"), vec!["$obj?->$m"]);
}

#[test]
fn static_call_with_variable_class_or_method() {
    assert_eq!(dynamic("$class::$m(1);"), vec!["$class::$m"]);
    assert_eq!(dynamic("$class::{$expr}(1);"), vec!["$class::{$expr}"]);
    assert_eq!(dynamic("Foo::$m();"), vec!["Foo::$m"]);
    assert_eq!(dynamic("static::$m();"), vec!["static::$m"]);
}

#[test]
fn dynamic_method_name_is_not_a_bogus_edge() {
    let all = calls("$this->$m();\nFoo::$m();\n$obj->{$expr}();");
    // nincs hibás célú él (`<dynamic>`, üres név): csak a három jelölés
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all.iter().all(|c| c.kind == "dynamic"
        && c.category == "dynamic_call"
        && c.resolved.is_none()
        && c.on.is_none()));
    assert!(all.iter().all(|c| !c.target.contains("<dynamic>")));
}

#[test]
fn variable_function_call() {
    assert_eq!(dynamic("$fn(1);"), vec!["$fn"]);
    assert_eq!(
        dynamic("$this->handlers['k'](1);"),
        vec!["$this->handlers['k']"]
    );
    assert_eq!(dynamic("($this->cb)(1);"), vec!["$this->cb"]);
    assert_eq!(dynamic("make()(1);"), vec!["make()"]);
}

#[test]
fn variable_class_instantiation() {
    assert_eq!(dynamic("new $class(1);"), vec!["new $class"]);
    assert_eq!(dynamic("new $obj->prop(1);"), vec!["new $obj->prop"]);
    assert_eq!(dynamic("new $class;"), vec!["new $class"]);
}

#[test]
fn call_user_func_with_non_literal_first_argument() {
    assert_eq!(
        dynamic("call_user_func($x, 1);"),
        vec!["call_user_func($x)"]
    );
    assert_eq!(
        dynamic("call_user_func_array([$obj, $m], []);"),
        vec!["call_user_func_array([$obj, $m])"]
    );
    assert_eq!(dynamic("\\call_user_func($x);"), vec!["call_user_func($x)"]);
    assert_eq!(
        dynamic("call_user_func(...$x);"),
        vec!["call_user_func($x)"]
    );
}

#[test]
fn call_user_func_keeps_its_func_call_and_adds_the_marker() {
    let all = calls("call_user_func($x);");
    assert!(
        all.iter()
            .any(|c| c.kind == "func_call" && c.target == "call_user_func")
    );
    assert!(all.iter().any(|c| c.category == "dynamic_call"));
}

#[test]
fn call_user_func_with_literal_callable_is_not_dynamic() {
    none("call_user_func('strlen', 'x');");
    none("call_user_func_array('strlen', ['x']);");
    none("call_user_func([$this, 'run']);");
    none("call_user_func_array([Foo::class, 'run'], []);");
    none("call_user_func(array($this, 'run'));");
    none("call_user_func();");
}

#[test]
fn container_resolution_with_non_literal_argument() {
    assert_eq!(dynamic("app($var);"), vec!["app($var)"]);
    assert_eq!(dynamic("resolve($var);"), vec!["resolve($var)"]);
    assert_eq!(
        dynamic("$this->app->make($var);"),
        vec!["$this->app->make($var)"]
    );
    assert_eq!(dynamic("app()->make($var);"), vec!["app()->make($var)"]);
    assert_eq!(dynamic("app($a . 'x');"), vec!["app($a . 'x')"]);
}

#[test]
fn container_resolution_with_literal_argument_is_not_dynamic() {
    none("app('cache');");
    none("app(Foo::class);");
    none("resolve('cache');");
    none("resolve(\\App\\Foo::class);");
    none("$this->app->make(Foo::class);");
    none("$this->app->make('cache');");
    none("app()->make(Foo::class);");
    none("app()->make('cache');");
    none("app();");
}

#[test]
fn make_on_other_receivers_is_not_dynamic() {
    none("$this->factory->make($var);");
    none("$obj->make($var);");
    none("$this->app->bind($var);");
}

#[test]
fn ordinary_calls_are_unchanged() {
    none("$this->run(); $obj->go(1); Foo::bar(); strlen($x); new Foo(1); $obj?->m();");
    none("(function () { return 1; })();");
    none("$a = fn() => 1; array_map($a, []);");
    none("$obj->prop;");
    let all = calls("$this->run(); Foo::bar(); new Foo(1); strlen($x);");
    let kinds: Vec<_> = all.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(
        kinds,
        vec!["method_call", "static_call", "new", "func_call"]
    );
}

#[test]
fn literal_method_chains_stay_ordinary() {
    none("$this->repo->find(1)->save();");
    none("Foo::make()->run();");
}

#[test]
fn long_expression_is_shortened() {
    let long =
        "$this->handlers['aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa']";
    let found = dynamic(&format!("{long}(1);"));
    assert_eq!(found.len(), 1);
    assert!(found[0].chars().count() <= 60, "{}", found[0]);
    assert!(found[0].ends_with('…'));
}

#[test]
fn multiline_expression_becomes_one_line() {
    let found = dynamic("$obj\n    ->{$expr}(1);");
    assert_eq!(found, vec!["$obj ->{$expr}"]);
}

#[test]
fn marker_carries_caller_line_and_container() {
    let all = calls("$x = 1;\n$this->$m();");
    let c = all.iter().find(|c| c.category == "dynamic_call").unwrap();
    assert_eq!(c.container.as_deref(), Some("A"));
    assert_eq!(c.caller.as_deref(), Some("f"));
    assert_eq!(c.line, 5);
    assert_eq!(c.kind, "dynamic");
}

/// Teljes forrás (saját osztállyal) hívásai.
fn full(src: &str) -> Vec<Call> {
    PhpBackend::with_defaults()
        .analyze_source("a.php", src.as_bytes())
        .graph
        .calls
}

fn full_dynamic(src: &str) -> Vec<String> {
    full(src)
        .into_iter()
        .filter(|c| c.category == "dynamic_call")
        .map(|c| c.target)
        .collect()
}

#[test]
fn closure_and_callable_parameters_are_callbacks_not_dynamic() {
    for hint in [
        "Closure",
        "\\Closure",
        "callable",
        "?Closure",
        "Closure|callable",
        "?callable",
    ] {
        let src = format!(
            "<?php\nclass M {{\n  function handle(Request $r, {hint} $next) {{ return $next($r); }}\n}}\n"
        );
        assert_eq!(full_dynamic(&src), Vec::<String>::new(), "{hint}");
    }
}

#[test]
fn untyped_or_mixed_parameter_call_stays_dynamic() {
    let src = "<?php\nclass M {\n  function a($handler) { $handler(1); }\n  function b(mixed $h) { $h(1); }\n  function c(string $s) { $s(1); }\n}\n";
    assert_eq!(full_dynamic(src), vec!["$handler", "$h", "$s"]);
}

#[test]
fn callback_parameter_of_another_method_does_not_leak() {
    let src = "<?php\nclass M {\n  function a(Closure $next) { $next(1); }\n  function b($next) { $next(1); }\n}\n";
    assert_eq!(full_dynamic(src), vec!["$next"]);
}

#[test]
fn local_closure_assignment_is_not_dynamic() {
    none("$f = function ($a) { return $a; }; $f(1);");
    none("$f = fn ($a) => $a; $f(1);");
    none("$f = static fn ($a) => $a; $f(1);");
    none("$f = static function ($a) { return $a; }; $f(1);");
    // más változó marad dinamikus
    assert_eq!(dynamic("$f = fn () => 1; $g(1);"), vec!["$g"]);
    // nem closure értékadás
    assert_eq!(dynamic("$f = $obj->get(); $f(1);"), vec!["$f"]);
}

#[test]
fn closure_parameter_of_a_closure_is_a_callback() {
    none("$f = function (Closure $next) { return $next(1); };");
    none("$f = fn (callable $cb) => $cb(1);");
}

#[test]
fn top_level_function_callback_parameter() {
    let src = "<?php\nfunction run(Closure $next, $other) { $next(1); $other(2); }\n";
    assert_eq!(full_dynamic(src), vec!["$other"]);
}

#[test]
fn invokable_class_property_is_an_invoke_call() {
    let src = "<?php\nnamespace App;\nclass S {\n  public function __construct(private readonly Gen $gen) {}\n  function run() { return ($this->gen)(1); }\n}\n";
    let all = full(src);
    assert!(all.iter().all(|c| c.category != "dynamic_call"), "{all:?}");
    let call = all
        .iter()
        .find(|c| c.target == "__invoke")
        .expect("__invoke call");
    assert_eq!(call.kind, "method_call");
    assert_eq!(call.resolved.as_deref(), Some("App\\Gen::__invoke"));
}

#[test]
fn declared_property_with_class_type_is_an_invoke_call() {
    let src = "<?php\nclass S {\n  private Gen $gen;\n  function run() { return ($this->gen)(1); }\n}\n";
    let all = full(src);
    assert!(all.iter().all(|c| c.category != "dynamic_call"), "{all:?}");
    assert!(
        all.iter()
            .any(|c| c.target == "__invoke" && c.resolved.as_deref() == Some("Gen::__invoke"))
    );
}

#[test]
fn closure_or_untyped_property_call_stays_dynamic() {
    let src = "<?php\nclass S {\n  private Closure $cb;\n  private \\Closure $cb2;\n  private $raw;\n  private callable $c3;\n  function run() { ($this->cb)(1); ($this->cb2)(1); ($this->raw)(1); ($this->c3)(1); ($this->unknown)(1); }\n}\n";
    assert_eq!(
        full_dynamic(src),
        vec![
            "$this->cb",
            "$this->cb2",
            "$this->raw",
            "$this->c3",
            "$this->unknown"
        ]
    );
}

#[test]
fn the_remaining_dynamic_shapes_stay_marked() {
    assert_eq!(
        dynamic("$this->{'create' . ucfirst($x) . 'Driver'}();"),
        vec!["$this->{'create' . ucfirst($x) . 'Driver'}"]
    );
    assert_eq!(dynamic("$obj->{$expr}(1);"), vec!["$obj->{$expr}"]);
    assert_eq!(dynamic("$obj->$m(...$x);"), vec!["$obj->$m"]);
    assert_eq!(
        dynamic("$this->app->make(self::cls());"),
        vec!["$this->app->make(self::cls())"]
    );
    assert_eq!(dynamic("app()->make($class);"), vec!["app()->make($class)"]);
    assert_eq!(dynamic("new $class;"), vec!["new $class"]);
}

#[test]
fn works_at_top_level() {
    let src = "<?php\n$fn(1);\n";
    let g = PhpBackend::with_defaults()
        .analyze_source("a.php", src.as_bytes())
        .graph;
    let c = g
        .calls
        .iter()
        .find(|c| c.category == "dynamic_call")
        .unwrap();
    assert_eq!(
        (c.target.as_str(), c.caller.as_deref()),
        ("$fn", Some("__top__"))
    );
}
