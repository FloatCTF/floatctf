import type { Challenges } from "@/entity";

/** API 返回的附件元数据（来自 latest ready revision；不包含任何 flag 明文）。 */
export type ChallengeAttachmentDto = {
	name: string;
	path: string;
	size?: number;
};

/**
 * Challenge 列表/详情 DTO：身份字段 + latest ready revision 摘要。
 * 由后端 ChallengesDto 序列化而来（比生成的 entity 类型多这几个字段）。
 */
export type ChallengesListItem = Challenges & {
	latest_version?: string;
	latest_build_status?: string;
	latest_image_ref?: string;
	attachment?: ChallengeAttachmentDto;
};
