#!/usr/bin/env bash
# A teljes, több platformos csomag EGYMÁS UTÁN: Linux (aarch64, x86_64) → Windows (x86_64) → csomag (macOS-en a két mac-bináris is).
# Minden Docker-lépés után a build-image és a cargo-volume törlődik, így egyszerre legfeljebb egy image foglal helyet.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bash "$ROOT/tools/build-linux.sh" || exit $?
bash "$ROOT/tools/build-windows.sh" || exit $?
bash "$ROOT/tools/build-skill.sh"
