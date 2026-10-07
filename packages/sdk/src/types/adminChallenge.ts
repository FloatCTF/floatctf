import type { ChallengesListItem } from "./challengeDto.js";

/** 题目完整性检查结果。 */
export type ChallengeCheckResult = {
	id: string;
	challenge_name: string;
	is_ok: boolean;
	docker_image: boolean;
	attachment: boolean;
	/** static / attachment-only 题目（无 src/Dockerfile）：没有镜像，无需 Build。 */
	static_content: boolean;
};

/** 题目构建结果。 */
export type BuildChallengeResult = {
	challenge_name: string;
	is_ok: boolean;
	message: string;
};

/** 题目目录扫描结果条目。 */
export type ChallengeScanItem = {
	safe_name: string;
	name: string | null;
	version: string | null;
	status: "added" | "skipped" | "error";
	message: string;
};

/** 题目导入响应。 */
export type ImportChallengeResponse = {
	challenge: ChallengesListItem;
};
