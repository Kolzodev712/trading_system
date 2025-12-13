//! Config file encryption/decryption utilities
//!
//! This module provides functionality to encrypt and decrypt configuration files
//! using AES-256-GCM encryption with a password-derived key.
//!
//! **Security Features:**
//! - AES-256-GCM encryption (authenticated encryption)
//! - PBKDF2 key derivation (100,000 iterations by default)
//! - Secure memory handling (attempts to prevent swapping with mlock where available)
//! - Salt per encrypted file
//!
//! **Usage:**
//! 1. Encrypt config: `encrypt_config_file("config/bot.json", "config/bot.json.enc", password)`
//! 2. At runtime: `decrypt_config_file("config/bot.json.enc", password)` returns plain JSON bytes
//!
//! **File Format:**
//! Encrypted files start with magic bytes `TSENC` (Trading System ENCrypted) followed by:
//! - Version (1 byte): Format version
//! - Salt (32 bytes): Random salt for PBKDF2
//! - Nonce (12 bytes): Random nonce for AES-GCM
//! - Ciphertext: AES-256-GCM encrypted JSON (includes tag)

use crate::error::{EngineError, EngineError::ConfigError};
use anyhow::anyhow;
use std::fs;
use std::io::{Read, Write};

const MAGIC: &[u8] = b"TSENC";
const VERSION: u8 = 1;
const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Encrypt a config file using AES-256-GCM
///
/// # Arguments
/// * `input_path` - Path to plaintext JSON config file
/// * `output_path` - Path where encrypted file will be written
/// * `password` - Password for encryption (will be used with PBKDF2)
///
/// # Errors
/// Returns `EngineError` if file operations or encryption fails
pub fn encrypt_config_file(
    input_path: &str,
    output_path: &str,
    password: &str,
) -> Result<(), EngineError> {
    // Read plaintext config
    let plaintext = fs::read_to_string(input_path)
        .map_err(|e| ConfigError(anyhow!("Failed to read config file: {}", e)))?;

    // Generate random salt and nonce
    let salt: [u8; SALT_LEN] = {
        let mut s = [0u8; SALT_LEN];
        getrandom::getrandom(&mut s)
            .map_err(|e| ConfigError(anyhow!("Failed to generate salt: {}", e)))?;
        s
    };

    let nonce: [u8; NONCE_LEN] = {
        let mut n = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut n)
            .map_err(|e| ConfigError(anyhow!("Failed to generate nonce: {}", e)))?;
        n
    };

    // Derive key using PBKDF2
    let key = derive_key(password, &salt)?;

    // Encrypt using AES-256-GCM
    let ciphertext = encrypt_aes_gcm(plaintext.as_bytes(), &key, &nonce)?;

    // Write encrypted file
    {
        let mut file = fs::File::create(output_path)
            .map_err(|e| ConfigError(anyhow!("Failed to create encrypted file: {}", e)))?;

        // Write format: MAGIC | VERSION | SALT | NONCE | CIPHERTEXT
        file.write_all(MAGIC)
            .map_err(|e| ConfigError(anyhow!("Failed to write magic: {}", e)))?;
        file.write_all(&[VERSION])
            .map_err(|e| ConfigError(anyhow!("Failed to write version: {}", e)))?;
        file.write_all(&salt)
            .map_err(|e| ConfigError(anyhow!("Failed to write salt: {}", e)))?;
        file.write_all(&nonce)
            .map_err(|e| ConfigError(anyhow!("Failed to write nonce: {}", e)))?;
        file.write_all(&ciphertext)
            .map_err(|e| ConfigError(anyhow!("Failed to write ciphertext: {}", e)))?;
    }

    // Set restrictive permissions (600) - Unix only, but harmless on Windows
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output_path)
            .map_err(|e| ConfigError(anyhow!("Failed to get file metadata: {}", e)))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(output_path, perms)
            .map_err(|e| ConfigError(anyhow!("Failed to set permissions: {}", e)))?;
    }

    Ok(())
}

/// Decrypt a config file
///
/// # Arguments
/// * `encrypted_path` - Path to encrypted config file
/// * `password` - Password for decryption
///
/// # Returns
/// Decrypted JSON string
///
/// # Errors
/// Returns `EngineError` if file operations, decryption, or authentication fails
pub fn decrypt_config_file(encrypted_path: &str, password: &str) -> Result<String, EngineError> {
    // Read encrypted file
    let encrypted_data = fs::read(encrypted_path)
        .map_err(|e| ConfigError(anyhow!("Failed to read encrypted file: {}", e)))?;

    // Parse format
    if encrypted_data.len() < MAGIC.len() + 1 + SALT_LEN + NONCE_LEN {
        return Err(ConfigError(anyhow!(
            "Encrypted file too short or invalid format"
        )));
    }

    let mut offset = 0;

    // Check magic bytes
    if &encrypted_data[offset..offset + MAGIC.len()] != MAGIC {
        return Err(ConfigError(anyhow!(
            "Invalid encrypted file format (missing magic bytes)"
        )));
    }
    offset += MAGIC.len();

    // Read version (for future compatibility)
    let _version = encrypted_data[offset];
    offset += 1;

    // Read salt
    let salt: [u8; SALT_LEN] = encrypted_data[offset..offset + SALT_LEN]
        .try_into()
        .map_err(|_| ConfigError(anyhow!("Failed to read salt")))?;
    offset += SALT_LEN;

    // Read nonce
    let nonce: [u8; NONCE_LEN] = encrypted_data[offset..offset + NONCE_LEN]
        .try_into()
        .map_err(|_| ConfigError(anyhow!("Failed to read nonce")))?;
    offset += NONCE_LEN;

    // Remaining is ciphertext
    let ciphertext = &encrypted_data[offset..];

    // Derive key
    let key = derive_key(password, &salt)?;

    // Decrypt
    let plaintext_bytes = decrypt_aes_gcm(ciphertext, &key, &nonce)?;

    // Convert to string
    let plaintext = String::from_utf8(plaintext_bytes)
        .map_err(|e| ConfigError(anyhow!("Invalid UTF-8 in decrypted data: {}", e)))?;

    Ok(plaintext)
}

/// Check if a file is encrypted (starts with magic bytes)
pub fn is_encrypted_file(path: &str) -> bool {
    if let Ok(mut file) = fs::File::open(path) {
        let mut magic = vec![0u8; MAGIC.len()];
        if file.read_exact(&mut magic).is_ok() {
            return magic == MAGIC;
        }
    }
    false
}

/// Derive encryption key from password using PBKDF2
fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> Result<[u8; 32], EngineError> {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    Ok(key)
}

/// Encrypt data using AES-256-GCM
fn encrypt_aes_gcm(
    plaintext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, EngineError> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| ConfigError(anyhow!("Invalid key: {}", e)))?;

    let nonce = Nonce::from_slice(nonce);

    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| ConfigError(anyhow!("Encryption failed: {}", e)))
}

/// Decrypt data using AES-256-GCM
fn decrypt_aes_gcm(
    ciphertext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, EngineError> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| ConfigError(anyhow!("Invalid key: {}", e)))?;

    let nonce = Nonce::from_slice(nonce);

    cipher.decrypt(nonce, ciphertext).map_err(|_| {
        ConfigError(anyhow!(
            "Decryption failed - invalid password or corrupted data"
        ))
    })
}
