//! AES-GCM encrypted credential storage.
//!
//! Uses `XChaCha20Poly1305` for authenticated encryption of sensitive values
//! (e.g. Steam passwords). The encryption key is stored as a raw binary file
//! on the local filesystem.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::XChaCha20Poly1305;
use std::path::{Path, PathBuf};

/// Length of the `XChaCha20Poly1305` nonce in bytes.
const NONCE_LEN: usize = 24;

/// Length of the secret key in bytes.
const KEY_LEN: usize = 32;

/// Errors that can occur during encryption or decryption operations.
#[derive(Debug)]
pub enum EncryptionError {
    /// The secret key file could not be read or generated.
    KeyError(std::io::Error),
    /// Decryption failed (wrong key or corrupted ciphertext).
    DecryptionFailed(chacha20poly1305::aead::Error),
    /// Decrypted bytes are not valid UTF-8.
    InvalidUtf8(std::string::FromUtf8Error),
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyError(e) => write!(f, "secret key error: {e}"),
            Self::DecryptionFailed(e) => write!(f, "decryption failed: {e}"),
            Self::InvalidUtf8(e) => write!(f, "decrypted value is not valid UTF-8: {e}"),
        }
    }
}

impl std::error::Error for EncryptionError {}

/// Loads and caches the local secret key used for encryption.
#[derive(Clone)]
pub struct EncryptionKey {
    key: [u8; KEY_LEN],
}

impl EncryptionKey {
    /// Load the secret key from the given path.
    ///
    /// # Errors
    /// Returns [`EncryptionError::KeyError`] if the file cannot be read
    /// or does not contain exactly 32 bytes.
    pub fn load(path: &Path) -> Result<Self, EncryptionError> {
        let data = std::fs::read(path).map_err(EncryptionError::KeyError)?;
        if data.len() != KEY_LEN {
            return Err(EncryptionError::KeyError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "secret key file must be exactly {KEY_LEN} bytes, got {}",
                    data.len()
                ),
            )));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&data);
        Ok(Self { key })
    }

    /// Encrypt a plaintext value and return `(nonce, ciphertext)`.
    ///
    /// A fresh 24-byte random nonce is generated for each encryption
    /// call to ensure uniqueness.
    ///
    /// # Errors
    /// Returns [`EncryptionError::DecryptionFailed`] if the AEAD
    /// operation fails (extremely unlikely with `OsRng`).
    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
        let cipher = XChaCha20Poly1305::new(&self.key.into());
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom(&mut nonce_bytes)?;
        let ciphertext = cipher
            .encrypt(&nonce_bytes.into(), plaintext.as_bytes())
            .map_err(EncryptionError::DecryptionFailed)?;
        Ok((nonce_bytes.to_vec(), ciphertext))
    }

    /// Decrypt a ciphertext using the provided nonce.
    ///
    /// # Errors
    /// Returns [`EncryptionError::DecryptionFailed`] if authentication
    /// fails (wrong key or tampered ciphertext).
    /// Returns [`EncryptionError::InvalidUtf8`] if the decrypted bytes
    /// are not valid UTF-8.
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<String, EncryptionError> {
        if nonce.len() != NONCE_LEN {
            return Err(EncryptionError::DecryptionFailed(
                chacha20poly1305::aead::Error,
            ));
        }
        let mut nonce_array = [0u8; NONCE_LEN];
        nonce_array.copy_from_slice(nonce);
        let cipher = XChaCha20Poly1305::new(&self.key.into());
        let plaintext_bytes = cipher
            .decrypt(&nonce_array.into(), ciphertext)
            .map_err(EncryptionError::DecryptionFailed)?;
        String::from_utf8(plaintext_bytes).map_err(EncryptionError::InvalidUtf8)
    }
}

/// Generate a random 32-byte secret key and write it to the given path.
///
/// The parent directory is created if it does not exist.
/// This function is idempotent in practice — callers should check
/// whether the file exists before calling.
///
/// # Errors
/// Returns [`EncryptionError::KeyError`] if the file cannot be written.
pub fn generate_secret_key(path: &Path) -> Result<PathBuf, EncryptionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(EncryptionError::KeyError)?;
    }
    let mut key_bytes = [0u8; KEY_LEN];
    getrandom(&mut key_bytes)?;
    std::fs::write(path, key_bytes).map_err(EncryptionError::KeyError)?;
    Ok(path.to_path_buf())
}

/// Fill a buffer with cryptographically secure random bytes.
fn getrandom(buf: &mut [u8]) -> Result<(), EncryptionError> {
    getrandom::getrandom(buf).map_err(|e| {
        EncryptionError::KeyError(std::io::Error::other(format!(
            "failed to generate random bytes: {e}"
        )))
    })
}
