/**
 * 权限感知原语。
 *
 * ## 两件事必须分清楚
 *
 * 1. **菜单显隐**（`visibleConsoleNav`）已经按权限过滤 —— 没权限就看不到入口。
 * 2. **动作显隐**（本模块）—— 只读权限的人进得去页面，但**不应该看到写按钮**。
 *    在此之前，前端从不检查权限：只有 `weapons.read` 的人也能看到"新增/编辑/删除"，
 *    点了才吃 403。那是很糟的体验，也把"权限"变成了事后惩罚而不是事前引导。
 *
 * ## 这不是授权
 *
 * `can()` 只决定**界面上显不显示**。真正的判定永远在服务端每个请求上——
 * 权限随时可能被撤销（后端每请求重解析），而前端手里的 `/me` 可能是几分钟前的。
 * 因此：**任何写动作都必须保留错误处理**，不能因为 `can()` 返回 true 就假设它一定成功。
 *
 * ## `grants_all` 必须短路
 *
 * 持有内置全权角色时，`permissions` 数组**通常是空的**（权限来自后端短路，
 * 不是一行行列出来的）。因此判断顺序永远是「先看 grants_all，再看集合」，
 * 否则全权管理员会看到一个空空的控制台。
 */
import type { ReactNode } from "react";

import { useSession } from "~/auth/store";
import { hasPermission } from "~/app/permission-logic";

export { hasPermission, type PermissionCarrier } from "~/app/permission-logic";

/** 当前会话是否具备某条平台权限（仅用于界面显隐）。 */
export function useCan(): (permission: string) => boolean {
	const me = useSession((s) => s.me);
	return (permission: string): boolean => hasPermission(me, permission);
}

/** 当前会话的有效权限集合（`grants_all` 下 `has` 恒为真）。 */
export function usePermissions(): ReadonlySet<string> {
	const me = useSession((s) => s.me);
	const owned = me?.permissions ?? [];
	return {
		has: (p: string) => hasPermission(me, p),
		// 其余 Set 方法这里用不到；显式给出以免调用方误以为它是完整 Set。
		size: me?.grants_all ? Number.POSITIVE_INFINITY : owned.length,
	} as unknown as ReadonlySet<string>;
}

/**
 * 声明式门控：没有权限时**不渲染**子节点。
 *
 * ```tsx
 * <Can permission="weapons.manage">
 *   <Button>新建武器</Button>
 * </Can>
 * ```
 *
 * 刻意**不**提供"禁用但不隐藏"的模式（除非调用方自己用 `useCan` 实现）：
 * 对一个真的点不动的按钮，隐藏比灰掉更诚实 —— 灰按钮会让人反复尝试、
 * 怀疑是网络问题，而它其实永远不会成功。
 */
export function Can({
	permission,
	children,
	fallback = null,
}: {
	permission: string;
	children: ReactNode;
	fallback?: ReactNode;
}) {
	const can = useCan();
	return <>{can(permission) ? children : fallback}</>;
}
