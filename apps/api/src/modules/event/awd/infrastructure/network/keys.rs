//! AWD 网络密钥材料管理。

use base64::{Engine, engine::general_purpose::STANDARD};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// 以 WireGuard 兼容 Base64 编码的密钥对。
#[derive(Clone)]
pub struct WgKeyPair {
    /// 32-byte clamped private key, Base64.
    pub private_key: String,
    /// 32-byte public key, Base64.
    pub public_key: String,
}

impl std::fmt::Debug for WgKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgKeyPair")
            .field("private_key", &"***")
            .field("public_key", &self.public_key)
            .finish()
    }
}

/// 生成新的 WireGuard 密钥对（`wg genkey` + `wg pubkey`）。
pub fn generate_keypair() -> WgKeyPair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    WgKeyPair {
        private_key: STANDARD.encode(secret.to_bytes()),
        public_key: STANDARD.encode(public.to_bytes()),
    }
}

/// 由 Base64 私钥派生公钥（`wg pubkey`）。
pub fn public_from_private(private_key_b64: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(private_key_b64.trim())
        .map_err(|e| format!("invalid private key base64: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("private key must be 32 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let secret = StaticSecret::from(arr);
    let public = PublicKey::from(&secret);
    Ok(STANDARD.encode(public.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_roundtrip_pubkey() {
        let kp = generate_keypair();
        assert_eq!(kp.private_key.len(), 44); // 32 bytes base64
        assert_eq!(kp.public_key.len(), 44);
        let derived = public_from_private(&kp.private_key).unwrap();
        assert_eq!(derived, kp.public_key);
    }

    #[test]
    fn private_key_not_in_debug() {
        let kp = generate_keypair();
        let dbg = format!("{kp:?}");
        assert!(!dbg.contains(&kp.private_key));
        assert!(dbg.contains("***"));
    }

    #[test]
    fn keys_are_unique() {
        let a = generate_keypair();
        let b = generate_keypair();
        assert_ne!(a.private_key, b.private_key);
        assert_ne!(a.public_key, b.public_key);
    }
}
