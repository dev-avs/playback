//! Electron's safeStorage on Windows, which is Chromium's os_crypt: one
//! AES-256-GCM key per profile, itself sealed with DPAPI, in the profile's
//! `Local State` (`os_crypt.encrypted_key`, base64 of "DPAPI" + the sealed
//! key); every secret is "v10" + a 12-byte nonce + ciphertext and tag.
//!
//! Playback's data folder is the Electron build's profile, so secrets it
//! sealed open here, and secrets sealed here open in it, which keeps them
//! across the switch to this build and a revert to an older release.

use crate::support::paths::data_file;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

const PREFIX: &[u8] = b"v10";
const NONCE_LEN: usize = 12;

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Copies DPAPI's answer out and frees it.
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
    let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
    bytes
}

/// Sealed for this Windows user only.
pub fn dpapi_seal(plain: &[u8]) -> Option<Vec<u8>> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&blob(plain), None, None, None, None, 0, &mut out).ok()?;
        Some(take(out))
    }
}

pub fn dpapi_open(sealed: &[u8]) -> Option<Vec<u8>> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&blob(sealed), None, None, None, None, 0, &mut out).ok()?;
        Some(take(out))
    }
}

/// The profile's key, or None before Electron ever made one.
fn profile_key() -> Option<Aes256Gcm> {
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data_file("Local State")).ok()?).ok()?;
    let sealed = B64
        .decode(state["os_crypt"]["encrypted_key"].as_str()?)
        .ok()?;
    let key = dpapi_open(sealed.strip_prefix(b"DPAPI")?)?;
    Aes256Gcm::new_from_slice(&key).ok()
}

pub fn open(sealed: &[u8]) -> Option<String> {
    open_with(&profile_key()?, sealed)
}

/// None when there is no profile key to seal with.
pub fn seal(plain: &str) -> Option<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).ok()?;
    seal_with(&profile_key()?, plain, &nonce)
}

fn open_with(key: &Aes256Gcm, sealed: &[u8]) -> Option<String> {
    let rest = sealed.strip_prefix(PREFIX)?;
    if rest.len() < NONCE_LEN {
        return None;
    }
    let (nonce, cipher) = rest.split_at(NONCE_LEN);
    String::from_utf8(key.decrypt(Nonce::from_slice(nonce), cipher).ok()?).ok()
}

fn seal_with(key: &Aes256Gcm, plain: &str, nonce: &[u8; NONCE_LEN]) -> Option<Vec<u8>> {
    let cipher = key
        .encrypt(Nonce::from_slice(nonce), plain.as_bytes())
        .ok()?;
    Some([PREFIX, nonce, &cipher].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_in_chromiums_format() {
        let key = Aes256Gcm::new_from_slice(&[7u8; 32]).unwrap();
        let sealed = seal_with(&key, "sl_sk_secret", &[1u8; NONCE_LEN]).unwrap();
        assert_eq!(&sealed[..3], b"v10");
        assert_eq!(&sealed[3..15], &[1u8; NONCE_LEN]);
        assert_eq!(sealed.len(), 3 + NONCE_LEN + "sl_sk_secret".len() + 16);
        assert_eq!(open_with(&key, &sealed).as_deref(), Some("sl_sk_secret"));
        assert_eq!(open_with(&key, b"v10short"), None);
    }

    #[test]
    fn dpapi_opens_what_it_sealed() {
        let sealed = dpapi_seal(b"hello").unwrap();
        assert_ne!(sealed, b"hello");
        assert_eq!(dpapi_open(&sealed).unwrap(), b"hello");
    }
}
