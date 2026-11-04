# Kód olvasása: callgraph.sh

Minden agentre érvényes (main, subagent). PHP-, `.vue`-, `.js`- és `.ts`-fájl megértéséhez az **első
lépés a `callgraph.sh`**, nem a teljes Read. A metódus törzsét is a callgraph adja (`--body`); célzott
Read (offset/limit) csak ha mégis kell. A ≤100 soros fájl egészben is olvasható.

```bash
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG --project <alias> app/Http/Controllers/X.php                  # osztály, metódusok sortartománnyal, hívások
bash $CG --project <alias> app/Http/Controllers/X.php::store --body    # egy metódus forrása
bash $CG --project <alias> app --grep 'Model::method'                  # hol hívják
bash $CG --project <alias> app --grep 'Model::method' --depth 3        # a hívók hívói
bash $CG --project <alias> resources/js/Pages/X.vue                    # props, emits, template, setup, függvények
bash $CG --project <alias> resources/js --grep 'route::orders\.'       # route-használat (Ziggy)
bash $CG --project <alias> resources/js --grep 'i18n::components\.x'   # fordítási kulcs helye
```

- Kimenet-korlát: 400 sor fölött summary / metódus-index; szűkíts (`::név`, `--grep`, almappa), `--full`
  csak indokolt esetben.
- Szöveges keresés, ami nem kódhívás / literál: Grep.
- **Ha a callgraph hibázik, hiányos vagy félrevezető, kötelező feedback:**
  `bash ~/.claude/skills/callgraph/feedback.sh add <error_id> "<mit vártál, mi történt>"` (error-id nélkül
  `add --note "<…>"`). A kerülőút ettől még mehet.
