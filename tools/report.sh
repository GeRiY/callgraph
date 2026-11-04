#!/usr/bin/env bash
# Egy feladat záró üzenete (claude2: JSON result; codex: a stdout vége) és költsége.
# Használat: bash tools/report.sh <kód>
set -uo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"
k="$1"
out="$R/work/$k/run.out"
if head -c1 "$out" | grep -q '{'; then
    python3 - "$out" <<'EOF'
import json, sys
d = json.loads(open(sys.argv[1]).read().strip().splitlines()[-1])
print(d.get("result", "")[:1200])
print("költség:", round(d.get("total_cost_usd", 0), 2), "USD, lépés:", d.get("num_turns"))
EOF
else
    tail -12 "$out"
    grep 'DISPATCH-USAGE' "$R/work/$k/run.err" | tail -1 | cut -c1-160
fi
