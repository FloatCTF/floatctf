import type { Challenges } from "@/entity";

/** API 返回的附件元数据（不包含任何 flag 明文）。 */
export type ChallengeAttachmentDto = {
	name: string;
	path: string;
	size?: number;
};

/**
 * Challenge 列表/详情 DTO：身份字段 + 当前版本 package 摘要（单版本模型）。
 * 由后端 ChallengesDto 序列化而来（比生成的 entity 类型多这几个字段）。
 */
export type ChallengesListItem = Challenges & {
	version?: string;
	build_status?: string;
	image_ref?: string;
	attachment?: ChallengeAttachmentDto;
};
