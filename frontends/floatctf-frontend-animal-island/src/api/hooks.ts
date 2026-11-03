/**
 * SDK 调用 → React Query 的薄封装。
 *
 * 存在的理由：SDK 的每个领域方法都返回 `UniResponse<T>`，而 `code !== 0`
 * 表示**平台业务失败**（HTTP 200）。如果每个页面各写一次解包，迟早有人漏掉一处，
 * 于是失败被当成成功渲染出来。这里统一在数据层解包并抛错。
 */
import {
	useMutation,
	useQuery,
	useQueryClient,
	type QueryKey,
	type UseMutationOptions,
	type UseQueryOptions,
} from "@tanstack/react-query";

import type { UniResponse } from "@floatctf/sdk";

import { unwrap } from "~/api/errors";

type Uni<T> = UniResponse<T> | { code?: number; message?: string; data?: T | null };

export function useUniQuery<T>(
	key: QueryKey,
	fn: () => Promise<Uni<T>>,
	action: string,
	options?: Omit<UseQueryOptions<T, Error, T, QueryKey>, "queryKey" | "queryFn">,
) {
	return useQuery<T, Error, T, QueryKey>({
		queryKey: key,
		queryFn: async () => unwrap<T>(await fn(), action),
		...options,
	});
}

export function useUniMutation<TData, TVars>(
	fn: (vars: TVars) => Promise<Uni<TData>>,
	action: string,
	options?: Omit<UseMutationOptions<TData, Error, TVars>, "mutationFn">,
) {
	const qc = useQueryClient();
	return useMutation<TData, Error, TVars>({
		mutationFn: async (vars) => unwrap<TData>(await fn(vars), action),
		...options,
		onSuccess: (data, vars, ctx, mutation) => {
			options?.onSuccess?.(data, vars, ctx, mutation);
			// 变更成功后统一失效：宁可多刷一点，也不要让界面停在旧数据上
			// （这个平台的"旧数据"常常意味着错误的比分或错误的赛事状态）。
			void qc.invalidateQueries();
		},
	});
}
