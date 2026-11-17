//! `rename term <régi> <új>`: egy fogalom (szó) átnevezése a teljes projekten, minden írásmódjában. A `rename class`
//! csak az osztályt írja át; egy modell-átnevezéshez (`Post` → `Article`) ennél több kell: a `PostController`,
//! `StorePostRequest`, `PostFactory`, a `posts()` reláció, a `$post` változó, a `/posts/{post}` útvonal, a
//! `admin.posts.index` route-név, az Inertia `Posts/Index` oldal és a `PostForm.vue` komponens mind ugyanannak a
//! szónak a változata.
//!
//! - **Szó**: az azonosítókat betűsorokra, azokat camelCase-határokon szavakra bontjuk (`StorePostRequest` →
//!   `Store` `Post` `Request`, `post_id` → `post` `id`, `/admin/posts/{post}` → `admin` `posts` `post`). Csak a
//!   teljes szó cserélődik (`postgres`, `compose` nem), az írásmód megmarad (`post` / `Post`, `posts` / `Posts`).
//! - **Többes szám**: alapból képzett (`-s`, `-es`, `-ies`); szabálytalannál megadható: `person/people`.
//! - **Kihagyott**: a csupa nagybetűs szó (`POST` HTTP-ige), a `->post(` / `.post(` / `::post(` / `postJson(` jellegű
//!   HTTP-hívás, a `database/migrations/` (a történeti migrációk érintetlenek), a `vendor` / `node_modules` / …
//! - **Jelzés** (nem változtatott): az érték-helyzetű `'post'` / `"post"` sztring (`=>`, `=`, `:` után; lehet
//!   HTTP-ige, pl. `method="post"`); a tömbkulcs és a hívás-argumentum (`'post' => $x`, `route('post')`) cserélődik.
//! - **Fájlok**: az útvonalban szereplő szó is cserélődik (`PostController.php`, `Pages/Posts/Index.vue`), a fájl
//!   az új helyre kerül, a tartalma is átíródik.
//! - **Séma**: ha a migrációk táblát / oszlopot hoznak létre a szóval (`posts`, `post_tag`, `post_id`), egy új,
//!   átnevező migráció készül (`Schema::rename`, `renameColumn`; `down()` visszafordítja), a régiek érintetlenek.

use std::collections::{BTreeMap, BTreeSet};

use super::RenameRequest;
use super::sites::{Plan, Site, Uncertain, line_of, line_snippet, walk};
use crate::generate::command::GroupFile;
use crate::generate::fs::{Env, stamp};
use crate::generate::{GenError, GenResult};

/// A bejárt fájlfajták.
const EXTS: &[&str] = &["php", "vue", "js", "ts", "jsx", "tsx", "mjs"];
const MIGRATIONS: &str = "database/migrations";
/// A HTTP-hívások nevei: ezek nem a fogalom részei.
const HTTP_CALLS: &[&str] = &[
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "head",
    "options",
    "getJson",
    "postJson",
    "putJson",
    "patchJson",
    "deleteJson",
];
/// A `Blueprint` metódusok, amelyek első argumentuma nem oszlopnév.
const NOT_COLUMN: &[&str] = &[
    "default",
    "comment",
    "constrained",
    "references",
    "on",
    "onDelete",
    "onUpdate",
    "charset",
    "collation",
    "storedAs",
    "virtualAs",
    "after",
];

/// A régi és az új szó egyes és többes alakja, kisbetűvel.
struct Forms {
    old: String,
    old_plural: String,
    new: String,
    new_plural: String,
}

fn plural(word: &str) -> String {
    let b = word.as_bytes();
    let consonant_before_y =
        b.len() > 1 && !matches!(b[b.len() - 2], b'a' | b'e' | b'i' | b'o' | b'u');
    if word.ends_with('y') && consonant_before_y {
        format!("{}ies", &word[..word.len() - 1])
    } else if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|e| word.ends_with(e))
    {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

/// `post` vagy `person/people` → `(egyes, többes)`, kisbetűvel.
fn parse_word(raw: &str) -> GenResult<(String, String)> {
    let (one, many) = match raw.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (raw.trim(), None),
    };
    let valid = |w: &str| !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphabetic());
    if !valid(one) || many.is_some_and(|m| !valid(m)) {
        return Err(GenError::new(crate::tf!(
            "{} is not a plain word (letters only, optionally `singular/plural`)",
            "a(z) {} nem egyszerű szó (csak betű, esetleg `egyes/többes`)",
            raw
        )));
    }
    let one = one.to_ascii_lowercase();
    let many = many.map_or_else(|| plural(&one), str::to_ascii_lowercase);
    Ok((one, many))
}

