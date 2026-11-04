#!/usr/bin/env bash
# Bájtra azonos-e egy callgraph kimenete az alapvonallal (baseline/, vagy a munkapéldány .baseline fájlja szerint) a teljes app + modules mappán.
# Használat: bash tools/identity_check.sh <callgraph-bináris>
set -uo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"; RPG="$1"; T="$(mktemp -d)"; ok=0
# alapvonal: a munkapéldány ./.baseline jelölőfájlja szerint (régebbi munkapéldányoknál a régi), különben a legutóbbi (baseline/)
B="$R/baseline"; [ -f ./.baseline ] && B="$R/$(cat ./.baseline)"
echo "alapvonal: ${B#$R/}"
# a két referencia-korpusz útja: tools/local.env (minta: tools/local.env.example)
[ -f "$R/tools/local.env" ] && . "$R/tools/local.env"
: "${CALLGRAPH_CORPUS_A:?állítsd be a tools/local.env-ben}" "${CALLGRAPH_CORPUS_B:?állítsd be a tools/local.env-ben}"
"$RPG" --lang php --no-cache "$CALLGRAPH_CORPUS_A" > "$T/app.jsonl" 2>/dev/null
"$RPG" --lang php --no-cache "$CALLGRAPH_CORPUS_B" > "$T/modules.jsonl" 2>/dev/null
for f in app modules; do
  if cmp -s "$B/$f.jsonl" "$T/$f.jsonl"; then echo "AZONOS  $f ($(wc -l < "$B/$f.jsonl" | tr -d ' ') fájl)"; else echo "ELTÉR   $f"; ok=1
    python3 - "$B/$f.jsonl" "$T/$f.jsonl" <<'PY'
import json,sys
a=[json.loads(l) for l in open(sys.argv[1])]; b=[json.loads(l) for l in open(sys.argv[2])]
print("  sorok:", len(a), "vs", len(b)); n=0
for x,y in zip(a,b):
    if x!=y:
        ks=[k for k in x if x.get(k)!=y.get(k)]; print("  ", x.get("file","?")[-70:], "eltérő mezők:", ks); n+=1
        if n>=5: break
PY
  fi
done
rm -rf "$T"; exit $ok
