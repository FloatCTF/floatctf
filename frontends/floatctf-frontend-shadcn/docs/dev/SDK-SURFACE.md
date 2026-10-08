# SDK 公共面速查（自动生成，勿手改）

> 由 `scripts/gen-sdk-surface.py` 从 `packages/sdk/src/api/**`、`packages/react/src/**`、
> `packages/frontend-runtime/src/**` 抽取真实声明生成。符号不存在就是不存在 —— **不要臆造**。
> 逃生舱写法见 AI-FRONTEND-GUIDE §5.4：`client.serviceHttp` / `client.adminHttp` / `client.transport.*` / 原生 WebSocket。

## `packages/sdk/src/api/admin/announcements.ts`

```ts
export type AnnouncementAdminApi = ReturnType<typeof createAnnouncementAdminApi>;
```

## `packages/sdk/src/api/admin/auth.ts`

```ts
export type AdminLoginFn = ReturnType<typeof createAdminLoginFn>;
```

## `packages/sdk/src/api/admin/challenges.ts`

```ts
export type ChallengeAdminApi = ReturnType<typeof createChallengeAdminApi>;
```

## `packages/sdk/src/api/admin/dashboard.ts`

```ts
export type DashboardSummary = {
	stats: {
		users: number;
		events: number;
		challenges: number;
		weapons: number;
		announcements: number;
		discussions: number;
		instances: number;
		gameboxes: number;
	};
	attention: {
		failed_tasks: Array<{
			task_name: string;
			task_key: string;
			error_msg: string | null;
			attempt_count: number;
			max_attempts: number;
			updated_at: string;
		}>;
		error_logs_24h: number;
		awd_alerts: Array<{
			event_id: string;
			title: string;
			status: string;
			phase: string;
		}>;
	};
	events: Array<{
		event_id: string;
		title: string;
		family: string;
		purpose: string;
		participant_mode: string;
		start_time: string;
		end_time: string | null;
		hidden: boolean;
		awd: {
			status: string;
			phase: string;
			started_at: string | null;
		} | null;
	}>;
	activity: {
		recent_solves: Array<{
			nickname: string;
			avatar: string | null;
			challenge_name: string;
			solved_at: string;
		}>;
		recent_signups: Array<{
			nickname: string;
			username: string;
			avatar: string | null;
			created_at: string;
		}>;
	};
};
```

```ts
export type DashboardAdminApi = ReturnType<typeof createDashboardAdminApi>;
```

## `packages/sdk/src/api/admin/database.ts`

```ts
export type DatabaseAdminApi = ReturnType<typeof createDatabaseAdminApi>;
```

## `packages/sdk/src/api/admin/discussions.ts`

```ts
export type DiscussionAdminApi = ReturnType<typeof createDiscussionAdminApi>;
```

## `packages/sdk/src/api/admin/docker.ts`

```ts
export interface FloatDockerContainer {
	id: string;
	name: string;
	status: string;
	image: string;
	ports: string;
	created: number;
}
```

```ts
export interface ContainerInfo {
	id: string;
	names: string[];
	image: string;
	image_id: string;
	state: string;
	status: string;
	created: number;
	ports: PortInfo[];
}
```

```ts
export interface PortInfo {
	IP?: string;
	PrivatePort: number;
	PublicPort?: number;
	Type: string;
}
```

```ts
export interface ImageInfo {
	id: string;
	repo_tags: string[];
	size: number;
	created: number;
}
```

```ts
export interface NetworkInfo {
	id: string;
	name: string;
	driver: string;
	scope: string;
	ipam_driver: string;
	subnet?: string;
	gateway?: string;
	created: number;
}
```

```ts
export type DockerAdminApi = ReturnType<typeof createDockerAdminApi>;
```

## `packages/sdk/src/api/admin/download.ts`

```ts
export type DownloadAdminApi = ReturnType<typeof createDownloadAdminApi>;
```

## `packages/sdk/src/api/admin/event_announcements.ts`

```ts
export type EventAnnouncementAdminApi = ReturnType<typeof createEventAnnouncementAdminApi>;
```

## `packages/sdk/src/api/admin/event_challenges.ts`

```ts
export type EventChallengeAdminApi = ReturnType<typeof createEventChallengeAdminApi>;
```

## `packages/sdk/src/api/admin/event_logs.ts`

```ts
export type EventLogAdminApi = ReturnType<typeof createEventLogAdminApi>;
```

## `packages/sdk/src/api/admin/event_teams.ts`

