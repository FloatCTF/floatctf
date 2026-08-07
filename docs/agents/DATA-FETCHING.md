# 前端数据请求规范（DATA-FETCHING.md）

> 数据页面（列表/详情/统计）的性能基线。**新增任何数据页面都要遵守本文件**，否则会退化：切 tab 反复请求、翻页闪骨架屏、窗口聚焦重刷。

## 现状：已实施的基础设施（勿回退）

| 位置 | 内容 |
|------|------|
| `apps/web/src/integrations/tanstack-query/root-provider.tsx` | QueryClient **全局默认**：`staleTime: 30s`、`gcTime: 10min`、`refetchOnWindowFocus: false`、`retry: 1`（指数退避上限 8s）；**模块级真单例**（HMR 热更新不重建缓存） |
| `apps/web/src/components/Table.tsx`（GenericTable） | 所有表格页内置 `staleTime: 30s` + `placeholderData: keepPreviousData`（翻页保留上一页数据） |
| 分级示例（低频覆盖） | `admin/version.tsx` changelog、`service/profile.tsx` profile → `staleTime: 5min`（低频数据覆盖全局） |
| 轮询（实时数据） | 得分榜/趋势/仪表盘等用 `refetchInterval`（30-60s），见各 events 页面 |

**为什么这样**：历史上 QueryClient 无默认值（staleTime=0）→ 43 个查询点每次进页面必重新请求 + 窗口聚焦重刷 + 失败重试 3 次，dev 下"切标签页很久才出内容"。30s 缓存让重复进入直接命中内存缓存，**连 304 条件请求都不发**。

## 新增数据页面的硬性规则

### 1. 数据获取只用 useQuery / useMutation
禁止裸 `axios + useState + useEffect`（无法复用缓存、无法失效，且 StrictMode 下双请求）。
```tsx
const { data, isLoading } = useQuery({
    queryKey: ["weapons"],                     // 稳定字符串前缀，见规则 4
    queryFn: () => serviceApi.weapons.fetch(), // 来自 src/api/，不直接写 URL
});
```

### 2. 按数据易变度分级设置 staleTime（覆盖全局 30s）
| 数据类型 | staleTime | 例子 |
|----------|-----------|------|
| 低频（几乎不变） | `5 * 60_000` | changelog、个人资料、管理员列表 |
| 默认（中频） | 不写（继承全局 30s） | 普通列表、详情 |
| 实时（会持续变化） | `refetchInterval: 30_000~60_000`（配合较短 staleTime） | 得分榜、趋势、仪表盘统计 |

低频率覆盖写法：
```tsx
useQuery({
    queryKey: ["profile"],
    queryFn: () => serviceApi.users.getMe(),
    staleTime: 5 * 60_000,   // 低频数据 5 分钟缓存
});
```
**前提**：该数据的修改类 mutation 必须 `invalidateQueries` 对应 key（见规则 5），否则显示旧数据。

### 3. 翻页/筛选列表必须 keepPreviousData
列表切页/改筛选时保留上一页数据占位，禁止整表骨架闪烁：
```tsx
useQuery({
    queryKey: [subject, page, limit],
    queryFn: () => queryFn({ page, limit, filter }),
    placeholderData: keepPreviousData,   // ← 必须
});
```
GenericTable 已内置，直接用它即可；自写列表必须手动加。

### 4. queryKey 命名与失效
- key = `["稳定前缀", 参数...]`，前缀用固定字符串，不拼接动态文案：`["event", id]`、`["awd-scores", eventId]`、`["profile"]`
- **同数据的 useQuery 与 invalidateQueries 必须用同一个 key**（前缀一致即可模糊失效）
- 修改数据的 mutation 在 `onSuccess` 里失效对应列表/详情：
```tsx
const deleteMutation = useMutation({
    mutationFn: removeFn,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: [subject] }),
});
```

### 5. 实时数据
- 优先用封装好的 `useAwdEventStream`（SSE + 轮询兜底，`hooks/useAwdEventStream.ts`），不要自己写轮询
- 轮询用 `refetchInterval`（React Query 内置，带后台暂停），不要用 `setInterval + 手动 setState`

### 6. entity 类型同步
- 页面引用 `src/entity/*.ts`（由 `db:gen` 生成），**Schema 变更后必须重新生成并同步页面字段**
- 引用了 entity 不存在的字段 → tsc 报错（历史遗留：solves.tsx 的 avatar/nickname、discussions 页面的字段）；改 Schema 后 `mise run db:gen` 并用 `pnpm exec tsc --noEmit` 校验

## 审查清单（新数据页面提交前）

- [ ] 用 useQuery 而非裸 axios/useEffect
- [ ] 低频数据覆盖 `staleTime: 5min`（或按表分级）；实时数据用 `refetchInterval`
- [ ] 列表有 `keepPreviousData`（GenericTable 已内置）
- [ ] queryKey 前缀稳定，mutation onSuccess 失效对应 key
- [ ] 未手动写 setInterval 轮询、未在 effect 里 fetch
- [ ] `pnpm exec tsc --noEmit` 无新增错误

## 验证命令

```bash
cd apps/web
pnpm exec tsc --noEmit          # 类型检查（含 entity 同步）
pnpm build                      # 生产构建
# dev 手动验证：切走再切回数据页，30s 内 Network 面板应无新请求
```
