> Laravel + Inertia (Vue) projektekben a callgraph a route() hívásokat a controllerhez köti, az űrlapokat a validációval veti össze, összefoglalja a modelleket és táblákat, és megtalálja az Inertia-oldalakat — az outline-ban, fájlolvasás nélkül.

Source: https://callgraph.eu/hu/laravel/

# Laravel és Inertia: route-ok, űrlapok, modellek és oldalak összekötve

Egy Laravel + Inertia (Vue) projektben a lényeges tények több fájlban laknak: a route-tábla, a controller, a validációs szabályok, a migrációk, a Vue-oldal. A callgraph ezeket beolvassa helyetted, és a rövid kapcsolatokat annak az egy fájlnak az outline-jába teszi, amelyet az agent éppen néz.

[Telepítés](https://callgraph.eu/hu/#install) [Generátor & rename](https://callgraph.eu/hu/gen/)

## Csomagok: miből áll a projekt

A callgraph offline beolvassa a `composer.json`, a `package.json` és a lock-fájlokat (composer, npm, yarn, pnpm, bun). A `profile` ezután egy `stack:` sorban mutatja a fontosabb csomagokat a lockolt verziójukkal (lock nélkül a deklarált tartománnyal, jelölve). MCP-n át az indításkori profil ennek tömör változatát kapja.

```
bash ~/.claude/skills/callgraph/callgraph.sh profile --project shop
bash ~/.claude/skills/callgraph/callgraph.sh profile --project shop --packages
```

A `--packages` minden közvetlen composer / npm függőséget felsorol: telepített és deklarált verzió, dev jelzés.

- Az outline-ban a függőségbe menő hívások a csomag rövid neve alatt csoportosítva jelennek meg, pl. `guzzle: GuzzleHttp\Client::request`. A JSON-kimenetben van `package` mező, a `locate` pedig „csomag” indoklást ad.

- A `uses <csomag> [--calls]` (MCP `package_usage`) megmutatja, hol használja a kód a függőséget: API-szimbólumonként hányszor, és melyik fájlban először. A `(type)` hívás nélküli importot jelöl; a `--calls` minden hívássort felsorol.

- A `deps` (MCP `project_profile` `deps: true`-val) a valószínűleg fölösleges, a hiányzó és a csak dev függőségeket listázza, soronként egyet, rövid indoklással. Téves jelzést a `.callgraphignore`-ban a `deps-ignore <név>` sor némít.

```
bash ~/.claude/skills/callgraph/callgraph.sh uses guzzle --calls --project shop
bash ~/.claude/skills/callgraph/callgraph.sh deps --project shop
```

## Route-ok: a route('x')-től a controllerig

A callgraph a betöltött `routes/*.php` fájlokból építi fel a route-táblát: csoportok, resource-ok, nevek és az `/api` előtag. Egy Vue- vagy JS-fájl outline-jában a `route('events.show')` hívás mellett ott a metódus, az URI és a controller-akció — vagy `⚠ no route`, ha a név nem létezik. A controller-metódus cserébe `route:` sort kap.

Outline-sor (rövidítve)

```
route::events.update → PUT /events/{event} · EventController@update
```

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format ziggy
```

A `ziggy` nézet kilistázza az ismeretlen route-neveket (JS, Vue és PHP) és azokat a route-fájlokat, amelyeket sehonnan nem tölt be a projekt. Ha vannak dinamikus nevű route-ok, az ismeretlen neveket csak `?` jelöli, nem hibaként jelenti.

## Űrlap-ellenőrzés: mezők a validációval szemben

Egy Inertia `useForm({...})` űrlapot összevet annak a controller-akciónak a validációjával, ahová küld — FormRequest vagy inline `validate`. Az eredmény egy `form:` sor a küldő függvény és az akció alatt:

- a küldött, de nem validált mezők,

- a nem küldött kötelező mezők,

- vagy `N fields ✓`, ha minden egyezik.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format forms
```

A `forms` nézet mindet felsorolja, így egy átnevezett mező vagy egy elfelejtett szabály előbb kiderül, mint hogy egy felhasználó rátalál.

## Modellek: tábla, oszlopok és relációk

Egy Eloquent-modell alatt az outline `table:` sort (az oszlopok a migrációkból) és `fillable:` sort mutat. A relációs sorok mellett ott a cél-modell. A `⚠` sor jelzi, ha egy fillable attribútum vagy cast nem oszlop.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format models
```

A `models` nézet felsorolja a modelleket, a modell nélküli táblákat, és azt a táblát, amelyet két migráció is létrehoz törlés nélkül — egy friss `migrate` ezen elbukna.

## Inertia-oldalak: a render() mindkét vége

A PHP `Inertia::render('Board')` hívás az oldal fájljára mutat (`Pages/Board.vue`). Az oldal outline-ja `rendered by:` sort kap, így a Vue-fájlból látszik, melyik controller küldi.

```
bash ~/.claude/skills/callgraph/callgraph.sh --project shop . --format inertia
```

Az `inertia` nézet oldalanként felsorolja a render-helyeket és a props-okat. Jelöli a hiányzó oldalfájlt és azt a props-ot, amelyet az oldal `defineProps`-a nem deklarál, és listázza a sehol nem renderelt oldalakat.

## Az egyfájlos outline gyors marad

Egy fájl outline-ja csak a szükséges route-, migrációs és űrlapfájlokat olvassa be. Az eredményt a `.callgraph/laravel.bin` fájlban tartja (csak ha a `.callgraph/` mappa létezik), a fájlok módosítási ideje és mérete alapján ellenőrzi, magától újraépül, és nyugodtan törölhető. A route-fájlokat útvonal szerinti sorrendben olvassa, így egy controller `route:` sorai minden nézetben ugyanabban a sorrendben jönnek.

## Mit jelent ez az agentednek

Ahelyett, hogy a route-fájlt, a controllert, a FormRequestet és a migrációt nyitná meg a „mit küld ez az űrlap, és validált-e?” kérdéshez, az agent a Vue-oldal vagy a controller outline-ját olvassa, és pár sorban megkapja a választ. Ugyanezek a nézetek MCP-n át is működnek (`framework_view` és `project_profile`), lásd: [MCP-szerver](https://callgraph.eu/hu/mcp/).

Azt szeretnéd, hogy a callgraph egy lépésben megírja a hozzá illő request-et, controllert, route-okat és oldalakat? Lásd: [a Laravel-generátor](https://callgraph.eu/hu/gen/#laravel).

## Kapcsolódó oldalak

[

### PHP, Laravel és Yii

Osztályok, kategorizált hívások és a framework-nézetek.

](https://callgraph.eu/hu/php/) [

### TypeScript / Vue

A frontend-oldal: komponensek, props, emits.

](https://callgraph.eu/hu/typescript/) [

### Generátor & rename

Laravel-funkció egy lépésben, route átnevezése mindenhol.

](https://callgraph.eu/hu/gen/) [

### Projekt-profil

Egy projekt rétegei és a `stack:` sora.

](https://callgraph.eu/hu/profile/)
