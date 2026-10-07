/**
 * 领域 API 门面：把各模块客户端按使用方（选手端 / 管理端）聚合成对象。
 *
 * 这些门面**不是工厂**：具体模块通过 `../transport` 里被绑定的 HTTP handle 发请求，
 * 因此 `createFloatCTFClient()` 绑定传输后，这里导出的门面即可直接使用。
 */

export { adminLoginFn } from "./admin/auth.js";
export { systemAdminApi } from "./admin/system.js";
export { settingAdminApi, type SettingsDto } from "./admin/settings.js";
export { announcementAdminApi } from "./admin/announcements.js";
export { challengeAdminApi } from "./admin/challenges.js";
export { userAdminApi } from "./admin/users.js";
export { eventAdminApi } from "./admin/events.js";
export { instanceAdminApi, type AdminInstanceRow } from "./admin/instances.js";
export { eventChallengeAdminApi } from "./admin/event_challenges.js";
export { eventUserAdminApi } from "./admin/event_users.js";
export { eventAnnouncementAdminApi } from "./admin/event_announcements.js";
export { eventWriteupAdminApi } from "./admin/event_writeups.js";
export { eventTeamAdminApi } from "./admin/event_teams.js";
export { eventLogAdminApi } from "./admin/event_logs.js";
export { databaseAdminApi } from "./admin/database.js";
export { scheduledTaskAdminApi } from "./admin/scheduled_tasks.js";
export { weaponsAdminApi } from "./admin/weapons.js";
export { logsAdminApi } from "./admin/logs.js";
export { downloadAdminApi } from "./admin/download.js";
export { discussionAdminApi } from "./admin/discussions.js";
export { dashboardAdminApi, type DashboardSummary } from "./admin/dashboard.js";
export { superAdminApi } from "./admin/super_admin.js";
export {
	dockerAdminApi,
	type FloatDockerContainer,
	type ContainerInfo,
	type PortInfo,
	type ImageInfo,
	type NetworkInfo,
} from "./admin/docker.js";

export { userServiceApi } from "./service/users.js";
export { eventServiceApi } from "./service/events.js";
export {
	challengeServiceApi,
	type UnifiedWriteupDetail,
	type UnifiedWriteupResult,
} from "./service/challenges.js";
export { instanceServiceApi } from "./service/instances.js";
export { submitServiceApi } from "./service/submit.js";
export { solveServiceApi, type SolveResult } from "./service/solves.js";
export { weaponsServiceApi } from "./service/weapons.js";
export { announcementServiceApi } from "./service/announcements.js";
export { uploadsServiceApi } from "./service/uploads.js";
export {
	discussionServiceApi,
	type DiscussionWithAuthor,
} from "./service/discussions.js";

import {
	adminLoginFn,
	announcementAdminApi,
	challengeAdminApi,
	dashboardAdminApi,
	databaseAdminApi,
	discussionAdminApi,
	dockerAdminApi,
	downloadAdminApi,
	eventAdminApi,
	eventAnnouncementAdminApi,
	eventChallengeAdminApi,
	eventLogAdminApi,
	eventTeamAdminApi,
	eventUserAdminApi,
	eventWriteupAdminApi,
	instanceAdminApi,
	logsAdminApi,
	scheduledTaskAdminApi,
	settingAdminApi,
	superAdminApi,
	systemAdminApi,
	userAdminApi,
	weaponsAdminApi,
} from "./admin/index.js";
import {
	announcementServiceApi,
	challengeServiceApi,
	discussionServiceApi,
	eventServiceApi,
	instanceServiceApi,
	solveServiceApi,
	submitServiceApi,
	uploadsServiceApi,
	userServiceApi,
	weaponsServiceApi,
} from "./service/index.js";
import { awdAdminApi, awdPlayerApi } from "./awd.js";

/** 管理端 API 门面（与原 `@/api` 的 `adminApi` 形状一致）。 */
export const adminApi = {
	login: adminLoginFn,
	system: systemAdminApi,
	settings: settingAdminApi,
	announcements: announcementAdminApi,
	challenges: challengeAdminApi,
	discussions: discussionAdminApi,
	users: userAdminApi,
	events: eventAdminApi,
	instances: instanceAdminApi,
	event_challenges: eventChallengeAdminApi,
	event_users: eventUserAdminApi,
	event_announcements: eventAnnouncementAdminApi,
	event_logs: eventLogAdminApi,
	event_writeups: eventWriteupAdminApi,
	event_teams: eventTeamAdminApi,
	database: databaseAdminApi,
	scheduled_tasks: scheduledTaskAdminApi,
	weapons: weaponsAdminApi,
	logs: logsAdminApi,
	download: downloadAdminApi,
	docker: dockerAdminApi,
	dashboard: dashboardAdminApi,
	super_admin: superAdminApi,
	awd: awdAdminApi,
};

/** 选手端 API 门面（与原 `@/api` 的 `serviceApi` 形状一致）。 */
export const serviceApi = {
	users: userServiceApi,
	events: eventServiceApi,
	challenges: challengeServiceApi,
	instances: instanceServiceApi,
	submit: submitServiceApi,
	solves: solveServiceApi,
	weapons: weaponsServiceApi,
	announcements: announcementServiceApi,
	discussions: discussionServiceApi,
	uploads: uploadsServiceApi,
	awd: awdPlayerApi,
};
