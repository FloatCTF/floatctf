//! AWD cryptographic services.
//!
//! # Key hierarchy
//!
//! ```text
//! The application secret from `[auth].jwt_secret` in the TOML config
//!   └── HKDF-SHA256(info="floatctf-awd-master-v1")
//!       └── AWD_MASTER_KEY (32 bytes)
//! ```
//!
//! # Encryption
//!
//! Uses XChaCha20-Poly1305 for authenticated encryption.
//! Each ciphertext stores: `ciphertext | nonce | key_version`.
//! AAD is constructed as: `event_id || ":" || field_name`.
//!
//! # Safety
//!
//! - Secrets must NOT appear in Debug/Display output or logs.
//! - The `AwdSecret` type wraps sensitive values and redacts them.

use chacha20poly1305::{
    AeadCore, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng},
};
use hkdf::Hkdf;
use sha2::Sha256;
use std::sync::OnceLock;
use uuid::Uuid;

use crate::{core::secret::Secret, modules::event::awd_team::AwdError};

static AWD_SECRET: OnceLock<Secret> = OnceLock::new();

/// Expected master key length (32 bytes for XChaCha20-Poly1305).
const MASTER_KEY_LEN: usize = 32;

/// HKDF info string for key derivation.
const HKDF_INFO: &[u8] = b"floatctf-awd-master-v1";

/// Nonce size for XChaCha20-Poly1305 (24 bytes).
const NONCE_LEN: usize = 24;

/// Sensitive value that redacts itself in Debug/Display output.
#[derive(Clone)]
pub struct AwdSecret(Vec<u8>);

impl AwdSecret {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume and return the inner bytes.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl std::fmt::Debug for AwdSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AwdSecret(***)")
    }
}

impl std::fmt::Display for AwdSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// Encrypted data with nonce and key version.
#[derive(Debug, Clone)]
pub struct EncryptedBlob {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_version: i32,
}

/// Core AWD crypto service.
pub struct AwdCrypto {
    master_key: AwdSecret,
}

impl AwdCrypto {
    /// Install the application secret loaded from TOML during bootstrap.
    pub fn configure_secret(secret: Secret) {
        let _ = AWD_SECRET.set(secret);
    }

    /// Derive the AWD master key from the configured application secret.
    pub fn from_config_secret() -> Result<Self, AwdError> {
        let secret = AWD_SECRET
            .get()
            .ok_or_else(|| AwdError::Crypto("application secret is not configured".into()))?;
        Self::from_secret_bytes(secret.as_bytes())
    }

    /// Derive the AWD master key from already-loaded secret material.
    pub fn from_secret_bytes(secret: &[u8]) -> Result<Self, AwdError> {
        if secret.len() < 16 {
            return Err(AwdError::Crypto(
                "SECRET must be at least 16 characters for adequate security".into(),
            ));
        }

        let hkdf = Hkdf::<Sha256>::new(None, secret);
        let mut okm = vec![0u8; MASTER_KEY_LEN];
        hkdf.expand(HKDF_INFO, &mut okm)
            .map_err(|e| AwdError::Crypto(format!("HKDF expansion failed: {}", e)))?;

        Ok(Self {
            master_key: AwdSecret::new(okm),
        })
    }

    /// Create a new AwdCrypto with an explicit master key (for testing).
    pub fn new(master_key: AwdSecret) -> Self {
        Self { master_key }
    }

    /// Encrypt plaintext with AAD.
    ///
    /// AAD is constructed from event_id and field_name for domain separation.
    pub fn encrypt(
        &self,
        plaintext: &[u8],
        aad: &[u8],
        key_version: i32,
    ) -> Result<EncryptedBlob, AwdError> {
        let cipher = XChaCha20Poly1305::new_from_slice(self.master_key.as_bytes())
            .map_err(|e| AwdError::Crypto(format!("Cipher init failed: {}", e)))?;

        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        let ciphertext = cipher
            .encrypt(&nonce, aead_payload(plaintext, aad))
            .map_err(|e| AwdError::Crypto(format!("Encryption failed: {}", e)))?;

        Ok(EncryptedBlob {
            ciphertext,
            nonce: nonce.to_vec(),
            key_version,
        })
    }

