//! OIDC 签名 key ring（RS256）。
//!
//! - 私钥来自 `[[oidc.signing_keys]]` 引用的 PEM 文件（PKCS#8 或 PKCS#1），启动时加载一次；
//!   加载失败即拒绝启动，绝不临时生成 key（多副本/重启后 RP 验签会失败）。
//! - 恰好一把 `active` key 用于签发；其余 key 只把公钥发布在 JWKS 中，供已签 token 过渡验签。
//! - 解析与公钥分量提取用 `ring`，JWS 编解码用 `jsonwebtoken`（底层同为 `ring`）；
//!   与平台 API 的 HS512 `jwt_secret` 完全无关。
//! - JWKS 只含 `kty/n/e/kid/alg/use`，永不输出私钥参数。

use std::collections::HashMap;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse,
    RSAKeyParameters, RSAKeyType,
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Serialize, de::DeserializeOwned};

use crate::core::config::OidcSigningKeyConfig;

/// 一把待加载的 key（与配置解耦，便于测试注入 PEM）。
pub struct KeyMaterial {
    pub kid: String,
    pub pem: Vec<u8>,
    pub active: bool,
}

pub struct KeyRing {
    active_kid: String,
    encoding: EncodingKey,
    decoding: HashMap<String, DecodingKey>,
    jwks: JwkSet,
}

impl std::fmt::Debug for KeyRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyRing")
            .field("active_kid", &self.active_kid)
            .field("kids", &self.decoding.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl KeyRing {
    /// 从配置引用的 PEM 文件加载。
    pub fn load(keys: &[OidcSigningKeyConfig]) -> Result<Self, String> {
        let mut materials = Vec::with_capacity(keys.len());
        for key in keys {
            let pem = std::fs::read(&key.private_key_file).map_err(|e| {
                format!(
                    "读取 OIDC 签名私钥失败 kid={} path={}: {e}",
                    key.kid,
                    key.private_key_file.display()
                )
            })?;
            warn_if_permissive(key);
            materials.push(KeyMaterial {
                kid: key.kid.clone(),
                pem,
                active: key.active,
            });
        }
        Self::from_materials(materials)
    }

    pub fn from_materials(materials: Vec<KeyMaterial>) -> Result<Self, String> {
        let mut active: Option<(String, EncodingKey)> = None;
        let mut decoding = HashMap::new();
        let mut jwks = Vec::new();

        for material in materials {
            let kid = material.kid;
            if decoding.contains_key(&kid) {
                return Err(format!("OIDC 签名 key 的 kid 重复：{kid}"));
            }
            let (n, e) = rsa_public_components(&material.pem)
                .map_err(|e| format!("OIDC 签名 key kid={kid} 无效：{e}"))?;
            let n_b64 = URL_SAFE_NO_PAD.encode(&n);
            let e_b64 = URL_SAFE_NO_PAD.encode(&e);
            let decoding_key = DecodingKey::from_rsa_components(&n_b64, &e_b64)
                .map_err(|e| format!("OIDC 公钥 kid={kid} 构造失败：{e}"))?;

            if material.active {
                if active.is_some() {
                    return Err("OIDC key ring 只能有一把 active key".into());
                }
                let encoding = EncodingKey::from_rsa_pem(&material.pem)
                    .map_err(|e| format!("OIDC 签名 key kid={kid} 无法用于签名：{e}"))?;
                // 自检：用该私钥签名、用导出的公钥验签，确保 JWKS 与签发 key 一一对应。
                self_test(&kid, &encoding, &decoding_key)?;
                active = Some((kid.clone(), encoding));
            }

            jwks.push(Jwk {
                common: CommonParameters {
                    public_key_use: Some(PublicKeyUse::Signature),
                    key_algorithm: Some(KeyAlgorithm::RS256),
                    key_id: Some(kid.clone()),
                    ..Default::default()
                },
                algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                    key_type: RSAKeyType::RSA,
                    n: n_b64,
                    e: e_b64,
                }),
            });
            decoding.insert(kid, decoding_key);
        }

        let (active_kid, encoding) =
            active.ok_or_else(|| "OIDC key ring 缺少 active 签发 key".to_string())?;
        Ok(Self {
            active_kid,
            encoding,
            decoding,
            jwks: JwkSet { keys: jwks },
        })
    }

    pub fn active_kid(&self) -> &str {
        &self.active_kid
    }

    /// 公开 JWKS（仅公钥）。
    pub fn jwks(&self) -> &JwkSet {
        &self.jwks
    }

    /// 用 active key 以 RS256 签名；header 带 `kid` 与 `typ=JWT`。
    pub fn sign<T: Serialize>(&self, claims: &T) -> Result<String, String> {
        self.sign_typed(claims, "JWT")
    }

    /// 同 [`Self::sign`]，但指定 JWS `typ`（Back-Channel Logout 1.0 §2.4 推荐 `logout+jwt`，
    /// 防止 Logout Token 与 ID Token 互相混用）。
    pub fn sign_typed<T: Serialize>(&self, claims: &T, typ: &str) -> Result<String, String> {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.active_kid.clone());
        header.typ = Some(typ.into());
        jsonwebtoken::encode(&header, claims, &self.encoding)
            .map_err(|e| format!("sign {typ}: {e}"))
    }

    /// 校验本 OP 签发的 JWT（只接受 RS256 + key ring 内的 `kid` + `iss`）。
    ///
    /// `allow_expired` 用于 `id_token_hint`（OIDC Core §3.1.2.1：过期的 ID Token 仍可作为 hint）。
    pub fn verify<T: DeserializeOwned>(
        &self,
        token: &str,
        issuer: &str,
        allow_expired: bool,
    ) -> Result<T, String> {
        let header = jsonwebtoken::decode_header(token).map_err(|e| format!("bad header: {e}"))?;
        if header.alg != Algorithm::RS256 {
            return Err("unexpected alg".into());
        }
        let kid = header.kid.ok_or("missing kid")?;
        let key = self.decoding.get(&kid).ok_or("unknown kid")?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[issuer]);
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["iss", "sub", "exp"]);
        if allow_expired {
            validation.validate_exp = false;
        }
        jsonwebtoken::decode::<T>(token, key, &validation)
            .map(|data| data.claims)
            .map_err(|e| format!("verify: {e}"))
    }
}

