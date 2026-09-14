import type { Settings } from "@/entity";
import { type UniResponse, admin_api } from "@/api/axios";

/**
 * 管理端 settings 接口的 API DTO。
 * 在生成实体上扩展非列的计算字段。
 */
export type SettingsDto = Settings & {
	resolved_value: string;
};

export const settingAdminApi = {
	fetch: async (): Promise<UniResponse<SettingsDto[]>> => {
		const res = await admin_api.get("/settings");
		return res.data;
	},
	create: async (
		setting: Partial<SettingsDto>,
	): Promise<UniResponse<SettingsDto>> => {
		const res = await admin_api.post("/settings", setting);
		return res.data;
	},
	remove: async (id_list: string[]): Promise<UniResponse<number>> => {
		const res = await admin_api.delete("/settings", { data: { id_list } });
		return res.data;
	},
	patch: async (
		setting: Partial<SettingsDto>,
	): Promise<UniResponse<SettingsDto>> => {
		const res = await admin_api.patch(`/settings/${setting.id}`, setting);
		return res.data;
	},
};
