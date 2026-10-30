//! Token 端点客户端认证方式识别（RFC 6749 §2.3.1、OIDC Core §9）。

use base64::{Engine, engine::general_purpose::STANDARD};

use super::error::OAuthError;

/// 请求实际呈现的认证方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentedClientAuth {
    /// `Authorization: Basic base64(urlencode(id):urlencode(secret))`
    Basic { client_id: String, secret: String },
    /// form 中的 `client_id` + `client_secret`
    Post { client_id: String, secret: String },
    /// 公开客户端：只有 form `client_id`
    None { client_id: String },
}

impl PresentedClientAuth {
    pub fn client_id(&self) -> &str {
        match self {
            Self::Basic { client_id, .. }
            | Self::Post { client_id, .. }
            | Self::None { client_id } => client_id,
        }
    }

    /// 与 `oidc_token_auth_method` 枚举的字符串值对应。
    pub fn method(&self) -> &'static str {
        match self {
            Self::Basic { .. } => "client_secret_basic",
            Self::Post { .. } => "client_secret_post",
            Self::None { .. } => "none",
        }
    }
}

/// 识别认证方式；同时使用多种方式（RFC 6749 §2.3 "MUST NOT"）或格式错误一律拒绝。
pub fn identify(
    authorization_header: Option<&str>,
    form_client_id: Option<&str>,
    form_client_secret: Option<&str>,
) -> Result<PresentedClientAuth, OAuthError> {
    if let Some(header) = authorization_header {
        let (client_id, secret) = parse_basic(header)?;
        if form_client_secret.is_some() {
            return Err(OAuthError::invalid_request(
                "multiple client authentication methods used",
            ));
        }
        if let Some(form_id) = form_client_id
            && form_id != client_id
        {
            return Err(OAuthError::invalid_request(
                "client_id does not match client authentication",
            ));
        }
        return Ok(PresentedClientAuth::Basic { client_id, secret });
    }
    match (form_client_id, form_client_secret) {
        (Some(id), Some(secret)) if !id.is_empty() => Ok(PresentedClientAuth::Post {
            client_id: id.to_string(),
            secret: secret.to_string(),
        }),
        (Some(id), None) if !id.is_empty() => Ok(PresentedClientAuth::None {
            client_id: id.to_string(),
        }),
        _ => Err(OAuthError::invalid_client("client authentication required")),
    }
}

fn parse_basic(header: &str) -> Result<(String, String), OAuthError> {
    let invalid = || OAuthError::invalid_client("malformed Basic client authentication");
    let (scheme, value) = header.trim().split_once(' ').ok_or_else(invalid)?;
    if !scheme.eq_ignore_ascii_case("basic") || value.len() > 2048 {
        return Err(invalid());
    }
    let decoded = STANDARD.decode(value.trim()).map_err(|_| invalid())?;
    let decoded = String::from_utf8(decoded).map_err(|_| invalid())?;
    let (raw_id, raw_secret) = decoded.split_once(':').ok_or_else(invalid)?;
    let client_id = form_decode(raw_id).ok_or_else(invalid)?;
    let secret = form_decode(raw_secret).ok_or_else(invalid)?;
    if client_id.is_empty() {
        return Err(invalid());
    }
    Ok((client_id, secret))
}

/// `application/x-www-form-urlencoded` 解码单个分量（`+` 为空格，§2.3.1）。
fn form_decode(component: &str) -> Option<String> {
    let replaced = component.replace('+', " ");
    percent_decode(&replaced)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = input.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic(id: &str, secret: &str) -> String {
        format!("Basic {}", STANDARD.encode(format!("{id}:{secret}")))
    }

    #[test]
    fn basic_header_is_decoded_with_form_encoding() {
        let h = basic("fctf_abc", "fcs_x%2By+z");
        assert_eq!(
            identify(Some(&h), None, None).unwrap(),
            PresentedClientAuth::Basic {
                client_id: "fctf_abc".into(),
                secret: "fcs_x+y z".into()
            }
        );
    }

    #[test]
    fn post_and_none_methods() {
        assert_eq!(
            identify(None, Some("c"), Some("s")).unwrap().method(),
            "client_secret_post"
        );
        assert_eq!(identify(None, Some("c"), None).unwrap().method(), "none");
    }

    #[test]
    fn multiple_methods_or_missing_auth_rejected() {
        let h = basic("c", "s");
        assert_eq!(
            identify(Some(&h), Some("c"), Some("s")).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            identify(Some(&h), Some("other"), None).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            identify(None, None, None).unwrap_err().error,
            "invalid_client"
        );
        assert_eq!(
            identify(None, Some(""), None).unwrap_err().error,
            "invalid_client"
        );
    }

    #[test]
    fn malformed_basic_rejected() {
        for h in [
            "Bearer abc",
            "Basic !!!",
            "Basic",
            &format!("Basic {}", STANDARD.encode("nocolon")),
        ] {
            assert_eq!(
                identify(Some(h), None, None).unwrap_err().error,
                "invalid_client",
                "{h}"
            );
        }
    }
}
