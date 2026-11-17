# callgraph — Kotlin / Android

`CG=<skill>/callgraph.sh`, as in [SKILL.md](../SKILL.md) (the core: start there).

## Kotlin / Android

Same switches (`.kt`, `.kts`). Container: `class` / `data class` / `sealed class` / `interface` / `enum` / `object`, `companion object`
(`Outer.Companion`), nested types; top-level functions go to the `<File>.kt` `module` container. Extends vs implements: a supertype
with a constructor call (`: ViewModel()`) is `extends`, the rest `implements`. "Methods": `fun` (`method` in a type, `function` at file level),
`suspend`, extension (`fun String.slug()`, the receiver is the first parameter), `composable` (`@Composable`), `init`, `computed` property;
visibility `public` (default) / `private` / `internal` / `protected`. Properties: `val` / `var`, constructor `val` / `var`, delegated
(`by viewModels<X>()`, `by lazy`); constants: `const val`, enum entries. Branches: `if` / `when`. The framework is `compose`, `android`
or `kotlin` (`*.gradle.kts` = `gradle`, not app code).
Call categories (first matching rule): `internal_helper`, `component_use` (a `@Composable`), `instantiation`, `compose_view` /
`compose_modifier` / `compose_api` (`Column`, `Modifier.padding`, `remember`, `LaunchedEffect`, `NavHost`), `coroutines_api`
(`launch`, `withContext`, `flow`, `stateIn`…), `android_api` (`Log.d`, `Intent`…), `native_kotlin` (stdlib), `global_helper`,
`framework_use` (annotations: `@Inject`, `@Dao`, `@Query`…), `external_call`.
Edges beyond plain calls: the receiver type comes from constructor parameters, properties, parameters and local `val`s
(`private val repo: TodoRepository` → `repo.load()` is `TodoRepository::load`; `?.` and `!!` do not break it); `inject`
(`hiltViewModel<X>()`, `by viewModels<X>()`, `@Inject constructor`, `@Inject lateinit var`); `provide` (`@Provides`, `@Binds`);
`nav` (`composable("detail/{id}") { DetailScreen() }`, `navController.navigate("detail/$id")` and type-safe routes link to
the screen, even across files); `(ref)` for references (`viewModel::refresh`, `::helper`; not real calls); interface ↔ implementation callers (`via`).
`profile` layers: `view` (Activity / Fragment / `@Composable`), `viewmodel`, `repository` (`@Dao`), `model` (`@Entity`), `api` (Retrofit),
`provider` (`@Module`, `di/`), `route`; `build.gradle(.kts)` marks the project root.
Limits: no type resolution across files (lambda receivers `it` / `this` stay `external_call` / bare names), infix calls (`a to b`) and operator
overloads are not call edges, anonymous `object : Iface {}` methods are not separate outline items, no `typealias`, no `dynamic_call` / property writes.
Details: `docs/LANG-KOTLIN.md`.
