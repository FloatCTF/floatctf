//! S3 兼容对象存储（RustFS）初始化。
//!
//! 启动竞态（release risk R2）：Compose 的 rustfs healthcheck 曾经只探 TCP 端口，
//! 而端口打开 ≠ S3/HTTP 层可用。API 在 `depends_on: service_healthy` 之后立刻调用
//! [`connect`]，`ensure_buckets` 一次性失败 → bootstrap panic → crash-loop。
//!
//! 因此这里对 bucket 初始化做**有界重试**，并把错误分成两类：
//! - [`ErrorClass::Transient`]：RustFS 尚未就绪（连接被拒 / 超时 / DNS 未解析 / 5xx /
//!   没有鉴权线索的 `service error`）→ 退避重试。
//! - [`ErrorClass::Permanent`]：凭据或权限配置错误（401/403、InvalidAccessKeyId、
//!   SignatureDoesNotMatch、AccessDenied 等）→ 立即失败，不浪费重试预算。

use std::time::Duration;

use anyhow::{Result, anyhow};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::primitives::ByteStream;
use tracing::{info, warn};

use crate::core::config::StorageConfig;

/// aws-sdk-s3 各操作错误的 raw response 类型（`SdkError<E, S3HttpResponse>`）。
type S3HttpResponse = aws_sdk_s3::config::http::HttpResponse;

pub async fn connect(config: &StorageConfig) -> Result<aws_sdk_s3::Client> {
    let creds = aws_sdk_s3::config::Credentials::new(
        config.access_key_id.clone(),
        config.secret_access_key.expose().to_string(),
        None,
        None,
        "floatctf",
    );

    let s3_config = aws_sdk_s3::Config::builder()
        .region(aws_sdk_s3::config::Region::new(config.region.clone()))
        .endpoint_url(&config.endpoint_url)
        .credentials_provider(creds)
        .force_path_style(true)
        .behavior_version(aws_config::BehaviorVersion::latest())
        .build();

    let client = aws_sdk_s3::Client::from_conf(s3_config);

    let policy = RetryPolicy::default();
    info!(
        endpoint = %config.endpoint_url,
        max_attempts = policy.max_attempts,
        deadline_secs = policy.total_deadline.as_secs(),
        "waiting for RustFS to become usable"
    );
    ensure_buckets_with_retry(&client, &policy).await?;
    info!("Rustfs connected OK");
    Ok(client)
}

// ── 重试策略 ────────────────────────────────────────────────────────────────

/// bucket 初始化重试策略：**有界**尝试次数 + **有界**总时长 + 指数退避（封顶）。
///
/// 默认 12 次 / 90s，退避 1s、2s、4s、8s、8s…（封顶 8s）。
/// 退避总和 1+2+4+8×8 = 71s < 90s，保证 12 次尝试都能在预算内真正发生。
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// 最大尝试次数（含首次）。绝不无限重试。
    pub max_attempts: u32,
    /// 重试总预算上界；耗尽即放弃。
    pub total_deadline: Duration,
    /// 首次退避时长，之后逐次翻倍。
    pub initial_backoff: Duration,
    /// 单次退避上限。
    pub max_backoff: Duration,
    /// 在退避之上额外叠加的随机抖动上限（避免多副本同时重试；0 = 关闭）。
    pub jitter: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 12,
            total_deadline: Duration::from_secs(90),
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(8),
            jitter: Duration::from_millis(250),
        }
    }
}

impl RetryPolicy {
    /// 第 `attempt` 次失败后应等待的**基础**退避（attempt 从 1 开始）：1s, 2s, 4s, 8s, 8s, …
    ///
    /// 抖动由调用方叠加，不改变这里的封顶值，便于断言上界。
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        // 限制移位，避免 attempt 很大时溢出（结果反正会被 max_backoff 截断）。
        let shift = (attempt - 1).min(16);
        self.initial_backoff
            .saturating_mul(1u32 << shift)
            .min(self.max_backoff)
    }

    /// 抖动偏移（`[0, jitter)`）。用系统时钟纳秒的低位，无需额外依赖。
    fn jitter(&self) -> Duration {
        if self.jitter.is_zero() {
            return Duration::ZERO;
        }
        let span = u64::try_from(self.jitter.as_nanos())
            .unwrap_or(u64::MAX)
            .max(1);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::from(d.subsec_nanos()))
            .unwrap_or(0);
        Duration::from_nanos(seed % span)
    }
}

