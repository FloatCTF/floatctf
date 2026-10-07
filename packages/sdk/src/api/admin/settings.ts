import type { Settings } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

/**
 * 管理端 settings 接口的 API DTO。
 * 在生成实体上扩展非列的计算字段。
 */
export type SettingsDto = Settings & {
	resolved_value: string;
};

export function createSettingAdminApi(http: FloatCTFHttpClient) {
	return {
		fetch: async (): Promise<UniResponse<SettingsDto[]>> => {
			const res = await http.get("/settings");
			return res.data;
		},
		create: async (
			setting: Partial<SettingsDto>,
		): Promise<UniResponse<SettingsDto>> => {
			const res = await http.post("/settings", setting);
			return res.data;
		},
		remove: async (id_list: string[]): Promise<UniResponse<number>> => {
			const res = await http.delete("/settings", { data: { id_list } });
			return res.data;
		},
		patch: async (
			setting: Partial<SettingsDto>,
		): Promise<UniResponse<SettingsDto>> => {
			const res = await http.patch(`/settings/${setting.id}`, setting);
			return res.data;
		},
};
}

export type SettingAdminApi = ReturnType<typeof createSettingAdminApi>;
