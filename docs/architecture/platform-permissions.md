# 平台路由 → 权限对照表（platform-permissions.md）

> 本文是 [IDENTITY-AUTHORIZATION-V2-PLAN.md](./IDENTITY-AUTHORIZATION-V2-PLAN.md) §7 / §12 / §24 的
> 配套**实现清单**：把当前源码里的**每一条** HTTP 路由映射到「认证级别 + 所需权限」。
>
> **实现状态：全部落地（Wave 0–11 完成）。**
> 分支 `feat/identity-authorization-v2` @ `52fe663`。本表描述的就是**当前实现**，
> 不再是目标态：逐路由的 `RequirePermission<P>` 已全部切换（156 个 handler），
> 粗粒度 `SuperAdminJwtGuard` 与 `LEGACY_GUARD_ALLOWLIST` 均已删除/清空。
>
> 落地内容：权限目录 `core::permission` 的 **29 条**权限；RBAC 三表
> （`platform_roles` / `user_platform_roles` / `platform_role_permissions`）；
> **仅内置 `super-admin`**（`protected` + `grants_all`，权限来自短路）；账号统一到
> `users`（v1 的 `super_admin` 表已由 M6 删除）；认证 v2：身份-only JWT
> （`sub`/`purpose`/`token_version`/`exp`）+ `AccountGuard` + `/api/session` 为**唯一**
> 登录入口 + `status`/`token_version` 每请求强制；授权每请求从 DB 解析（不缓存）。
> 守卫的**完整性**由 `apps/api/tests/authorization_route_coverage.rs` 与
> `http_auth_contract.rs` 的目录守卫锁定（**156** 条 admin 路由）。
>
> **401 / 403 契约（v2 起固定）**：401 = 未认证 / 令牌无效 / 凭据已吊销 /
> 用途不符；403 = 已认证但不允许（账号禁用 / 缺权限 / 对象级授权失败）。
> 注意这与 v1 不同：v1 把「无权限」也返回 401，v2 分开——403 不该触发
> 「清除令牌跳登录页」。
>
> 核验基线：`main` @ `7c382e6b35a39148cbf6ecb581a6611ac61c673d`。
> 路由来源：`apps/api/src/bootstrap/routes.rs` + 各模块 `configure_*_routes` /
> `admin_events_routes` / `admin_platform_routes` 的注册顺序 + handler 上的
> `#[get|post|patch|put|delete]` 属性 + `apps/api/tests/common/routes.rs` 的三方交叉验证。
>
> **实现约定**：`/api/admin/**` 一律**显式**声明所需权限（`RequirePermission<P>`）；
> 没有任何一条 admin 路由可以只做「已认证」检查。§23 Wave 5 的 Gate 逐条断言本表。

---

## 0. 认证级别图例

| 级别 | 含义 | 失败语义 |
|---|---|---|
| `public` | 无需 `Authorization` | — |
| `account` | 需要有效 Session token（`AccountGuard`） | 无/坏 token → **401**；`disabled` → **403** |
| `perm:<p>` | 需要 `account` **且** 拥有权限 `<p>` | 无权限 → **403** |

**401 / 403 的唯一权威定义**见计划 §11.3。禁止任何 admin handler 只写
`actor: AccountGuard` 而不写权限——那等于把管理端点变成「登录即可用」。

---

## 1. Identity / Accounts

