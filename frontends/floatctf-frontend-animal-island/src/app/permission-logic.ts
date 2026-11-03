/**
 * 权限判断的**纯逻辑**。
 *
 * 这个文件刻意**不 import 任何东西**（没有 React、没有 `~/` 别名、没有 store）：
 * 它是唯一需要被单元测试的授权相关逻辑，因此必须能被独立编译运行
 * （见 `tests/nav-permissions.test.ts`）。
 *
 * React 绑定在 `~/app/permissions.tsx`，它只是这里的薄封装。
 */

/** `/me` 里与授权有关的两个字段（其余字段与显隐判断无关）。 */
export type PermissionCarrier =
	| {
			permissions?: readonly string[] | null;
			grants_all?: boolean | null;
	  }
	| null
	| undefined;

/**
 * 某会话是否具备某条平台权限。
 *
 * ## 规则：**先看 `grants_all`，再看集合**
 *
 * 持有内置全权角色时，后端返回的 `permissions` **通常是空数组** ——
 * 它的权限来自短路，不是一行行列出来的。
 * 如果反过来先看集合，全权管理员会被判成"什么权限都没有"，
 * 然后看到一个空空的控制台 —— 而"看不到东西"通常不会有人报 bug。
 *
 * ## 这不是授权
 *
 * 返回值只用于**界面显隐**。真正的判定永远在服务端每个请求上：
 * 权限随时可能被撤销（后端每请求重解析），前端手里的 `/me` 可能是几分钟前的。
 * 因此任何写动作都必须保留错误处理，不能因为这里是 true 就假设一定成功。
 */
export function hasPermission(me: PermissionCarrier, permission: string): boolean {
	if (!me) return false;
	if (me.grants_all) return true;
	return (me.permissions ?? []).includes(permission);
}
