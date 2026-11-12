#!/usr/bin/env bash
# Az MCPB-csomag (dist/callgraph.mcpb) előállítása a hivatalos MCP Registry-hez és az MCPB-t ismerő kliensekhez
# (pl. Claude Desktop): manifest.json + az összes platform binárisa + az architektúrát választó indító.
# Használat: bash tools/build-mcpb.sh   (bemenet: dist/bin/<os>-<arch>/callgraph[.exe], a CI package jobja tölti le)
# A self-update (--auto-update) itt nincs bekapcsolva: a csomag frissítése a kliens dolga, a bináris nem írja felül magát.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="$ROOT/dist"
STAGE="$DIST/mcpb"
OUT="$DIST/callgraph.mcpb"

[[ -d "$DIST/bin" ]] || { echo "nincs dist/bin (a platform-binárisok a CI-ből jönnek)" >&2; exit 1; }
rm -rf "$STAGE" "$OUT"
mkdir -p "$STAGE/bin"
cp -R "$DIST/bin/." "$STAGE/bin/"
cp tools/mcpb/callgraph-mcp.sh "$STAGE/bin/callgraph-mcp.sh"
chmod +x "$STAGE/bin/callgraph-mcp.sh" "$STAGE"/bin/*/callgraph 2>/dev/null || true
cp LICENSE "$STAGE/LICENSE"

# a csomagban lévő platformok (a manifest compatibility.platforms mezője ebből jön)
platforms=()
compgen -G "$STAGE/bin/darwin-*" >/dev/null && platforms+=(darwin)
compgen -G "$STAGE/bin/linux-*" >/dev/null && platforms+=(linux)
[[ -d "$STAGE/bin/windows-x86_64" ]] && platforms+=(win32)
[[ ${#platforms[@]} -gt 0 ]] || { echo "nincs bináris a dist/bin alatt" >&2; exit 1; }

jq -n --arg v "$VERSION" --args '$ARGS.positional' "${platforms[@]}" | jq --arg v "$VERSION" '{
  manifest_version: "0.3",
  name: "callgraph",
  display_name: "callgraph",
  version: $v,
  description: "AST call graph and code map for AI coding agents: PHP, JS, TS, Vue, Rust, Swift, Python, Go.",
  long_description: "Token-efficient code navigation without reading whole files: ranked starting files for a task (locate), callers and impact analysis, method bodies by line range, string and constant search, and framework views (Laravel routes, validation, events; Vue props and emits). Runs locally, no account, telemetry is opt-in only.",
  author: { name: "Makra Gergő", url: "https://github.com/GeRiY" },
  repository: { type: "git", url: "https://github.com/GeRiY/callgraph" },
  homepage: "https://callgraph.eu",
  documentation: "https://callgraph.eu/mcp/",
  support: "https://github.com/GeRiY/callgraph/issues",
  license: "MIT",
  keywords: ["call graph", "code navigation", "ast", "php", "laravel", "vue", "typescript", "rust"],
  server: {
    type: "binary",
    entry_point: "bin/callgraph-mcp.sh",
    mcp_config: {
      command: "/bin/sh",
      args: ["${__dirname}/bin/callgraph-mcp.sh", "mcp"],
      platform_overrides: {
        win32: { command: "${__dirname}/bin/windows-x86_64/callgraph.exe", args: ["mcp"] }
      }
    }
  },
  tools_generated: true,
  compatibility: { platforms: . }
}' > "$STAGE/manifest.json"

# az MCPB egy zip (.mcpb kiterjesztéssel); a zip megtartja a unix futtatási jogokat
(cd "$STAGE" && zip -qr -X "$OUT" manifest.json LICENSE bin)
rm -rf "$STAGE"
ls -l "$OUT"
