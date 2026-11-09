//! Részleges elemzés végzetes szintaktikai hibánál: a forrás egyre rövidebb előtagjai, amelyeket a nyelv újra
//! megpróbálhat (`prefix_cuts`), és a hiba utáni részek visszanyerése (`recover_segments`): a hibás szakasz
//! szóközökre cserélve (az újsorok maradnak), így a bájt-eltolások és a sorszámok az eredetiek maradnak.

/// Legfeljebb ennyi előtagot próbál a hívó (a hibás sor eleje + néhány korábbi határ).
const MAX_CUTS: usize = 4;

/// Legfeljebb ennyi hibás szakaszt ugrik át a `recover_segments` (utána csak az addig ép rész marad).
const MAX_SEGMENTS: usize = 4;

/// Végzetes szintaktikai hiba utáni helyreállítás. Az ép előtag (`prefix_cuts`, amelyet a `fatal_at` elfogad)
/// után a hibás sort követő első felső szintű utasítástól (`next_statement`: a `starts` valamelyikével vagy üres
/// sor után kezdődő sor) folytatja: a kettő közti szakaszt szóközökre cseréli, és az egészet újra próbálja;
/// újabb hibánál ugyanígy, legfeljebb `MAX_SEGMENTS`-szer. A `fatal_at` egy jelölt szövegre `None`-t ad, ha
/// elemezhető, különben a (legkorábbi) hiba bájt-eltolását. Az eredmény ugyanolyan hosszú, ugyanannyi soros szöveg,
/// mint a `text`; `None`, ha még az előtag sem elemezhető, vagy semmi nem marad meg.
pub fn recover_segments(
    text: &str,
    first_error: usize,
    starts: &[&str],
    mut fatal_at: impl FnMut(&str) -> Option<usize>,
) -> Option<String> {
    let mut current = text.to_owned();
    let mut error = first_error;
    let mut best = None;
    for _ in 0..MAX_SEGMENTS {
        let Some(cut) = kept_prefix(&current, error, &mut fatal_at) else {
            break;
        };
        let resume = next_statement(&current, error, starts);
        let candidate = blank(&current, cut, resume.unwrap_or(current.len()));
        let Some(next) = fatal_at(&candidate) else {
            // csupa szóköz: nincs mit megtartani
            return if candidate.trim().is_empty() {
                best
            } else {
                Some(candidate)
            };
        };
        // tartalék: az eddig ép rész (az előtag elemezhető volt, a záró szóközök nem rontják el)
        best = Some(blank(&current, cut, current.len()));
        match resume {
            Some(resume) if next >= resume => {
                current = candidate;
                error = next;
            }
            _ => break,
        }
    }
    best
}

/// Az elfogadott előtag vége: a hibás sor eleje, ha előtte csak szóköz van, különben az első `prefix_cuts`-vágás,
/// amelyet a `fatal_at` elemezhetőnek talál.
fn kept_prefix(
    text: &str,
    error: usize,
    fatal_at: &mut impl FnMut(&str) -> Option<usize>,
) -> Option<usize> {
    let error = char_floor(text, error);
    let line = line_start(text, error);
    if text[..line].trim().is_empty() {
        return Some(line);
    }
    prefix_cuts(text, error)
        .into_iter()
        .find(|&cut| fatal_at(&text[..cut]).is_none())
}

/// A hibát tartalmazó sor utáni első felső szintű utasítás eleje: a legkisebb behúzású (a szöveg nem üres sorai
/// közül) sor, amely a `starts` egyikével kezdődik (betűvel végződő elemnél szóhatárral, pl. `export`, `fn`;
/// másnál puszta előtagként, pl. `@`, `#[`), vagy üres sor után betűvel / `_` / `$` / `@` / `#` kezdődik.
fn next_statement(text: &str, error_offset: usize, starts: &[&str]) -> Option<usize> {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()?;
    let error = char_floor(text, error_offset);
    let mut at = text[error..].find('\n').map(|i| error + i + 1)?;
    let mut blank_before = false;
    while at < text.len() {
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        let line = &text[at..end];
        let body = line.trim_start();
        if body.is_empty() {
            blank_before = true;
        } else {
            let top = line.len() - body.len() == indent;
            if top && (starts_with_any(body, starts) || (blank_before && plain_start(body))) {
                return Some(at);
            }
            blank_before = false;
        }
        at = end + 1;
    }
    None
}

/// Üres sor utáni utasítás-kezdet: betű, `_`, `$`, `@` vagy `#`.
fn plain_start(body: &str) -> bool {
    body.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$' | '@' | '#'))
}

/// A sor egy kezdő elemmel indul-e (betűvel / számjeggyel végződő elemnél szóhatárral).
fn starts_with_any(body: &str, starts: &[&str]) -> bool {
    starts.iter().any(|start| {
        body.strip_prefix(start).is_some_and(|rest| {
            !start.ends_with(|c: char| c.is_alphanumeric())
                || !rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
        })
    })
}

/// A `[start, end)` szakasz szóközökre cserélve (az újsorok maradnak; több bájtos karakter helyén annyi szóköz),
/// így a szöveg hossza és a sorok száma változatlan.
fn blank(text: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    for c in text[start..end].chars() {
        if matches!(c, '\n' | '\r') {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        }
    }
    out.push_str(&text[end..]);
    out
}