/// 从 RSA 私钥 PEM 取公钥 `(n, e)` 大端字节（同时完成 ring 的私钥一致性校验与 2048 位下限）。
fn rsa_public_components(pem_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let block = pem::parse(pem_bytes).map_err(|e| format!("PEM 解析失败：{e}"))?;
    let key_pair = match block.tag() {
        "PRIVATE KEY" => ring::signature::RsaKeyPair::from_pkcs8(block.contents()),
        "RSA PRIVATE KEY" => ring::signature::RsaKeyPair::from_der(block.contents()),
        "ENCRYPTED PRIVATE KEY" => {
            return Err("不支持加密 PEM（请提供未加密、权限受限的私钥文件）".into());
        }
        other => return Err(format!("期望 RSA 私钥 PEM，实际为 {other}")),
    }
    .map_err(|e| format!("不是可用于签名的 RSA 私钥（需 2048..=4096 位）：{e}"))?;
    if key_pair.public().modulus_len() < 256 {
        return Err("RSA 模长不足 2048 位".into());
    }
    let components: ring::rsa::PublicKeyComponents<Vec<u8>> = key_pair.public().into();
    Ok((components.n, components.e))
}

fn self_test(kid: &str, encoding: &EncodingKey, decoding: &DecodingKey) -> Result<(), String> {
    #[derive(Serialize, serde::Deserialize)]
    struct Probe {
        sub: String,
        exp: i64,
    }
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let probe = Probe {
        sub: "self-test".into(),
        exp: chrono::Utc::now().timestamp() + 60,
    };
    let token = jsonwebtoken::encode(&header, &probe, encoding)
        .map_err(|e| format!("OIDC key kid={kid} 签名自检失败：{e}"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_aud = false;
    jsonwebtoken::decode::<Probe>(&token, decoding, &validation)
        .map_err(|e| format!("OIDC key kid={kid} 公私钥不一致：{e}"))?;
    Ok(())
}

#[cfg(unix)]
fn warn_if_permissive(key: &OidcSigningKeyConfig) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(&key.private_key_file)
        && meta.permissions().mode() & 0o007 != 0
    {
        tracing::warn!(
            kid = %key.kid,
            path = %key.private_key_file.display(),
            "OIDC 签名私钥文件对其他用户可读，请改为 0600/0640"
        );
    }
}

#[cfg(not(unix))]
fn warn_if_permissive(_key: &OidcSigningKeyConfig) {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 测试用 2048 位 RSA key（PKCS#8）。仅用于单元测试，与任何部署无关。
    pub(crate) const TEST_KEY_A: &str = include_str!("testdata/test-key-a.pem");
    /// 测试用第二把 key（PKCS#1），用于轮换场景。
    pub(crate) const TEST_KEY_B: &str = include_str!("testdata/test-key-b.pem");

    pub(crate) fn ring_ab(active_a: bool) -> KeyRing {
        KeyRing::from_materials(vec![
            KeyMaterial {
                kid: "key-a".into(),
                pem: TEST_KEY_A.as_bytes().to_vec(),
                active: active_a,
            },
            KeyMaterial {
                kid: "key-b".into(),
                pem: TEST_KEY_B.as_bytes().to_vec(),
                active: !active_a,
            },
        ])
        .unwrap()
    }

    #[derive(Serialize, serde::Deserialize, Debug)]
    struct Claims {
        iss: String,
        sub: String,
        exp: i64,
    }

    fn claims(exp_offset: i64) -> Claims {
        Claims {
            iss: "https://ctf.example.com".into(),
            sub: "u1".into(),
            exp: chrono::Utc::now().timestamp() + exp_offset,
        }
    }

    #[test]
    fn jwks_contains_only_public_parameters() {
        let ring = ring_ab(true);
        let json = serde_json::to_value(ring.jwks()).unwrap();
        let keys = json["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 2);
        for key in keys {
            let obj = key.as_object().unwrap();
            for private in ["d", "p", "q", "dp", "dq", "qi", "oth", "k"] {
                assert!(
                    !obj.contains_key(private),
                    "JWKS leaked private param {private}"
                );
            }
            assert_eq!(obj["kty"], "RSA");
            assert_eq!(obj["alg"], "RS256");
            assert_eq!(obj["use"], "sig");
            assert!(obj["n"].as_str().unwrap().len() >= 342); // 2048 bit
            assert_eq!(obj["e"], "AQAB");
        }
        let text = json.to_string();
        assert!(!text.contains("PRIVATE"));
    }

    #[test]
    fn signed_token_carries_kid_and_verifies_against_jwks() {
        let ring = ring_ab(true);
        let token = ring.sign(&claims(60)).unwrap();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some("key-a"));
        // 只用公开 JWKS 验签（模拟 RP）。
        let jwk = ring.jwks().find("key-a").unwrap();
        let key = DecodingKey::from_jwk(jwk).unwrap();
        let mut v = Validation::new(Algorithm::RS256);
        v.validate_aud = false;
        jsonwebtoken::decode::<Claims>(&token, &key, &v).unwrap();
        // 用另一把公钥验签必须失败。
        let other = DecodingKey::from_jwk(ring.jwks().find("key-b").unwrap()).unwrap();
        assert!(jsonwebtoken::decode::<Claims>(&token, &other, &v).is_err());
    }

    #[test]
    fn rotation_keeps_old_tokens_verifiable() {
        let old_ring = ring_ab(true);
        let old_token = old_ring.sign(&claims(60)).unwrap();
        let new_ring = ring_ab(false);
        let new_token = new_ring.sign(&claims(60)).unwrap();
        assert_eq!(
            jsonwebtoken::decode_header(&new_token)
                .unwrap()
                .kid
                .as_deref(),
            Some("key-b")
        );
        // 新 ring 仍发布旧 key 的公钥，旧 token 继续可验。
        new_ring
            .verify::<Claims>(&old_token, "https://ctf.example.com", false)
            .unwrap();
        new_ring
            .verify::<Claims>(&new_token, "https://ctf.example.com", false)
            .unwrap();
    }

    #[test]
    fn verify_rejects_wrong_issuer_alg_none_unknown_kid_and_expiry() {
        let ring = ring_ab(true);
        let token = ring.sign(&claims(60)).unwrap();
        assert!(
            ring.verify::<Claims>(&token, "https://evil.example", false)
                .is_err()
        );

        // alg=none：手工拼装无签名 token。
        let none_header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT","kid":"key-a"}"#);
        let payload = token.split('.').nth(1).unwrap();
        let alg_none = format!("{none_header}.{payload}.");
        assert!(
            ring.verify::<Claims>(&alg_none, "https://ctf.example.com", false)
                .is_err()
        );

        // HS256 算法混淆：用公钥 n 当 HMAC 密钥签名。
        let mut hs = Header::new(Algorithm::HS256);
        hs.kid = Some("key-a".into());
        let n = match &ring.jwks().find("key-a").unwrap().algorithm {
            AlgorithmParameters::RSA(p) => p.n.clone(),
            _ => unreachable!(),
        };
        let confused =
            jsonwebtoken::encode(&hs, &claims(60), &EncodingKey::from_secret(n.as_bytes()))
                .unwrap();
        assert!(
            ring.verify::<Claims>(&confused, "https://ctf.example.com", false)
                .is_err()
        );

        // 未知 kid
        let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
        parts[0] = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT","kid":"nope"}"#);
        assert!(
            ring.verify::<Claims>(&parts.join("."), "https://ctf.example.com", false)
                .is_err()
        );

        // 过期：hint 场景允许，普通校验拒绝
        let expired = ring.sign(&claims(-3600)).unwrap();
        assert!(
            ring.verify::<Claims>(&expired, "https://ctf.example.com", false)
                .is_err()
        );
        assert!(
            ring.verify::<Claims>(&expired, "https://ctf.example.com", true)
                .is_ok()
        );

        // 篡改签名
        let mut tampered = token.clone();
        tampered.pop();
        tampered.push(if token.ends_with('A') { 'B' } else { 'A' });
        assert!(
            ring.verify::<Claims>(&tampered, "https://ctf.example.com", false)
                .is_err()
        );
    }

    #[test]
    fn invalid_key_material_fails_closed() {
        let bad = KeyRing::from_materials(vec![KeyMaterial {
            kid: "x".into(),
            pem: b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n".to_vec(),
            active: true,
        }]);
        assert!(bad.is_err());
        let no_active = KeyRing::from_materials(vec![KeyMaterial {
            kid: "a".into(),
            pem: TEST_KEY_A.as_bytes().to_vec(),
            active: false,
        }]);
        assert!(no_active.unwrap_err().contains("active"));
        let dup = KeyRing::from_materials(vec![
            KeyMaterial {
                kid: "a".into(),
                pem: TEST_KEY_A.as_bytes().to_vec(),
                active: true,
            },
            KeyMaterial {
                kid: "a".into(),
                pem: TEST_KEY_B.as_bytes().to_vec(),
                active: false,
            },
        ]);
        assert!(dup.unwrap_err().contains("重复"));
    }

    #[test]
    fn debug_output_does_not_expose_key_material() {
        let ring = ring_ab(true);
        let out = format!("{ring:?}");
        assert!(out.contains("key-a"));
        assert!(!out.contains("PRIVATE"));
    }
}
