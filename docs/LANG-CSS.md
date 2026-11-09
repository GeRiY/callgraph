# CSS és SCSS leképezése a FileGraph sémába

Ez a dokumentum az R3a `LanguageFrontend` kimenetének szerződése. A CSS és az
SCSS ugyanazokat a FileGraph mezőket tölti ki; ami az adott fájlban nincs jelen,
az üres tömb (`[]`), illetve ahol a séma skalárt kér, `null`.

## Közös alapok

- `file`: a bemeneti fájl abszolút útja.
- `framework`: `null`; a CSS/SCSS nem framework-detektáló backend.
- `namespace`: `null`; a modulnév nem nyelvi namespace.
- `containers`: pontosan egy `stylesheet` konténer, a teljes stíluslap. A neve a
  fájlneve kiterjesztés nélkül, `line: 1`, `endLine`: a fájl utolsó sora.
- `properties`, `writes`: `[]`. A deklarációk a metódus `body`-jában maradnak,
  nem objektummezőként.

A sortartományok 1-alapúak és inkluzívak. Egy elem `line`-ja a nyitó token sora
(`@mixin`, `@function`, szelektor vagy at-rule), az `endLine` a hozzá tartozó
záró `}` sora; egysoros blokk esetén ugyanaz a két érték. Komment önálló
elemének sora a komment kezdősora. A több soros `/* ... */` komment kezdő- és
zárósora közötti teljes tartományt nem szűkítjük a szöveg miatt.

## Mezők és döntések

### Konténerek és „metódusok”

Az egész fájl egy `stylesheet` container (`name: "theme"`). A `methods` mezőben
CSS-ben minden szabályblokk (például `.card, .panel:hover { ... }`) egy
`rule` metódus; a neve a szelektorlista forrás szerinti, whitespace-normalizált
szövege. Ez a döntés azért hasznos, mert a CSS-ben nincs deklarált függvény,
viszont a szabályblokk a legfontosabb végrehajtási egység, és a deklarációk,
beágyazott media/supports blokkok hozzá köthetők.

Az at-szabály is metódus, `kind: "at-rule"`, a neve a teljes fejléc: a blokkos
(`@media …`, `@supports …`, `@layer …`, `@keyframes …`, `@font-face`, és a
Tailwind v4 `@theme`, `@utility …`, `@custom-variant …`, `@variant …`) a záró
`}`-ig, a `;`-re végződő (`@import`, `@tailwind`, `@plugin`, `@source`,
`@config`, `@custom-variant`, `@layer`, `@charset`, `@namespace`, `@reference`)
a saját sorain; a szabályon belüli `@apply` nem metódus. A `line` a fejléc
első karakterének sora (az előtte álló komment és üres sorok nem számítanak).
Az outline a `rule` és `at-rule` neve után nem ír `()`-t.

SCSS-ben a szabályblokk ugyanígy `rule`, továbbá:

- `@mixin rounded($r: 4px)` → `mixin` metódus, `params: ["$r: 4px"]`;
- `@function rem($n)` → `function` metódus, a `@return` a body része;
- a `methods` `container` és `caller` mezője mindig a `stylesheet` neve,
  illetve a lexikális szülő metódus neve; legfelső szinten a caller `null`.

Az egyéb metódusmezők (`visibility`, `static`, `abstract`, `returnType`, `doc`)
CSS/SCSS-ben rendre `null`/`false`/`false`/`null`/`null`, ahol a konkrét
metódustípushoz nem értelmezhetők. A `kind` a `rule`/`mixin`/`function` típus,
és a `body` a blokk forrása; ha a frontend csak strukturális outline-t ad, a
body megengedetten `null`.

### Imports

Az `imports` kulcs-érték térkép:

- CSS `@import url("print.css")` → `"print.css": "print.css"`;
- SCSS `@use "tokens" as t` → `"t": "tokens"`;
- `@forward "tokens"` → a modul neve alias nélkül (`"tokens": "tokens"`).

CSS `url()` deklarációban nem import: az `url()` hívásként jelenik meg.
Azonos modul több előfordulása egy import-bejegyzésben egyesül.

### Calls

Minden hívás a közös `calls` alakot használja: `container`, `caller`, `line`,
`type`, `target`, opcionális `args`. CSS/SCSS esetén a `category` értékei:

- `include` — `@include button($brand)`;
- `extend` — `@extend %card`;
- `variable` — `var(--brand, blue)` vagy SCSS `$brand`-hivatkozás;
- `function` — `darken($brand, 10%)`, `map-get(...)`, `calc(...)`;
- `url` — `url("fonts/app.woff2")`.

Az `@debug` és `@warn` diagnosztikai direktíva `directive` típusú, rendre
`debug` és `warn` célú call; az `@return` a function body-jának része.

`target` a hívás neve (`button`, `%card`, `--brand`, `darken`, `url`),
`caller` a körülvevő rule/mixin/function neve. A CSS custom property
deklarációja nem hívás, csak a `var(--x)` olvasása az.

### Constants

`constants` deklarációs rekordok:

- SCSS `$brand: #2463eb` és `$spacing: (sm: 4px, md: 8px)` → `kind: "scss-variable"`;
- CSS/SCSS `--brand: #2463eb` → `kind: "custom-property"`;
- map értékénél egy rekord keletkezik a változó sorára, `value` a teljes map
  forrása; a map kulcsai nem külön konstansok.

Minden rekordban `name`, `line`, `value`, `kind`, `container`, `caller` van.
### Control flow

`controlFlow` rekord az SCSS `@if`, `@else if`, `@else`, `@each`, `@for`,
`@while`, valamint a CSS/SCSS `@media` és `@supports` számára. A `type` az
at-rule neve, a `line` a kezdő `@` sora, a `cond` az at-rule utáni fejléc,
`branches` pedig az ágak inkluzív `startLine`/`endLine` tartományaival. A
beágyazott media a lexikális callerhez tartozik. A `@else` külön branch,
nem külön control-flow rekord.

### Strings és comments

`strings` minden idézett CSS/SCSS stringet és minden selector/property értékben
szereplő string-literal értéket tartalmaz, a token sorával (`value`, `line`,
`container`, `caller`). Az idézőjelek nélkül tárolt szövegértékek nem stringek.
SCSS interpoláció (`#{$name}`) külön `interpolation` call, a benne levő
változó pedig `variable` call.

`comments` rekordjai: `container`, `caller`, `line`, `tag: null`, `text`.
Mind a `// ...`, mind a `/* ... */` komment bekerül; az utóbbihoz a komment
szövege kerül, nem a határoló.

## Rövid példa

```scss
$brand: #2463eb;
.button { color: var(--text); @include rounded(4px); }
```

Itt a container `button`-t tartalmazó `stylesheet`, a methods egy `rule`
`.button`-nal, a constants egy `$brand` rekorddal, a calls egy `variable`
`--text` és egy `include` `rounded` hívással, a strings üres. A sorok minden
rekordnál a fenti inkluzív, 1-alapú szabályt követik.
