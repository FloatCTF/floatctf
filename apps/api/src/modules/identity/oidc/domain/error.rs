//! OAuth 2.0 / OIDC 标准错误（RFC 6749 §4.1.2.1 / §5.2，OIDC Core §3.1.2.6，RFC 6750 §3.1）。

/// 标准错误码 + 描述。
///
/// `description` 会出现在重定向 URL 与 JSON 响应中，必须是 RFC 6749 NQSCHAR
/// （可打印 ASCII，不含 `"` 与 `\`），因此一律使用英文短句，且不得包含任何令牌/口令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthError {
    pub error: &'static str,
    pub description: String,
}

macro_rules! oauth_errors {
    ($($fn_name:ident => $code:literal),* $(,)?) => {
        impl OAuthError {
            $(
                pub fn $fn_name(description: impl Into<String>) -> Self {
                    Self { error: $code, description: sanitize(description.into()) }
                }
            )*
        }
    };
}

oauth_errors! {
    invalid_request => "invalid_request",
    unauthorized_client => "unauthorized_client",
    access_denied => "access_denied",
    unsupported_response_type => "unsupported_response_type",
    invalid_scope => "invalid_scope",
    server_error => "server_error",
    temporarily_unavailable => "temporarily_unavailable",
    login_required => "login_required",
    consent_required => "consent_required",
    account_selection_required => "account_selection_required",
    interaction_required => "interaction_required",
    request_not_supported => "request_not_supported",
    request_uri_not_supported => "request_uri_not_supported",
    registration_not_supported => "registration_not_supported",
    invalid_client => "invalid_client",
    invalid_grant => "invalid_grant",
    unsupported_grant_type => "unsupported_grant_type",
    invalid_token => "invalid_token",
}

/// 只保留 NQSCHAR，防止描述破坏 URL/JSON/`WWW-Authenticate` 语法。
fn sanitize(description: String) -> String {
    description
        .chars()
        .filter(|c| matches!(*c as u32, 0x20..=0x21 | 0x23..=0x5B | 0x5D..=0x7E))
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_is_restricted_to_nqschar() {
        let e = OAuthError::invalid_request("bad \"param\" \\ 参数\n");
        assert_eq!(e.error, "invalid_request");
        assert_eq!(e.description, "bad param  ");
    }
}