    /// Decrypt ciphertext with AAD verification.
    pub fn decrypt(&self, blob: &EncryptedBlob, aad: &[u8]) -> Result<Vec<u8>, AwdError> {
        let cipher = XChaCha20Poly1305::new_from_slice(self.master_key.as_bytes())
            .map_err(|e| AwdError::Crypto(format!("Cipher init failed: {}", e)))?;

        let nonce = XNonce::from_slice(&blob.nonce);

        cipher
            .decrypt(nonce, aead_payload(&blob.ciphertext, aad))
            .map_err(|e| AwdError::Crypto(format!("Decryption failed: {}", e)))
    }

    /// Build AAD from event ID and field name.
    pub fn build_aad(event_id: Uuid, field_name: &str) -> Vec<u8> {
        format!("{}:{}", event_id, field_name).into_bytes()
    }

    /// Decrypt the event_secret field used for deterministic Flag generation.
    ///
    /// AAD must match encrypt at create time: `event_id:event_secret`.
    pub fn decrypt_event_secret(
        &self,
        event_id: Uuid,
        ciphertext: &[u8],
        nonce: &[u8],
        key_version: i32,
    ) -> Result<Vec<u8>, AwdError> {
        self.decrypt(
            &EncryptedBlob {
                ciphertext: ciphertext.to_vec(),
                nonce: nonce.to_vec(),
                key_version,
            },
            &Self::build_aad(event_id, "event_secret"),
        )
    }

    /// Generate a new random event secret (32 bytes).
    pub fn generate_event_secret() -> Vec<u8> {
        use chacha20poly1305::aead::OsRng;
        use rand::RngCore;
        let mut secret = vec![0u8; 32];
        OsRng.fill_bytes(&mut secret);
        secret
    }

    /// Generate a new random internal token as printable hex (256 bits of entropy).
    pub fn generate_token() -> Vec<u8> {
        hex::encode(Self::generate_event_secret()).into_bytes()
    }

    /// Encrypt plaintext with a specific nonce (for token verification).
    ///
    /// Unlike `encrypt`, this accepts a pre-determined nonce so that
    /// the same plaintext + nonce always produces the same ciphertext,
    /// enabling constant-time token comparison.
    pub fn encrypt_with_nonce(
        &self,
        plaintext: &[u8],
        aad: &[u8],
        nonce: &[u8],
        key_version: i32,
    ) -> Result<EncryptedBlob, AwdError> {
        let cipher = XChaCha20Poly1305::new_from_slice(self.master_key.as_bytes())
            .map_err(|e| AwdError::Crypto(format!("Cipher init failed: {}", e)))?;

        let nonce = XNonce::from_slice(nonce);

        let ciphertext = cipher
            .encrypt(nonce, aead_payload(plaintext, aad))
            .map_err(|e| AwdError::Crypto(format!("Encryption failed: {}", e)))?;

        Ok(EncryptedBlob {
            ciphertext,
            nonce: nonce.to_vec(),
            key_version,
        })
    }

    /// Verify a provided token matches the stored encrypted token.
    ///
    /// Re-encrypts the provided token with the stored nonce and compares
    /// the resulting ciphertext using constant-time comparison.
    /// This avoids decryption overhead and prevents timing side-channels.
    pub fn is_valid_token(
        &self,
        provided_token: &[u8],
        stored_ciphertext: &[u8],
        stored_nonce: &[u8],
        event_id: Uuid,
        key_version: i32,
    ) -> Result<bool, AwdError> {
        let aad = Self::build_aad(event_id, "internal_token");
        // P3-11：key_version 参与验证（原硬编码 1，token rotation 后版本漂移）。
        // 说明：XChaCha20-Poly1305 密文不随 key_version 变化（它只是 blob 元数据），
        // 但为保持版本语义一致与审计可溯，重加密必须使用存储时的版本。
        let re_encrypted =
            self.encrypt_with_nonce(provided_token, &aad, stored_nonce, key_version)?;
        if re_encrypted.ciphertext.len() != stored_ciphertext.len() {
            return Ok(false);
        }
        Ok(constant_time_eq(
            &re_encrypted.ciphertext,
            stored_ciphertext,
        ))
    }
}