```ts
export type EventTeamAdminApi = ReturnType<typeof createEventTeamAdminApi>;
```

## `packages/sdk/src/api/admin/event_users.ts`

```ts
export type EventUserAdminApi = ReturnType<typeof createEventUserAdminApi>;
```

## `packages/sdk/src/api/admin/event_writeups.ts`

```ts
export type EventWriteupAdminApi = ReturnType<typeof createEventWriteupAdminApi>;
```

## `packages/sdk/src/api/admin/events.ts`

```ts
export type EventAdminApi = ReturnType<typeof createEventAdminApi>;
```

## `packages/sdk/src/api/admin/instances.ts`

```ts
export type AdminInstanceRow = {
	id: string;
	instance_type: "challenge" | "gamebox";
	status: string;
	identifier: string;
	event_id?: string | null;
	event_title?: string | null;
	user_id?: string | null;
	user_name?: string | null;
	team_id?: string | null;
	team_name?: string | null;
	content_title?: string | null;
	challenge_id?: string | null;
	gamebox_id?: string | null;
	runtime_generation?: number | null;
	created_at: string;
	updated_at: string;
	destroy_at?: string | null;
};
```

```ts
export type InstanceAdminApi = ReturnType<typeof createInstanceAdminApi>;
```

## `packages/sdk/src/api/admin/logs.ts`

```ts
export type LogsAdminApi = ReturnType<typeof createLogsAdminApi>;
```

## `packages/sdk/src/api/admin/scheduled_tasks.ts`

```ts
export type ScheduledTaskAdminApi = ReturnType<typeof createScheduledTaskAdminApi>;
```

## `packages/sdk/src/api/admin/settings.ts`

```ts
export type SettingsDto = Settings & {
	resolved_value: string;
};
```

```ts
export type SettingAdminApi = ReturnType<typeof createSettingAdminApi>;
```

## `packages/sdk/src/api/admin/super_admin.ts`

```ts
export type SuperAdminApi = ReturnType<typeof createSuperAdminApi>;
```

## `packages/sdk/src/api/admin/system.ts`

```ts
export type SystemAdminApi = ReturnType<typeof createSystemAdminApi>;
```

## `packages/sdk/src/api/admin/users.ts`

```ts
export type UserAdminApi = ReturnType<typeof createUserAdminApi>;
```

## `packages/sdk/src/api/admin/weapons.ts`

```ts
export type WeaponsAdminApi = ReturnType<typeof createWeaponsAdminApi>;
```

## `packages/sdk/src/api/awd.ts`

```ts
export type AwdEventStatus = {
	event_id: string;
	status: string;
	phase: string;
	round_count: number | null;
	round_duration_secs: number;
	initial_score: number;
	free_reset_count: number;
	extra_reset_penalty: number;
	judge_max_concurrency: number;
	judge_default_timeout_secs: number;
	judge_retry_interval_secs: number;
	archive_retention_hours: number;
	planned_start_at: string | null;
	verified_at: string | null;
	started_at: string | null;
	updated_at: string;
	/** Derived: final round completed, Judge still settling. Competition closed. */
	final_settlement: boolean;
};
```

```ts
export type AwdEventConfigInput = {
	/** PATCH 乐观锁版本；首次创建可省略。 */
	expected_updated_at?: string;
	round_count?: number;
	round_duration_secs?: number;
	initial_score?: number;
	free_reset_count?: number;
	extra_reset_penalty?: number;
	judge_max_concurrency?: number;
	judge_default_timeout_secs?: number;
	judge_retry_interval_secs?: number;
	archive_retention_hours?: number;
	planned_start_at?: string;
	clear_planned_start?: boolean;
};
```

```ts
export type AwdGameBox = {
	id: string;
	team_id: string;
	event_gamebox_id: string;
	gamebox_name: string;
	status: string;
	gamebox_ip: string;
	container_name: string;
	health_status: string;
};
```

```ts
export type GameBoxLibraryDto = {
	id: string;
	name: string;
	safe_name: string;
	category: string;
	description: string;
	hidden: boolean;
	version: string | null;
	build_status: string | null;
	package_digest: string | null;
	image_ref: string | null;
	image_repo_digest: string | null;
	username: string | null;
	cpu_millis: number | null;
	memory_bytes: number | null;
	pids_limit: number | null;
	healthchecks_json: unknown | null;
	judge_script_name: string | null;
	judge_script_content: string | null;
	judge_args_json: unknown | null;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
};
```

