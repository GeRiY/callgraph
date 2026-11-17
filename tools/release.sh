#!/usr/bin/env bash
# Kiadás egy develop-checkoutból: verzió-emelés, teljes check, kiadási commit, main, annotált tag.
# Használat: bash tools/release.sh <x.y.z> [--dry]
#   --dry   minden lépést és parancsot kiír, de semmit nem változtat (a precondíciók ellenőrzése is csak olvas)
# A változásnapló SZÖVEGÉT a changelog-writer agent írja (/changelog), ezt a script soha; ha a `[x.y.z]` szakasz
# hiányzik, megáll. Egyetlen build fut: a develop- és a main-push kiadási commitnál kimarad (lásd release.yml),
# a `v*` tag-futás épít, és az frissíti a gördülő `dev` előzetes kiadást is.
# Opcionális: CALLGRAPH_DEPLOY_KEY = a push SSH-kulcsa (ha be van állítva, GIT_SSH_COMMAND-ba kerül).
set -euo pipefail

VER=""; DRY=0
for a in "$@"; do
    case "$a" in
        --dry) DRY=1 ;;
        -*) echo "ismeretlen kapcsoló: $a" >&2; exit 2 ;;
        *) [[ -z "$VER" ]] && VER="$a" || { echo "több verzió megadva" >&2; exit 2; } ;;
    esac
done
[[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "használat: bash tools/release.sh <x.y.z> [--dry]" >&2; exit 2; }

cd "$(git rev-parse --show-toplevel)"
TODAY="$(date +%Y-%m-%d)"
ID=(-c user.name=mgeri1993 -c user.email=mgeri1993@gmail.com)
if [[ -n "${CALLGRAPH_DEPLOY_KEY:-}" ]]; then
    export GIT_SSH_COMMAND="ssh -i $CALLGRAPH_DEPLOY_KEY -o IdentitiesOnly=yes"
fi

# --dry-nál a precondíció-hiba csak figyelmeztetés (a lépések listája így is végigmegy)
die() { echo "release.sh: $*" >&2; [[ $DRY -eq 1 ]] || exit 1; }
step() { echo; echo "== $*"; }
# végrehajt, vagy --dry-nál csak kiír
run() {
    if [[ $DRY -eq 1 ]]; then echo "   [dry] $*"; else echo "   \$ $*"; "$@"; fi
}

[[ $DRY -eq 1 ]] && echo "(--dry: semmi nem változik)"

step "1. precondíciók"
# a changelog-writer még nem commitolt CHANGELOG*.md módosítása megengedett (a kiadási commitba kerül)
[[ -z "$(git status --porcelain | grep -vE ' (CHANGELOG\.md|CHANGELOG\.hu\.md)$' || true)" ]] || die "a munkafa nem tiszta (a CHANGELOG*.md kivételével)"
branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$branch" == develop ]] || die "a develop ágon kell állni (most: $branch)"
if [[ $DRY -eq 0 ]]; then git fetch origin develop main --tags --quiet || die "git fetch sikertelen"; fi
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/develop)" ]] \
    || die "a develop nem egyezik az origin/develop-pal (pull / push kell előbb)"
! git rev-parse -q --verify "refs/tags/v$VER" >/dev/null || die "a v$VER tag már létezik"
echo "   develop = origin/develop = $(git rev-parse --short HEAD)"

if command -v gh >/dev/null; then
    sha="$(git rev-parse HEAD)"
    concl="$(gh run list --branch develop --commit "$sha" --workflow build --limit 1 --json conclusion --jq '.[0].conclusion' 2>/dev/null || true)"
    case "$concl" in
        success) echo "   CI ($sha): success" ;;
        "")      die "nincs CI-futás a develop HEAD-hez ($sha); várd meg, vagy nézd meg kézzel" ;;
        *)       die "a develop HEAD CI-futása nem sikeres: $concl" ;;
    esac
else
    echo "   FIGYELEM: nincs gh → a CI-eredményt kézzel ellenőrizd (develop HEAD zöld-e)"
fi

for f in CHANGELOG.md CHANGELOG.hu.md; do
    if grep -q "^## \[$VER\]" "$f"; then
        grep -q "^## \[$VER\] — $TODAY" "$f" || echo "   FIGYELEM: $f [$VER] szakasza nem mai dátumú ($TODAY)"
        echo "   $f: [$VER] megvan"
    else
        # nem üres [Unreleased]: a fejléc és a következő `## [` között van szöveg
        body="$(awk '/^## \[Unreleased\]/{on=1;next} /^## \[/{on=0} on' "$f" | tr -d '[:space:]')"
        [[ -n "$body" ]] || die "$f: nincs [Unreleased] tartalom és [$VER] sincs"
        die "$f: nincs [$VER] szakasz → futtasd a changelog-writer agentet (/changelog $VER), majd újra"
    fi
done

step "2. verzió-emelés (Cargo.toml, Cargo.lock)"
if [[ $DRY -eq 1 ]]; then
    echo "   [dry] Cargo.toml: version = \"$VER\" (az első version sor a [package]-ben)"
    echo "   [dry] Cargo.lock: a callgraph csomag verziója → $VER"
else
    sed -i.bak -E "0,/^version *= *\".*\"/s//version = \"$VER\"/" Cargo.toml
    awk -v v="$VER" '
        /^name = "callgraph"$/ {hit=1}
        hit && /^version = / {print "version = \"" v "\""; hit=0; next}
        {print}' Cargo.lock > Cargo.lock.new && mv Cargo.lock.new Cargo.lock
    rm -f Cargo.toml.bak
    git diff --stat -- Cargo.toml Cargo.lock
fi

step "3. teljes check (fmt + clippy + test)"
run bash tools/dev.sh check

step "4. kiadási commit"
run git add Cargo.toml Cargo.lock CHANGELOG.md CHANGELOG.hu.md
run git "${ID[@]}" commit -m "Release $VER: version and changelog"

step "5. main beolvasztása (ha van új commitja)"
if [[ -n "$(git rev-list HEAD..origin/main 2>/dev/null)" ]]; then
    if [[ $DRY -eq 1 ]]; then
        echo "   [dry] git merge origin/main -m 'Merge main into develop before release $VER' (konfliktusnál abort)"
    else
        git "${ID[@]}" merge origin/main -m "Merge main into develop before release $VER" \
            || { git merge --abort; die "konfliktus a main beolvasztásakor; megszakítva, kézzel oldd fel"; }
    fi
else
    echo "   az origin/main minden commitja már a HEADben van"
fi

step "6. push: develop → main (ugyanaz az SHA) → annotált tag → tag-push"
echo "   (a develop-push a kiadási commit miatt nem épít; a tag-futás épít, kiad és a dev-et is frissíti)"
run git push origin develop
run git push origin HEAD:refs/heads/main
run git "${ID[@]}" tag -a "v$VER" -m "callgraph $VER"
run git push origin "v$VER"

step "7. teendők a kiadás után"
cat <<EOF
   - tag-futás figyelése: gh run watch \$(gh run list --branch v$VER --limit 1 --json databaseId --jq '.[0].databaseId')
     (Release + dev előzetes kiadás + MCP Registry; ellenőrizd: gh release view v$VER)
   - weboldal (gh-pages ág): checkout, majd
       python3 site/build.py --changelog-only <gh-pages checkout>   # csak a változásnapló + verziószövegek
       python3 site/check.py <gh-pages checkout>
     látható változásról screenshot jóváhagyásra, utána commit + push a gh-pages-re (lásd site/README.md)
   - a poszt kitételét az owner végzi; mérési számok soha nem kerülnek a repóba
EOF
