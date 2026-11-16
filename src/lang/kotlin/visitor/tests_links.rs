//! A keretrendszer-kapcsolatok tesztjei: Compose-képernyő ↔ ViewModel, Hilt / Dagger DI, Compose Navigation,
//! interfész ↔ implementáció a hívó-indexen át.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::kotlin::KotlinFrontend;
use crate::model::{Call, FileGraph};

fn graph_of(path: &str, text: &str) -> FileGraph {
    KotlinFrontend::with_defaults()
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap()
        .graph
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

const INJECTION: &str = r#"package com.example.app

import androidx.compose.runtime.Composable
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.activity.viewModels

@Composable
fun TodoScreen(viewModel: TodoViewModel = hiltViewModel(), onOpen: (String) -> Unit) {
    viewModel.load()
}

@Composable
fun OtherScreen() {
    val other = hiltViewModel<OtherViewModel>()
    val typed: ThirdViewModel = viewModel()
    other.refresh()
    typed.go()
}

class MainActivity : ComponentActivity() {
    private val vm: MainViewModel by viewModels()
    private val named by viewModels<NamedViewModel>()
    @Inject lateinit var repo: TodoRepository

    fun start() { vm.go() }
}
"#;

#[test]
fn view_model_factories_become_inject_edges_with_the_type() {
    let g = graph_of("/p/Screens.kt", INJECTION);
    let default_value = call(&g, "hiltViewModel(TodoViewModel)");
    assert_eq!(default_value.kind, "inject");
    assert_eq!(default_value.caller.as_deref(), Some("TodoScreen"));
    let generic = call(&g, "hiltViewModel(OtherViewModel)");
    assert_eq!(generic.kind, "inject");
    assert_eq!(generic.caller.as_deref(), Some("OtherScreen"));
    assert_eq!(call(&g, "viewModel(ThirdViewModel)").kind, "inject");
    let delegated = call(&g, "viewModels(MainViewModel)");
    assert_eq!(
        (
            delegated.kind.as_str(),
            delegated.container.as_deref(),
            delegated.caller.as_deref()
        ),
        ("inject", Some("MainActivity"), Some("__top__"))
    );
    assert_eq!(call(&g, "viewModels(NamedViewModel)").kind, "inject");
}

#[test]
fn view_model_calls_resolve_through_the_injected_type() {
    let g = graph_of("/p/Screens.kt", INJECTION);
    assert_eq!(
        call(&g, "viewModel.load").resolved.as_deref(),
        Some("TodoViewModel::load")
    );
    assert_eq!(
        call(&g, "other.refresh").resolved.as_deref(),
        Some("OtherViewModel::refresh")
    );
    assert_eq!(
        call(&g, "typed.go").resolved.as_deref(),
        Some("ThirdViewModel::go")
    );
    assert_eq!(
        call(&g, "vm.go").resolved.as_deref(),
        Some("MainViewModel::go")
    );
}

#[test]
fn field_injection_is_an_inject_edge() {
    let g = graph_of("/p/Screens.kt", INJECTION);
    let field = call(&g, "@Inject(TodoRepository)");
    assert_eq!(field.kind, "inject");
    assert_eq!(field.container.as_deref(), Some("MainActivity"));
}

const HILT: &str = r#"package com.example.di

@HiltViewModel
class TodoViewModel @Inject constructor(
    private val repo: TodoRepository,
    private val lazyClock: Lazy<Clock>,
    val name: String,
) : ViewModel() {
    fun load() { repo.fetchAll() }
}

@Module
@InstallIn(SingletonComponent::class)
abstract class DataModule {
    @Binds
    abstract fun bindRepo(impl: RepoImpl): TodoRepository

    companion object {
        @Provides
        @Singleton
        fun provideApi(retrofit: Retrofit): TodoApi = retrofit.create(TodoApi::class.java)
    }
}
"#;

#[test]
fn constructor_injection_lists_the_injected_types() {
    let g = graph_of("/p/Hilt.kt", HILT);
    let repo = call(&g, "@Inject(TodoRepository)");
    assert_eq!(
        (
            repo.kind.as_str(),
            repo.container.as_deref(),
            repo.caller.as_deref()
        ),
        ("inject", Some("TodoViewModel"), Some("constructor"))
    );
    assert_eq!(call(&g, "@Inject(Clock)").kind, "inject");
    assert!(
        g.calls.iter().all(|c| c.target != "@Inject(String)"),
        "{:?}",
        g.calls
    );
}

#[test]
fn provides_and_binds_name_the_provider_method_of_the_type() {
    let g = graph_of("/p/Hilt.kt", HILT);
    let binds = call(&g, "@Binds(TodoRepository <- RepoImpl)");
    assert_eq!(
        (
            binds.kind.as_str(),
            binds.container.as_deref(),
            binds.caller.as_deref()
        ),
        ("provide", Some("DataModule"), Some("bindRepo"))
    );
    let provides = call(&g, "@Provides(TodoApi)");
    assert_eq!(provides.kind, "provide");
    assert_eq!(provides.caller.as_deref(), Some("provideApi"));
}

const NAV: &str = r#"@Composable
fun AppNav(navController: NavHostController = rememberNavController()) {
    NavHost(navController, startDestination = "list") {
        composable("list") { ListScreen(onOpen = { id -> navController.navigate("detail/$id") }) }
        composable("detail/{id}") { DetailScreen() }
        composable(route = "settings") { SettingsScreen() }
    }
}

@Composable
fun ListScreen(onOpen: (Long) -> Unit) {}
"#;

#[test]
fn nav_host_composables_link_routes_to_screens() {
    let g = graph_of("/p/Nav.kt", NAV);
    let screens: Vec<(&str, Option<&Vec<String>>)> = g
        .calls
        .iter()
        .filter(|c| c.kind == "nav" && c.args.as_ref().is_some_and(|a| a[0] != "detail/$id"))
        .map(|c| (c.target.as_str(), c.args.as_ref()))
        .collect();
    assert!(
        screens.contains(&("ListScreen", Some(&vec!["list".to_string()]))),
        "{screens:?}"
    );
    assert!(screens.contains(&("DetailScreen", Some(&vec!["detail/{id}".to_string()]))));
    assert!(screens.contains(&("SettingsScreen", Some(&vec!["settings".to_string()]))));
    let screen = call(&g, "ListScreen");
    assert_eq!(screen.caller.as_deref(), Some("AppNav"));
}

#[test]
fn navigate_with_a_matching_literal_prefix_links_the_screen() {
    let g = graph_of("/p/Nav.kt", NAV);
    let linked: Vec<&Call> = g
        .calls
        .iter()
        .filter(|c| c.kind == "nav" && c.target == "DetailScreen")
        .collect();
    assert_eq!(linked.len(), 2, "{linked:?}");
    assert!(
        linked
            .iter()
            .any(|c| c.args.as_deref() == Some(&["detail/{id}".to_string()][..]) && c.line == 4)
    );
    // az eredeti navigate-hívás is megmarad
    assert!(g.calls.iter().any(|c| c.target == "navController.navigate"));
}

#[test]
fn interface_and_implementation_callers_see_each_other_for_kotlin() {
    let repo = graph_of(
        "/p/Repo.kt",
        r#"interface TodoRepo {
    fun load(): List<Todo>
}

class TodoRepoImpl : TodoRepo {
    override fun load(): List<Todo> = emptyList()
}
"#,
    );
    let model = graph_of(
        "/p/Vm.kt",
        r#"class TodoViewModel @Inject constructor(private val repo: TodoRepo) : ViewModel() {
    fun refresh() { repo.load() }
}
"#,
    );
    let direct = graph_of(
        "/p/Direct.kt",
        r#"class Direct(private val impl: TodoRepoImpl) {
    fun go() { impl.load() }
}
"#,
    );
    assert_eq!(
        repo.containers
            .iter()
            .find(|c| c.name == "TodoRepoImpl")
            .and_then(|c| c.implements.clone()),
        Some(vec!["TodoRepo".to_string()])
    );
    let index = CallerIndex::build([
        ("/p/Repo.kt", &repo),
        ("/p/Vm.kt", &model),
        ("/p/Direct.kt", &direct),
    ]);
    let via_of = |pattern: &str| {
        let mut found: Vec<(String, Option<String>)> = index
            .callers(pattern, 1)
            .unwrap()
            .into_iter()
            .map(|r| (r.caller, r.via))
            .collect();
        found.sort();
        found
    };
    assert_eq!(
        via_of("TodoRepoImpl::load"),
        vec![
            ("go".to_owned(), None),
            ("refresh".to_owned(), Some("TodoRepo".to_owned())),
        ]
    );
    assert_eq!(
        via_of("TodoRepo::load"),
        vec![
            ("go".to_owned(), Some("TodoRepoImpl".to_owned())),
            ("refresh".to_owned(), None),
        ]
    );
}

