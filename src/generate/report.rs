//! A kimeneti sorok sablonjai egy helyen: az `--into` / `--replace` összegzései (a kiírt sor fájlonként) és a `--dry`
//! hely-fejléce. Így az "egy sor / fájl" ígéret és a tokenszám egy modulban látszik és változtatható; a Facade-ok csak a
//! számokat (sorok, hely, címke) adják át.

/// A `--dry` kimenete `--into` / `--replace` célnál: egy hely-fejléc sor, utána a kód — az ágens így látja, hova
/// kerülne, mielőtt ír.
pub fn with_header(header: String, block: Vec<String>) -> Vec<String> {
    let mut lines = Vec::with_capacity(block.len() + 1);
    lines.push(header);
    lines.extend(block);
    lines
}

/// A címke ` (név)` alakban, vagy semmi.
fn label_suffix(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!(" ({label})")
    }
}

/// `→ app/Pay.php::Pay after charge (L41)`: a beszúrás helye és első sora (`--dry`, PHP-tagok).
pub fn insert_header(place: &str, start: usize) -> String {
    format!("→ {place} (L{start})")
}

/// `→ src/A.vue: script L12-14, template L20-25`: a beszúrt szakaszok helye (`--dry`, Vue / HTML).
pub fn sections_header(rel: &str, places: &[String]) -> String {
    format!("→ {rel}: {}", places.join(", "))
}

/// `→ app/Pay.php replace L41-47 → L41-52 (charge)`: a csere helye és új sortartománya (`--dry`).
pub fn replace_header(rel: &str, old: (usize, usize), new: (usize, usize), label: &str) -> String {
    let label = label_suffix(label);
    format!(
        "→ {rel} replace L{}-{} → L{}-{}{label}",
        old.0, old.1, new.0, new.1
    )
}

/// `inserted 2 member(s) into app/Pay.php::Pay after charge (L41-52)`: a PHP `--into` összegzése.
pub fn inserted_members(count: usize, place: &str, start: usize, end: usize) -> String {
    crate::tf!(
        "inserted {} member(s) into {} (L{}-{})",
        "beszúrva {} tag ide: {} (L{}-{})",
        count,
        place,
        start,
        end
    )
}

/// `inserted into src/A.vue: script L12-14, template L20-25`: a Vue / HTML `--into` összegzése.
pub fn inserted_sections<S: AsRef<str>>(rel: &str, places: &[S]) -> String {
    let places: Vec<&str> = places.iter().map(AsRef::as_ref).collect();
    crate::tf!(
        "inserted into {}: {}",
        "beszúrva ide: {}: {}",
        rel,
        places.join(", ")
    )
}

/// `replaced app/Pay.php L41-47 → L41-52 (charge)`: a `--replace` összegzése (`label`: a metódus, vagy üres).
pub fn replaced(rel: &str, old: (usize, usize), new: (usize, usize), label: &str) -> String {
    let label = label_suffix(label);
    crate::tf!(
        "replaced {} L{}-{} → L{}-{}{}",
        "kicserélve: {} L{}-{} → L{}-{}{}",
        rel,
        old.0,
        old.1,
        new.0,
        new.1,
        label
    )
}
