//! redirect_uri 规则（RFC 9700 §4.1 / RFC 6749 §3.1.2 / RFC 8252 §7.3）。
//!
//! - 注册时：绝对 URL、无 fragment、无 userinfo、无通配；`https` 必需；
//!   `http` 只允许 loopback 主机且需显式开关 `allow_loopback_redirect_uris`；不支持自定义 scheme。
//! - 授权时：与已注册值**逐字节相等**（不做规范化、不做前缀/子域/端口放宽）。

use crate::core::config::is_loopback_host;

pub const MAX_REDIRECT_URI_LEN: usize = 2000;
pub const MAX_REDIRECT_URIS: usize = 10;

pub fn validate_registration(uri: &str, allow_loopback: bool) -> Result<(), String> {
    if uri.is_empty() || uri.len() > MAX_REDIRECT_URI_LEN {
        return Err("回调地址长度必须在 1..=2000 之间".into());
    }
    if uri.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("回调地址不得包含空白或控制字符".into());
    }
    if uri.contains('#') {
        return Err("回调地址不得包含 fragment（#）".into());
    }
    let parsed = url::Url::parse(uri).map_err(|e| format!("回调地址不是合法的绝对 URL：{e}"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("回调地址不得包含用户名/密码".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "回调地址缺少主机名".to_string())?;
    if host.contains('*') {
        return Err("回调地址不允许通配符".into());
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(host) => {
            if allow_loopback {
                Ok(())
            } else {
                Err("本平台未开启 loopback（http://127.0.0.1 / localhost）回调".into())
            }
        }
        "http" => Err("回调地址必须使用 https（http 仅限已开启的 loopback 开发回调）".into()),
        other => Err(format!("不支持的回调 scheme：{other}")),
    }
}

/// 授权请求中的 redirect_uri 是否与某个注册值逐字节一致。
pub fn is_registered(registered: &[String], requested: &str) -> bool {
    registered.iter().any(|r| r == requested)
}

/// 在合法回调上追加授权响应参数（保留回调自身的 query）。
pub fn append_query(redirect_uri: &str, params: &[(&str, &str)]) -> String {
    let mut url = match url::Url::parse(redirect_uri) {
        Ok(url) => url,
        // 只对已注册（已校验可解析）的地址调用；兜底返回原值，绝不拼接到别的地址。
        Err(_) => return redirect_uri.to_string(),
    };
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in params {
            pairs.append_pair(k, v);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_required_and_loopback_gated() {
        assert!(validate_registration("https://rp.example.com/cb", false).is_ok());
        assert!(validate_registration("http://rp.example.com/cb", true).is_err());
        assert!(validate_registration("http://127.0.0.1:8080/cb", false).is_err());
        assert!(validate_registration("http://127.0.0.1:8080/cb", true).is_ok());
        assert!(validate_registration("http://localhost/cb", true).is_ok());
        assert!(validate_registration("http://[::1]:9/cb", true).is_ok());
        assert!(validate_registration("com.example.app:/oauth", true).is_err());
        assert!(validate_registration("javascript:alert(1)", true).is_err());
    }

    #[test]
    fn rejects_fragment_userinfo_wildcard_and_relative() {
        assert!(validate_registration("https://rp.example.com/cb#x", false).is_err());
        assert!(validate_registration("https://u:p@rp.example.com/cb", false).is_err());
        assert!(validate_registration("https://*.example.com/cb", false).is_err());
        assert!(validate_registration("/relative/cb", false).is_err());
        assert!(validate_registration("https://rp.example.com/c b", false).is_err());
    }

    #[test]
    fn exact_match_only() {
        let reg = vec!["https://rp.example.com/cb".to_string()];
        assert!(is_registered(&reg, "https://rp.example.com/cb"));
        for bad in [
            "https://rp.example.com/cb/",
            "https://rp.example.com/cb?x=1",
            "https://rp.example.com/CB",
            "https://RP.example.com/cb",
            "https://rp.example.com:443/cb",
            "https://rp.example.com/cb#f",
            "https://evil.rp.example.com/cb",
            "https://rp.example.com.evil.com/cb",
            "https://rp.example.com/c%62",
            "http://rp.example.com/cb",
            "https://rp.example.com/cbx",
        ] {
            assert!(!is_registered(&reg, bad), "{bad} must not match");
        }
    }

    #[test]
    fn append_query_keeps_existing_query() {
        assert_eq!(
            append_query(
                "https://rp.example.com/cb?tenant=a",
                &[("code", "x y"), ("state", "s&t")]
            ),
            "https://rp.example.com/cb?tenant=a&code=x+y&state=s%26t"
        );
    }
}
