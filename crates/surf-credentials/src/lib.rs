//! Read-only R2 credentials embedded at build time.
//!
//! See `build.rs` for how values are supplied. This crate has no knowledge of
//! storage clients; it only hands back the decoded values.

use std::fmt;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// The credentials compiled into this binary.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("account_id", &self.account_id)
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field("bucket", &self.bucket)
            .finish()
    }
}

/// Decode the embedded credentials.
pub fn embedded() -> Credentials {
    Credentials {
        account_id: unmask(&ACCOUNT_ID, &KEY),
        access_key_id: unmask(&ACCESS_KEY_ID, &KEY),
        secret_access_key: unmask(&SECRET_ACCESS_KEY, &KEY),
        bucket: unmask(&BUCKET, &KEY),
    }
}

fn unmask(data: &[u8], key: &[u8]) -> String {
    let bytes: Vec<u8> = data
        .iter()
        .zip(key.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    String::from_utf8(bytes).expect("embedded credentials are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmask_round_trips() {
        let key = [7u8, 42, 255];
        let masked: Vec<u8> = b"secret"
            .iter()
            .zip(key.iter().cycle())
            .map(|(b, k)| b ^ k)
            .collect();
        assert_eq!(unmask(&masked, &key), "secret");
    }

    #[test]
    fn embedded_values_decode() {
        let c = embedded();
        assert!(!c.account_id.is_empty());
        assert!(!c.bucket.is_empty());
    }

    #[test]
    fn debug_redacts_secrets() {
        let c = embedded();
        let dbg = format!("{c:?}");
        assert!(!dbg.contains(&c.secret_access_key));
        assert!(dbg.contains("<redacted>"));
    }
}
