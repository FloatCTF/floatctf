import { Spinner } from "@primer/react";

/**
 * 路由切换加载态。
 *
 * 切换 navlist 导航时，目标路由 chunk 懒加载 / loader 执行期间
 * 渲染在内容区（Outlet），替代默认白屏。
 * 由 main.tsx 的 router.defaultPendingComponent 全局挂载。
 */
export function RouteLoading() {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <Spinner size="large" />
      <span className="ml-2 text-sm text-gray-500">加载中…</span>
    </div>
  );
}