#[test]
fn navigate_resolves_the_route_declared_in_another_file() {
    let host = graph_of(
        "/p/AppNavHost.kt",
        "@Composable\nfun AppNavHost(nav: NavHostController) {\n    NavHost(nav, startDestination = \"list\") {\n        composable(\"list\") { ListScreen() }\n        composable(\"detail/{id}\") { DetailScreen() }\n        composable<Settings> { SettingsScreen() }\n    }\n}\n",
    );
    let list = graph_of(
        "/p/ListScreen.kt",
        "@Composable\nfun ListScreen(nav: NavHostController) {\n    val id = 1\n    nav.navigate(\"detail/$id\")\n    nav.navigate(Settings)\n}\n",
    );
    let detail = graph_of("/p/DetailScreen.kt", "@Composable\nfun DetailScreen() {}\n");
    assert_eq!(
        call(&list, "detail/$id").on.as_deref(),
        Some("route:detail/$id")
    );
    let index = CallerIndex::build([
        ("/p/AppNavHost.kt", &host),
        ("/p/ListScreen.kt", &list),
        ("/p/DetailScreen.kt", &detail),
    ]);
    let rows = index.callers("DetailScreen", 1).unwrap();
    assert!(rows.iter().any(|r| r.caller == "ListScreen"), "{rows:?}");
    let typed = index.callers("SettingsScreen", 1).unwrap();
    assert!(typed.iter().any(|r| r.caller == "ListScreen"), "{typed:?}");
    let rewritten = index.rewritten("DetailScreen");
    assert_eq!(rewritten.len(), 1, "{rewritten:?}");
    assert_eq!(rewritten[0].caller, "ListScreen");
}
