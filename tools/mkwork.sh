#!/usr/bin/env bash
# Munkapéldány egy feladathoz: work/<kód>/ = a fő fa Cargo.toml + Cargo.lock + src + tests másolata (saját target/).
set -euo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"; K="$1"; W="$R/work/$K"
[ -e "$W" ] && { echo "már létezik: $W" >&2; exit 1; }
mkdir -p "$W"; cp "$R/Cargo.toml" "$R/Cargo.lock" "$W/"; [ -f "$R/build.rs" ] && cp "$R/build.rs" "$W/"; cp -R "$R/src" "$W/src"; [ -d "$R/tests" ] && cp -R "$R/tests" "$W/tests"
echo "$W"
