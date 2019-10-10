use failure::ResultExt;
use ring::aead;

const PREFIX: &'static str = "daylog.1";

fn base64_config() -> base64::Config {
    base64::Config::new(base64::CharacterSet::UrlSafe, false)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::decode_config(s, base64_config())
}

fn base64_encode(bytes: &[u8]) -> String {
    base64::encode_config(bytes, base64_config())
}

pub fn is_our_message_id(s: &str) -> bool {
    s.starts_with(PREFIX)
}

pub fn gen_message_id(username: &str, date: &str, key_bytes: [u8; 32]) -> Result<String, failure::Error> {
    let plaintext = format!("{}.{}", username, date);

    let key = aead_key(key_bytes);
    let nonce = TimeNonce::new();

    let mut encrypted = plaintext.into_bytes();
    key.seal_in_place_append_tag(nonce.as_aead(), ring::aead::Aad::from(PREFIX.as_bytes()), &mut encrypted).unwrap();

    Ok(format!("{}.{}.{}", PREFIX, nonce.base64(), base64_encode(&encrypted)))
}

pub fn verify_message_id(message_id: &str, key_bytes: [u8; 32]) -> Result<(String, String), failure::Error> {
    let mut parts = message_id.split('.');
    let mut extract = || parts.next().ok_or_else(|| failure::err_msg("not enough parts"));

    let ident = extract()?;
    let ver = extract()?;
    let nonce_base64 = extract()?;
    let encrypted_base64 = extract()?;
    if parts.next().is_some() {
        return Err(failure::err_msg("too many parts"));
    }

    let prefix = format!("{}.{}", ident, ver);
    if prefix != PREFIX {
        return Err(failure::err_msg("unrecognized prefix"));
    }

    let nonce = TimeNonce::parse(nonce_base64)
        .context("invalid nonce base64")?;

    let mut encrypted = base64_decode(encrypted_base64)
        .context("invalid encrypted base64")?;

    let key = aead_key(key_bytes);
    let decrypted = key.open_in_place(nonce.as_aead(), aead::Aad::from(prefix.as_bytes()), &mut encrypted)
        .map_err(|_| failure::err_msg("failed to validate encrypted data"))?;

    // get the parts in reverse order and limit to 2, in case username contains a '.'
    let mut result_parts = decrypted.rsplitn(2, |b| *b == b'.');
    let mut extract_result = || -> Result<String, failure::Error> {
        result_parts.next()
            .ok_or_else(|| failure::err_msg("not enough result parts"))
            .map(Vec::from)
            .and_then(|vec| {
                String::from_utf8(vec)
                    .map_err(|e| {
                        failure::err_msg(format!("invalid utf-8 in decrypted content: {}", e))
                    })
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
        use std::convert::TryInto;
        // take nanos as little-endian bytes, and use the low-order 12 bytes for the nonce
        let array: [u8; 12] = (&self.nanos.to_le_bytes()[0..12]).try_into().unwrap();
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

    pub fn parse(s: &str) -> Result<Self, failure::Error> {
        use std::convert::TryInto;
        let mut bytes = base64_decode(s)
            .context("invalid base64 for nonce")?;
        bytes.resize(16, 0);
        let nanos = u128::from_le_bytes((&bytes[..]).try_into().unwrap());
        Ok(Self { nanos })
    }
}

fn aead_key(key_bytes: [u8; 32]) -> aead::LessSafeKey {
    use ring::aead::*;
    let algorithm = &CHACHA20_POLY1305;
    LessSafeKey::new(UnboundKey::new(algorithm, &key_bytes)
        .expect("failed to make key"))
}
