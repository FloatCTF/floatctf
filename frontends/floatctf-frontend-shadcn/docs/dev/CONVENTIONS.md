# 实现约定（feat/ 作业口径）

> 本文件是**并行实现 feature 页面**的作业口径。动手前通读一遍，写代码时对照 §5、§6、§7。
> 上游文档：`../../docs/frontend/AI-FRONTEND-GUIDE.md`（平台契约）、
> `FRONTEND-PLAN.md`（本前端的身份 / 交互模型 / 路由计划）、
> `../../docs/frontend/CAPABILITY-MATRIX.md`（能力基准）。

## 0. 铁律（违反即返工）

1. **只允许 import**：`@floatctf/sdk`、`@floatctf/frontend-runtime`、`@floatctf/react`、
   `@tanstack/react-query`、`@tanstack/react-table`、`react`、`react-dom`、`react-markdown`、
   `remark-gfm`、`lucide-react`、`sonner`、`cmdk`、`radix-ui`、`class-variance-authority`、
   以及本包内的 `~/...` 与相对路径。
   **禁止** `frontends/default/*`、`apps/web/*`、`packages/*/src/*`、`@/...`、任何逃逸出本包的路径。
2. **禁止假数据 / 占位数据**。页面每条数据都必须来自真实接口；没有就显示空态。
3. **三态齐全**：loading（`<QueryState>` 或骨架）、empty（`EmptyBlock`）、error（`ErrorBlock`
   或 `QueryState` 内置）。**不得白屏**、不得静默失败。
4. **每个 mutation 都要反馈**：`onSuccess` → `toast.success(...)` + 失效相关 query；
   `onError` → `toast.apiError("失败标题", error)`。
5. **破坏性操作必须 `useConfirm()`**，并在 `consequences` 里写**真实后果**；
   高危操作（删赛事/删用户/执行 SQL/归档）用 `confirmPhrase`。
6. **禁止原生 `alert/confirm/prompt`**；用 `useConfirm()` / `toast` / `AlertDialog`。
7. **token 绝不进 URL / query string**；只在 `~/auth/store` 里读写。
8. **绝不渲染** `password`、`static_flag_value`（admin 之外）、`source_toml`；选手端 flag 只显示
   自己实例的（`InstancesDto.flag` 已由后端脱敏为空，别当成真值展示）。
9. **状态判定必须与后端一致**：用 `~/lib/event-status.ts` 的事件状态、SDK 枚举
   （`@floatctf/sdk/entity` 的 `AwdpPhase` / `EventFamily` 等），不得前端自造。
10. **只改你自己目录下的文件**。共享层（`src/api/**`、`src/app/**`、`src/components/**`、
    `src/lib/**`、`src/features/types.ts`）由集成方维护：缺东西**先报告**，不要改别人的文件。
    `src/app/nav.ts` 已经包含全部计划路由，**不需要**你新增导航项。

## 1. 你要交付什么

在你负责的 `src/features/<你的域>/` 目录下：

```
src/features/<域>/
├── routes.tsx      # 必须存在：导出 `feature: FeatureModule`
├── pages.tsx       # 页面组件（可按页拆多个文件）
└── components.tsx  # 域内复用组件（可选）
```

```tsx
// src/features/<域>/routes.tsx
import type { FeatureModule } from "~/features/types";
import { EventsPage } from "./pages";

export const feature: FeatureModule = {
	shell: "player", // "public" | "player" | "admin"
	routes: [
		{ path: "/events", element: <EventsPage /> },
		{ path: "/events/:eventId", element: <EventWorkspacePage /> },
	],
};
```

- `shell: "player"` 的路由**自动**被 `RequireUser` + 选手端外壳包裹；`"admin"` 同理。
- 路径必须 root-absolute；React Router v7 自动排名，静态段与参数段顺序无影响。
- 页面里**不要**再写鉴权判断（守卫已处理）；但要在数据层处理 403（`PermissionDeniedBlock`）。

## 2. 数据层（唯一正确写法）

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { errorText } from "~/api/errors";
import { toast } from "~/components/app/toast";
import { QueryState, EmptyBlock } from "~/components/app/states";

