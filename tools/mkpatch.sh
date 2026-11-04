#!/usr/bin/env bash
# Patch készítése egy munkapéldányból a fő fához képest: work/patches/<kód>.patch
# Használat: bash tools/mkpatch.sh <kód> [<kód> ...]
set -uo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$R/work/patches"
cd "$R"
for k in "$@"; do
    diff -ruN src "work/$k/src" > "work/patches/$k.patch"
    files=$(grep '^+++ ' "work/patches/$k.patch" | awk '{print $2}' | sed "s#work/$k/src/##" | tr '\n' ' ')
    echo "== $k: $files"
done
