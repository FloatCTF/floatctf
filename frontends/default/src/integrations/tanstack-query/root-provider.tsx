import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

// 全局默认：避免每个 tab 切换/窗口聚焦都重复请求，同时保证切换后能尽快拿到新数据。
// staleTime 5s：5 秒内重复进入同一页面直接走缓存；超过 5s 的再次进入会在保留旧数据渲染的前提下后台刷新（不白屏）。
//   —— 曾为 30s，用户反馈"切换 tab/nav 长期看不到新数据，只有刷新才有"，故调短；
//      React Query 在数据 stale 时挂载仍会先渲染缓存再后台 refetch，不会退化为"切标签页卡顿"。
// refetchOnWindowFocus false：从 IDE 切回浏览器时不重刷；
// retry 1：接口失败只重试一次，避免默认 3 次退避叠加等待。
//
// 注意：全局只是兜底，按数据易变度分级应放在具体 useQuery 上覆盖，
// 例如低频数据（changelog/管理员列表）用 staleTime: 5 * 60_000，
// 实时数据（得分榜等）用更短 staleTime 或 refetchInterval。
function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 5_000,
        gcTime: 10 * 60_000,
        refetchOnWindowFocus: false,
        retry: 1,
        retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 8000),
      },
    },
  })
}

// 模块级真单例：getContext 只被 main.tsx 调用一次，
// 但把实例提升到模块顶层更严谨——HMR 替换页面组件时
// QueryClient 不被重建，内存缓存跨热更新保留。
const queryClient = createQueryClient()

export function getContext() {
  return {
    queryClient,
  }
}

export function Provider({
  children,
  queryClient,
}: {
  children: React.ReactNode
  queryClient: QueryClient
}) {
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}
