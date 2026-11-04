# callgraph — párhuzamos munkamenet

A port kis, fájlhoz kötött feladatokra bontva, hullámokban (terminál-todo: `W<hullám>·<kód>`). Egy hullámon
belül a feladatok **párhuzamosan** futnak (claude2 és codex egyszerre), mert mindegyik a saját fájljait írja.

## Hullámok

| Hullám | Feladatok (párhuzamosan) | Feltétel |
|---|---|---|
| 0 | R3 váz | — |
| 1 | S1 containers · S2 methods · S3 properties · S4 constants · N1 imports · F1 strings · F2 comments · D1 framework-detektor · C0 natív-függvénylista · I1 walker | R3 |
| 2 | N2 névfeloldás · C1 hívások · F3 if/else · F4 match/switch · F5 writes · D2 stratégiák · I2 JSONL cache | N1 / S2 / D1 / R3 |
| 3 | C2 kategória-lánc · C3 névtér-szabályok · C4 típuskövetés · I3 párhuzamos elemzés · I4 cache-takarítás | C1, C0, N2, D2, I1, I2 |
| 4 | G1 teljes golden + sebesség → G2 callgraph.sh-bekötés | minden fenti |
| 5 | K1 callgraph skill | G2 |

## Szabályok minden feladatra

1. **Munkapéldány.** A végrehajtó NEM a fő fában dolgozik, hanem a saját másolatában:
   `work/<kód>/` (a fő `Cargo.toml`, `Cargo.lock`, `src/`, `tests/` másolata; saját `target/`). A golden-mérce
   közös: `../../golden/ref.jsonl`. Így a párhuzamos munkák nem törik el egymás buildjét.
2. **Fájl-tulajdonlás.** Csak a feladatban megnevezett fájl(ok)at írhatja (+ a hozzájuk tartozó unit teszteket
   ugyanabban a fájlban). Ha egy közös fájlhoz kell nyúlni (pl. egy `mod.rs`-ben a modul regisztrálása, a
   `model/` egy mezője, a `visitor/mod.rs` sorrendje), azt **egy sorban** tegye, és a záró üzenetben jelezze —
   az összefésülést a main végzi.
3. **Mérce.** A viselkedés a `~/.claude/skills/callgraph/ast_graph.php` szerint (l. `docs/ast_graph_map.txt`
   sorszámait — feladatonként lent). Golden-mérés a saját mezőre:
   `python3 ../../golden/compare.py --ref ../../golden/ref.jsonl --cmd "target/release/callgraph {file}" --fields <mező> --show 5`.
4. **Kész** = a saját mező(k) 100% (vagy a feladatban megadott cél), `cargo fmt`, `cargo clippy --release -- -D warnings`
   tiszta, a többi mező értéke nem romlott. Architektúra: `ARCHITECTURE.md`.
5. Záró üzenet: max. 10 sor — a mért %, a módosított fájlok listája, a közös fájlokba tett egysoros változások.

## Mérce-függvények feladatonként (ast_graph.php)

| Feladat | ast_graph.php |
|---|---|
| S1 containers | handleContainer L532–590, handleTraitUse L593–602, cleanDocComment L140 |
| S2 methods | handleClassMethod L722–778, extractMethodBody L786–802, extractLines L810–828, formatParam L222, typeToString L169, defaultToString L195 |
| S3 properties | handleProperty L608–630 (+ promoted: handleClassMethod) |
| S4 constants | handleClassConst L633–647, handleEnumCase L650–661 |
| N1 imports | handleNamespace L516–519, handleUse L521–529 |
| N2 névfeloldás | resolveClassName L1240–1269 |
| F1 strings | handleStringLiteral L1157–1169 |
| F2 comments | collectComments L470–493, cleanPlainComment L496–514 |
| F3 if/else | handleIf L1065–1088, stmtsRange L1044–1051 |
| F4 match/switch | handleMatch L1091–1109, handleSwitch L1115–1150 |
| F5 writes | handleAssign L838–891, handleAssignOp L897–920 |
| C0 natív lista | isNativePhp L129 |
| C1 hívások | enterNode L435–462, buildCallerName L426–433, handleStaticCall L922–946, handleMethodCall L952–1017, handleNew L1020–1041, handleFuncCall L1171–1192, handleClassConstFetch L668–692, handleStaticPropertyFetch L699–720, exprToShortString L1054–1058, leaveNode L1194–1217 |
| C2/C3 kategóriák | categorizeNamespace L1271–1293, categorizeResolvedType L1305–1344 |
| C4 típuskövetés | resolveTrackableType L1352–1364, resolveTypedExpr L1375–1398, handleAssign (varTypes rész) |
| D1/D2 framework | FRAMEWORK_PACKAGE_MAP L240, FRAMEWORK_KNOWN_SHORT_NAMES L250, findComposerJson L265, detectFramework L290 |
| I1 walker | collectPhpFiles L1499, IGNORE_DIRS L1526, loadGitignoreDirs L1535, collectPhpFilesInDir L1561 |
| I2/I3 cache, elemzés | analyzeFileCached L1451, analyzeAll L1607, analyzeFile L1407 |

## Lemez
Minden munkapéldány saját `target/`-je ~300–800 MB. Beolvasztás (vagy elvetés) után a `work/<kód>/target` törlendő; a forrás, a brief és a jelentés marad.

## Docker-buildek (felhasználó, 2026-11-04)
A Linux/Windows build-konténerek egymás után futnak (`tools/build-all.sh`), és minden lépés után a build-image és a cargo-volume törlődik (`tools/docker-build-lib.sh`), hogy ne foglalják a helyet.
