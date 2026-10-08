/**
 * 查询键工厂 —— **所有 `useQuery` 的 key 都从这里取**，不要写裸数组。
 *
 * 两条硬约束：
 * 1. AWD 的键必须与 `@floatctf/react` 的 `AWD_PLAYER_QUERY_KEYS` / `AWD_ADMIN_QUERY_KEYS`
 *    常量对齐（两段式 `[key, eventId]`），否则 SSE 事件到达时 `invalidateAwdQueries`
 *    失效不到本前端的页面 —— 面板会整场比赛不更新而看起来「正常」。
 * 2. 参数必须进 key（分页 / 过滤），否则切页拿不到新数据。
 */

import type { QueryParams } from "@floatctf/sdk";

type Filters = QueryParams & Record<string, unknown>;

export const qk = {
	auth: {
		me: () => ["me"] as const,
	},

	events: {
		all: ["events"] as const,
		list: (params: Filters = {}) => ["events", "list", params] as const,
		/** 赛事详情：与 `AWD_*_QUERY_KEYS` 的 "event" 对齐（SSE 失效覆盖得到）。 */
		detail: (eventId: string) => ["event", eventId] as const,
		challenges: (eventId: string, params: Filters = {}) =>
			["event", eventId, "challenges", params] as const,
		challengeInstance: (eventId: string, challengeId: string) =>
			["event", eventId, "challenges", challengeId, "instance"] as const,
		scoreboard: (eventId: string) => ["event", eventId, "scoreboard"] as const,
		trend: (eventId: string) => ["event", eventId, "trend"] as const,
		instances: (eventId: string) => ["event", eventId, "instances"] as const,
		announcements: (eventId: string) => ["announcements", eventId] as const,
		ownWriteup: (eventId: string) => ["event", eventId, "own-writeup"] as const,
	},

	challenges: {
		all: ["challenges"] as const,
		list: (params: Filters = {}) => ["challenges", "list", params] as const,
		detail: (challengeId: string) => ["challenge", challengeId] as const,
		instance: (challengeId: string) => ["challenge", challengeId, "instance"] as const,
		myWriteup: (challengeId: string) => ["challenge", challengeId, "my-writeup"] as const,
		writeups: (challengeId: string, params: Filters = {}) =>
			["challenge", challengeId, "writeups", params] as const,
	},

	writeups: {
		all: ["writeups"] as const,
		list: (params: Filters = {}) => ["writeups", "list", params] as const,
		detail: (writeupId: string) => ["writeups", "detail", writeupId] as const,
	},

	sets: {
		all: ["challenge-sets"] as const,
		list: (params: Filters = {}) => ["challenge-sets", "list", params] as const,
		detail: (setId: string) => ["challenge-sets", setId] as const,
	},

	instances: {
		all: ["instances"] as const,
		list: (params: Filters = {}) => ["instances", "list", params] as const,
	},

	solves: {
		all: ["solves"] as const,
		list: (params: Filters = {}) => ["solves", "list", params] as const,
		top15: () => ["solves", "top15"] as const,
	},

	announcements: {
		all: ["announcements"] as const,
		list: (params: Filters = {}) => ["announcements", "list", params] as const,
	},

	discussions: {
		all: ["discussions"] as const,
		list: (params: Filters = {}) => ["discussions", "list", params] as const,
		detail: (discussionId: string) => ["discussions", "detail", discussionId] as const,
		comments: (discussionId: string) => ["discussions", discussionId, "comments"] as const,
		mine: (params: Filters = {}) => ["discussions", "mine", params] as const,
	},

	weapons: {
		all: ["weapons"] as const,
		list: (params: Filters = {}) => ["weapons", "list", params] as const,
	},

	/** AWD 选手端 —— 键名与 `AWD_PLAYER_QUERY_KEYS` 一致。 */
	awd: {
		status: (eventId: string) => ["awd-player-status", eventId] as const,
		gameboxes: (eventId: string) => ["awd-gameboxes", eventId] as const,
		scores: (eventId: string) => ["awd-scores", eventId] as const,
		wireguard: (eventId: string) => ["awd-wg", eventId] as const,
		ssh: (eventId: string) => ["awd-ssh", eventId] as const,
		/** 管理端 —— 键名与 `AWD_ADMIN_QUERY_KEYS` 一致。 */
		adminStatus: (eventId: string) => ["admin-awd-status", eventId] as const,
		adminScores: (eventId: string) => ["admin-awd-scores", eventId] as const,
		prechecks: (eventId: string) => ["admin-awd-prechecks", eventId] as const,
		eventGameboxes: (eventId: string) => ["admin-awd-event-gameboxes", eventId] as const,
		eventNetwork: (eventId: string) => ["admin-awd-event-network", eventId] as const,
		library: (params: Filters = {}) => ["admin-awd-library", params] as const,
		platformNetwork: () => ["admin-awd-platform-network"] as const,
		platformHealth: () => ["admin-awd-platform-health"] as const,
		platformAllocations: () => ["admin-awd-platform-allocations"] as const,
	},

	awdp: {
		overview: (eventId: string) => ["awdp-overview", eventId] as const,
		instance: (eventId: string) => ["awdp-instance", eventId] as const,
		rounds: (eventId: string) => ["awdp-rounds", eventId] as const,
		/** 键名与 `@floatctf/react` 的 `useAwdpEventStream` 失效常量一致（`awdp-evals`）。 */
		evaluations: (eventId: string) => ["awdp-evals", eventId] as const,
		scores: (eventId: string) => ["awdp-scores", eventId] as const,
		scoreboard: (eventId: string) => ["awdp-scoreboard", eventId] as const,
		trend: (eventId: string) => ["awdp-trend", eventId] as const,
		adminConfig: (eventId: string) => ["admin-awdp-config", eventId] as const,
		adminGameboxes: (eventId: string) => ["admin-awdp-gameboxes", eventId] as const,
		adminInstances: (eventId: string) => ["admin-awdp-instances", eventId] as const,
		adminScores: (eventId: string) => ["admin-awdp-scores", eventId] as const,
		adminData: (eventId: string) => ["admin-awdp-data", eventId] as const,
	},

	runs: {
		all: ["awdp-runs"] as const,
		/** 键名与 `useAwdpRunStream` 的失效常量一致（`gamebox-catalog`）。 */
		catalog: (params: Filters = {}) => ["gamebox-catalog", params] as const,
		run: (runId: string) => ["awdp-run", runId] as const,
		instance: (runId: string) => ["awdp-run-instance", runId] as const,
		rounds: (runId: string) => ["awdp-run-rounds", runId] as const,
		/** 键名与 `useAwdpRunStream` 的失效常量一致（`awdp-run-evals`）。 */
		evaluations: (runId: string) => ["awdp-run-evals", runId] as const,
		scores: (runId: string) => ["awdp-run-scores", runId] as const,
		writeup: (runId: string) => ["awdp-run-writeup", runId] as const,
	},

	admin: {
		dashboard: () => ["admin-dashboard"] as const,
		system: () => ["admin-system"] as const,
		version: () => ["admin-version"] as const,
		users: (params: Filters = {}) => ["admin-users", params] as const,
		challenges: (params: Filters = {}) => ["admin-challenges", params] as const,
		challengeSets: (params: Filters = {}) => ["admin-challenge-sets", params] as const,
		challengeSet: (setId: string) => ["admin-challenge-set", setId] as const,
		events: (params: Filters = {}) => ["admin-events", params] as const,
		event: (eventId: string) => ["admin-event", eventId] as const,
		eventChallenges: (eventId: string, params: Filters = {}) =>
			["admin-event-challenges", eventId, params] as const,
		eventUsers: (eventId: string, params: Filters = {}) =>
			["admin-event-users", eventId, params] as const,
		eventTeams: (eventId: string) => ["admin-event-teams", eventId] as const,
		eventAnnouncements: (eventId: string) => ["admin-event-announcements", eventId] as const,
		eventLogs: (eventId: string, params: Filters = {}) =>
			["admin-event-logs", eventId, params] as const,
		eventWriteups: (eventId: string, params: Filters = {}) =>
			["admin-event-writeups", eventId, params] as const,
		eventInstances: (eventId: string) => ["admin-event-instances", eventId] as const,
		eventData: (eventId: string) => ["admin-event-data", eventId] as const,
		settings: (params: Filters = {}) => ["admin-settings", params] as const,
		logs: (params: Filters = {}) => ["admin-logs", params] as const,
		announcements: (params: Filters = {}) => ["admin-announcements", params] as const,
		discussions: (params: Filters = {}) => ["admin-discussions", params] as const,
		discussion: (discussionId: string) => ["admin-discussion", discussionId] as const,
		discussionComments: (discussionId: string) =>
			["admin-discussion-comments", discussionId] as const,
		weapons: (params: Filters = {}) => ["admin-weapons", params] as const,
		superAdmins: (params: Filters = {}) => ["admin-super-admins", params] as const,
		scheduledTasks: (params: Filters = {}) => ["admin-scheduled-tasks", params] as const,
		dockerContainers: () => ["admin-docker-containers"] as const,
		dockerImages: () => ["admin-docker-images"] as const,
		dockerNetworks: () => ["admin-docker-networks"] as const,
	},
} as const;
