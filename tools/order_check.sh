#!/usr/bin/env bash
# A bejárási fájlsorrend összevetése a referenciával (ast_graph.php) egy mappán.
# Használat: bash tools/order_check.sh <callgraph-bináris> <mappa>
set -uo pipefail
RPG="$1"; DIR="$2"
T="$(mktemp -d)"
php -d display_errors=0 "$HOME/.claude/skills/callgraph/ast_graph.php" "$DIR" \
    | python3 -c "import sys,json;[print(json.loads(l)['file']) for l in sys.stdin if l.strip()]" > "$T/ref.txt"
"$RPG" "$DIR" 2>/dev/null \
    | python3 -c "import sys,json;[print(json.loads(l)['file']) for l in sys.stdin if l.strip()]" > "$T/callgraph.txt"
r=$(wc -l < "$T/ref.txt" | tr -d ' ')
c=$(wc -l < "$T/callgraph.txt" | tr -d ' ')
if cmp -s "$T/ref.txt" "$T/callgraph.txt"; then echo "EGYEZIK  $r fájl  $DIR"; else echo "ELTÉR    ref=$r callgraph=$c  $DIR"; diff "$T/ref.txt" "$T/callgraph.txt" | head -5; fi
rm -rf "$T"
