> PHP-osztályok, metódusok és kategorizált hívások, minden hívó és Laravel / Yii nézetek (routes, validation, events) a fájlok beolvasása nélkül.

Source: https://callgraph.eu/hu/php/

# PHP call graph és hatáselemzés AI agenteknek

A callgraph a szintaxisfából olvassa a `.php` fájlokat, és megmondja az agentnek, mi van egy osztályban, melyik metódus melyik sorokon él, mit hív — kategóriánként —, és ki hívja az egész projektben. Laravel- és Yii-ismerettel, a teljes fájlok beolvasása nélkül.

[Telepítés](https://callgraph.eu/hu/#install) [Használat MCP-n át](https://callgraph.eu/hu/mcp/)

## Mit nyer ki a callgraph a PHP-kódból

- Osztályok, traitek és enumok, az `extends`-szel és a névtérrel.

- Metódusok pontos sortartománnyal, property-k, konstansok és docblockok.

- Hívások típuskövetéssel, mindegyik kategória-címkével.

- String-literálok és `if` / `match` / `switch` ágak (`--format flow`).

- Framework-felismerés: Laravel és Yii.

A PHP-outline hívás-kategóriái között van a `native_php`, `orm_relation`, `model_use`, `global_helper`, `internal_helper`, `external_call` és a `framework_use` — az agent a szándékot látja, nem csak neveket. Szűrés rájuk: `--category model_use,orm_relation`.

## Egy modell outline-ja

Egy parancs a fájl beolvasása helyett:

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app/Models/Order.php
```

Kimenet

```
FILE  /srv/shop/app/Models/Order.php  NS:App\Models

class Order extends Illuminate\Database\Eloquent\Model  L7-23
  constants (1):
    STATUS_OPEN = 'open'
  properties (1):
    mixed $fillable
  saveTranslations()  L13-20
    external_call     create, info
    global_helper     logger
    internal_helper   translations
  translations()  L22-22
    orm_relation      hasMany
```

Utána csak a szükséges metódust olvasd, sorszámozva (a fölötte lévő docblock és az üres sorok kimaradnak, a törzsbeli kommentek maradnak; a `--doc` a docblockot is hozza, a `--raw` a pontos forrást):

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app/Models/Order.php::save --body
```

## Ki hívja ezt a PHP-metódust?

A név szerinti mintával kezdd — ez minden hívást megtalál, akkor is, ha a fogadó típusa ismeretlen, örökölt vagy traitből jön. A `Típus::metódus` csak az adott típusra feloldott hívásokat adja, ezért egy gyakori név (`save`, `find`) szűkítésére való:

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep '(^|::|\.|->)cancel$'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep 'Order::cancel'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --grep 'Order::cancel' --depth 3
```

A `--depth N` a hívók hívóit is bejárja hatáselemzéshez:

```
$ callgraph.sh --project shop app --grep saveTranslations --depth 2
== 1. szint — közvetlenül hívja: /saveTranslations/
  /srv/shop/app/Models/Order.php  Caller::run()  L25-25

== 2. szint — az előző szintet hívja
  (nincs további hívó)
```

- Minden találat mellett ott a hívás saját forrássora, pl. `L25 │ $this->repo->find($id)`, így az agent újabb olvasás nélkül látja az argumentumokat.

- Az `Osztály::metódus` keresés az interfészén, absztrakt szülőjén vagy traitjén át tett hívásokat is megtalálja, `via Resolver` jelöléssel.

- A gyerekosztályok `$this->metódus()` alakú hívásai is előkerülnek, a gyerekosztállyal jelölve (`L139 via EventeditController │ …`). Ha a gyerekosztálynak saját változata van, az nem számít.

- A `--guess` a csak név szerinti egyezéseket is hozza, `?` jellel.

## Laravel- és Yii-nézetek

A framework-nézetek egy mappára futnak, és mindegyik egy célzott listát ad: `routes`, `middleware`, `validation`, `jobs`, `events`, `providers`, `magic`, `di` és `views`.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop routes --format routes
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --format validation
```

MCP-n ugyanezek a `framework_view` eszközzel érhetők el. Ismeretlen Laravel- vagy Yii-projektben a [`profile`](https://callgraph.eu/hu/profile/) előbb a rétegzett MVC-felépítést mutatja meg: melyik mappában vannak a controllerek, service-ek, repository-k és modellek.

## Stringek és konstansok

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --strings 'Order not found'
bash ~/.claude/skills/callgraph/callgraph.sh --project shop app --const STATUS_ACTIVE
```

A `--const` a definíciót, a hivatkozásokat és a továbbexportálásokat (pl. `use const App\NAME;`) is listázza, `[re-export]` jelöléssel.

## Korlátok

Ez statikus elemzés: a dinamikus hívások (`$this->$method()`), a magic metódusok és a futásidőben összerakott osztálynevek nem oldhatók fel. A `--depth` alapból csak a feloldott hívókat követi; a `--guess` a csak név szerintieket is mutatja. A pontos vezérlési logikához olvasd a metódust a `--body`-val.

## Kapcsolódó oldalak

[

### locate

Rangsorolt fájlok a feladat kezdéséhez.

](https://callgraph.eu/hu/locate/) [

### Projekt-profil

Egy Laravel- / Yii-projekt rétegei és mintái.

](https://callgraph.eu/hu/profile/) [

### MCP-szerver

Ugyanezek a lekérdezések Claude Desktopban, Cursorban, VS Code-ban.

](https://callgraph.eu/hu/mcp/) [

### TypeScript / Vue

Egy Laravel + Vue alkalmazás frontend-oldala.

](https://callgraph.eu/hu/typescript/)
