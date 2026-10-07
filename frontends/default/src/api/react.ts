/**
 * Default Frontend 的 React 绑定接线（pages 通过 `@/api/react` 使用）。
 *
 * `@floatctf/react` 是**可选**的 headless 包：它不知道 token 存在哪里，也不知道
 * 用哪个状态库。这里把本前端的 Zustand 选择器注入进去，得到与迁移前
 * `@/hooks/*`、`@/api/queries` **同名同签名**的 hook 与 query 工厂，
 * 因此页面代码零改动。
 *
 * 非 React 前端完全不使用本文件——它们直接用 `@floatctf/sdk`。
 */

import { createFloatCTFReact } from "@floatctf/react";

import { useAuthStore } from "@/stores/AuthStore";
import { client } from "./client";

const bindings = createFloatCTFReact({
	client,
	useUserToken: () => useAuthStore((s) => s.token),
	useAdminToken: () => useAuthStore((s) => s.adminToken),
});

export const {
	// 实时事件流
	useAwdEventStream,
	useAdminAwdEventStream,
	useAwdpEventStream,
	useAwdpRunStream,
	// query options 工厂
	eventInfoQueryOptions,
	challengeQueryOptions,
	challengeInstanceQueryOptions,
	systemInformationQueryOptions,
} = bindings;