```ts
export type ImportGameBoxResponse = {
	gamebox: GameBoxLibraryDto;
};
```

```ts
export type GameBoxScanItem = {
	safe_name: string;
	name: string | null;
	version: string | null;
	status: "added" | "skipped" | "error";
	message: string;
};
```

```ts
export type GameBoxCheckResult = {
	id: string;
	gamebox_name: string;
	is_ok: boolean;
	docker_image: boolean;
	package_dir: boolean;
};
```

```ts
export type GameBoxBuildResult = {
	gamebox_name: string;
	is_ok: boolean;
	message: string;
};
```

```ts
export type EventGameBoxDto = {
	id: string;
	gamebox_id: string;
	gamebox_name: string;
	gamebox_safe_name: string;
	gamebox_version: string | null;
	host_offset: number;
	enabled: boolean;
	hidden: boolean;
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
	attack_score: number;
	judge_down_penalty: number;
	first_bonus: number;
	created_at: string;
};
```

```ts
export type GameBoxConfigPayload = {
	name?: string;
	category?: string;
	description?: string;
	hidden?: boolean;
};
```

```ts
export type AwdScoreRow = {
	team_id: string;
	team_name: string;
	attack_score: number;
	defense_score: number;
	total_score: number;
	rank: number;
};
```

```ts
export type AwdPrecheckRun = {
	id: string;
	event_id: string;
	status: string;
	trigger?: string | null;
	revision?: number | null;
	error_msg?: string | null;
	started_at?: string | null;
	completed_at?: string | null;
};
```

```ts
export type AwdPlayerStatus = {
	event_id: string;
	status: string;
	phase: string;
	current_round: number | null;
	round_count: number | null;
	banned: boolean;
	score: number | null;
	/** Derived: final round completed, Judge still settling. Competition closed. */
	final_settlement: boolean;
};
```

```ts
export type WireGuardConfigResponse = {
	config: string;
};
```

```ts
export type PlatformNetworkSettings = {
	gamebox_pool: string;
	gamebox_event_prefix: number;
	gamebox_team_prefix: number;
	wireguard_pool: string;
	wireguard_event_prefix: number;
	wireguard_team_prefix: number;
	wireguard_port_min: number;
	wireguard_port_max: number;
	wireguard_public_endpoint: string | null;
	updated_at: string;
	// 容量预览（§67，来自 GET 计算）
	gamebox_event_capacity: number;
	gamebox_team_capacity_per_event: number;
	gamebox_hosts_per_team: number;
	wireguard_event_capacity: number;
	wireguard_team_capacity_per_event: number;
	wireguard_port_capacity: number;
};
```

```ts
export type PlatformNetworkSettingsUpdate = {
	gamebox_pool?: string;
	gamebox_event_prefix?: number;
	gamebox_team_prefix?: number;
	wireguard_pool?: string;
	wireguard_event_prefix?: number;
	wireguard_team_prefix?: number;
	wireguard_port_min?: number;
	wireguard_port_max?: number;
	wireguard_public_endpoint?: string | null;
};
```

```ts
export type PlatformNetworkSettingsUpdateResponse = Partial<
	Pick<
		PlatformNetworkSettings,
		| "gamebox_pool"
		| "wireguard_pool"
		| "wireguard_public_endpoint"
		| "updated_at"
	>
> & { note?: string };
```

```ts
export type PlatformNetworkHealth = {
	nftables: string;
	wireguard: string;
	docker: string;
	firewall_runtime: string;
	floatctf_table: string;
	docker_firewall_backend: string | null;
	firewalld: string;
	ipv4_forwarding: string | null;
	ipv6_policy: string;
	capability_supported: boolean;
	notes: string[];
};
```

```ts
export type PlatformNetworkAllocation = {
	event_id: string;
	event_title: string | null;
	kind: string;
	cidr: string;
	allocated_at: string;
	released_at: string | null;
	active: boolean;
};
```

```ts
export type EventNetworkInfo = {
	event_id: string;
	allocation_mode: string;
	gamebox_cidr: string;
	wireguard_cidr: string;
	infrastructure_subnet: string;
	flagserver_ip: string;
	judgeserver_ip: string;
	wireguard_interface_name: string;
	wireguard_listen_port: number;
	docker_network_name: string;
	locked: boolean;
};
```

```ts
export type NetworkAllocationRequest = {
	allocation_mode?: "automatic" | "manual";
	gamebox_cidr?: string;
	wireguard_cidr?: string;
	wireguard_listen_port?: number;
};
```

