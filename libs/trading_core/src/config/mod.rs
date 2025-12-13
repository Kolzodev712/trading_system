//! Configuration utilities and encryption

pub mod channel;
pub mod encryption;

// Re-export for convenience
pub use channel::ChannelConfig;
pub use encryption::{decrypt_config_file, encrypt_config_file, is_encrypted_file};
