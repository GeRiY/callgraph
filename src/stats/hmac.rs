//! HMAC-SHA256 (RFC 2104) a meglévő `sha2` crate-tel, új függőség nélkül, és a telepítésenkénti véletlen kulcs.
//! Csak a helyi munkamenet-állapot (`session.rs`) használja: a célfájl / mappa / projekt kulcsolt hash-e kizárólag
//! a `rel` / `rep` jelzők kiszámításához kell, a gépet soha nem hagyja el (sem a kulcs, sem a hash).

use sha2::{Digest, Sha256};

/// Az SHA-256 blokkmérete bájtban.
const BLOCK: usize = 64;

/// SHA-256 egy bájtsorra, 32 bájtos tömbként.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    for (o, b) in out.iter_mut().zip(digest.iter()) {
        *o = *b;
    }
    out
}

/// HMAC-SHA256(key, message).
pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    // a blokknál hosszabb kulcs helyett a hash-e
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(BLOCK + message.len());
    inner.extend(block.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(message);
    let inner_hash = sha256(&inner);
    let mut outer = Vec::with_capacity(BLOCK + 32);
    outer.extend(block.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

/// Bájtok kisbetűs hexában.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hexa → bájtok; érvénytelen szövegnél `None`.
pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A kulcsolt hash szöveg-alakja (hexa): ez kerül a helyi `session.json`-ba.
pub(crate) fn keyed_hex(key: &[u8], text: &str) -> String {
    to_hex(&hmac_sha256(key, text.as_bytes()))
}

/// Új, 32 bájtos véletlen kulcs. Nincs külön véletlen-forrás crate: a szabványos könyvtár véletlen hash-kulcsai
/// (`RandomState`, amit az OS véletlenforrása táplál — ugyanaz, mint a telemetria kötegazonosítójánál), és mellé
/// az idő (ns), a folyamat-azonosító és egy veremváltozó címe, mindez SHA-256-on át.
pub(crate) fn random_key() -> [u8; 32] {
    use std::hash::{BuildHasher, Hasher};
    let mut seed = Vec::with_capacity(64);
    for at in 0..4u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(at);
        seed.extend_from_slice(&h.finish().to_le_bytes());
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    seed.extend_from_slice(&nanos.to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    let local = 0u8;
    seed.extend_from_slice(&(std::ptr::addr_of!(local) as usize).to_le_bytes());
    sha256(&seed)
}
