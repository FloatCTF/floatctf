# Event Module Migration Baseline (E0)

Generated: 2026-07-28
Branch: awd

## Old directories
### modules
src/modules/events/admin_service.rs
src/modules/events/mod.rs
src/modules/events/player_service.rs
src/modules/events/repo.rs
src/modules/events/team_service.rs
src/modules/instances/domain.rs
src/modules/instances/mod.rs
src/modules/instances/repo.rs
src/modules/instances/runtime.rs
src/modules/instances/service.rs
src/modules/mod.rs
src/modules/submissions/dto.rs
src/modules/submissions/mod.rs
src/modules/submissions/repo.rs
src/modules/submissions/scoring.rs
src/modules/submissions/service.rs

### strategies/event
src/strategies/event/common.rs
src/strategies/event/dynamic_score.rs
src/strategies/event/factory.rs
src/strategies/event/implementations/awd_adapter.rs
src/strategies/event/implementations/jeopardy_practice.rs
src/strategies/event/implementations/jeopardy_single.rs
src/strategies/event/implementations/jeopardy_team.rs
src/strategies/event/implementations/mod.rs
src/strategies/event/jeopardy_core.rs
src/strategies/event/mod.rs
src/strategies/event/scoreboard.rs
src/strategies/event/trait_def.rs
src/strategies/event/trend.rs

### awd
src/awd/api/admin.rs
src/awd/api/auth.rs
src/awd/api/dto.rs
src/awd/api/internal.rs
src/awd/api/mod.rs
src/awd/api/player.rs
src/awd/crypto.rs
src/awd/domain/event_ext.rs
src/awd/domain/flag.rs
src/awd/domain/gamebox_ext.rs
src/awd/domain/mod.rs
src/awd/domain/network.rs
src/awd/domain/round_ext.rs
src/awd/domain/score.rs
src/awd/error.rs
src/awd/infrastructure/mod.rs
src/awd/infrastructure/network/keys.rs
src/awd/infrastructure/network/mod.rs
src/awd/infrastructure/network/runtime.rs
src/awd/infrastructure/persistence/mapping.rs
src/awd/infrastructure/persistence/mod.rs
src/awd/mod.rs
src/awd/repo/ban_repo.rs
src/awd/repo/event_repo.rs
src/awd/repo/flag_repo.rs
src/awd/repo/gamebox_repo.rs
src/awd/repo/judge_repo.rs
src/awd/repo/mod.rs
src/awd/repo/round_repo.rs
src/awd/repo/score_repo.rs
src/awd/repo/wireguard_repo.rs
src/awd/scheduler/mod.rs
src/awd/service/archive_service.rs
src/awd/service/deploy_service.rs
src/awd/service/event_service.rs
src/awd/service/flag_service.rs
src/awd/service/gamebox_service.rs
src/awd/service/judge_service.rs
src/awd/service/mod.rs
src/awd/service/network_policy_service.rs
src/awd/service/precheck_service.rs
src/awd/service/recovery_service.rs
src/awd/service/reset_service.rs
src/awd/service/score_service.rs
src/awd/service/submission_service.rs
src/awd/service/wireguard_service.rs
src/awd/system/command.rs
src/awd/system/conntrack.rs
src/awd/system/firewall.rs
src/awd/system/mod.rs
src/awd/system/wireguard.rs
src/awd/websocket.rs

## Reference counts
EventStrategy refs: 33
modules::events/instances/submissions: 20
crate::awd: 66