| Method | Path | 级别 | 说明 |
|---|---|---|---|
| `GET` | `/api/admin/users` | `perm:users.read` | 账号列表（含 `status`、`roles:[{key,name}]`） |
| `GET` | `/api/admin/users/{user_id}` | `perm:users.read` | 账号详情 |
| `POST` | `/api/admin/users` | `perm:users.manage` | 建账号 |
| `PATCH` | `/api/admin/users/{user_id}` | `perm:users.manage` | 改资料 / 改密（改密同时 `token_version += 1`）/ 启用禁用 |
| `DELETE` | `/api/admin/users` | `perm:users.manage` | 批量删账号（受 §20.1 最后超管保护 + §20.3 自我锁定保护） |
| `GET` | `/api/admin/users/{user_id}/roles` | `perm:users.read` | **新增**：某账号的角色授予（含 `roles[]` 与**有效权限** `permissions[]`；`is_last_super_admin` 见计划 §29.5-6） |
| `PUT` | `/api/admin/users/{user_id}/roles` | `perm:roles.manage` | **新增**：整体替换角色集合（受 §20.2 反提权约束） |
| `GET` | `/api/admin/permissions` | `perm:roles.read` | **新增**：只读权限目录（enum 派生：`key`/`domain`/`description`/`high_risk`） |
| `GET` | `/api/admin/roles` | `perm:roles.read` | **新增**：角色列表（含 `protected`/`grants_all`/已授予账号数） |
| `GET` | `/api/admin/roles/{role_id}` | `perm:roles.read` | **新增**：角色详情 + 权限集（实现期改为按 UUID 寻址，见计划 §29.5-6） |
| `POST` | `/api/admin/roles` | `perm:roles.manage` | **新增**：建自定义角色（不可创建 `grants_all`；权限集 ⊆ actor） |
| `PATCH` | `/api/admin/roles/{role_id}` | `perm:roles.manage` | **新增**：改 name/description/权限集（`protected` 的权限集 → **409 Conflict**） |
| `DELETE` | `/api/admin/roles/{role_id}` | `perm:roles.manage` | **新增**：删角色（`protected` → **409 Conflict**） |

**Wave 10 删除**（现状存在，统一后由上面取代）：

| Method | Path | 现状级别 |
|---|---|---|
| `POST` | `/api/admin/session` | `public`（→ 迁移为 `/api/session`） |
| `GET` | `/api/admin/super_admin` | 旧 `SuperAdminJwtGuard` |
| `GET` | `/api/admin/super_admin/{super_user_id}` | 旧 `SuperAdminJwtGuard` |
| `POST` | `/api/admin/super_admin` | 旧 `SuperAdminJwtGuard` |
| `POST` | `/api/admin/super_admin/{super_user_id}` | 旧 `SuperAdminJwtGuard`（注意是 **POST** 不是 PATCH） |
| `DELETE` | `/api/admin/super_admin` | 旧 `SuperAdminJwtGuard` |

---

## 2. Content — Challenges

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/challenges` | `perm:challenges.read` |
| `GET` | `/api/admin/challenges/{challenge_id}` | `perm:challenges.read` |
| `POST` | `/api/admin/challenges` | `perm:challenges.manage` |
| `PATCH` | `/api/admin/challenges/{challenge_id}` | `perm:challenges.manage` |
| `DELETE` | `/api/admin/challenges` | `perm:challenges.manage` |
| `POST` | `/api/admin/challenges/check` | `perm:challenges.manage` |
| `POST` | `/api/admin/challenges/import` | `perm:challenges.manage` |
| `POST` | `/api/admin/challenges/build` | `perm:challenges.manage` |
| `POST` | `/api/admin/challenges/scan` | `perm:challenges.manage` |
| `GET` | `/api/admin/challenge_sets` | `perm:challenges.read` |
| `GET` | `/api/admin/challenge_sets/{challenge_set_id}` | `perm:challenges.read` |
| `POST` | `/api/admin/challenge_sets` | `perm:challenges.manage` |
| `PATCH` | `/api/admin/challenge_sets/{challenge_set_id}` | `perm:challenges.manage` |
| `DELETE` | `/api/admin/challenge_sets` | `perm:challenges.manage` |
| `POST` | `/api/admin/challenge_sets/{challenge_set_id}/challenges` | `perm:challenges.manage` |
| `DELETE` | `/api/admin/challenge_sets/{challenge_set_id}/challenges` | `perm:challenges.manage` |

> **`/api/admin/download?key=` 不在此域**。它的授权锚点是**赛事**（`event_writeup` 行 /
> 服务器生成的报告 key 中的 `event_id`），权限是 **`events.read`**，并要求
> **对象级授权**（`resolve_private_artifact`）。详见 §14.1。

---

## 3. Content — GameBoxes / Weapons

`GameBox 库`虽然挂在 `/api/admin/awd/` 路径下，域上属于**内容库**（AWD 与 AWDP 共用），
因此权限是 `gameboxes.*` 而不是 `awd.*`。

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/awd/gameboxes` | `perm:gameboxes.read` |
| `POST` | `/api/admin/awd/gameboxes/import` | `perm:gameboxes.manage` |
| `POST` | `/api/admin/awd/gameboxes/check` | `perm:gameboxes.manage` |
| `POST` | `/api/admin/awd/gameboxes/build` | `perm:gameboxes.manage` |
| `POST` | `/api/admin/awd/gameboxes/scan` | `perm:gameboxes.manage` |
| `PATCH` | `/api/admin/awd/gameboxes/{gamebox_id}` | `perm:gameboxes.manage` |
| `POST` | `/api/admin/awd/gameboxes/{gamebox_id}/hide` | `perm:gameboxes.manage` |
| `DELETE` | `/api/admin/awd/gameboxes` | `perm:gameboxes.manage` |
| `GET` | `/api/admin/weapons` | `perm:weapons.read` |
| `POST` | `/api/admin/weapons` | `perm:weapons.manage` |
| `PATCH` | `/api/admin/weapons/{weapon_id}` | `perm:weapons.manage` |
| `DELETE` | `/api/admin/weapons` | `perm:weapons.manage` |
| `POST` | `/api/admin/weapons/{weapon_id}/upload` | `perm:weapons.manage` |

