/**
 * 领域 API 门面工厂：把各模块客户端按使用方（选手端 / 管理端）聚合成对象。
 *
 * **每个门面绑定到传入的 HTTP handle**，因此不同 `createFloatCTFClient()` 实例
 * 之间完全隔离（不存在模块级共享绑定）。领域模块本身也是工厂：
 * 它们只依赖注入进来的 `FloatCTFHttpClient`。
 */

import type { FloatCTFHttpClient } from "../transport.js";

export {
	createAdminLoginFn,
	createAnnouncementAdminApi,
	createChallengeAdminApi,
	createDashboardAdminApi,
	createDatabaseAdminApi,
	createDiscussionAdminApi,
	createDockerAdminApi,
	createDownloadAdminApi,
	createEventAdminApi,
	createEventAnnouncementAdminApi,
	createEventChallengeAdminApi,
	createEventLogAdminApi,
	createEventTeamAdminApi,
	createEventUserAdminApi,
	createEventWriteupAdminApi,
	createInstanceAdminApi,
	createLogsAdminApi,
	createScheduledTaskAdminApi,
	createSettingAdminApi,
	createSuperAdminApi,
	createSystemAdminApi,
	createUserAdminApi,
	createWeaponsAdminApi,
} from "./admin/index.js";
export {
	createAnnouncementServiceApi,
	createChallengeServiceApi,
	createDiscussionServiceApi,
	createEventServiceApi,
	createInstanceServiceApi,
	createSolveServiceApi,
	createSubmitServiceApi,
	createUploadsServiceApi,
	createUserServiceApi,
	createWeaponsServiceApi,
} from "./service/index.js";

export type {
	AdminInstanceRow,
	ContainerInfo,
	DashboardSummary,
	FloatDockerContainer,
	ImageInfo,
	NetworkInfo,
	PortInfo,
	SettingsDto,
} from "./admin/index.js";
export type {
	DiscussionWithAuthor,
	SolveResult,
	UnifiedWriteupDetail,
	UnifiedWriteupResult,
} from "./service/index.js";

import {
	createAdminLoginFn,
	createAnnouncementAdminApi,
	createChallengeAdminApi,
	createDashboardAdminApi,
	createDatabaseAdminApi,
	createDiscussionAdminApi,
	createDockerAdminApi,
	createDownloadAdminApi,
	createEventAdminApi,
	createEventAnnouncementAdminApi,
	createEventChallengeAdminApi,
	createEventLogAdminApi,
	createEventTeamAdminApi,
	createEventUserAdminApi,
	createEventWriteupAdminApi,
	createInstanceAdminApi,
	createLogsAdminApi,
	createScheduledTaskAdminApi,
	createSettingAdminApi,
	createSuperAdminApi,
	createSystemAdminApi,
	createUserAdminApi,
	createWeaponsAdminApi,
} from "./admin/index.js";
import { createAwdAdminApi, createAwdPlayerApi } from "./awd.js";
import { createAwdpAdminApi, createAwdpPlayerApi } from "./awdp.js";
import { createAwdpRunApi } from "./awdpRuns.js";
import {
	createAnnouncementServiceApi,
	createChallengeServiceApi,
	createDiscussionServiceApi,
	createEventServiceApi,
	createInstanceServiceApi,
	createSolveServiceApi,
	createSubmitServiceApi,
	createUploadsServiceApi,
	createUserServiceApi,
	createWeaponsServiceApi,
} from "./service/index.js";

/**
 * 管理端 API 门面。
 *
 * @param http 该客户端**自己的**管理端 HTTP handle（见 `createFloatCTFTransport`）。
 */
export function createAdminApi(http: FloatCTFHttpClient) {
	return {
		login: createAdminLoginFn(http),
		system: createSystemAdminApi(http),
		settings: createSettingAdminApi(http),
		announcements: createAnnouncementAdminApi(http),
		challenges: createChallengeAdminApi(http),
		discussions: createDiscussionAdminApi(http),
		users: createUserAdminApi(http),
		events: createEventAdminApi(http),
		instances: createInstanceAdminApi(http),
		event_challenges: createEventChallengeAdminApi(http),
		event_users: createEventUserAdminApi(http),
		event_announcements: createEventAnnouncementAdminApi(http),
		event_logs: createEventLogAdminApi(http),
		event_writeups: createEventWriteupAdminApi(http),
		event_teams: createEventTeamAdminApi(http),
		database: createDatabaseAdminApi(http),
		scheduled_tasks: createScheduledTaskAdminApi(http),
		weapons: createWeaponsAdminApi(http),
		logs: createLogsAdminApi(http),
		download: createDownloadAdminApi(http),
		docker: createDockerAdminApi(http),
		dashboard: createDashboardAdminApi(http),
		super_admin: createSuperAdminApi(http),
		awd: createAwdAdminApi(http),
	};
}

export type AdminApi = ReturnType<typeof createAdminApi>;

/**
 * 选手端 API 门面。
 *
 * @param http 该客户端**自己的**选手端 HTTP handle。
 */
export function createServiceApi(http: FloatCTFHttpClient) {
	return {
		users: createUserServiceApi(http),
		events: createEventServiceApi(http),
		challenges: createChallengeServiceApi(http),
		instances: createInstanceServiceApi(http),
		submit: createSubmitServiceApi(http),
		solves: createSolveServiceApi(http),
		weapons: createWeaponsServiceApi(http),
		announcements: createAnnouncementServiceApi(http),
		discussions: createDiscussionServiceApi(http),
		uploads: createUploadsServiceApi(http),
		awd: createAwdPlayerApi(http),
	};
}

export type ServiceApi = ReturnType<typeof createServiceApi>;
