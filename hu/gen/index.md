> PHP, Vue, JS / TS modul, HTML-oldal és teljes Laravel-funkció egy rövid kifejezésből, a projekt stílusában — szintaxis-ellenőrizve, mentéssel, visszavonhatóan. Route, osztály és metódus átnevezése az egész projektben.

Source: https://callgraph.eu/hu/gen/

# Generátor és rename: kód írása és átnevezése biztonságosan

A `gen` egy rövid, Emmet-szerű kifejezésből valódi kódot ír — PHP-osztályt, Vue-komponenst, JS / TS modult, HTML-oldalt, egy teljes Laravel-funkciót — a projekted stílusában. A `rename` egy route-, osztály- vagy metódusnevet cserél ki mindenhol, ahol használják. Mindkettő írás előtt ellenőrzi a szintaxist, és egyetlen paranccsal visszavonható.

[Telepítés](https://callgraph.eu/hu/#install) [Laravel & Inertia](https://callgraph.eu/hu/laravel/)

## PHP: osztályok és tagok

A kifejezés megmondja, mi az osztály, mit örököl és mit valósít meg, és mik a tagjai: `+` public, `-` private, `#` protected. A névtér a `composer.json` `psr-4` részéből jön, az importok a projektből. Azokat a neveket, amelyeket a callgraph nem tud importálni, `unresolved: Foo` formában jelzi.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'c!:PaymentGateway<BaseGateway>[Chargeable] > ctor(-Client $http, -int $retries=3) + +charge(Order $order):Receipt + -sign(array $p):string' --write app/Services/PaymentGateway.php
```

Cél nélkül, vagy `--dry`-jal csak kiírja a kódot. A fenti osztály így készül el:

Generált kód

```
<?php

declare(strict_types=1);

namespace App\Services;

final class PaymentGateway extends BaseGateway implements Chargeable
{
    public function __construct(
        private Client $http,
        private int $retries = 3,
    ) {}

    public function charge(Order $order): Receipt
    {
        throw new \LogicException('TODO');
    }

    private function sign(array $p): string
    {
        throw new \LogicException('TODO');
    }
}
```

Tag hozzáadása egy meglévő osztályhoz, egy adott metódus után:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen '+refund(Order $o):void{ $this->http->post($o->id); }' --into app/Services/PaymentGateway.php::PaymentGateway --after charge
```

## Vue: komponensek és composable-ök

A `vue:Név>(script>…)+(template>…)+(style>…)` egyfájlos komponenst ír TypeScriptben vagy JavaScriptben, a projekt szerint. A scriptben használható a `props(...)`, `emits(...)`, `ref:`, `computed:`, `fn:`, a `use:` import és más. A template Emmet-szerű elemeket használ, a komponenseket magától importálja. A `comp:useX>…` composable-t ír.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'vue:Counter>(script>ref:n:int=0+fn:inc(){ n.value++ })+(template>button[@click=inc]{Add})' --dry
```

Generált kód

```
<script setup lang="ts">
import { ref } from 'vue'

const n = ref<number>(0)

function inc() {
  n.value++
}
</script>

<template>
  <button @click="inc">Add</button>
</template>
```

Új oldal helyett egy meglévőt is bővíthetsz:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'script>ref:loading:bool=false+fn:reset(){ form.reset() }' --into resources/js/Pages/Auth/Register.vue
```

## Sima JS / TS modulok

A `mod:` modult, a `class:` osztályt ír. Alapból minden exportált; a `-fn:` privát függvényt ad, a `!fn:` / `!class:` `export default`-ot. A `.ts` fájl típusokat kap, a `.js` fájl JSDoc-ot.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'mod:util>fn:join(p:list<str>):str{ return p.join("/") }+const:MAX:int=10' --write resources/js/util.ts
```

Generált kód

```
export function join(p: string[]): string {
  return p.join("/")
}

export const MAX: number = 10
```

Az `--into x.ts[::Osztály]` tagokat szúr egy meglévő fájlba; a `--replace x.ts::fn` vagy `x.ts::Osztály::metódus` egyet lecserél.

## HTML-dokumentumok

A `html:Cím[lang=hu]>(head>…)+(body>…)` teljes dokumentumot ír: doctype, charset, viewport és title. A head-rövidítések: `meta:description=…`, `link:style.css`, `script:app.js` és `base:/app/`. Bármilyen elem és attribútum használható; az inline `style{…}`, `script{…}` és `comment{…}` is megy, a CSS-t és a JS-t írás előtt ellenőrzi.

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'html:Title[lang=hu]>(head>meta:description=Hi+link:style.css+script:app.js)+(body>header>nav>a[href=/]{Home}^^main>form[action=/x method=post]>input[type=email name=e required]+button{Send})' --write index.html
```

Az `--into x.html` a `<body>` végére vagy a `::#id`, `::.class`, `::tag` elembe szúr; a `--replace x.html::#id` egy elemet cserél.

## Laravel-funkció egy lépésben

A `laravel:` preset együtt írja meg a FormRequesteket, a controller-metódusokat, a route-okat és az Inertia / Vue oldalakat:

```
bash ~/.claude/skills/callgraph/callgraph.sh gen 'laravel:Fee[auth,prefix=admin]>create(name:str,amount:int,paid?:bool)+edit+index'
```

A válasz (fájlonként egy sor)

```
írtam: app/Http/Requests/StoreFeeRequest.php (27 sor)
írtam: app/Http/Requests/UpdateFeeRequest.php (27 sor)
írtam: app/Http/Controllers/FeeController.php (48 sor)
routes/web.php: +5 route (admin.fees.index, admin.fees.create, admin.fees.store, +2) az auth csoportban
írtam: resources/js/Pages/Fees/Index.vue (51 sor)
írtam: resources/js/Pages/Fees/Create.vue (45 sor)
írtam: resources/js/Pages/Fees/Edit.vue (54 sor)
✓ form ↔ rules (2 form, 6 mező) · route-nevek (5)
⚠ a(z) Fee modell nincs meg (app/Models/Fee.php) — modellt, migrációt nem generáltam
visszavonás: a `callgraph undo` egy lépésben mindet visszaállítja
```

Ha a projektben nincs Ziggy, az oldalak `route('…')` helyett sima URL-eket (`'/admin/fees'`) használnak.

- Ellenőrzi, hogy az űrlap mezői egyeznek-e a validációs szabályokkal.

- Figyelmeztet az alapérték nélküli NOT NULL oszlopokra, amelyeket az űrlap nem küld.

- Egyetlen `undo` az összes fájlját visszavonja.

A kapcsolatokat, amelyekre épít (route-ok, űrlapok, modellek, oldalak), a [Laravel & Inertia](https://callgraph.eu/hu/laravel/) oldal írja le.

## Hová kerül a kód

- `--write <fájl>`: csak új fájl. Meglévőt soha nem ír felül.

- `--into <fájl>[::Osztály] [--after <metódus>]`: hozzáadás egy meglévő fájlhoz vagy osztályhoz.

- `--replace <fájl>:L<a>-L<b>` vagy `--replace <fájl>::<metódus>`: egy sortartomány vagy egy tag cseréje.

- `--expect <sha256-előtag>` a `--replace` mellett: elutasítja a cserét, ha közben megváltoztak azok a sorok.

- `--dry`, vagy cél nélkül: csak kiírja a kódot.

A válasz fájlonként egy sor, így az agentnek szinte nem kerül tokenbe elolvasni.

## Biztonság: ellenőrzés, mentés, undo

- Az új szöveg szintaxisát írás előtt ellenőrzi; hibánál semmi nem íródik.

- Egy meglévő fájl minden módosítása előtt mentés készül a `.callgraph/backups/` mappába (fájlonként a legutóbbi 10).

- Az `undo [<fájl>]` visszaállítja a legfrissebb mentést; maga az `undo` is visszavonható.

```
bash ~/.claude/skills/callgraph/callgraph.sh undo
bash ~/.claude/skills/callgraph/callgraph.sh undo app/Services/PaymentGateway.php
```

## Route, osztály vagy metódus átnevezése

A `rename route|class|method <régi> <új>` mindenhol átírja a nevet PHP-ban, Blade-ben, JS-ben és Vue-ban:

- **route**: `->name()`, csoportok, `'profile.*'` minták, resource route-ok `->names()`-szel, és minden `route('x')` hívás.

- **class**: a deklaráció, a fájl áthelyezése, névtér, `use` sorok, típusjelzések és docblockok. A `--strings` a sztring-hivatkozásokat is átírja; a `--keep-table` megtartja az Eloquent táblanevet.

- **method**: felülírások, feloldott hívások, callable tömbök és route-akciók.

```
bash ~/.claude/skills/callgraph/callgraph.sh rename route events.show events.view --dry
bash ~/.claude/skills/callgraph/callgraph.sh rename route events.show events.view
```

Minden módosított fájl szintaxisát előbb ellenőrzi, a `--dry` csak jelent, a válasz fájlonként egy sor. A dinamikus hivatkozásokat (futásidőben összeállított neveket) csak listázza, nem találgat. Egyetlen `undo` a teljes átnevezést visszavonja.

## MCP-n át: opt-in

Az MCP `generate` és `rename` eszköz fájlokat ír, ezért alapból ki van kapcsolva. Csak akkor szerepel az eszközlistában, ha a szerver `--generate`-tel vagy `CALLGRAPH_GENERATE=1`-gyel fut; nélküle az eszközlista pontosan a korábbi. A csak olvasó eszközökkel ellentétben `readOnlyHint: false` jelzést adnak.

```
claude mcp add -s user callgraph -- ~/.claude/skills/callgraph/bin/$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update --generate
```

A többi kliens beállítása: [MCP-szerver](https://callgraph.eu/hu/mcp/).

## Kapcsolódó oldalak

[

### Laravel & Inertia

Route-ok, űrlapok, modellek és oldalak összekötve az outline-ban.

](https://callgraph.eu/hu/laravel/) [

### TypeScript / Vue

Vue-komponensek és JS / TS modulok outline-ja.

](https://callgraph.eu/hu/typescript/) [

### PHP, Laravel és Yii

Osztályok outline-ja és minden hívó egy átnevezés előtt.

](https://callgraph.eu/hu/php/) [

### MCP-szerver

Az opt-in `generate` és `rename` eszköz.

](https://callgraph.eu/hu/mcp/)