```ts
export type AwdAdminApi = ReturnType<typeof createAwdAdminApi>;
```

```ts
export type AwdPlayerApi = ReturnType<typeof createAwdPlayerApi>;
```

```ts
export type SshInstanceInfo = {
	id: string;
	gamebox_ip: string;
	username: string;
	container_name: string;
	health_status: string;
};
```

```ts
export type SshAccessResponse = {
	port: number;
	password: string;
	instances: SshInstanceInfo[];
};
```

## `packages/sdk/src/api/awdp.ts`

```ts
export type AwdpPhase = "pending" | "break" | "preparing_fix" | "fix" | "ended";
```

```ts
export type AwdpEndpoint = {
	protocol: "http" | "tcp";
	container_port: number;
	public_host: string;
	public_port: number;
};
```

```ts
export type AwdpInstance = {
	instance_id: string;
	runtime_state: string;
	runtime_generation: number;
	/** 玩家手动 Reset 次数（比赛 subject×gamebox；练习恒 0）。 */
	reset_count: number;
	endpoints: AwdpEndpoint[];
};
```

```ts
export type AwdpGameBox = {
	id: string;
	gamebox_id: string;
	name: string;
	category: string;
	enabled: boolean;
	hidden: boolean;
	exposed: [string, number][];
	broken: boolean;
	instance: AwdpInstance | null;
	source_code_dir?: string | null;
};
```

```ts
export type AwdpOverview = {
	event_id: string;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	total_rounds: number;
	break_score: number;
	fix_round_score: number;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
	my_score: number;
	gameboxes: AwdpGameBox[];
};
```

```ts
export type BreakSubmitResponse = {
	accepted: boolean;
	scored: boolean;
	already_broken: boolean;
};
```

```ts
export type PatchSubmitResponse = {
	status: "applied" | "failed";
	error_message?: string | null;
};
```

```ts
export type ManualCheckDto = {
	/** 创建的 manual 评估 id。 */
	evaluation_id: string;
	/** 终态（"completed"）；结果字段随 status 一并返回。 */
	status: string;
	healthcheck_ok: boolean | null;
	healthcheck_detail: string[] | null;
	judge_ok: boolean | null;
	judge_detail: string | null;
	/** exploit 诊断展示（不计分）：练习与竞赛 manual Test Check 均执行；未执行到 exploit 时为 null。 */
	exploit_ok: boolean | null;
	exploit_detail: string | null;
};
```

```ts
export type AllCheckDto = {
	/** 终态：patched=修复成功（swept=true）；其余 = 本次未通过（不落账，等官方 check）。 */
	status: string;
	/** status=patched：剩余回合已全部计分且 run 已结束。 */
	swept: boolean;
	swept_rounds: number;
	target_round: number;
	healthcheck_detail: string | null;
	judge_detail: string | null;
	exploit_detail: string | null;
};
```

```ts
export type AwdpEventConfigDto = {
	event_id: string;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	break_score: number;
	fix_round_score: number;
	total_rounds: number;
	configuration_generation: number;
	updated_at: string;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
};
```

```ts
export type AwdpConfigPatchInput = {
	expected_updated_at?: string;
	break_duration_secs?: number;
	fix_duration_secs?: number;
	fix_round_interval_secs?: number;
	break_score?: number;
	fix_round_score?: number;
};
```

```ts
export type AwdpAdminEventGameBoxDto = {
	id: string;
	event_id: string;
	gamebox_id: string;
	name: string;
	safe_name: string;
	category: string;
	enabled: boolean;
	hidden: boolean;
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	awdp_capable: boolean;
	awdp_source_code_dir: string | null;
	build_status: string | null;
};
```

```ts
export type AwdpAdminInstanceDto = {
	instance_id: string;
	event_gamebox_id: string;
	gamebox_name: string;
	owner_user_id: string | null;
	owner_team_id: string | null;
	runtime_state: string;
	runtime_generation: number;
	container_name: string;
	endpoints: AwdpEndpoint[];
};
```

```ts
export type AwdpScoreRow = {
	subject_id: string;
	subject_name: string;
	break_score: number;
	fix_score: number;
	total_score: number;
	rank: number;
};
```

```ts
export type AwdpScoreboardGameBox = {
	id: string;
	name: string;
	category: string;
};
```

```ts
export type AwdpScoreboardRound = {
	sequence: number;
	status: string;
	cutoff_at: string;
};
```

