//! A Kotlin-visitor tesztjei a teljes frontenden át.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::kotlin::KotlinFrontend;
use crate::model::FileGraph;

fn graph(text: &str) -> FileGraph {
    KotlinFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/Sample.kt",
            text.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g crate::model::Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

fn container<'g>(g: &'g FileGraph, name: &str) -> &'g crate::model::Container {
    g.containers
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no container {name}: {:?}", g.containers))
}

fn method<'g>(g: &'g FileGraph, name: &str) -> &'g crate::model::Method {
    g.methods
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no method {name}: {:?}", g.methods))
}

const VIEW_MODEL: &str = r#"package com.example.app.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Holds the todo list state. */
@HiltViewModel
class TodoViewModel @Inject constructor(
    private val repository: TodoRepository,
    private val clock: Clock,
) : ViewModel() {
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state

    fun load() {
        viewModelScope.launch {
            val items = repository.fetchAll()
            _state.update { it.copy(items = items) }
            log("loaded ${items.size}")
        }
    }

    // TODO: debounce the refresh calls
    fun refresh() = load()

    private fun log(message: String) {
        println(message)
    }
}

data class UiState(val items: List<String> = emptyList(), val loading: Boolean = false)
"#;

#[test]
fn view_model_outline() {
    let g = graph(VIEW_MODEL);
    assert_eq!(g.framework.as_deref(), Some("android"));
    assert_eq!(g.imports.get("Inject"), Some("javax.inject.Inject"));
    let vm = container(&g, "TodoViewModel");
    assert_eq!(vm.kind, "class");
    assert_eq!(vm.extends.as_deref(), Some(&["ViewModel".to_string()][..]));
    assert_eq!(vm.doc.as_deref(), Some("Holds the todo list state."));
    assert_eq!(container(&g, "UiState").kind, "data class");
    let names: Vec<&str> = g
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some("TodoViewModel"))
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, vec!["constructor", "load", "refresh", "log"]);
    assert_eq!(method(&g, "constructor").kind.as_deref(), Some("init"));
    assert_eq!(method(&g, "log").visibility.as_deref(), Some("private"));
    assert_eq!(method(&g, "load").visibility.as_deref(), Some("public"));
}

#[test]
fn view_model_properties() {
    let g = graph(VIEW_MODEL);
    let prop = |n: &str| g.properties.iter().find(|p| p.name == n).unwrap();
    assert_eq!(
        prop("repository").type_hint.as_deref(),
        Some("TodoRepository")
    );
    assert_eq!(prop("repository").visibility, "private");
    assert!(prop("repository").readonly);
    assert_eq!(
        prop("_state").type_hint.as_deref(),
        Some("MutableStateFlow")
    );
    assert_eq!(
        prop("state").type_hint.as_deref(),
        Some("StateFlow<UiState>")
    );
}

#[test]
fn view_model_calls_and_receivers() {
    let g = graph(VIEW_MODEL);
    let launch = call(&g, "viewModelScope.launch");
    assert_eq!(launch.category, "coroutines_api");
    assert_eq!(launch.caller.as_deref(), Some("load"));
    assert_eq!(launch.on.as_deref(), Some("viewModelScope"));
    let fetch = call(&g, "repository.fetchAll");
    assert_eq!(fetch.category, "external_call");
    assert_eq!(fetch.resolved.as_deref(), Some("TodoRepository::fetchAll"));
    assert_eq!(fetch.on.as_deref(), Some("repository"));
    let update = call(&g, "_state.update");
    assert_eq!(update.resolved.as_deref(), Some("MutableStateFlow::update"));
    assert_eq!(update.category, "coroutines_api");
    assert_eq!(call(&g, "log").category, "internal_helper");
    assert_eq!(call(&g, "load").category, "internal_helper");
    assert_eq!(call(&g, "println").category, "native_kotlin");
    assert_eq!(call(&g, "it.copy").category, "native_kotlin");
    assert_eq!(call(&g, "MutableStateFlow").category, "coroutines_api");
    assert_eq!(call(&g, "UiState").category, "instantiation");
    assert_eq!(call(&g, "UiState").caller.as_deref(), Some("__top__"));
}

#[test]
fn annotations_become_framework_calls() {
    let g = graph(VIEW_MODEL);
    let inject = call(&g, "@Inject");
    assert_eq!(
        (
            inject.category.as_str(),
            inject.kind.as_str(),
            inject.caller.as_deref()
        ),
        ("framework_use", "annotation", Some("constructor"))
    );
    let hilt = call(&g, "@HiltViewModel");
    assert_eq!(hilt.container.as_deref(), Some("TodoViewModel"));
}

#[test]
fn strings_and_comments() {
    let g = graph(VIEW_MODEL);
    assert!(
        g.strings.iter().any(|s| s.value == "loaded ${…}"),
        "{:?}",
        g.strings
    );
    let todo = &g.comments[0];
    assert_eq!(todo.tag.as_deref(), Some("TODO"));
}

const SCREEN: &str = r#"package com.example.app.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable

