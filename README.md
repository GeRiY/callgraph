# callgraph

Token-hatékony kód-térkép agenteknek (Claude Code skill): PHP, JS / TS / Vue, CSS / SCSS, JSON és Markdown fájlok
szerkezete, kategorizált hívás-gráfja, metódus-törzse, „ki hívja” keresése és hatáselemzése — a fájl beolvasása nélkül.
Az elemző egy önálló Rust-bináris (`callgraph`); a `skill/callgraph.sh` vékony burok fölötte.

- Használat és telepítés: [`skill/SKILL.md`](skill/SKILL.md), [`skill/README.md`](skill/README.md)
- Felépítés: [`ARCHITECTURE.md`](ARCHITECTURE.md), a lekérdező réteg: [`docs/QUERY.md`](docs/QUERY.md), új nyelv: [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md)

## Build

```bash
cargo build --release          # target/release/callgraph
cargo test --release
bash tools/build-skill.sh      # dist/callgraph-<verzió>.tar.gz (a skill-csomag)
```

A több platformos csomagot (macOS arm64 / x86_64, Linux arm64 / x86_64 musl, Windows x86_64) a GitHub Actions
készíti (`.github/workflows/release.yml`); `v*` tagnél GitHub Release-be teszi.

## Licenc

MIT — lásd [`LICENSE`](LICENSE).
