//! A kifejezés tokenizálója (az Interpreter minta első lépése). Két üzemmódja van, mert a `+` / `-` / `#` elem
//! elején láthatóság-előtag, elem után operátor: `item()` elemet vár (szó vagy `(`), `op()` operátort vagy a
//! szóhoz tartozó `{…}` / `[…]` / `*n` toldalékot.
//!
//! Egy „szó” (elem-fej) nem tartalmaz szóközt, kivéve zárójelek / `<…>` / idézőjelek között, és a `:` / `=` / `,`
//! körül. A `{…}` nyers törzs és a `[…]` lista külön token. Operátorok: `>` gyerek, `+` testvér, `^` fel,
//! `*n` ismétlés, `( )` csoport; az operátor nélküli új sor testvér.

use super::error::{GenError, GenResult};

/// Az ismétlés felső korlátja (elszabadult `*100000` ellen).
pub const MAX_REPEAT: u32 = 200;

/// Egy token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Word(String),
    Raw(String),
    List(String),
    Open,
    Close,
    Child,
    Sibling,
    NewlineSibling,
    Up,
    Repeat(u32),
    End,
}

/// A tokenizáló: karaktertömb és pozíció.
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    start: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            start: 0,
        }
    }

    /// Az utolsó token kezdő oszlopa (1-alapú) a hibaüzenetekhez.
    pub fn token_col(&self) -> usize {
        self.start + 1
    }

    /// Hiba a `pos` oszlopnál.
    pub fn error_at(&self, col: usize, message: &str) -> GenError {
        GenError::new(crate::tf!(
            "expression error at col {}: {}",
            "kifejezés-hiba a(z) {}. oszlopnál: {}",
            col,
            message
        ))
    }

    fn error(&self, message: &str) -> GenError {
        self.error_at(self.pos + 1, message)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// Szóközök átugrása; igaz, ha új sor is volt köztük.
    fn skip_ws(&mut self) -> bool {
        let mut newline = false;
        while let Some(c) = self.peek() {
            if !c.is_whitespace() {
                break;
            }
            newline |= c == '\n';
            self.pos += 1;
        }
        newline
    }

    /// A következő elem: szó vagy `(`; a lista végén `End`.
    pub fn item(&mut self) -> GenResult<Tok> {
        self.skip_ws();
        self.start = self.pos;
        match self.peek() {
            None => Ok(Tok::End),
            Some('(') => {
                self.pos += 1;
                Ok(Tok::Open)
            }
            Some(c @ (')' | '>' | '^' | '*' | '{' | '[')) => Err(self.error(&crate::tf!(
                "expected an element, found `{}`",
                "elemet várok, de ez jött: `{}`",
                c
            ))),
            Some(_) => Ok(Tok::Word(self.word()?)),
        }
    }

    /// Elem utáni token: toldalék (`{…}`, `[…]`, `*n`), operátor, `)` vagy a vége.
    pub fn op(&mut self) -> GenResult<Tok> {
        let newline = self.skip_ws();
        self.start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Tok::End);
        };
        match c {
            '>' => self.single(Tok::Child),
            '^' => self.single(Tok::Up),
            ')' => self.single(Tok::Close),
            '*' => self.repeat(),
            '+' => {
                // új sor után a `+x` láthatóság-előtag (implicit testvér); a `+ ` operátor
                let spaced = self.peek_at(1).is_none_or(char::is_whitespace);
                if newline && !spaced {
                    Ok(Tok::NewlineSibling)
                } else {
                    self.single(Tok::Sibling)
                }
            }
            '{' => {
                let open = self.pos;
                self.skip_balanced('{', '}')?;
                let inner: String = self.chars[open + 1..self.pos - 1].iter().collect();
                Ok(Tok::Raw(dedent(&inner)))
            }
            '[' => {
                let open = self.pos;
                self.skip_balanced('[', ']')?;
                let inner: String = self.chars[open + 1..self.pos - 1].iter().collect();
                Ok(Tok::List(inner))
            }
            _ if newline => Ok(Tok::NewlineSibling),
            other => Err(self.error(&crate::tf!(
                "expected `+`, `>` or `^` between elements, found `{}` (an element head has no spaces: `+charge(Order $o):Receipt`)",
                "elemek közé `+`, `>` vagy `^` kell, de ez jött: `{}` (az elem-fejben nincs szóköz: `+charge(Order $o):Receipt`)",
                other
            ))),
        }
    }

    /// A `^` utáni token: újabb `^`, `+`, a vége / `)`, különben az elem közvetlenül jön (nem fogyaszt).
    pub fn after_up(&mut self) -> GenResult<Tok> {
        self.skip_ws();
        self.start = self.pos;
        match self.peek() {
            Some('^') => self.single(Tok::Up),
            Some('+') if self.peek_at(1).is_none_or(char::is_whitespace) => self.single(Tok::Sibling),
            None | Some(')') => self.op(),
            _ => Ok(Tok::NewlineSibling),
        }
    }

    fn single(&mut self, tok: Tok) -> GenResult<Tok> {
        self.pos += 1;
        Ok(tok)
    }

    fn repeat(&mut self) -> GenResult<Tok> {
        self.pos += 1;
        let from = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let digits: String = self.chars[from..self.pos].iter().collect();
        match digits.parse::<u32>() {
            Ok(n) if (1..=MAX_REPEAT).contains(&n) => Ok(Tok::Repeat(n)),
            _ => Err(self.error(&crate::tf!(
                "`*` needs a count between 1 and {} (e.g. `*3`)",
                "a `*` után 1 és {} közti szám kell (pl. `*3`)",
                MAX_REPEAT
            ))),
        }
    }

    /// Az `open`-nel kezdődő blokk végéig (a záró jel után) lép; idézőjeleket és megjegyzéseket átugrik.
    fn skip_balanced(&mut self, open: char, close: char) -> GenResult<()> {
        let from = self.pos;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            match c {
                '\'' | '"' => self.skip_string(c)?,
                '/' if self.peek_at(1) == Some('/') => self.skip_line(),
                '#' if open == '{' && self.peek_at(1) != Some('[') => self.skip_line(),
                '/' if self.peek_at(1) == Some('*') => self.skip_block_comment(),
                c if c == open => {
                    depth += 1;
                    self.pos += 1;
                }
                c if c == close => {
                    depth = depth.saturating_sub(1);
                    self.pos += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => self.pos += 1,
            }
        }
        Err(self.error_at(
            from + 1,
            &crate::tf!("unclosed `{}`", "lezáratlan `{}`", open),
        ))
    }

    fn skip_string(&mut self, quote: char) -> GenResult<()> {
        let from = self.pos;
        self.pos += 1;
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == '\\' {
                self.pos += 1;
            } else if c == quote {
                return Ok(());
            }
        }
        Err(self.error_at(
            from + 1,
            crate::t!("unclosed string", "lezáratlan idézőjel"),
        ))
    }

    fn skip_line(&mut self) {
        while self.peek().is_some_and(|c| c != '\n') {
            self.pos += 1;
        }
    }

    fn skip_block_comment(&mut self) {
        self.pos += 2;
        while let Some(c) = self.peek() {
            if c == '*' && self.peek_at(1) == Some('/') {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }

    /// Egy elem-fej: a végéig a legfelső szinten nem szóköz / operátor karakterek.
    fn word(&mut self) -> GenResult<String> {
        let from = self.pos;
        let list_form = self.chars[from..].starts_with(&['u', 's', 'e', ':']);
        if matches!(self.peek(), Some('+' | '-' | '#')) {
            self.pos += 1;
        }
        let mut angle = 0usize;
        while let Some(c) = self.peek() {
            let prev = self.pos.checked_sub(1).map(|p| self.chars[p]);
            match c {
                '(' => self.skip_balanced('(', ')')?,
                '\'' | '"' => self.skip_string(c)?,
                '[' if prev == Some('=') => self.skip_balanced('[', ']')?,
                '{' | '[' => break,
                '<' => {
                    if prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '\\') {
                        angle += 1;
                    }
                    self.pos += 1;
                }
                '>' if angle > 0 && prev != Some('=') => {
                    angle -= 1;
                    self.pos += 1;
                }
                '>' | '+' | '^' | '*' | ')' => break,
                c if c.is_whitespace() => {
                    if angle > 0 || self.continues_over_space(list_form) {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                _ => self.pos += 1,
            }
        }
        let text: String = self.chars[from..self.pos].iter().collect();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(self.error(crate::t!("empty element", "üres elem")));
        }
        Ok(text)
    }

    /// A szóköz után is ugyanahhoz az elemhez tartozik-e a szöveg (`charge() : Receipt`, `$x = 5`, `use:A, B as C`).
    fn continues_over_space(&self, list_form: bool) -> bool {
        let mut j = self.pos;
        while self.chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        let Some(&next) = self.chars.get(j) else {
            return false;
        };
        let prev = self.pos.checked_sub(1).map(|p| self.chars[p]);
        if matches!(prev, Some(':' | '=' | ',')) || matches!(next, ':' | '=') {
            return true;
        }
        let after_as = self.pos >= 3 && self.chars[self.pos - 3..self.pos] == [' ', 'a', 's'];
        list_form && (after_as || self.chars[j..].starts_with(&['a', 's', ' ']))
    }
}

/// A nyers törzs közös behúzásának levétele; az első sor (a `{` után) külön számít, az elején / végén a üres sorok
/// elmaradnak.
pub fn dedent(text: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim();
    let tail: Vec<&str> = lines.collect();
    let indent_of = |l: &str| l.len() - l.trim_start().len();
    let indent = tail
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_of(l))
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = Vec::new();
    if !first.is_empty() {
        out.push(first.to_owned());
    }
    for l in tail {
        if l.trim().is_empty() {
            out.push(String::new());
        } else {
            let cut = l.get(indent.min(indent_of(l))..).unwrap_or(l.trim_start());
            out.push(cut.trim_end().to_owned());
        }
    }
    while out.first().is_some_and(String::is_empty) {
        out.remove(0);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.join("\n")
}
