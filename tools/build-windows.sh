#!/usr/bin/env bash
# Windows-bináris Dockerben (rust:1 Debian + mingw-w64, cross-fordítás x86_64-pc-windows-gnu targetre).
# Használat: bash tools/build-windows.sh   → dist/windows-build/callgraph-windows-x86_64.exe
# (a build-skill.sh innen veszi fel a csomagba: bin/windows-x86_64/callgraph.exe). Apple Siliconon az amd64 image emulációval fut.
# Utána a cargo-volume és a letöltött image törlődik (docker-build-lib.sh).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/dist/windows-build"
mkdir -p "$OUT"
source "$ROOT/tools/docker-build-lib.sh"
T=x86_64-pc-windows-gnu
docker_build rust:1 linux/amd64 callgraph-cargo-windows "$OUT/build-windows.log" \
    "apt-get update -qq && apt-get install -y -qq mingw-w64 >/dev/null && rustup target add $T && CARGO_TARGET_DIR=/tmp/t cargo build --release --locked --target $T && cp /tmp/t/$T/release/callgraph.exe /out/callgraph-windows-x86_64.exe"
rc=$?
echo "windows-x86_64: rc=$rc ($(tail -1 "$OUT/build-windows.log"))"
exit $rc
