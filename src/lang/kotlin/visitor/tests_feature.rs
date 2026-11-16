//! Egy valósághű, többfájlos Android-feature (`feature/todo`): képernyő + navigáció, ViewModel, repository
//! (interfész + implementáció), Room DAO és Hilt-modul — vázlat, inject / provide / nav élek és hívó-index.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::kotlin::KotlinFrontend;
use crate::model::{Call, FileGraph};

const SCREEN_PATH: &str = "/app/feature/todo/TodoScreen.kt";
const VIEW_MODEL_PATH: &str = "/app/feature/todo/TodoViewModel.kt";
const REPOSITORY_PATH: &str = "/app/feature/todo/TodoRepository.kt";
const DAO_PATH: &str = "/app/feature/todo/TodoDao.kt";
const MODULE_PATH: &str = "/app/feature/todo/di/TodoModule.kt";

const SCREEN: &str = r#"package com.example.feature.todo

import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.composable

@Composable
fun TodoScreen(viewModel: TodoViewModel = hiltViewModel(), onOpen: (Long) -> Unit) {
    val state by viewModel.state.collectAsState()
    LaunchedEffect(Unit) { viewModel.load() }
    LazyColumn {
        items(state.items) { item ->
            Text(item.title, modifier = Modifier.clickable { onOpen(item.id) })
        }
    }
    Button(onClick = viewModel::refresh) { Text("Refresh") }
}

fun NavGraphBuilder.todoGraph(navController: NavHostController) {
    composable("todos") { TodoScreen(onOpen = { id -> navController.navigate("todo/$id") }) }
    composable("todo/{id}") { TodoDetailScreen() }
}
"#;

const VIEW_MODEL: &str = r#"package com.example.feature.todo

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

@HiltViewModel
class TodoViewModel @Inject constructor(
    private val repository: TodoRepository,
) : ViewModel() {
    private val _state = MutableStateFlow(TodoUiState())
    val state: StateFlow<TodoUiState> = _state.asStateFlow()

    fun load() {
        viewModelScope.launch {
            val items = repository.fetchAll()
            _state.update { it.copy(items = items) }
        }
    }

    fun refresh() = load()

    fun toggle(id: Long) {
        viewModelScope.launch { repository.toggle(id) }
    }
}

data class TodoUiState(val items: List<Todo> = emptyList(), val loading: Boolean = false)
"#;

const REPOSITORY: &str = r#"package com.example.feature.todo

import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

interface TodoRepository {
    suspend fun fetchAll(): List<Todo>
    suspend fun toggle(id: Long)
}

class TodoRepositoryImpl @Inject constructor(
    private val dao: TodoDao,
    private val api: TodoApi,
) : TodoRepository {
    override suspend fun fetchAll(): List<Todo> = withContext(Dispatchers.IO) {
        dao.getAll().map { it.toModel() }
    }

    override suspend fun toggle(id: Long) {
        dao.toggle(id)
        api.sync(id)
    }
}
"#;

const DAO: &str = r#"package com.example.feature.todo

import androidx.room.Dao
import androidx.room.Entity
import androidx.room.Insert
import androidx.room.PrimaryKey
import androidx.room.Query

@Dao
interface TodoDao {
    @Query("SELECT * FROM todo ORDER BY id")
    suspend fun getAll(): List<TodoEntity>

    @Query("UPDATE todo SET done = NOT done WHERE id = :id")
    suspend fun toggle(id: Long)

    @Insert
    suspend fun insert(item: TodoEntity)
}

@Entity(tableName = "todo")
data class TodoEntity(@PrimaryKey val id: Long, val title: String, val done: Boolean = false)
"#;

const MODULE: &str = r#"package com.example.feature.todo.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

@Module
@InstallIn(SingletonComponent::class)
abstract class TodoModule {
    @Binds
    abstract fun bindRepository(impl: TodoRepositoryImpl): TodoRepository

    companion object {
        @Provides
        @Singleton
        fun provideDao(db: AppDatabase): TodoDao = db.todoDao()
    }
}
"#;

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