@Composable
fun TodoScreen(viewModel: TodoViewModel = hiltViewModel(), onOpen: (String) -> Unit) {
    val state by viewModel.state.collectAsState()
    var query by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { viewModel.load() }
    Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
        Text("Todos: ${state.items.size}")
        state.items.forEach { item -> TodoRow(item, onClick = { onOpen(item) }) }
        Button(onClick = viewModel::refresh) { Text("Refresh") }
    }
}

@Composable
private fun TodoRow(title: String, onClick: () -> Unit) {
    Text(title, modifier = Modifier.clickable(onClick = onClick))
}

@Composable
fun AppNav(navController: NavHostController = rememberNavController()) {
    NavHost(navController, startDestination = "home") {
        composable("home") { TodoScreen(onOpen = { navController.navigate("detail/$it") }) }
        composable("detail/{id}") { DetailScreen() }
    }
}
"#;

#[test]
fn compose_screen_outline() {
    let g = graph(SCREEN);
    assert_eq!(g.framework.as_deref(), Some("compose"));
    let module = &g.containers[0];
    assert_eq!(
        (module.kind.as_str(), module.name.as_str()),
        ("module", "Sample.kt")
    );
    let screen = method(&g, "TodoScreen");
    assert_eq!(screen.kind.as_deref(), Some("composable"));
    assert_eq!(screen.params.len(), 2);
    assert!(screen.params[0].starts_with("viewModel: TodoViewModel"));
    assert_eq!(method(&g, "TodoRow").visibility.as_deref(), Some("private"));
    assert_eq!(method(&g, "AppNav").kind.as_deref(), Some("composable"));
}

#[test]
fn compose_calls_are_categorized() {
    let g = graph(SCREEN);
    assert_eq!(
        call(&g, "viewModel.state.collectAsState").category,
        "compose_api"
    );
    assert_eq!(call(&g, "remember").category, "compose_api");
    assert_eq!(call(&g, "mutableStateOf").category, "compose_api");
    assert_eq!(call(&g, "LaunchedEffect").category, "compose_api");
    assert_eq!(call(&g, "Column").category, "compose_view");
    assert_eq!(call(&g, "Button").category, "compose_view");
    assert_eq!(call(&g, "Modifier.padding").category, "compose_modifier");
    assert_eq!(call(&g, "Modifier.clickable").category, "compose_modifier");
    let chained = call(&g, ".fillMaxWidth");
    assert_eq!(chained.category, "compose_modifier");
    assert_eq!(call(&g, "TodoRow").category, "component_use");
    assert_eq!(call(&g, "NavHost").category, "compose_api");
    // ismeretlen PascalCase hívás composable-kontextusban composable (a ViewModel `UiState(..)` hívása példányosítás)
    assert_eq!(call(&g, "DetailScreen").category, "component_use");
    assert_eq!(call(&g, "composable").category, "compose_api");
}

#[test]
fn compose_receivers_come_from_parameters() {
    let g = graph(SCREEN);
    let load = call(&g, "viewModel.load");
    assert_eq!(load.resolved.as_deref(), Some("TodoViewModel::load"));
    assert_eq!(load.caller.as_deref(), Some("TodoScreen"));
    let navigate = call(&g, "navController.navigate");
    assert_eq!(
        navigate.resolved.as_deref(),
        Some("NavHostController::navigate")
    );
}

#[test]
fn method_references_are_calls() {
    let g = graph(SCREEN);
    let reference = call(&g, "viewModel.refresh");
    assert_eq!(reference.kind, "reference");
    assert_eq!(
        reference.resolved.as_deref(),
        Some("TodoViewModel::refresh")
    );
    assert_eq!(reference.caller.as_deref(), Some("TodoScreen"));
    let g = graph(
        r#"class A {
    fun go() { listOf(1).forEach(::helper); run(this::local); items.map(Item::name) }
    fun local() {}
}
fun helper(x: Int) {}
"#,
    );
    assert_eq!(call(&g, "helper").kind, "reference");
    assert_eq!(call(&g, "helper").category, "internal_helper");
    let this_ref = call(&g, "this.local");
    assert_eq!(
        (this_ref.kind.as_str(), this_ref.on.as_deref()),
        ("reference", Some("this"))
    );
    assert_eq!(
        call(&g, "Item.name").resolved.as_deref(),
        Some("Item::name")
    );
}

const DAO: &str = r#"@Dao
interface TodoDao {
    @Query("SELECT * FROM todo ORDER BY id")
    fun observeAll(): Flow<List<TodoEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(item: TodoEntity)
}

@Entity(tableName = "todo")
data class TodoEntity(@PrimaryKey val id: Long, val title: String)
"#;

