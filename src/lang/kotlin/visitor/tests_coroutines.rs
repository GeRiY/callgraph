//! Coroutines / Flow-nehéz kód: `flow { emit() }`, `combine` + `stateIn`, `collectLatest`, `withContext`, `suspend`
//! láncok — kategóriák, fogadó-feloldás és hívó-index.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::kotlin::KotlinFrontend;
use crate::model::{Call, FileGraph};

const REPO_PATH: &str = "/app/feed/FeedRepository.kt";
const VM_PATH: &str = "/app/feed/FeedViewModel.kt";

const REPO: &str = r#"package com.example.feed

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext

class FeedRepository(
    private val api: FeedApi,
    private val dao: FeedDao,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    fun observe(): Flow<List<Post>> = flow {
        val cached = dao.getAll()
        emit(cached)
        val fresh = api.fetchPosts()
        dao.replace(fresh)
        emit(fresh)
    }.catch { emit(emptyList()) }.flowOn(dispatcher)

    suspend fun load(id: Long): Post = withContext(Dispatchers.IO) { api.fetchPost(id) }

    suspend fun loadAll(ids: List<Long>): List<Post> = coroutineScope {
        ids.map { async { load(it) } }.awaitAll()
    }
}
"#;

const VIEW_MODEL: &str = r#"package com.example.feed

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

class FeedViewModel(
    private val repo: FeedRepository,
    private val settings: SettingsRepository,
) : ViewModel() {
    private val query = MutableStateFlow("")

    val posts: StateFlow<List<Post>> = combine(repo.observe(), query) { posts, q ->
        posts.filter { it.title.contains(q) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    init {
        viewModelScope.launch {
            settings.theme.collectLatest { applyTheme(it) }
        }
    }

    fun open(id: Long) {
        viewModelScope.launch {
            val post = repo.load(id)
            log(post.title)
        }
    }

    private fun applyTheme(theme: Theme) {}

    private fun log(message: String) {}
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

#[test]
fn flow_builder_and_operators_are_coroutines_api() {
    let g = graph_of(REPO_PATH, REPO);
    let flow = call(&g, "flow");
    assert_eq!(
        (flow.category.as_str(), flow.caller.as_deref()),
        ("coroutines_api", Some("observe"))
    );
    assert_eq!(call(&g, "emit").category, "coroutines_api");
    assert_eq!(call(&g, "emit").caller.as_deref(), Some("observe"));
    // az operátor-lánc (`flow { }.catch { }.flowOn(..)`) a hívás-eredményen megy tovább
    assert_eq!(call(&g, ".catch").category, "coroutines_api");
    assert_eq!(call(&g, ".flowOn").category, "coroutines_api");
    assert_eq!(call(&g, "emptyList").category, "native_kotlin");
}

#[test]
fn suspend_chain_resolves_receivers_inside_lambdas() {
    let g = graph_of(REPO_PATH, REPO);
    let with_context = call(&g, "withContext");
    assert_eq!(with_context.category, "coroutines_api");
    assert_eq!(with_context.caller.as_deref(), Some("load"));
    let fetch = call(&g, "api.fetchPost");
    assert_eq!(fetch.resolved.as_deref(), Some("FeedApi::fetchPost"));
    assert_eq!(fetch.caller.as_deref(), Some("load"));
    assert_eq!(
        call(&g, "dao.getAll").resolved.as_deref(),
        Some("FeedDao::getAll")
    );
    assert_eq!(call(&g, "coroutineScope").category, "coroutines_api");
    assert_eq!(call(&g, "async").category, "coroutines_api");
    // a fájlban definiált `load` puszta hívása belső segéd, a lambda a `loadAll`-hoz tartozik
    let inner = call(&g, "load");
    assert_eq!(
        (inner.category.as_str(), inner.caller.as_deref()),
        ("internal_helper", Some("loadAll"))
    );
    let methods: Vec<(&str, Option<&str>)> = g
        .methods
        .iter()
        .filter(|m| m.container.as_deref() == Some("FeedRepository"))
        .map(|m| (m.name.as_str(), m.kind.as_deref()))
        .collect();
    assert!(methods.contains(&("load", Some("suspend"))), "{methods:?}");
    assert!(
        methods.contains(&("loadAll", Some("suspend"))),
        "{methods:?}"
    );
}

#[test]
fn view_model_flow_pipeline_is_categorized() {
    let g = graph_of(VM_PATH, VIEW_MODEL);
    assert_eq!(call(&g, "MutableStateFlow").category, "coroutines_api");
    assert_eq!(call(&g, "combine").category, "coroutines_api");
    let state_in = call(&g, ".stateIn");
    assert_eq!(state_in.category, "coroutines_api");
    // a property-inicializáló hívásai a `__top__` hívóhoz tartoznak
    assert_eq!(state_in.caller.as_deref(), Some("__top__"));
    let observe = call(&g, "repo.observe");
    assert_eq!(observe.resolved.as_deref(), Some("FeedRepository::observe"));
    assert_eq!(observe.caller.as_deref(), Some("__top__"));
    // `init { }` blokk: a törzs hívásai az `init` hívóhoz
    let collect = call(&g, "settings.theme.collectLatest");
    assert_eq!(
        (collect.category.as_str(), collect.caller.as_deref()),
        ("coroutines_api", Some("init"))
    );
    let theme = call(&g, "applyTheme");
    assert_eq!(
        (theme.category.as_str(), theme.caller.as_deref()),
        ("internal_helper", Some("init"))
    );
    assert_eq!(call(&g, "viewModelScope.launch").category, "coroutines_api");
}

#[test]
fn callers_resolve_across_files() {
    let repo = graph_of(REPO_PATH, REPO);
    let model = graph_of(VM_PATH, VIEW_MODEL);
    let index = CallerIndex::build([(REPO_PATH, &repo), (VM_PATH, &model)]);
    let callers_of = |pattern: &str| -> Vec<String> {
        let mut found: Vec<String> = index
            .callers(pattern, 1)
            .unwrap()
            .into_iter()
            .map(|r| r.caller)
            .collect();
        found.sort();
        found
    };
    // a `suspend fun load` hívója a ViewModel `open`-je (a lambdán át is)
    assert!(callers_of("FeedRepository::load").contains(&"open".to_owned()));
    assert_eq!(callers_of("FeedRepository::observe").len(), 1);
    assert_eq!(callers_of("FeedDao::getAll"), vec!["observe".to_owned()]);
}
