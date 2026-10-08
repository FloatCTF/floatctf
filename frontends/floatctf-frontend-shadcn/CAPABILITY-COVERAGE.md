# 能力覆盖表（CAPABILITY-COVERAGE）

> 由 `scripts/gen-capability-coverage.py` 从
> [`docs/frontend/CAPABILITY-MATRIX.md`](../../docs/frontend/CAPABILITY-MATRIX.md) 生成。
> **完整性 = 能力覆盖，不是路由对齐**：同一能力在本前端可能落在与 Default 完全不同的页面/交互里。

## 结论

- `required`：**89/89 覆盖**（complete 范围）
- `optional`：30 行，除个别平台侧只读信息外均已实现
- `specialized`：8 行，Docker / SQL / 终端 / 平台网络 / 前端选择器均已实现

## 逐行覆盖

| 域 | 能力 | 类别 | 落地位置 | 证据 / 说明 |
|---|---|---|---|---|
| 平台引导与运行时契约 · Platform | 公开引导元数据 `GET /api/frontend`（`active_frontend` / `platform_version` / `api_contract_version` / `frontend_runtime_version` / `capabilities`） | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 前端挂载契约 `mount(context)` 与 `FloatCTFMountContext` 字段（`root` / `apiBaseUrl` / `assetBaseUrl` / `frontendId` / `frontendVersion` / `platformVersion` / `apiContractVersion` / `frontendRuntimeVersion` / `capabilities`） | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 契约版本常量与兼容性判断 | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 制品 manifest / 本地注册表解析 | optional | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 破窗回退（`?frontend=<id>` / 内置 `default`） | optional | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 统一响应封装与成功码 | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | 统一错误模型 | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 平台引导与运行时契约 · Platform | SSE 传输原语（Bearer 走 `Authorization`，指数退避重连） | required | `src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx` | bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount） |
| 认证与会话 · Auth | 选手注册 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 选手登录（签发 JWT） | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 当前用户信息（`GET /users/me`） | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 修改个人资料 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 头像上传 | optional | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 忘记密码（发送重置邮件） | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 凭 token 重置密码 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 选手登出 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 管理端登录 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 管理端登出 | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 401 统一处理（清对应 token；重新登录的引导方式由前端决定） | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | token 注入（选手 / 管理端两个来源） | required | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 认证与会话 · Auth | 非 401 错误回调 | optional | `src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts` | 真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域 |
| 选手端：账号与社区内容 · Community | 全站公告列表 | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 武器库（Arsenal）列表 | optional | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 解题流水（分页 / 筛选） | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | Top15 用户排行榜 | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 讨论区列表 | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 讨论详情 | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 发帖 / 编辑 / 删除自己的讨论 | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 讨论点赞 / 取消 | optional | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：账号与社区内容 · Community | 讨论评论 CRUD | required | `src/features/community/*` · `src/features/profile/*` | 真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传 |
| 选手端：赛事与 Jeopardy · Events | 赛事列表（含 `hidden` / 家族 / 赛制过滤） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事详情（`EventInfo.event` / `joined` / `team_result`） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 加入 / 退出赛事 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 战队创建 / 加入 / 退出 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事公告 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事积分榜 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事趋势（分数随时间） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事实例列表 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Events | 赛事 writeup 状态 / PDF 提交 | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 题目目录（挑战列表，分页 / 分类筛选） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 题目详情（附件 / 描述 / 分值） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 独立题目实例获取 / 启动 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 独立题目实例销毁 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | flag 提交（挑战维度） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 我的实例列表 / 批量销毁 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 题集（Challenge Sets）列表 / 详情 | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 题集内题目实例 + flag 提交 | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 我的题解（per challenge）读取 / 保存 | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 题解列表 / 详情（他人 writeups） | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 赛事题目列表（按赛事开放集合） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | 赛事题目实例获取 / 启动 | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Jeopardy | flag 提交（赛事维度） | required | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：赛事与 Jeopardy · Media | Markdown 编辑器图片上传 | optional | `src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*` | 真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势 |
| 选手端：AWD · AWD | 赛事状态（`status` / `phase` / `current_round` / `banned` / `score`） | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | GameBox 列表与我的实例（IP / 容器 / 健康） | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | GameBox 重置 | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | flag 提交（攻击得分） | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | 积分榜（`attack_score` / `defense_score` / `rank`） | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | WireGuard 配置下发 | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | 队伍 SSH 凭据（端口 / 密码 / 实例清单） | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWD · AWD | 选手端实时流 | required | `src/features/awd/*`（`/arena/:eventId` 驾驶舱） | AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询 |
| 选手端：AWDP 比赛 · AWDP | 赛事总览（`phase` / 时长配置 / 我的 GameBox / `my_score`） | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 实例启动 / 停止 / 重置 / 查询 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | Break 阶段 flag 提交 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 补丁上传（Fix 阶段） | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 手工 Test Check | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 源码下载（presigned URL） | optional | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 轮次列表 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 我的官方评测结果 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 我的积分流水（score ledger） | optional | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 积分榜（矩阵明细 `AwdpScoreboardDetail`） | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 趋势 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP 比赛 · AWDP | 选手端实时流 | required | `src/features/awdp/*`（`/lab/:eventId` 工作台） | 真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 GameBox 目录 + 开始训练（`capability: "awdp"`） | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run 生命周期（读取 / 开始 / 停止 / 重置 / 结束 / 切阶段 / 重新训练） | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run 实例管理 | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run 破题 / 补丁 / 自检 / 全量校验 / 源码 | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run 轮次 / 评测 / 积分 | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run writeup 读写 | optional | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 选手端：AWDP Training Ground（练习） · AWDP Training | 练习 Run 实时流 | required | `src/features/training/*`（`/training`、`/training/runs/:runId`） | 练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流 |
| 管理端：总览与内容 · Admin | Dashboard 聚合总览（统计 / 需关注项 / 赛事 / 动态） | required | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 系统监控（CPU / 内存 / 磁盘 / 网卡 / Docker 概况） | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 平台版本（API 版本） | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 用户管理 CRUD | required | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 挑战管理 CRUD + 导入 / 校验 / 构建 / 扫描 | required | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 题集管理 CRUD + 题目增删 | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 全局公告 CRUD | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 讨论管理（列表 / 删除 / 评论删除） | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 武器库管理 CRUD + 文件上传 | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 超管账号 CRUD | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 操作日志查询 | optional | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：总览与内容 · Admin | 动态设置 CRUD（含受保护键 `FRONTEND_ACTIVE`） | required | `src/features/admin/core/*` | 真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络 |
| 管理端：赛事管理 · Admin Events | 赛事列表 / 创建 / 编辑 / 删除 | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事详情读写（含家族 / 赛制 / `hidden`） | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事数据大屏（题目 / 解题 / 积分 / 趋势聚合） | optional | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事题目管理（增删 / 改分 / 开放 / 隐藏） | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事用户管理（增删 / 封禁 / 解封） | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事战队管理（列表 / 删除 / 封禁 / 解封） | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事公告管理 CRUD | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事日志 | optional | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事 writeup 列表 + 报告导出 / 下载 | optional | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：赛事管理 · Admin Events | 赛事统一实例列表（challenge + gamebox 归一化） | required | `src/features/admin/events/*` | 真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据） |
| 管理端：AWD 运维 · Admin AWD | AWD 赛事配置（创建 / 读取 / 更新，乐观锁 `expected_updated_at`） | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 生命周期：deploy / start / pause / resume / finish / archive | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 凭据轮换与预检（precheck + 历史 `AwdPrecheckRun`） | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 分数调整与积分榜 | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 战队封禁 / 解封 | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 赛事 GameBox 挂载（列表 / 添加 / 更新 / 移除） | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | GameBox 库管理（导入 / 扫描 / 校验 / 构建 / 隐藏 / 删除 / 更新） | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 赛事网络分配 / 重新分配 | required | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWD 运维 · Admin AWD | 平台网络设置 / 健康 / 分配容量（全局控制面） | specialized | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | `/admin/network`（`awd.admin.getPlatformNetwork*`） |
| 管理端：AWD 运维 · Admin AWD | 赛事 GameBox 实例重置 | specialized | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | `client.awd.admin.resetGamebox` |
| 管理端：AWD 运维 · Admin AWD | 管理端实时流 | optional | `src/features/admin/awd-ops/*`（`/admin/events/:id/awd`） | 真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明） |
| 管理端：AWDP 运维 · Admin AWDP | AWDP 赛事配置读写（时长 / 分值 / 乐观锁） | required | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：AWDP 运维 · Admin AWDP | 生命周期：start / break-to-fix / finish | required | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：AWDP 运维 · Admin AWDP | 赛事 GameBox 挂载 / 卸载 / 列表 | required | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：AWDP 运维 · Admin AWDP | 赛事实例列表 | required | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：AWDP 运维 · Admin AWDP | 积分榜 | required | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：AWDP 运维 · Admin AWDP | 数据大屏 | optional | `src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`） | 真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏 |
| 管理端：基础设施、系统与平台设置 · Admin Infra | Docker 容器管理（列表 / 启动 / 停止 / 删除） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | `/admin/infra/docker`（容器 / 镜像 / 网络三块） |
| 管理端：基础设施、系统与平台设置 · Admin Infra | Docker 镜像管理（列表 / 删除） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 设置（含 FRONTEND_ACTIVE）/前端选择器（逃生舱）/日志/计划任务/超管/公告/讨论/武器库/Docker/SQL/终端/版本 |
| 管理端：基础设施、系统与平台设置 · Admin Infra | Docker 网络管理（列表 / 创建 / 删除） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 设置（含 FRONTEND_ACTIVE）/前端选择器（逃生舱）/日志/计划任务/超管/公告/讨论/武器库/Docker/SQL/终端/版本 |
| 管理端：基础设施、系统与平台设置 · Admin Infra | SQL 控制台（多语句执行） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | `/admin/infra/sql`，执行前需输入确认词 |
| 管理端：基础设施、系统与平台设置 · Admin Infra | 计划任务 CRUD + 手动运行 | optional | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 设置（含 FRONTEND_ACTIVE）/前端选择器（逃生舱）/日志/计划任务/超管/公告/讨论/武器库/Docker/SQL/终端/版本 |
| 管理端：基础设施、系统与平台设置 · Admin Infra | Web 终端（session 授权 + WebSocket 交互） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 逃生舱实现：`client.adminHttp.post("/terminal/session")` + 原生 `WebSocket`（SDK 无终端抽象） |
| 管理端：基础设施、系统与平台设置 · Admin Platform | 已安装前端选择器（读取本地注册表 + 写 `FRONTEND_ACTIVE`） | specialized | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 逃生舱实现：同源 `fetch(DEFAULT_REGISTRY_URL)` + `parseRegistry`；写设置走 `admin.settings.patch` |
| 管理端：基础设施、系统与平台设置 · Admin Platform | 静态制品下载（presigned URL → Blob） | optional | `src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*` | 设置（含 FRONTEND_ACTIVE）/前端选择器（逃生舱）/日志/计划任务/超管/公告/讨论/武器库/Docker/SQL/终端/版本 |

## 说明

1. `required` 行全部实现：每一项都用**真实接口**取数，并处理 loading / empty / error；
   分页 / 过滤 / 权限边界按后端语义实现（事件 `joined`、`AwdPlayerStatus.banned`、`AwdpOverview.phase` 等直接来自后端字段）。
2. **AWD 容器级验证范围**：本机同时运行生产栈，`awd deploy` 会在共享宿主创建容器与网络，
   因此 AWD 验收覆盖到「配置 / 状态 / 成员 / 战队 / 网络分配前」的真实读取与全部前端状态机，
   未执行 `deploy`（GameBox 实例与判题面板以真实空态呈现）。
3. 逃生舱（AI-FRONTEND-GUIDE §5.4，class B）逐项记录在 README §7 与本表 `证据 / 说明` 列。
