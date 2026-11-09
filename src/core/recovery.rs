//! Részleges elemzés végzetes szintaktikai hibánál: a forrás egyre rövidebb előtagjai, amelyeket a nyelv újra
//! megpróbálhat. Előtag, ezért a sorszámok az eredetiek maradnak.

/// Legfeljebb ennyi előtagot próbál a hívó (a hibás sor eleje + néhány korábbi határ).
const MAX_CUTS: usize = 4;

/// A próbálható vágási pontok (bájt-eltolás, szigorúan csökkenő sorrendben): először a hibát tartalmazó sor eleje,
/// utána a korábbi határok — üres sor utáni sor eleje, vagy egy 0. oszlopban kezdődő, nem záró-jeles sor eleje
/// (felső szintű deklaráció). Csak a nem üres (nem csupa szóköz) előtagok kerülnek bele.
pub fn prefix_cuts(text: &str, error_offset: usize) -> Vec<usize> {
    let mut offset = error_offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
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

    #[test]
    fn clamp_moves_end_of_file_to_last_content() {
        let text = b"a\nb( {\n";
        assert_eq!(clamp_to_content(text, text.len()), 5);
        assert_eq!(clamp_to_content(text, 1), 1);
    }
}
