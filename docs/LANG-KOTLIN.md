# LANG-KOTLIN — a Kotlin / Android frontend mezőtérképe

Modul: `src/lang/kotlin/` (`KotlinFrontend`, `LanguageRole::Frontend`, azonosító `kotlin`, kiterjesztés `.kt` és
`.kts`). Parser: `tree-sitter-kotlin-ng`; tree-sitter-típus csak a `parser/` és a `visitor/` alatt jelenik meg. A
hívás-kategorizálás parser-független (`categorize.rs`), az ismert nevek adatfájlja `known.rs`. A `kt` és a `kts` egy
nyelvcsalád a hívó-keresésben (`locate`), a `--body` tömör nézete a Kotlin megjegyzés- és nyers-string szintaxisát
ismeri.

## Fájl-szintű keretek
- **framework**: `compose` (ha van `androidx.compose.*` import), `android` (`android.*` / `androidx.*`), különben
  `kotlin`; a `*.gradle.kts` build-szkript `gradle` jelölést kap (ugyanúgy elemezzük, de nem alkalmazás-kód).
- **imports**: `import a.b.C` → `{"C": "a.b.C"}`, alias (`as D`) a kulcs. Az import csomagot / osztályt nevez meg,
  fájlútra nem oldódik fel.
- **Szintaktikai hiba**: a tree-sitter hibatűrő, a gráf így is elkészül; az első hiba sora figyelmeztetés, nem végzetes.

## Mit ismer fel
- **Containerek**: `class`, `data class`, `sealed class` / `interface`, `enum`, `object`, `interface`, `companion
  object` (`Külső.Companion`), beágyazott típus (`Külső.Belső`). Típuson kívüli függvény / konstans / utasítás esetén
  az első container `module`, neve a fájlnév (`Util.kt`). `typealias` nem container.
- **Öröklés**: konstruktor-hívásos őstípus (`: ViewModel()`) `extends`, a többi (interfész, ismert interfész)
  `implements`; ez adja az interfész ↔ implementáció hívó-összekötést.
- **Metódusok**: `fun` (típusban `method`, fájl-szinten `function`), `suspend`, `extension` (`fun String.slug()`, a
  fogadó az első paraméter), `composable` (`@Composable`), `init` (`constructor`, `init { }`), `computed` property,
  `top-level` (`.kts` szkript-utasítások). `visibility`: `public` (alap) / `private` / `internal` / `protected`.
- **Property-k**: `val` (readonly) / `var`, konstruktor `val` / `var` paraméterek, delegált property (`by
  viewModels<X>()` → `by viewModels X`, `by lazy`), `private set`. **Konstansok**: `const val`, enum-értékek.
- **Vezérlési ágak**: `if` / `else if` / `else`, `when` (alany + ágankénti feltétel, `else` = `default`).
- **Stringek, megjegyzések, doc** (`/** */` KDoc): mint a többi nyelvnél; a string-sablon kifejezései (`${…}`) hívásai
  is felkerülnek.

## Hívás-kategóriák (az első illeszkedő szabály dönt)
| Kategória | Mikor |
|---|---|
| `internal_helper` | a fájlban definiált függvény puszta hívása, vagy `this.f()` |
| `component_use` | `@Composable` függvény (fájlbeli, vagy ismeretlen PascalCase hívás composable törzsben) |
| `instantiation` | típus-konstruktor (fájlbeli típus, vagy ismeretlen típus nem composable-ben) |
| `compose_view` / `compose_modifier` / `compose_api` | Compose-nézet (`Column`, `Text`…), `Modifier` lánc (`padding`, `clickable`…), állapot / effekt (`remember`, `LaunchedEffect`, `collectAsState`, `NavHost`…) |
| `coroutines_api` | `launch`, `async`, `withContext`, `flow { }`, `emit`, `stateIn`, `combine`, `collectLatest`, `MutableStateFlow`… (puszta vagy tag-hívásként) |
| `android_api` | `Log.d`, `Toast.makeText`, `Intent`, ismert Android-metódusok (`finish`…) |
| `native_kotlin` | stdlib függvény / típus / gyakori metódus (`println`, `listOf`, `map`, `let`, `apply`, `with`, `to`…) |
| `global_helper` | ismeretlen puszta kisbetűs függvény |
| `framework_use` | annotációk (`@Inject`, `@Dao`, `@Query`…), `kind: annotation` |
| `external_call` | minden más tag-hívás (`repository.fetchAll()`) |

