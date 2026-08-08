//! Application state — centralized dependency container.
//!
//! `AppState` holds all shared resources and is injected via `web::Data<AppState>`.
//! Existing handlers continue to use individual `app_data` entries during migration.

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::core::AppConfig;
use crate::infrastructure::audit::AuditService;
use crate::infrastructure::logging::LogService;
use crate::infrastructure::realtime::EventPublisher;
use crate::modules::event::EventModuleRegistry;
use crate::modules::event::awd_team::crypto::AwdCrypto;
use crate::modules::event::awd_team::infrastructure::firewall::FirewallRuntime;
use crate::modules::event::awd_team::infrastructure::network::AwdNetworkRuntime;
use crate::scheduler::TaskScheduler;
use fcmc::AwdContainerRuntime;

/// Central application state, shared across all request handlers.
///
/// New modules should depend on `web::Data<AppState>` instead of
/// extracting individual resources from `app_data`.
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
    /// Competition mode registry (Jeopardy modes + capability dispatch).
    pub event_registry: EventModuleRegistry,
    /// Domain service aggregation.
    pub modules: ModuleServices,
}

/// AWD-specific dependencies.
///
/// Separated from `AppState` because not all AWD deps are needed by
/// non-AWD handlers.
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
        event_registry: EventModuleRegistry,
    ) -> Self {
        let modules = ModuleServices {
            event: event_registry.clone(),
        };
        Self {
            config,
            db,
            docker,
            storage,
            log,
            audit,
            publisher,
            scheduler,
            event_registry,
            modules,
        }
    }
}

/// Aggregated domain services (expand as modules grow DI needs).
#[derive(Clone, Default)]
pub struct ModuleServices {
    pub event: crate::modules::event::EventModuleRegistry,
}