```ts
export type AwdpScoreboardRow = {
	subject_id: string;
	subject_name: string;
	rank: number;
	break_score: number;
	fix_score: number;
	total_score: number;
	/** 当前登录用户/队伍（自己的行高亮）。 */
	is_me: boolean;
	/** 每题是否已攻破（对齐 gameboxes）。 */
	break_status: boolean[];
	/** 每题 fix 实际计分（score events 权威；对齐 gameboxes）。 */
	fix_gamebox_score: number[];
	/** 每题 × 每回合官方评估终态（[gamebox][round]；无实例/未评估 = null）。 */
	fix_round_status: (string | null)[][];
};
```

```ts
export type AwdpScoreboardDetail = {
	/** individual / team（前端按赛制区分"人数"/"队伍数"文案）。 */
	participant_mode: "individual" | "team";
	gameboxes: AwdpScoreboardGameBox[];
	rounds: AwdpScoreboardRound[];
	rows: AwdpScoreboardRow[];
};
```

```ts
export type AwdpTrendPoint = {
	name: string;
	score: number;
	time: string;
};
```

```ts
export type AwdpTrendItem = {
	name: string;
	points: AwdpTrendPoint[];
};
```

```ts
export type AwdpDataGameBox = {
	id: string;
	name: string;
	category: string;
	break_count: number;
	fix_count: number;
};
```

```ts
export type AwdpDataActivity = {
	subject_name: string;
	gamebox_name: string;
	gamebox_category: string;
	action: "break" | "fix";
	delta: number;
	created_at: string;
};
```

```ts
export type AwdpDataPresent = {
	event: import("../entity/index.js").Events;
	user_count: number;
	team_count: number;
	gameboxes: AwdpDataGameBox[];
	scoreboard_top10: AwdpScoreRow[];
	trend: AwdpTrendItem[];
	recent_activity: AwdpDataActivity[];
};
```

```ts
export type AwdpAdminApi = ReturnType<typeof createAwdpAdminApi>;
```

```ts
export type AwdpPlayerApi = ReturnType<typeof createAwdpPlayerApi>;
```

```ts
export type AwdpRoundDto = {
	id: string;
	sequence: number;
	starts_at: string;
	cutoff_at: string;
	status: string;
};
```

```ts
export type AwdpEvaluationDto = {
	id: string;
	instance_id: string;
	event_gamebox_id: string;
	fix_round_id: string | null;
	round_sequence: number | null;
	kind: "manual" | "official";
	status: string;
	healthcheck_result: string | null;
	judge_result: string | null;
	finished_at: string | null;
};
```

## `packages/sdk/src/api/awdpRuns.ts`

```ts
export type GameBoxCatalogDto = {
	id: string;
	name: string;
	description: string;
	category: string;
	version: string | null;
	/** 作者（gameboxes.username）。 */
	author?: string | null;
	updated_at?: string;
	awdp_capable: boolean;
	recommended_cpu_millis: number;
	recommended_memory_bytes: number;
	recommended_pids_limit: number;
	/** 当前用户同 gamebox 的 active Practice Run（若有）。 */
	active_training: {
		run_id: string;
		phase: AwdpPhase;
		score: number;
	} | null;
	/** 当前用户是否训练过该 GameBox（练习 run 至少启动过一次实例）。 */
	solved: boolean;
};
```

```ts
export type RunInstanceDto = {
	instance_id: string;
	gamebox_id: string;
	runtime_state: string;
	runtime_generation: number;
	/** 玩家手动 Reset 次数（练习恒 0）。 */
	reset_count: number;
	broken: boolean;
	endpoints: AwdpEndpoint[];
};
```

```ts
export type AwdpRunDto = {
	run_id: string;
	gamebox_id: string;
	/** 后端实现补充：practice run 的 GameBox 展示信息（无需再查目录）。 */
	gamebox_name: string;
	gamebox_category: string;
	gamebox_description: string;
	event_id: string | null;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	break_score: number;
	fix_round_score: number;
	total_rounds: number;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
	my_score: number;
	/** 练习模式每轮 check 失败扣分（History 展示用）。 */
	fix_round_penalty: number;
	/** Fix 阶段才非空。 */
	source_code_dir: string | null;
	instances: RunInstanceDto[];
	/** 练习 data plane Flag Server endpoint（仅 GameBox 内部网络可达）。 */
	judge_endpoint: {
		base_url: string;
		flag_url: string;
		scope: "gamebox_internal";
	} | null;
};
```

