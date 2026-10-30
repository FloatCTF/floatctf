//! 授权请求解析与校验（OIDC Core §3.1.2.1–§3.1.2.2，RFC 6749 §4.1.1，RFC 7636 §4.3）。
//!
//! 分两段：
//! 1. [`client_and_redirect`]：取出 `client_id` / `redirect_uri`。这两者不合法时**不能**重定向，
//!    调用方渲染 FloatCTF 自己的错误页。
//! 2. [`validate`]：回调已确认合法后的其余校验；失败返回 [`OAuthError`]，由调用方重定向回 RP。

use super::error::OAuthError;
use super::{pkce, scope};

pub const MAX_STATE_LEN: usize = 2048;
pub const MAX_NONCE_LEN: usize = 512;
pub const MAX_LOGIN_HINT_LEN: usize = 256;
pub const MAX_ID_TOKEN_HINT_LEN: usize = 8192;

/// 参数重复出现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateParameter;

/// 原始请求参数（保留重复项以便检测）。
#[derive(Debug, Clone, Default)]
pub struct RawParams(pub Vec<(String, String)>);

impl RawParams {
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self(
            pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }

    fn values(&self, name: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// 单值参数：重复出现视为非法（RFC 6749 §3.1 "MUST NOT be included more than once"）。
    /// 空字符串按 RFC 6749 §3.1 "treated as if they were omitted"。
    pub fn single(&self, name: &str) -> Result<Option<&str>, DuplicateParameter> {
        let values = self.values(name);
        match values.as_slice() {
            [] | [""] => Ok(None),
            [v] => Ok(Some(v)),
            _ => Err(DuplicateParameter),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.0.iter().any(|(k, v)| k == name && !v.is_empty())
    }

    /// 用于错误回传的 `state`：只在恰好出现一次且长度合法时回传。
    pub fn echo_state(&self) -> Option<String> {
        match self.single("state") {
            Ok(Some(s)) if s.len() <= MAX_STATE_LEN => Some(s.to_string()),
            _ => None,
        }
    }
}

/// 预校验失败（不可重定向）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreRedirectError {
    MissingClientId,
    DuplicateClientId,
    MissingRedirectUri,
    DuplicateRedirectUri,
}

impl PreRedirectError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingClientId => "缺少 client_id 参数",
            Self::DuplicateClientId => "client_id 参数重复",
            Self::MissingRedirectUri => "缺少 redirect_uri 参数",
            Self::DuplicateRedirectUri => "redirect_uri 参数重复",
        }
    }
}

pub fn client_and_redirect(params: &RawParams) -> Result<(String, String), PreRedirectError> {
    let client_id = params
        .single("client_id")
        .map_err(|_| PreRedirectError::DuplicateClientId)?
        .ok_or(PreRedirectError::MissingClientId)?;
    let redirect_uri = params
        .single("redirect_uri")
        .map_err(|_| PreRedirectError::DuplicateRedirectUri)?
        .ok_or(PreRedirectError::MissingRedirectUri)?;
    Ok((client_id.to_string(), redirect_uri.to_string()))
}

/// `prompt` 参数（§3.1.2.1）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prompt {
    pub none: bool,
    pub login: bool,
    pub consent: bool,
    pub select_account: bool,
}

impl Prompt {
    pub fn parse(raw: Option<&str>) -> Result<Self, OAuthError> {
        let mut prompt = Self::default();
        let Some(raw) = raw else {
            return Ok(prompt);
        };
        for value in raw.split(' ').filter(|v| !v.is_empty()) {
            match value {
                "none" => prompt.none = true,
                "login" => prompt.login = true,
                "consent" => prompt.consent = true,
                "select_account" => prompt.select_account = true,
                _ => return Err(OAuthError::invalid_request("unsupported prompt value")),
            }
        }
        if prompt.none && (prompt.login || prompt.consent || prompt.select_account) {
            return Err(OAuthError::invalid_request(
                "prompt=none must not be combined with other values",
            ));
        }
        Ok(prompt)
    }

    pub fn to_values(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.none {
            out.push("none".to_string());
        }
        if self.login {
            out.push("login".to_string());
        }
        if self.consent {
            out.push("consent".to_string());
        }
        if self.select_account {
            out.push("select_account".to_string());
        }
        out
    }