## Key call sites
src/modules/events/admin_service.rs:28:    modules::events::player_service::{get_scoreboard, get_trend},
src/modules/events/admin_service.rs:29:    modules::submissions::scoring::calculate_next_dynamic_score,
src/modules/events/player_service.rs:25:    modules::submissions::scoring::calculate_next_dynamic_score,
src/modules/events/player_service.rs:26:    strategies::event::{self, EventStrategyFactory, ScoreboardItem, TrendItem},
src/modules/events/player_service.rs:222:            let strategy = EventStrategyFactory::create(&event.r#type);
src/modules/events/player_service.rs:268:    let strategy = EventStrategyFactory::create(&event_ctx.event.r#type);
src/modules/events/player_service.rs:306:    let strategy = EventStrategyFactory::create(&event_ctx.event.r#type);
src/modules/events/player_service.rs:527:// ── Scoreboard / trend adapters (EventStrategyFactory) ────────────────────
src/modules/events/player_service.rs:535:    let strategy = EventStrategyFactory::create(&event.r#type);
src/modules/events/player_service.rs:548:    let strategy = EventStrategyFactory::create(&event.r#type);
src/modules/events/player_service.rs:597:    let strategy = EventStrategyFactory::create(&event.r#type);
src/modules/events/team_service.rs:11:use crate::awd::{AwdError, AwdResult};
src/modules/submissions/service.rs:14:    modules::instances::InstanceService,
src/api/service/submit.rs:52:    let strategy = event::EventStrategyFactory::create(&event_ctx.event.r#type);
src/api/service/instances.rs:141:    let strategy = event::EventStrategyFactory::create(&event_ctx.event.r#type);
src/api/service/instances.rs:182:    let strategy = event::EventStrategyFactory::create(&event_ctx.event.r#type);
src/api/service/events.rs:1://! Player event HTTP handlers — thin adapters over `modules::events::player_service`.
src/api/service/events.rs:9:    modules::events::player_service::{self as svc},
src/api/service/events.rs:14:pub use crate::modules::events::player_service::{
src/api/admin/mod.rs:298:    crate::awd::api::admin_routes(cfg);
src/api/admin/events.rs:1://! Admin event HTTP handlers — thin adapters over `modules::events::admin_service`.
src/api/admin/events.rs:7:    modules::events::admin_service::{self as svc},
src/api/admin/events.rs:12:pub use crate::modules::events::admin_service::{
src/api/app_error.rs:15:use crate::awd::AwdError;
src/api/error.rs:73:impl From<crate::awd::AwdError> for UniError {
src/api/error.rs:74:    fn from(value: crate::awd::AwdError) -> Self {
src/api/error.rs:75:        use crate::awd::AwdError;
src/awd/crypto.rs:30:use crate::awd::AwdError;
src/strategies/event/common.rs:23:    let service = crate::modules::instances::InstanceService::with_docker(
src/strategies/event/common.rs:43:    let service = crate::modules::instances::InstanceService::with_docker(
src/scheduler/handlers/pratice_handlers.rs:4:    modules::instances::InstanceService,
src/bootstrap/scheduler.rs:26:    network: Arc<dyn crate::awd::infrastructure::network::AwdNetworkRuntime>,
src/awd/scheduler/mod.rs:20:use crate::awd::{
src/awd/scheduler/mod.rs:61:        crate::awd::service::precheck_service::run_precheck(
src/awd/scheduler/mod.rs:74:    pub network: std::sync::Arc<dyn crate::awd::infrastructure::network::AwdNetworkRuntime>,
src/awd/scheduler/mod.rs:162:        crate::awd::repo::event_repo::update_phase(self.db.get_ref(), awd_event.id, phase.clone())
src/awd/scheduler/mod.rs:341:            crate::awd::service::archive_service::archive_event(
src/strategies/event/factory.rs:8:pub struct EventStrategyFactory;
src/strategies/event/factory.rs:10:impl EventStrategyFactory {
src/strategies/event/factory.rs:37:        let strategy = EventStrategyFactory::create(&EventType::AwdTeam);
src/strategies/event/factory.rs:50:            let _ = EventStrategyFactory::create(&et);
src/strategies/event/implementations/jeopardy_practice.rs:5:    modules::submissions::submit_practice,
src/awd/system/wireguard.rs:8:use crate::awd::{
src/awd/system/wireguard.rs:158:    crate::awd::system::conntrack::flush_for_cidr(runner, gamebox_cidr).await
src/bootstrap/state.rs:10:use crate::awd::crypto::AwdCrypto;
src/bootstrap/state.rs:11:use crate::awd::infrastructure::network::AwdNetworkRuntime;
src/strategies/event/dynamic_score.rs:1://! Compatibility re-export — implementation lives in `modules::submissions::scoring`.
src/strategies/event/dynamic_score.rs:3:pub use crate::modules::submissions::scoring::{calculate_next_dynamic_score, dynamic_score};
src/strategies/event/implementations/awd_adapter.rs:3://! WireGuard / Judge / rounds stay in `crate::awd`. Unsupported generic
src/awd/system/conntrack.rs:16:use crate::awd::{
src/bootstrap/mod.rs:95:    let awd_network: Arc<dyn crate::awd::infrastructure::network::AwdNetworkRuntime> =
src/bootstrap/mod.rs:99:                Arc::new(crate::awd::infrastructure::network::HostNetworkRuntime::new())
src/bootstrap/mod.rs:101:            _ => Arc::new(crate::awd::infrastructure::network::NoopNetworkRuntime),
src/bootstrap/mod.rs:156:                crypto: Arc::new(AwdCrypto::new(crate::awd::crypto::AwdSecret::new(vec![
src/bootstrap/mod.rs:210:            .configure(crate::awd::api::player_routes),
src/bootstrap/mod.rs:214:    cfg.configure(crate::awd::api::internal_routes);
src/awd/system/firewall.rs:11:use crate::awd::{
src/awd/domain/network.rs:6:use crate::awd::AwdError;
src/strategies/event/jeopardy_core.rs:2://! Formal flag scoring lives in `modules::submissions`.
src/strategies/event/jeopardy_core.rs:15:    modules::submissions::{
src/strategies/event/jeopardy_core.rs:23:pub use crate::modules::submissions::SolveSubject;
src/strategies/event/mod.rs:10:pub use factory::EventStrategyFactory;
src/awd/service/flag_service.rs:6:use crate::awd::{
src/awd/service/submission_service.rs:10:use crate::awd::{
src/awd/service/wireguard_service.rs:16:use crate::awd::{
src/awd/service/wireguard_service.rs:156:        use crate::awd::system::{command::RealCommandRunner, wireguard};
src/awd/infrastructure/persistence/mapping.rs:45:    use crate::awd::domain::AwdEventStatusExt;
src/awd/service/archive_service.rs:9:use crate::awd::{
src/awd/service/recovery_service.rs:9:use crate::awd::{
src/awd/service/recovery_service.rs:82:    let instances = crate::awd::repo::gamebox_repo::find_instances_by_event(db, event.event_id)
src/awd/service/recovery_service.rs:116:                    crate::awd::repo::gamebox_repo::update_instance_status(
src/awd/service/recovery_service.rs:202:    if let Some(round) = crate::awd::repo::round_repo::find_active_round(db, event_id)
src/awd/service/recovery_service.rs:207:        crate::awd::repo::round_repo::update_round_status(db, round.id, RoundStatus::Paused)
src/awd/service/reset_service.rs:21:use crate::awd::{
src/awd/service/reset_service.rs:153:        use crate::awd::crypto::{AwdCrypto, EncryptedBlob};
src/awd/service/judge_service.rs:12:use crate::awd::{
src/awd/service/network_policy_service.rs:6:use crate::awd::system::firewall::RenderedRules;
src/awd/service/network_policy_service.rs:7:use crate::awd::{
src/awd/service/precheck_service.rs:25:use crate::awd::{
src/awd/service/event_service.rs:6:use crate::awd::{

---

## Migration status (post E0–E9 residual cleanup)

- Old dirs deleted: `src/awd`, `modules/events|instances|submissions`, `strategies/`
- Handlers live under `modules/event/common/api` and `modules/event/jeopardy/api`
- `EventModuleRegistry` is on `AppState` and `web::Data`
- Routes: `/api/events/{id}/awd/*`, `/api/admin/events/{id}/awd/*`, capabilities at `/api/events/{id}/capabilities`
- Alignment check: `scripts/check_event_route_alignment.sh`
- Boundary tests: `modules::event::registry::e9_boundary_tests`