```ts
export type AwdpRunEvaluationDto = {
	id: string;
	instance_id: string;
	gamebox_id: string;
	fix_round_id: string | null;
	round_sequence: number | null;
	kind: "manual" | "official";
	status: string;
	healthcheck_result: string | null;
	judge_result: string | null;
	/** exploit 结果详情（练习 manual / official 终态才有）。 */
	exploit_result: string | null;
	finished_at: string | null;
};
```

```ts
export type ScoreEventDto = {
	id: string;
	score_type: "break" | "fix";
	gamebox_id: string;
	fix_round_id: string | null;
	delta: number;
	created_at: string;
};
```

```ts
export type AwdpRunScoresDto = {
	total: number;
	history: ScoreEventDto[];
};
```

```ts
export type AwdpRunWriteupDto = {
	run_id: string;
	content: string;
	updated_at: string | null;
};
```

```ts
export type AwdpRunApi = ReturnType<typeof createAwdpRunApi>;
```

## `packages/sdk/src/api/index.ts`

```ts
export type AdminApi = ReturnType<typeof createAdminApi>;
```

```ts
export type ServiceApi = ReturnType<typeof createServiceApi>;
```

## `packages/sdk/src/api/service/announcements.ts`

```ts
export type AnnouncementServiceApi = ReturnType<typeof createAnnouncementServiceApi>;
```

## `packages/sdk/src/api/service/challenges.ts`

```ts
export type UnifiedWriteupResult = {
	id: string;
	writeup_type: "challenge" | "gamebox";
	nickname: string;
	avatar?: string | null;
	email: string;
	content_id: string;
	content_name: string;
	updated_at: string;
};
```

```ts
export type UnifiedWriteupDetail = {
	id: string;
	writeup_type: "challenge" | "gamebox";
	content_id: string;
	content_name: string;
	category?: string | null;
	nickname: string;
	avatar?: string | null;
	email: string;
	content: string;
	created_at: string;
	updated_at: string;
};
```

```ts
export type ChallengeServiceApi = ReturnType<typeof createChallengeServiceApi>;
```

## `packages/sdk/src/api/service/discussions.ts`

```ts
export type DiscussionWithAuthor = Discussions & {
	author_nickname: string;
	author_avatar?: string;
	is_liked: boolean;
};
```

```ts
export type DiscussionServiceApi = ReturnType<typeof createDiscussionServiceApi>;
```

## `packages/sdk/src/api/service/events.ts`

```ts
export type EventServiceApi = ReturnType<typeof createEventServiceApi>;
```

## `packages/sdk/src/api/service/instances.ts`

```ts
export type InstanceServiceApi = ReturnType<typeof createInstanceServiceApi>;
```

## `packages/sdk/src/api/service/solves.ts`

```ts
export type SolveResult = ChallengeSolves & {
	nickname: string;
	avatar?: string;
	challenge_name: string;
};
```

```ts
export type SolveServiceApi = ReturnType<typeof createSolveServiceApi>;
```

## `packages/sdk/src/api/service/submit.ts`

```ts
export type SubmitServiceApi = ReturnType<typeof createSubmitServiceApi>;
```

## `packages/sdk/src/api/service/uploads.ts`

```ts
export type UploadsServiceApi = ReturnType<typeof createUploadsServiceApi>;
```

## `packages/sdk/src/api/service/users.ts`

```ts
export type UserServiceApi = ReturnType<typeof createUserServiceApi>;
```

## `packages/sdk/src/api/service/weapons.ts`

```ts
export type WeaponsServiceApi = ReturnType<typeof createWeaponsServiceApi>;
```

## `packages/react/src/index.ts`

```ts
export interface CreateFloatCTFReactOptions {
	/** 已绑定的 FloatCTF 客户端（来自 `@floatctf/sdk` 的 `createFloatCTFClient()`）。 */
	client: FloatCTFClient;
	/** 前端自有的"读当前选手 token"hook。 */
	useUserToken: UseTokenSource;
	/** 前端自有的"读当前管理员 token"hook；省略时复用 `useUserToken`。 */
	useAdminToken?: UseTokenSource;
}
```

```ts
export type FloatCTFReactBindings = ReturnType<typeof createFloatCTFReact>;
```

## `packages/react/src/useAdminAwdEventStream.ts`

```ts
export type UseAdminAwdEventStreamOptions = {
	eventId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};
```

