//! Szövegszerkesztés sorok felett a meglévő `.vue` / `.ts` fájlokhoz: a beszúrt blokkok követése (a hozzájuk
//! tartozó sorszámok a későbbi szerkesztések után is helyesek), a kapcsos zárójeleket követő utasítás-keresés
//! (függvény / `const` neve alapján), behúzás.

/// Egy beszúrt blokk helye (a jelentéshez).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub label: &'static str,
    /// 0-alapú kezdősor.
    pub start: usize,
    pub len: usize,
}

/// A szerkesztett fájl sorai és a beszúrt blokkok.
#[derive(Debug, Clone, Default)]
pub struct Doc {
    pub lines: Vec<String>,
    pub placed: Vec<Placed>,
}

impl Doc {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines,
            placed: Vec::new(),
        }
    }

    /// A `[from, to)` sorok cseréje; a mögötte álló jelölt blokkok sorszáma követi.
    pub fn splice(&mut self, from: usize, to: usize, new: Vec<String>) {
        let to = to.min(self.lines.len());
        let from = from.min(to);
        let removed = to - from;
        let added = new.len();
        self.lines.splice(from..to, new);
        for p in &mut self.placed {
            if p.start >= to {
                p.start = p.start + added - removed;
            }
        }
    }

    /// Beszúrás a `at` indexű sor elé.
    pub fn insert(&mut self, at: usize, block: Vec<String>) {
        self.splice(at, at, block);
    }

    /// Jelölés: a `start`-tól `len` sor a `label` szakasz új tartalma.
    pub fn mark(&mut self, label: &'static str, start: usize, len: usize) {
        self.placed.push(Placed { label, start, len });
    }

    /// A teljes szöveg `\n`-nel (a sorszámokhoz; a valódi sorvéget az író teszi vissza).
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// A sor behúzása (szóközök).
pub fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A fájl behúzás-egysége: 4, ha a legkisebb nem nulla behúzás 4 többszöröse, különben 2 (a generátor 2-vel ír).
pub fn indent_unit(lines: &[String]) -> usize {
    match lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_of(l))
        .filter(|&n| n > 0)
        .min()
    {
        Some(n) if n % 4 == 0 => 4,
        _ => 2,
    }
}

/// A generált (2 szóközös szintekkel írt) blokk átírása a fájl behúzás-egységére (`unit`): szintenként `unit` szóköz.
pub fn reindent(block: &[String], unit: usize) -> Vec<String> {
    if unit == 2 {
        return block.to_vec();
    }
    block
        .iter()
        .map(|l| {
            let n = indent_of(l);
            format!("{}{}", " ".repeat(n / 2 * unit + n % 2), &l[n..])
        })
        .collect()
}

/// A blokk minden nem üres sora `n` szóközzel beljebb.
pub fn indent_block(block: &[String], n: usize) -> Vec<String> {
    let pad = " ".repeat(n);
    block
        .iter()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect()
}

/// Az utolsó nem üres sor a `[from, to)` tartományban.
pub fn last_nonblank(lines: &[String], from: usize, to: usize) -> Option<usize> {
    let to = to.min(lines.len());
    (from..to).rev().find(|&i| !lines[i].trim().is_empty())
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// A sor a `name` nevű függvényt / változót deklarálja-e (`function n(`, `async function n`, `const n =`, `export …`).
fn declares(line: &str, name: &str) -> bool {
    let mut t = line.trim_start();
    for prefix in ["export ", "default ", "async "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim_start();
        }
    }
    for keyword in ["function* ", "function ", "const ", "let ", "var "] {
        if let Some(rest) = t.strip_prefix(keyword) {
            let Some(after) = rest.trim_start().strip_prefix(name) else {
                return false;
            };
            return !after.chars().next().is_some_and(ident_char);
        }
    }
    false
}

/// A `start` sorban kezdődő utasítás utolsó sora (0-alapú, zárt): a zárójelek egyensúlyáig, sztringek és
/// megjegyzések átugrásával; a folytatódó sor (`=`, `+`, `,`, `=>` … végű) nem zár.
pub fn statement_end(lines: &[String], start: usize, limit: usize) -> usize {
    let limit = limit.min(lines.len());
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut comment = false;
    for (i, line) in lines.iter().enumerate().take(limit).skip(start) {
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            let next = chars.get(k + 1).copied();
            if comment {
                if c == '*' && next == Some('/') {
                    comment = false;
                    k += 1;
                }
            } else if let Some(q) = quote {
                if c == '\\' {
                    k += 1;
                } else if c == q {
                    quote = None;
                }
            } else {
                match c {
                    '\'' | '"' | '`' => quote = Some(c),
                    '/' if next == Some('/') => break,
                    '/' if next == Some('*') => {
                        comment = true;
                        k += 1;
                    }
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
            }
            k += 1;
        }
        if matches!(quote, Some('\'' | '"')) {
            quote = None;
        }
        let tail = line.trim_end();
        let continues = ["=", "+", "-", "*", "&&", "||", "?", ":", "=>", ",", "("]
            .iter()
            .any(|op| tail.ends_with(op));
        if depth <= 0 && quote.is_none() && !comment && !continues {
            return i;
        }
    }
    limit.saturating_sub(1).max(start)
}

/// A `name` nevű függvény / változó utasítása a `[from, to)` sorokban: `(első, utolsó)` sor (0-alapú, zárt).
pub fn find_statement(
    lines: &[String],
    name: &str,
    from: usize,
    to: usize,
) -> Option<(usize, usize)> {
    let to = to.min(lines.len());
    let start = (from..to).find(|&i| declares(&lines[i], name))?;
    Some((start, statement_end(lines, start, to)))
}

/// A deklarált (felső szintű) nevek a `[from, to)` sorokban: a hibaüzenetekhez.
pub fn declared_names(lines: &[String], from: usize, to: usize) -> Vec<String> {
    let to = to.min(lines.len());
    let mut out = Vec::new();
    for line in &lines[from.min(to)..to] {
        let mut t = line.trim_start();
        for prefix in ["export ", "default ", "async "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                t = rest.trim_start();
            }
        }
        for keyword in ["function ", "const ", "let ", "var "] {
            if let Some(rest) = t.strip_prefix(keyword) {
                let name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|&c| ident_char(c))
                    .collect();
                if !name.is_empty() {
                    out.push(name);
                }
                break;
            }
        }
    }
    out
}
