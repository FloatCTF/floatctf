# Fault injection / recovery checklist (manual acceptance)

Use on a **dedicated** staging or local stack — not production.
Mark each row after exercising the fault and confirming the expected recovery.

Environment notes:

- Prefer non-prod DB + Docker socket.
- Capture scheduler logs, API logs, and `docker ps` before/after.
- Restore services after each scenario before the next.

## Scenarios

| # | Fault | How to inject | Expected behaviour | Pass? |
|---|--------|---------------|--------------------|-------|
| 1 | Kill API process mid-request | `kill -9 <floatctf pid>` while clients hit health/login | Process restarts (systemd/compose); DB remains source of truth; no partial container orphan without DB row recovery on boot | ☐ |
| 2 | Kill scheduler mid-task | Stop process while a scheduled AWD task is `running` | On restart: recovery marks stale tasks / retries per `attempt_count` / `max_attempts` / `timeout_secs`; handler re-runs or fails cleanly | ☐ |
| 3 | Docker stop gamebox | `docker stop <gamebox-name>` for a deployed instance | Health/judge path reports down; score/down points rules apply when judge runs; reset/redeploy restores container via runtime | ☐ |
| 4 | Docker kill FlagServer | `docker kill` infrastructure flagserver for an event | Flag issue fails for gameboxes; platform logs error; restart/ensure path recreates container; no host network leak | ☐ |
| 5 | Docker kill JudgeServer | `docker kill` judgeserver | Batch health checks fail or timeout; callback not success; restart restores `/health` | ☐ |
| 6 | Network policy fail | Break/remove event bridge or apply deny-all on gamebox net | Isolation holds (no unexpected cross-team path); restore policy + conntrack flush on phase change restores intended connectivity | ☐ |
| 7 | DB disconnect | Pause Postgres or `iptables`/compose pause the DB container briefly | API returns 5xx/timeouts; no silent data corruption; reconnect after DB up; in-flight scheduler tasks fail and retry rather than double-apply side effects unchecked | ☐ |
| 8 | DB unique / concurrent deploy | Trigger two deploys for same team/gamebox | Unique constraints / service locks prevent duplicate containers; one winner, other errors cleanly | ☐ |
| 9 | Platform restart mid-event | Full stack restart during attack phase | Recovery service reconciles containers from DB; WG/firewall（nftables desired-state reconcile）restore 由 P1-11/P1-16 执行；event phase preserved | ☐ |
| 10 | Internal token mismatch | Wrong `INTERNAL_TOKEN` on Flag/Judge | Internal routes reject; no flag leakage; fix token + restart restores issue/judge | ☐ |

## Scheduler-specific checks

- [ ] Task rows expose `attempt_count`, `timeout_secs`, `max_attempts` (see migration / entity).
- [ ] Timed-out `running` tasks become retryable or failed after recovery, not stuck forever.
- [ ] Handler identity is stable across restarts (no “unknown handler” drop without log).

## Docker / runtime checks

- [ ] All create/remove goes through `DockerContainerRuntime` / AWD runtime (no ad-hoc free helpers).
- [ ] Test/smoke resources use labels; production labels untouched by E2E scripts.
- [ ] `scripts/e2e_flag_judge.sh` with `RUN_DOCKER_TESTS=1` cleans up on exit.

## Network / phase checks

- [ ] Phase switch flushes conntrack for the event（reconcile 后 flush_event_connections）。
- [ ] Hardening vs attack vs pause matrix still matches plan (flag issue, submission, judge allow/deny).
- [ ] Failure of one event’s net does not tear down another event’s resources.

## Observability

- [ ] Errors include structured codes suitable for ops (not only free-text).
- [ ] SSE / realtime (if enabled) eventually reflects gamebox state after recovery.

## Sign-off

| Role | Name | Date | Notes |
|------|------|------|-------|
| Operator | | | |
| Dev | | | |

Related scripts:

- `scripts/e2e_flag_judge.sh` — Flag/Judge container smoke
- `scripts/load_smoke.sh` — concurrent HTTP against `BASE_URL`
- `scripts/verify_refactor.sh` — structural + optional optional harness hooks