## `packages/react/src/useAwdEventStream.ts`

```ts
export type AwdStreamEvent = {
	type: string;
	sequence?: number;
	payload?: unknown;
	occurred_at?: string;
};
```

```ts
export type UseAwdEventStreamOptions = {
	eventId: string;
	/** 流不可用时的 REST 快照间隔（毫秒）。默认 15000。 */
	pollMs?: number;
	/** 为 true 时尝试 SSE。默认 true。 */
	preferStream?: boolean;
	enabled?: boolean;
};
```

## `packages/react/src/useAwdpEventStream.ts`

```ts
export type AwdpStreamEvent = {
	type: string;
	sequence?: number;
	payload?: unknown;
	occurred_at?: string;
};
```

```ts
export type UseAwdpEventStreamOptions = {
	eventId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};
```

## `packages/react/src/useAwdpRunStream.ts`

```ts
export type UseAwdpRunStreamOptions = {
	runId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};
```

## `packages/frontend-runtime/src/bootstrap.ts`

```ts
export interface FloatCTFBootstrapInfo {
	active_frontend: string;
	platform_version: string;
	api_contract_version: string;
	frontend_runtime_version: string;
	capabilities: string[];
}
```

```ts
export interface BootstrapFrontendOptions {
	/** 前端渲染宿主元素（会被清空）。 */
	root: HTMLElement;
	/** API base URL，默认 `/api`。 */
	apiBaseUrl?: string;
	/** 注册表 URL，默认 `/__floatctf/frontends/registry.json`。 */
	registryUrl?: string;
	/** 前端资产前缀，默认 `/__floatctf/frontends`。 */
	frontendBaseUrl?: string;
	/** 兜底 UI 宿主，默认 `document.body`。 */
	emergencyHost?: HTMLElement;
	/**
	 * 浏览器本地紧急覆盖（`?frontend=<id>`）。
	 * 只接受注册表里已安装的安全 ID；其他值一律忽略并记入诊断。
	 */
	overrideFrontendId?: string | null;
	/** 回退用的前端 ID，默认 `default`。 */
	fallbackFrontendId?: string;
	/** 注入依赖（测试用）。 */
	fetchImpl?: typeof fetch;
	/** 动态 import 实现（测试用）。 */
	importModule?: (url: string) => Promise<unknown>;
	/** 样式注入实现（测试用）；返回该 link 元素便于回退时移除。 */
	loadStyle?: (url: string, doc: Document) => HTMLElement;
	/** 诊断回调（测试/日志用）。 */
	onDiagnostics?: (diagnostics: BootstrapDiagnostics) => void;
}
```

```ts
export interface BootstrapFrontendResult {
	mounted: boolean;
	frontendId?: string;
	frontendVersion?: string;
	diagnostics: BootstrapDiagnostics;
}
```

## `packages/frontend-runtime/src/emergency.ts`

```ts
export interface BootstrapDiagnostics {
	/** 平台版本（来自 `/api/frontend`，可能取不到）。 */
	platformVersion?: string;
	apiContractVersion?: string;
	frontendRuntimeVersion?: string;
	/** 平台设置里的 FRONTEND_ACTIVE。 */
	activeFrontendId?: string | null;
	/** `?frontend=` 覆盖值（若有效）。 */
	overrideFrontendId?: string | null;
	/** 本次实际尝试加载过的前端 ID（按顺序）。 */
	attempted: string[];
	/** 每次尝试的失败原因（与 `attempted` 对位）。 */
	errors: string[];
}
```

## `packages/frontend-runtime/src/manifest.ts`

```ts
export interface FloatCTFFrontendCompatibility {
	/** 期望的前端运行时契约 major（如 `"1"`）。 */
	frontendRuntime: string;
	/** 期望的对外 HTTP API 契约 major（如 `"1"`）。 */
	apiContract: string;
	/** 可选：构建时使用的 `@floatctf/sdk` 版本（**信息性**，运行时不强制）。 */
	sdk?: string;
}
```

```ts
export interface FloatCTFFrontendManifest {
	schemaVersion: number;
	id: string;
	name: string;
	version: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	/** 相对本制品根目录的 ESM 入口（必须导出 `mount`）。 */
	entry: string;
	/** 相对本制品根目录的样式文件（bootstrap 负责注入 `<link>`）。 */
	styles?: string[];
}
```

```ts
export type ManifestParseResult =
	| { ok: true; manifest: FloatCTFFrontendManifest; warnings: string[] }
	| { ok: false; errors: string[] };
```