---

## 4. Events（公共赛事域）

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/events` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}` | `perm:events.read` |
| `POST` | `/api/admin/events` | `perm:events.manage` |
| `PATCH` | `/api/admin/events/{event_id}` | `perm:events.manage` |
| `DELETE` | `/api/admin/events` | `perm:events.manage` |
| `GET` | `/api/admin/events/{event_id}/data` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/report` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/instances` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/writeups` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/logs` | `perm:logs.read` |
| `GET` | `/api/admin/events/{event_id}/users` | `perm:events.read` |
| `POST` | `/api/admin/events/{event_id}/users` | `perm:events.participants.manage` |
| `DELETE` | `/api/admin/events/{event_id}/users` | `perm:events.participants.manage` |
| `POST` | `/api/admin/events/{event_id}/users/{user_id}/banned` | `perm:events.participants.manage` |
| `POST` | `/api/admin/events/{event_id}/users/{user_id}/unbanned` | `perm:events.participants.manage` |
| `GET` | `/api/admin/events/{event_id}/teams` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/teams/{team_id}` | `perm:events.read` |
| `POST` | `/api/admin/events/{event_id}/teams` | `perm:events.participants.manage` |
| `DELETE` | `/api/admin/events/{event_id}/teams` | `perm:events.participants.manage` |
| `POST` | `/api/admin/events/{event_id}/teams/{team_id}/users` | `perm:events.participants.manage` |
| `DELETE` | `/api/admin/events/{event_id}/teams/{team_id}/users` | `perm:events.participants.manage` |
| `POST` | `/api/admin/events/{event_id}/teams/{team_id}/banned` | `perm:events.participants.manage` |
| `POST` | `/api/admin/events/{event_id}/teams/{team_id}/unbanned` | `perm:events.participants.manage` |
| `GET` | `/api/admin/events/{event_id}/challenges` | `perm:events.read` |
| `POST` | `/api/admin/events/{event_id}/challenges` | `perm:events.manage` |
| `PATCH` | `/api/admin/events/{event_id}/challenges` | `perm:events.manage` |
| `DELETE` | `/api/admin/events/{event_id}/challenges` | `perm:events.manage` |
| `POST` | `/api/admin/events/{event_id}/challenges/hidden` | `perm:events.manage` |
| `POST` | `/api/admin/events/{event_id}/challenges/open` | `perm:events.manage` |
| `GET` | `/api/admin/events/{event_id}/announcements` | `perm:events.read` |
| `GET` | `/api/admin/events/{event_id}/announcements/{announcement_id}` | `perm:events.read` |
| `POST` | `/api/admin/events/{event_id}/announcements` | `perm:events.manage` |
| `PATCH` | `/api/admin/events/{event_id}/announcements/{announcement_id}` | `perm:events.manage` |
| `DELETE` | `/api/admin/events/{event_id}/announcements` | `perm:events.manage` |

**设计说明（重要）**：`events.read` 覆盖了赛事**下挂只读集合**的 GET
（users/teams/challenges/announcements/writeups/data/report/instances）。
这不是「一个 endpoint 一个权限」，而是「读赛事」这一件事的多个视图。
写操作按**改动对象**分流到 `events.manage`（赛事内容）/
`events.participants.manage`（参与关系）。

---

## 5. Events — AWD

### 5.1 赛事级（`/api/admin/events/...`）

| Method | Path | 级别 |
|---|---|---|
| `POST` | `/api/admin/events/awd` | `perm:awd.manage` （创建 AWD 赛事配置） |
| `GET` | `/api/admin/events/{event_id}/awd` | `perm:awd.read` |
| `PATCH` | `/api/admin/events/{event_id}/awd` | `perm:awd.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/start` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/pause` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/resume` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/finish` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/archive` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/deploy` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/precheck` | `perm:events.lifecycle.manage` |
| `GET` | `/api/admin/events/{event_id}/awd/prechecks` | `perm:awd.read` |
| `GET` | `/api/admin/events/{event_id}/awd/judge` | `perm:awd.read` |
| `GET` | `/api/admin/events/{event_id}/awd/scores` | `perm:awd.read` |
| `POST` | `/api/admin/events/{event_id}/awd/score/adjust` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/teams/{team_id}/ban` | `perm:events.lifecycle.manage` |
| `DELETE` | `/api/admin/events/{event_id}/awd/teams/{team_id}/ban` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/tokens/rotate` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/gameboxes/{instance_id}/reset` | `perm:events.lifecycle.manage` |
| `GET` | `/api/admin/events/{event_id}/awd/gameboxes` | `perm:awd.read` |
| `POST` | `/api/admin/events/{event_id}/awd/gameboxes` | `perm:awd.manage` |
| `PATCH` | `/api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}` | `perm:awd.manage` |
| `DELETE` | `/api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}` | `perm:awd.manage` |
| `GET` | `/api/admin/events/{event_id}/awd/network` | `perm:awd.read` |
| `PUT` | `/api/admin/events/{event_id}/awd/network` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awd/network/reallocate` | `perm:events.lifecycle.manage` |
| `GET` | `/api/admin/events/{event_id}/awd/stream` | `perm:awd.read` （SSE，Bearer 头） |

### 5.2 平台级（`/api/admin/awd/...`）

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/awd/network` | `perm:awd.read` |
| `PATCH` | `/api/admin/awd/network` | `perm:awd.manage` |
| `GET` | `/api/admin/awd/network/health` | `perm:awd.read` |
| `GET` | `/api/admin/awd/network/allocations` | `perm:awd.read` |

