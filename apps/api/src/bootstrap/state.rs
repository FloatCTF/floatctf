//! 应用状态——集中依赖容器。
//!
//! 新模块应依赖 `web::Data<AppState>`；迁移期间既有处理器仍可使用独立 `app_data`。

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::core::AppConfig;
use crate::infrastructure::audit::AuditService;
use crate::infrastructure::logging::LogService;
use crate::infrastructure::realtime::EventPublisher;
use crate::modules::event::awd::crypto::AwdCrypto;
use crate::modules::event::awd::infrastructure::firewall::FirewallRuntime;
use crate::modules::event::awd::infrastructure::network::AwdNetworkRuntime;
use crate::scheduler::TaskScheduler;
use fcmc::AwdContainerRuntime;

/// 中央应用状态，在全部请求处理器间共享。
///
/// 新模块应依赖 `web::Data<AppState>`，而非
/// 从 `app_data` 逐个提取资源。
#[derive(Clone)]
pub struct AppState {
    /// Typed process-static configuration.
    pub config: Arc<AppConfig>,
    /// Database connection.
    pub db: DatabaseConnection,
    /// Docker client.
    pub docker: bollard::Docker,
    /// S3-compatible storage client.
    pub storage: aws_sdk_s3::Client,
    /// Structured logging service.
    pub log: LogService,
    /// High-value audit trail.
    pub audit: AuditService,
    /// Real-time event publisher (WS hub or noop).
    pub publisher: Arc<dyn EventPublisher>,
    /// Task scheduler.
    pub scheduler: Arc<TaskScheduler>,
}

/// AWD 专用依赖。
///
/// 与 `AppState` 分离：非 AWD 处理器无需全部 AWD 依赖。
///
#[derive(Clone)]
pub struct AwdDependencies {
    /// Encryption service for AWD secrets and tokens.
    pub crypto: Arc<AwdCrypto>,
    /// Same publisher as AppState (shared).
    pub publisher: Arc<dyn EventPublisher>,
    /// Docker-backed AWD container/network runtime.
    pub containers: Arc<dyn AwdContainerRuntime>,
    /// Host WireGuard / conntrack runtime.
    pub network: Arc<dyn AwdNetworkRuntime>,
    /// Native nftables firewall runtime（唯一生产实现，Phase 1）。
    pub firewall: Arc<dyn FirewallRuntime>,
    /// 进程内限流器（P5-10）。
    pub rate_limiter: Arc<crate::infrastructure::ratelimit::RateLimiter>,
    /// 结构化审计（P5-11：管理员敏感操作）。
    pub audit: crate::infrastructure::audit::AuditService,
}

impl AppState {
    /// Create AppState from individual components.
    pub fn new(
        config: Arc<AppConfig>,
        db: DatabaseConnection,
        docker: bollard::Docker,
        storage: aws_sdk_s3::Client,
        log: LogService,
        audit: AuditService,
        publisher: Arc<dyn EventPublisher>,
        scheduler: Arc<TaskScheduler>,
    ) -> Self {
        Self {
            config,
            db,
            docker,
            storage,
            log,
            audit,
            publisher,
            scheduler,
        }
    }
}