/// Az `offset` a szövegen belülre és karakterhatárra igazítva (lefelé).
fn char_floor(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// A próbálható vágási pontok (bájt-eltolás, szigorúan csökkenő sorrendben): először a hibát tartalmazó sor eleje,
/// utána a korábbi határok — üres sor utáni sor eleje, vagy egy 0. oszlopban kezdődő, nem záró-jeles sor eleje
/// (felső szintű deklaráció). Csak a nem üres (nem csupa szóköz) előtagok kerülnek bele.
pub fn prefix_cuts(text: &str, error_offset: usize) -> Vec<usize> {
    let offset = char_floor(text, error_offset);
    let mut cuts = Vec::new();
    let first = line_start(text, offset);
    if first > 0 {
        cuts.push(first);
    }
    let mut at = first;
    while cuts.len() < MAX_CUTS && at > 0 {
        let Some(previous) = previous_boundary(text, at) else {
            break;
        };
        cuts.push(previous);
        at = previous;
    }
    cuts.retain(|&cut| !text[..cut].trim().is_empty());
    cuts.dedup();
    cuts
}

/// A szövegrész vége a hiba helyéhez: a záró szóközök (pl. a fájl végi újsor) előtti utolsó tartalom, hogy a
/// „váratlan fájlvég” a hiányos sorra, ne az utána következő üres sorra mutasson.
pub fn clamp_to_content(text: &[u8], offset: usize) -> usize {
    let last = text
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    offset.min(last)
}

/// Az `offset`-et tartalmazó sor eleje.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |i| i + 1)
}

/// Az `at` (egy sor eleje) előtti legközelebbi határ: üres sor utáni sor, vagy felső szintű sor eleje.
fn previous_boundary(text: &str, at: usize) -> Option<usize> {
    let mut end = at.checked_sub(1)?; // az előző sor záró újsora
    loop {
        let start = line_start(text, end);
        if start == 0 {
            return None;
        }
        let line = &text[start..end];
        let before = &text[line_start(text, start - 1)..start - 1];
        let top_level = line
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, '}' | ')' | ']'));
        if !line.trim().is_empty() && (top_level || before.trim().is_empty()) {
            return Some(start);
        }
        end = start - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_cut_is_the_error_line_then_earlier_boundaries() {
        let text = "fn a() {}\nfn b() {\n  x\n}\nfn c( {\n";
        let error = text.find("c(").unwrap();
        let cuts = prefix_cuts(text, error);
        assert_eq!(&text[..cuts[0]], "fn a() {}\nfn b() {\n  x\n}\n");
        assert_eq!(&text[..cuts[1]], "fn a() {}\n");
        assert_eq!(cuts.len(), 2);
    }

    #[test]
    fn error_on_first_line_gives_no_cut() {
        assert!(prefix_cuts("fn c( {\n", 3).is_empty());
    }

    /// Teszt-„parser”: a `!` tartalmú sor hibás (az első ilyen bájt-eltolása), a többi elemezhető.
    fn bang(text: &str) -> Option<usize> {
        text.find('!')
    }

    #[test]
    fn segments_skip_the_broken_statement_and_keep_offsets() {
        let text = "fn a() {}\nfn b(!) {}\nfn c() {}\n";
        let out = recover_segments(text, text.find('!').unwrap(), &["fn"], bang).unwrap();
        assert_eq!(out, "fn a() {}\n          \nfn c() {}\n");
        assert_eq!(out.len(), text.len());
    }

    #[test]
    fn segments_handle_two_errors_and_an_error_on_the_first_line() {
        let text = "fn a(!) {}\nfn b() {}\nfn c(!) {}\nfn d() {}\n";
        let out = recover_segments(text, 5, &["fn"], bang).unwrap();
        assert_eq!(out.lines().collect::<Vec<_>>()[1], "fn b() {}");
        assert_eq!(out.lines().collect::<Vec<_>>()[3], "fn d() {}");
        assert!(!out.contains('!') && out.lines().count() == 4);
    }

    #[test]
    fn segments_fall_back_to_the_prefix_without_a_next_statement() {
        let text = "fn a() {}\nfn b(!) {\n  x\n";
        let out = recover_segments(text, text.find('!').unwrap(), &["fn"], bang).unwrap();
        assert!(out.starts_with("fn a() {}\n") && out.trim_end() == "fn a() {}");
    }

    #[test]
    fn next_statement_uses_the_smallest_indent_and_word_boundaries() {
        let text = "  a(\n    exported\n  exports\n\n  foo()\n";
        assert_eq!(next_statement(text, 0, &["export"]), text.find("  foo"));
        let text = "x(\n@Dec\nclass A {}\n";
        assert_eq!(next_statement(text, 0, &["@"]), text.find("@Dec"));
    }

    #[test]
    fn clamp_moves_end_of_file_to_last_content() {
        let text = b"a\nb( {\n";
        assert_eq!(clamp_to_content(text, text.len()), 5);
        assert_eq!(clamp_to_content(text, 1), 1);
    }
}