### 5.3 `awd.manage` vs `events.lifecycle.manage` 的切分原则

| 问题 | 归属 |
|---|---|
| 「这个 AWD 赛事怎么配置」 | `awd.manage` |
| 「现在开始/暂停/结束这场比赛」 | `events.lifecycle.manage` |
| 「给某队调分 / 封队 / 重置靶机 / 轮换令牌 / 网络生效」 | `events.lifecycle.manage` |
| 「看比赛状态与分数」 | `awd.read` |

理由：**配置**与**推动时钟**是两种职责。一个只负责开赛/收赛的值班同学不应能改赛事配置；
一个负责搭赛场的人不应在比赛进行中随意重置靶机。这样 `ops-manager` 可以只拿
`events.lifecycle.manage` 值班，而 `event-manager` 拿全套。

---

## 6. Events — AWDP

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/events/{event_id}/awdp` | `perm:awdp.read` |
| `PATCH` | `/api/admin/events/{event_id}/awdp` | `perm:awdp.manage` |
| `POST` | `/api/admin/events/{event_id}/awdp/start` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awdp/break-to-fix` | `perm:events.lifecycle.manage` |
| `POST` | `/api/admin/events/{event_id}/awdp/finish` | `perm:events.lifecycle.manage` |
| `GET` | `/api/admin/events/{event_id}/awdp/runs` | `perm:awdp.read` |
| `GET` | `/api/admin/events/{event_id}/awdp/instances` | `perm:awdp.read` |
| `GET` | `/api/admin/events/{event_id}/awdp/gameboxes` | `perm:awdp.read` |
| `POST` | `/api/admin/events/{event_id}/awdp/gameboxes` | `perm:awdp.manage` |
| `DELETE` | `/api/admin/events/{event_id}/awdp/gameboxes/{eg_id}` | `perm:awdp.manage` |
| `GET` | `/api/admin/events/{event_id}/awdp/scores` | `perm:awdp.read` |
| `GET` | `/api/admin/events/{event_id}/awdp/data` | `perm:awdp.read` |

