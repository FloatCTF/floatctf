/**
 * 应用装配与生命周期。
 *
 * `mount(context)` 的返回值是**清理函数**：平台可能卸载本前端（切换前端 / 回退 Default），
 * 因此这里必须真正销毁 React 树并断开 QueryClient，而不是只移除 DOM。
 */
import { useEffect, useState } from "react";
import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";
import {
	MutationCache,
	QueryCache,
	QueryClient,
	QueryClientProvider,
} from "@tanstack/react-query";
import { Notification } from "animal-island-ui";
import { BrowserRouter } from "react-router";
import { createRoot, type Root } from "react-dom/client";

import { createRuntime, registerSessionHandlers, type Runtime } from "~/api/client";
import { errorText } from "~/api/errors";
import { useSession } from "~/auth/store";
import { RuntimeContext } from "~/app/runtime-context";
import { App } from "~/app/App";
import { ConfirmProvider } from "~/components/confirm";

function buildQueryClient(): QueryClient {
	// 统一的失败呈现：任何未被页面自己处理的 query/mutation 失败都要**可见**。
	// 平台级要求是「错误必须可见、绝不静默失败」，在数据层兜底比要求每个页面都记得更可靠。
	return new QueryClient({
		defaultOptions: {
			queries: {
				retry: (failureCount, error) => {
					// 4xx 是确定性失败，重试没有意义（只会让用户多等）。
					const status = (error as { httpStatus?: number })?.httpStatus;
					if (status && status >= 400 && status < 500) return false;
					return failureCount < 2;
				},
				staleTime: 10_000,
				refetchOnWindowFocus: false,
			},
		},
		queryCache: new QueryCache({
			onError: (error, query) => {
				// 只在后台刷新失败时提示：首屏失败由页面自己的三态呈现，
				// 否则用户会同时看到「出错了」和一条 toast。
				if (query.state.data === undefined) return;
				Notification.error({ message: "数据刷新失败", description: errorText(error) });
			},
		}),
		mutationCache: new MutationCache({
			onError: (error) => {
				Notification.error({ message: "操作未成功", description: errorText(error) });
			},
		}),
	});
}

/**
 * 会话引导：token 存在时拉一次 `/me`。
 *
 * 为什么必须在**应用级**做而不是各页面自己做：导航显隐、管理端入口、账号名
 * 都依赖 `/me`；若让页面各自拉取，会出现「顶栏还没权限、侧栏已经有了」的撕裂状态。
 */
function SessionBootstrap({ runtime }: { runtime: Runtime }) {
	const token = useSession((s) => s.token);
	const setMe = useSession((s) => s.setMe);
	const clear = useSession((s) => s.clear);
	const [checked, setChecked] = useState(false);

	useEffect(() => {
		if (!token) {
			setChecked(true);
			return;
		}
		let cancelled = false;
		runtime.client.service.users
			.getMe()
			.then((res) => {
				if (cancelled) return;
				// `/me` 返回 MeDto：账号字段 + permissions / grants_all。
				// 这些权限**只用于界面显隐**，真正的判定在服务端。
				const data = (res as { data?: unknown }).data ?? res;
				setMe(data as Parameters<typeof setMe>[0]);
			})
			.catch(() => {
				if (cancelled) return;
				// 401 已由 transport 的 onUnauthorized 清会话；
				// 这里只保证引导流程不会卡在「加载中」。
				clear();
			})
			.finally(() => {
				if (!cancelled) setChecked(true);
			});
		return () => {
			cancelled = true;
		};
	}, [token, runtime, setMe, clear]);

	if (!checked) return null;
	return null;
}

export function mountApp(context: FloatCTFMountContext): () => void {
	const runtime = createRuntime(context);
	const queryClient = buildQueryClient();

	// 会话失效 / 无权限的用户可见反馈。
	registerSessionHandlers({
		onSessionExpired: (_scope, reason) => {
			Notification.warning({ message: "需要重新登录", description: reason });
		},
		onForbidden: (message) => {
			// 403 **不**清会话：重新登录解决不了权限问题。
			Notification.error({ message: "没有权限", description: message });
		},
	});

	const host = document.createElement("div");
	host.className = "ai-app";
	context.root.replaceChildren(host);
	const root: Root = createRoot(host);

	root.render(
		<QueryClientProvider client={queryClient}>
			<RuntimeContext.Provider value={runtime}>
				<BrowserRouter>
					<ConfirmProvider>
						<SessionBootstrap runtime={runtime} />
						<App />
					</ConfirmProvider>
				</BrowserRouter>
			</RuntimeContext.Provider>
		</QueryClientProvider>,
	);

	void context; // platformVersion / capabilities 经 runtime 暴露

	return () => {
		root.unmount();
		// 断开查询缓存：否则卸载后仍有 in-flight 请求回调写进已废弃的缓存。
		queryClient.clear();
		host.remove();
	};
}
