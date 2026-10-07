/**
 * `@/api` —— Default Frontend 的领域 API 门面。
 *
 * 客户端实现（传输、错误归一化、领域方法）全部来自 `@floatctf/sdk`；
 * 这个文件只是"把**本前端自己的**客户端 + 本前端的接线"暴露成页面直接可用的形状，
 * 因此页面的 `import { serviceApi, adminApi } from "@/api"` 语义与迁移前一致。
 *
 * **实例归属**：这里导出的每个值都绑定到 `./client` 里那**一个** SDK 客户端实例
 * （`@floatctf/sdk` 自身没有任何模块级绑定），因此页面拿到的是本前端的传输，
 * 不可能"串"到别的客户端上。
 */

import { client } from "./client";

/** 选手端 API（与迁移前的 `serviceApi` 形状一致）。 */
export const serviceApi = client.service;

/** 管理端 API（与迁移前的 `adminApi` 形状一致）。 */
export const adminApi = client.admin;

/**
 * 各领域的**实例绑定**具名导出。
 *
 * 页面历史上直接从 `@floatctf/sdk` 具名导入这些对象；SDK 现在不再提供
 * 全局单例（客户端必须显式创建），因此它们统一由本前端自己的实例提供。
 */
export const awdPlayerApi = client.awd.player;
export const awdAdminApi = client.awd.admin;
export const awdpPlayerApi = client.awdp.player;
export const awdpAdminApi = client.awdp.admin;
export const awdpRunApi = client.awdp.runs;

/** 单个领域模块（页面按需具名导入，全部绑定本实例）。 */
export const instanceAdminApi = client.admin.instances;
export const eventAdminApi = client.admin.events;
export const settingAdminApi = client.admin.settings;
export const challengeAdminApi = client.admin.challenges;
export const userAdminApi = client.admin.users;
export const eventChallengeAdminApi = client.admin.event_challenges;
export const eventUserAdminApi = client.admin.event_users;
export const eventAnnouncementAdminApi = client.admin.event_announcements;
export const eventWriteupAdminApi = client.admin.event_writeups;
export const eventTeamAdminApi = client.admin.event_teams;
export const eventLogAdminApi = client.admin.event_logs;
export const dashboardAdminApi = client.admin.dashboard;
export const dockerAdminApi = client.admin.docker;
export const logsAdminApi = client.admin.logs;
export const downloadAdminApi = client.admin.download;
export const databaseAdminApi = client.admin.database;
export const scheduledTaskAdminApi = client.admin.scheduled_tasks;
export const weaponsAdminApi = client.admin.weapons;
export const superAdminApi = client.admin.super_admin;
export const systemAdminApi = client.admin.system;
export const discussionAdminApi = client.admin.discussions;
export const announcementAdminApi = client.admin.announcements;
export const adminLoginFn = client.admin.login;
export const userServiceApi = client.service.users;
export const eventServiceApi = client.service.events;
export const challengeServiceApi = client.service.challenges;
export const instanceServiceApi = client.service.instances;
export const submitServiceApi = client.service.submit;
export const solveServiceApi = client.service.solves;
export const weaponsServiceApi = client.service.weapons;
export const announcementServiceApi = client.service.announcements;
export const uploadsServiceApi = client.service.uploads;
export const discussionServiceApi = client.service.discussions;

export { client };
export type { FloatCTFClient } from "@floatctf/sdk";

/** Docker/DTO 类型（迁移前由 `@/api` 再导出，保持调用方不变）。 */
export type {
	ContainerInfo,
	FloatDockerContainer,
	ImageInfo,
	NetworkInfo,
	PortInfo,
} from "@floatctf/sdk";