---

## 7. Community

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/announcements` | `perm:announcements.manage` |
| `POST` | `/api/admin/announcements` | `perm:announcements.manage` |
| `PATCH` | `/api/admin/announcements/{announcement_id}` | `perm:announcements.manage` |
| `DELETE` | `/api/admin/announcements` | `perm:announcements.manage` |
| `GET` | `/api/admin/discussions` | `perm:discussions.moderate` |
| `GET` | `/api/admin/discussions/{discussion_id}` | `perm:discussions.moderate` |
| `DELETE` | `/api/admin/discussions` | `perm:discussions.moderate` |
| `GET` | `/api/admin/discussions/{discussion_id}/comments` | `perm:discussions.moderate` |
| `DELETE` | `/api/admin/discussions/{discussion_id}/comments/{comment_id}` | `perm:discussions.moderate` |

**为什么公告的 GET 也要 `announcements.manage`**：公告**内容**本身是公开的
（`GET /api/announcements` 仅需登录），`/api/admin/announcements` 是**管理视图**
（包含未发布/历史/全量）。把它拆成 `announcements.read` 只会多一个 99% 场景下
与 `.manage` 同时授予的权限。

---

## 8. Platform（设置 / 系统 / 日志 / 调度）

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/settings` | `perm:settings.read` |
| `POST` | `/api/admin/settings` | `perm:settings.manage` ⚠️ 高危 |
| `PATCH` | `/api/admin/settings/{setting_id}` | `perm:settings.manage` ⚠️ 高危 |
| `DELETE` | `/api/admin/settings` | `perm:settings.manage` ⚠️ 高危 |
| `GET` | `/api/admin/dashboard/summary` | `perm:system.read` |
| `GET` | `/api/admin/system/monitor` | `perm:system.read` |
| `GET` | `/api/admin/system/version` | `perm:system.read` |
| `GET` | `/api/admin/logs` | `perm:logs.read` |
| `GET` | `/api/admin/logs/{log_id}` | `perm:logs.read` |
| `GET` | `/api/admin/scheduled_tasks` | `perm:scheduler.manage` |
| `GET` | `/api/admin/scheduled_tasks/{task_id}` | `perm:scheduler.manage` |
| `POST` | `/api/admin/scheduled_tasks` | `perm:scheduler.manage` |
| `PATCH` | `/api/admin/scheduled_tasks/{task_id}` | `perm:scheduler.manage` |
| `DELETE` | `/api/admin/scheduled_tasks` | `perm:scheduler.manage` |
| `POST` | `/api/admin/scheduled_tasks/{task_id}/run` | `perm:scheduler.manage` |

**前端管理没有独立权限**：切换生效前端完全通过 `settings` 的 `FRONTEND_ACTIVE`
键完成（`platform/frontend/api.rs` 只有未认证的 `GET /api/frontend`，无 admin 路由），
因此由 `settings.read` / `settings.manage` 覆盖，**不新增** `frontends.*` 权限。

---

## 9. Infrastructure（Docker / Database / Terminal）

| Method | Path | 级别 |
|---|---|---|
| `GET` | `/api/admin/docker/containers` | `perm:docker.read` |
| `POST` | `/api/admin/docker/containers/{container_id}/start` | `perm:docker.manage` ⚠️ 高危 |
| `POST` | `/api/admin/docker/containers/{container_id}/stop` | `perm:docker.manage` ⚠️ 高危 |
| `DELETE` | `/api/admin/docker/containers/{container_id}` | `perm:docker.manage` ⚠️ 高危 |
| `GET` | `/api/admin/docker/images` | `perm:docker.read` |
| `DELETE` | `/api/admin/docker/images/{image_id}` | `perm:docker.manage` ⚠️ 高危 |
| `GET` | `/api/admin/docker/networks` | `perm:docker.read` |
| `POST` | `/api/admin/docker/networks` | `perm:docker.manage` ⚠️ 高危 |
| `DELETE` | `/api/admin/docker/networks/{network_id}` | `perm:docker.manage` ⚠️ 高危 |
| `POST` | `/api/admin/database/exec_sql` | `perm:database.execute` ⚠️ 高危 |
| `POST` | `/api/admin/terminal/session` | `perm:terminal.open` ⚠️ 高危 |
| `GET` | `/api/admin/terminal/ws` | `perm:terminal.open` ⚠️ 高危 |