const client = useClient();
const queryClient = useQueryClient();

const query = useQuery({
	queryKey: qk.events.list({ page, limit: pageSize }),
	queryFn: () => callList<EventInfo>(client.service.events.fetch({ page, limit: pageSize })),
});

return (
	<QueryState
		query={query}
		isEmpty={(result) => result.items.length === 0}
		empty={<EmptyBlock title="暂无赛事" description="等管理员开放赛事后这里会出现。" />}
	>
		{(result) => <DataTable data={result.items} /* … */ />}
	</QueryState>
);
```

要点：

- **所有 query key 从 `~/api/keys.ts` 的 `qk` 取**；需要新 key → 报告（不要写裸数组）。
  例外：AWD 的 key **必须**用 `qk.awd.*`（与 `@floatctf/react` 的 SSE 失效常量对齐）。
- `call(promise, "动作名")` → `T`；`callList(promise)` → `{ items, meta }`；
  `callVoid(promise)` → `void`；`callMaybe(promise)` → `T | null`。
- 平台业务失败（HTTP 200 + `code !== 0`）已被信封守卫转成 rejection，所以 `await` 会抛错，
  `useQuery` 的 `isError` 会成立 —— **不要**自己读 `.code` / `.data`。
- 分页：`meta` 是 `QueryParams`（`{ offset, limit, page, total, filter }`）。列表页统一
  「page 从 1 开始 + pageSize」`<PaginationBar page pageSize total onPageChange onPageSizeChange />`。
- 轮询周期沿用平台已核实的语义：积分榜 / 趋势 **30s**，赛事公告 **60s**，管理端 dashboard **60s**，
  其余按需。用 `refetchInterval` 时同时给 `refetchIntervalInBackground: false`。
- mutation 后按域失效：
  ```tsx
  const mutate = useMutation({
  	mutationFn: (input: X) => call(client.service.xxx.doIt(input), "提交"),
  	onSuccess: () => {
  		toast.success("提交成功");
  		void queryClient.invalidateQueries({ queryKey: qk.challenges.all });
  		void queryClient.invalidateQueries({ queryKey: qk.instances.all });
  	},
  	onError: (error) => toast.apiError("提交失败", error),
  });
  ```

## 3. 认证 / 会话

- 选手 token：`useAuthStore`（`~/auth/store`），已注入 SDK；顶栏与资料页读 `useMe()`。
- 管理端 token 独立；`useIsAdminAuthenticated()`。
- 401 → transport 清对应 token 并跳登录页（带 `next`），页面**不需要**自己跳转。

## 4. 实时（SSE）

```tsx
import { useBindings } from "~/api/client";
import { RealtimePill } from "~/components/app/badges";

