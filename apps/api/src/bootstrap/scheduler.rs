use std::sync::Arc;

use anyhow::Result;
use fcmc::DockerRuntime;
use tracing::info;

use crate::{
    infrastructure::LogService,
    infrastructure::{WebDb, WebDocker, WebRustfs},
    modules::event::awd_team::{
        crypto::AwdCrypto,
        infrastructure::firewall::FirewallRuntime,
        scheduler::{
            AwdArchiveCleanupHandler, AwdAutoPrecheckHandler, AwdEventStartHandler,
            AwdRoundEndHandler, AwdRoundGraceEndHandler, AwdRoundStartHandler,
        },
    },
    scheduler::{
        CheckPracticeEventHandler, CleanRunningInstancesHandler, CleanUnusedRustFSFilesHandler,
        TaskHandler, TaskScheduler,
    },
};

/// Build the scheduler and register every handler exactly once.
pub async fn build_task_scheduler(
    db: WebDb,
    docker: WebDocker,
    rustfs: WebRustfs,
    logger: LogService,
    network: Arc<dyn crate::modules::event::awd_team::infrastructure::network::AwdNetworkRuntime>,
    firewall: Arc<dyn FirewallRuntime>,
    crypto: Arc<AwdCrypto>,
) -> Result<TaskScheduler> {
    let mut scheduler = TaskScheduler::new(db.clone(), docker.clone(), rustfs.clone(), logger);

    // AWD 容器 runtime：自动 precheck 与 archive cleanup 共享同一实例。
    let awd_containers: Arc<dyn fcmc::AwdContainerRuntime> =
        Arc::new(DockerRuntime::new(docker.get_ref().clone()));

    let handlers: Vec<Arc<dyn TaskHandler>> = vec![
        Arc::new(CheckPracticeEventHandler { db: db.clone() }),
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
        }),
        Arc::new(AwdRoundStartHandler {
            db: db.clone(),
            network: network.clone(),
            firewall: firewall.clone(),
        }),
        Arc::new(AwdRoundEndHandler { db: db.clone() }),
        Arc::new(AwdRoundGraceEndHandler { db: db.clone() }),
        Arc::new(AwdArchiveCleanupHandler {
            db,
            docker: docker.clone(),
            network,
            containers: awd_containers,
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
    scheduler.validate_enabled_task_keys().await?;
    Ok(scheduler)
}