### 9.1 Terminal 的特殊要求

`POST /api/admin/terminal/session` 签发 60 秒、**单次使用**的 HttpOnly ticket
cookie（`path=/api/admin/terminal/ws`）；`GET /api/admin/terminal/ws` 消费 ticket
并升级 WebSocket。因此：

1. **两个端点都要 `terminal.open`**。仅保护 `/session` 不够——直接开 WS 也要拦。
2. WS 端点在 ticket 消费后**必须重新校验权限**（不能只信 ticket 里的 admin_id）。
   否则：用户在 ticket 有效期内被撤销 `terminal.open`，仍能拿到 shell。
3. 现状 `terminal_ws` 只在 `super_admin` 表里找 `admin_id`（`terminal.rs:236`）；
   Wave 4/5 要改成「`users` 里存在 + `status=active` + 有 `terminal.open`」。
4. ticket 里建议同时编码 `token_version`，消费时比对（与 §9.4 一致）。

---

## 10. 公共 / 账号级路由（非权限门禁）

这些路由**不属于** Platform RBAC，故意不带权限：

| Method | Path | 级别 | 说明 |
|---|---|---|---|
| `POST` | `/api/session` | `public` | 唯一登录入口（Wave 4） |
| `POST` | `/api/users/session` | `public` | 过渡别名 → Wave 10 删除 |
| `POST` | `/api/admin/session` | `public` | 过渡别名 → Wave 10 删除 |
| `POST` | `/api/users` | `public` | 注册 |
| `POST` | `/api/users/reset_password` | `public` | 发重置邮件 |
| `POST` | `/api/users/reset?token=` | `public` | 用 `PasswordReset` 用途 JWT 重置 |
| `GET` | `/api/frontend` | `public` | 未认证前端引导元数据（登录页依赖） |
| `GET` | `/api/users/me` | `account` | **新 `MeDto`**（含 roles/permissions，无 password） |
| `PATCH` | `/api/users/me` | `account` | 改自己资料 / 改密（`token_version += 1`） |
| `GET` | `/api/announcements` | `account` | 选手看公告 |
| `GET` | `/api/weapons` | `account` | 选手看武器库 |
| `...` | `/api/events/**`、`/api/discussions/**`、`/api/submit`、`/api/instances`、`/api/challenges/**`、`/api/service/**`、`/api/uploads/**` | `account` | 选手域；授权仍由**赛事参与关系**（`event_users`/`event_teams`/`event_team_members`）决定，**与 Platform RBAC 无关** |

**关键区分（§5 边界）**：`/api/**` 的选手路由即使由「持有 `events.lifecycle.manage`
的管理员账号」访问，也**必须**按选手规则校验（该账号是否参加了这场比赛），
不能因为它是管理员就放行。反之亦然。这条要写成测试（计划 §19 不变式 8）。

---

## 11. 内部路由（不在 RBAC 内）

| Method | Path | 认证 |
|---|---|---|
| `POST` | `/internal/awd/events/{id}/flags/issue` | `AwdInternalAuth`（事件级密钥） |
| `POST` | `/internal/awd/events/{id}/judge/claim` | 同上 |
| `POST` | `/internal/awd/events/{id}/judge/tasks/{task_id}/heartbeat` | 同上 |
| `POST` | `/internal/awd/events/{id}/judge/tasks/{task_id}/result` | 同上 |
| `GET` | `/internal/awd/events/{id}/health` | 同上 |
| `POST` | `/internal/awdp/practice/...` | AWDP internal auth |

**永不**接入 `AccountGuard` / `RequirePermission`。它们是**服务主体**，
由 `AwdInternalAuth` + `AwdInternalPrincipal` 识别（计划 §15.2、§19 不变式 7）。

---

## 12. 权限统计

