/**
 * `@/api` —— Default Frontend 的领域 API 门面。
 *
 * 客户端实现（传输、错误归一化、领域方法）全部来自 `@floatctf/sdk`；
 * 这个文件只是"把 SDK 门面 + 本前端的接线"合并成页面直接可用的形状，
 * 因此页面的 `import { serviceApi, adminApi } from "@/api"` 语义与迁移前一致。
 */

import {
	adminApi as adminApiFacade,
	serviceApi as serviceApiFacade,
} from "@floatctf/sdk";

import { client } from "./client";

/** 选手端 API（与迁移前的 `serviceApi` 形状一致）。 */
export const serviceApi = serviceApiFacade;

/** 管理端 API（与迁移前的 `adminApi` 形状一致）。 */
export const adminApi = adminApiFacade;

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