impl Forms {
    fn parse(old: &str, new: &str) -> GenResult<Self> {
        let (old, old_plural) = parse_word(old)?;
        let (new, new_plural) = parse_word(new)?;
        if old == new {
            return Err(GenError::new(
                crate::t!(
                    "the new word equals the old one",
                    "az új szó megegyezik a régivel"
                )
                .to_owned(),
            ));
        }
        Ok(Self {
            old,
            old_plural,
            new,
            new_plural,
        })
    }

    /// A szó új alakja, ha ez a fogalom (az írásmód megmarad); `None`: más szó.
    fn map(&self, word: &str) -> Option<String> {
        let lower = word.to_ascii_lowercase();
        let target = if lower == self.old {
            &self.new
        } else if lower == self.old_plural {
            &self.new_plural
        } else {
            return None;
        };
        let mut chars = target.chars();
        let first = chars.next()?;
        Some(if word.as_bytes()[0].is_ascii_uppercase() {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        } else {
            target.clone()
        })
    }

    fn is_plural(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case(&self.old_plural)
    }
}

/// Egy betűsor szavai (camelCase-határokon): `(kezdet, vég)` a sorra vetítve.
fn split_run(run: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..run.len() {
        let (p, c) = (run[i - 1], run[i]);
        let next_lower = run.get(i + 1).is_some_and(u8::is_ascii_lowercase);
        if (p.is_ascii_lowercase() && c.is_ascii_uppercase())
            || (p.is_ascii_uppercase() && c.is_ascii_uppercase() && next_lower)
        {
            out.push((start, i));
            start = i;
        }
    }
    out.push((start, run.len()));
    out
}

/// Egy találat: a szó helye, az új alakja, és a betűsor (azonosító) helye.
struct Hit {
    start: usize,
    end: usize,
    new: String,
    run: (usize, usize),
}

/// A szöveg összes találata (a csupa nagybetűs szó kimarad).
fn hits(text: &str, forms: &Forms) -> Vec<Hit> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let from = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        for (ws, we) in split_run(&b[from..i]) {
            let word = &text[from + ws..from + we];
            let shouting = word.len() > 1 && word.bytes().all(|c| c.is_ascii_uppercase());
            if !shouting && let Some(new) = forms.map(word) {
                out.push(Hit {
                    start: from + ws,
                    end: from + we,
                    new,
                    run: (from, i),
                });
            }
        }
    }
    out
}

/// `$this->post(`, `form.post(`, `Route::post(`: HTTP-hívás, nem a fogalom.
fn is_http_call(text: &str, run: (usize, usize)) -> bool {
    let name = &text[run.0..run.1];
    if !HTTP_CALLS.contains(&name) || !text[run.1..].starts_with('(') {
        return false;
    }
    let before = &text[..run.0];
    before.ends_with("->") || before.ends_with("::") || before.ends_with('.')
}

/// Pontosan `'post'` / `"post"` (a teljes sztring egyetlen egyes szám): lehet HTTP-ige, ezért csak jelzés.
fn is_bare_quoted(text: &str, hit: &Hit, forms: &Forms) -> bool {
    let (before, after) = (&text[..hit.run.0], &text[hit.run.1..]);
    let found = ["'", "\""]
        .iter()
        .find(|q| before.ends_with(**q) && after.starts_with(**q));
    let Some(quote) = found else {
        return false;
    };
    // csak érték-helyzet (`'method' => 'post'`, `method="post"`, `method: 'post'`) lehet HTTP-ige;
    // a tömbkulcs és a hívás-argumentum (`'post' => $post`, `route('post')`) az átnevezendő fogalom
    let head = before[..before.len() - quote.len()].trim_end();
    hit.run == (hit.start, hit.end)
        && !forms.is_plural(&text[hit.start..hit.end])
        && (head.ends_with("=>") || head.ends_with('=') || head.ends_with(':'))
}