| 域 | 权限数 | 权限 |
|---|---|---|
| Identity | 4 | `users.read` `users.manage` `roles.read` `roles.manage` |
| Content | 6 | `challenges.read` `challenges.manage` `gameboxes.read` `gameboxes.manage` `weapons.read` `weapons.manage` |
| Events | 4 | `events.read` `events.manage` `events.participants.manage` `events.lifecycle.manage` |
| AWD | 2 | `awd.read` `awd.manage` |
| AWDP | 2 | `awdp.read` `awdp.manage` |
| Community | 2 | `announcements.manage` `discussions.moderate` |
| Platform | 4 | `settings.read` `settings.manage` `system.read` `logs.read` |
| Scheduler | 1 | `scheduler.manage` |
| Infra | 3 | `docker.read` `docker.manage` `database.execute` |
| Terminal | 1 | `terminal.open` |
| **合计** | **29** | 高危 5：`settings.manage` `docker.manage` `database.execute` `terminal.open` `roles.manage` |

**路由统计**：`/api/admin/**` 约 **155** 条（含 Wave 5 新增 7 条、不含 Wave 10 删除的 6 条
`super_admin` 端点）。`apps/api/tests/common/routes.rs` 的
`admin_routes()` 必须覆盖到该数量（计划 §23 Wave 0 Gate：≥150）。

---

## 13. 维护规则

1. **新增 admin 路由 → 必须在本表登记一行并给出权限**，同时在
   `apps/api/tests/common/routes.rs` 加一条目录项。三处不一致 = CI 红。
2. **新增权限 → 必须**：加 `Permission` enum 变体 + 在本表 §12 统计更新 +
   在计划 §7.2 表更新 + 重新生成 guard（宏）。
3. **禁止**出现「按 HTTP method 自动推导权限」（`GET`→`.read`、其余→`.manage`）
   的偷懒实现：`POST /api/admin/session` 是 public、`POST .../awd/precheck` 是
   lifecycle、`POST /api/admin/users/{id}/roles` 是 `roles.manage` 而非 `users.manage`
   —— 语义无法机械推导。
4. **禁止**在本表出现 `perm:*` 之类的通配写法；权限必须是 §12 的 29 个之一。

---

## 14. 需要「对象级授权」而非仅权限的端点

绝大多数 admin 端点的授权是**静态**的：`RequirePermission<P>` 通过即可。
以下端点还需要**在 handler 内把请求参数解析为一个平台资源，再对该资源授权**。
这类端点不能只靠权限守卫。

### 14.1 `/api/admin/download?key=`（Wave 0 必修，计划 §7.6）

| 项 | 内容 |
|---|---|
| 静态权限 | `events.read` |
| 动态授权 | `resolve_private_artifact(key) → Option<event_id>`，`None` → **404** |
| 授权锚点 | 族 A：`event_writeup.file_url` **DB 主键检索**；族 B：服务器生成的报告 key 中的 `event_id` + `events` 表存在性 |
| 拒绝形态 | 空串 / 超长 / 以 `/` 开头 / 含 `\` / 含 `..` / 含 `//` / 无锚点 key |
| 明确的非目标 | **禁止**实现成 `key.starts_with("writeups/")` 就放行 |
| 消费者 | Default 三个赛制的 `writeups.tsx`；shadcn `tab-records.tsx`；xzmu 不使用 |

**为什么锚点是 event 而不是 challenge**：`event_writeup` 主键是 `(event_id, user_id)`，
列表端点 `GET /api/admin/events/{id}/writeups` 已由 `events.read` 保护。
若用 `challenges.read`，一个题库管理员将能读取**所有赛事**的写题记录，超出最小权限。

**第三方前端契约**：该端点**不变形**（仍 `?key=`），因此三个前端无需改动；
但第三方前端不得假设「有 `challenges.read` 就能下载任意 key」——
它需要 `events.read`，且 key 必须是**平台已知的**那两族之一。

### 14.2 未来新增此类端点的规则

若未来出现第三族私有制品（新前缀），**必须**：

1. 为它建立可核验的锚点（DB 行、或服务器生成的、内嵌资源 id 的 key 格式）；
2. 在 `resolve_private_artifact`（或对应的 resolve 函数）里显式登记；
3. 在本表 §14 增补一行并说明锚点；
4. 补 §14.1 同级的拒绝形态测试。

**禁止**为了让新前缀「能用」而放宽成前缀匹配。
