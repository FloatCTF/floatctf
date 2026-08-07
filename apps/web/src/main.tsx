import { BaseStyles, ThemeProvider } from "@primer/react";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { StrictMode } from "react";
import ReactDOM from "react-dom/client";
import * as TanStackQueryProvider from "./integrations/tanstack-query/root-provider.tsx";
import { RouteLoading } from "./components/RouteLoading.tsx";
import reportWebVitals from "./reportWebVitals.ts";
// Import the generated route tree
import { routeTree } from "./routeTree.gen";
import "./style.css";
// Create a new router instance

const TanStackQueryProviderContext = TanStackQueryProvider.getContext();
export const router = createRouter({
  routeTree,
  context: {
    ...TanStackQueryProviderContext,
  },
  defaultPreload: "intent",
  scrollRestoration: true,
  defaultStructuralSharing: true,
  defaultPreloadStaleTime: 0,
  // 懒加载 chunk / loader 期间在内容区显示加载态，替代默认白屏。
  defaultPendingComponent: RouteLoading,
  defaultPendingMs: 100, // 100ms 内完成的路由切换不闪烁加载态
  defaultPendingMinMs: 300, // 加载态至少展示 300ms，避免快速加载时闪一下
  // 路由加载失败 / 404 时给出提示，避免整页白屏。
  defaultErrorComponent: ({ error }: { error: Error }) => (
    <div className="flex h-full w-full items-center justify-center">
      <p className="text-red-600">页面加载失败：{String(error)}</p>
    </div>
  ),
  defaultNotFoundComponent: () => (
    <div className="flex h-full w-full items-center justify-center">
      <p>404 · 页面不存在</p>
    </div>
  ),
});

// Register the router instance for type safety
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

// Render the app
const rootElement = document.getElementById("app");
if (rootElement && !rootElement.innerHTML) {
  const root = ReactDOM.createRoot(rootElement);
  root.render(
    <StrictMode>
      <TanStackQueryProvider.Provider {...TanStackQueryProviderContext}>
        <ThemeProvider>
          <BaseStyles>
            <RouterProvider router={router} />
          </BaseStyles>
        </ThemeProvider>
      </TanStackQueryProvider.Provider>
    </StrictMode>
  );
}

// If you want to start measuring performance in your app, pass a function
// to log results (for example: reportWebVitals(console.log))
// or send to an analytics endpoint. Learn more: https://bit.ly/CRA-vitals
reportWebVitals();
