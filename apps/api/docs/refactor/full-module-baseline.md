# Full Modularization Baseline (Wave 0)

Date: 2026-07-28
API HEAD: 7e4c086

## Event module (already migrated)
src/modules/event/awd_team/api/admin.rs
src/modules/event/awd_team/api/auth.rs
src/modules/event/awd_team/api/dto.rs
src/modules/event/awd_team/api/internal.rs
src/modules/event/awd_team/api/mod.rs
src/modules/event/awd_team/api/player.rs
src/modules/event/awd_team/crypto.rs
src/modules/event/awd_team/domain/event_ext.rs
src/modules/event/awd_team/domain/flag.rs
src/modules/event/awd_team/domain/gamebox_ext.rs
src/modules/event/awd_team/domain/mod.rs
src/modules/event/awd_team/domain/network.rs
src/modules/event/awd_team/domain/round_ext.rs
src/modules/event/awd_team/domain/score.rs
src/modules/event/awd_team/error.rs
src/modules/event/awd_team/infrastructure/mod.rs
src/modules/event/awd_team/infrastructure/network/keys.rs
src/modules/event/awd_team/infrastructure/network/mod.rs
src/modules/event/awd_team/infrastructure/network/runtime.rs
src/modules/event/awd_team/infrastructure/persistence/mapping.rs
src/modules/event/awd_team/infrastructure/persistence/mod.rs
src/modules/event/awd_team/mod.rs
src/modules/event/awd_team/repo/ban_repo.rs
src/modules/event/awd_team/repo/event_repo.rs
src/modules/event/awd_team/repo/flag_repo.rs
src/modules/event/awd_team/repo/gamebox_repo.rs
src/modules/event/awd_team/repo/judge_repo.rs
src/modules/event/awd_team/repo/mod.rs
src/modules/event/awd_team/repo/round_repo.rs
src/modules/event/awd_team/repo/score_repo.rs
src/modules/event/awd_team/repo/wireguard_repo.rs
src/modules/event/awd_team/scheduler/mod.rs
src/modules/event/awd_team/service/archive_service.rs
src/modules/event/awd_team/service/deploy_service.rs
src/modules/event/awd_team/service/event_service.rs
src/modules/event/awd_team/service/flag_service.rs
src/modules/event/awd_team/service/gamebox_service.rs
src/modules/event/awd_team/service/judge_service.rs
src/modules/event/awd_team/service/mod.rs
src/modules/event/awd_team/service/network_policy_service.rs
src/modules/event/awd_team/service/precheck_service.rs
src/modules/event/awd_team/service/recovery_service.rs
src/modules/event/awd_team/service/reset_service.rs
src/modules/event/awd_team/service/score_service.rs
src/modules/event/awd_team/service/submission_service.rs
src/modules/event/awd_team/service/wireguard_service.rs
src/modules/event/awd_team/system/command.rs
src/modules/event/awd_team/system/conntrack.rs
src/modules/event/awd_team/system/firewall.rs
src/modules/event/awd_team/system/mod.rs
src/modules/event/awd_team/system/wireguard.rs
src/modules/event/awd_team/websocket.rs
src/modules/event/common/api/admin.rs
src/modules/event/common/api/mod.rs
src/modules/event/common/api/player.rs
src/modules/event/common/application/admin_service.rs
src/modules/event/common/application/mod.rs
src/modules/event/common/application/player_service.rs
src/modules/event/common/application/team_service.rs
src/modules/event/common/domain/capability.rs
src/modules/event/common/domain/event_type.rs
src/modules/event/common/domain/event.rs
src/modules/event/common/domain/mod.rs
src/modules/event/common/domain/participation.rs
src/modules/event/common/domain/time_state.rs
src/modules/event/common/infrastructure/event_repository.rs
src/modules/event/common/infrastructure/mod.rs
src/modules/event/common/mod.rs
src/modules/event/error.rs
src/modules/event/jeopardy_practice/application.rs
src/modules/event/jeopardy_practice/mod.rs
src/modules/event/jeopardy_practice/policy.rs
src/modules/event/jeopardy_single/application.rs
src/modules/event/jeopardy_single/mod.rs
src/modules/event/jeopardy_single/policy.rs
src/modules/event/jeopardy_team/application.rs
src/modules/event/jeopardy_team/mod.rs
src/modules/event/jeopardy_team/policy.rs
src/modules/event/jeopardy/api/instances.rs
src/modules/event/jeopardy/api/mod.rs

## Remaining api/admin handlers
src/api/admin/announcements.rs
src/api/admin/challenge_sets.rs
src/api/admin/challenges.rs
src/api/admin/database.rs
src/api/admin/discussions.rs
src/api/admin/docker.rs
src/api/admin/download.rs
src/api/admin/dto.rs
src/api/admin/event_announcements.rs
src/api/admin/event_challenges.rs
src/api/admin/event_logs.rs
src/api/admin/event_teams.rs
src/api/admin/event_users.rs
src/api/admin/event_writeups.rs
src/api/admin/instances.rs
src/api/admin/logs.rs
src/api/admin/mod.rs
src/api/admin/scheduled_tasks.rs
src/api/admin/settings.rs
src/api/admin/super_admin.rs
src/api/admin/system.rs
src/api/admin/terminal.rs
src/api/admin/users.rs
src/api/admin/weapons.rs

## Remaining api/service handlers
src/api/service/announcements.rs
src/api/service/challenge_sets.rs
src/api/service/challenge_solves.rs
src/api/service/challenge_writeups.rs
src/api/service/challenges.rs
src/api/service/discussions.rs
src/api/service/download.rs
src/api/service/mod.rs
src/api/service/super_admin.rs
src/api/service/uploads.rs
src/api/service/users.rs
src/api/service/weapons.rs

## Root legacy files
src/auth.rs
src/config.rs
src/db.rs
src/lib.rs
src/log.rs
src/main.rs
src/prelude.rs

## OOB/kv usage (business)

## Event strategy residual
