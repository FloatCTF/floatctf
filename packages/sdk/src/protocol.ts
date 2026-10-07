/**
 * FloatCTF 平台响应封装（`UniResponse`）——**以后端真实行为为准**。
 *
 * 后端所有 handler 都返回 `UniResponse<T>`（Rust `UniResult<T>`），
 * 序列化为 `{ code, message, data, meta }`：
 * - `code`：平台业务码（0 = 成功；错误响应用 `AppError` 映射）
 * - `message`：平台文案（可直接展示，也可能是英文技术文案）
 * - `data`：业务数据；无数据时为 `null` 或缺失
 * - `meta`：列表接口的分页元信息
 *
 * 这里**不做包装/解包转换**：SDK 原样透传后端 envelope，避免发明第二套协议。
 */

/** 列表/分页查询参数。 */
export type QueryParams = {
	offset?: number;
	limit?: number;
	page?: number;
	total?: number;
	filter?: string;
};

/** 平台统一响应封装。 */
export type UniResponse<T> = {
	code: number;
	message: string;
	data?: T;
	meta?: QueryParams;
};

/** 平台成功业务码。 */
export const UNI_SUCCESS_CODE = 0;
