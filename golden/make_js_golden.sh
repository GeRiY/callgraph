#!/usr/bin/env bash
# JS/TS/Vue golden: a régi js_graph.mjs kimenetét {"path","graph"} sorokba csomagolja (mint a PHP ref.jsonl).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
JS="$HOME/.claude/skills/callgraph/js/js_graph.mjs"

run() { # $1 = fájllista, $2 = kimenet
  tr '\n' '\0' < "$1" | xargs -0 node "$JS" 2>"$2.err" | python3 -c '
import sys, json
for line in sys.stdin:
    if line.strip():
        g = json.loads(line)
        print(json.dumps({"path": g["file"], "graph": g}, ensure_ascii=False, separators=(",", ":")))
' > "$2"
  echo "$2: $(wc -l < "$2") fájl, $(wc -l < "$2.err") hiba"
  [ -s "$2.err" ] || rm -f "$2.err"
}

ls "$HERE"/samples/js/* | LC_ALL=C sort > "$HERE/.samples-js.txt"
run "$HERE/corpus-js.txt" "$HERE/ref-js.jsonl"
run "$HERE/.samples-js.txt" "$HERE/ref-js-samples.jsonl"
rm -f "$HERE/.samples-js.txt"
