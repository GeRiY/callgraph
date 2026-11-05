#!/usr/bin/env bash
# Fejlesztés Dockerben: a Rust toolchain (fmt, clippy, musl), python3, php és node a konténerben van, helyben csak Docker kell.
# Használat: bash tools/dev.sh <parancs> [argumentumok]
#   build              cargo build --release
#   test [arg…]        cargo test --release [arg…]
#   clippy             cargo clippy --release -- -D warnings
#   fmt                cargo fmt
#   check              cargo fmt --check + clippy + test (a commit előtti kör)
#   skill              bash tools/build-skill.sh (a natív Linux-csomag)
#   run <cmd…>         tetszőleges parancs a konténerben (pl. run target/docker/release/callgraph src/main.rs)
#   shell              interaktív shell a konténerben
#   image              az image újraépítése (pl. Rust-frissítéshez)
# A buildkimenet: target/docker/ (a helyi target/-tól külön). A cargo-registry a callgraph-cargo volume-ban marad meg.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${CALLGRAPH_DEV_IMAGE:-callgraph-dev}"
VOLUME="${CALLGRAPH_DEV_VOLUME:-callgraph-cargo}"

command -v docker >/dev/null || { echo "dev.sh: nincs docker a PATH-on" >&2; exit 1; }

build_image() { docker build -t "$IMAGE" -f "$ROOT/docker/dev.Dockerfile" "$ROOT/docker"; }

in_container() {
    docker image inspect "$IMAGE" >/dev/null 2>&1 || build_image
    local tty=()
    [[ -t 0 && -t 1 ]] && tty=(-it)
    docker run --rm "${tty[@]}" \
        --user "$(id -u):$(id -g)" \
        -v "$ROOT:/src" \
        -v "$VOLUME:/cargo" \
        "$IMAGE" "$@"
}

cmd="${1:-}"
[[ $# -gt 0 ]] && shift
case "$cmd" in
    build)  in_container cargo build --release "$@" ;;
    test)   in_container cargo test --release "$@" ;;
    clippy) in_container cargo clippy --release "$@" -- -D warnings ;;
    fmt)    in_container cargo fmt "$@" ;;
    check)  in_container bash -c 'cargo fmt --check && cargo clippy --release -- -D warnings && cargo test --release' ;;
    skill)  in_container bash tools/build-skill.sh "$@" ;;
    run)    [[ $# -gt 0 ]] || { echo "dev.sh run: add meg a parancsot" >&2; exit 2; }; in_container "$@" ;;
    shell)  in_container bash ;;
    image)  build_image ;;
    *)      sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; [[ -z "$cmd" ]] || exit 2 ;;
esac
