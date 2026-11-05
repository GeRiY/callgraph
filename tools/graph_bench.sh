#!/usr/bin/env bash
# A projekt-gráf időmérése egy megadott projekten: hideg építés, majd meleg lekérdezések (többször), a gráffal és
# összevetésül gráf nélkül (CALLGRAPH_NO_GRAPH=1). Az eredmény csak a terminálra megy, sehova nem íródik.
#
# Használat: bash tools/graph_bench.sh <projekt-gyökér> [<hatókör a gyökérhez képest> [<minta> [<ismétlés>]]]
#   CALLGRAPH_BIN=…   a mérendő bináris (alapból target/release/callgraph)
# Példa:     CALLGRAPH_BIN=target/x86_64-unknown-linux-musl/release/callgraph \
#              bash tools/graph_bench.sh ~/src/laravel-framework src 'Container::make' 3
# Figyelem: a hideg méréshez a projekt .callgraph/ mappájából törli a gráfot (a notes.jsonl marad).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${CALLGRAPH_BIN:-$ROOT/target/release/callgraph}"
PROJECT="${1:?projekt-gyökér kell}"
SCOPE="${2:-.}"
PATTERN="${3:-find}"
REPEAT="${4:-3}"
[ -x "$BIN" ] || { echo "graph_bench: nincs bináris: $BIN (CALLGRAPH_BIN=…)" >&2; exit 1; }

# egy lekérdezés ideje másodpercben (a kimenet eldobva)
timed() {
    local start end
    start=$(date +%s.%N)
    "$@" >/dev/null 2>&1 || echo "  (hiba: $*)" >&2
    end=$(date +%s.%N)
    awk -v s="$start" -v e="$end" 'BEGIN { printf "%.3f", e - s }'
}

show() { "$BIN" show --project "$PROJECT" "$@"; }

STORE="$PROJECT/.callgraph"
rm -rf "$STORE/graph" "$STORE/graph.pack" "$STORE/index.bin" "$STORE/facts.bin" "$STORE/callers.bin" \
    "$STORE/index.jsonl" "$STORE/cache.jsonl"
printf 'cold build (--depth 2):          %ss\n' "$(timed show "$SCOPE" --grep "$PATTERN" --depth 2)"

queries=(
    "grep level 1|$SCOPE --grep $PATTERN"
    "grep --depth 2|$SCOPE --grep $PATTERN --depth 2"
    "strings|$SCOPE --strings error"
    "const|$SCOPE --const LIMIT"
)
for entry in "${queries[@]}"; do
    label="${entry%%|*}"
    read -r -a args <<<"${entry#*|}"
    line=""
    for _ in $(seq "$REPEAT"); do
        line+=" $(timed show "${args[@]}")"
    done
    nograph="$(CALLGRAPH_NO_GRAPH=1 timed show "${args[@]}")"
    printf '%-32s graph:%s s   no graph: %s s\n' "warm $label" "$line" "$nograph"
done
du -sh "$STORE" 2>/dev/null | awk '{ print ".callgraph/ size: " $1 }'
