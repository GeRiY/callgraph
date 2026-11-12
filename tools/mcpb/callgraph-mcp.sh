#!/bin/sh
# Az MCPB-csomag indítója macOS-en és Linuxon: a gép architektúrájához tartozó binárist indítja
# (az MCPB platform_overrides csak OS szerint választ, architektúra szerint nem). Windowson a manifest közvetlenül
# a bin/windows-x86_64/callgraph.exe-t indítja.
d=$(cd "$(dirname "$0")" && pwd)
case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) echo "callgraph: unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=aarch64 ;;
    *) echo "callgraph: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
# a mac ARM-mappa neve darwin-arm64 (a Linuxé linux-aarch64)
[ "$os-$arch" = darwin-aarch64 ] && arch=arm64
b="$d/$os-$arch/callgraph"
if [ ! -f "$b" ]; then
    echo "callgraph: no binary for $os-$arch in this bundle" >&2
    exit 1
fi
# a kicsomagoló nem mindig tartja meg a futtatási jogot
[ -x "$b" ] || chmod +x "$b" 2>/dev/null
exec "$b" "$@"
