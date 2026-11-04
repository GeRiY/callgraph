#!/usr/bin/env bash
# Egy munkapéldány MEGADOTT (a feladatnak kiosztott) fájljainak átvétele a fő fába.
# Csak a név szerint felsorolt fájlokat másolja — a munkapéldány régebbi másolatai így nem írják felül a fő fa
# közben beolvasztott változásait. Közös fájlt (pl. mod.rs) kézzel kell összefésülni.
# Használat: bash tools/merge.sh <kód> <src-hez képesti út> [...]
set -uo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"
k="$1"; shift
[ $# -gt 0 ] || { echo "add meg az átveendő fájlokat (src/-hez képest)" >&2; exit 1; }
cd "$R"
for f in "$@"; do
    if [ ! -e "work/$k/src/$f" ]; then echo "  NINCS a munkapéldányban: $f" >&2; continue; fi
    mkdir -p "src/$(dirname "$f")"
    cp "work/$k/src/$f" "src/$f"
    echo "  átvéve: $f"
done
