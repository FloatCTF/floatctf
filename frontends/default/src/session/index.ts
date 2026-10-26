/**
 * Default Frontend 的主站登录态编排（服务端 Cookie 会话 + 旧版 JWT 兼容）。
 *
 * 平台规定能力语义，这里决定 UI 反应：
 * - 启动时 `ensureSession()` 向服务端确认登录态（不看 localStorage）；
 * - 旧版 `auth-storage.token` 在统一会话部署上一次性换成服务端会话后删除（平滑迁移，不直接丢弃）；
 * - 本浏览器恰好一个可恢复账号时自动恢复（例如刚在 LearnOS 的 FloatCTF 登录页输入过密码）；
 * - 登录 / 切换 / 退出后广播给其他标签页；当前用户变化时清空查询缓存，避免上一个账号的数据残留。
 */
import type { WebSessionStatus } from "@floatctf/sdk";

import { getContext } from "@/integrations/tanstack-query/root-provider";
import { useAuthStore } from "@/stores/AuthStore";
import { useSessionStore } from "@/stores/SessionStore";

const CHANNEL = "floatctf-web-session";

let inflight: Promise<WebSessionStatus> | null = null;

/**
 * 延迟取得 SDK 会话接口：`@/api/client` 在创建客户端时依赖本模块（会话头），
 * 这里若静态反向引用会形成 client → session → client → router 的初始化环。
 */
async function api() {
	const { client } = await import("@/api/client");
	return client.service.webSession;
}
let channel: BroadcastChannel | null = null;

function legacyStatus(): WebSessionStatus {
	return {
		mode: "legacy",
		authenticated: false,
		user: null,
		csrf_token: null,
		accounts: [],
		auto_restore: false,
		max_accounts: 1,
		global_logout: false,
	};
}

/** 当前是否为服务端 Cookie 会话模式。 */
export function isCookieMode(): boolean {
	return useSessionStore.getState().mode === "cookie";
}

/**
 * SDK `getUserSessionHeaders`：Cookie 模式下随每个选手端请求附带 CSRF 与期望账号。
 * 尚无 CSRF（浏览器还没有账号容器）时返回 null，由具体接口显式传入。
 */
export function sessionHeaders(): Record<string, string> | null {
	const { mode, status } = useSessionStore.getState();
	if (mode !== "cookie" || !status?.csrf_token) return null;
	const headers: Record<string, string> = { "X-CSRF-Token": status.csrf_token };
	if (status.user) headers["X-FloatCTF-Account"] = status.user.id;
	return headers;
}

/** 当前 CSRF token（尚无容器时用 `anonymous`，服务端只要求自定义头存在）。 */
export function csrfToken(): string {
	return useSessionStore.getState().status?.csrf_token ?? "anonymous";
}

/** 写入最新状态；当前用户变化时清空查询缓存，并（可选）通知其他标签页。 */
export function applyStatus(next: WebSessionStatus, broadcast = false): void {
	const previous = useSessionStore.getState().status;
	useSessionStore.getState().setStatus(next);
	if (previous && previous.user?.id !== next.user?.id) {
		getContext().queryClient.clear();
	}
	if (next.user) {
		// 只是展示用缓存（旧页面读取 username/nickname）；浏览器禁用存储时不能影响登录态切换。
		try {
			useAuthStore.getState().setUsername(next.user.username);
			useAuthStore.getState().setNickname(next.user.nickname);
		} catch {
			// ignore
		}
	}
	if (broadcast) channel?.postMessage({ type: "changed" });
}

async function bootstrap(): Promise<WebSessionStatus> {
	let status: WebSessionStatus;
	try {
		status = await (await api()).status();
	} catch (error) {
		// 旧版 API（没有该接口）或 OIDC 未启用：按旧版 JWT 模式运行。
		const httpStatus = (error as { httpStatus?: number }).httpStatus;
		if (httpStatus === 404) return legacyStatus();
		throw error;
	}
	if (status.mode !== "cookie") return status;

	const legacyToken = useAuthStore.getState().token;
	if (legacyToken) {
		if (!status.authenticated) {
			try {
				status = await (await api()).migrate(
					legacyToken,
					status.csrf_token,
				);
			} catch (error) {
				const httpStatus = (error as { httpStatus?: number }).httpStatus;
				// 网络故障时保留旧令牌，下次再迁移；服务端明确拒绝（过期/无效）才丢弃。
				if (!httpStatus || httpStatus >= 500) throw error;
			}
		}
		useAuthStore.getState().removeToken();
	}
	if (!status.authenticated && status.auto_restore) {
		try {
			status = await (await api()).restore(status.csrf_token);
		} catch {
			// 并发标签页已改变状态（409）等：保留未登录状态，由用户选择或登录。
			status = await (await api()).status();
		}
	}
	return status;
}

function listen(): void {
	if (channel || typeof BroadcastChannel === "undefined") return;
	channel = new BroadcastChannel(CHANNEL);
	channel.onmessage = () => {
		void refreshSession();
	};
}

/** 启动时确认登录态（只执行一次；路由守卫都 await 它）。 */
export function ensureSession(): Promise<WebSessionStatus> {
	if (!inflight) {
		inflight = bootstrap()
			.then((status) => {
				applyStatus(status);
				if (status.mode === "cookie") listen();
				return status;
			})
			.catch((error) => {
				inflight = null;
				throw error;
			});
	}
	return inflight;
}

/** 重新向服务端读取状态（401 之后、其他标签页通知、切换账号后）。 */
export async function refreshSession(): Promise<WebSessionStatus> {
	const status = await (await api()).status();
	applyStatus(status);
	inflight = Promise.resolve(status);
	return status;
}

/** 主站是否已登录（两种模式统一判断）。 */
export async function isSignedIn(): Promise<boolean> {
	const status = await ensureSession();
	if (status.mode === "cookie") return status.authenticated;
	return Boolean(useAuthStore.getState().token);
}

/**
 * 以当前 CSRF 调用状态变更接口；CSRF 已被轮换（另一标签页登录/退出）时 403 → 刷新状态后重试一次。
 * 结果写回状态并通知其他标签页。
 */
export async function mutateSession(
	run: (csrf: string) => Promise<WebSessionStatus>,
): Promise<WebSessionStatus> {
	let next: WebSessionStatus;
	try {
		next = await run(csrfToken());
	} catch (error) {
		if ((error as { httpStatus?: number }).httpStatus !== 403) throw error;
		await refreshSession();
		next = await run(csrfToken());
	}
	applyStatus(next, true);
	inflight = Promise.resolve(next);
	return next;
}
