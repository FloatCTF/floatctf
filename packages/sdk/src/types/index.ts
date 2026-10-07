/**
 * 前端可见的 API DTO / 视图无关类型。
 *
 * 这些类型**曾经散落在路由页面文件里**，被 `api/*` 反向 import（架构倒挂）。
 * 现在集中在这里：页面 → sdk 是唯一允许的方向。
 */

export type { ChallengeAttachmentDto, ChallengesListItem } from "./challengeDto.js";
export type { TopUser } from "./top.js";
export type { TrendItem, TrendPoint } from "./trend.js";
export type { ChallengeScoreboard, ScoreboardItem } from "./scoreboard.js";
export type { EventInstanceResult } from "./eventInstance.js";
export type { EventChallengeResult } from "./eventChallenge.js";
export type { Instances, InstancesDto } from "./instanceDto.js";
export type { ChallengeWriteupResult } from "./challengeWriteup.js";
export type {
	EventInfo,
	EventTeamMemberResult,
	EventTeamResult,
} from "./eventInfo.js";
export type { EventUserResult } from "./adminEventUser.js";
export type { TeamMemberResult, TeamResult } from "./adminEventTeam.js";
export type {
	DataEventChallenge,
	DataEventChallengeSolve,
	DataPresent,
} from "./dataPresent.js";
export type { EventChallenge, EventChallengeResult as AdminEventChallengeResult } from "./adminEventChallenge.js";
export type { SqlResult, SqlStatement } from "./database.js";
export type {
	DiskInformation,
	DockerImageInfo,
	DockerInformation,
	NetworkInterfaceInfo,
	SystemInformation,
} from "./systemInformation.js";
export type {
	BuildChallengeResult,
	ChallengeCheckResult,
	ChallengeScanItem,
	ImportChallengeResponse,
} from "./adminChallenge.js";