    pub fn from_values(values: &[String]) -> Self {
        Self {
            none: values.iter().any(|v| v == "none"),
            login: values.iter().any(|v| v == "login"),
            consent: values.iter().any(|v| v == "consent"),
            select_account: values.iter().any(|v| v == "select_account"),
        }
    }
}

/// 校验所需的应用视图。
#[derive(Debug, Clone)]
pub struct ClientPolicy {
    pub allowed_scopes: Vec<String>,
    pub require_pkce: bool,
}

/// 校验通过的授权请求。
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub state: Option<String>,
    pub nonce: Option<String>,
    pub code_challenge: Option<String>,
    pub prompt: Prompt,
    pub max_age: Option<i64>,
    pub login_hint: Option<String>,
    pub id_token_hint: Option<String>,
}

fn single<'a>(params: &'a RawParams, name: &str) -> Result<Option<&'a str>, OAuthError> {
    params
        .single(name)
        .map_err(|_| OAuthError::invalid_request(format!("duplicate parameter: {name}")))
}

/// 第二段校验（回调已确认合法）。
pub fn validate(
    params: &RawParams,
    client_id: &str,
    redirect_uri: &str,
    policy: &ClientPolicy,
) -> Result<AuthorizationRequest, OAuthError> {
    // 任何参数重复出现都拒绝。
    let mut names: Vec<&str> = params.0.iter().map(|(k, _)| k.as_str()).collect();
    names.sort_unstable();
    if let Some(dup) = names.windows(2).find(|w| w[0] == w[1]) {
        return Err(OAuthError::invalid_request(format!(
            "duplicate parameter: {}",
            dup[0]
        )));
    }

    // 未支持的高级参数给出规范规定的专用错误码（§3.1.2.6）。
    if params.has("request") {
        return Err(OAuthError::request_not_supported(
            "request objects are not supported",
        ));
    }
    if params.has("request_uri") {
        return Err(OAuthError::request_uri_not_supported(
            "request_uri is not supported",
        ));
    }
    if params.has("registration") {
        return Err(OAuthError::registration_not_supported(
            "registration parameter is not supported",
        ));
    }

    match single(params, "response_type")? {
        None => return Err(OAuthError::invalid_request("missing response_type")),
        Some("code") => {}
        Some(_) => {
            return Err(OAuthError::unsupported_response_type(
                "only response_type=code is supported",
            ));
        }
    }
    if let Some(mode) = single(params, "response_mode")?
        && mode != "query"
    {
        return Err(OAuthError::invalid_request(
            "only response_mode=query is supported",
        ));
    }

    let raw_scope =
        single(params, "scope")?.ok_or_else(|| OAuthError::invalid_request("missing scope"))?;
    let requested =
        scope::parse(raw_scope).ok_or_else(|| OAuthError::invalid_scope("malformed scope"))?;
    if !requested.contains(&scope::OPENID) {
        return Err(OAuthError::invalid_scope("scope must include openid"));
    }
    let scopes = scope::grantable(&requested, &policy.allowed_scopes);
    if !scopes.iter().any(|s| s == scope::OPENID) {
        return Err(OAuthError::invalid_scope(
            "client is not allowed to request openid",
        ));
    }

    let state = single(params, "state")?.map(str::to_string);
    if state.as_ref().is_some_and(|s| s.len() > MAX_STATE_LEN) {
        return Err(OAuthError::invalid_request("state too long"));
    }
    let nonce = single(params, "nonce")?.map(str::to_string);
    if nonce.as_ref().is_some_and(|n| n.len() > MAX_NONCE_LEN) {
        return Err(OAuthError::invalid_request("nonce too long"));
    }

    let code_challenge = match (
        single(params, "code_challenge")?,
        single(params, "code_challenge_method")?,
    ) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(OAuthError::invalid_request(
                "code_challenge_method without code_challenge",
            ));
        }
        // RFC 7636 §4.3：缺省方法为 plain，本 OP 只支持 S256，因此缺省也拒绝（防降级）。
        (Some(_), None) => {
            return Err(OAuthError::invalid_request(
                "code_challenge_method must be S256",
            ));
        }
        (Some(_), Some(method)) if method != pkce::METHOD_S256 => {
            return Err(OAuthError::invalid_request(
                "code_challenge_method must be S256",
            ));
        }
        (Some(challenge), Some(_)) => {
            if !pkce::is_valid_s256_challenge(challenge) {
                return Err(OAuthError::invalid_request("malformed code_challenge"));
            }
            Some(challenge.to_string())
        }
    };
    if code_challenge.is_none() {
        if policy.require_pkce {
            return Err(OAuthError::invalid_request(
                "code_challenge (PKCE S256) is required for this client",
            ));
        }
        // RFC 9700 §2.1.1：不用 PKCE 的机密 OIDC 客户端必须用 nonce 防授权码注入。
        if nonce.is_none() {
            return Err(OAuthError::invalid_request(
                "nonce is required when PKCE is not used",
            ));
        }
    }

    let prompt = Prompt::parse(single(params, "prompt")?)?;
    let max_age = match single(params, "max_age")? {
        None => None,
        Some(raw) => match raw.parse::<i64>() {
            Ok(v) if v >= 0 => Some(v),
            _ => {
                return Err(OAuthError::invalid_request(
                    "max_age must be a non-negative integer",
                ));
            }
        },
    };
    let login_hint = single(params, "login_hint")?
        .filter(|h| h.len() <= MAX_LOGIN_HINT_LEN)
        .map(str::to_string);
    let id_token_hint = single(params, "id_token_hint")?.map(str::to_string);
    if id_token_hint
        .as_ref()
        .is_some_and(|h| h.len() > MAX_ID_TOKEN_HINT_LEN)
    {
        return Err(OAuthError::invalid_request("id_token_hint too long"));
    }

    Ok(AuthorizationRequest {
        client_id: client_id.to_string(),
        redirect_uri: redirect_uri.to_string(),
        scopes,
        state,
        nonce,
        code_challenge,
        prompt,
        max_age,
        login_hint,
        id_token_hint,
    })
}