## Kapcsolatok (nem puszta hívások)
- **Fogadó-típus**: a tag-hívás `resolved` célja `Típus::név`, ha a fogadó típusa konstruktor-paraméterből, property-ből,
  paraméterből vagy helyi `val`-ból következik (`private val repo: TodoRepository` → `repo.load()` →
  `TodoRepository::load`). A safe call (`a?.b()`) és a `!!` (`a.b!!.c()`) nem szakítja meg az utat.
- **`inject`**: `hiltViewModel<X>()` / `viewModel()` / `by viewModels<X>()` (a típus a típus-argumentumból vagy a
  deklarált típusból), `@Inject constructor(..)` paraméterei és `@Inject lateinit var` (`@Inject(Típus)` célú él).
- **`provide`**: `@Provides` / `@Binds` metódus (`@Provides(Típus)`, `@Binds(Interfész <- Implementáció)`).
- **`nav`**: `composable("útvonal") { Képernyő() }` a képernyőre mutat (az argumentum az útvonal); a
  `navController.navigate("detail/$id")` a literál-előtag alapján (`detail/{id}`) ugyanarra a képernyőre is `nav`
  élet ad; típusbiztos útvonalnál (`composable<Detail>`, `navigate(Detail(1))`) a típusnév a párosítás.
- **Hivatkozás `(ref)`**: `viewModel::refresh`, `::helper`, `this::local`, `Item::name` — `reference` fajtájú hívás
  (nem tényleges hívás; a kimenet `(ref)`-fel jelöli az outline / grep / callers nézetben).
- **Interfész ↔ implementáció**: az interfész-metódus hívói az implementációnál, és fordítva is megjelennek (`via`).

## Navigáció
A `navigate("detail/$id")` / `navigate(Detail(1))` hívás `nav` élet kap a `composable("detail/{id}") { DetailScreen() }`
képernyőre akkor is, ha a NavHost másik fájlban van: a projekt-index a leghosszabb literál-előtag (típusbiztos
útvonalnál a típusnév) alapján párosít.

## Projekt-profil (rétegek)
A `profile` Kotlin / Android projektben a mappa-, név- és kódjelek alapján rétegez: `view` (Activity / Fragment /
`@Composable`, `ui/`, `screens/`), `viewmodel` (`ViewModel` leszármazott, `@HiltViewModel`, `viewmodel/`),
`repository` (`@Dao`, `data/`, `dao/`), `model` (`@Entity`, `entity/`), `api` (Retrofit-interfész: `@GET`…),
`provider` (`@Module`, `di/`), `route` (`navigation/`), `util`. Az `android` architektúra-minta a `view` + `viewmodel`
rétegekből áll. `build.gradle(.kts)` / `settings.gradle.kts` a projekt-gyökér jele.

## Ismert korlátok
- Nincs típus-feloldás és fájlok közötti szimbólum-tábla: a kategória a fájlbeli nevekből és az ismert listákból
  (`known.rs`) dől el; a lambda-fogadók (`it`, `this` a `let` / `apply` / `with` törzsében) típusa nem ismert, ezek
  hívása `external_call` / puszta név marad.
- Az infix hívás (`a to b`, `x shl 2`) és az operátor-túlterhelés (`a + b`) nem hívás-él.
- A névtelen objektum (`object : Iface { … }`) metódusai nem külön outline-elemek, a törzs hívásai a külső
  függvényhez tartoznak.
- A `typealias` nem jelenik meg az outline-ban.
- A dinamikus hívás-jelölés (`dynamic_call`) és a `writes` (property-írások) nincs.