fn scan(rel: &str, text: &str, forms: &Forms, plan: &mut Plan) {
    for hit in hits(text, forms) {
        if is_http_call(text, hit.run) {
            continue;
        }
        if is_bare_quoted(text, &hit, forms) {
            plan.uncertain.push(Uncertain {
                rel: rel.to_owned(),
                line: line_of(text, hit.start),
                text: line_snippet(text, hit.start),
            });
            continue;
        }
        plan.sites
            .push(Site::replace(rel, hit.start, hit.end, hit.new));
    }
}

/// A szöveg minden találata cserélve (HTTP-védelem nélkül: útvonalakhoz, táblanevekhez).
fn rewrite(text: &str, forms: &Forms) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for hit in hits(text, forms) {
        out.push_str(&text[at..hit.start]);
        out.push_str(&hit.new);
        at = hit.end;
    }
    out.push_str(&text[at..]);
    out
}

// ------------------------------------------------------------ séma

/// Egy `Schema::create` / `Schema::table` blokk: a tábla neve és a törzs tartománya.
struct Block {
    table: String,
    start: usize,
    end: usize,
}

/// A `from` utáni első idézett literál tartalmának `(kezdet, vég)` helye.
fn quoted(text: &str, from: usize) -> Option<(usize, usize)> {
    let rest = &text[from..];
    let open = from + (rest.len() - rest.trim_start().len());
    let quote = *text.as_bytes().get(open)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let close = text[open + 1..].find(quote as char)? + open + 1;
    Some((open + 1, close))
}

fn blocks(text: &str) -> Vec<Block> {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for needle in ["Schema::create(", "Schema::table("] {
        starts.extend(
            text.match_indices(needle)
                .map(|(at, _)| (at, at + needle.len())),
        );
    }
    starts.sort_unstable();
    let mut out = Vec::new();
    for (n, (at, after)) in starts.iter().enumerate() {
        let Some((a, z)) = quoted(text, *after) else {
            continue;
        };
        out.push(Block {
            table: text[a..z].to_owned(),
            start: *at,
            end: starts.get(n + 1).map_or(text.len(), |s| s.0),
        });
    }
    out
}

/// Az oszlopnév-jelöltek egy blokkban: `->method('lit'` első argumentumai, snake_case, a fogalommal.
fn columns(text: &str, block: &Block, forms: &Forms, tables: &BTreeSet<String>) -> Vec<String> {
    let body = &text[block.start..block.end];
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = body[i..].find(['\'', '"']) {
        let open = i + off;
        let Some((a, z)) = quoted(body, open) else {
            break;
        };
        i = z + 1;
        let lit = &body[a..z];
        let snake = !lit.is_empty()
            && lit
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_alphanumeric() || b == b'_');
        if !snake || tables.contains(lit) || rewrite(lit, forms) == lit {
            continue;
        }
        let Some(call) = body[..open].trim_end().strip_suffix('(') else {
            continue;
        };
        let name_len = call
            .bytes()
            .rev()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        let name = &call[call.len() - name_len..];
        if call[..call.len() - name_len].ends_with("->") && !NOT_COLUMN.contains(&name) {
            out.push(lit.to_owned());
        }
    }
    out
}

