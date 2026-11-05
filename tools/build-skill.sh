#!/usr/bin/env bash
# A callgraph skill-csomag előállítása: dist/callgraph/ + dist/callgraph-<verzió>.tar.gz
# Használat: bash tools/build-skill.sh
#   - A skill fájljai a skill/ mappából jönnek (SKILL.md, README.md, callgraph.sh, lib.sh, install.sh, projects.sh, feedback.sh, extras/, help/).
#   - Binárisok: ha van dist/bin/<os>-<arch>/callgraph[.exe] (pl. a CI-ből), azok mennek a csomagba;
#     különben helyben: macOS-en a két mac-target, máshol a natív build (a teljes, több platformos csomagot a GitHub Actions készíti).
#   - Tiltólista (gép-/projektfüggő szöveg a csomagban): tools/forbidden.local (nem verziózott, egy regex), különben '/Users/|/home/'.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME="callgraph"
DIST="$ROOT/dist"
PKG="$DIST/$NAME"
TAR="$DIST/$NAME-$VERSION.tar.gz"

rm -rf "$PKG"; rm -f "$DIST/$NAME"-*.tar.gz
mkdir -p "$PKG/bin"
cp skill/SKILL.md skill/README.md skill/TELEPITES.md skill/callgraph.sh skill/lib.sh skill/install.sh skill/projects.sh skill/feedback.sh "$PKG/"
cp -R skill/extras "$PKG/extras"
cp -R skill/help "$PKG/help"   # a súgók (help/<script>.<en|hu>.txt); a felhasználó lang fájlja NEM kerül a csomagba
cp LICENSE "$PKG/LICENSE"

if [[ -d "$DIST/bin" ]]; then
    cp -R "$DIST/bin/." "$PKG/bin/"
else
    # helyi build: a binárisokba ne égjen be a gép útvonala
    export CARGO_TARGET_DIR="$ROOT/target/pkg"
    export RUSTFLAGS="--remap-path-prefix=$ROOT=callgraph --remap-path-prefix=$HOME=~"
    # a C-kódos crate-ek (tree-sitter, ring) assert()-jei a __FILE__-lal a teljes utat égetnék be
    export CFLAGS="${CFLAGS:-} -ffile-prefix-map=$ROOT=callgraph -ffile-prefix-map=$HOME=~ -ffile-prefix-map=${CARGO_HOME:-$HOME/.cargo}=~/.cargo"
    host="$(rustc -vV | sed -n 's/^host: //p')"
    pairs=("$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)=$host")
    [[ "$(uname -s)" == Darwin ]] && pairs=("darwin-arm64=aarch64-apple-darwin" "darwin-x86_64=x86_64-apple-darwin")
    for pair in "${pairs[@]}"; do
        dir="${pair%%=*}"; triple="${pair#*=}"
        if ! rustup target list --installed 2>/dev/null | grep -qx "$triple"; then
            echo "⚠ kihagyva: $dir (rustup target add $triple)"; continue
        fi
        cargo build --release --locked --target "$triple"
        mkdir -p "$PKG/bin/$dir"
        cp "$CARGO_TARGET_DIR/$triple/release/callgraph" "$PKG/bin/$dir/callgraph"
        [[ "$(uname -s)" == Darwin ]] && codesign -s - -f "$PKG/bin/$dir/callgraph" 2>/dev/null || true
    done
fi
chmod +x "$PKG"/*.sh "$PKG"/bin/*/callgraph* "$PKG"/extras/hooks/* "$PKG"/extras/tools/* 2>/dev/null || true

FORBIDDEN='/Users/|/home/'
[[ -f "$ROOT/tools/forbidden.local" ]] && FORBIDDEN="$(head -1 "$ROOT/tools/forbidden.local")"
if grep -rIn -i -E "$FORBIDDEN" "$PKG" --exclude-dir=bin; then echo "✗ gép- vagy projektfüggő szöveg (fent)"; exit 1; fi
for b in "$PKG"/bin/*/callgraph*; do
    if strings "$b" | grep -q -i -E "$FORBIDDEN"; then echo "✗ gépfüggő útvonal a binárisban: ${b#$PKG/}"; exit 1; fi
done
echo "✓ tiltólistás ellenőrzés tiszta (szöveg + bináris)"

COPYFILE_DISABLE=1 tar -czf "$TAR" -C "$DIST" "$NAME"
for b in "$PKG"/bin/*/callgraph*; do echo "  ${b#$PKG/}: $(du -h "$b" | cut -f1)"; done
echo "✓ $TAR ($(du -h "$TAR" | cut -f1))"
