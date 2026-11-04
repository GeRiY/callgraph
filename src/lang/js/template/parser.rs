//! Tűrő template-parser: a `{{ }}`, elemek, kommentek, raw-text elemek felépítik a csomópontfát.

use super::attrs::{is_ws, parse_open_tag};
use super::entities::decode;
use super::node::{Element, Node, Prop};
use crate::lang::js::LineIndex;

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Egy nyitott elem a veremben (a `v-pre` jelzővel).
struct Open {
    el: Element,
    pre: bool,
}

struct Parser<'a> {
    src: &'a str,
    end: usize,
    lines: &'a LineIndex,
    stack: Vec<Open>,
    root: Vec<Node>,
}

/// A `[start, end)` sávban lévő template tartalom csomópontjai.
pub fn parse(src: &str, start: usize, end: usize, lines: &LineIndex) -> Vec<Node> {
    let mut p = Parser {
        src,
        end,
        lines,
        stack: Vec::new(),
        root: Vec::new(),
    };
    p.run(start);
    p.root
}

impl Parser<'_> {
    fn in_pre(&self) -> bool {
        self.stack.iter().any(|o| o.pre)
    }

    fn push(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(o) => o.el.children.push(node),
            None => self.root.push(node),
        }
    }

    fn flush_text(&mut self, from: usize, to: usize) {
        if to > from {
            let raw = self.src[from..to].to_string();
            let line = self.lines.line_of(from);
            self.push(Node::Text {
                line,
                raw,
                decode: true,
            });
        }
    }

    fn run(&mut self, start: usize) {
        let b = self.src.as_bytes();
        let (mut i, mut text) = (start, start);
        while i < self.end {
            match b[i] {
                b'<' => {
                    if let Some(next) = self.markup(i, text) {
                        i = next;
                        text = i;
                    } else {
                        i += 1;
                    }
                }
                b'{' if b.get(i + 1) == Some(&b'{') && !self.in_pre() => {
                    match self.src[i + 2..self.end].find("}}") {
                        Some(rel) => {
                            self.flush_text(text, i);
                            let inner = &self.src[i + 2..i + 2 + rel];
                            let content = decode(
                                inner.trim_matches(|c: char| c.is_ascii() && is_ws(c as u8)),
                            );
                            let line = self.lines.line_of(i);
                            self.push(Node::Interpolation { line, content });
                            i += rel + 4;
                            text = i;
                        }
                        None => i = self.end,
                    }
                }
                _ => i += 1,
            }
        }
        self.flush_text(text, self.end);
        while !self.stack.is_empty() {
            let line = self.lines.line_of(self.end.saturating_sub(1));
            self.close_top(line);
        }
    }

    /// Tag / komment kezelése a `<` helyén; `None`, ha a `<` csak szöveg. A pozíció, ahol folytatni kell.
    fn markup(&mut self, at: usize, text: usize) -> Option<usize> {
        let b = self.src.as_bytes();
        let next = *b.get(at + 1)?;
        if next.is_ascii_alphabetic() {
            self.flush_text(text, at);
            return Some(self.open_tag(at));
        }
        if next == b'/' && b.get(at + 2).is_some_and(|c| c.is_ascii_alphabetic()) {
            self.flush_text(text, at);
            return Some(self.close_tag(at));
        }
        if next == b'!' || next == b'?' {
            self.flush_text(text, at);
            let rest = &self.src[at..self.end];
            let stop = if let Some(body) = rest.strip_prefix("<!--") {
                body.find("-->").map(|r| r + 7)
            } else {
                rest.find('>').map(|r| r + 1)
            };
            return Some(at + stop.unwrap_or(rest.len()));
        }
        None
    }

    fn open_tag(&mut self, at: usize) -> usize {
        let in_pre = self.in_pre();
        let Some(open) = parse_open_tag(self.src, at, self.end, self.lines, in_pre) else {
            return self.end;
        };
        let lower = open.tag.to_ascii_lowercase();
        let pre = open
            .props
            .iter()
            .any(|p| matches!(p, Prop::Dir { name, .. } if name == "pre"));
        let line = self.lines.line_of(at);
        let end_line = self.lines.line_of(open.gt);
        let after = open.gt + 1;
        let is_void = VOID.contains(&open.tag.as_str());
        let el = Element {
            tag: open.tag,
            line,
            end_line,
            props: open.props,
            children: Vec::new(),
        };
        if open.self_closing || is_void {
            self.push(Node::Element(el));
            return after;
        }
        self.stack.push(Open { el, pre });
        if matches!(lower.as_str(), "script" | "style" | "textarea" | "title") {
            return self.raw_text(&lower, after);
        }
        after
    }

    /// RAWTEXT / RCDATA: a záró tagig tartó szöveg egy csomópont.
    fn raw_text(&mut self, name: &str, from: usize) -> usize {
        let hay = self.src[from..self.end].to_ascii_lowercase();
        let pat = format!("</{name}");
        let close = hay
            .match_indices(&pat)
            .map(|(r, _)| r)
            .find(|r| {
                hay.as_bytes()
                    .get(r + pat.len())
                    .is_none_or(|c| is_ws(*c) || *c == b'/' || *c == b'>')
            })
            .map_or(self.end, |r| from + r);
        if close > from {
            if matches!(name, "textarea" | "title") {
                self.rcdata(from, close);
            } else {
                let raw = self.src[from..close].to_string();
                let line = self.lines.line_of(from);
                self.push(Node::Text {
                    line,
                    raw,
                    decode: false,
                });
            }
        }
        close
    }

    /// RCDATA: nincs elem, de az interpoláció él.
    fn rcdata(&mut self, from: usize, to: usize) {
        let (mut i, mut text) = (from, from);
        while let Some(rel) = self.src[i..to].find("{{") {
            let open = i + rel;
            let Some(len) = self.src[open + 2..to].find("}}") else {
                break;
            };
            self.flush_text(text, open);
            let inner = &self.src[open + 2..open + 2 + len];
            self.push(self.interpolation(open, inner));
            i = open + len + 4;
            text = i;
        }
        self.flush_text(text, to);
    }

    fn interpolation(&self, at: usize, inner: &str) -> Node {
        let content = decode(inner.trim_matches(|c: char| c.is_ascii() && is_ws(c as u8)));
        Node::Interpolation {
            line: self.lines.line_of(at),
            content,
        }
    }

    fn close_tag(&mut self, at: usize) -> usize {
        let b = self.src.as_bytes();
        let name_end = (at + 2..self.end).find(|&i| is_ws(b[i]) || b[i] == b'/' || b[i] == b'>');
        let name_end = name_end.unwrap_or(self.end);
        let written = &self.src[at + 2..name_end];
        let name = written.to_ascii_lowercase();
        let gt = (name_end..self.end).find(|&i| b[i] == b'>');
        let after = gt.map_or(self.end, |g| g + 1);
        if VOID.contains(&written) {
            return after;
        }
        let Some(depth) = self
            .stack
            .iter()
            .rposition(|o| o.el.tag.to_ascii_lowercase() == name)
        else {
            return after;
        };
        let explicit = self.lines.line_of(gt.unwrap_or(self.end.saturating_sub(1)));
        let implied = self.lines.line_of(at);
        while self.stack.len() > depth + 1 {
            self.close_top(implied);
        }
        self.close_top(explicit);
        after
    }

    fn close_top(&mut self, end_line: u32) {
        if let Some(mut open) = self.stack.pop() {
            open.el.end_line = end_line;
            self.push(Node::Element(open.el));
        }
    }
}