/// Az átnevező migráció a meglévő migrációk táblái / oszlopai alapján (ha van mit átnevezni).
fn migration(env: Env<'_>, forms: &Forms, plan: &mut Plan) {
    let dir = env.root.join(MIGRATIONS);
    let files: Vec<(String, String)> = walk(env.fs, &dir, &["php"])
        .into_iter()
        .filter_map(|(rel, path)| {
            env.fs
                .read(&path)
                .ok()
                .map(|text| (format!("{MIGRATIONS}/{rel}"), text))
        })
        .collect();
    let mut tables: BTreeSet<String> = BTreeSet::new();
    for (_, text) in &files {
        tables.extend(blocks(text).into_iter().map(|b| b.table));
    }
    let renamed: Vec<String> = tables
        .iter()
        .filter(|t| rewrite(t, forms) != **t)
        .cloned()
        .collect();
    let mut cols: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_, text) in &files {
        for block in blocks(text) {
            for col in columns(text, &block, forms, &tables) {
                cols.entry(block.table.clone()).or_default().insert(col);
            }
        }
    }
    if renamed.is_empty() && cols.is_empty() {
        return;
    }
    let last = files
        .iter()
        .filter_map(|(rel, _)| {
            rel.rsplit('/')
                .next()
                .map(|n| n.chars().take(17).collect::<String>())
        })
        .max()
        .unwrap_or_default();
    let name = format!(
        "{}_rename_{}_to_{}.php",
        prefix(&stamp(env.clock.now_secs()), &last),
        forms.old,
        forms.new
    );
    let rel = format!("{MIGRATIONS}/{name}");
    plan.notes.push(crate::tf!(
        "⚠ schema: new migration {} ({} tables, {} columns); the old migrations are left as they were, run `php artisan migrate`",
        "⚠ séma: új migráció: {} ({} tábla, {} oszlop); a régi migrációk érintetlenek, futtasd a `php artisan migrate`-et",
        rel,
        renamed.len(),
        cols.values().map(BTreeSet::len).sum::<usize>()
    ));
    plan.created.push(GroupFile {
        rel,
        text: migration_text(forms, &renamed, &cols),
    });
}

/// `YYYY_MM_DD_HHMMSS` a `yyyymmddThhmmss` bélyegből, a meglévő migrációk utánra tolva.
fn prefix(ts: &str, last: &str) -> String {
    let now = format!("{}_{}_{}_{}", &ts[0..4], &ts[4..6], &ts[6..8], &ts[9..15]);
    if now.as_str() > last {
        return now;
    }
    let bumped = last
        .rsplit('_')
        .next()
        .and_then(|n| n.parse::<u32>().ok())
        .map_or(0, |n| n + 1);
    format!("{}{bumped:06}", &last[..last.len().min(11)])
}

fn migration_text(
    forms: &Forms,
    renamed: &[String],
    cols: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let column_lines = |reverse: bool| -> String {
        let mut s = String::new();
        for (table, list) in cols {
            // up: a táblák átnevezése előtt, down: a táblák visszanevezése után: mindkétszer a régi név
            s.push_str(&format!(
                "        Schema::table('{table}', function (Blueprint $table) {{\n"
            ));
            for col in list {
                let (from, to) = (col.clone(), rewrite(col, forms));
                let (from, to) = if reverse { (to, from) } else { (from, to) };
                s.push_str(&format!(
                    "            $table->renameColumn('{from}', '{to}');\n"
                ));
            }
            s.push_str("        });\n");
        }
        s
    };
    let table_lines = |reverse: bool| -> String {
        renamed
            .iter()
            .map(|t| {
                let (from, to) = (t.clone(), rewrite(t, forms));
                let (from, to) = if reverse { (to, from) } else { (from, to) };
                format!("        Schema::rename('{from}', '{to}');\n")
            })
            .collect()
    };
    // up: előbb az oszlopok (régi táblanéven), aztán a táblák; down: előbb a táblák vissza, aztán az oszlopok
    format!(
        "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{{\n    public function up(): void\n    {{\n{}{}    }}\n\n    public function down(): void\n    {{\n{}{}    }}\n}};\n",
        column_lines(false),
        table_lines(false),
        table_lines(true),
        column_lines(true)
    )
}

/// A terv a fogalom-átnevezéshez: `(a jelentés tárgya, a terv)`.
pub(super) fn plan(env: Env<'_>, req: &RenameRequest) -> GenResult<(String, Plan)> {
    let forms = Forms::parse(&req.old, &req.new)?;
    let mut plan = Plan::default();
    for (rel, path) in walk(env.fs, env.root, EXTS) {
        if rel.starts_with(&format!("{MIGRATIONS}/")) {
            continue;
        }
        let Ok(text) = env.fs.read(&path) else {
            continue;
        };
        scan(&rel, &text, &forms, &mut plan);
        let moved = rewrite(&rel, &forms);
        if moved != rel {
            // a 0 hosszú szerkesztés a fájlt a tervbe veszi akkor is, ha a szövegében nincs találat
            plan.sites.push(Site::insert(&rel, 0, ""));
            plan.moves.push((rel, moved));
        }
    }
    migration(env, &forms, &mut plan);
    Ok((format!("term {} → {}", forms.old, forms.new), plan))
}
