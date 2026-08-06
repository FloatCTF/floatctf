-- ============================================================
-- AWD Indexes
-- ============================================================

-- awd_events lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_events_event_id" ON "awd_events" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_events_status" ON "awd_events" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_events_phase" ON "awd_events" ("phase");

-- awd_team_networks lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_team_networks_event" ON "awd_team_networks" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_team_networks_team" ON "awd_team_networks" ("team_id");

-- awd_gamebox_templates lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_templates_event" ON "awd_gamebox_templates" ("event_id");

-- awd_gamebox_instances lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_event" ON "awd_gamebox_instances" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_team" ON "awd_gamebox_instances" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_template" ON "awd_gamebox_instances" ("template_id");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_status" ON "awd_gamebox_instances" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_gamebox_instances_container" ON "awd_gamebox_instances" ("container_id");

-- awd_wireguard_peers lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_event" ON "awd_wireguard_peers" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_team" ON "awd_wireguard_peers" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_user" ON "awd_wireguard_peers" ("user_id");
CREATE INDEX IF NOT EXISTS "idx_awd_wg_peers_status" ON "awd_wireguard_peers" ("status");

-- awd_rounds lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_rounds_event" ON "awd_rounds" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_rounds_status" ON "awd_rounds" ("status");

-- awd_flag_issues lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_event_round" ON "awd_flag_issues" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_instance" ON "awd_flag_issues" ("gamebox_instance_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_issues_hash" ON "awd_flag_issues" ("flag_hash");

-- awd_flag_submissions lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_event_round" ON "awd_flag_submissions" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_attacker" ON "awd_flag_submissions" ("attacker_team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_victim" ON "awd_flag_submissions" ("victim_team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_flag_submissions_user" ON "awd_flag_submissions" ("submitted_by_user_id");

-- awd_score_events lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_event" ON "awd_score_events" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_team" ON "awd_score_events" ("team_id");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_type" ON "awd_score_events" ("event_type");
CREATE INDEX IF NOT EXISTS "idx_awd_score_events_idempotency" ON "awd_score_events" ("idempotency_key");

-- awd_judge_batches lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_judge_batches_event" ON "awd_judge_batches" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_batches_round" ON "awd_judge_batches" ("round_id");

-- awd_judge_tasks lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_batch" ON "awd_judge_tasks" ("batch_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_event_round" ON "awd_judge_tasks" ("event_id", "round_id");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_status" ON "awd_judge_tasks" ("status");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_deadline" ON "awd_judge_tasks" ("deadline_at");
CREATE INDEX IF NOT EXISTS "idx_awd_judge_tasks_callback" ON "awd_judge_tasks" ("callback_idempotency_key");

-- awd_reset_records lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_reset_records_event" ON "awd_reset_records" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_reset_records_instance" ON "awd_reset_records" ("gamebox_instance_id");

-- awd_team_bans lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_team_bans_event" ON "awd_team_bans" ("event_id");
CREATE INDEX IF NOT EXISTS "idx_awd_team_bans_team" ON "awd_team_bans" ("team_id");

-- awd_precheck_runs lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_precheck_runs_event" ON "awd_precheck_runs" ("event_id");

-- awd_runtime_resources lookup indexes
CREATE INDEX IF NOT EXISTS "idx_awd_runtime_resources_event" ON "awd_runtime_resources" ("event_id");