#[test]
fn room_dao_and_entity() {
    let g = graph(DAO);
    let dao = container(&g, "TodoDao");
    assert_eq!(
        (dao.kind.as_str(), dao.is_abstract),
        ("interface", Some(true))
    );
    let observe = method(&g, "observeAll");
    assert!(observe.is_abstract);
    assert_eq!(
        observe.return_type.as_deref(),
        Some("Flow<List<TodoEntity>>")
    );
    assert_eq!(method(&g, "insert").kind.as_deref(), Some("suspend"));
    assert_eq!(call(&g, "@Query").caller.as_deref(), Some("observeAll"));
    assert_eq!(
        call(&g, "@Query").args.as_deref(),
        Some(&["\"SELECT * FROM todo ORDER BY id\"".to_string()][..])
    );
    assert_eq!(call(&g, "@Insert").category, "framework_use");
    assert_eq!(call(&g, "@Dao").caller.as_deref(), Some("__top__"));
    assert_eq!(call(&g, "@Entity").container.as_deref(), Some("TodoEntity"));
    let entity = container(&g, "TodoEntity");
    assert_eq!(entity.kind, "data class");
    let id = g.properties.iter().find(|p| p.name == "id").unwrap();
    assert_eq!(id.type_hint.as_deref(), Some("@PrimaryKey Long"));
    assert!(id.readonly);
}

const MISC: &str = r#"const val MAX_ITEMS = 50

fun String.slug(): String = lowercase().replace(" ", "-")

enum class Mode { LIGHT, DARK }

object Registry {
    val cache = mutableMapOf<String, Int>()
}

class Home : ComponentActivity() {
    companion object {
        const val TAG = "Home"
        fun newIntent(ctx: Context) = Intent(ctx, Home::class.java)
    }

    private val vm by viewModels<HomeViewModel>()
    val name by lazy { "x" }
    var count: Int = 0
        private set

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        if (count > 1) {
            Log.d(TAG, "many")
        } else if (count == 0) {
            finish()
        } else {
            count++
        }
        when (count) {
            0 -> finish()
            else -> Toast.makeText(this, "x", 1).show()
        }
        vm.load()
    }
}
"#;

#[test]
fn top_level_constants_extensions_and_objects() {
    let g = graph(MISC);
    let max = &g.constants[0];
    assert_eq!(
        (
            max.name.as_str(),
            max.kind.as_str(),
            max.value.as_deref(),
            max.container.as_deref()
        ),
        ("MAX_ITEMS", "const", Some("50"), Some("Sample.kt"))
    );
    let slug = method(&g, "slug");
    assert_eq!(slug.kind.as_deref(), Some("extension"));
    assert_eq!(slug.params[0], "receiver: String");
    assert_eq!(container(&g, "Mode").kind, "enum");
    let cases: Vec<&str> = g
        .constants
        .iter()
        .filter(|c| c.kind == "enum_case")
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(cases, vec!["LIGHT", "DARK"]);
    assert_eq!(container(&g, "Registry").kind, "object");
    assert_eq!(
        container(&g, "Home").extends.as_deref(),
        Some(&["ComponentActivity".to_string()][..])
    );
    assert_eq!(container(&g, "Home.Companion").kind, "companion object");
    assert!(
        g.constants
            .iter()
            .any(|c| c.name == "TAG" && c.container.as_deref() == Some("Home.Companion"))
    );
    assert!(
        g.methods
            .iter()
            .any(|m| m.name == "newIntent" && m.container.as_deref() == Some("Home.Companion"))
    );
}

#[test]
fn delegated_properties_and_private_set() {
    let g = graph(MISC);
    let prop = |n: &str| g.properties.iter().find(|p| p.name == n).unwrap();
    assert_eq!(
        prop("vm").type_hint.as_deref(),
        Some("by viewModels HomeViewModel")
    );
    assert_eq!(prop("name").type_hint.as_deref(), Some("by lazy"));
    assert_eq!(
        prop("count").type_hint.as_deref(),
        Some("Int (private set)")
    );
    assert!(!prop("count").readonly);
}

#[test]
fn android_calls_and_flow() {
    let g = graph(MISC);
    let log = call(&g, "Log.d");
    assert_eq!(log.category, "android_api");
    assert_eq!(log.resolved.as_deref(), Some("Log::d"));
    assert_eq!(call(&g, "finish").category, "android_api");
    assert_eq!(call(&g, "Toast.makeText").category, "android_api");
    assert_eq!(
        call(&g, "vm.load").resolved.as_deref(),
        Some("HomeViewModel::load")
    );
    assert_eq!(call(&g, "super.onCreate").on.as_deref(), Some("this"));
    let iff = g.control_flow.iter().find(|f| f.kind == "if").unwrap();
    let labels: Vec<&str> = iff.branches.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["if (count > 1)", "elseif (count == 0)", "else"]
    );
    let when = g.control_flow.iter().find(|f| f.kind == "when").unwrap();
    assert_eq!(when.cond.as_deref(), Some("count"));
    assert_eq!(when.branches.len(), 2);
    assert_eq!(when.branches[1].label, "default");
}

#[test]
fn missing_fields_are_optional() {
    // üres fájl és csak komment: nincs container, nincs hiba
    let g = graph("// just a note about nothing in particular\n");
    assert!(g.containers.is_empty() && g.methods.is_empty());
}
