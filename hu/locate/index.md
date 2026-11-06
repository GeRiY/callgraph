> A callgraph locate egy hívásban kapja a feladat neveit és fogalmait, és rangsorolja a először megnyitandó fájlokat, indoklással. Mikor jobb a grep.

Source: https://callgraph.eu/hu/locate/

# locate: honnan induljon egy kódolási feladat

Add át a `locate`-nek egy hívásban a feladat neveit és fogalmait. A projekt-térképből válaszol, a forrás beolvasása nélkül: rövid, rangsorolt lista azokról a fájlokról, amiket először érdemes megnyitni — a legjobbal kezdve, mindegyik az indoklásával.

[Telepítés](https://callgraph.eu/hu/#install) [A `locate` MCP-eszköz](https://callgraph.eu/hu/mcp/)

## Példa

Függvény-, osztály- és fájlnevek, oszlopnevek, útvonalak — mind egyszerre:

```
bash ~/.claude/skills/callgraph/callgraph.sh locate RefundController processRefund refund payment_status checkout.vue --project myapp
```

Kimenet (rövidítve)

```
locate: 5 kifejezés · 14 találat 6 fájlban · innen indulj ↓
1. app/Http/Controllers/RefundController.php  ★★★
   class RefundController  L6-16  ← "RefundController", "refund" (pontos class)
   store()  L14  hívja: RefundService::processRefund  ← "processRefund" (hívás)
2. app/Services/RefundService.php  ★★
   processRefund()  L6-9  hívók: 2 (RefundController::store, RefundJob::handle)  ← "processRefund", "refund" (pontos metódus)
…
```

Minden fájl mellett ott vannak az illeszkedő osztályok vagy metódusok, a sortartományuk és az egyező kifejezések, így az agent egyből a jó hely [outline](https://callgraph.eu/hu/#usage)-jára vagy `--body`-jára léphet.

## Hogyan rangsorol

- Az egyezés annál többet ér, minél erősebb: pontos definíció, aztán fájlnév, aztán egy név része (`refund` ~ `RefundJob`, `refunds_table`, egyes / többes szám), aztán string-literál, végül hívás.

- Az a fájl, amelyik több különböző kifejezésre illik, előrébb kerül, mint amelyik egyre sokszor.

- Alapból 5 fájl látszik; a `--all` minden találatot listáz.

- A végén a találat nélküli és a túl általános kifejezések.

**Tipp:** a nem angol szakmai szó mellé add meg az angol kódbeli nevét is (pl. a *visszatérítés* mellé a `refund`-ot).

## locate, grep vagy callgraph?

| A kérdés | Ezt használd |
|---|---|
| Pontos string, naplóüzenet vagy konfig-érték | grep — a sima szövegkeresés ebben a legjobb |
| Honnan induljak ezzel a feladattal? | `locate` |
| Definíciók, szerkezet, hívók, hatás | callgraph: outline, `--body`, `--grep`, `--depth` |

Egy feladat szokásos menete: `locate` → a legjobb fájlok outline-ja → `--body` csak a szükséges metódusra → `--grep` a hívóihoz.

## MCP-n át

A `locate` eszköz a `terms` paramétert (egy string vagy lista), egy opcionális `project` aliast és az `all: true`-t (minden találat) kapja. A szerver kapcsolódáskor küldött útmutatója azt kéri az agenttől, hogy ez legyen a feladat első hívása. [Az MCP-szerver beállítása](https://callgraph.eu/hu/mcp/).

## Kapcsolódó oldalak

[

### Projekt-profil

Ismeretlen projektben előbb ezt futtasd egyszer.

](https://callgraph.eu/hu/profile/) [

### PHP

Outline, hívók és Laravel- / Yii-nézetek.

](https://callgraph.eu/hu/php/) [

### TypeScript / Vue

Route-ok, i18n-kulcsok és komponensek mint keresőkulcsok.

](https://callgraph.eu/hu/typescript/) [

### Újdonság a v0.5.0-ban

A `locate` a 0.5.0-val érkezett.

](https://callgraph.eu/hu/changelog/#v0.5.0)