const { useAwdEventStream } = useBindings();
const stream = useAwdEventStream({ eventId, enabled: Boolean(eventId) });
// stream.connected / stream.connectionState / stream.lastEvent / stream.lastError / stream.invalidateAwd
<RealtimePill state={stream.connectionState} onRefresh={() => stream.invalidateAwd()} />
```

- SSE 的 401 **不会**触发 `onUnauthorized`，只会把 `connectionState` 变成 `"auth_error"`：
  **必须**把这个状态显示出来（`RealtimePill` 已覆盖）。
- hook 内部在非 connected 时自动降级轮询；**不要**再叠加 `refetchInterval`。
- AWDP 的 `lastEvent` 来自 ref（不一定触发重渲染）；用 `invalidateAwdp` / 失效 key 驱动刷新。
- 只用 `useAwdEventStream` / `useAdminAwdEventStream` / `useAwdpEventStream` / `useAwdpRunStream`；
  手写流时 `client.sse.connect({ url, headers: {}, signal, onEvent })` 三个字段都必填。

## 5. 共享组件 API（`src/components/app/*`）

### `states.tsx`

```tsx
<QueryState query={query} isEmpty={(d) => d.length === 0} empty={<EmptyBlock …/>}
            skeleton={<TableSkeleton rows={6} columns={4} />} errorTitle="加载赛事失败">
  {(data) => …}
</QueryState>
<LoadingBlock label="加载中…" />
<TableSkeleton rows columns />
<ErrorBlock error={error} title="…" onRetry={refetch} />
<InlineError error={error} />
<EmptyBlock title description icon action variant="empty"|"filtered" />
<PermissionDeniedBlock description />
<NotFoundBlock title description action />
<RefreshingBadge active={query.isFetching && !query.isPending} />
```

### `page.tsx`

```tsx
<PageHeader title description actions badge breadcrumbs />
<PageBody>…</PageBody>                                  // 统一容器：max-w-screen-2xl + 响应式边距
<SectionCard title description actions footer contentClassName>…</SectionCard>
<StatCard label value hint icon tone="default|success|warning|danger" />
<Toolbar>…</Toolbar>
<KeyValueList items={[{ key, value, hint? }]} columns={1|2|3} />
<MonoText>…</MonoText> <CopyText value label /> <SecretValue value label />
<PaginationBar page pageSize total onPageChange onPageSizeChange pageSizeOptions />
<TagList items={string[]} />
<ReadonlyBlock>…</ReadonlyBlock>
<Scrollable>…</Scrollable>
```

### `data-table.tsx`

```tsx
<DataTable
  data={rows}
  getRowId={(row) => row.id}
  columns={[
    { id: "title", header: "标题", cell: (row) => <span>{row.event.title}</span>,
      sortValue: (row) => row.event.title },
    { id: "score", header: "分数", align: "right", cell: (row) => <MonoText>{row.score}</MonoText> },
  ]}
  rowActions={(row) => <RowMenu row={row} />}
  onRowClick={(row) => navigate(`/events/${row.id}`)}
  selectable selectedIds={ids} onSelectionChange={setIds}   // 批量操作
  mobileCard={(row) => <MobileRow row={row} />}             // 移动端卡片（建议提供）
  empty={<span>暂无数据</span>}
/>
```

### `confirm.tsx` / `toast.tsx`

```tsx
const confirm = useConfirm();
const ok = await confirm({
  title: "销毁实例？", description: "该操作会停止容器。",
  consequences: ["容器会被删除，未保存的进度丢失"],
  tone: "danger", confirmText: "销毁", confirmPhrase: "destroy",
});
toast.success("已销毁"); toast.apiError("销毁失败", error); toast.info("…");
```

### `form.tsx`

```tsx
<FormSheet open onOpenChange title description width="md|lg|xl" footer={<FormFooter … />}>
  <Field label="标题" htmlFor="title" required hint="…" error={errors.title}>
    <Input id="title" … />
  </Field>
  <FormGrid columns={2}>…</FormGrid>
</FormSheet>
```
表单库：简单表单用受控 state；复杂表单用 `react-hook-form` + `zod`（`@hookform/resolvers/zod`）
搭配 shadcn `Form` / `FormItem` / `FormControl` / `FormMessage`。两种都可，但**校验错误必须可见**。

### `markdown.tsx`

```tsx
<MarkdownView>{challenge.description}</MarkdownView>
<MarkdownEditor value={value} onChange={setValue} minHeight={280} />
```

### `badges.tsx`

```tsx
<TonePill tone="neutral|muted|success|warning|danger|info" icon>…</TonePill>
<RealtimePill state={stream.connectionState} onRefresh={…} />
<BooleanPill value={event.hidden} trueText="已隐藏" falseText="公开" trueTone="muted" />
```

### `charts.tsx`

```tsx
<TrendChart series={[{ name: item.name, points: item.points.map(p => ({ x: Date.parse(p.time), y: p.score })) }]}
            formatY={(v) => v.toFixed(0)} formatX={(x) => new Date(x).toLocaleTimeString("zh-CN", { hour12: false })} />
<Sparkline points={[1,3,2,5]} />
```

### `user-cell.tsx` / `lib/format.ts` / `lib/hooks.ts` / `lib/event-status.ts`

```tsx
<UserCell username nickname avatar /> <UserAvatar name avatar size="sm|default|lg" />
<RelativeTime value={iso} /> <AbsoluteTime value={iso} seconds />
formatDateTime(v, { seconds }) / formatDate / formatRelative / formatDuration(secs) / formatScore(v)
formatDelta(v) / formatBytes(n) / formatPercent(r) / truncate(s, n) / countdownText(t) / formatRange(a, b)
toDatetimeLocalValue(iso) / fromDatetimeLocalValue(v)
useDebouncedValue(v, ms) / useNow(ms) / useMediaQuery(q) / useCopyToClipboard() / useDisclosure() / useDocumentTitle(t)
computeEventStatus(start, end) / EVENT_STATUS_LABEL / eventStatusTone / EVENT_FAMILY_LABEL /
PARTICIPANT_MODE_LABEL / EVENT_PURPOSE_LABEL / eventCapabilities(event)
```

## 6. SDK 陷阱（已核实，逐条避免）

1. 领域方法返回 **信封**；`data?: T` 可选 —— 用 `call*` 助手，别读 `.data`。
2. **柯里化方法**（`admin.event_challenges.fetch(eventId)` 返回函数）必须二次调用：
   `client.admin.event_challenges.fetch(id)({ page: 1 })`。
   受影响：`event_challenges.fetch/remove`、`event_users.fetch/delete`、`event_announcements.*`、
   `event_logs.fetch`、`event_writeups.fetch`、`event_teams.getTeams/remove`、
   `challenges.getChallengeSet/removeChallengeFromSet`。`getTeams(id)()` 第二层**无参数**。
3. **位置参数顺序相反**：`instances.launchSingle(challenge_id, event_id)` ↔
   `events.launchSingleInstance(event_id, challenge_id)`。
4. `admin.event_users.add` 运行时返回 AxiosResponse（我的 `unwrap` 已兼容，直接用 `call`）。
5. `admin.super_admin.patch(id, data)` 实际发 **POST**；`uploads.upload_avatar` 实际发 **PATCH**。
6. `patch(Partial<X>)` 系列**必须带 `.id`**（URL 用它拼接）。
7. 批量删除是 `DELETE` + body：`remove(id_list: string[])`。
8. `QueryParams` 只有 `offset/limit/page/total/filter` 五个键 —— `{ search }`、`{ pageSize }` 是**类型错误**。
   搜索用 `filter`，页码用 `page`，每页条数用 `limit`。
9. 上传字段名：`package_zip`（challenges.importChallenge、awd.importGamebox）、`writeup_pdf`
   （submit.submitWriteup）、`image_file`（uploads）、`weapon`（weapons.upload）、`patch_file`（AWDP patch）。
10. AWD/AWDP config PATCH 的乐观锁 `expected_updated_at`：读详情时把 `updated_at` 存下来回传。
11. `awd.admin.getEventNetwork` 在**未分配**时返回 404（reject）——要当成「未分配」而不是「加载失败」。
12. `awd.admin.getStatus` 的 `data` 可能是 `null`（赛事未开 AWD）——用 `callMaybe`。
13. `AwdPrecheckRun.error_msg` 是 **JSON 字符串**，需要 `JSON.parse`。
14. `GameBoxLibraryDto.healthchecks_json/judge_args_json` 读是 `unknown|null`，写是 JSON **字符串**
    （要 `JSON.stringify`）；`updateEventGamebox`/`updateGamebox` 里显式传 `null` = **清空**。
15. `admin.events.exportWriteUps` 与 `getReport` 是同一个 `GET /events/{id}/report`。
16. `submit.submit` 与 `submitSingle` 是同一个 `POST /submit/flag`（后者 body 带 `event_id`）。
17. `admin.download.download` 返回 `void`（内部触发浏览器下载），失败抛普通 `Error`。
18. `docker` 的 `PortInfo` 是 PascalCase（`IP`/`PrivatePort`/`PublicPort`/`Type`），其余 DTO 全是 snake_case。
19. `awdp` 有两个 `AwdpPhase`：`api/awdp.ts` 的是字符串联合（含 **`preparing_fix`** 过渡态，
   必须如实展示），`entity/sea_orm_active_enums.ts` 的是 TS enum。
20. 字段不存在时不要臆造：`EventInfo` 没有 `title`/`family`（都在 `.event` 下）；
   管理端 `AdminInstanceRow` 没有 `flag`；`AwdPlayerStatus` 没有 `round_duration_secs`（那在 `AwdEventStatus`）。
21. `resolveSseUrl` 不做重复前缀检测：`client.sse.connect({ url })` 传**相对**路径（不带 `/api`）。
22. `users.reset` 由 SDK 把 token 拼进查询串（SDK 行为，非本前端）；我们只调用它，不自己拼。
23. `client.service.awd === client.awd.player`、`client.admin.awd === client.awd.admin`（同一对象）。
24. Web 终端没有 SDK 抽象：用逃生舱 `client.adminHttp.post("/terminal/session")` + 原生 `WebSocket`，
   并在交付说明中声明（Gap class B）。
25. 前端本地注册表：`DEFAULT_REGISTRY_URL` + `parseRegistry`（`@floatctf/frontend-runtime`），
   取数用同源 `fetch`；**这是逃生舱**，也要在交付说明里声明。
26. ⚠️ **`EventInfo.id` 运行时不存在（后端不返回）**：选手端 `GET /events` 与 `GET /events/{id}`
   的响应只有 `{ event, team_result, joined }`，ID 真值在 **`event.id`**。直接用 `info.id` 会发出
   `/events/undefined/...` 请求（已实测 400「路径参数格式错误」）。请一律用
   `~/lib/event-status` 的 **`eventIdOf(info)`**。管理端 `admin.events.*` 返回的是完整实体，有 `id`。

## 7. 版式与代码风格

- 只用 shadcn 组件（`~/components/ui/*`）+ Tailwind 类；**不要**引入别的 UI 库、不要写 CSS 文件
  （唯一例外：确有必要时在自己的域目录下加 `styles.css` 并由 `routes.tsx` `import "./styles.css";`）。
- 颜色只用语义 token（`bg-card`、`text-muted-foreground`、`text-destructive`、`text-[var(--success)]`…），
  不写死 hex；数值/flag/时间戳加 `tnum font-mono`（可用 `<MonoText>`）。
- 中文文案，简洁、专业；错误文案来自后端（`errorText`）。
- TypeScript：`verbatimModuleSyntax`（类型导入必须 `import type`）、`noUnusedLocals`/`noUnusedParameters`
  （不许留未使用的 import 与变量）、`strict`。**不要**用 `any` / `as any` 绕过类型。
- 删除类操作给 `variant="destructive"`；破坏性图标按钮加 `aria-label`。
- 列表/详情页顶部用 `<PageHeader>`；区块用 `<SectionCard>`；表格用 `<DataTable>`。

## 8. 交付前自查（必须真的跑）

```bash
cd /home/fb0sh/Projects/floatctf
mise exec -- pnpm --filter @floatctf/frontend-shadcn exec tsc --noEmit 2>&1 | grep -E "你的域|features/<你的域>" || echo "你的域无类型错误"
```

- [ ] 我负责的每个能力都有真实接口调用，没有假数据
- [ ] 每个列表/详情有 loading / empty / error
- [ ] 每个 mutation 有成功提示 + 可见错误 + query 失效
- [ ] 破坏性操作有 `useConfirm()` 且写明后果
- [ ] 没有 `alert(` / `confirm(` / `prompt(`、没有 `@/` 导入、没有逃逸路径
- [ ] 没有渲染 `password` / `static_flag_value`
- [ ] `tsc --noEmit` 在我的域内 0 错误（别人的域报错不用管，但要报告）

## 9. 交付时报告什么

1. 覆盖的能力（逐条，对应 CAPABILITY-MATRIX 行文案）；
2. 新增的路由路径；
3. 用到的**逃生舱**（`serviceHttp`/`adminHttp`/`transport`/原生 WebSocket/裸 fetch）及原因；
4. 任何公共契约缺口（`PUBLIC SDK GAP: …`）——**不要**改后端或平台包；
5. 未完成 / 有疑问的点。
