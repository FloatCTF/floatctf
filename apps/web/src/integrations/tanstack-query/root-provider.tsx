import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

// 全局默认：避免每个 tab 切换/窗口聚焦都重新请求。
// staleTime 30s：30 秒内重复进入同一页面直接走缓存，不再发请求；
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
        staleTime: 30_000,
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