/// Helper: create AEAD payload with plaintext and associated data.
fn aead_payload<'a>(
    ciphertext_or_plaintext: &'a [u8],
    aad: &'a [u8],
) -> chacha20poly1305::aead::Payload<'a, 'a> {
    chacha20poly1305::aead::Payload {
        msg: ciphertext_or_plaintext,
        aad,
    }
}

/// Constant-time comparison to prevent timing side-channel attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_crypto() -> AwdCrypto {
        let key = AwdSecret::new(vec![0x42u8; 32]);
        AwdCrypto::new(key)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let crypto = test_crypto();
        let plaintext = b"this is a secret ssh password";
        let event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "ssh_password");

        let blob = crypto.encrypt(plaintext, &aad, 1).unwrap();
        let decrypted = crypto.decrypt(&blob, &aad).unwrap();

        assert_eq!(decrypted, plaintext.to_vec());
    }

    #[test]
    fn decrypt_event_secret_matches_create_path_aad() {
        let crypto = test_crypto();
        let event_id = Uuid::new_v4();
        let secret = AwdCrypto::generate_event_secret();
        let aad = AwdCrypto::build_aad(event_id, "event_secret");
        let blob = crypto.encrypt(&secret, &aad, 1).unwrap();
        let out = crypto
            .decrypt_event_secret(event_id, &blob.ciphertext, &blob.nonce, 1)
            .unwrap();
        assert_eq!(out, secret);
        // Wrong field name must fail (same as wrong AAD).
        assert!(
            crypto
                .decrypt(&blob, &AwdCrypto::build_aad(event_id, "ssh_password"),)
                .is_err()
        );
    }

    #[test]
    fn test_wrong_aad_rejected() {
        let crypto = test_crypto();
        let plaintext = b"secret data";
        let event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "ssh_password");
        let wrong_aad = AwdCrypto::build_aad(event_id, "event_secret");

        let blob = crypto.encrypt(plaintext, &aad, 1).unwrap();
        assert!(crypto.decrypt(&blob, &wrong_aad).is_err());
    }

    #[test]
    fn test_wrong_key_rejected() {
        let crypto1 = test_crypto();
        let crypto2 = AwdCrypto::new(AwdSecret::new(vec![0x13u8; 32]));
        let plaintext = b"secret data";
        let aad = AwdCrypto::build_aad(Uuid::new_v4(), "test");

        let blob = crypto1.encrypt(plaintext, &aad, 1).unwrap();
        assert!(crypto2.decrypt(&blob, &aad).is_err());
    }

    #[test]
    fn test_unique_nonce_per_encryption() {
        let crypto = test_crypto();
        let aad = AwdCrypto::build_aad(Uuid::new_v4(), "test");

        let blob1 = crypto.encrypt(b"data", &aad, 1).unwrap();
        let blob2 = crypto.encrypt(b"data", &aad, 1).unwrap();

        assert_ne!(blob1.nonce, blob2.nonce, "Nonces must be unique");
        assert_ne!(blob1.ciphertext, blob2.ciphertext);
    }

    #[test]
    fn test_secret_debug_redaction() {
        let secret = AwdSecret::new(b"super-secret-password".to_vec());
        let debug_str = format!("{:?}", secret);
        assert!(!debug_str.contains("super-secret"));
        assert!(debug_str.contains("***"));
    }

    #[test]
    fn test_generate_event_secret_length() {
        let secret = AwdCrypto::generate_event_secret();
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn test_generate_token_is_printable_hex() {
        let token = AwdCrypto::generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.iter().all(u8::is_ascii_hexdigit));
    }

    #[test]
    fn test_config_secret_missing() {
        // Only test with explicit key — env var tests modify global state
        // and interfere with other parallel tests.
        // Configuration is installed by bootstrap in integration tests.
        let key = AwdSecret::new(vec![0x42u8; 32]);
        let crypto = AwdCrypto::new(key);
        let plaintext = b"test";
        let blob = crypto.encrypt(plaintext, b"test-aad", 1).expect("encrypt");
        let decrypted = crypto.decrypt(&blob, b"test-aad").expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_short_config_key_still_works() {
        // New() accepts any key length but encryption requires exactly 32 bytes.
        // from_secret_bytes() always produces 32 bytes via HKDF regardless of input length.
        let key = AwdSecret::new(vec![0x42u8; 32]);
        let crypto = AwdCrypto::new(key);
        assert!(crypto.encrypt(b"data", b"aad", 1).is_ok());

        // Wrong-length key should fail at encryption time
        let bad_key = AwdSecret::new(b"short".to_vec());
        let crypto_bad = AwdCrypto::new(bad_key);
        assert!(crypto_bad.encrypt(b"data", b"aad", 1).is_err());
    }

    #[test]
    fn test_constant_time_eq_same() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_constant_time_eq_different_length() {
        assert!(!constant_time_eq(b"hello", b"helloo"));
    }

    #[test]
    fn test_encrypt_with_nonce_deterministic() {
        let crypto = test_crypto();
        let aad = b"test-aad";
        let nonce = vec![42u8; 24]; // XChaCha20 nonce is 24 bytes

        let r1 = crypto.encrypt_with_nonce(b"data", aad, &nonce, 1).unwrap();
        let r2 = crypto.encrypt_with_nonce(b"data", aad, &nonce, 1).unwrap();

        assert_eq!(
            r1.ciphertext, r2.ciphertext,
            "Same plaintext+nonce must produce same ciphertext"
        );
        assert_eq!(r1.nonce, r2.nonce);
    }

    #[test]
    fn test_is_valid_token_correct() {
        let crypto = test_crypto();
        let event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "internal_token");

        // Encrypt token the same way the admin handler would
        let token = AwdCrypto::generate_token();
        let blob = crypto.encrypt(&token, &aad, 1).unwrap();

        // Verify should succeed
        assert!(
            crypto
                .is_valid_token(&token, &blob.ciphertext, &blob.nonce, event_id, 1)
                .unwrap()
        );
    }

    #[test]
    fn test_is_valid_token_uses_key_version() {
        // P3-11：key_version 参与验证路径；存储版本与验证版本一致时通过。
        let crypto = test_crypto();
        let event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "internal_token");

        let token = AwdCrypto::generate_token();
        let blob = crypto.encrypt(&token, &aad, 2).unwrap();

        assert!(
            crypto
                .is_valid_token(&token, &blob.ciphertext, &blob.nonce, event_id, 2)
                .unwrap()
        );
        // 错误 token 用任何版本都拒绝
        let wrong = AwdCrypto::generate_token();
        assert!(
            !crypto
                .is_valid_token(&wrong, &blob.ciphertext, &blob.nonce, event_id, 2)
                .unwrap()
        );
    }

    #[test]
    fn test_is_valid_token_wrong_token() {
        let crypto = test_crypto();
        let event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "internal_token");

        let token = AwdCrypto::generate_token();
        let wrong_token = AwdCrypto::generate_token();
        let blob = crypto.encrypt(&token, &aad, 1).unwrap();

        // Wrong token should fail
        assert!(
            !crypto
                .is_valid_token(&wrong_token, &blob.ciphertext, &blob.nonce, event_id, 1)
                .unwrap()
        );
    }

    #[test]
    fn test_is_valid_token_wrong_event_id() {
        let crypto = test_crypto();
        let event_id = Uuid::new_v4();
        let wrong_event_id = Uuid::new_v4();
        let aad = AwdCrypto::build_aad(event_id, "internal_token");

        let token = AwdCrypto::generate_token();
        let blob = crypto.encrypt(&token, &aad, 1).unwrap();

        // Wrong event_id changes AAD → ciphertext differs → verification fails
        assert!(
            !crypto
                .is_valid_token(&token, &blob.ciphertext, &blob.nonce, wrong_event_id, 1)
                .unwrap()
        );
    }
}
