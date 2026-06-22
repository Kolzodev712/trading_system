//! Load Ed25519 signing key from environment.

use std::env;

use sui_crypto::ed25519::Ed25519PrivateKey;
use sui_crypto::SignatureError;
use sui_sdk_types::Address;
use trading_core::venue::dex::DexError;

/// Signing credentials for Sui transaction submission.
pub struct SuiSigner {
    key: Ed25519PrivateKey,
    address: Address,
}

impl SuiSigner {
    pub fn from_env(var: &str) -> Result<Self, DexError> {
        let raw = env::var(var).map_err(|_| {
            DexError::InvalidConfig(
                format!("missing signing key env var `{var}` (required for DeepBook execution)")
                    .into(),
            )
        })?;
        Self::from_secret(&raw)
    }

    pub fn from_secret(raw: &str) -> Result<Self, DexError> {
        let key = parse_private_key(raw.trim())?;
        let address = key.public_key().derive_address();
        Ok(Self { key, address })
    }

    pub fn key(&self) -> &Ed25519PrivateKey {
        &self.key
    }

    pub fn address(&self) -> Address {
        self.address
    }
}

fn parse_private_key(raw: &str) -> Result<Ed25519PrivateKey, DexError> {
    if raw.starts_with("suiprivkey") {
        return Ed25519PrivateKey::from_suiprivkey(raw).map_err(map_sig_err);
    }
    if let Some(hex) = raw.strip_prefix("0x") {
        return decode_hex_key(hex);
    }
    if raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return decode_hex_key(raw);
    }
    Err(DexError::InvalidConfig(
        "SUI private key must be suiprivkey, 0x-hex, or 64-char hex".into(),
    ))
}

fn decode_hex_key(hex: &str) -> Result<Ed25519PrivateKey, DexError> {
    let bytes = hex::decode(hex)
        .map_err(|e| DexError::InvalidConfig(format!("invalid hex private key: {e}").into()))?;
    if bytes.len() != 32 {
        return Err(DexError::InvalidConfig(
            format!("ed25519 private key must be 32 bytes, got {}", bytes.len()).into(),
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Ed25519PrivateKey::new(arr))
}

fn map_sig_err(e: SignatureError) -> DexError {
    DexError::InvalidConfig(format!("invalid SUI private key: {e}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_64_char_hex_key() {
        let hex = "a".repeat(64);
        let signer = SuiSigner::from_secret(&hex).unwrap();
        assert_ne!(signer.address(), Address::ZERO);
    }
}
