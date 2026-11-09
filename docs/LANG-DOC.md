# LanguageDocument: JSON és Markdown leképezés (R4a)

Ez a dokumentum rögzíti, hogyan lesz a JSON/JSONC és Markdown fájlból a közös
`FileGraph`. A dokumentum maga egyetlen `container`; a kimenetben a
`FileGraph` minden mezője szerepel, az alkalmazhatatlan mezők üres tömbök vagy
`null` értékek.

## Közös szabályok

- `file` a bemenet abszolút útja; `framework` és `namespace` `null`.
- A dokumentum-konténer `type: "document"`, neve a fájlnév, `line: 1`,
  `endLine`: a fájl utolsó sora. A sorszámok 1-alapúak és a forrás eredeti
  sortörései szerintiek.
- `abstract`, `final` hamis, `extends`, `implements`, `traits`, `doc` üres/
  `null`; `properties`, `writes`, `comments`, `controlFlow` alapból üres.
- A `method.container` és a `call.container` a dokumentum-konténer neve.
  A dokumentumoknál a `method.body`, `params`, `returnType` és a PHP-s
  láthatósági/absztrakt jelentés nem értelmezett.

## JSON és JSONC

### Konténer és konstansok

Egy JSON-dokumentum egy konténer. Minden objektumkulcs egy `constant`:

```json
{"scripts":{"build":"vite build"}}
```

```text
scripts        -> name "scripts", value "object (1)", line: 1
scripts.build  -> name "scripts.build", value "\"vite build\"", line: 1
```

`constant.kind` mindig `"key"`, `name` a pontozott kulcsútvonal (a tömbindex
nem része a névnek), `value` a rövid érték: a string idézőjelben (60 karakter
fölött `…`-ra vágva), a szám, a `true` / `false` és a `null` maga, az objektum
és a tömb a fajtája az elemszámmal (`object (3)`, `array (2)`);
`visibility: "public"`, `doc: null`. A `fájl.json::a.b.c` szelektor (tömb-elem
indexszel: `files.0`) a lekérdezés idejére `key` fajtájú szakasz a kulcs
sorától az értéke végéig — a `--body` így a kulcs JSON-részfáját adja. A
`locate` a kulcsot pontos egyezéssel találja meg (a teljes út vagy az utolsó
tagja). A kulcs
sora a kulcs idézőjelének kezdősora; egysoros JSON-nál ezért minden kulcs
sora 1. Nincs mélységi korlát: a teljes objektumfa bekerül. Tömbben lévő
objektumok kulcsai ugyanígy a környező kulcsútvonal alatt jelennek meg.

### Imports / hivatkozások

Az `imports` sorrendtartó alias–cél térkép:

- minden `$ref` és `extends` skalár értéke bekerül (`"$ref"` / `"extends"`
  alias-szal);
- `package.json`: `dependencies` és `devDependencies` csomagjai;
- `composer.json`: `require` és `require-dev` csomagjai;
- más kulcs alatti tetszőleges string nem import, csak konstansérték.

A csomag-import célja a verzió-korlát (például `"^6.2"`), mert a közös séma
nem külön dependency-típust tartalmaz. Ismétlődő aliasnál az utolsó érték
marad; ha nincs import, az érték `[]`.

### Kommentek és sorvégek

A szabványos JSON komment nélküli. JSONC esetén a `//` és `/* ... */`
kommenteket figyelmen kívül hagyja a parser, de a kulcs- és értékpozíciók
eredeti sorai megmaradnak. Hibás JSONC best-effort módban elemezhető; a
bizonytalan csomópontot nem kell kitalálni vagy importként felvenni.
`endLine` JSON-kulcshoz nincs a közös `Constant` mezőiben: a kulcs kezdősora
az egyetlen rögzített pozíció, a konténer `endLine` viszont mindig a fájl vége.

## Markdown

### Konténer és címsor-szakaszok

Egy Markdown-fájl egy `document` konténer. Minden `#`–`######` címsor egy
`method`, mert a közös sémában ez a legközelebbi strukturális egység.
`method.name` a címsor szövege, `line` a címsor sora, `endLine` a következő,
azonos vagy magasabb szintű címsor előtti sor (utolsó szakasznál a fájl vége).
`visibility` hordozza a címsorszintet `h1`…`h6` alakban; `static` és `abstract`
hamis. A címsorok nem külön konténerek, így a beágyazás a
`container: document` + `visibility: hN` viszonyban marad, nem generál
ál-konténereket.

### Linkek és képek

Minden Markdown link egy `call`, `line` a linket tartalmazó sor, `caller` a
legközelebbi címsor neve (címsor nélküli szövegnél `__top__`), `target` a cél.
`type` `link` vagy `image`, `category` pedig:

- `internal_file`: relatív fájl vagy útvonal;
- `anchor`: `#...` horgony;
- `external_url`: `http:`, `https:` és más abszolút séma;
- `image`: képhivatkozás.

Az URL, a fájl és a horgony feloldása külön `LanguageDocument`-felelősség,
a golden csak a nyers célt rögzíti. `endLine` a `Call` mezőiben nincs; a link
kezdősora az egyetlen pozíció.

### Kódblokkok, táblázatok és egyéb Markdown

Minden zárt fenced kódblokk egy `string`. `line` a nyitó fence sora,
`value` formája `code:<nyelv>; lines:<kezdő>-<záró>`; nyelv hiányában
`plaintext`. Így a közös `StringLit` endLine nélküli mezője mellett is
deterministikusan megmarad a teljes sortartomány. A fence-sorok a tartományba
beletartoznak.

Táblázatok, listák, inline-kód, HTML-kommentek és szabad szöveg nem kerülnek
önálló FileGraph-mezőbe. A táblázat sorai a körülvevő címsor-szakaszban
maradnak; `controlFlow` nem készül. Linkek és képek kódblokkon belül viszont
nem számítanak Markdown-hivatkozásnak.

## Golden ellenőrzés

`golden/ref-json.jsonl` és `golden/ref-md.jsonl` egy-egy sort tartalmaz
fájlonként, `{"path": ..., "graph": {...}}` alakban. A corpus-fájlok ugyanazt
a sorrendet és abszolút útvonalakat sorolják. A kiválasztott JSON-korpuszból
származó `package-lock.json` csak akkor kerülhet elemzésre, ha a projekt
függőségi lock-fájljaként szükséges; `node_modules`, `vendor`, `public` és
`storage` kizárt.