/// 有界重试地初始化 bucket；永久性配置错误立即失败。
pub async fn ensure_buckets_with_retry(
    client: &aws_sdk_s3::Client,
    policy: &RetryPolicy,
) -> Result<()> {
    let started = std::time::Instant::now();
    let mut last: Option<BucketInitError> = None;

    for attempt in 1..=policy.max_attempts.max(1) {
        match ensure_buckets(client).await {
            Ok(()) => {
                if attempt > 1 {
                    info!(
                        attempt,
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "RustFS became usable after retry"
                    );
                }
                return Ok(());
            }
            Err(err) => {
                let classified = classify_anyhow(&err);
                warn!(
                    attempt,
                    max_attempts = policy.max_attempts,
                    class = classified.class.as_str(),
                    error = %classified.detail,
                    "RustFS bucket initialization failed"
                );

                if classified.class == ErrorClass::Permanent {
                    return Err(anyhow!(
                        "RustFS storage configuration error (permanent, not retryable): {}",
                        classified.detail
                    ));
                }

                let delay = policy.backoff_for_attempt(attempt) + policy.jitter();
                let exhausted = attempt >= policy.max_attempts
                    || started.elapsed().saturating_add(delay) >= policy.total_deadline;
                last = Some(classified);
                if exhausted {
                    break;
                }
                warn!(
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "retrying RustFS bucket initialization"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }

    match last {
        Some(last) => Err(exhaustion_error(policy, started.elapsed(), &last)),
        None => Err(anyhow!(
            "RustFS bucket initialization did not run (retry policy allows no attempts)"
        )),
    }
}

fn exhaustion_error(
    policy: &RetryPolicy,
    elapsed: Duration,
    last: &BucketInitError,
) -> anyhow::Error {
    anyhow!(
        "RustFS did not become usable within the bounded retry window ({} attempts / {}s, elapsed {}s); last classified error: {}",
        policy.max_attempts,
        policy.total_deadline.as_secs(),
        elapsed.as_secs(),
        last.detail
    )
}

// ── 错误分类 ────────────────────────────────────────────────────────────────

/// 错误分类结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorClass {
    /// 暂时性：值得在预算内重试。
    Transient,
    /// 永久性：重试不会变好，必须带上清晰原因立刻失败。
    Permanent,
}

impl ErrorClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Permanent => "permanent",
        }
    }
}

/// SDK 失败形态（与 SDK 的 `SdkError` 变体一一对应，抽出后便于单测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SdkFailureKind {
    /// 请求没有到达服务端：连接被拒 / DNS 未解析 / 连接被重置 / 未就绪的代理。
    Dispatch,
    /// 请求超时。
    Timeout,
    /// 客户端侧构造请求失败（非服务端状态）。
    Construction,
    /// 服务端返回了错误响应。
    Service,
    /// 响应无法解析（例如尚未就绪的中间层返回半截 HTTP）。
    Response,
    /// SDK 非穷尽枚举的兜底。
    #[default]
    Unknown,
}

impl SdkFailureKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dispatch => "dispatch failure",
            Self::Timeout => "timeout",
            Self::Construction => "construction failure",
            Self::Service => "service error",
            Self::Response => "response error",
            Self::Unknown => "unclassified s3 error",
        }
    }
}

/// 从 SDK 错误中抽取的、**与凭据无关**的最小分类信号。
///
/// 刻意不保存 HTTP body / `ErrorMetadata::message()`：RustFS/AWS 的错误消息里可能回显
/// 请求签名与 access key 片段，日志里只允许出现状态码与错误码。
#[derive(Debug, Clone, Default)]
struct S3FailureSignals {
    kind: SdkFailureKind,
    http_status: Option<u16>,
    code: Option<String>,
}

impl S3FailureSignals {
    fn describe(&self) -> String {
        match (self.http_status, self.code.as_deref()) {
            (Some(status), Some(code)) => {
                format!("{} (http_status={status}, code={code})", self.kind.as_str())
            }
            (Some(status), None) => format!("{} (http_status={status})", self.kind.as_str()),
            (None, Some(code)) => format!("{} (code={code})", self.kind.as_str()),
            (None, None) => self.kind.as_str().to_string(),
        }
    }
}

/// 鉴权类错误码（小写比较）。命中即视为永久性配置错误。
const PERMANENT_ERROR_CODES: &[&str] = &[
    "invalidaccesskeyid",
    "signaturedoesnotmatch",
    "accessdenied",
    "invalidaccesskey",
    "invalidsecurity",
    "invalidtoken",
    "expiredtoken",
    "tokenrefreshrequired",
    "accountproblem",
    "authorizationheadermalformed",
];