/// 是否需要用户重新输入密码（`prompt=login`、`max_age` 超时）。
///
/// `max_age=0` 等价于 `prompt=login`（OIDC Core §3.1.2.1）。
pub fn requires_fresh_login(
    prompt: &Prompt,
    max_age: Option<i64>,
    auth_time: i64,
    now: i64,
) -> bool {
    if prompt.login {
        return true;
    }
    match max_age {
        Some(max_age) => now.saturating_sub(auth_time) > max_age || max_age == 0,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn policy(require_pkce: bool) -> ClientPolicy {
        ClientPolicy {
            allowed_scopes: vec!["openid".into(), "profile".into()],
            require_pkce,
        }
    }

    fn base() -> Vec<(&'static str, &'static str)> {
        vec![
            ("client_id", "c1"),
            ("redirect_uri", "https://rp.example.com/cb"),
            ("response_type", "code"),
            ("scope", "openid profile email"),
            ("state", "st"),
            ("nonce", "n-1"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ]
    }

    fn run(
        pairs: Vec<(&str, &str)>,
        require_pkce: bool,
    ) -> Result<AuthorizationRequest, OAuthError> {
        let params = RawParams::from_pairs(pairs);
        let (c, r) = client_and_redirect(&params).unwrap();
        validate(&params, &c, &r, &policy(require_pkce))
    }

    fn without(name: &str) -> Vec<(&'static str, &'static str)> {
        base().into_iter().filter(|(k, _)| *k != name).collect()
    }

    fn with(name: &'static str, value: &'static str) -> Vec<(&'static str, &'static str)> {
        let mut v = without(name);
        v.push((name, value));
        v
    }

    #[test]
    fn valid_request_is_accepted_and_scopes_reduced() {
        let req = run(base(), true).unwrap();
        assert_eq!(req.scopes, vec!["openid", "profile"]);
        assert_eq!(req.state.as_deref(), Some("st"));
        assert_eq!(req.nonce.as_deref(), Some("n-1"));
        assert_eq!(req.code_challenge.as_deref(), Some(CHALLENGE));
    }

    #[test]
    fn pre_redirect_errors() {
        let p = RawParams::from_pairs(vec![("redirect_uri", "https://x/cb")]);
        assert_eq!(
            client_and_redirect(&p),
            Err(PreRedirectError::MissingClientId)
        );
        let p = RawParams::from_pairs(vec![
            ("client_id", "a"),
            ("client_id", "b"),
            ("redirect_uri", "x"),
        ]);
        assert_eq!(
            client_and_redirect(&p),
            Err(PreRedirectError::DuplicateClientId)
        );
        let p = RawParams::from_pairs(vec![("client_id", "a")]);
        assert_eq!(
            client_and_redirect(&p),
            Err(PreRedirectError::MissingRedirectUri)
        );
    }

    #[test]
    fn response_type_and_mode() {
        assert_eq!(
            run(without("response_type"), true).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            run(with("response_type", "token"), true).unwrap_err().error,
            "unsupported_response_type"
        );
        assert_eq!(
            run(with("response_type", "code id_token"), true)
                .unwrap_err()
                .error,
            "unsupported_response_type"
        );
        assert_eq!(
            run(with("response_mode", "fragment"), true)
                .unwrap_err()
                .error,
            "invalid_request"
        );
        assert!(run(with("response_mode", "query"), true).is_ok());
    }

    #[test]
    fn scope_must_include_openid() {
        assert_eq!(
            run(with("scope", "profile"), true).unwrap_err().error,
            "invalid_scope"
        );
        assert_eq!(
            run(without("scope"), true).unwrap_err().error,
            "invalid_request"
        );
    }

    #[test]
    fn pkce_rules() {
        // 缺 challenge（要求 PKCE）
        assert_eq!(
            run(
                without("code_challenge")
                    .into_iter()
                    .filter(|(k, _)| *k != "code_challenge_method")
                    .collect(),
                true
            )
            .unwrap_err()
            .error,
            "invalid_request"
        );
        // plain / 未知方法 / 缺方法（缺省=plain）全部拒绝
        assert_eq!(
            run(with("code_challenge_method", "plain"), true)
                .unwrap_err()
                .error,
            "invalid_request"
        );
        assert_eq!(
            run(with("code_challenge_method", "S512"), true)
                .unwrap_err()
                .error,
            "invalid_request"
        );
        assert_eq!(
            run(without("code_challenge_method"), true)
                .unwrap_err()
                .error,
            "invalid_request"
        );
        // 格式错误的 challenge
        assert_eq!(
            run(with("code_challenge", "short"), true)
                .unwrap_err()
                .error,
            "invalid_request"
        );
    }

    #[test]
    fn optional_pkce_requires_nonce() {
        let no_pkce: Vec<_> = base()
            .into_iter()
            .filter(|(k, _)| *k != "code_challenge" && *k != "code_challenge_method")
            .collect();
        assert!(run(no_pkce.clone(), false).is_ok());
        let no_pkce_no_nonce: Vec<_> = no_pkce.into_iter().filter(|(k, _)| *k != "nonce").collect();
        assert_eq!(
            run(no_pkce_no_nonce, false).unwrap_err().error,
            "invalid_request"
        );
    }

    #[test]
    fn duplicates_and_unsupported_parameters() {
        let mut dup = base();
        dup.push(("scope", "openid"));
        assert_eq!(run(dup, true).unwrap_err().error, "invalid_request");
        assert_eq!(
            run(with("request", "eyJ..."), true).unwrap_err().error,
            "request_not_supported"
        );
        assert_eq!(
            run(with("request_uri", "https://x/r"), true)
                .unwrap_err()
                .error,
            "request_uri_not_supported"
        );
        assert_eq!(
            run(with("registration", "{}"), true).unwrap_err().error,
            "registration_not_supported"
        );
    }

    #[test]
    fn prompt_and_max_age() {
        assert!(
            run(with("prompt", "login consent"), true)
                .unwrap()
                .prompt
                .login
        );
        assert_eq!(
            run(with("prompt", "none login"), true).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            run(with("prompt", "bogus"), true).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            run(with("max_age", "-1"), true).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            run(with("max_age", "abc"), true).unwrap_err().error,
            "invalid_request"
        );
        assert_eq!(
            run(with("max_age", "300"), true).unwrap().max_age,
            Some(300)
        );
    }

    #[test]
    fn echo_state_only_for_single_value() {
        let p = RawParams::from_pairs(vec![("state", "a"), ("state", "b")]);
        assert_eq!(p.echo_state(), None);
        let p = RawParams::from_pairs(vec![("state", "a")]);
        assert_eq!(p.echo_state().as_deref(), Some("a"));
    }

    #[test]
    fn fresh_login_rules() {
        let none = Prompt::default();
        let login = Prompt {
            login: true,
            ..Default::default()
        };
        assert!(requires_fresh_login(&login, None, 100, 100));
        assert!(!requires_fresh_login(&none, None, 0, 10_000));
        assert!(requires_fresh_login(&none, Some(0), 100, 100));
        assert!(!requires_fresh_login(&none, Some(60), 100, 160));
        assert!(requires_fresh_login(&none, Some(60), 100, 161));
    }
}
