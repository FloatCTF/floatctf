/**
 * 选手端领域模块的聚合出口（**全是工厂**）。
 *
 * 模块自身不再持有任何模块级 HTTP 绑定：每个 `create*` 都要求注入
 * `FloatCTFHttpClient`，因此不同客户端实例天然隔离。
 */

export {
	createUserServiceApi,
	type UserServiceApi,
} from "./users.js";
export {
	createEventServiceApi,
	type EventServiceApi,
} from "./events.js";
export {
	createChallengeServiceApi,
	type ChallengeServiceApi,
	type UnifiedWriteupDetail,
	type UnifiedWriteupResult,
} from "./challenges.js";
export {
	createInstanceServiceApi,
	type InstanceServiceApi,
} from "./instances.js";
export {
	createSubmitServiceApi,
	type SubmitServiceApi,
} from "./submit.js";
export {
	createSolveServiceApi,
	type SolveServiceApi,
	type SolveResult,
} from "./solves.js";
export {
	createWeaponsServiceApi,
	type WeaponsServiceApi,
} from "./weapons.js";
export {
	createAnnouncementServiceApi,
	type AnnouncementServiceApi,
} from "./announcements.js";
export {
	createUploadsServiceApi,
	type UploadsServiceApi,
} from "./uploads.js";
export {
	createDiscussionServiceApi,
	type DiscussionServiceApi,
	type DiscussionWithAuthor,
} from "./discussions.js";