/// 分类规则（保持小而可读；无法判定的一律按「暂时性但有界」处理）：
///
/// - Dispatch / Timeout / Construction → 暂时性（连接未建立 / 服务端还没就绪）。
/// - Service / Response：
///   - HTTP 401 / 403，或错误码命中 [`PERMANENT_ERROR_CODES`] → 永久性（凭据/权限）。
///   - 其余（5xx、404、408、429、无状态码等）→ 暂时性：启动竞态下 RustFS 常返回
///     5xx 或没有 error code 的裸 `service error`，用有界重试兜底。
/// - Unknown（SDK 新增变体）→ 暂时性但有界。
fn classify(signals: &S3FailureSignals) -> ErrorClass {
    match signals.kind {
        SdkFailureKind::Dispatch | SdkFailureKind::Timeout | SdkFailureKind::Construction => {
            ErrorClass::Transient
        }
        SdkFailureKind::Service | SdkFailureKind::Response => {
            let auth_code = signals
                .code
                .as_deref()
                .map(|c| {
                    let lower = c.to_ascii_lowercase();
                    PERMANENT_ERROR_CODES.contains(&lower.as_str())
                })
                .unwrap_or(false);
            if auth_code || matches!(signals.http_status, Some(401 | 403)) {
                ErrorClass::Permanent
            } else {
                ErrorClass::Transient
            }
        }
        SdkFailureKind::Unknown => ErrorClass::Transient,
    }
}

/// 把 SDK 错误抽成分类信号（不含任何凭据文本）。
fn signals_from_sdk_error<E>(err: &SdkError<E, S3HttpResponse>) -> S3FailureSignals
where
    E: ProvideErrorMetadata,
{
    let mut signals = S3FailureSignals {
        kind: match err {
            SdkError::TimeoutError(_) => SdkFailureKind::Timeout,
            SdkError::DispatchFailure(_) => SdkFailureKind::Dispatch,
            SdkError::ServiceError(_) => SdkFailureKind::Service,
            SdkError::ResponseError(_) => SdkFailureKind::Response,
            SdkError::ConstructionFailure(_) => SdkFailureKind::Construction,
            _ => SdkFailureKind::Unknown,
        },
        ..Default::default()
    };
    if let Some(raw) = err.raw_response() {
        signals.http_status = Some(raw.status().as_u16());
    }
    signals.code = err.code().map(str::to_string);
    signals
}

/// bucket 初始化失败的分类载体：让重试循环拿到「类别 + 与凭据无关的描述」。
///
/// 放进 `anyhow` 后仍可用 `downcast_ref` 取回（`ensure_buckets` 的签名保持 `Result<()>`）。
#[derive(Debug)]
struct BucketInitError {
    class: ErrorClass,
    detail: String,
}

impl std::fmt::Display for BucketInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl std::error::Error for BucketInitError {}

/// 单个 S3 操作错误 → 分类载体。
fn s3_failure<E>(err: SdkError<E, S3HttpResponse>) -> BucketInitError
where
    E: ProvideErrorMetadata,
{
    let signals = signals_from_sdk_error(&err);
    BucketInitError {
        class: classify(&signals),
        detail: signals.describe(),
    }
}

/// 取回 `ensure_buckets` 抛出的分类结果。
///
/// 认不出的错误（理论上只会来自本文件的 SDK 调用）按「暂时性但有界」处理：
/// 宁可多等一个有界窗口，也不要因为 SDK 换了错误类型就直接 crash-loop。
fn classify_anyhow(err: &anyhow::Error) -> BucketInitError {
    match err.downcast_ref::<BucketInitError>() {
        Some(e) => BucketInitError {
            class: e.class,
            detail: e.detail.clone(),
        },
        None => BucketInitError {
            class: ErrorClass::Transient,
            detail: err.to_string(),
        },
    }
}

// ── bucket 初始化 ───────────────────────────────────────────────────────────

