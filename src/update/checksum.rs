//! A letöltött csomag ellenőrzése: SHA-256 (`sha2`, tiszta Rust) és a kiadás `SHA256SUMS` fájlja
//! (`sha256sum` formátum: soronként `<hex>  <fájlnév>`, a bináris jelölés `<hex> *<fájlnév>` is elfogadott).

use sha2::{Digest, Sha256};

/// A kiadás ellenőrzőösszeg-fájljának neve (release-asset, a zipek mellett).
pub(crate) const SUMS_FILE: &str = "SHA256SUMS";

/// A `SHA256SUMS` felső mérete (néhány sor).
pub(crate) const SUMS_LIMIT: u64 = 64 * 1024;

/// A bájtok SHA-256-ja kisbetűs hexában (64 karakter).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Érvényes SHA-256 hexa-e (64 hexa számjegy); kisbetűsítve adja vissza.
pub(crate) fn normalize_hex(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| text.to_ascii_lowercase())
}

/// A `SHA256SUMS` szövegéből a `name` fájl várt összege; `None`, ha nincs rá (érvényes) sor.
pub(crate) fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hex, file) = line.trim_end().split_once(' ')?;
        let file = file.strip_prefix([' ', '*']).unwrap_or(file);
        if file == name {
            normalize_hex(hex)
        } else {
            None
        }
    })
}

/// Egy cím / út melletti fájl címe (az utolsó `/` vagy `\` utáni rész cseréje); elválasztó nélkül maga a név.
pub(crate) fn sibling(location: &str, name: &str) -> String {
    match location.rfind(['/', '\\']) {
        Some(i) => format!("{}{name}", &location[..=i]),
        None => name.to_owned(),
    }
}

/// A cím / út utolsó tagja (a fájlnév).
pub(crate) fn file_name(location: &str) -> &str {
    location.rsplit(['/', '\\']).next().unwrap_or(location)
}

/// Az ellenőrzés eltérése: várt és tényleges összeg.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mismatch {
    pub(crate) expected: String,
    pub(crate) actual: String,
}

/// A bájtok összevetése a várt (kisbetűs hexa) összeggel.
pub(crate) fn verify(bytes: &[u8], expected: &str) -> Result<(), Mismatch> {
    let actual = sha256_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(Mismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn known_vectors() {
        assert_eq!(sha256_hex(b""), EMPTY);
        assert_eq!(sha256_hex(b"abc"), ABC);
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn parses_sha256sum_lines() {
        let sums = format!(
            "{EMPTY}  callgraph-0.2.0.tar.gz\n{}  callgraph-linux-x86_64.zip\r\n{ABC} *callgraph-windows-x86_64.zip\nbroken line\n",
            ABC.to_uppercase()
        );
        assert_eq!(
            expected_sum(&sums, "callgraph-linux-x86_64.zip").as_deref(),
            Some(ABC)
        );
        assert_eq!(
            expected_sum(&sums, "callgraph-windows-x86_64.zip").as_deref(),
            Some(ABC)
        );
        assert_eq!(
            expected_sum(&sums, "callgraph-0.2.0.tar.gz").as_deref(),
            Some(EMPTY)
        );
        assert_eq!(expected_sum(&sums, "callgraph-darwin-arm64.zip"), None);
        assert_eq!(expected_sum("xyz  a.zip", "a.zip"), None);
        assert_eq!(normalize_hex(" ABC "), None);
        assert_eq!(normalize_hex(&ABC.to_uppercase()).as_deref(), Some(ABC));
    }

    #[test]
    fn verifies_and_reports_mismatch() {
        assert_eq!(verify(b"abc", ABC), Ok(()));
        assert_eq!(
            verify(b"abd", ABC),
            Err(Mismatch {
                expected: ABC.into(),
                actual: sha256_hex(b"abd"),
            })
        );
    }

    #[test]
    fn sibling_and_file_name_of_urls_and_paths() {
        let url = "https://h/o/r/releases/download/v0.2.0/callgraph-linux-x86_64.zip";
        assert_eq!(
            sibling(url, SUMS_FILE),
            "https://h/o/r/releases/download/v0.2.0/SHA256SUMS"
        );
        assert_eq!(file_name(url), "callgraph-linux-x86_64.zip");
        assert_eq!(sibling(r"C:\dl\cg.zip", SUMS_FILE), r"C:\dl\SHA256SUMS");
        assert_eq!(file_name(r"C:\dl\cg.zip"), "cg.zip");
        assert_eq!(sibling("cg.zip", SUMS_FILE), SUMS_FILE);
        assert_eq!(file_name("cg.zip"), "cg.zip");
    }
}
