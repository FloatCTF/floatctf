//! Scope：V1 只有 `openid` / `profile` / `email`。

pub const OPENID: &str = "openid";
pub const PROFILE: &str = "profile";
pub const EMAIL: &str = "email";

/// 平台支持的全部 scope（规范顺序）。
pub const SUPPORTED: [&str; 3] = [OPENID, PROFILE, EMAIL];

/// RFC 6749 §3.3 `scope-token = 1*NQCHAR`。
fn is_scope_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x5B | 0x5D..=0x7E))
}

/// 解析空格分隔的 scope 参数；含非法字符时返回 `None`。
pub fn parse(raw: &str) -> Option<Vec<&str>> {
    let tokens: Vec<&str> = raw.split(' ').filter(|t| !t.is_empty()).collect();
    if tokens.iter().all(|t| is_scope_token(t)) {
        Some(tokens)
    } else {
        None
    }
}

/// 请求 scope ∩ 平台支持 ∩ 应用允许，按规范顺序去重返回。
///
/// 未知/未允许的 scope 按 RFC 6749 §3.3 静默忽略（授权结果在 token 响应 `scope` 中如实返回）。
pub fn grantable(requested: &[&str], client_allowed: &[String]) -> Vec<String> {
    SUPPORTED
        .iter()
        .filter(|s| requested.contains(s) && client_allowed.iter().any(|a| a == *s))
        .map(|s| s.to_string())
        .collect()
}

/// 已授权集合是否覆盖本次申请。
pub fn covers(granted: &[String], requested: &[String]) -> bool {
    requested.iter().all(|r| granted.contains(r))
}

/// 两个集合的并集（规范顺序）。
pub fn union(a: &[String], b: &[String]) -> Vec<String> {
    SUPPORTED
        .iter()
        .filter(|s| a.iter().any(|x| x == *s) || b.iter().any(|x| x == *s))
        .map(|s| s.to_string())
        .collect()
}

pub fn join(scopes: &[String]) -> String {
    scopes.join(" ")
}

/// 应用登记时的 scope 白名单校验：只能是支持的 scope，且必须含 `openid`。
pub fn normalize_client_scopes(input: &[String]) -> Result<Vec<String>, String> {
    for s in input {
        if !SUPPORTED.contains(&s.as_str()) {
            return Err(format!(
                "不支持的 scope：{s}（只允许 openid / profile / email）"
            ));
        }
    }
    let normalized = union(input, &[]);
    if !normalized.iter().any(|s| s == OPENID) {
        return Err("scope 必须包含 openid".into());
    }
    Ok(normalized)
}

/// 从 JSONB 数组列读取 scope 列表（非字符串元素忽略）。
pub fn from_json(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn to_json(scopes: &[String]) -> serde_json::Value {
    serde_json::Value::from(scopes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn grantable_intersects_supported_and_client_allowed() {
        let req = parse("email openid address phone profile").unwrap();
        assert_eq!(
            grantable(&req, &s(&["openid", "profile"])),
            s(&["openid", "profile"])
        );
        assert_eq!(
            grantable(&req, &s(&["openid", "profile", "email"])),
            s(&["openid", "profile", "email"])
        );
    }

    #[test]
    fn parse_rejects_illegal_characters() {
        assert!(parse("openid \"profile\"").is_none());
        assert!(parse("openid pro\\file").is_none());
        assert_eq!(
            parse("  openid   profile ").unwrap(),
            vec!["openid", "profile"]
        );
    }

    #[test]
    fn covers_and_union() {
        assert!(covers(&s(&["openid", "profile"]), &s(&["openid"])));
        assert!(!covers(&s(&["openid"]), &s(&["openid", "email"])));
        assert_eq!(
            union(&s(&["email", "openid"]), &s(&["profile"])),
            s(&["openid", "profile", "email"])
        );
    }

    #[test]
    fn client_scope_registration_requires_openid_and_known_values() {
        assert!(normalize_client_scopes(&s(&["profile"])).is_err());
        assert!(normalize_client_scopes(&s(&["openid", "admin"])).is_err());
        assert_eq!(
            normalize_client_scopes(&s(&["email", "openid", "email"])).unwrap(),
            s(&["openid", "email"])
        );
    }
}