pub async fn ensure_buckets(client: &aws_sdk_s3::Client) -> Result<()> {
    let floatctf_public_bucket_name = "floatctf-public";

    let floatctf_public_bucket = client
        .head_bucket()
        .bucket(floatctf_public_bucket_name)
        .send()
        .await;
    if floatctf_public_bucket.is_err() {
        client
            .create_bucket()
            .bucket(floatctf_public_bucket_name)
            .send()
            .await
            .map_err(s3_failure)?;
        let policy = format!(
            r#"{{
                "Version": "2012-10-17",
                "Statement": [
                    {{
                        "Sid": "PublicReadGetObject",
                        "Effect": "Allow",
                        "Principal": "*",
                        "Action": ["s3:GetObject"],
                        "Resource": ["arn:aws:s3:::{}/*"]
                    }}
                ]
            }}"#,
            floatctf_public_bucket_name
        );

        client
            .put_bucket_policy()
            .bucket(floatctf_public_bucket_name)
            .policy(policy)
            .send()
            .await
            .map_err(s3_failure)?;
        info!("Bucket {} created", floatctf_public_bucket_name);
    }

    let public_dirs = ["images/", "weapons/", "challenges/"];
    for dir in public_dirs {
        client
            .put_object()
            .bucket(floatctf_public_bucket_name)
            .key(dir)
            .body(ByteStream::from(vec![]))
            .send()
            .await
            .map_err(s3_failure)?;
    }
    info!("Public dirs created");

    let floatctf_private_bucket_name = "floatctf-private";
    let floatctf_private_bucket = client
        .head_bucket()
        .bucket(floatctf_private_bucket_name)
        .send()
        .await;

    if floatctf_private_bucket.is_err() {
        client
            .create_bucket()
            .bucket(floatctf_private_bucket_name)
            .send()
            .await
            .map_err(s3_failure)?;
        info!("Bucket {} created", floatctf_private_bucket_name);
    }

    let private_dirs = ["writeups"];
    for dir in private_dirs {
        client
            .put_object()
            .bucket(floatctf_private_bucket_name)
            .key(dir)
            .body(ByteStream::from(vec![]))
            .send()
            .await
            .map_err(s3_failure)?;
    }
    info!("Private dirs created");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(
        kind: SdkFailureKind,
        http_status: Option<u16>,
        code: Option<&str>,
    ) -> S3FailureSignals {
        S3FailureSignals {
            kind,
            http_status,
            code: code.map(str::to_string),
        }
    }

    // ── 分类器：暂时性 ──

    #[test]
    fn dispatch_failure_is_transient() {
        // 连接被拒 / DNS 未解析 / 连接重置：RustFS 还没起来。
        assert_eq!(
            classify(&signals(SdkFailureKind::Dispatch, None, None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn timeout_is_transient() {
        assert_eq!(
            classify(&signals(SdkFailureKind::Timeout, None, None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn construction_failure_is_transient_bounded() {
        assert_eq!(
            classify(&signals(SdkFailureKind::Construction, None, None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn service_5xx_without_code_is_transient() {
        for status in [500, 502, 503, 504] {
            assert_eq!(
                classify(&signals(SdkFailureKind::Service, Some(status), None)),
                ErrorClass::Transient,
                "status {status} should be transient"
            );
        }
    }

    #[test]
    fn bare_service_error_without_status_is_transient() {
        // Phase 13 观察到的 `service error`：没有状态码、没有鉴权线索 → 重试。
        assert_eq!(
            classify(&signals(SdkFailureKind::Service, None, None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn service_404_and_other_transient_4xx_are_bounded() {
        // head_bucket 在 bucket 尚未创建时会 404；由 ensure_buckets 自己处理，
        // 分类上仍属「有界重试」，绝不无限循环。
        assert_eq!(
            classify(&signals(
                SdkFailureKind::Service,
                Some(404),
                Some("NoSuchBucket")
            )),
            ErrorClass::Transient
        );
        assert_eq!(
            classify(&signals(SdkFailureKind::Service, Some(429), None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn response_error_5xx_is_transient() {
        assert_eq!(
            classify(&signals(SdkFailureKind::Response, Some(503), None)),
            ErrorClass::Transient
        );
    }

    #[test]
    fn unknown_sdk_variant_is_transient_bounded() {
        assert_eq!(
            classify(&signals(SdkFailureKind::Unknown, None, None)),
            ErrorClass::Transient
        );
    }

    // ── 分类器：永久性 ──

    #[test]
    fn http_403_and_401_are_permanent() {
        for status in [401, 403] {
            assert_eq!(
                classify(&signals(SdkFailureKind::Service, Some(status), None)),
                ErrorClass::Permanent,
                "status {status} should be permanent"
            );
            assert_eq!(
                classify(&signals(SdkFailureKind::Response, Some(status), None)),
                ErrorClass::Permanent
            );
        }
    }

    #[test]
    fn auth_error_codes_are_permanent_even_without_auth_status() {
        for code in [
            "InvalidAccessKeyId",
            "SignatureDoesNotMatch",
            "AccessDenied",
            "InvalidSecurity",
            "ExpiredToken",
        ] {
            assert_eq!(
                classify(&signals(SdkFailureKind::Service, Some(400), Some(code))),
                ErrorClass::Permanent,
                "code {code} should be permanent"
            );
        }
    }

    #[test]
    fn auth_error_code_match_is_case_insensitive() {
        assert_eq!(
            classify(&signals(
                SdkFailureKind::Service,
                None,
                Some("invalidaccesskeyid")
            )),
            ErrorClass::Permanent
        );
    }

    #[test]
    fn non_auth_service_codes_stay_transient() {
        for code in ["InternalError", "ServiceUnavailable", "SlowDown"] {
            assert_eq!(
                classify(&signals(SdkFailureKind::Service, Some(503), Some(code))),
                ErrorClass::Transient,
                "code {code} should be transient"
            );
        }
    }

    // ── anyhow 往返 ──

    #[test]
    fn classify_anyhow_preserves_permanent_class() {
        let err = anyhow::Error::from(BucketInitError {
            class: ErrorClass::Permanent,
            detail: "service error (http_status=403)".to_string(),
        });
        let classified = classify_anyhow(&err);
        assert_eq!(classified.class, ErrorClass::Permanent);
        assert!(classified.detail.contains("http_status=403"));
    }

    #[test]
    fn classify_anyhow_falls_back_to_transient_bounded() {
        let err = anyhow!("something unexpected");
        let classified = classify_anyhow(&err);
        assert_eq!(classified.class, ErrorClass::Transient);
    }

    // ── 重试策略边界 ──

    #[test]
    fn default_policy_is_bounded() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 12, "attempts must be finite");
        assert_eq!(policy.total_deadline, Duration::from_secs(90));
        assert_eq!(policy.max_backoff, Duration::from_secs(8));
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let policy = RetryPolicy::default();
        let schedule: Vec<u64> = (1..=policy.max_attempts)
            .map(|attempt| policy.backoff_for_attempt(attempt).as_secs())
            .collect();
        assert_eq!(schedule, vec![1, 2, 4, 8, 8, 8, 8, 8, 8, 8, 8, 8]);
        // 封顶：任何一次退避都不超过 max_backoff。
        for attempt in 1..=64 {
            assert!(policy.backoff_for_attempt(attempt) <= policy.max_backoff);
        }
        assert_eq!(policy.backoff_for_attempt(0), Duration::ZERO);
    }

    #[test]
    fn total_backoff_schedule_fits_the_deadline() {
        // 12 次尝试的退避总和必须小于总预算，否则后面的尝试永远不会发生。
        let policy = RetryPolicy::default();
        let total: Duration = (1..=policy.max_attempts)
            .map(|attempt| policy.backoff_for_attempt(attempt))
            .sum();
        assert!(
            total < policy.total_deadline,
            "backoff total {total:?} must stay below {:?}",
            policy.total_deadline
        );
        // 抖动是额外叠加的，最多 250ms × 11 次，仍在预算内。
        assert!(total + policy.jitter * (policy.max_attempts - 1) < policy.total_deadline);
    }

    #[test]
    fn jitter_stays_below_configured_bound() {
        let policy = RetryPolicy::default();
        for _ in 0..64 {
            assert!(policy.jitter() < policy.jitter);
        }
        let no_jitter = RetryPolicy {
            jitter: Duration::ZERO,
            ..RetryPolicy::default()
        };
        assert_eq!(no_jitter.jitter(), Duration::ZERO);
    }

    #[test]
    fn exhaustion_error_mentions_bounded_window_and_last_error() {
        let policy = RetryPolicy::default();
        let last = BucketInitError {
            class: ErrorClass::Transient,
            detail: "service error (http_status=503)".to_string(),
        };
        let msg = exhaustion_error(&policy, Duration::from_secs(90), &last).to_string();
        assert!(
            msg.contains("did not become usable within the bounded retry window"),
            "unexpected message: {msg}"
        );
        assert!(
            msg.contains("12 attempts / 90s"),
            "unexpected message: {msg}"
        );
        assert!(msg.contains("http_status=503"), "unexpected message: {msg}");
    }

    #[test]
    fn detail_only_carries_status_and_code() {
        let service_error = signals(
            SdkFailureKind::Service,
            Some(403),
            Some("SignatureDoesNotMatch"),
        );
        assert_eq!(
            service_error.describe(),
            "service error (http_status=403, code=SignatureDoesNotMatch)"
        );
        assert_eq!(
            signals(SdkFailureKind::Dispatch, None, None).describe(),
            "dispatch failure"
        );
    }
}
