#!/usr/bin/env bash
# callgraph skill telepítő: ellenőrzi a callgraph platform-binárist, létrehozza az üres projects.json-t.
# Az elemzést és a projekt- / feedback-kezelést a natív callgraph bináris végzi (bin/<os>-<arch>/callgraph) — PHP, Node és jq nem kell.
# Használat:  bash install.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"; arch="$(uname -m)"
case "$os" in mingw*|msys*|cygwin*) os=windows ;; esac   # Git Bash: MINGW64_NT-… → windows
case "$arch" in amd64|AMD64) arch=x86_64 ;; esac
# macOS: a böngészőből letöltött (aláíratlan) bináris karantén-jelzőjét a Gatekeeper blokkolja — levesszük a bin/-ről.
if [[ "$os" == darwin ]] && command -v xattr >/dev/null; then
    xattr -dr com.apple.quarantine bin 2>/dev/null || true
fi
chmod +x ./*.sh bin/*/callgraph* 2>/dev/null || true
RPG=""
for c in "bin/$os-$arch/callgraph" "bin/$os-$arch/callgraph.exe" "bin/callgraph" "bin/callgraph.exe"; do
    [[ -x "$c" ]] && { RPG="$c"; break; }
done
if [[ -n "$RPG" ]]; then
    echo "✓ callgraph bináris: $RPG"
else
    echo "⚠ Nincs callgraph bináris ehhez a platformhoz (bin/$os-$arch/callgraph)."
    echo "  Építsd meg (cargo build --release) és másold oda, vagy add meg: CALLGRAPH_BIN=/út/callgraph"
fi
[[ -f projects.json ]] || { echo '{}' > projects.json; echo "✓ üres projects.json"; }
echo "Kész. Projekt regisztrálása: bash projects.sh add <alias> <abszolút_út>"
