use axum_extra::extract::cookie::Key as PrivateCookieKey;
use base64::engine::general_purpose::STANDARD;
use base64::{DecodeError, Engine};

/// Decode a master key in base64 into an Axum cookie key
pub fn encode_master_key(key: axum_extra::extract::cookie::Key) -> String {
    let master_key = key.master();
    STANDARD.encode(master_key)
}

/// Decode a master key in base64 into an Axum cookie key
pub fn decode_master_key(
    master_key: impl Into<String>,
) -> core::result::Result<PrivateCookieKey, DecodeError> {
    let master_key = master_key.into();
    match STANDARD.decode(master_key) {
        Ok(k) => Ok(PrivateCookieKey::derive_from(k.as_slice())),
        Err(error) => Err(error),
    }
}