## `packages/frontend-runtime/src/module.ts`

```ts
export interface FloatCTFMountContext {
	/** 前端渲染的宿主元素（bootstrap 已确保为空）。 */
	root: HTMLElement;
	/** API base URL，例如 `/api`（同源）。前端自行决定如何调用。 */
	apiBaseUrl: string;
	/** 本前端制品根目录的同源绝对路径（用于加载自己的图片/字体等）。 */
	assetBaseUrl: string;
	/** 当前前端 ID 与版本（诊断/日志用）。 */
	frontendId: string;
	frontendVersion: string;
	/** 平台版本（`/api/frontend` 返回，纯展示/诊断用）。 */
	platformVersion: string;
	/** 对外 HTTP API 契约 major。 */
	apiContractVersion: string;
	/** 前端运行时契约 major。 */
	frontendRuntimeVersion: string;
	/** 真实且稳定的平台能力标记（列表内元素由平台声明，前端按需探测）。 */
	capabilities: readonly string[];
}
```

```ts
export type FloatCTFFrontendUnmount = () => void;
```

```ts
export interface FloatCTFFrontendModule {
	/** 可选：制品内自述的 ID/版本（仅用于诊断日志，平台以注册表为准）。 */
	manifest?: { id?: string; version?: string };
	/**
	 * 把前端挂载到 `context.root`。允许 async；抛错会触发 bootstrap 回退。
	 *
	 * 返回类型里**刻意保留 `void`**：绝大多数前端只是把界面挂上去、不返回任何东西，
	 * `export function mount(context) { … }` 这种最自然的写法必须能直接满足契约；
	 * 若改写成 `FloatCTFFrontendUnmount | undefined`，所有"不返回清理函数"的实现
	 * 都会被 TS 判为不可赋值（`() => void` 不能赋给 `() => undefined`）。
	 * 这里表达的语义就是"可以没有返回值"，biome 的 `noConfusingVoidType` 属误报。
	 */
	// biome-ignore lint/suspicious/noConfusingVoidType: void 是刻意的（见上方说明）
	mount(context: FloatCTFMountContext): void | FloatCTFFrontendUnmount | Promise<FloatCTFFrontendUnmount | undefined>;
}
```

```ts
export type FloatCTFFrontendImport = Partial<FloatCTFFrontendModule> & {
	default?: Partial<FloatCTFFrontendModule> | FloatCTFFrontendModule;
};
```

## `packages/frontend-runtime/src/paths.ts`

```ts
export interface PathCheckResult {
	ok: boolean;
	reason?: string;
}
```

## `packages/frontend-runtime/src/registry.ts`

```ts
export interface FloatCTFRegistryVersion {
	version: string;
	name: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	entry: string;
	styles: string[];
	installedAt: string;
}
```

```ts
export interface FloatCTFRegistryFrontend {
	id: string;
	/** 显式当前版本指针；必须存在于 `versions` 中。 */
	currentVersion: string;
	/** `default` 前端为 true：常规前端管理不得移除。 */
	protected: boolean;
	versions: Record<string, FloatCTFRegistryVersion>;
}
```

```ts
export interface FloatCTFRegistry {
	schemaVersion: number;
	updatedAt: string;
	frontends: Record<string, FloatCTFRegistryFrontend>;
}
```

```ts
export type RegistryParseResult =
	| { ok: true; registry: FloatCTFRegistry }
	| { ok: false; errors: string[] };
```

```ts
export interface ResolvedFrontend {
	id: string;
	version: string;
	name: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	/** 入口 ESM 的**同源绝对路径**（已用注册表校验过的相对路径拼成）。 */
	entryUrl: string;
	/** 样式表同源绝对路径。 */
	styleUrls: string[];
	/** 制品根目录的同源绝对路径（前端可用于加载自身资产）。 */
	assetBaseUrl: string;
}
```

```ts
export type ResolveFrontendResult =
	| { ok: true; frontend: ResolvedFrontend }
	| { ok: false; errors: string[] };
```

```ts
export interface ResolveFrontendOptions {
	/** 平台设置里的 `FRONTEND_ACTIVE`（前端 ID）。 */
	requestedId: string | null | undefined;
	/** 前端资产挂载前缀，默认 `/__floatctf/frontends`。 */
	frontendBaseUrl: string;
	frontendRuntimeVersion?: string;
	apiContractVersion?: string;
}
```

