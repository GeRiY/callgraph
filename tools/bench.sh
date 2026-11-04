#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RPG="$ROOT/target/release/callgraph"
REF_SCRIPT="$HOME/.claude/skills/callgraph/ast_graph.php"
CACHE="${TMPDIR:-/tmp}/callgraph-bench-cache-$$.json"
ONE="$(sed -n '/^[^[:space:]]/p' "$ROOT/golden/corpus.txt" | head -n 1)"
[ -f "$ROOT/tools/local.env" ] && . "$ROOT/tools/local.env"
CORPUS_A="${CALLGRAPH_CORPUS_A:?tools/local.env}"
CORPUS_B="${CALLGRAPH_CORPUS_B:?tools/local.env}"

trap 'rm -f "$CACHE"' EXIT

measure() {
    local label="$1"; shift
    local tfile
    tfile="$(mktemp "${TMPDIR:-/tmp}/callgraph-bench.XXXXXX")"
    /usr/bin/time -p "$@" >/dev/null 2>"$tfile"
    local real
    real="$(awk '$1 == "real" { print $2 }' "$tfile")"
    rm -f "$tfile"
    printf '%-12s %8s s\n' "$label" "${real:-error}"
}

run_set() {
    local name="$1"; local input="$2"
    printf '\n%-12s %s\n' "$name" "$input"
    rm -f "$CACHE"
    measure 'callgraph cold' "$RPG" --cache "$CACHE" --no-cache "$input"
    measure 'callgraph warm' "$RPG" --cache "$CACHE" "$input"
    measure 'ref cold' php -d display_errors=0 "$REF_SCRIPT" "$input"
    measure 'ref warm' php -d display_errors=0 "$REF_SCRIPT" "$input"
}

printf '%-12s %s\n' 'benchmark' 'seconds (cold/warm)'
run_set 'one file' "$ONE"
run_set 'korpusz-A' "$CORPUS_A"
run_set 'korpusz-B' "$CORPUS_B"
