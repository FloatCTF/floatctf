//! 启动时构建调度器并注册全部处理器。

use std::sync::Arc;

use anyhow::Result;
use fcmc::DockerRuntime;
use tracing::info;

use crate::{
    core::AppConfig,
    infrastructure::LogService,
    infrastructure::realtime::EventPublisher,
    infrastructure::{WebDb, WebDocker, WebRustfs},
    modules::event::awd::{
        crypto::AwdCrypto,
        infrastructure::firewall::FirewallRuntime,
        scheduler::{
            AwdArchiveCleanupHandler, AwdAutoPrecheckHandler, AwdEventStartHandler,
            AwdHardeningEndHandler, AwdRoundEndHandler, AwdRoundStartHandler,
        },
    },
    modules::event::awdp::scheduler::{
        AwdpEvalWorkerHandler, AwdpPracticeJudgeHandler, AwdpTickHandler,
    },
    scheduler::{
        CheckPracticeEventHandler, CleanRunningInstancesHandler, CleanUnusedRustFSFilesHandler,
        TaskHandler, TaskScheduler,
    },
};

/// 构建调度器，并将每个处理器恰好注册一次。
pub async fn build_task_scheduler(
    db: WebDb,
    docker: WebDocker,
    rustfs: WebRustfs,
    logger: LogService,
    network: Arc<dyn crate::modules::event::awd::infrastructure::network::AwdNetworkRuntime>,
    firewall: Arc<dyn FirewallRuntime>,
    crypto: Arc<AwdCrypto>,
    publisher: Arc<dyn EventPublisher>,
    config: Arc<AppConfig>,
) -> Result<TaskScheduler> {
    let mut scheduler = TaskScheduler::new(db.clone(), docker.clone(), rustfs.clone(), logger);
    // seed 需要裸 connection；handler 构造完成后 db 可能被 move，提前克隆。
    let seed_db = db.get_ref().clone();

    // AWD 容器 runtime：自动 precheck 与 archive cleanup 共享同一实例。
    let awd_containers: Arc<dyn fcmc::AwdContainerRuntime> =
        Arc::new(DockerRuntime::new(docker.get_ref().clone()));

    let handlers: Vec<Arc<dyn TaskHandler>> = vec![
        Arc::new(CheckPracticeEventHandler {
            db: db.clone(),
            docker: docker.clone(),
            config: config.clone(),
        }),
        Arc::new(CleanRunningInstancesHandler {
            db: db.clone(),
            docker: docker.clone(),
        }),
        Arc::new(CleanUnusedRustFSFilesHandler {
            db: db.clone(),
            rustfs,
        }),
        Arc::new(AwdAutoPrecheckHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
            containers: awd_containers.clone(),
            crypto: crypto.clone(),
        }),
        Arc::new(AwdEventStartHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
            publisher: publisher.clone(),
        }),
        Arc::new(AwdRoundStartHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
            publisher: publisher.clone(),
        }),
        Arc::new(AwdRoundEndHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
            publisher: publisher.clone(),
        }),
        Arc::new(AwdHardeningEndHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
            publisher: publisher.clone(),
        }),
        Arc::new(AwdArchiveCleanupHandler {
            db: db.clone(),
            docker: docker.clone(),
            network,
            firewall,
            containers: awd_containers,
        }),
        Arc::new(AwdpTickHandler {
            db: db.clone(),
            docker: docker.clone(),
            config: config.clone(),
            publisher: publisher.clone(),
        }),
        Arc::new(AwdpEvalWorkerHandler {
            db: db.clone(),
            docker: docker.clone(),
            config: config.clone(),
        }),
        Arc::new(AwdpPracticeJudgeHandler {
            db: db.clone(),
            docker: docker.clone(),
            config: config.clone(),
        }),
    ];

    for handler in handlers {
        info!(
            "[Init] Registering scheduler handler: {}",
            handler.task_key()
        );
        scheduler.register_handler(handler)?;
    }

    scheduler.seed_startup_tasks().await?;
    // AWDP 引擎：3 个 recurring cron（tick / eval worker / practice judge ensure），幂等 seed。
    crate::modules::event::awdp::scheduler::seed_awdp_recurring_tasks(&seed_db).await?;
    scheduler.validate_enabled_task_keys().await?;
    Ok(scheduler)
}
