use anyhow::{anyhow, bail, Context};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use chrono::NaiveDate;
use ring::aead;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const PREFIX: &str = "daylog.1";
const SECRET_KEY_LEN: usize = 32;

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE.decode(s)
}

fn base64_encode(bytes: &[u8]) -> String {
    URL_SAFE.encode(bytes)
}

pub fn read_secret_key(path: &Path) -> io::Result<[u8; SECRET_KEY_LEN]> {
    let mut key = [0u8; SECRET_KEY_LEN];
    let mut file = File::open(path)?;
    file.read_exact(&mut key)?;
    Ok(key)
}

pub fn is_our_message_id(s: &str) -> bool {
    s.starts_with(PREFIX)
}

pub fn gen_message_id(username: &str, date: NaiveDate, key_bytes: [u8; SECRET_KEY_LEN]) -> anyhow::Result<String> {
    let plaintext = format!("{}.{}", username, date.format("%Y-%m-%d"));

    let key = aead_key(key_bytes);
    let nonce = TimeNonce::new();

    let mut encrypted = plaintext.into_bytes();
    key.seal_in_place_append_tag(nonce.as_aead(), ring::aead::Aad::from(PREFIX.as_bytes()), &mut encrypted).unwrap();

    Ok(format!("{}.{}.{}", PREFIX, nonce.base64(), base64_encode(&encrypted)))
}

pub fn verify_message_id(message_id: &str, key_bytes: [u8; SECRET_KEY_LEN]) -> anyhow::Result<(String, String)> {
    let mut parts = message_id.split('@').next().unwrap().split('.');
    let mut extract = || parts.next().ok_or_else(|| anyhow!("not enough parts"));

    let ident = extract()?;
    let ver = extract()?;
    let nonce_base64 = extract()?;
    let encrypted_base64 = extract()?;
    if parts.next().is_some() {
        bail!("too many parts");
    }

    let prefix = format!("{ident}.{ver}");
    if prefix != PREFIX {
        bail!("unrecognized prefix");
    }

    let nonce = TimeNonce::parse(nonce_base64)
        .context("invalid nonce base64")?;

    let mut encrypted = base64_decode(encrypted_base64)
        .context("invalid encrypted base64")?;

    let key = aead_key(key_bytes);
    let decrypted = key.open_in_place(nonce.as_aead(), aead::Aad::from(prefix.as_bytes()), &mut encrypted)
        .map_err(|_| anyhow!("failed to validate encrypted data"))?;

    // get the parts in reverse order and limit to 2, in case username contains a '.'
    let mut result_parts = decrypted.rsplitn(2, |b| *b == b'.');
    let mut extract_result = || -> anyhow::Result<String> {
        result_parts.next()
            .ok_or_else(|| anyhow!("not enough result parts"))
            .map(Vec::from)
            .and_then(|vec| {
                String::from_utf8(vec)
                    .context("invalid utf-8 in decrypted content")
            })
    };
    let date = extract_result()?;
    let user = extract_result()?;
    Ok((user, date))
}

struct TimeNonce {
    nanos: u128,
}

impl TimeNonce {
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        TimeNonce {
            nanos: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        }
    }

    pub fn as_aead(&self) -> aead::Nonce {
        // take nanos as little-endian bytes, and use the low-order 12 bytes for the nonce
        let array: [u8; 12] = self.nanos.to_le_bytes()[0..12].try_into().unwrap();
        aead::Nonce::assume_unique_for_key(array)
    }

    pub fn base64(&self) -> String {
        // take nanos as little-endian bytes, truncate trailing zeroes, and base64-encode
        let bytes = self.nanos.to_le_bytes();
        let mut end = bytes.len();
        for i in (0 .. bytes.len()).rev() {
            if bytes[i] == 0 {
                end -= 1;
            } else {
                break;
            }
        }
        base64_encode(&bytes[..end])
    }

    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let mut bytes = base64_decode(s)
            .context("invalid base64 for nonce")?;
        bytes.resize(16, 0);
        let nanos = u128::from_le_bytes(bytes[..].try_into().unwrap());
        Ok(Self { nanos })
    }
}

fn aead_key(key_bytes: [u8; SECRET_KEY_LEN]) -> aead::LessSafeKey {
    use ring::aead::*;
    let algorithm = &CHACHA20_POLY1305;
    LessSafeKey::new(UnboundKey::new(algorithm, &key_bytes)
        .expect("failed to make key"))
}
