/**
 * 管理端领域模块的聚合出口（**全是工厂**）。
 *
 * 模块自身不再持有任何模块级 HTTP 绑定：每个 `create*` 都要求注入
 * `FloatCTFHttpClient`，因此不同客户端实例天然隔离。
 */

export { createAdminLoginFn, type AdminLoginFn } from "./auth.js";
export {
	createSystemAdminApi,
	type SystemAdminApi,
} from "./system.js";
export {
	createSettingAdminApi,
	type SettingAdminApi,
	type SettingsDto,
} from "./settings.js";
export {
	createAnnouncementAdminApi,
	type AnnouncementAdminApi,
} from "./announcements.js";
export {
	createChallengeAdminApi,
	type ChallengeAdminApi,
} from "./challenges.js";
export { createUserAdminApi, type UserAdminApi } from "./users.js";
export { createEventAdminApi, type EventAdminApi } from "./events.js";
export {
	createInstanceAdminApi,
	type InstanceAdminApi,
	type AdminInstanceRow,
} from "./instances.js";
export {
	createEventChallengeAdminApi,
	type EventChallengeAdminApi,
} from "./event_challenges.js";
export {
	createEventUserAdminApi,
	type EventUserAdminApi,
} from "./event_users.js";
export {
	createEventAnnouncementAdminApi,
	type EventAnnouncementAdminApi,
} from "./event_announcements.js";
export {
	createEventWriteupAdminApi,
	type EventWriteupAdminApi,
} from "./event_writeups.js";
export {
	createEventTeamAdminApi,
	type EventTeamAdminApi,
} from "./event_teams.js";
export {
	createEventLogAdminApi,
	type EventLogAdminApi,
} from "./event_logs.js";
export {
	createDatabaseAdminApi,
	type DatabaseAdminApi,
} from "./database.js";
export {
	createScheduledTaskAdminApi,
	type ScheduledTaskAdminApi,
} from "./scheduled_tasks.js";
export {
	createWeaponsAdminApi,
	type WeaponsAdminApi,
} from "./weapons.js";
export { createLogsAdminApi, type LogsAdminApi } from "./logs.js";
export {
	createDownloadAdminApi,
	type DownloadAdminApi,
} from "./download.js";
export {
	createDiscussionAdminApi,
	type DiscussionAdminApi,
} from "./discussions.js";
export {
	createDashboardAdminApi,
	type DashboardAdminApi,
	type DashboardSummary,
} from "./dashboard.js";
export {
	createSuperAdminApi,
	type SuperAdminApi,
} from "./super_admin.js";
export {
	createDockerAdminApi,
	type DockerAdminApi,
	type FloatDockerContainer,
	type ContainerInfo,
	type PortInfo,
	type ImageInfo,
	type NetworkInfo,
} from "./docker.js";
