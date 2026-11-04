//! Az if-ágak sortartománya (nikic `stmtsRange`): első statement kezdő sora, utolsó záró sora.

use mago_span::HasSpan;
use mago_syntax::cst::{Statement, TriviaKind};

use super::Collector;

impl Collector<'_> {
    /// Statement-lista `[startLine, endLine]`-je; üres listánál a fallback sor. `until` a lezáró
    /// token (`}`, `elseif`, `endif` ...) offsetje: a közé eső záró kommentek is az ághoz tartoznak.
    pub(super) fn range<'s>(
        &self,
        stmts: impl Iterator<Item = &'s Statement<'s>>,
        fallback: u32,
        until: Option<u32>,
    ) -> (u32, u32) {
        // a `?>` / `<?php` címkék nikic-nál nem statementek; a puszta újsor pedig nem is statement
        let all: Vec<&Statement<'_>> = stmts.collect();
        let v: Vec<(usize, &Statement<'_>)> = all
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| match s {
                Statement::ClosingTag(_) | Statement::OpeningTag(_) => false,
                Statement::Inline(_) => !self.inline_text(s).is_empty(),
                _ => true,
            })
            .collect();
        let (Some(&(fi, first)), Some(&(_, last))) = (v.first(), v.last()) else {
            return (fallback, fallback);
        };
        let close = fi
            .checked_sub(1)
            .and_then(|i| matches!(all[i], Statement::ClosingTag(_)).then(|| all[i].span().start));
        let start = self.start_line(first, close.map(|p| p.offset));
        let mut end = match last {
            Statement::Inline(_) => self.parsed.line_of(last.span().end.offset),
            // a `?>`-vel lezárt statement a címke utáni újsorral együtt ér véget (nikic)
            _ if self.parsed.text(last.span()).ends_with("?>") => {
                self.parsed.line_of(last.span().end.offset + 1)
            }
            _ => self.parsed.lines_of(last).1,
        };
        if let Some(u) = until {
            end = end.max(self.trailing_comment_line(last.span().end.offset, u));
        }
        (start, end)
    }

    /// Az utolsó komment záró sora a `[from, until]` sávban (0, ha nincs).
    fn trailing_comment_line(&self, from: u32, until: u32) -> u32 {
        let trivia = &self.parsed.program.trivia.nodes;
        let first = trivia.partition_point(|t| t.span.start.offset < from);
        trivia[first..]
            .iter()
            .take_while(|t| t.span.end.offset <= until)
            .filter(|t| !matches!(t.kind, TriviaKind::WhiteSpace))
            .last()
            .map_or(0, |t| {
                self.parsed.line_of(t.span.end.offset.saturating_sub(1))
            })
    }

    /// Az inline HTML szövege a vezető (a `?>` után elnyelt) újsor nélkül.
    fn inline_text(&self, s: &Statement<'_>) -> String {
        let t = self.parsed.text(s.span());
        let t = t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n'));
        t.map_or_else(|| self.parsed.text(s.span()), str::to_string)
    }

    /// Kezdő sor; az inline HTML a `?>` utáni újsor után kezdődik, kivéve ha a `?>` előtt komment áll
    /// (a nikic ilyenkor a `?>` sorát adja).
    fn start_line(&self, s: &Statement<'_>, close_tag: Option<u32>) -> u32 {
        let (start, _) = self.parsed.lines_of(s);
        let Statement::Inline(_) = s else {
            return start;
        };
        if close_tag.is_some_and(|c| self.comment_before(c)) {
            return start;
        }
        let t = self.parsed.text(s.span());
        let skip = if t.starts_with("\r\n") {
            2
        } else {
            u32::from(t.starts_with('\n'))
        };
        self.parsed.line_of(s.span().start.offset + skip).max(start)
    }

    /// Komment áll közvetlenül (csak szóközökkel elválasztva) az adott offset előtt?
    fn comment_before(&self, offset: u32) -> bool {
        let trivia = &self.parsed.program.trivia.nodes;
        let mut pos = offset;
        for t in trivia[..trivia.partition_point(|t| t.span.end.offset <= offset)]
            .iter()
            .rev()
        {
            if t.span.end.offset != pos {
                return false;
            }
            if !matches!(t.kind, TriviaKind::WhiteSpace) {
                return true;
            }
            pos = t.span.start.offset;
        }
        false
    }

    /// Egy ág törzse: blokknál a belső statementek, egyébként maga az egy statement.
    pub(super) fn stmt_range(&self, stmt: &Statement<'_>, fallback: u32) -> (u32, u32) {
        match stmt {
            Statement::Block(b) => self.range(
                b.statements.iter(),
                fallback,
                Some(b.right_brace.start.offset),
            ),
            other => self.range(std::iter::once(other), fallback, None),
        }
    }
}
