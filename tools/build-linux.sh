#!/usr/bin/env bash
# Linux-binárisok Dockerben (hivatalos rust:1-alpine → statikus musl-bináris, bármely disztribúción fut).
# Használat: bash tools/build-linux.sh   → dist/linux-build/callgraph-linux-aarch64, callgraph-linux-x86_64
# (a build-skill.sh innen veszi fel őket a csomagba). Az amd64 Apple Siliconon emulációval fordul (lassabb).
# A két architektúra EGYMÁS UTÁN fut; utána a cargo-volume és a letöltött image törlődik (docker-build-lib.sh).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/dist/linux-build"
mkdir -p "$OUT"
source "$ROOT/tools/docker-build-lib.sh"
rc_all=0
for pair in arm64:linux-aarch64 amd64:linux-x86_64; do
    plat="${pair%%:*}"; name="${pair#*:}"
    docker_build rust:1-alpine "linux/$plat" "callgraph-cargo-$plat" "$OUT/build-$name.log" \
        "apk add --no-cache musl-dev gcc >/dev/null && CARGO_TARGET_DIR=/tmp/t cargo build --release --locked && cp /tmp/t/release/callgraph /out/callgraph-$name"
    rc=$?
    echo "$name: rc=$rc ($(tail -1 "$OUT/build-$name.log"))"
    (( rc == 0 )) || rc_all=$rc
done
exit $rc_all
