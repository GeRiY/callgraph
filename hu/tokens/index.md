> Miért fogyaszt sok tokent a Claude Code Grep-pel és teljes fájlok olvasásával, és hogyan talál meg egy metódust pár sorban a call graph kód-térkép.

Source: https://callgraph.eu/hu/tokens/

# Claude Code token-használat csökkentése
    A kódoló agent tokenjeinek nagy része nem a kód írására megy el, hanem a megtalálására:
      Grep-találatokra, egy függvény miatt végigolvasott teljes fájlokra, és tíz perc múlva ugyanannak a fájlnak az újraolvasására.
      A callgraph egy ingyenes call graph és kód-térkép, amellyel a Claude Code csak azokat a sorokat nyitja meg, amelyekre szüksége van.

      [Telepítés](https://callgraph.eu/hu/#install)
      [MCP-n keresztül](https://callgraph.eu/hu/mcp/)

  ## Miért fogy el a token a keresgélésre

    - **A Grep minden találatot visszaad.** Egy gyakori név tucatnyi sorra illeszkedik olyan fájlokban, amelyek semmit nem számítanak a feladathoz, és minden sorért fizetni kell.
    - **Teljes fájlokat olvas.** Egyetlen metódus ellenőrzéséhez megnyitja az egész fájlt, gyakran ezer sort, aminek a nagy részét sosem használja.
    - **Ugyanazt újra elolvassa.** Hosszú munkamenet vagy kontextus-törlés után az agent elfelejti, mit látott, és másodszor is beolvassa.
    - **A találgatás pluszkör.** Egy rossz fájl felesleges olvasás, utána jön a következő tipp.

  Mindez kontextus, amit a modell a munkamenet hátralévő részében is cipel, ezért a költség a keresés után is nő.

  ## Mit csinál ehelyett a callgraph
  A callgraph egyszer AST-indexbe elemzi a kódot, és négy kérdésre válaszol, mindegyikre pár sorban.

    - **Hol kezdjem?** A `locate` a feladat neveiből rangsorolja azt a pár fájlt, amit érdemes megnyitni, indoklással.
    - **Mi van ebben a fájlban?** Az outline osztályokat és metódusokat listáz sortartománnyal, és megmutatja, mit hív mindegyik.
    - **Mutasd az egyik metódust.** A `file::method --body` csak azt a metódust írja ki, sorszámozva.
    - **Ki hívja?** A `--grep` megtalálja a hívókat, így látszik, mit érint egy változtatás.

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG locate refund --project shop                          # 1. melyik fájlok számítanak
bash $CG --project shop app/Services/RefundService.php         # 2. outline sortartományokkal
bash $CG --project shop app/Services/RefundService.php::processRefund --body   # 3. egy metódus
bash $CG --project shop app --grep '(^|::|\.|->)processRefund$'                # 4. hívók
```
  Az 1. és 2. lépés kimenete (rövidítve)
```
locate: 1 terms · 6 matches in 3 files · start here ↓
1. app/Services/RefundService.php  ★★★
   processRefund()  L6-19  callers: 2  ← "refund" (exact method)
2. app/Http/Controllers/RefundController.php  ★★
   store()  L14-22  calls RefundService::processRefund

FILE  app/Services/RefundService.php
class RefundService  L5-41
  processRefund()  L6-19
  notifyCustomer()  L21-30
  logRefund()  L32-40
```
  Az agent kb. egy tucat sort olvas el a fájl megtalálásához, aztán egy tizenöt soros metódust egy négyszáz soros fájl helyett.
    Ugyanez működik PHP, JavaScript / TypeScript, Vue, React, Rust, Swift, Python, Go és Kotlin kódnál is.
    [Többet a locate-ről](https://callgraph.eu/hu/locate/).

  ## Beállítás három lépésben

    - **Letöltés és telepítés.** Futtasd a [kezdőlapon](https://callgraph.eu/hu/#install) található egysoros telepítőt. Kicsomagolja a Claude Code skillt és egy binárist; csak a `bash` kell hozzá.
    - **Telepítő szkript.** A `bash ~/.claude/skills/callgraph/install.sh` regisztrálja a projektet, és megír egy egymondatos szabályfájlt, a `.claude/rules/callgraph.md`-t, így a Claude Code magától a callgraph-ot használja.
    - **Vagy MCP-szerver.** Ha inkább eszközöket szeretnél skill helyett, regisztráld a `callgraph mcp`-t, és ugyanezek a lekérdezések eszközként jelennek meg. Lásd az [MCP-szerver beállítását](https://callgraph.eu/hu/mcp/).

```
bash ~/.claude/skills/callgraph/install.sh
```
  Ezután indítsd újra a Claude Code-ot, és adj neki egy szokásos feladatot. Promptonként nincs mit beállítani.

  ## Cursor, Codex és VS Code is
  A skill a Claude Code-hoz való. Minden más agent az MCP-szervert használja: ugyanaz a `locate`, outline,
    metódus-törzs és hívó-eszköz, helyben, a gépeden futva. A Claude Desktop, Cursor, VS Code és Codex beállítása az
    [MCP oldalon](https://callgraph.eu/hu/mcp/) van. A kód nem hagyja el a számítógépedet.

  ## Kérdések
  ### Módosítja a callgraph a kódomat?
Nem. Csak olvassa a projektet, és válaszol: hol van valami, mit hív és ki hívja. A forrásfájlokba nem ír.
  ### Másképp kell promptolnom a Claude Code-ot?
Nem. Miután az `install.sh` megírja az egymondatos szabályfájlt, a Claude Code feladat indulásakor magától a callgraph-hoz nyúl. Névvel is kérheted.
  ### Működik Cursorral, Codexszel és VS Code-dal?
Igen. Futtasd a callgraph-ot helyi MCP-szerverként, és bármelyik MCP-kliens ugyanazokat az eszközöket kapja: locate, outline, metódus-törzs és hívók.
  ### Milyen nyelveket támogat?
PHP, JavaScript / TypeScript, Vue, React, Rust, Swift, Python, Go és Kotlin, továbbá a CSS, JSON, Markdown és HTML fájlok szerkezetét.

  ## Kapcsolódó

    [### locateRangsorolt fájlok a feladat kezdéséhez.](https://callgraph.eu/hu/locate/)
    [### MCP-szerverUgyanezek a lekérdezések bármelyik MCP-kliensben.](https://callgraph.eu/hu/mcp/)
    [### Projekt-profilEgy ismeretlen projekt felépítése kb. 15 sorban.](https://callgraph.eu/hu/profile/)
    [### TelepítésEgy sor macOS-re, Linuxra és Windowsra.](https://callgraph.eu/hu/#install)
