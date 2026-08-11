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
 * `static_flag_value` 仅 admin 列表/详情接口返回（secret）。
 * container_port 用 Omit 覆盖：后端序列化为 JSON number（entity 生成器误标为 string）。
 */
export type ChallengesListItem = Omit<Challenges, "container_port"> & {
	version?: string;
	/** 包 manifest 作者（spec_json.author）。 */
	author?: string;
	build_status?: string;
	image_ref?: string;
	attachment?: ChallengeAttachmentDto;
	container_port?: number | null;
	recommended_cpu_millis?: number;
	recommended_memory_bytes?: number;
	recommended_pids_limit?: number;
	static_flag_value?: string | null;
};