/// A container metódusainak nevei sorrendben.
fn names_in<'g>(g: &'g FileGraph, container: &str) -> Vec<&'g str> {
    g.methods
        .iter()
        .filter(|m| m.container.as_deref() == Some(container))
        .map(|m| m.name.as_str())
        .collect()
}

#[test]
fn screen_outline_and_navigation() {
    let g = graph_of(SCREEN_PATH, SCREEN);
    assert_eq!(g.framework.as_deref(), Some("compose"));
    let screen = g.methods.iter().find(|m| m.name == "TodoScreen").unwrap();
    assert_eq!(screen.kind.as_deref(), Some("composable"));
    assert!(g.methods.iter().any(|m| m.name == "todoGraph"));
    // a `composable("útvonal") { Képernyő() }` párok nav élek az útvonallal
    let routed: Vec<(&str, &str)> = g
        .calls
        .iter()
        .filter(|c| c.kind == "nav")
        .filter_map(|c| Some((c.target.as_str(), c.args.as_ref()?.first()?.as_str())))
        .collect();
    assert!(routed.contains(&("TodoScreen", "todos")), "{routed:?}");
    assert!(
        routed.contains(&("TodoDetailScreen", "todo/{id}")),
        "{routed:?}"
    );
    // a `navigate("todo/$id")` a paraméteres útvonal képernyőjére mutat: az útvonal-pár mellett egy második nav él
    let to_detail = g
        .calls
        .iter()
        .filter(|c| c.kind == "nav" && c.target == "TodoDetailScreen")
        .count();
    assert_eq!(to_detail, 2);
    assert!(g.calls.iter().any(|c| c.target == "navController.navigate"));
}

#[test]
fn screen_calls_reach_the_view_model() {
    let g = graph_of(SCREEN_PATH, SCREEN);
    assert_eq!(call(&g, "hiltViewModel(TodoViewModel)").kind, "inject");
    let load = call(&g, "viewModel.load");
    assert_eq!(load.resolved.as_deref(), Some("TodoViewModel::load"));
    assert_eq!(load.caller.as_deref(), Some("TodoScreen"));
    let refresh = call(&g, "viewModel.refresh");
    assert_eq!(refresh.kind, "reference");
    assert_eq!(refresh.resolved.as_deref(), Some("TodoViewModel::refresh"));
    assert_eq!(call(&g, "LazyColumn").category, "compose_view");
    assert_eq!(call(&g, "LaunchedEffect").category, "compose_api");
}

#[test]
fn view_model_outline_and_calls() {
    let g = graph_of(VIEW_MODEL_PATH, VIEW_MODEL);
    assert_eq!(
        names_in(&g, "TodoViewModel"),
        vec!["constructor", "load", "refresh", "toggle"]
    );
    let inject = call(&g, "@Inject(TodoRepository)");
    assert_eq!(
        (inject.kind.as_str(), inject.caller.as_deref()),
        ("inject", Some("constructor"))
    );
    let fetch = call(&g, "repository.fetchAll");
    assert_eq!(fetch.resolved.as_deref(), Some("TodoRepository::fetchAll"));
    assert_eq!(fetch.caller.as_deref(), Some("load"));
    assert_eq!(call(&g, "_state.asStateFlow").category, "coroutines_api");
    assert_eq!(call(&g, "viewModelScope.launch").category, "coroutines_api");
}

#[test]
fn repository_outline_has_interface_and_implementation() {
    let g = graph_of(REPOSITORY_PATH, REPOSITORY);
    let interface = g
        .containers
        .iter()
        .find(|c| c.name == "TodoRepository")
        .unwrap();
    assert_eq!(interface.kind, "interface");
    assert_eq!(names_in(&g, "TodoRepository"), vec!["fetchAll", "toggle"]);
    let implementation = g
        .containers
        .iter()
        .find(|c| c.name == "TodoRepositoryImpl")
        .unwrap();
    assert_eq!(
        implementation.implements.as_deref(),
        Some(&["TodoRepository".to_string()][..])
    );
    assert_eq!(
        names_in(&g, "TodoRepositoryImpl"),
        vec!["constructor", "fetchAll", "toggle"]
    );
    assert_eq!(call(&g, "@Inject(TodoDao)").kind, "inject");
    assert_eq!(call(&g, "@Inject(TodoApi)").kind, "inject");
    assert_eq!(call(&g, "withContext").category, "coroutines_api");
    let dao = call(&g, "dao.getAll");
    assert_eq!(dao.resolved.as_deref(), Some("TodoDao::getAll"));
    assert_eq!(dao.caller.as_deref(), Some("fetchAll"));
}

#[test]
fn dao_outline_and_room_annotations() {
    let g = graph_of(DAO_PATH, DAO);
    let dao = g.containers.iter().find(|c| c.name == "TodoDao").unwrap();
    assert_eq!(
        (dao.kind.as_str(), dao.is_abstract),
        ("interface", Some(true))
    );
    assert_eq!(names_in(&g, "TodoDao"), vec!["getAll", "toggle", "insert"]);
    let entity = g
        .containers
        .iter()
        .find(|c| c.name == "TodoEntity")
        .unwrap();
    assert_eq!(entity.kind, "data class");
    assert_eq!(call(&g, "@Dao").category, "framework_use");
    assert_eq!(call(&g, "@Insert").caller.as_deref(), Some("insert"));
}

#[test]
fn module_provides_and_binds_edges() {
    let g = graph_of(MODULE_PATH, MODULE);
    let binds = call(&g, "@Binds(TodoRepository <- TodoRepositoryImpl)");
    assert_eq!(
        (
            binds.kind.as_str(),
            binds.container.as_deref(),
            binds.caller.as_deref()
        ),
        ("provide", Some("TodoModule"), Some("bindRepository"))
    );
    let provides = call(&g, "@Provides(TodoDao)");
    assert_eq!(
        (provides.kind.as_str(), provides.caller.as_deref()),
        ("provide", Some("provideDao"))
    );
    assert_eq!(
        call(&g, "db.todoDao").resolved.as_deref(),
        Some("AppDatabase::todoDao")
    );
}

#[test]
fn callers_follow_the_interface_and_the_implementation() {
    let screen = graph_of(SCREEN_PATH, SCREEN);
    let model = graph_of(VIEW_MODEL_PATH, VIEW_MODEL);
    let repository = graph_of(REPOSITORY_PATH, REPOSITORY);
    let dao = graph_of(DAO_PATH, DAO);
    let module = graph_of(MODULE_PATH, MODULE);
    let index = CallerIndex::build([
        (SCREEN_PATH, &screen),
        (VIEW_MODEL_PATH, &model),
        (REPOSITORY_PATH, &repository),
        (DAO_PATH, &dao),
        (MODULE_PATH, &module),
    ]);
    let callers_of = |pattern: &str| {
        let mut found: Vec<(String, Option<String>)> = index
            .callers(pattern, 1)
            .unwrap()
            .into_iter()
            .map(|r| (r.caller, r.via))
            .collect();
        found.sort();
        found
    };
    // az interfész-metódus hívója a ViewModel, az implementációnál `via` az interfésszel
    assert_eq!(
        callers_of("TodoRepository::fetchAll"),
        vec![("load".to_owned(), None)]
    );
    assert_eq!(
        callers_of("TodoRepositoryImpl::fetchAll"),
        vec![("load".to_owned(), Some("TodoRepository".to_owned()))]
    );
    // a DAO-t az implementáció hívja
    assert_eq!(
        callers_of("TodoDao::getAll"),
        vec![("fetchAll".to_owned(), None)]
    );
    // a ViewModel `load`-ját a képernyő hívja
    assert!(
        callers_of("TodoViewModel::load")
            .iter()
            .any(|(caller, _)| caller == "TodoScreen" || caller == "refresh"),
        "{:?}",
        callers_of("TodoViewModel::load")
    );
}
