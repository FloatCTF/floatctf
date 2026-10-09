# 身份与授权 v2 实施计划（IDENTITY-AUTHORIZATION-V2-PLAN.md）

> **状态：IMPLEMENTED / VALIDATED（部分 Wave）** —— 见文末 §29 Implementation Status。
> 分支 `feat/identity-authorization-v2`，当前 HEAD `2aaa092`；**未 push / 未 merge / 未 tag**。
> 已完成的 Wave 均为**本地提交**且 `mise run check` 全绿。
> 目标：把「管理员账号」从一种**独立账号类型**降级为「持有平台角色的普通账号」，让
> **身份（Authentication）** 与 **授权（Authorization）** 彻底分离，并把平台管理能力
> 收敛到可审计的 RBAC 模型。
>
> 事实基线：`main` @ `7c382e6b35a39148cbf6ecb581a6611ac61c673d`
> （本计划准备期间 HEAD 从 Prompt 给出的 `be44fa0` 前进到 `02905ef`，再到 `7c382e6`；
> 未做 reset / rebase / 回退，工作区里既有的 `chore/*.md` 删除与 submodule 改动未被触碰）。
>
> 阅读顺序建议：本文 §1 → §3 → §4 建立目标，§6–§8 是数据与权限设计，§13–§15 是迁移，
> §23–§24 是执行节奏（Wave + Gate）。路由→权限对照表见
> [platform-permissions.md](./platform-permissions.md)。
>
> 本文档遵循仓库既有文档风格（`docs/agents/ARCHITECTURE.md` / `docs/frontend/ARCHITECTURE.md`）：
> 中文正文 + 英文技术名词、编号章节、密集表格、ASCII 图。
> **目录选择说明**：§41 要求 `docs/architecture/`，该目录此前不存在（`docs/` 下只有
> `agents/`、`frontend/`、`deployment/`），因此本计划新建 `docs/architecture/` 作为
> 「跨域架构决策」目录，并沿用 `docs/agents/*.md` 的大写文件名约定；不把跨域架构文档塞进
> `docs/agents/`（那是 AI 作业手册，不是设计文档）。

---

## 1. Executive Summary

FloatCTF 今天有两个**互不相通的人类身份空间**：

- `players` → `public.users`，登录 `POST /api/users/session`，JWT `role = User`
- `admins` → `public.super_admin`，登录 `POST /api/admin/session`，JWT `role = SuperAdmin`

两套表、两套登录、两个 token（前端 `token` / `adminToken`）、两套守卫
（`UserJwtGuard` / `SuperAdminJwtGuard`）、两套 FK 主体列（`logs.user_id` vs
`logs.superadmin_id`）。授权粒度是**全有或全无**：`SuperAdminJwtGuard` 出现在
**35 个源文件 / 约 100 个 handler** 上，绝大多数 handler 甚至不读守卫的值
（`_user: SuperAdminJwtGuard`）——守卫本身就是授权，任何超管可以做任何事。
`modules/identity/authorization/mod.rs` 只有 3 行，是 `jwt::{AuthClaims, Role}` 的
re-export：**平台今天不存在 RBAC**。

本计划把模型改为：

```text
users（唯一人类账户表）
  │
  ├── Player identity / profile
  ├── Event membership（event_users / event_teams / event_team_members，语义不变）
  └── Platform RBAC
          │
          ▼
    user_platform_roles ──▶ platform_roles ──▶ platform_role_permissions
```

关键决策（详见 §28）：统一 `users` + 单会话 token + 平台角色/权限；**保留 `/api/admin`
命名空间**（它不是身份边界，而是「平台管理域」边界）；JWT **不携带** roles/permissions；
**代码 enum 是权限权威**（不建 DB permission catalog）；新增 `token_version` 与
`users.status`；`super-admin` 用 `protected=true, grants_all=true` **结构化**表达，
而不是一堆 permission 行；`AwdJudger` 是**死代码**（全仓无构造点）应当删除而不是迁移；
第一版**不做** Redis 权限缓存；因为公共 API 契约破坏，`API_CONTRACT_VERSION` 从 `"1"` 升到 `"2"`。

执行切分为 **12 个 Wave（0–11）+ 6 个 migration**，每个 Wave 有独立验收 Gate，
不允许跨 Wave 偷做后续逻辑。后端每个 Wave 内「migration + Rust 代码」同一次提交，
天然原子；前端因两个 submodule（`floatctf-frontend-xzmu` / `floatctf-frontend-shadcn`）
在**独立仓库**，采用**有强制拆除 Wave 的分段切换**（不是永久兼容层）。

同时发现 3 个必须先修的高风险问题（Wave 0）：

1. **密码哈希通过 API 泄漏**：`UsersDto.password` / `SuperAdminDto.password` 直接序列化
   argon2 哈希，`GET /api/admin/users`、`GET /api/admin/super_admin` 等 7 个端点全部外泄。
2. **SDK 把用户列表打进浏览器控制台**：`packages/sdk/src/api/admin/users.ts:9` 的
   `console.log(res.data)` 在每次管理端拉用户列表时把哈希打到 console。
3. **E2E 鉴权路由目录有大面积盲区**：`apps/api/tests/common/routes.rs` 覆盖约 94 条 admin
   路由，但**完全不含** `/api/admin/terminal/*`、`/api/admin/dashboard/summary`、
   `/api/admin/awd/**`、`/api/admin/events/*/awd/**`、`/api/admin/events/*/awdp/**`
   ——恰恰是最危险的一批端点。

---

## 2. Current Architecture（as-built，全部经源码核验）

### 2.1 身份与登录

| 项 | players | admins |
|---|---|---|
| 表 | `public.users` | `public.super_admin` |
| 主键 | `uuid_generate_v4()` | `uuid_generate_v4()` |
| 唯一约束 | `users_username_key`、`users_nickname_key`、`idx_users_email`（unique） | `super_admin_username_key` |
| 其它列 | `nickname NOT NULL`、`avatar NULL`、`created_at/updated_at` | 无 nickname / avatar |
| 登录 | `POST /api/users/session` | `POST /api/admin/session` |
| 签发 | `gen_jwt_token(id, Role::User, None)` | `gen_jwt_token(id, Role::SuperAdmin, None)` |
| 守卫 | `UserJwtGuard(users::Model)` | `SuperAdminJwtGuard(super_admin::Model)` |
| 守卫校验 | `role == User` **且** `users.find_by_id(sub)` | `role == SuperAdmin` **且** `super_admin.find_by_id(sub)` |

源码：`apps/api/src/api/extractor/auth.rs:26-103`、
`apps/api/src/modules/identity/authentication/mod.rs:19-73`、
`apps/api/src/modules/identity/administrator/mod.rs:26-84`。

### 2.2 路由与守卫分布

- `apps/api/src/bootstrap/routes.rs:9-20`：`/api` scope 下挂 player 路由 + `/api/admin` 子 scope。
- **注意**：`POST /api/admin/session` 由 `configure_player_routes` 注册
  （`routes.rs:29-31` → `identity::configure_session_routes`），handler 自带绝对路径
  `#[post("/admin/session")]`（`administrator/mod.rs:27`）。即「管理端登录」在路由装配上
  属于 player 配置，这是历史耦合的一个具体形态。
- `SuperAdminJwtGuard` 出现在 **35 个文件**，约 100 个 handler。绝大多数形如
  `_user: SuperAdminJwtGuard`——**守卫即授权**。

### 2.3 JWT

```rust
// apps/api/src/core/security/jwt.rs:28-42
pub enum Role { User, SuperAdmin, ResetAccount, AwdJudger }
pub struct AuthClaims { pub sub: Uuid, pub role: Role, pub exp: usize }
```

- 算法 HS512，密钥来自 TOML（`Secret`），进程级 `OnceLock`。
- **`Role::AwdJudger` 是死代码**：全仓只有声明处
  （`jwt.rs:33`）与两个测试使用（`apps/api/tests/regression_security_concurrency.rs:180,186`），
  **没有任何构造点**。AWD 内部服务认证走完全独立的机制：
  `AwdInternalAuth` extractor + `awd_events` 事件级密钥
  （`apps/api/src/modules/event/awd/api/auth.rs:13-80`），与 JWT `Role` 无关。
- 日志中 `Role::ResetAccount` 是唯一被真正用作「用途」的变体
  （`authentication/mod.rs:176,236`），其余三个变体全部当作「身份类型」用。

### 2.4 授权

```rust
// apps/api/src/modules/identity/authorization/mod.rs —— 全文 3 行
pub use crate::core::security::jwt::{AuthClaims, Role};
```

**平台不存在任何 RBAC / permission 概念。** 事件域有一个同名但语义不同的
`EventCapabilities`（`modules/event/common/domain/capability.rs:8-97`），描述「这个赛制
支不支持实例/靶机/轮次」，与「这个账号能不能管理」无关——两者**绝不能混用**。

### 2.5 审计主体

```rust
// apps/api/src/infrastructure/logging.rs:40-70
pub async fn add_log(&self, level, category, action, message, details,
                     user_id: Option<Uuid>, superadmin_id: Option<Uuid>, request)
```

- `logs` 表同时有 `user_id uuid` 与 `superadmin_id uuid`（baseline `:1580-1591`），
  索引 `idx_sys_logs_user_op ON logs(user_id, superadmin_id)`（`:2489`）。
- 薄门面 `AuditService::record(action, msg, details, actor_user_id, actor_superadmin_id, req)`
  （`infrastructure/audit/service.rs:70-92`）。
- 全仓 `add_log` / `record` 调用点 **109 处**；其中 108 处是具体调用，
  **1 处是 `AuditService::record` 内部的转发**（`service.rs:81`）。
  **经脚本核验：没有任何具体调用点同时传入两个主体**，因此「主体归一」的
  `COALESCE(superadmin_id, user_id)` 回填是无损的。
- 「无主体」的语义今天已经存在：系统/内部动作传 `None, None`
  （如 `InternalAuthFailed`），即 **NULL 主体 = 系统/服务动作**，无需新增 `actor_type`。

### 2.6 管理动作外键（全部 7 个）

| Table.Column | → | ON DELETE | 可空 |
|---|---|---|---|
| `announcements.publisher_id` | `super_admin(id)` | **CASCADE** | NOT NULL |
| `awd_internal_token_rotations.rotated_by` | `super_admin(id)` | SET NULL | NULL |
| `awd_score_events.created_by` | `super_admin(id)` | SET NULL | NULL |
| `awd_team_bans.banned_by` | `super_admin(id)` | SET NULL | NULL |
| `awd_team_bans.unbanned_by` | `super_admin(id)` | SET NULL | NULL |
| `logs.superadmin_id` | `super_admin(id)` | SET NULL | NULL |
| `awd_reset_records.requested_by_admin` | `super_admin(id)` | SET NULL | NULL |

来源：baseline `20260810121925-initial-schema.sql:2049,2106,2163,2184,2199,2331`；
`20260914120000-awd-reset-requester-admin.sql:16-18`。
`super_admin` 在 **48 个 migration 中只出现于这两处**。

### 2.7 引导数据（bootstrap）

- 唯一来源：baseline `20260810121926-initial-data.sql`，**非幂等** plain INSERT：
  `id = 00000000-0000-0000-0000-000000000000`（**nil UUID**）、`sysadmin`、
  固定 argon2id 哈希、`sysadmin@system.com`、`created_at/updated_at = 2025-09-29 13:04:49.689893`。
- **Rust 里没有任何 admin 种子代码**。`bootstrap/mod.rs:135-136` 只调用
  `seed_default_settings`（settings 表）。
- `scripts/install.sh` **零命中** `super_admin` / `sysadmin` / `ADMIN_USERNAME` /
  `ADMIN_PASSWORD`。安装器会随机化基础设施密钥（`install.sh:1882-1889`：
  `POSTGRES_PASSWORD` / `RUSTFS_SECRET_KEY` / `JWT_SECRET` / `AWD_ROOT_KEY` /
  `INTERNAL_TOKEN_KEY`），**但管理员口令不随机化**。
- 无强制改密：无 `must_change_password` / `force_password` 类字段或逻辑。
- `docs/agents/DATABASE.md:69` 明文规定：「**sysadmin credential 策略 accepted as-is，
  不要改 seed**」——所以本计划**不修改 baseline**，只在**新增 migration** 里迁移它。
- 另外 `core/system_ids.rs:17` 用 `Uuid::from_u128(0)` 作为 `SCHED_CHECK_PRACTICE_EVENT`，
  与 sysadmin 的 nil UUID **数值相同但不同表**；该文件已明文声明「序号按表独立分配，
  不同表可复用同一 n，无跨表唯一约束」。

### 2.8 SDK 与前端

- **SDK**：`FloatCTFAuthScope = "user" | "admin"`；两个 axios 实例
  （`createFloatCTFTransport("user"|"admin", ...)`），差异**只有** baseURL、token getter、
  401 的 scope 标签；`onUnauthorized` 只在 401 触发；`FloatCTFError.unauthorized`
  严格等于 `httpStatus === 401`，**403 没有独立字段**。
- **Default 前端**：`AuthStore`（zustand persist，key `auth-storage`）持有
  `token` / `adminToken` / `username` / `nickname`；`api/client.ts:24-34` 按 `scope`
  分支处理 401；`routes/admin/route.tsx:18-26` 的 `beforeLoad` **只检查
  `adminToken` 是否存在**；导航 `navigation/admin-navigation.tsx:52-182` 无 `isVisible`。
- **已存在但未接线的权限接缝**：`NavigationVisibilityContext { permissions: ReadonlySet<string>;
  features: ReadonlySet<string> }`（`navigation/sidebar-types.ts:8-15`）+
  `filterNavigationSections()`（`navigation/sidebar-utils.ts:131-140`）+ `SideBar` 的
  `visibility` prop ——**今天两个调用点都没传**，默认 `EMPTY_NAVIGATION_VISIBILITY`。
  这是本计划最省力的一处落点。
- **三个前端**：`frontends/default`（in-repo）、`frontends/floatctf-frontend-xzmu`
  与 `frontends/floatctf-frontend-shadcn`（**git submodule，独立仓库**）。后两者同样
  使用 `getUserToken` / `getAdminToken` / `onUnauthorized({scope})`，并且**自行**在
  transport 上装了 envelope guard。

### 2.9 契约版本

- `apps/api/src/core/contract.rs:18` 与 `packages/frontend-runtime/src/version.ts:17`
  两处 `API_CONTRACT_VERSION = "1"`，由 `scripts/check-architecture.sh`（rule #7）
  断言一致，`mise run lint` / `check` 会跑。
- `contract.rs` 的规则原文：「**加法式变更不升 major**……只有『旧前端会因此坏掉』的
  变更才升 major」。
- `PLATFORM_CAPABILITIES = ["jeopardy","awd","awdp","discussions","writeups","web_terminal"]`
  是**未认证**的 `GET /api/frontend` 暴露的**静态平台能力**，经
  `FloatCTFMountContext.capabilities` 传给前端。

---

## 3. Problems

### P1 — 两套人类身份空间（架构债）

管理员不是「有权限的账号」，而是「另一种账号」。后果：

- 一个既参赛又管理的人必须有**两个账号**，两次登录，两个 token。
- 前端 `AuthStore` 永久持有两个 token 分支；`onUnauthorized` 被迫按 `scope` 分流。
- `AdminRouteGuardWithRedirect` 无法表达「已登录但没有管理权限」，只能表达「有没有 adminToken」。
- 审计主体分裂：同一个人在两列里出现，跨表 join 才能拼出「这个人干过什么」。

### P2 — 授权粒度全有或全无

`SuperAdminJwtGuard` = 授权。约 100 个管理端 handler 里绝大多数形如
`_user: SuperAdminJwtGuard`，连守卫值都不读。无法表达
「赛事管理员」/「只读审计员」/「只发公告的人」。

### P3 — 密码哈希经 API 外泄（安全，必须最先修）

```rust
// apps/api/src/modules/identity/user/dto.rs:6-15
pub struct UsersDto { pub id, pub username, pub nickname, pub password: String, pub email, ... }
// apps/api/src/modules/identity/administrator/dto.rs:6-13
pub struct SuperAdminDto { pub id, pub username, pub password: String, pub email, ... }
```

- `GET /api/admin/users`、`GET /api/admin/users/{id}`、`POST /api/admin/users`、
  `PATCH /api/admin/users/{user_id}` 返回 `UsersDto`（哈希未清空）。
- `GET /api/admin/super_admin`、`GET /api/admin/super_admin/{id}`、
  `POST /api/admin/super_admin`、`PATCH /api/admin/super_admin/{id}` 返回 `SuperAdminDto`（哈希未清空）。
- 唯一清空的地方是 `GET /api/users/me`（`user/mod.rs:33` `user.password = "".to_string()`），
  但 DTO **仍有该字段**，只是值为空串。
- SDK 把同样带 `password` 的生成实体当作响应类型
  （`packages/sdk/src/entity/users.ts:5`、`entity/super_admin.ts:4`，
  被 `api/admin/users.ts:7-19`、`api/service/users.ts:7-13`、`api/admin/super_admin.ts:9-27` 复用）。

### P4 — SDK 把用户列表打进浏览器 console

`packages/sdk/src/api/admin/users.ts:9` `console.log(res.data);` ——每次管理端拉
用户列表都会把**整份含 hash 的列表**写进浏览器 console。（同文件
`api/admin/event_challenges.ts:54` 的 `console.log(id_list)` 属无害但应一并清理。）

### P5 — 403 语义缺失，401/403 会被前端误当「登录过期」

- `AppError::Unauthorized → 401`、`AppError::Forbidden → 403` 后端**已经区分**
  （`apps/api/src/api/app_error.rs:62,112`）。
- 但守卫在「角色不对 / 查不到账号」时一律返回 **401**（`auth.rs:56-60,96-100`）。
- 前端拿到 401 会 `removeAdminToken()` + 跳登录页（`api/client.ts:24-34`）。
- 结论：**一旦 RBAC 上线，任何「已认证但无权限」若返回 401，都会把用户踢下线。**
  403 契约必须先立起来。

### P6 — `impl Responder for UniResponse<T>` 恒返回 HTTP 200

```rust
// apps/api/src/api/response.rs:66-68
fn respond_to(self, _req) -> HttpResponse { HttpResponse::Ok().json(self) }
```

已核验：`UniResponse::err` 全仓**只有 1 个调用点**，在 `AppError::to_response()`
（`app_error.rs:71`），而它只经 `ResponseError::error_response` 到达 → **当前后端不会
产生「HTTP 200 + code != 0」**。因此这是**潜在**陷阱而非现存缺陷：一旦有人（尤其新写的
permission guard）直接 `return UniResponse::err(403, ...)`，客户端会当成成功。
本计划把「权限失败必须用 `AppError::Forbidden`」写成硬约束，并把
`floatCTFErrorFromEnvelope` 接入 SDK 拦截器作为纵深防御。

### P7 — E2E 鉴权路由目录盲区

`apps/api/tests/common/routes.rs` 的 `admin_routes()` 覆盖到
`/api/admin/docker/networks` 为止，**不含**：

- `POST /api/admin/terminal/session`、`GET /api/admin/terminal/ws`
- `GET /api/admin/dashboard/summary`
- `/api/admin/awd/**`（平台网络、GameBox 库）
- `/api/admin/events/awd`、`/api/admin/events/{id}/awd/**`
- `/api/admin/events/{id}/awdp/**`

即「每个受保护路由拒绝匿名」这条最强契约，恰好漏掉了终端 / 原始 SQL 相邻的
高危面与整个 AWD/AWDP 管理面。测试的 `catalog_sizes` 只断言 `a.len() >= 60`，
无法发现遗漏。

### P8 — 有权限接缝但无数据源

前端 `NavigationVisibilityContext.permissions` + `filterNavigationSections()` 已实现且
有单测（`navigation/__tests__/sidebar-utils.test.ts:115-154`），但**没有任何地方
把真实权限喂进去**——因为没有 `/me` 返回 roles/permissions，SDK 也没有对应类型。
三个前端因此都只能「登录即可见全部管理导航」。

### P9 — 三个前端 + 两个独立仓库的切换耦合

`floatctf-frontend-xzmu` / `floatctf-frontend-shadcn` 是 submodule。平台发布的
`package-sdk-dist.sh` 只产出一份 SDK tarball，submodule 指针必须指向**已存在**的提交。
因此「平台 + 三个前端在同一个 commit 原子切换」在物理上不可行。

---

## 4. Target Architecture

### 4.1 核心原则

> **User is identity, not a role.**
> 管理员不再是一种账号类型，而是「持有平台角色」的普通账号。
>
> **Authentication = 你是谁；Authorization = 你能做什么。**

### 4.2 目标请求链

```text
Authorization: Bearer <session JWT>
        │
        ▼
  validate_jwt()                        // 签名 + exp
        │
        ▼
  purpose == Session ?                  // TokenPurpose 分派
        │  ├── PasswordReset → 只允许 reset 端点
        │  └── Session → 继续
        ▼
  AccountGuard
    ├── users.find_by_id(sub)           // 唯一人类账户表
    ├── status == active ?              // 否则 403（不是 401）
    └── token_version == claim ?        // 否则 401（凭据已失效）
        │
        ▼
  AuthContext { user, roles, permissions }
        │
        ▼
  RequirePermission(P)                  // 声明式 extractor
        │  ├── 无权限 → AppError::Forbidden → HTTP 403
        │  └── 通过 → handler
        ▼
      handler(actor: AuthContext, ...)
```

### 4.3 身份与授权分离

```text
┌──────────────────────────────────────────────────────────────────────┐
│  Authentication                                                      │
│  users（唯一账户表）+ Session JWT（sub / purpose / token_version / exp）│
└──────────────────────────────────────────────────────────────────────┘
                    │  同一个 Bearer token
                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Authorization（Platform RBAC，每次请求解析，不进 JWT）                │
│                                                                      │
│  user_platform_roles ──▶ platform_roles ──▶ platform_role_permissions │
│                            │                                          │
│                            └── grants_all=true → 全集（super-admin）    │
│                                                                      │
│  effective_permissions(u) = ⋃ permissions(roles(u))                  │
└──────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌──────────────────────────────────────────────────────────────────────┐
│  领域边界（不变）                                                     │
│  event_users / event_teams / event_team_members → 赛事参与关系        │
│  AwdInternalAuth + AwdInternalPrincipal → 服务主体（非 Platform Role） │
│  EventCapabilities → 赛制能力位（非权限）                              │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.4 管理域仍在 `/api/admin`

```text
/api/*        authenticated account / player domain
/api/admin/*  permission-protected platform management domain
```

**统一用户 ≠ 合并 API。** `/api/admin` 保留，语义从「超管专属」变成
「需要对应 permission 的平台管理域」。理由见 §12。

---

## 5. Domain Boundaries

以下边界是**硬约束**，实现时不得模糊。

| 概念 | 载体 | 回答的问题 | 明确不是 |
|---|---|---|---|
| Account | `users` | 一个真实平台账户 / 登录身份 / profile | 角色 |
| Player identity | `users`（nickname/avatar/email） | 展示与画像 | 授权 |
| Event membership | `event_users`、`event_teams`、`event_team_members` | 这用户是否参加这场比赛、属于哪队、是否被赛事封禁 | 平台 RBAC |
| Platform Role | `platform_roles` | 这账户能管理 FloatCTF 的哪些平台能力 | 赛事角色、队伍角色 |
| Permission | 代码 `Permission` enum + `platform_role_permissions` | 授权的最小稳定单元 | HTTP endpoint |
| Service Principal | `AwdInternalAuth` / `AwdInternalPrincipal` | 内部服务（FlagServer / JudgeServer）的身份 | Platform Role |
| Token Purpose | `TokenPurpose::{Session, PasswordReset}` | 这枚 JWT 被签出来是干什么的 | 身份类型 |
| Event capability | `EventCapabilities` | 这个**赛制**支持哪些功能 | 账号权限 |
| Frontend capability | `PLATFORM_CAPABILITIES` | 这个**平台**实现了哪些功能 | 账号权限 |

### 5.1 命名冲突警告（必须在代码与文档中避免）

`capabilities` 一词在平台里**已被占用**两次：

1. `GET /api/frontend` 的 `capabilities` = `PLATFORM_CAPABILITIES`
   → `FloatCTFMountContext.capabilities`（静态平台能力）。
2. `EventCapabilities`（赛制能力位）。

**禁止**把「账号权限」叫成 capabilities。账号权限一律叫 **`permissions`**，
角色一律叫 **`roles`**。这条要写进 `docs/frontend/AI-FRONTEND-GUIDE.md`。

### 5.2 `AwdJudger` 的处置

`Role::AwdJudger` 是**死代码**（§2.3 已核验：无构造点）。AWD 内部认证走
`AwdInternalAuth` + 事件级密钥，与 JWT 无关。因此：

- **不**把它迁进 `TokenPurpose`（那会把死代码合法化）。
- 在 Wave 4 直接**删除** `AwdJudger` 变体与 `regression_security_concurrency.rs`
  里对应的两个测试用例（它们只断言「这个 role 进不了任何守卫」，删除变体后该保证
  由类型系统强制，更强）。
- 在 §19 安全不变式里登记为「服务主体永不进入 Platform RBAC」。

---

## 6. Database Model

### 6.1 目标 schema

```text
users ──< user_platform_roles >── platform_roles ──< platform_role_permissions
  ▲                                     ▲
  │ granted_by (self FK)                │ protected / grants_all
  │
  └── token_version, status（新增列）
```

### 6.2 `users`（新增 2 列）

```sql
-- 新增（M2）
status        account_status NOT NULL DEFAULT 'active'   -- enum: active | disabled
token_version integer        NOT NULL DEFAULT 0
```

- **`status`（新增，推荐）**：没有它就无法「禁用账号而不删账号」。当前删账号会
  `ON DELETE SET NULL` 掉审计主体（`logs` 等），破坏审计可追踪性（§19 不变式 10）。
  只有两个值，**不引入** `suspended` / `pending` / `deleted`——过度设计。
- **`token_version`（新增，推荐）**：见 §28 决策 6。今天的现实是：
  改密码后旧 token 依然有效直到 `exp`，且**没有任何 logout 端点**
  （前端 logout 只清 localStorage）。加一个 integer 的代价极小，
  换来「改密 / 禁用 / 全端吊销」的即时生效。
- **不新增**其它字段。特别地**不**在 `users` 上加 `is_admin` / `role`（见 §40）。
- `nickname` 保持 `NOT NULL UNIQUE`、`email` 保持 `UNIQUE`——这是迁移冲突的来源，见 §17。

### 6.3 `platform_roles`

```sql
CREATE TABLE IF NOT EXISTS platform_roles (
    id          uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    key         text NOT NULL,            -- 稳定标识，唯一
    name        text NOT NULL,            -- 展示名（可改）
    description text NOT NULL DEFAULT '',
    protected   boolean NOT NULL DEFAULT false,
    grants_all  boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT platform_roles_key_key UNIQUE (key),
    CONSTRAINT platform_roles_key_format CHECK (key ~ '^[a-z][a-z0-9-]{1,63}$'),
    CONSTRAINT platform_roles_key_not_empty CHECK (length(btrim(name)) > 0)
);
CREATE TRIGGER trg_platform_roles_updated_at BEFORE UPDATE ON platform_roles
  FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();   -- 复用既有函数
```

语义（本节即权威定义）：

| 字段 | 语义 | 规则 |
|---|---|---|
| `key` | 稳定标识，权限判定与审计**只用 key** | 唯一；创建后**不可改**（改名走 `name`）；格式 `^[a-z][a-z0-9-]{1,63}$` |
| `name` | 人类展示名 | 可改；`protected=true` 时也允许改（纯展示，不影响安全） |
| `description` | 说明 | 可改 |
| `protected` | **系统内置角色**标记 | `protected=true` → **禁止删除**；`key` 不可改；`grants_all` / permission 集**不可改**。语义与既有先例一致：`settings.protected`（`settings/api.rs:194-196`）、`scheduled_tasks.protected`（`scheduled_tasks.rs:416-418`）。 |
| `grants_all` | **全权**标记 | `true` → 解析时短路返回**全集**（含代码里将来新增的权限）。仅 `super-admin` 使用。 |

**为什么 `super-admin` 不该只是一堆 permission 行**：如果它是行，那么（a）运维可以从
DB 里删掉一行而让最后一个超管变砖；（b）将来新增权限时，若忘了给 super-admin 补行，
超管会**静默失去**该能力——安全工作最忌讳的失败模式。（b）尤其致命：升级期间
「新功能上线但最后管理员没有该权限」= 平台不可维护。`grants_all` 把「超管能做一切，
包括明天才存在的权限」变成**结构性保证**，而不是一份需要持续维护的数据。

自定义角色：`protected=false`，可删、可改 `key`（不，key 仍不可改，只是可删重建）、
可任意改 permission 集（受 §20 反提权约束）。

### 6.4 `user_platform_roles`

```sql
CREATE TABLE IF NOT EXISTS user_platform_roles (
    user_id    uuid NOT NULL REFERENCES users(id)          ON DELETE CASCADE,
    role_id    uuid NOT NULL REFERENCES platform_roles(id) ON DELETE CASCADE,
    granted_by uuid NULL     REFERENCES users(id)          ON DELETE SET NULL,
    granted_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX IF NOT EXISTS idx_user_platform_roles_role ON user_platform_roles (role_id);
```

| 决定 | 取值 | 理由 |
|---|---|---|
| 主键 | `(user_id, role_id)` 复合主键 | 天然去重（同一角色不重复授予），比 surrogate id + unique 更省 |
| `granted_by` FK | `users(id) ON DELETE SET NULL` | 授权人本身可能被删；审计不应因删账号而丢失（与既有「管理员主体列 SET NULL」约定一致，见 `20260914120000` 注释） |
| 删除用户 | `CASCADE` | 账号不存在则其角色授予无意义 |
| 删除角色 | `CASCADE` | 角色不存在则授予无意义；自定义角色删除是显式动作，且受 §20 约束 |
| 审计 | `granted_by` + `granted_at` 是**行内**最小审计；完整轨迹另写 `logs`（`ROLE_GRANTED` / `ROLE_REVOKED`） | §15 |

**为什么不给 `user_platform_roles` 加 surrogate `id`**：没有任何外部表引用它，
复合主键就够；加 `id` 只会多一个没人用的 UUID 与额外唯一索引。

### 6.5 `platform_role_permissions`

```sql
CREATE TABLE IF NOT EXISTS platform_role_permissions (
    role_id    uuid NOT NULL REFERENCES platform_roles(id) ON DELETE CASCADE,
    permission text NOT NULL,
    PRIMARY KEY (role_id, permission),
    CONSTRAINT platform_role_permissions_format
      CHECK (permission ~ '^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$')
);
```

### 6.6 决策：Permission 用代码 enum 还是 DB catalog 表

**推荐：代码 enum / 稳定字符串为准，DB 只存字符串（方案 B）。不建 DB permission catalog。**

| 维度 | A: DB catalog 表 + FK | **B: 代码 enum + text 列（推荐）** |
|---|---|---|
| 类型安全 | 仍然需要代码 enum 去做 `require(P)`，DB 表**不减少**代码侧维护 | 单点定义：`Permission` enum 是唯一权威 |
| 新增权限的成本 | enum 改 + **一条 INSERT migration**（每次加权限都要 migration） | enum 改 + 跑一次 `ensure_builtin_roles`（无 migration） |
| 与 `grants_all` 的关系 | catalog 行必须与 enum 同步，否则超管「全权」含义漂移 | enum 即全集，毫秒级一致 |
| orphan 风险 | 有 FK，但 orphan 表现为 catalog 与 enum 不一致（更难发现） | 写入路径全部经 enum 校验；启动时**检测**未识别字符串 |
| UI 展示（domain/名称/风险） | 从 DB 读，可能与代码不一致 | **从 enum 派生**的只读 `GET /api/admin/permissions`，与执行代码同版本 |
| 迁移成本 | 每个权限 = 一条 migration，历史 migration 不可改 → 追加型 catalog 会长得很快 | 一次性建表 |

**关键论据**：代码**必须**知道每个权限（否则无法 `require`）。catalog 表不会消除这个
需求，只会引入第二个真相源。而且 `platform_role_permissions` 里的字符串在
`grants_all` 短路下对 super-admin 根本不被读取——此时 catalog 表的存在感几乎为零。

**orphan permission 的处置**（方案 B 的代价，必须明确兜住）：

1. **写路径**：所有写 `platform_role_permissions` 的 API 都用 `Permission::from_str`
   校验，非法字符串直接 `AppError::Validation`（400）。DB CHECK 只做格式兜底。
2. **启动自检**：`ensure_builtin_roles()` 在 bootstrap 时（既有先例
   `seed_default_settings`，`bootstrap/mod.rs:135-136`）做三件事：
   - upsert `super-admin` 角色为 `protected=true, grants_all=true`（幂等）；
   - **检测** `platform_role_permissions` 中无法解析为 `Permission` 的字符串，
     记 `WARN` 日志（含 role key + 字符串），**不静默删除**；
   - 校验「内置角色的 permission 集与代码声明一致」，不一致则 `WARN`（不自动改，避免
     掩盖人为改动）。
   之所以不自动删：删除权限是安全相关动作，静默做等于「升级后某个角色悄悄少了权限」。
   要清理就发一条**显式**的新 migration 或走 API。
3. 前端 `GET /api/admin/permissions` 只返回 enum 派生项，因此 UI 不会展示 orphan。

### 6.7 生成实体

`platform_roles` / `user_platform_roles` / `platform_role_permissions` 与 `users` 的新列
**全部由 `mise run db:gen` 生成**（见 §22 铁律），不手写、不手改。

---

## 7. Permission Catalog（第一版，29 项）

### 7.1 设计规则

- 粒度：**域级**，不是 endpoint 级。多个 endpoint 共用一个权限是**期望**行为。
- 无 deny 模型；无 user-level direct override；多角色取**并集**。
- 命名：`<domain>.<action>`，全小写 `[a-z0-9.]`，至少一段点。
- 目标区间 20–40，本版 **29**。

```text
effective_permissions(user) = ⋃ { permissions(r) | r ∈ roles(user) }
                               ∪ (∃ r ∈ roles(user).grants_all ? ALL : ∅)
```

### 7.2 完整表

| # | Permission | 含义 | 代表端点（详见 [platform-permissions.md](./platform-permissions.md)） | 建议模板角色 |
|---|---|---|---|---|
| 1 | `users.read` | 查看账号列表/详情/其角色 | `GET /api/admin/users[/{id}]` | user-manager, auditor |
| 2 | `users.manage` | 建/改/删/禁用账号 | `POST/PATCH/DELETE /api/admin/users*` | user-manager |
| 3 | `roles.read` | 查看角色目录与权限目录 | `GET /api/admin/roles*`, `GET /api/admin/permissions` | user-manager, auditor |
| 4 | `roles.manage` | 增删改角色 + 授予/撤销角色给账号 | `POST/PATCH/DELETE /api/admin/roles*`, `PUT /api/admin/users/{id}/roles` | super-admin |
| 5 | `challenges.read` | 查看题目/题集 | `GET /api/admin/challenges*`, `GET /api/admin/challenge_sets*` | challenge-manager, auditor |
| 6 | `challenges.manage` | 题目/题集增删改 + 导入/构建/扫描/检查 | `POST/PATCH/DELETE /api/admin/challenges*`, `.../challenge_sets*` | challenge-manager |
| 7 | `gameboxes.read` | 查看 GameBox 库 | `GET /api/admin/awd/gameboxes*` | challenge-manager, ops-manager |
| 8 | `gameboxes.manage` | GameBox 导入/构建/扫描/检查/改/隐藏/删 | `POST/PATCH/DELETE /api/admin/awd/gameboxes*` | challenge-manager |
| 9 | `weapons.read` | 查看武器库 | `GET /api/admin/weapons` | ops-manager |
| 10 | `weapons.manage` | 武器增删改/上传 | `POST/PATCH/DELETE /api/admin/weapons*` | ops-manager |
| 11 | `events.read` | 查看赛事、数据、报表、参与记录、日志、Writeup、**私有制品下载** | `GET /api/admin/events*`（含 `/data` `/report` `/instances` `/logs` `/writeups` `/users` `/teams`）、`GET /api/admin/download`（对象级授权，见 §7.6） | event-manager, auditor |
| 12 | `events.manage` | 赛事倒角 + 赛题挂载 + 赛事公告 | `POST/PATCH/DELETE /api/admin/events*`, `.../challenges*`, `.../announcements*` | event-manager |
| 13 | `events.participants.manage` | 参赛用户/队伍增删 + 赛事级封禁 | `.../users*`, `.../teams*` | event-manager |
| 14 | `events.lifecycle.manage` | **推动比赛**：开始/暂停/恢复/结束/归档/部署/预检/重置靶机/调分/轮换令牌/网络生效/AWD 封队 | `.../awd/{start,pause,resume,finish,archive,deploy,precheck}`, `.../awd/{teams/*/ban,score/adjust,tokens/rotate,gameboxes/*/reset,network*}`, `.../awdp/{start,finish,break-to-fix}` | event-manager, ops-manager |
| 15 | `awd.read` | 查看 AWD 配置/预检/判题批次/分数/网络 | `GET /api/admin/events/{id}/awd*`, `GET /api/admin/awd/network*` | event-manager, ops-manager, auditor |
| 16 | `awd.manage` | 配置 AWD 赛事 + 赛事 GameBox 挂载 | `PATCH/POST /api/admin/events/{id}/awd`, `.../awd/gameboxes*` | event-manager |
| 17 | `awdp.read` | 查看 AWDP 配置/run/实例/分数/数据 | `GET /api/admin/events/{id}/awdp*` | event-manager, ops-manager, auditor |
| 18 | `awdp.manage` | 配置 AWDP 赛事 + GameBox 挂载 | `PATCH /api/admin/events/{id}/awdp`, `.../awdp/gameboxes*` | event-manager |
| 19 | `announcements.manage` | 平台公告增删改查 | `/api/admin/announcements*` | event-manager |
| 20 | `discussions.moderate` | 讨论/评论审核与删除 | `/api/admin/discussions*` | event-manager |
| 21 | `settings.read` | 查看平台设置 | `GET /api/admin/settings` | auditor, ops-manager |
| 22 | `settings.manage` | **高危** 增删改平台设置（含 `FRONTEND_ACTIVE`） | `POST/PATCH/DELETE /api/admin/settings*` | super-admin |
| 23 | `system.read` | 仪表盘 / 系统监控 / 版本 | `GET /api/admin/dashboard/summary`, `/system/monitor`, `/system/version` | 全部管理模板角色 |
| 24 | `logs.read` | 查看平台与赛事审计日志 | `GET /api/admin/logs[/{id}]`, `GET /api/admin/events/{id}/logs` | auditor |
| 25 | `scheduler.manage` | 计划任务增删改/手动运行 | `/api/admin/scheduled_tasks*` | ops-manager |
| 26 | `docker.read` | 查看容器/镜像/网络 | `GET /api/admin/docker/{containers,images,networks}` | ops-manager |
| 27 | `docker.manage` | **高危** 容器启停删 / 镜像删 / 网络增删 | `POST/DELETE /api/admin/docker/*` | ops-manager |
| 28 | `database.execute` | **高危** 执行任意 SQL | `POST /api/admin/database/exec_sql` | super-admin |
| 29 | `terminal.open` | **高危** 打开宿主机 Web 终端 | `POST /api/admin/terminal/session`, `GET /api/admin/terminal/ws` | ops-manager |

> **本列语义（重要）**：「建议模板角色」= **默认随模板一起授予**，不是「谁被允许拥有」。
> `docker.manage` / `terminal.open` 默认进 `ops-manager` 模板；`database.execute` /
> `settings.manage` / `roles.manage` 默认**只**在 `super-admin`（`grants_all`）。
> **「默认模板不给」≠「禁止自定义角色获得」**——自定义角色仍可获得任意高危权限，
> 只是必须受 §20.2 反提权规则与 §15 审计约束（见 §7.4.3 与 §8.2）。

### 7.3 被**刻意排除**的权限（附依据）

| 候选 | 为什么不建 | 依据 |
|---|---|---|
| `frontends.read` / `frontends.manage` | **平台没有前端管理 API。** 切换前端完全通过通用 `settings` 的 `FRONTEND_ACTIVE` 键完成（`platform/frontend/api.rs:10,18`；`settings/api.rs:8,21`），前端侧的注册表读取由浏览器直接 fetch。因此前端管理已由 `settings.read` / `settings.manage` 覆盖。 | 源码核验：`platform/frontend/api.rs` 只有 `#[get("")] get_frontend_bootstrap` 一个 handler，无 admin 路由 |
| `announcements.read` | 公告**内容本身是公开的**（`GET /api/announcements` 只要登录即可读），管理端 `/api/admin/announcements` 是管理界面，用 `announcements.manage` 一次覆盖读+写。 | `platform/mod.rs:18-28,36-47` |
| `events.lifecycle.manage` 拆分（如 `awd.scores.manage`） | 见 §28 决策 9 的取舍说明；第一版保持稳定，若真实部署需要职责分离再追加 | — |
| `users.roles.manage`（与 `roles.manage` 分离） | 见 §20：`roles.manage` 已受「不可授予超出自身有效权限」硬约束，拆成两个权限只是把一个能力拆成两步，收益低于多一个权限的长期维护成本 | §20 |
| `event_teams.*` / `event_users.*` 细分 | 参与关系整体归 `events.participants.manage`，避免按资源类型爆炸 | §5 边界 |

### 7.4 高危权限（§25）

```text
terminal.open      ← 宿主机 shell
database.execute   ← 任意 SQL
docker.manage      ← 容器/网络生命周期
settings.manage    ← 含 FRONTEND_ACTIVE、JWT 相关动态设置
roles.manage       ← 可定义权限集合
```

要求（详见 §20、§25）：

1. 前端在 Roles UI 中对这 5 项加 **danger 标注**（Default 用 Primer `Label variant="danger"`
   + `Tooltip`），并在提交前用 `Dialog` 二次确认。
2. 授予/撤销这些权限**必须**写审计（`ROLE_PERMISSIONS_CHANGED`，见 §15）。
3. **允许**自定义角色获得这些权限——**不**自动限制为 super-admin。依据：
   - 平台已把「宿主机特权」收敛到 helper（AGENTS.md 铁律 6），`terminal.open` /
     `docker.manage` 的效果由 helper 的受限协议与策略兜底，不是靠「只有超管能点」；
   - `database.execute` 走的是平台自己的 DB 连接（不是宿主特权），其风险等价于
     「能改平台数据」，而 `settings.manage` / `roles.manage` 也在同一量级；
   - 现实中存在合法的窄角色（例如「只给 DBA 的 `database.execute`」「只给值班同学的
     `terminal.open`」），禁掉会迫使运维共用超管账号，**降低**可审计性；
   - 真正的防线是 §20 的反提权规则 + §15 的审计 + Wave 0 的测试覆盖，而不是硬编码。

### 7.5 权限数量结论

**29 项**（区间目标 20–40）。按域分布：Identity 4、Content 6、Events 8、Community 2、
Platform 6（settings/system/logs/scheduler）、Infra 3（docker×2 + database）、Terminal 1。
`AWD`/`AWDP` 各 2 项、`Events` 4 项是刻意保持「一个能力一个权限」而非按赛制×动作爆炸。

### 7.6 `/api/admin/download` 的对象级授权（**Wave 0 必修安全项**）

**现状缺陷（Wave 0 修复前）**：`GET /api/admin/download?key=<任意字符串>`
直接把 `key` 交给 `presign_private_download_url()` 去签 `floatctf-private` 桶的
预签名 URL（`platform/files/download.rs:88-121`），**没有任何归属校验**。
即：任何能通过该端点守卫的调用者可以读取私有桶里的**任意对象**。

**私有桶 key 空间核验（源码推导，决定授权模型）**：
遍历 `floatctf-private` 的全部写入点后，可达该端点的 key **只有两族**：

| # | Key 形态 | 产生位置 | 是否存在 DB 锚点 |
|---|---|---|---|
| A | `writeups/{file_name}` | 选手提交 Writeup（`jeopardy/api/submit.rs:218-250` 写入 `event_writeup.file_url`） | **是**：`event_writeup.file_url` 行 |
| B | `writeups/{event_id}/{safe_title}_{event_id}.zip` | 管理员导出报告（`event/common/application/admin_service.rs:857-867`） | 否（服务器生成，`event_id` 内嵌于 key） |

其它私有桶内容（AWDP `source.tar.gz`）位于
`gameboxes/{gamebox_id}/awdp/{digest}/source.tar.gz`（`gamebox/source_artifact.rs:51`），
走**另一条** DB 锚定的取键路径（`awdp/api/player.rs:508`、`awdp/api/training.rs:636`
从 `gamebox.awdp_source_artifact_key` 取键），**不经**该端点。

**最终授权模型（不做 prefix-only 检查）**：核心是 **`key → 解析出 event_id → 对 event 授权**。
伪造 key 字符串只能「命名」一个资源，不能绕过对该资源的授权判定。

```text
resolve_private_artifact(key) -> Option<Uuid /* event_id */>
  0. 形状拒绝（任一不满足即 None）：
       - 空串 / 长度 > 512
       - 以 '/' 开头 / 含 '\\' / 含 ".." / 含 "//"
  1. 族 A（DB 锚定，不可伪造）：
       SELECT event_id FROM event_writeup WHERE file_url = $key LIMIT 1
       → 命中则返回该 event_id
  2. 族 B（服务器生成格式 + 存在性 + 对解析结果授权）：
       正则 ^writeups/(?P<ev>[0-9a-fA-F-]{36})/[^/]+$
       且 ev 可解析为 UUID
       且 events 表存在该 id
       → 返回 ev
  3. 其余 → None
```

调用链：

```text
key
  → resolve_private_artifact              // 无锚点 → 404 NotFound（不泄露桶结构）
  → 已由 RequirePermission<EventsRead> 静态保证 events.read
  → presign_private_download_url
```

**权限归属修正**：计划早期版本把该端点映射为 `challenges.read`。按源码复核，
该端点的唯一消费方是**赛事 Writeup 页**（Default 三个赛制的 `writeups.tsx` 与
shadcn `tab-records.tsx`；xzmu 不使用该端点），而授权锚点是 **event**——
`event_writeup` 的主键是 `(event_id, user_id)`，管理端列表端点是
`GET /api/admin/events/{id}/writeups`（已由 `events.read` 保护）。因此正确权限是
**`events.read`**，不是 `challenges.read`：后者会让「题库管理员」读到**所有赛事**的
写题记录，属于超出最小权限。

**为什么这不是「prefix-only 检查」**：

- 族 A 的判定依据是**数据库主键检索**，字符串前缀不参与授权。
- 族 B 的判定依据是**解析出的 `event_id` 再去查 `events` 表存在性**，
  并把授权决策建立在**解析出的 event** 上。攻击者可以构造一个指向他人赛事的 key，
  但随后仍然要通过该赛事的 `events.read` —— 这正是应有的门槛。

**兼容性**：无需 API 形状变更（仍收 `?key=`），无需改 SDK / 三个前端——
因为两族合法 key 都照常通过。若未来出现第三族私有制品，
**必须**先为它建立 DB 锚点或显式登记，再扩展 `resolve_private_artifact`；
**禁止**为了省事放宽成 `starts_with("writeups/")`。

**必须的测试**（Wave 0 Gate）：

```text
族 A 合法 key（DB 有行）                    → 2xx
族 B 合法 key（服务器生成的报告 key）        → 2xx
无关对象键（如 gameboxes/.../source.tar.gz）→ 404
路径穿越形态（../、..%2f、/etc/passwd）      → 404
未知 key（无 DB 行且不匹配族 B 形状）        → 404
随机 UUID 作为族 B 的 ev（events 无此行）    → 404
普通用户 token                              → 403（无 events.read）
events.read 持有者                           → 仅能读其 events.read 覆盖的赛事制品
```

---

## 8. Built-in Roles

### 8.1 只内置 `super-admin`

| key | name | protected | grants_all | permissions |
|---|---|---|---|---|
| `super-admin` | Super Admin | `true` | `true` | （空，短路为全集） |

由 migration **M3** 幂等插入（固定 UUID，见 §13.3）。它是**引导不变式**：
平台必须永远存在至少一个能做任何事的账号，否则不可恢复。

**为什么只内置它**：内置角色 `protected=true` 意味着**不可删、权限集不可改**，
这是永久的兼容义务。另外 5 个候选角色（event-manager / challenge-manager /
user-manager / ops-manager / auditor）编码的是**组织分工**，每个部署都不一样：

- 硬编码会造成「想改个名字都改不了」/「想多给一项权限只能新建角色」的摩擦；
- 我们无法基于真实使用验证它们的边界，过早固化等于把猜测写进不可变数据；
- 它们对开箱体验的价值可以用**模板**（可一键创建、可自由改）等价获得，且没有
  不可变的代价。

因此：**5 个模板角色以「推荐角色模板」形式提供**（文档 + Roles UI 的
「从模板创建」按钮），落到 DB 后是普通 `protected=false` 角色，可随意调整。
E2E 场景（§21.6）通过 API 创建它们，而不是依赖内置数据。

### 8.2 推荐角色模板（非内置）

| 模板 key | 权限集合 | 用途 |
|---|---|---|
| `event-manager` | `events.read` `events.manage` `events.participants.manage` `events.lifecycle.manage` `awd.read` `awd.manage` `awdp.read` `awdp.manage` `announcements.manage` `discussions.moderate` `challenges.read` `gameboxes.read` `gameboxes.manage` `system.read` | 办比赛：建赛、挂题、管参与、开赛/收赛、配 AWD/AWDP |
| `challenge-manager` | `challenges.read` `challenges.manage` `gameboxes.read` `gameboxes.manage` `events.read` `system.read` | 只做题库与 GameBox，不碰赛事时钟、不碰基础设施 |
| `user-manager` | `users.read` `users.manage` `roles.read` `system.read` | 账号生命周期；**无** `roles.manage`（不能改权限） |
| `ops-manager` | `docker.read` `docker.manage` `terminal.open` `scheduler.manage` `settings.read` `logs.read` `system.read` `events.read` `events.lifecycle.manage` `awd.read` `awdp.read` | 值班运维：容器、终端、计划任务、看日志 |
| `auditor` | `logs.read` `events.read` `awd.read` `awdp.read` `challenges.read` `users.read` `roles.read` `settings.read` `system.read` | 只读审计；**全部为 `.read`**，天然无写入 |

**验证性检查**（应作为 Roles UI 的模板自检，也是 §21.6 E2E 断言）：
- `event-manager` ∌ `docker.*` / `database.execute` / `terminal.open`
- `auditor` 的所有权限都以 `.read` 结尾
- `user-manager` ∌ `roles.manage`

---

## 9. Authentication Model

### 9.1 JWT 重构

```rust
// 目标（唯一权威形态：只有两个变体，不留未来占位）
pub enum TokenPurpose {
    Session,
    PasswordReset,
}

pub struct AuthClaims {
    pub sub: Uuid,            // users.id（唯一人类账户表）
    pub purpose: TokenPurpose,
    pub token_version: u32,   // 仅 Session 校验
    pub exp: usize,
}
```

> **注意**：上面的目标代码块**刻意不含**任何 `AwdJudge` / `AwdJudger` 变体。
> 若将来真的需要「被 JWT 承载的服务主体」，它应当作为**新的**
> `TokenPurpose` 变体引入（而不是 Platform Role），但**本版不实现、也不预留占位**——
> 预留一个无构造点的变体正是当前 `Role::AwdJudger` 变成死代码的原因。

**迁移映射**：

| 旧 `Role` | 新 `purpose` | 处置 |
|---|---|---|
| `User` | `Session` | 直接 |
| `SuperAdmin` | `Session` | 直接（不再有 admin 专用 token） |
| `ResetAccount` | `PasswordReset` | 直接（名副其实） |
| `AwdJudger` | — | **删除，不迁移**（全仓无构造点的死代码；AWD 服务认证走 `AwdInternalAuth`） |

**`token_version` 在非 Session 用途上**：仍然序列化（统一结构，避免 enum-tagged
payload），但只对 `Session` 校验。文档与代码注释要写明这一点。

### 9.2 为什么 roles / permissions **不进 JWT**

| 理由 | 说明 |
|---|---|
| 撤权即时性 | 进了 JWT 就要等 `exp`（现在默认 8 小时，`jwt.rs:58`）才生效，与 §19 不变式 4「权限变更下一次请求生效」直接冲突 |
| stale permission | 角色/权限变更是**运行时**操作；JWT 是**签发时**快照，二者必然漂移 |
| token 体积 | 29 项权限 + 角色列表进 claim，每个请求都要带 |
| 无法表达 `grants_all` | 「所有权限，含未来新增」无法在签发时枚举成列表而不漂移 |
| 双真相源 | 一旦 claim 里有权限，就必须回答「以 claim 为准还是以 DB 为准」，答案永远是以 DB 为准 → claim 里的那份只制造 bug |

因此 JWT 只回答**身份**（`sub` + `purpose` + `token_version`），授权**每请求**从 DB 解析。

### 9.3 单一 Session

```
POST /api/session        ← 唯一登录入口（新）
POST /api/admin/session  ← Wave 4–9 过渡别名，Wave 10 删除
POST /api/users/session  ← Wave 4–9 过渡别名，Wave 10 删除
```

请求体不变：`{ username, password }`。响应：`UniResponse<String>`，`data` 仍是裸 JWT。

登录逻辑（统一）：

1. 按 `username` 在 `users` 里查。
2. argon2 校验（保持 `Argon2::default()`）。
3. `status == disabled` → `AppError::Forbidden("账号已被禁用")`（**403，不是 401**）。
4. 签发 `gen_session_token(user.id, user.token_version)`。
5. 成功/失败都写审计（保持既有 `AUTH`/`LOGIN` 行为与中文文案）。

**关于哪些人能登录管理后台**：登录本身**不做**角色判断。任何 `active` 账号都能拿到
Session token；「能不能进管理后台」由 `/me` 返回的 `permissions` 与后端
permission guard 决定。这样「普通用户被授予 event-manager」无需第二次登录（§44 Case A）。

### 9.4 `token_version` 语义（§28）

```rust
pub struct AccountGuard(users::Model);

// AccountGuard 内部
let claims = validate_jwt(token)?;
if claims.purpose != TokenPurpose::Session { return Err(unauthorized()); }
let user = users::find_by_id(claims.sub).one(db).await?.ok_or(unauthorized())?;
if user.status == AccountStatus::Disabled { return Err(AppError::Forbidden(...)); }
if user.token_version != claims.token_version { return Err(unauthorized()); }
```

`token_version` **自增的时机**（穷举，实现时必须只有这几处）：

| 时机 | 动作 |
|---|---|
| 用户自己改密码（`PATCH /api/users/me` 带 `password`） | `token_version += 1`（旧会话全部失效） |
| 管理员重置某用户密码（`PATCH /api/admin/users/{id}` 带 `password`） | `+= 1` |
| 邮件重置密码（`POST /api/users/reset`） | `+= 1` |
| 管理员禁用账号（`status → disabled`） | `+= 1`（即使将来 `status` 检查被绕过，旧 token 也已失效） |
| 安全事件「吊销该账号全部会话」 | `+= 1` |

**不做**独立的「会话表 / 登录记录表」。理由：`token_version` 已覆盖「全端吊销」；
逐会话吊销（列出设备、踢掉某一个）是产品功能，不在本版范围。明确记入 §27 备查。

**与 RBAC revision 的关系**：**不复用同一个字段**。`token_version` 管**凭据**，
权限变更走 DB 实时解析（无 revision，因为不缓存）。把两者合并会在未来引入权限缓存时
立刻产生语义纠缠。见 §27。

### 9.5 `/me` 契约（§15）

```
GET /api/users/me      ← 保留（Wave 4 起返回新 DTO）
GET /api/me            ← 新增别名（可选，见 §12 决策）
```

响应（新 `MeDto`，**绝不含 password**）：

```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "username": "sysadmin",
  "nickname": "sysadmin",
  "email": "sysadmin@system.com",
  "avatar": null,
  "status": "active",
  "roles": [
    { "key": "super-admin", "name": "Super Admin" }
  ],
  "permissions": [
    "users.read", "users.manage", "roles.read", "roles.manage",
    "challenges.read", "challenges.manage", "gameboxes.read", "gameboxes.manage",
    "weapons.read", "weapons.manage", "events.read", "events.manage",
    "events.participants.manage", "events.lifecycle.manage",
    "awd.read", "awd.manage", "awdp.read", "awdp.manage",
    "announcements.manage", "discussions.moderate",
    "settings.read", "settings.manage", "system.read", "logs.read",
    "scheduler.manage", "docker.read", "docker.manage",
    "database.execute", "terminal.open"
  ]
}
```

| 决定 | 取值 | 理由 |
|---|---|---|
| `roles` 完整返回？ | 是，作为 `{key, name}` 数组 | UI 需要展示角色**名字**；只返回 key 会逼出第二次请求。`{key,name}` 比完整 role 对象小得多，且不带 permissions（避免同一份数据两处出现） |
| `permissions` 完整返回？ | 是，**完全展开** | 直接喂给既有的 `NavigationVisibilityContext.permissions: ReadonlySet<string>`；前端无需实现通配符语义 |
| super-admin 是否展开全部？ | **是**，展开为全部 29 项 | 若返回 `"*"` 哨兵，每个消费者都要实现通配逻辑（三个前端 × 路由守卫 × 按钮 gating），是 bug 温床。29 个短字符串约 500 B，可接受 |
| 是否另有 `capabilities` 端点？ | **不建** | 「谁能不能做什么」= `permissions`；「平台支不支持某功能」= `GET /api/frontend` 的 `capabilities`（未认证，已存在）。第三个概念只会制造 §5.1 的命名冲突 |
| 是否适合前端导航？ | 是 | 即 §2.8 那个已存在但空转的接缝 |

**缓存语义**：`GET /api/users/me` 必须**不可缓存**（`Cache-Control: no-store`），
因为它是「当前权限」的读点。前端 React Query 侧用 `staleTime: 0`。

---

## 10. Authorization Model

### 10.1 解析

```rust
pub struct AuthContext {
    pub user: users::Model,
    pub roles: Vec<PlatformRoleRef>,        // {id, key, name, grants_all}
    pub permissions: PermissionSet,          // HashSet<Permission> 或 bitset
}

impl AuthContext {
    pub fn has(&self, p: Permission) -> bool {
        self.roles.iter().any(|r| r.grants_all) || self.permissions.contains(&p)
    }
}
```

解析顺序（**单次查询预算**）：

```text
1. users.find_by_id(sub)                                  -- AccountGuard 已取
2. SELECT r.id, r.key, r.name, r.grants_all
     FROM user_platform_roles upr JOIN platform_roles r ON r.id = upr.role_id
    WHERE upr.user_id = $1
3. 若任一 grants_all → permissions = Permission::ALL  (短路，不再查)
   否则 SELECT permission FROM platform_role_permissions
          WHERE role_id = ANY($role_ids)
```

**2 + 3 = 最多两条 SQL**（正常情况 2 条，super-admin 1 条）。不引入 N+1。

### 10.2 多角色取并集

```text
user 拥有 event-manager + challenge-manager
  → effective = permissions(event-manager) ∪ permissions(challenge-manager)
```

无「拒绝」语义、无优先级、无冲突解决——并集是唯一的组合运算（§44 Case C）。

### 10.3 权限缓存（§27）

**第一版：不做任何缓存。每个管理请求查 DB。**

| 选项 | 评估 |
|---|---|
| A: 每请求查 DB（**推荐 v1**） | 正确性最高；撤权**下一次请求**立即生效（§19 不变式 4）。代价是最多 2 条按主键/索引的窄查询，相对 handler 自身的业务查询（多数是重查询 + 可能的 Docker/helper 调用）可忽略 |
| B: Redis effective-permission 缓存 | 需要引入 `user permission revision`、角色更新失效、授予变更失效、缓存与 DB 的一致性窗口。当前阶段**没有**证据表明权限解析是瓶颈 |

**推荐明确的触发条件**（不是「以后再说」，而是可测量的门槛）：当
`pg_stat_statements` 显示角色解析查询进入 P95 延迟前 5，或管理端 QPS 使
权限解析占请求总耗时 > 5% 时，再引入缓存。届时**必须**同时实现：

1. `platform_roles.updated_at` / `user_platform_roles` 变更为失效信号；
2. `user_platform_roles` 变更与角色 permission 变更都显式 `DEL` 相关用户缓存；
3. 缓存 TTL ≤ 30s 作为兜底（防止漏失效导致永久错误权限）；
4. 一条测试证明「撤销角色后，即使缓存层存在，下一个请求也返回 403」。

**明确拒绝**「因为性能而牺牲撤权即时性」的做法。

---

## 11. Guard Design

### 11.1 现状

```rust
pub struct UserJwtGuard(users::Model);
pub struct SuperAdminJwtGuard(super_admin::Model);
```

两者都是 `FromRequest`，各自 `find_by_id(claims.sub)`（`api/extractor/auth.rs:26-103`）。

### 11.2 方案比较

**方案 A：`actor.require(Permission::X)?`（handler 内显式调用）**

```rust
#[post("/settings")]
pub async fn create_setting(actor: AccountGuard, ...) -> UniResult<...> {
    actor.require(Permission::SettingsManage)?;   // ← 可以忘
    ...
}
```

**方案 B：声明式 typed guard（extractor 承载权限）**

```rust
#[post("/settings")]
pub async fn create_setting(
    actor: Actor<PermSettingsManage>,   // ← 类型即权限
    ...
) -> UniResult<...>
```

或等价地：`actor: RequirePermission<SettingsManage>`。

| 维度 | A: 显式 require | **B: 声明式 typed guard（推荐）** |
|---|---|---|
| handler 忘调用 | **会**——新增 handler 忘写一行，端点就变「登录即可用」，且编译器不报错 | **不会**——权限在类型里，忘记写参数就没有 `AuthContext`，而 handler 需要 actor 写审计 |
| 可测试性 | 需要跑 DB seed 才能验证「这个 handler 检查了权限」 | 可写「路由→权限」表驱动测试；权限与 handler 的绑定是**签名**的一部分，可静态审查 |
| Actix 人体工学 | 最自然 | `FromRequest` 实现泛型 extractor，Actix 支持良好（既有 `SuperAdminJwtGuard` 就是 `FromRequest`） |
| 审计可读性 | 要读 handler 体 | **路由装配处一眼可见**：`scope("/settings").service(create_setting.requires(Permission::SettingsManage))` 或 handler 签名 |
| 代码重复 | 无 | 每个权限一个零大小类型 → 用宏生成（`define_permission_guard!`） |
| 宏是否值得 | — | **值得**：29 个权限 × 每个约 6 行样板 = 174 行纯样板；宏把它变成 29 行声明，且保证 29 个类型与 `Permission` enum **一一对应**（宏同时生成 enum 与 guard，从结构上消灭「有权限没 guard」） |

**最终推荐：方案 B（声明式 typed guard），用 `define_permission_guards!` 宏从
`Permission` enum 一次性生成 guard 类型。**

```rust
// 单一真相源：宏同时产出 enum 与 guard 类型
define_permissions! {
    UsersRead        => "users.read",
    UsersManage      => "users.manage",
    ...
    TerminalOpen     => "terminal.open",
}
```

同时保留 `AuthContext::has()` 作为**运行时二次判断**手段（例如 handler 内部需要
「有 A 或 B」的复杂条件），但它是补充，不是主防线。

### 11.3 目标类型

```rust
/// 认证 + 账户状态 + token_version
pub struct AccountGuard(pub AuthContext);          // 替换 UserJwtGuard / SuperAdminJwtGuard

/// 声明式权限守卫（宏生成，每个权限一个）
pub struct RequirePermission<P: PermissionMarker>(pub AuthContext);

/// 复合条件（少量、显式）
pub struct RequireAny<P1, P2>(pub AuthContext);
```

`AccountGuard` 的失败语义（必须精确，见 §19/§26）：

| 情况 | 结果 | HTTP |
|---|---|---|
| 无 Authorization 头 | `AppError::Unauthorized` | 401 |
| token 格式/签名无效 / `exp` 过期 | `AppError::Unauthorized` | 401 |
| `purpose != Session` | `AppError::Unauthorized` | 401 |
| `sub` 在 `users` 里不存在（已删账号） | `AppError::Unauthorized` | 401 |
| `token_version` 不匹配 | `AppError::Unauthorized` | 401 |
| `status == disabled` | `AppError::Forbidden("账号已被禁用")` | **403** |
| 已认证但无所需权限 | `AppError::Forbidden("没有权限执行该操作")` | **403** |

**「禁用」用 403 而不是 401 的理由**：401 会触发前端「清 token → 跳登录」，
但用户重新登录仍会被拒（403），形成无效循环并丢失原因；403 让前端能展示明确的
「账号已被禁用」提示。这与「凭据失效」（401，应重新登录）是不同语义。

### 11.4 过渡期守卫（Wave 4）

Wave 4 引入 `AccountGuard` 时，**管理路由尚不能只靠「已认证」**（否则任何玩家可访问
`/api/admin/**`）。因此 Wave 4 提供临时守卫：

```rust
/// Wave 4 过渡：已认证 且 持有 super-admin 角色
pub struct RequireSuperAdmin(pub AuthContext);   // = AccountGuard + roles 含 grants_all
```

它**精确保持**当前「任何超管可以做任何事」的语义，只是把身份来源从
`super_admin` 表换成 `users` + `super-admin` 角色。Wave 5 再逐路由替换成
`RequirePermission<P>`。**Wave 4 结束时管理端授权语义必须与 Wave 3 完全等价**——
这是该 Wave 的核心 Gate。

---

## 12. API Contract

### 12.1 命名空间

```text
/api/*        authenticated account / player domain
/api/admin/*  permission-protected platform management domain
```

**保留 `/api/admin`（明确选择）。** 理由：

1. `/api/admin` 是**授权域**边界，不是**身份域**边界。统一账号不改变「平台管理端点
   与选手端点风险等级不同」这一事实。
2. 三个前端与 SDK 的领域 API 已经按 `service` / `admin` 分层
   （`client.service.*` / `client.admin.*`），base URL 分离是既有公共契约；
   合并 URL 会放大破坏面（§16）而没有收益。
3. SDK 的 `client.awdp.runs` 已经证明「base URL ≠ 授权域」
   （它走 service transport 但路径是 `/service/...`），保留两个前缀不与权限模型矛盾。

### 12.2 会话端点

| 端点 | 处置 | 时间 |
|---|---|---|
| `POST /api/session` | **新增**，唯一登录入口 | Wave 4 |
| `POST /api/users/session` | 过渡别名（同 handler） | Wave 4 引入→**Wave 10 删除** |
| `POST /api/admin/session` | 过渡别名（同 handler） | Wave 4 引入→**Wave 10 删除** |

**是否统一 `/api/users/session` 为 `/api/session`：是。** 理由：登录不属于
`/users` 资源子路径（它不是「用户的某个操作」，而是「建立会话」）；
`/api/users/session` 与 `/api/admin/session` 并存本身就是双身份模型的语言残留。

**是否删除 `/api/admin/session`：是，但不在 Wave 4。** 见 §23 的切分策略。

### 12.3 `/me`

| 端点 | 处置 |
|---|---|
| `GET /api/users/me` | **保留**（`/users/me` 语义正确：我是用户资源的一个视图） |
| `PATCH /api/users/me` | 保留 |
| `GET /api/me` | **不新增**。多一个别名只会多一份契约、多一处 drift，而 `/api/users/me` 已足够清晰。 |

### 12.4 新增的管理端点

```text
GET    /api/admin/permissions            roles.read   只读权限目录（enum 派生）
GET    /api/admin/roles                  roles.read
POST   /api/admin/roles                  roles.manage
GET    /api/admin/roles/{key}            roles.read
PATCH  /api/admin/roles/{key}            roles.manage
DELETE /api/admin/roles/{key}            roles.manage
GET    /api/admin/users/{id}/roles       users.read   查看某账号的角色
PUT    /api/admin/users/{id}/roles       roles.manage 整体替换角色集合
```

`PUT`（整体替换）而非 `POST`/`DELETE` 逐条：一次请求即一次原子变更，
审计只写一条（含 before/after），反提权校验只需做一次。

### 12.5 被删除的管理端点（Wave 10）

```text
GET    /api/admin/super_admin
GET    /api/admin/super_admin/{id}
POST   /api/admin/super_admin
POST   /api/admin/super_admin/{id}
DELETE /api/admin/super_admin
```

替代：账号用 `/api/admin/users*`（统一后它们就是同一张表）；角色用 `/api/admin/roles*`。

### 12.6 是否值得直接 breaking cutover（§23）

**推荐：Option A —— 有强制拆除 Wave 的分段切换（gated staged cutover）。**

理由（都与本仓库的真实约束有关，不是「保守起见」）：

1. **两个前端在独立仓库。** `floatctf-frontend-xzmu` /
   `floatctf-frontend-shadcn` 是 git submodule（`.gitmodules`）。submodule 指针只能
   指向**已存在**的提交，而 `scripts/release-checksums.sh` / `package-sdk-dist.sh`
   在一次发布里只产出一份 SDK tarball。因此「平台 + 三前端在同一个 commit 完成切换」
   在物理上不可执行。
2. **`API_CONTRACT_VERSION` 的语义是给已安装前端看的。** 已安装的外部前端
   （README 明示从 tarball 安装）在 v1 平台上升级到 v2 时会先看到
   `api_contract_version = "2"`。没有过渡别名，它们**立刻**全挂；有别名则能给出
   一次清晰的升级窗口。
3. **别名是有主的、会被删的。** 与「永久 compatibility wrapper」的区别在于：
   每个别名对应一个明确的 Wave 与 Gate，Wave 10 的 Gate 是「全仓 grep 零命中」，
   不通过则**阻塞发布**。

**明确写入计划**：不允许出现「长期同时维护 users + super_admin 两套登录」；
Wave 10 的 Gate 在 §24 中定义为硬门禁。

**Option B（原子切换）被否决的代价说明**：需要一次性协调 3 个前端仓库 + 1 个 SDK
tarball + 1 次平台发布，且**没有回退余地**——一旦某个外部前端未及时升级，
它在 v2 平台上完全不可用，而 SDK 已不再提供旧契约。在本仓库当前的发布工具
（单一 tarball、submodule 指针）下，这个风险不对称地集中在运维侧。

---

## 13. Migration Strategy（§22）

### 13.1 migration 硬规则（引自 `docs/agents/DATABASE.md`，不得违背）

1. **只新增 migration**：`mise run db:migration:new <名称>`；
   `apps/api/src/sql/migrations/` 下**已存在**的 48 个文件（含两条 baseline）
   **绝对禁止**修改 / 删除 / 重命名 / 重写 / squash。
2. **禁止**手改 `merged.sql`（`db:migration:merge` 重新生成）。
3. **禁止**在 migration 里写 `BEGIN` / `COMMIT` / `ROLLBACK`
   （`migrate.sh` 为每个 migration 开独立事务）。
4. **禁止**在 migration 里操作 `schema_migrations`（`validate` 会拒绝）。
5. **禁止**手写 / 手改 `apps/api/src/entity/` 与 `packages/sdk/src/entity/`：
   只能 `mise run db:gen`。`sea-orm-cli` **必须 1.1.20**。
6. **幂等**：`CREATE TABLE IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS` /
   `CREATE INDEX IF NOT EXISTS` / `ON CONFLICT DO NOTHING`，可重复执行。
7. **中文注释**：每个新表 `COMMENT ON TABLE`，每个新列 `COMMENT ON COLUMN`。
8. 外键必须带 `ON DELETE` 行为。
9. **不保留永久兼容表 / alias**。
10. `merge` 产出的 `merged.sql` 用于**生产 fresh DB** bootstrap
    （挂到 `/docker-entrypoint-initdb.d/00-init.sql`）；因此**每条 migration 都必须在
    fresh DB 上也能正确执行**（不能假设「表里已经有历史数据」）。
11. 本计划**不修改** baseline 的 sysadmin seed（`DATABASE.md:69` 明令 accepted as-is）。

### 13.2 migration 顺序与依赖

```text
M1  add-platform-rbac              ── 只建表；无数据、无行为变化
M2  identity-account-columns-and-preflight
                                   ── users.status / users.token_version
                                   ── 冲突 preflight（RAISE EXCEPTION）
M3  migrate-super-admin-to-users   ── 复制账号 + 授 super-admin 角色（幂等）
M4  repoint-admin-actor-fks-to-users
                                   ── 7 个 FK 重指向 users(id)；纯 DDL，无代码改动
M5  unify-logs-actor              ── logs.actor_user_id + 回填 + 删旧列/旧索引
M6  drop-super-admin-table        ── 删 trigger + 表（仅在代码零依赖后）
```

**依赖图**：

```text
M1 ─┬─▶ M3 ─▶ M4 ─▶ M6
M2 ─┘         ▲
              │
        M5 ───┘（M5 需要 M4 之后所有 FK 已指向 users）
```

### 13.3 M1 — `add-platform-rbac`

- 建 `platform_roles`、`user_platform_roles`、`platform_role_permissions`（DDL 见 §6.3–6.5）。
- 建 `trg_platform_roles_updated_at`（复用既有 `update_updated_at_column()`）。
- **不插入任何角色行**（`super-admin` 行由 M3 插入，与授予动作同事务，保证 FK 满足）。
- `COMMENT ON TABLE/COLUMN` 全覆盖。

### 13.4 M2 — `identity-account-columns-and-preflight`

```sql
-- 幂等枚举（PG 没有 CREATE TYPE IF NOT EXISTS，按既有约定包 DO 块）
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'account_status') THEN
    CREATE TYPE account_status AS ENUM ('active', 'disabled');
  END IF;
END $$;

ALTER TABLE users ADD COLUMN IF NOT EXISTS status account_status NOT NULL DEFAULT 'active';
ALTER TABLE users ADD COLUMN IF NOT EXISTS token_version integer NOT NULL DEFAULT 0;
COMMENT ON COLUMN users.status IS '账号状态：active=可登录，disabled=已禁用（每请求校验）';
COMMENT ON COLUMN users.token_version IS '会话版本：改密/禁用/吊销时自增，使旧 Session JWT 全部失效';
```

**冲突 preflight**（同一文件内，紧跟列新增之后）：

```sql
DO $$
DECLARE n integer; detail text;
BEGIN
  -- 1) username 冲突
  SELECT count(*), string_agg(format('super_admin.username=%L ↔ users.username=%L', s.username, u.username), '; ')
    INTO n, detail
    FROM super_admin s JOIN users u ON u.username = s.username;
  IF n > 0 THEN
    RAISE EXCEPTION 'IDENTITY_MIGRATION_PREFLIGHT: username collision (% rows): %', n, detail;
  END IF;

  -- 2) email 冲突（users.email 有唯一索引；super_admin.email 无约束，需同时查自冲突）
  SELECT count(*), string_agg(...) INTO n, detail
    FROM super_admin s JOIN users u ON u.email = s.email;
  IF n > 0 THEN RAISE EXCEPTION '... email collision with users (% rows): %', n, detail; END IF;

  SELECT count(*), string_agg(...) INTO n, detail
    FROM super_admin a JOIN super_admin b ON a.email = b.email AND a.id < b.id;
  IF n > 0 THEN RAISE EXCEPTION '... duplicate email within super_admin (% pairs): %', n, detail; END IF;

  -- 3) UUID 冲突（两个 uuid_generate_v4 空间，理论上不会，但必须断言）
  SELECT count(*), string_agg(...) INTO n, detail
    FROM super_admin s JOIN users u ON u.id = s.id;
  IF n > 0 THEN RAISE EXCEPTION '... uuid collision (% rows): %', n, detail; END IF;

  -- 4) nickname 冲突（super_admin 没有 nickname，需要合成；先看 username 直接可用否）
  SELECT count(*), string_agg(...) INTO n, detail
    FROM super_admin s JOIN users u ON u.nickname = s.username;
  IF n > 0 THEN RAISE EXCEPTION '... nickname would collide with existing users.nickname (% rows): %', n, detail; END IF;
END $$;
```

**失败时的运维流程（必须写进升级文档）**：

1. migration 在事务内 `RAISE EXCEPTION` → 该 migration 整体回滚、`schema_migrations`
   不记录、**前序已提交的 migration 保留**、平台未启动（安装器此时只起了 postgres）。
2. 异常消息**列出具体冲突行**（username/email/username-等值）。
3. 运维**人工决策**（三选一，全部是显式 SQL，绝不自动改名）：
   - 改掉冲突的**玩家**账号 username/email/nickname；
   - 改掉冲突的**管理员**账号（`UPDATE super_admin SET username = ...`）；
   - 若确认是同一人的重复账号，按 §17.5 的「合并」或「保留两侧」决策执行。
4. 重新运行 `migrate.sh apply`（幂等：只补失败的那条及其后）。
5. **绝不自动偷偷 rename**——这是本计划的一等约束。

### 13.5 M3 — `migrate-super-admin-to-users`

**先插入 `super-admin` 角色（幂等，固定 UUID）**：

```sql
-- 固定 UUID：与 core/system_ids.rs 的 PLATFORM_ROLE_SUPER_ADMIN 常量一致
INSERT INTO platform_roles (id, key, name, description, protected, grants_all)
VALUES ('00000000-0000-0000-0000-000000000000', 'super-admin', 'Super Admin',
        '平台全权角色：拥有全部权限（含将来新增的权限）。系统内置，不可删除。',
        true, true)
ON CONFLICT (id) DO NOTHING;
```

沿用 `core/system_ids.rs` 已确立的约定（「**权威在 Rust 常量**」+
`Uuid::from_u128(n)` → `00000000-…-00000000000n`；「序号按表独立分配，不同表可复用同一
n」）。注意该值与 sysadmin 的 `users.id` 数值相同但**不同表**，文件头已明文允许。
实现时在 `core/system_ids.rs` 增加：

```rust
/// 平台角色 `super-admin` 主键（`platform_roles` 表）。
pub const PLATFORM_ROLE_SUPER_ADMIN: Uuid = Uuid::from_u128(0);
/// 其语义键。
pub const PLATFORM_ROLE_SUPER_ADMIN_KEY: &str = "super-admin";
```

**再在同一事务内做 preflight 复检 + 复制**：

```sql
-- 复检（防 M2 与 M3 之间发生写入；单文件事务内 PUT 与 INSERT 之间无窗口）
DO $$ ... 与 M2 相同的四条断言 ... $$;

-- 1) 账号复制：password hash / created_at / updated_at 原样保留；nickname 合成；avatar = NULL
INSERT INTO users (id, username, nickname, password, email, created_at, updated_at, avatar, status, token_version)
SELECT s.id,
       s.username,
       btrim(s.username),            -- nickname 合成：暂用 username（preflight 已断言不与既有 nickname 冲突）
       s.password,                   -- ★ hash 原样保留，不改口令
       s.email,                      -- ★ preflight 已断言唯一
       s.created_at,                 -- ★ 时间戳保留
       s.updated_at,
       NULL,                         -- avatar 缺失 → NULL
       'active'::account_status,
       0
  FROM super_admin s
 WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.id = s.id)
ON CONFLICT (id) DO NOTHING;

-- 2) 授予 super-admin 角色（granted_by = NULL：迁移不是人类发起的授予）
INSERT INTO user_platform_roles (user_id, role_id, granted_by, granted_at)
SELECT s.id, '00000000-0000-0000-0000-000000000000', NULL, now()
  FROM super_admin s
ON CONFLICT (user_id, role_id) DO NOTHING;
```

**保留 `super_admin` 表**：M3 只复制，不删。因此 M3 之后系统行为**完全不变**
（守卫仍读 `super_admin`），这是一个可独立验证的**加法 Wave**。

**M3 的「幂等」语义必须精确理解**（实现期修正）：

| 场景 | 行为 | 说明 |
|---|---|---|
| DDL 层（M1/M2） | 可重复执行 | `CREATE/ADD COLUMN IF NOT EXISTS` + `DO` 判存 |
| M3 在**未复制**状态下重复执行 | 成功且不重复写入 | `INSERT ... WHERE NOT EXISTS` + `ON CONFLICT DO NOTHING` |
| M3 失败后重跑 | 成功 | 事务回滚使身份空间回到未合并状态，前置条件重新成立 |
| **M3 成功后重跑** | **被 preflight 拒绝**（fail closed） | 成功后 `super_admin` 与 `users` 身份空间已重叠，username/email/id 必然冲突 |

最后一行是**有意设计**：选择「失败得响亮」而不是「静默双写」。
实践中不构成问题——`migrate.sh` 以 `schema_migrations` 为门禁，**永不重跑已应用的迁移**。
该行为由 `apps/api/tests/identity_migration_contract.rs` 的可执行断言锁定。

**M3 之后的一个已知且接受的语义变化**（必须写入发布说明）：

> 统一后，原本只能从 `/api/admin/session` 登录的管理员账号，也可以从
> `/api/users/session`（→ Wave 4 起 `/api/session`）登录，并被当作**参赛者身份**使用。
> 这是统一的必然结果（§44 Case A 正是要「一个账号既参赛又管理」），
> 不构成权限提升：能登录的人本来就持有该账号的凭据。

### 13.6 M4 — `repoint-admin-actor-fks-to-users`

对 7 个 FK：先 `DROP CONSTRAINT`，再 `ADD CONSTRAINT ... REFERENCES users(id)`，
**保持原有 `ON DELETE` 行为与可空性不变**（除 `publisher_id` 见下）。数据无需转换——
`users` 已在 M3 拥有相同 UUID 的行。

```sql
-- 唯一的语义变更：publisher_id 由 NOT NULL/CASCADE 变为 NULL/SET NULL（§13.6 决策）
ALTER TABLE announcements DROP CONSTRAINT IF EXISTS announcements_publisher_id_fkey;
ALTER TABLE announcements ALTER COLUMN publisher_id DROP NOT NULL;
ALTER TABLE announcements ADD  CONSTRAINT announcements_publisher_id_fkey
  FOREIGN KEY (publisher_id) REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE awd_internal_token_rotations DROP CONSTRAINT IF EXISTS awd_internal_token_rotations_rotated_by_fkey;
ALTER TABLE awd_internal_token_rotations ADD  CONSTRAINT awd_internal_token_rotations_rotated_by_fkey
  FOREIGN KEY (rotated_by) REFERENCES users(id) ON DELETE SET NULL;

-- awd_score_events.created_by / awd_team_bans.banned_by / awd_team_bans.unbanned_by
-- awd_reset_records.requested_by_admin / logs.superadmin_id  → 同样模式
```

**`announcements.publisher_id` 的取舍（必须显式决策）**：

现状 `NOT NULL + ON DELETE CASCADE`，即「删管理员 → 其公告被一起删除」。统一后账号是
普通用户，删账号会连带删公告，风险更高（用户管理页的批量删除会静默删掉公告）。
两个选项：

**最终决定（已固化）：`publisher_id` 改为 `NULL` + `ON DELETE SET NULL`，纳入 M4。**

```sql
ALTER TABLE announcements DROP CONSTRAINT IF EXISTS announcements_publisher_id_fkey;
ALTER TABLE announcements ALTER COLUMN publisher_id DROP NOT NULL;
ALTER TABLE announcements ADD  CONSTRAINT announcements_publisher_id_fkey
  FOREIGN KEY (publisher_id) REFERENCES users(id) ON DELETE SET NULL;
```

理由：

- **删除/清理账号不应该删除平台内容**：公告是平台级内容，其生命周期不应绑定到某个
  账号的存续。统一 Account 后 `publisher_id` 表达的是**历史 actor**，不是公告 owner。
- **审计与内容历史应优先保留**：M4 之后 7 个 actor 列的删除语义**统一为 SET NULL**，
  不再有例外（此前 `announcements` 是唯一的 `CASCADE`）。

**不要再单独开一个 M7**：可空性变更与 FK 重指向属于同一件事
（「把 actor 引用从旧身份空间迁到统一身份空间」），放进 M4 一个事务里，
避免出现「FK 已指向 users 但仍是 NOT NULL CASCADE」的中间态。

**展示名快照**：`announcements.publisher text NOT NULL` **已经存在**
（baseline `:183`，发布时写入的展示名快照），因此：

- **继续使用既有的 `publisher` 列**作为展示名——**不新增** snapshot 字段。
- `publisher_id IS NULL` 时 UI/DTO 的 fallback 文案为**「已注销账号」**，
  但仍优先展示 `publisher` 快照（它保留了发布当时的用户名，信息量更大）。

**受影响面（M4 同步更新，缺一不可）**：

| 层 | 变更 |
|---|---|
| migration M4 | 上面三条 DDL |
| SeaORM entity | `announcements.rs` 的 `publisher_id: Uuid → Option<Uuid>`（**只能** `mise run db:gen`） |
| API DTO | `AnnouncementsDto.publisher_id: Option<Uuid>`（`platform/announcements/dto.rs`） |
| API handler | `create_announcement` 写 `Set(Some(publisher_id))`；`publisher_id IS NULL` 时保持 `publisher` 快照展示 |
| SDK type | `packages/sdk/src/entity/announcements.ts`（生成）+ 手写 DTO 同步 |
| Default Frontend | 公告页对 `publisher_id === null` 展示「已注销账号」+ `publisher` 快照 |
| 两个外部 Frontend | 若展示 publisher，同样处理 null（xzmu / shadcn） |
| migration tests | 「删除发布者账号 → 公告行仍存在且 `publisher_id IS NULL`、`publisher` 快照保留」（§21.3） |

### 13.7 M5 — `unify-logs-actor`

```sql
ALTER TABLE logs ADD COLUMN IF NOT EXISTS actor_user_id uuid NULL REFERENCES users(id) ON DELETE SET NULL;
COMMENT ON COLUMN logs.actor_user_id IS '操作者账号 ID（统一指向 users.id；NULL=系统/服务动作，无人类主体）';

-- 回填：108 个具体调用点全部只设其一（已脚本核验），故 COALESCE 无损
UPDATE logs SET actor_user_id = COALESCE(superadmin_id, user_id) WHERE actor_user_id IS NULL;

DROP INDEX IF EXISTS idx_sys_logs_user_op;
CREATE INDEX IF NOT EXISTS idx_logs_actor_op ON logs (actor_user_id, category, created_at DESC);

ALTER TABLE logs DROP CONSTRAINT IF EXISTS logs_superadmin_id_fkey;
ALTER TABLE logs DROP COLUMN IF EXISTS superadmin_id;
ALTER TABLE logs DROP COLUMN IF EXISTS user_id;
```

**回填正确性的前置断言**（写进 M5）：

```sql
DO $$
DECLARE n integer;
BEGIN
  SELECT count(*) INTO n FROM logs WHERE user_id IS NOT NULL AND superadmin_id IS NOT NULL;
  IF n > 0 THEN
    RAISE EXCEPTION 'LOGS_ACTOR_UNIFICATION: % rows have BOTH user_id and superadmin_id; manual decision required', n;
  END IF;
END $$;
```

（源码已核验为 0，但生产库是独立事实源，必须在迁移时再断言一次。）

**M5 与代码同 Wave**：删除 `logs.user_id` / `logs.superadmin_id` 后，109 个调用点
必须改用 `actor_user_id`。由于 `apps/api` 是单一部署单元，migration + Rust 改动
放在**同一 Wave 的同一次提交**即天然原子。

### 13.8 M6 — `drop-super-admin-table`

```sql
DROP TRIGGER IF EXISTS trg_super_admin_updated_at ON super_admin;
DROP TABLE IF EXISTS super_admin;
```

**前置证明（§39，Wave 10 的 Gate，缺一不可）**：

1. `grep -rn 'super_admin' apps/api/src --include=*.rs | grep -v '^.*//'` → 0 命中。
2. `grep -rn '/super_admin' apps/api/src --include=*.rs` → 0 命中。
3. `grep -rn 'super_admin\|SuperAdmin' packages/sdk/src --include=*.ts` → 0 命中。
4. `grep -rn 'super_admins\|super_admin' frontends/ --include=*.ts --include=*.tsx`
   （三个前端）→ 0 命中。
5. `SELECT` 全库无其它 FK 指向 `super_admin`：
   ```sql
   SELECT conname, conrelid::regclass FROM pg_constraint
    WHERE confrelid = 'public.super_admin'::regclass;   -- 必须 0 行
   ```
6. `mise run db:gen` 后 `apps/api/src/entity/super_admin.rs` 与
   `packages/sdk/src/entity/super_admin.ts` **消失**，且没有其它实体引用它。
7. `cargo check -p floatctf` + `mise run check` 全绿。

**M6 与 M4 的顺序不可交换**：M6 之前必须 M4 完成（否则 `DROP TABLE` 会因 FK 依赖失败）。
这也是为什么 M4 必须早于所有代码切换——M4 是「数据层先解耦」，代码随后。

### 13.9 为什么不是「一个超大 SQL」

| 理由 | 说明 |
|---|---|
| 失败粒度 | `migrate.sh` 每个 migration 一个事务。合并成一条 → 任何一步失败都要整体重来，且无法「只重跑第 5 步」 |
| 可独立验证 | M1（纯建表）、M2（纯加列 + preflight）、M3（纯数据复制）、M4（纯 FK）各自可单独断言，Gate 清晰 |
| 一个 migration 做一件事 | `DATABASE.md` 明文要求 |
| 与代码解耦 | M4 可在**零代码改动**下落地上线并验证，这是把大切换拆小的关键 |

---

## 14. FK Migration Inventory（§18）

从 48 个 migration 的 DDL 与生成实体双向核验（baseline `:2049,2106,2163,2184,2199,2331`；
`20260914120000:16-18`；`entity/announcements.rs`、`awd_score_events.rs`、
`awd_internal_token_rotations.rs`、`awd_team_bans.rs`（`SuperAdmin1`/`SuperAdmin2`）、
`awd_reset_records.rs`、`logs.rs`）。

| # | Table.Column | 当前 FK | 目标 FK | 可空 | ON DELETE | 数据迁移 | 迁移 |
|---|---|---|---|---|---|---|---|
| 1 | `announcements.publisher_id` | `super_admin(id)` | `users(id)` | **NOT NULL → NULL** | **CASCADE → SET NULL** | 无需（UUID 在 M3 已存在）。`publisher text` 冗余列**保留**（历史展示名快照）。**唯一一处语义变更**，见 §13.6 | M4 |
| 2 | `awd_internal_token_rotations.rotated_by` | `super_admin(id)` | `users(id)` | NULL | SET NULL → SET NULL | 无需 | M4 |
| 3 | `awd_score_events.created_by` | `super_admin(id)` | `users(id)` | NULL | SET NULL → SET NULL | 无需 | M4 |
| 4 | `awd_team_bans.banned_by` | `super_admin(id)` | `users(id)` | NULL | SET NULL → SET NULL | 无需 | M4 |
| 5 | `awd_team_bans.unbanned_by` | `super_admin(id)` | `users(id)` | NULL | SET NULL → SET NULL | 无需 | M4 |
| 6 | `awd_reset_records.requested_by_admin` | `super_admin(id)` | `users(id)` | NULL | SET NULL → SET NULL | 无需。**注意**：`CHECK awd_reset_records_requester_check CHECK (NOT (requested_by IS NOT NULL AND requested_by_admin IS NOT NULL))` 必须**保留**——统一主体后语义从「玩家 vs 管理员」变成「赛事参与主体 vs 平台操作主体」，两者仍互斥 | M4 |
| 7 | `logs.superadmin_id` | `super_admin(id)` | **删除**，并入 `logs.actor_user_id` | NULL | SET NULL | `actor_user_id = COALESCE(superadmin_id, user_id)`（已核验无损） | M5 |

**额外受影响的 schema 对象**：

| 对象 | 处置 | 迁移 |
|---|---|---|
| `idx_sys_logs_user_op ON logs(user_id, superadmin_id)` | 删除，替换为 `idx_logs_actor_op (actor_user_id, category, created_at DESC)` | M5 |
| `trg_super_admin_updated_at ON super_admin` | 随表删除 | M6 |
| `super_admin_pkey` / `super_admin_username_key` | 随表删除 | M6 |
| `awd_reset_records.requested_by` → `users(id)` | **不变**（本来就指向 users，`20260914120000` 的理由） | — |
| `users_username_key` / `users_nickname_key` / `idx_users_email` | 不变，但成为 M2 preflight 的判定依据 | — |

**关于「是否还要扫出新 FK」的方法论**：本清单通过两条独立路径交叉验证——
（a）`grep 'REFERENCES public.super_admin' migrations/*.sql`；
（b）遍历 `apps/api/src/entity/*.rs` 中所有 `belongs_to = "super::super_admin::Entity"`。
两条路径给出同一组 7 处（`awd_team_bans` 有两个），互为佐证。
**实现 M4 前必须再跑一次这两条命令**，因为 HEAD 在推进（本计划期间已前进 2 次）。

---

## 15. Audit Model

### 15.1 目标

```text
logs
  actor_user_id uuid NULL → users(id) ON DELETE SET NULL
  ...
```

- **人类账号**：`actor_user_id = users.id`。玩家动作与管理动作**同一列**。
- **系统 / 服务动作**：`actor_user_id = NULL`。语义由 `category` / `action` /
  `details` 表达（例如 `category='AWD_AUTH'`, `action='INTERNAL_AUTH_FAILED'`）。
  这是**既有语义**（今天传 `None, None` 的调用点就是这个意思），不是新增约定。
- **不引入 `actor_type`**。理由：唯一需要区分的两类主体（人类 / 系统）已由
  `NULL` vs `NOT NULL` 表达；引入 enum 会立刻面临「第三种主体是什么」的问题，
  而 AWD 内部服务主体已经有自己的身份机制（`AwdInternalPrincipal`），
  **不应该**被塞进 `users`（§19 不变式 7）。

### 15.2 为什么**不**把服务主体塞进 `users`

| 理由 | 说明 |
|---|---|
| 语义 | `users` 是「真实平台账户 / 可登录身份 / 有 profile」。FlagServer 不是账户，没有密码、不该有 nickname |
| 安全 | 若服务主体进 `users`，它就可能被授予 Platform Role，从而把「内部回调」与「平台管理」混在一条授权路径上 |
| 既有设计 | `AwdInternalAuth` + `awd_events` 事件级密钥已是成熟且隔离的机制（`awd/api/auth.rs`） |
| 需求 | 服务主体只需「被识别」，不需要「能管理」 |

### 15.3 新增审计动作（`AuditAction` enum 扩展）

```rust
RoleCreated            // 创建角色
RoleUpdated            // 改 name/description/permissions（details 带 before/after）
RoleDeleted            // 删除角色
RoleGranted            // 给账号授予角色
RoleRevoked            // 从账号撤销角色
AccountStatusChanged   // 启用/禁用账号
SessionsRevoked        // token_version 自增（改密/禁用/显式吊销）
```

`AuditAction::category()` 新增 `"IDENTITY"`。均为 `INFO` 级；「撤销最后一个 super-admin
被拒绝」记 `WARN`（该动作未发生，但值得可见）。

### 15.4 迁移到 `actor_user_id` 的调用点

- 109 处调用点中 108 处是具体调用；唯一「双主体」是 `AuditService::record` 内部转发
  （`service.rs:81`）。因此：
  - `LogService::add_log` 签名从 `(…, user_id: Option<Uuid>, superadmin_id: Option<Uuid>, request)`
    改为 `(…, actor_user_id: Option<Uuid>, request)`。
  - `AuditService::record` 从 `(…, actor_user_id, actor_superadmin_id, request)`
    改为 `(…, actor_user_id, request)`。
  - 108 处具体调用点机械替换：`Some(user.id), None` → `Some(user.id)`；
    `None, Some(admin.id)` → `Some(admin.id)`；`None, None` → `None`。
- 管理端点改成 `AccountGuard` 后，`admin.id` 就是 `users.id`，
  因此 `Some(actor.user.id)` 在统一模型下语义正确。

### 15.5 审计可读性

`logs` 的 `actor_user_id` 只存 UUID，展示需要 join `users`。为避免日志页 N+1，
`GET /api/admin/logs` 的 DTO 增加**计算字段** `actor_username: Option<String>`
（在 handler 内一次批量查询解析，**不写回生成实体**——遵循
`DATABASE.md:154`「API 计算字段放 manual DTO」）。

---

## 16. SDK Migration

### 16.1 目标 auth 面

```ts
// 新
export interface FloatCTFClientOptions {
  baseUrl?: string;
  adminBaseUrl?: string;
  getToken?: () => string | null | undefined;   // ← 唯一 token 来源
  onUnauthorized?: (ctx: UnauthorizedContext) => void;   // 仅 401
  onForbidden?: (ctx: ForbiddenContext) => void;         // ← 新增，仅 403
  onError?: (error: FloatCTFError, scope: FloatCTFTransportScope) => void;
  requestConfig?: FloatCTFRequestConfig;
}
```

| 位置 | 现状 | 目标 |
|---|---|---|
| token getter | `getUserToken` + `getAdminToken` | `getToken`（单一）；**过渡期**保留两者作为 fallback（Wave 7 内），Wave 10 删除 |
| `FloatCTFAuthScope` | `"user" \| "admin"`，公开导出，含义是**认证域** | 改名 `FloatCTFTransportScope`，含义降级为**传输/URL 域**（仅决定 baseURL 与错误归属）。过渡期保留 `FloatCTFAuthScope` 作为 deprecated alias，Wave 10 删除 |
| `UnauthorizedContext` | `{scope, status, error}` | 保持（`scope` 改为 transport 语义） |
| 403 | 无 | 新增 `ForbiddenContext { scope, status, error }` + `onForbidden` 回调；`FloatCTFError` 增加 `forbidden: boolean`（`httpStatus === 403`） |
| `FloatCTFError.unauthorized` | `httpStatus === 401` | 不变（语义正确） |

### 16.2 两个 domain client 是否保留

**保留 `client.service` / `client.admin` 两个领域门面与两个 transport。**
理由（§12.1）：base URL 分离是既有公共契约，三个前端与领域 API 都已按此分层；
统一**认证**不要求合并**命名空间**。变化只有一处：

```ts
// 目标：两个 transport 共用同一个 token
const serviceTransport = createFloatCTFTransport("service", options, baseUrl);
const adminTransport   = createFloatCTFTransport("admin",   options, adminBaseUrl);
// 两者都调用 options.getToken()
```

### 16.3 transport base URL 为什么仍可独立

`client.awdp.runs` 已经证明 base URL 与授权域无关（service transport + `/service/...` 路径）。
base URL 表达的是「这组端点在 URL 空间里的位置」，不是「用哪个凭据」。

### 16.4 scope 是否仍有意义

**有，但语义降级**：`onError(error, scope)` / `onUnauthorized({scope})` 里的 scope
用来回答「是哪个传输收到 401/403」，前端据此决定跳 `/admin` 还是 `/`。
统一 token 后它**不再表示凭据来源**。文档必须改写，否则会延续
`useAdminAwdEventStream.ts:4-7` 那种「admin token 是独立认证域」的错误表述。

### 16.5 401 回调新模型

| 状态 | 回调 | 前端行为（Default） |
|---|---|---|
| 401 | `onUnauthorized({scope})` | 清 `token`（唯一 token）+ 记 `redirectAfterLogin` + 跳登录页 |
| 403 | `onForbidden({scope})` | **不清 token、不跳登录**；显示「没有权限执行该操作」；若是路由级拒绝，跳一次 `/admin`（或 403 页面） |

**这是本次 SDK 改动里最关键的一处**：如果 403 走 401 的路径，RBAC 一上线
所有权限不足的用户都会被踢下线（§3 P5）。必须有专门测试锁死
（§21.4）。

### 16.6 SSE

- `connect()` / `connectAdmin()` **保留**（URL 域不同），但默认 token 都取 `getToken`。
- `connectSse` 的 `auth_error` 状态需要拆分：401（`auth_error`，token 问题）vs
  403（新增 `forbidden` 状态，不重连且**不**清 token）。当前
  `connectSse.ts:164-168` 把 401/403 都当 `auth_error`，必须改。
- 管理端 SSE `/api/admin/events/{id}/awd/stream` 在 Wave 5 要求
  `awd.read`（不再是「任何已登录账号」）。

### 16.7 React bindings（`@floatctf/react`）

```ts
// 现状
export interface CreateFloatCTFReactOptions {
  client: FloatCTFClient;
  useUserToken: UseTokenSource;
  useAdminToken?: UseTokenSource;   // 省略时复用 useUserToken
}
// 目标
export interface CreateFloatCTFReactOptions {
  client: FloatCTFClient;
  useToken: UseTokenSource;         // 唯一
}
```

四个 hook 工厂不变（`useAwdEventStream` / `useAdminAwdEventStream` /
`useAwdpEventStream` / `useAwdpRunStream`），只是都把 `useToken` 传下去。
`useAdminAwdEventStream.ts:4-7` 的注释必须改写。

### 16.8 新增/修复的 SDK 面

| 项 | 内容 |
|---|---|
| 终端 API | **新增** `client.admin.terminal.createSession()` → `POST /terminal/session`；WS URL 由前端自行拼（SDK 不引 WebSocket 抽象）。今天终端完全不在 SDK 里，Default 手写 `client.adminHttp.post("/terminal/session")` |
| `/me` DTO | **新增** `MeDto` 类型（§9.5），`client.service.users.getMe(): UniResponse<MeDto>` |
| 角色 API | **新增** `client.admin.roles.{fetch,get,create,patch,remove}`、`client.admin.permissions.fetch`、`client.admin.users.{fetchRoles,setRoles}` |
| envelope guard | **接线** `floatCTFErrorFromEnvelope` 进 `attachInterceptors`（今天只被两个前端各自绕过 SDK 实现） |
| `console.log` | **删除** `api/admin/users.ts:9`（泄漏 hash）与 `api/admin/event_challenges.ts:54` |
| 密码类型 | **停止**用 `entity/users.ts` / `entity/super_admin.ts` 作为响应类型；改用具名 view DTO |
| 缺失方法的补齐 | `GET /api/admin/logs/{id}`、`GET /api/admin/scheduled_tasks/{id}`、`GET /api/admin/users/{id}`（Wave 0 记录，Wave 7 补齐，使路由↔权限表零缺口） |

### 16.9 不允许永久保留旧 alias

`getUserToken` / `getAdminToken` / `FloatCTFAuthScope` / `client.admin.login` /
`connectAdmin` 的 **token 默认来源** 都只保留到 Wave 10。Wave 10 的 Gate 是
「全仓（含三个前端）grep 零命中」。`connectAdmin` 与 `client.admin.*` 保留
（它们是 URL 域，不是 token alias）；`client.admin.login` 删除（登录收敛到
`client.service.session.login` 或新的 `client.auth.login`）。

---

## 17. Default Frontend Migration

### 17.1 AuthStore v2

```ts
type AuthState = {
  token: string | null;                                  // 唯一会话 token
  account: MeDto | null;                                 // /me 结果（含 roles/permissions）
  setToken: (t: string) => void;
  setAccount: (a: MeDto | null) => void;
  clearSession: () => void;                              // 替代 removeToken + removeAdminToken
};
export const useAuthStore = create(persist<AuthState>(set => ({...}), {
  name: "auth-storage",
  version: 2,                                            // ★ 必须
  migrate: (persisted, version) => {                     // ★ 必须
    // v0/v1 的 {token, adminToken, username, nickname} → v2
    // 旧 adminToken 丢弃（admin token 已不存在）；token 保留；account 置 null（下次 /me 填充）
    const old = persisted as { token?: string | null } | undefined;
    return { token: old?.token ?? null, account: null };
  },
  partialize: (s) => ({ token: s.token }),               // account 不持久化（每次启动重新拉）
}));
```

**`version` + `migrate` 是硬要求**（§2.8 已核验：当前无 `version`/`migrate`）：
否则旧 `auth-storage` 里的 `adminToken` 会静默残留在新 state 里，
而旧 `token` 若属于一个**管理端**账号也可能留下误导性登录态。

**`account` 不持久化**：权限必须每次启动从 `/me` 重新解析，避免「localStorage 里
存着一份旧权限」成为前端越权展示源（§19 不变式 3：前端隐藏 UI 不是授权，
但也不该用陈旧数据进行前端判断）。

### 17.2 Session 恢复

```
启动 → 读 localStorage token
     → 有 token：query GET /api/users/me
         ├── 成功 → setAccount(me)
         └── 401   → clearSession() → 未登录
     → 无 token：未登录
```

`beforeLoad` 守卫必须**等待** `/me` 的结果（TanStack Router 支持 async `beforeLoad`）。
今天的守卫是同步读 localStorage（`routes/admin/route.tsx:18-26`）。

### 17.3 单一登录

| 现状 | 目标 |
|---|---|
| `/`（`routes/index.tsx`）—— 选手登录，`POST /api/users/session` | 保留为**唯一登录页**，改调 `POST /api/session` |
| `/admin/`（`routes/admin/index.tsx`）—— 管理登录，`POST /api/admin/session` | **删除**；`/admin` 未认证时跳 `/`（带 `redirect` 参数） |

细节：
- `adminIgnoreRoutes`（`navigation/admin-navigation.tsx:184`）由
  `["/admin", "/admin/"]` 变成 `[]`（不再有 admin 登录页需要豁免），
  或保留空数组以最小化 diff——**推荐删除常量并移除 `route.tsx:19` 的跳过逻辑**，
  因为它存在的唯一理由（避免自我重定向循环）随登录页消失而消失。
- `AdminRouteGuardWithRedirect`（`admin/route.tsx:59-64`）/`ServiceRouteGuardWithRedirect`
  （`service/route.tsx:58-63`）必须**改成权限感知**：已登录但无任何管理权限的用户
  访问 `/admin` 不能进 dashboard，否则会被后端 403 —— 应显示「无管理权限」而非循环重定向。
- `ServiceRouteGuardWithRedirect` 同时是 `/`、`/register`、`/reset`、`/reset_password`
  的 loader（§2.8）。统一后若用「已登录」单标志把已登录用户从 `/` 弹走，
  管理端账号将无法访问注册/重置页 → 需要按「当前路由是否需要未登录」细分。
- logout：`clearSession()` 清唯一 token；`Header` 的 `ServiceHeader` / `AdminHeader`
  合并为一个 logout 行为。**不做**服务端 logout（无端点）；如需「吊销全部会话」，
  由账号设置页显式调用「吊销会话」动作（`token_version += 1`），
  Wave 5 提供后端能力，Wave 8 提供 UI 入口。

### 17.4 权限感知导航

**利用既有接缝（最省力）**：

```tsx
// routes/admin/route.tsx / routes/service/route.tsx
const account = useAuthStore(s => s.account);
const visibility = useMemo(() => ({
  permissions: new Set(account?.permissions ?? []),
  features: new Set<string>(),                 // 静态平台能力已在 mount context 里
}), [account]);

<HierarchicalSideBar sections={adminNavigation} visibility={visibility} />
```

然后给 `admin-navigation.tsx` 的节点加 `isVisible`：

```tsx
const can = (p: string) => ({ permissions }: NavigationVisibilityContext) =>
  permissions.has(p);

item("access.super-admins", "Super Admins", "/admin/super_admins", {
  icon: <ShieldLockIcon />, isVisible: can("users.read"),
}),
item("infrastructure.terminal", "Terminal", "/admin/terminal", {
  icon: <TerminalIcon />, isVisible: can("terminal.open"),
}),
```

导航节点 → 权限对照（与 §7 表一致）：

| 导航节点 | 路径 | `isVisible` |
|---|---|---|
| `access.users` | `/admin/users` | `users.read` |
| `access.super-admins` → 改名 **Roles & Admins** | `/admin/super_admins` → `/admin/roles` | `roles.read` |
| `events.all` | `/admin/events` | `events.read` |
| `content.challenges.*` | `/admin/challenges*` | `challenges.read` |
| `content.gameboxes` | `/admin/awd/gameboxes` | `gameboxes.read` |
| `content.weapons` | `/admin/weapons` | `weapons.read` |
| `community.announcements` | `/admin/announcements` | `announcements.manage` |
| `community.discussions` | `/admin/discussions` | `discussions.moderate` |
| `infrastructure.networking` | `/admin/awd/network` | `awd.read` |
| `infrastructure.terminal` | `/admin/terminal` | `terminal.open` |
| `infrastructure.docker` | `/admin/docker` | `docker.read` |
| `infrastructure.database` | `/admin/database` | `database.execute` |
| `operations.logs` | `/admin/logs` | `logs.read` |
| `operations.scheduled-tasks` | `/admin/scheduled_tasks` | `scheduler.manage` |
| `system.settings` | `/admin/settings` | `settings.read` |
| `system.version` | `/admin/version` | `system.read` |
| `overview.dashboard` | `/admin/dashboard` | `system.read` |

### 17.5 路由守卫

```tsx
// routes/admin/route.tsx
beforeLoad: async ({ location }) => {
  const { token, account, refreshAccount } = useAuthStore.getState();
  if (!token) throw redirect({ to: "/", search: { redirect: location.href } });
  const me = account ?? await refreshAccount();          // 已 await /me
  if (!me) throw redirect({ to: "/" });                  // /me 401
  if (me.permissions.length === 0) {
    // 已认证但完全没有管理权限 → 明确提示，不进入后台
    throw redirect({ to: "/service/top", search: { denied: "admin" } });
  }
},
```

**每条路由的细粒度权限**（例如 `/admin/database` 需要 `database.execute`）在
**页面级**再加一次判断，并渲染 403 状态组件（`MissingPermission`），
而不是重定向——让用户看到「你没有这个权限」，而非神秘跳转。

### 17.6 按钮 / 动作级 gating

```tsx
const { has } = usePermissions();       // 新 hook，包 useAuthStore(account)
<Button disabled={!has("docker.manage")} …>Stop</Button>
```

规则：
- 「需要权限才能执行」的按钮：**禁用 + Tooltip 说明所需权限**（比隐藏更好，
  因为用户能知道为什么做不到）。
- 「完全无关」的入口：隐藏（导航级）。
- 破坏性动作（删账号、删角色、撤销最后一个超管、`database.execute`）：
  Default 必须用既有 `useConfirm` / `Dialog`，**禁止原生 `confirm(`**
  （`docs/agents/RULES.md` §1–§4 仅约束 `frontends/default/**`）。

### 17.7 403 状态与 role 变更

- 新增通用组件 `MissingPermission`（Primer `Banner variant="warning"` +
  所需权限名），供页面级 403 使用。
- 全局 403 处理：`onForbidden` → 显示 banner（不跳登录、不清 token）。
- **登录后角色变更**：管理端页面在 mutation 成功后 `invalidateQueries(["profile"])` 并
  强制 `refreshAccount()`；因为信任后端每请求校验，前端只需在**下一次导航**重算导航。
  不引入 webSocket 推送权限变更（第一版）。

### 17.8 新页面：Roles / Users

```
/admin/roles            角色列表 + 从模板创建 + 新建/编辑/删除
/admin/roles/{key}      角色详情：name / key / description / protected / grants_all
                        + 权限分域勾选（按 §7.2 的 domain 分组）
                        + 已获该角色的账号数
/admin/users            账号列表（+ 状态列 + 角色列）
/admin/users/{id}       账号详情：资料 / status / 角色（可改） / 有效权限（展开、只读）
```

| 要求 | 实现要点 |
|---|---|
| 权限分域展示 | 按 `Identity / Content / Events / AWD / AWDP / Community / Platform / Infra` 分组（权限的 `domain()` 由代码提供，经 `GET /api/admin/permissions` 返回） |
| 高危提示 | 5 个高危权限（§7.4）在勾选框旁加 danger 标记 + 说明文案 |
| protected 角色限制 | `protected=true` 的角色：删除按钮禁用 + Tooltip；权限勾选禁用（`grants_all` 时显示「全部权限（含将来新增）」而**不是** 29 个全勾——避免暗示它是静态集合） |
| last super-admin 防误操作 | 涉及最后一个超管的操作（删账号/禁用/移除角色）前端**禁用 + 说明**；后端仍必须拒绝（前端不是防线） |
| 角色授予审计 | Users 详情页展示 `granted_by` / `granted_at`（来自 `GET /api/admin/users/{id}/roles`）；变更后 invalidate `["logs"]` |

**页面形态参照**（`docs/agents/RULES.md` 铁律 9）：角色/账号列表用
`components/Table.tsx` 的 `GenericTable` CRUD 形态，**参照页**为
`frontends/default/src/routes/admin/challenges.tsx`（完整）与
`frontends/default/src/routes/admin/users.tsx`（最简）；
带行内动作对话框 + 改密的参照是
`frontends/default/src/routes/admin/super_admins.tsx`（Wave 10 删除，Role 页接管其形态）。
弹窗用 `useConfirm` / `Dialog`；提示用 `useMsgBanner`；错误文案用
`components/apiErrorMessage.ts` 的 `formatApiError`。

---

## 18. External Frontend Contract

第三方前端**不应该**读 Default 的私有 store 才能判断权限。因此 RBAC 能力必须
作为**公共契约**暴露：

### 18.1 公共面清单

| 能力 | 公共面 | 说明 |
|---|---|---|
| 登录 | `client.service.users.login()` 或新增 `client.auth.login()` → `POST /api/session` | 返回裸 JWT |
| 当前身份与权限 | `GET /api/users/me` → `MeDto { id, username, nickname, email, avatar, status, roles:[{key,name}], permissions:[string] }` | 权限是**字符串数组**，可直接放进 `Set` |
| 角色标识 | `roles[].key`（稳定、`^[a-z][a-z0-9-]{1,63}$`） | 前端**只应**用 key 做逻辑；`name` 只用于展示 |
| 权限标识 | `permissions[]`（§7.2 的 29 个字符串） | 前端不得硬编码「哪些权限算管理权限」以外的东西 |
| 403 行为 | `FloatCTFError.forbidden === true` / `httpStatus === 403` / `onForbidden(ctx)` | **不得**把 403 当 401 处理 |
| 401 行为 | `FloatCTFError.unauthorized === true` / `onUnauthorized(ctx)` | 清 token + 重新登录，**不要**在 403 时做 |
| 高风险权限清单 | SDK 导出 `HIGH_RISK_PERMISSIONS: readonly string[]` | 让第三方前端也能做 danger 提示，而不必抄一份清单 |
| 权限元数据 | `GET /api/admin/permissions` → `[{ key, domain, name, description, high_risk }]` | 用于渲染角色编辑 UI |

### 18.2 SDK 导出要求

```ts
// @floatctf/sdk 新增导出
export type { MeDto } from "./types/me.js";
export { HIGH_RISK_PERMISSIONS } from "./permissions.js";
export type { PlatformRoleRef, PermissionKey } from "./permissions.js";
```

`PermissionKey` 用**字符串字面量联合**（29 个字面量）而不是 `string`，
这样第三方前端能享受类型补全；同时保留 `(string & {})` 兜底以免新增权限时
旧 SDK 直接编译失败（加法式变更不升 major 的原则，§2.9）。

> **注意**：本计划建议 `API_CONTRACT_VERSION` 升到 `"2"`（§28 决策 10），
> 但**新增**的 `MeDto` / `HIGH_RISK_PERMISSIONS` 等本身是加法式的。
> 升 major 是因为**删除/改语义**的部分（`/api/admin/session`、`/super_admin`、
> `/users/me` 的响应结构、`getUserToken`/`getAdminToken`），不是因为新增。

### 18.3 需要更新的文档

| 文档 | 更新内容 |
|---|---|
| `docs/frontend/AI-FRONTEND-GUIDE.md` | 认证章节重写：单 token、`/me` 权限、401 vs 403、禁止把 `capabilities` 与 `permissions` 混用（§5.1） |
| `docs/frontend/CAPABILITY-MATRIX.md` | 新增「平台 RBAC（roles / permissions / 403）」能力行，标注 `required`，指向 `MeDto` 与 `client.service.users.getMe` |
| `packages/sdk/README.md` | 认证模型重写；`getToken`；`onForbidden`；`MeDto`；终端 API |
| `docs/frontend/ARCHITECTURE.md` | 版本策略一节：说明 `API_CONTRACT_VERSION` 升 `"2"` 的原因与已安装前端的升级路径 |

---

## 19. Security Invariants

以下为**必须始终成立**的不变量。每条都要有对应测试（§21），
并在 Code Review 时逐条核对。

| # | 不变量 | 强制方式 | 测试 |
|---|---|---|---|
| 1 | **password hash 永不通过任何 API response 返回** | Response DTO 不包含 `password`/`password_hash` 字段；`MeDto`/`AdminUserDto`/`RoleDto` 全部无该字段 | 反射式测试：对每个 admin 端点的 2xx 响应体断言 JSON 无 `password` 键且无 `$argon2` 子串 |
| 2 | **每个 `/api/admin/**` 端点必须做后端 permission check** | 声明式 `RequirePermission<P>` 在 handler 签名里；路由目录测试断言「每条 admin 路由都有对应权限」 | `admin_routes()` 表驱动：无 token → 401；有 token 无权限 → 403；有权限 → 非 401/403 |
| 3 | **前端隐藏 UI 不是授权** | 403 必须由后端返回；前端只做 UX | E2E：直接用 `curl` 打被隐藏的端点，必须 403 |
| 4 | **权限变更下一次请求生效** | 不缓存权限（§10.3）；每请求解析 | 「撤销 `event-manager` 后，下一个管理请求 403」（§44 Case B） |
| 5 | **last super-admin 不可消失** | §20.1 的四条 DB 级检查 + 应用级检查 | 4 个测试：删账号 / 禁用 / 移除角色 / 删角色，全部必须 409/400 |
| 6 | **non-super-admin 不可权限提升** | §20.2 的 before/after 双侧范围规则 | `event-manager` 给自己加 `terminal.open` → 403（§44 Case E） |
| 7 | **internal service token 不参与 Platform RBAC** | `AwdInternalAuth` 只产出 `AwdInternalPrincipal`，不产出 `AuthContext`；`AwdJudger` 变体删除 | 测试：`AwdJudger` 用途的 token 打 `/api/admin/**` → 401；内部 token 打 `/api/admin/**` → 401 |
| 8 | **event membership ≠ platform role** | `event_users` / `event_teams` / `event_team_members` 不参与 `effective_permissions` | 测试：赛事 A 的管理员对赛事 B 无任何赛事权限（除非持有平台角色） |
| 9 | **active account check 每请求生效** | `AccountGuard` 在每次请求查 `users.status` | 测试：禁用账号后，其既有 token 的下一个请求 → 403 |
| 10 | **audit actor 必须可追踪** | 每个管理写操作写 `logs` 且 `actor_user_id` 非空；`ON DELETE SET NULL` 保留流水 | 测试：管理写操作后查 `logs`，`actor_user_id` = 操作者 id；删除该账号后该行仍存在且 `actor_user_id` 变 NULL |

**补充不变量（本计划识别，建议同批加入）**：

| # | 不变量 | 说明 |
|---|---|---|
| 11 | `token_version` 是**凭据**版本，不是权限版本 | §9.4；引入权限缓存时不得合并（§27） |
| 12 | 「已认证但无权限」返回 **403**，「凭据无效/无账号」返回 **401** | §11.3 的表是唯一权威 |
| 13 | 禁用账号返回 **403** | §11.3；避免前端「清 token → 重登 → 再被拒」的无效循环 |
| 14 | 权限检查**不依赖** JWT 中的任何授权信息 | §9.2；JWT 只承载身份 |
| 15 | `platform_roles.key` 创建后不可变 | §6.3；审计与判定只用 key，改名会破坏历史可读性 |

---

## 20. Privilege Escalation Protection

### 20.1 Last Super Admin Protection（§8.1）

不变式：**系统任何时刻必须存在 ≥ 1 个 `status = active` 且持有 `grants_all` 角色的账号。**

必须覆盖的 6 条路径：

| # | 操作 | 端点 | 检查 | 结果 |
|---|---|---|---|---|
| 1 | 删除最后一个超管账号 | `DELETE /api/admin/users`（批量） | 若删除后有效超管数 = 0 → 拒绝 | `409 Conflict`「不能删除最后一个超级管理员」 |
| 2 | 禁用最后一个超管账号 | `PATCH /api/admin/users/{id}`（`status=disabled`） | 同上 | `409` |
| 3 | 从某账号移除 `super-admin` 角色 | `PUT /api/admin/users/{id}/roles` | 若该账号是最后一个 → 拒绝 | `409` |
| 4 | 删除 `super-admin` 角色本身 | `DELETE /api/admin/roles/{key}` | `protected=true` → 一律拒绝（与是否最后一个无关） | `409`「系统内置角色不可删除」 |
| 5 | 修改 `super-admin` 的 `grants_all` | `PATCH /api/admin/roles/{key}` | `protected=true` → `grants_all` 与 permission 集不可改 | `409` |
| 6 | 修改 `super-admin` 的 `protected` | `PATCH /api/admin/roles/{key}` | `protected=true` → `protected` 不可改 | `409` |

**核心并发机制（权威）：应用事务级串行化 + 对固定 `super-admin` 角色行 `FOR UPDATE`。**

**不采用**普通 `BEFORE DELETE OR UPDATE` **行级** trigger 去「每行立刻重数」。
原因：这类实现在**同一事务内的合法转移**上会误报——例如一个事务里
「先把 `super-admin` 授予 A，再移除 B 的 `super-admin`」，中间态会瞬时出现 0 个超管，
但**最终状态合法**。行级 trigger 在中间态就抛异常，把合法操作变成失败。

**唯一权威流程**（所有可能影响「有效 `grants_all` 账号数」的写操作都必须走）：

```text
BEGIN transaction
  ↓
SELECT id, grants_all, protected
  FROM platform_roles
 WHERE key = 'super-admin'
   FOR UPDATE                       -- ← 统一 lock point，序列化全部相关 mutation
  ↓
读取当前有效 grants_all accounts（见下方计数口径）
  ↓
计算本次 mutation 的**最终状态**下有效 grants_all accounts 数
  ↓
若 < 1 → ROLLBACK + 409 Conflict（友好文案）
  ↓
执行 mutation（同一事务内）
  ↓
COMMIT
```

**为什么用固定 `super-admin` 角色行作为统一 lock point**：它把下列**全部**互斥地在
同一把锁上串行化，而不需要任何跨表锁顺序推理：

| # | 操作 | 端点 | 检查 | 结果 |
|---|---|---|---|---|
| 1 | 删除最后一个超管账号 | `DELETE /api/admin/users`（批量，按**最终集合**计算） | 若删除后有效超管数 = 0 → 拒绝 | `409 Conflict`「不能删除最后一个超级管理员」 |
| 2 | 禁用最后一个超管账号 | `PATCH /api/admin/users/{id}`（`status=disabled`） | 同上 | `409` |
| 3 | 从某账号移除 `super-admin` 角色 | `PUT /api/admin/users/{id}/roles` | 若该账号是最后一个 → 拒绝 | `409` |
| 4 | 删除 `super-admin` 角色本身 | `DELETE /api/admin/roles/{key}` | `protected=true` → 一律拒绝（与是否最后一个无关） | `409`「系统内置角色不可删除」 |
| 5 | 修改 `super-admin` 的 `grants_all` | `PATCH /api/admin/roles/{key}` | `protected=true` → `grants_all` 与 permission 集不可改 | `409` |
| 6 | 修改 `super-admin` 的 `protected` | `PATCH /api/admin/roles/{key}` | `protected=true` → `protected` 不可改 | `409` |
| 7 | 创建/删除**任何** `grants_all` 角色 | 角色管理 | 只有 migration/bootstrap 能建立 `grants_all`；API 不提供该字段 | 该字段不存在于 DTO |

**关键约束（实现必须遵守）**：

1. **先算最终状态，再判定**。对批量删除必须按「本次请求要删的**全部** id」计算，
   不能逐个判定（逐个判定在批量删除全部超管时会全部通过）。
2. **必须在同一事务内**读取 → 判定 → mutation。任何「先查后改」跨事务的写法在并发下
   都会被绕过。
3. **锁必须在读取计数之前获取**，否则两个事务都能读到「还有 1 个」。
4. 错误码统一 `409 Conflict`（不是 500、不是 403）。
5. 该锁**同时**服务 §20.2 的 before/after 快照需求：在同一事务、同一锁下读取
   目标角色的 before 集合与目标账号的 before 有效权限，避免读到脏值。

**关于 DB trigger**：**不再是必须项**。只有当能证明某个 trigger 实现是**真正的
transaction-end / deferred invariant**（例如 `CONSTRAINT TRIGGER ... DEFERRABLE
INITIALLY DEFERRED`，在 COMMIT 时才校验最终状态）时，才可以作为
**defense-in-depth** 加上。若不能证明它保持合法事务通过，**不要**加普通
`BEFORE` 行级 trigger。

**并发回归测试（必须有）**：

```text
前置：恰好 2 个 active grants_all account（A、B）
并发：请求 1 移除 A 的 super-admin 角色；请求 2 移除 B 的 super-admin 角色
期望：最终至少保留 1 个 grants_all account
     恰好一个请求成功（2xx），另一个 409 Conflict
```

**计数口径（必须与实现一致，写进代码注释）**：

```sql
SELECT count(DISTINCT u.id)
  FROM users u
  JOIN user_platform_roles upr ON upr.user_id = u.id
  JOIN platform_roles r       ON r.id = upr.role_id
 WHERE u.status = 'active' AND r.grants_all = true;
```

> 「最终状态」的算法：先按该口径算出 `current`，再叠加本次 mutation 的确定性影响
> （删除账号集合 `D`、禁用集合 `S`、角色变更映射 `M`），得到 `final`。
> 实现上建议抽成纯函数 `fn super_admin_count_after(current: usize, mutation: &Mutation) -> usize`
> 以便无 DB 单测（§21.1）。

### 20.2 Role Escalation —— 非超管不可授予自己没有的权限（§8.2）

**核心原则**（统一表述）：

> **非 `grants_all` 发起人只能操作「变更前与变更后都完全落在自己权限范围内」的对象。**

形式化记号：
- `actor.permissions`：发起人的 effective permissions（`grants_all` → 全集）
- `before(X)` / `after(X)`：操作**前** / 操作**后**对象 X 的权限集合
- `eff(U)`：账号 U 的 effective permissions（= 其全部角色的并集）

#### 20.2.1 编辑一个普通角色 R

非 `grants_all` actor 需要：

```text
actor.has(RolesManage)
∧ before_permissions(R) ⊆ actor.permissions
∧ after_permissions(R)  ⊆ actor.permissions
```

语义：**你只能编辑一个「当前完全处于你权限范围内」的角色，并且修改后的角色也不能
超出你的权限范围。**

这**同时允许**：

```text
增加一个你自己拥有的 permission
删除一个你自己拥有的 permission        ← 修正点：旧规则 before ⊆ after 禁止了这条
重命名 name / 修改 description
```

又**禁止**：

```text
修改一个当前包含你没有的 terminal.open 的角色   ← before ⊄ actor
给一个你完全掌握的角色加上你没有的 database.execute  ← after ⊄ actor
```

> **为什么 before 与 after 都要检查**（这是对旧规则的修正）：
> 旧规则写的是 `after ⊆ actor ∧ before ⊆ after`。第二条会把
> `before ⊆ after` 变成「只能加不能减」，导致即使一个角色完全在管理员权限范围内，
> 他也**永远无法删除该角色里的任何 permission**——这与「你只能管你已有的东西」
> 的意图无关，纯粹是过度约束。
> 正确的约束是**两侧都限制在 actor 权限内**：只要 before 与 after 都是 actor 权限的
> 子集，那么无论是增是减，actor 都只是在操作自己掌控的那部分权限，不构成提权，
> 也不构成对他人权限的未授权削弱（因为他本来就完全掌握该角色）。

#### 20.2.2 修改某账号 U 的角色集合（`PUT /api/admin/users/{id}/roles`）

非 `grants_all` actor 需要：

```text
actor.has(RolesManage)
∧ before_effective_permissions(U) ⊆ actor.permissions
∧ after_effective_permissions(U)  ⊆ actor.permissions
```

**若目标账号拥有任意 `grants_all` 角色**：

```text
只有 grants_all actor 可以修改其 role assignment / disable / delete
```

> **为什么 before 侧必须检查账号的有效权限**（这是另一处修正）：
> 若只检查 mutation **之后**的状态，一个低权限管理员可以把一个**比他权限高**的
> 管理员直接**降权**（例如移除对方的 `terminal.open` 角色）——after 变小当然满足
> `after ⊆ actor`，但这属于对他人权限的未授权修改。加上 before 侧检查后，
> 低权限管理员根本无法触碰任何「权限高于自己」的账号。

#### 20.2.3 禁用 / 删除其它账号

非 `grants_all` actor：

```text
target.current_effective_permissions ⊆ actor.permissions
```

否则 `403 Forbidden`。然后再叠加 §20.1（Last Super Admin）与 §20.3（Self-lockout）。

#### 20.2.4 自己修改自己

**遵守完全相同的规则**（不为「自己」开特例）。因此：

```text
给自己增加一个自己不拥有的 terminal.open        → 403
删除自己某个已有的普通角色                       → 允许
但若该操作会破坏 last super-admin                → 409
```

#### 20.2.5 `grants_all` 角色与 `protected` 角色

- 任何 `protected=true` 或 `grants_all=true` 的角色**不可**被非 `grants_all` actor
  编辑 / 删除 / 分配。
- **不能**通过 API 创建 `grants_all=true` 的角色：该字段**根本不出现在**
  创建/更新请求 DTO 里，只由 migration / bootstrap 建立。

**回答 Prompt 的具体问题**：「一个拥有 `roles.manage` 的管理员能否创建一个包含
`terminal.open` / `database.execute` / `docker.manage` 的角色？」

> 分两种情况：
> 1. **创建新角色**（`after(R) ⊆ actor.permissions` 且不存在 before）：
>    只有当该管理员**自己已经拥有**这三项权限时才能。
> 2. **编辑一个已含 `terminal.open` 的角色**：还需要
>    `before(R) ⊆ actor.permissions`，所以他必须**也**拥有 `terminal.open`。
>
> 若他只有 `roles.manage` + `events.*`，两种路径都会得到 `403 Forbidden`
> （「不能操作超出自身权限范围的能力」）。
> 这同时意味着：**允许**自定义角色获得高危权限（§7.4），但**不允许**通过
> 自建/自改角色绕过「你只能管你已有的东西」。

**实现要点**：

- 检查必须**在同一个事务内**、读到 **before 与 after 两份**集合，再与 actor 的
  **当前**权限比较（避免 TOCTOU：先授予再检查；并避免 before 被并发改写而读到脏值）。
- **快照 before**：在事务内先 `SELECT` 出对象当前权限集合并持有（配合 §20.1 的
  `FOR UPDATE` 锁点），再执行 mutation，不要用 mutation 之后的结果反推 before。
- 前端 Roles UI 对超出自身权限的勾选框**禁用 + 说明**（UX 层，不是防线）。
- 该规则**不适用于** `grants_all` 的 actor（超管天然可以操作一切）。

### 20.3 Self-lockout（§8.3）

| 操作 | 行为 | 结果 |
|---|---|---|
| 管理员删除自己的角色 | **允许**（受 §20.1 约束：若会变 0 个超管则拒绝） | 200，但下一个请求就开始 403；前端应提示「你已移除自己的管理权限」 |
| 管理员禁用自己 | **拒绝** | `409`「不能禁用自己的账号」 |
| 管理员删除自己的账号 | **拒绝** | `409`「不能删除自己的账号」 |
| 管理员移除自己最后一个角色（且不持有其它管理角色） | **允许**，除非会导致 0 超管；前端二次确认 | 200 |

**为什么「删除自己的角色」允许而「禁用/删除自己」拒绝**：前者是**可恢复的**
（超管可以再授予回来），后者是**自我毁灭且需要他人介入**。这符合
「最小惊讶 + 避免不可恢复状态」。

**辅助机制（推荐，非必需）**：`GET /api/admin/users/{id}/roles` 的响应里对
「这是最后一个超管」加 `is_last_super_admin: true` 标记，让前端能做精确的禁用态
（而不是前端自己去数）。

---

## 21. Test Matrix（§31）

### 21.1 Unit（`cargo test -p floatctf`，无 DB）

| 测试 | 断言 |
|---|---|
| permission union | 两角色并集 = 期望集合 |
| `grants_all` 短路 | `grants_all` 角色使 `has(任意 Permission::ALL 元素)` 全为 true，**包括**一个模拟「新增」的权限 |
| 角色 permission 去重 | 两角色含同一权限 → 集合里只出现一次 |
| `Permission::from_str` | 29 个合法字符串往返；非法字符串拒绝（含空串、大写、单段、`users.` 结尾） |
| escalation validation | 编辑角色：`before(R) ⊆ actor ∧ after(R) ⊆ actor` → `Ok`；before 或 after 越界 → `Err`；**允许在自身范围内删除** permission。用户角色集合：before/after 有效权限双侧检查；目标含 `grants_all` 时非 `grants_all` actor → `Err` |
| last super-admin 计数口径 | 纯函数化后测：0 / 1 / 2 个有效超管 × 删/禁用/移除角色的判定 |
| disabled account | `AccountGuard` 对 `status=disabled` 返回 `Forbidden`（403）而非 `Unauthorized` |
| token purpose | `PasswordReset` token 打普通端点 → 401；`Session` token 打 reset 端点 → 400/401 |
| `token_version` | claim 版本 != DB 版本 → 401 |
| `TokenPurpose` serde | 序列化/反序列化往返；未知 `purpose` 字符串 → 反序列化失败（**不是**默认成 Session） |
| 401/403 映射 | `AppError::Unauthorized.code()==401` / `.status_code()==UNAUTHORIZED`；`Forbidden` 同理 |

### 21.2 API integration（需要 DB；`apps/api/tests/`）

| 测试 | 断言 |
|---|---|
| anonymous → 401 | 扩展后的 admin 路由目录里**每条** admin 路由无 token 都要 401 且 JSON envelope |
| authenticated no permission → **403** | 玩家 token 打每条 admin 路由 → 403（**不是 401**） |
| permission granted → 2xx | 按 §7 表授予权限后可达（非 401/403） |
| permission revoked → 下一请求 403 | 授予 → 2xx；撤销 → **立即** 403（§44 Case B） |
| multiple roles union | `event-manager` + `challenge-manager` 的并集行为（§44 Case C） |
| super-admin all access | `super-admin` 对**全部** admin 路由非 401/403 |
| **响应无 password** | 对全部 admin 端点 2xx 响应断言无 `password` 键、无 `$argon2` 子串、无 `password_hash` |
| disabled account | 禁用后既有 token → 403；重新启用后新登录 → 2xx |
| `token_version` 失效 | 改密后旧 token → 401；新登录 token → 2xx |
| self-lockout | 禁用自己 → 409；删除自己 → 409；删除自己角色（非最后超管）→ 200 |
| last super-admin | 4 条路径全测（§19 不变式 5） |
| escalation | `roles.manage` 持有者授予 `terminal.open` 给自己 → 403（§44 Case E） |
| protected role | 编辑/删除 `super-admin` → 409 |
| event membership ≠ role | §19 不变式 8 |

### 21.3 Migration（每张 migration 都有正向 + 边界）

| 测试 | 断言 |
|---|---|
| no collision（正常路径） | fresh DB + 单超管 → M1..M6 全部 apply 成功；`users` 含 sysadmin；`logs.actor_user_id` 已回填 |
| username collision | 预置 `users` 有 `sysadmin` → M2 以 `IDENTITY_MIGRATION_PREFLIGHT` 失败，事务回滚，`schema_migrations` **不含** M2 |
| email collision | 预置 `users.email = 'sysadmin@system.com'` → 同上且消息含 email |
| email collision within super_admin | 两条同 email 的超管 → 同上 |
| UUID collision | 预置 `users.id = 0000…0000`（非 sysadmin）→ 同上 |
| nickname collision | 预置 `users.nickname = 'sysadmin'` → 同上 |
| 已有 super_admin 数据 | 3 条超管 → M3 后 `users` 增 3 行，password hash 与 created_at/updated_at **逐字节相同**，`user_platform_roles` 3 行 |
| hash preservation | 直接比对迁移前后 `password` 字段（防「顺手 rehash」） |
| FK preservation | M4 前后：随机取 7 张表各 1 行，`pg_constraint.confrelid` 从 `super_admin` 变 `users`，`confdeltype` 不变 |
| logs 回填无损 | 构造 `user_id` 只有 / `superadmin_id` 只有 / 都没有 三类行 → `actor_user_id` 分别等于原列 / 原列 / NULL |
| logs 双主体拒绝 | 构造一行两者都非空 → M5 `RAISE EXCEPTION`，回滚 |
| **失败原子性** | 对任一 migration 注入失败断言：该 migration 的 DDL **全部**回滚，前序已提交的保留 |
| **rerun** | apply 两次：第二次 0 applied，无错误，无重复数据 |
| **fresh vs 升级等价** | fresh DB 跑 M1..M6 与 升级路径（含 3 条超管）跑 M1..M6，在「去掉 UUID/时间戳」后 schema 等价；数据行数按预期差异 |
| M6 preflight | 有 FK 残留时 `DROP TABLE` 失败（证明 Gate 有效） |
| `merged.sql` 一致性 | `mise run db:migration:merge` 两次 sha256 相同；对 fresh DB apply `merged.sql` 后 `schema_migrations` 行数 = 54（48 + 6） |

### 21.4 SDK（`pnpm --filter @floatctf/sdk test`）

| 测试 | 断言 |
|---|---|
| one token for both transports | 单个 `getToken` 同时驱动 service 与 admin 请求（拦截器断言 Authorization 一致） |
| legacy fallback（过渡期） | 只给 `getUserToken`/`getAdminToken` 时仍工作（Wave 7 内） |
| 401 | 触发 `onUnauthorized({scope})`；`FloatCTFError.unauthorized === true` |
| **403** | 触发 `onForbidden({scope})`；**不**触发 `onUnauthorized`；`FloatCTFError.forbidden === true` |
| envelope guard | HTTP 200 + `code !== 0` → reject（证明 `floatCTFErrorFromEnvelope` 已接线） |
| SSE 401 vs 403 | 401 → `auth_error`；403 → 新 `forbidden` 状态且不重连、不清 token |
| SSE token 刷新 | `getToken()` 每次重连重新读取 |
| `MeDto` 类型 | 29 个权限的字面量类型可赋值；`HIGH_RISK_PERMISSIONS` 含全部 5 项 |
| 无 storage 写入 | SDK 源码/运行时均不触碰 `localStorage`（既有约束，加测试防回归） |

### 21.5 Default Frontend（`pnpm --filter @floatctf/web test`）

| 测试 | 断言 |
|---|---|
| 单次登录 | 登录一次后 `token` 非空、`account.permissions` 来自 `/me` |
| persist migrate | 预置 `auth-storage` v1（含 `adminToken`）→ 迁移后 state **无** `adminToken`（§17.1） |
| Session 恢复 | 有 token → 启动即拉 `/me` 并填充 `account`；`/me` 401 → `clearSession()` |
| admin 导航可见性 | 仅 `events.read` 的账号 → 只看到 Events 组；`terminal.open` → 看到 Terminal |
| route guard | 无 token 访问 `/admin/users` → 跳 `/`（带 redirect）；有 token 无 `users.read` → `MissingPermission` |
| 按钮 gating | `docker.read` 无 `docker.manage` → Stop 按钮禁用 + Tooltip |
| 403 不登出 | 触发 403 后 token 仍存在、未跳登录页 |
| live permission change | mock `/me` 返回新权限 → 导航重算（不需要刷新页面） |
| 无原生弹窗 | `frontends/default/**` 内 grep 无 `alert(` / `confirm(`（既有铁律的自动化检查） |
| 既有 test 不回归 | `filterNavigationSections` / `SideBar` / navigation 测试全绿 |

### 21.6 E2E 角色场景（§32）

账号（全部通过 API 创建，不依赖内置 seed）：

| 账号 | 角色 | 断言 |
|---|---|---|
| `player` | （无） | 登录成功；`/admin` 所有端点 403（§44 Case A 的 admin denied 部分）；能参赛 |
| `auditor` | `auditor` 模板 | `GET /api/admin/logs` 2xx；`DELETE /api/admin/logs` 不存在；`POST /api/admin/settings` 403；`POST /api/admin/database/exec_sql` 403 |
| `event-manager` | `event-manager` 模板 | `POST /api/admin/events` 2xx；`POST /api/admin/events/{id}/awd/start` 2xx；`POST /api/admin/terminal/session` 403；`POST /api/admin/database/exec_sql` 403；`DELETE /api/admin/docker/containers/{id}` 403 |
| `challenge-manager` | `challenge-manager` 模板 | `POST /api/admin/challenges` 2xx；`POST /api/admin/events/{id}/awd/finish` **403**（不能推动比赛时钟） |
| `ops-manager` | `ops-manager` 模板 | `GET /api/admin/docker/containers` 2xx；`POST /api/admin/terminal/session` 2xx；`POST /api/admin/roles` 403；`PATCH /api/admin/users/{id}` 403 |
| `super-admin` | `super-admin` | **所有** admin 路由非 401/403 |
| 兼有 `event-manager` + `challenge-manager` | 两角色 | 并集可达（§44 Case C） |
| AwdJudge 场景 | — | `AwdJudger` 变体已删除；内部 token 打 `/api/admin/**` → 401（§44 Case F） |

### 21.7 既有领域回归

必须在每个 Wave 收尾跑（`mise run check` + 相关 DB-gated 测试）：

| 域 | 测试 |
|---|---|
| Jeopardy | `jeopardy_modes_live.rs`、`http_flow.rs`、`event_domain_invariants.rs` |
| AWD | `awd_live.rs`、`awd_scenarios.rs`、`awd_score_semantics.rs`、`awd_ban_recovery.rs`、`awd_reset_requester.rs`、`awd_submission_boundaries.rs` |
| AWDP | `awdp_domain.rs`、`awdp_training.rs`、`awdp_practice_judge.rs`、`awdp_scoreboard.rs` |
| Auth/安全 | `http_auth_contract.rs`、`internal_auth_contract.rs`、`regression_security_concurrency.rs`、`flag_leak_and_terminal.rs`、`writeup_idor_and_files.rs` |
| 赛事参与语义 | `team_membership_lifecycle.rs` |

**特别断言**：平台 RBAC **不改变**赛事 player 语义——
`event_users` / `event_teams` / `event_team_members` 的读写路径与判定逻辑
（`jeopardy/api/submit.rs:150-181`、`awdp/service/authorization.rs`）**一行不改**。

---

## 22. Rollout / Upgrade（§39）

### 22.1 三种场景

| 场景 | 路径 | 验证 |
|---|---|---|
| **fresh install** | 安装器 → postgres initdb 执行 `merged.sql`（48 + 6 = 54 条）→ `schema_migrations` 54 行 → TRACKED / 0 pending | fresh DB apply 全绿；`users` 含 sysadmin；无 `super_admin` 表 |
| **existing installation upgrade** | 重跑同一安装器（幂等）→ `apply_migrations` 起 postgres → 等 healthy（≤180s）→ `migrate.sh apply` 补 6 条 → 失败则平台**未启动** | M1..M6 全部 applied；`db:migration:verify` 无 MODIFIED |
| **dev** | `mise run dev` → `mise run db:migration:apply`（dev compose **不挂** merged.sql） | 48→54 |

### 22.2 升级前置（必须写进 release note）

1. **先备份**：`sudo $FLOATCTF_HOME/backup.sh --out <file>`。
   备份是**明文未加密**的（`BACKUP_FORMAT_VERSION=1`），注意介质安全。
2. **确认没有 username / email / nickname 冲突**。可直接预检：
   ```sql
   SELECT s.username FROM super_admin s JOIN users u ON u.username = s.username;
   SELECT s.email    FROM super_admin s JOIN users u ON u.email    = s.email;
   SELECT s.username FROM super_admin s JOIN users u ON u.nickname = s.username;
   SELECT a.email FROM super_admin a JOIN super_admin b ON a.email = b.email AND a.id < b.id;
   SELECT s.id FROM super_admin s JOIN users u ON u.id = s.id;
   ```
   任一有行 → 先按 §13.4 的流程人工处理，再升级。
   （`docs/agents/TESTING.md` 的 `sysadmin@system.com` 在真实部署里大概率不冲突，
   但**玩家**账号叫 `sysadmin` 或邮箱同值是完全可能的。）
3. **确认 sysadmin 口令已改**（既有隐患：baseline 里是文档公开的默认口令，无强制改密）。

### 22.3 merged.sql / migration lock / rerun / partial failure

| 关注点 | 事实与处置 |
|---|---|
| `merged.sql` | `db:migration:merge` **确定性**生成（同批文件两次 sha256 一致）；只用于 **fresh** DB（生产 compose 挂到 `/docker-entrypoint-initdb.d/00-init.sql`）；**禁止**用它覆盖既有库；不手改、不入 git |
| migration lock | `pg_advisory_lock(1179204948)`（`0x464C4154` = `'FLAT'`）全程持有；`new` 另有本地 `flock .migrate.lock`（仅保护文件创建） |
| rerun | 幂等：`SELECT NOT EXISTS(...)\gset` + `\if :pending_v`；重复 apply 只补 pending |
| partial failure | 每个 migration 独立事务：失败者整体回滚、`schema_migrations` 不记录、前序已提交保留；修好重跑只补失败那条及其后 |
| checksum | `db:migration:verify` 对已应用文件做 sha256 比对；**改历史 = MODIFIED = 全环境漂移** |
| 版本一致性 | 每条 migration 都必须在 fresh DB 上可执行（不能假设历史数据存在） |

### 22.4 Wave 级升级顺序（部署侧）

```text
Wave 0  → 无 schema 变化：可直接发布（纯 API/测试/SDK 修复）
Wave 1  → M1：建表，零行为变化
Wave 2  → M2 + M3：加列 + 复制账号，零行为变化（守卫仍读 super_admin）
Wave 3  → M4：FK 重指向，零代码改动
Wave 4  → auth v2 代码 + AccountGuard + RequireSuperAdmin（**授权语义与 Wave 3 等价**）
Wave 5  → 逐路由替换为 RequirePermission + 角色/账号 API + 反提权/最后超管
Wave 6  → M5 + 109 个调用点改 actor_user_id（同一提交）
Wave 7  → SDK v2
Wave 8  → Default 前端 v2
Wave 9  → 两个 submodule 前端 + 文档 + API_CONTRACT_VERSION=2
Wave 10 → M6 删表 + 删除全部 alias
Wave 11 → 全量回归
```

**每个 Wave 都是可独立发布、可独立回滚（回滚 = 重装上一版安装器）的单元**——
除了 Wave 6（M5 删列与代码同一次提交，必须一起上）与 Wave 10（M6 必须在
代码零依赖之后）。

### 22.5 回滚策略（§39 的关键约束）

- **migration 是 forward-only，没有 downgrade。**
  `RELEASE.md` §5 原文：「数据库升级出错 | **数据库迁移是 forward-only**，
  无法 downgrade；只能 `sudo $FLOATCTF_HOME/restore.sh <备份归档> --yes` 从备份恢复」。
- 因此**每个含 migration 的 Wave 发布前必须做备份**，并把备份路径写进发布记录。
- Wave 的「回滚」= 重装上一版安装器（代码回退）+ 若 migration 已提交则从备份恢复。
  这就是为什么 M1–M4 被设计成**纯加法、零行为变化**：它们可以先上线、观察，
  在代码切换之前**不产生任何可感知差异**，把风险窗口与代码风险窗口分离。
- `scripts/restore.sh` 的安全能力可直接复用（`--yes` / `--dry-run` / `--force`、
  sha256 + MANIFEST 校验、拒绝路径穿越/符号链接、PG major 检查）。

### 22.6 安装器侧注意

- 安装器**不创建管理员**（已核验：`install.sh` 零命中 `super_admin`/`sysadmin`/
  `ADMIN_USERNAME`/`ADMIN_PASSWORD`），所以**不需要**改安装器的账号创建逻辑。
- 安装器 `apply_migrations` 只起 postgres（不起 API），因此升级期间**没有并发写入**，
  M2/M3 的 preflight 与复制之间没有竞态窗口。但 `mise run db:migration:apply`
  在 dev 下**不会**停 API，所以 M3 内部**也要**复检 preflight（§13.5）。
- 安装器每次升级会**清空并重新填充** `$FLOATCTF_HOME/db/migrations/`，
  因此 release 必须包含全部 54 条 migration（`release-checksums.sh` 已经打包
  `db/migrations/`）。

---

## 23. Implementation Waves

> 每个 Wave 是**可独立提交、可独立验收、可独立发布**的单元。
> 禁止跨 Wave 偷做后续逻辑。

### Wave 0 — Security prerequisites（无 schema 变化）

**目标**：在任何模型变化之前，先把已存在的泄漏与测试盲区修掉。这些修复与 v2
**独立成立**——即使 v2 暂缓，它们也应该上线。

**范围**：
- `apps/api/src/modules/identity/user/dto.rs` — 移除 `UsersDto.password`；
  新增 `AdminUserDto`（无 password）用于管理端点。
- `apps/api/src/modules/identity/administrator/dto.rs` — 移除 `SuperAdminDto.password`。
- `apps/api/src/modules/identity/user/mod.rs:33` — 删掉 `user.password = "".to_string()`
  的补丁式写法（DTO 已无该字段）。
- `packages/sdk/src/api/admin/users.ts:9` — 删除 `console.log(res.data)`。
- `packages/sdk/src/api/admin/event_challenges.ts:54` — 删除 `console.log(id_list)`。
- `packages/sdk/src/entity/users.ts` / `entity/super_admin.ts` 的**使用方式**：
  管理/`/me` 响应改用 SDK 手写 DTO 类型（生成实体仍是 DB 列类型，不写回）。
- `apps/api/tests/common/routes.rs` — 补全 admin 路由目录（terminal / dashboard /
  awd / awdp / event-scoped awd+awdp）。**不只**用 `len >= N` 兜底：改为以
  「源码 handler 清单 ↔ 目录条目」的表驱动一致性为准（新增源码路由未登记即失败）。
- 新增「响应无 password」测试（§21.2）。
- **修复 `/api/admin/download?key=` 对象级授权（§7.6，必做）**：
  新增 `resolve_private_artifact()`（key → `event_id` → 授权锚定），
  拒绝无锚点/穿越形态/未知 key，权限归属修正为 `events.read`。
  **禁止**只做 `starts_with("writeups/")` 就宣称修复完成。
- 记录 §3 P6（HTTP 200 + code != 0）为硬约束文档 + 一个守卫测试。

**验收 Gate**：
- 全仓任何 admin 端点 2xx 响应体无 `password` 键、无 `$argon2` 子串。
- `apps/api/tests/common/routes.rs` 覆盖**全部** admin handler（含
  `/api/admin/terminal/*`、`/api/admin/dashboard/summary`、`/api/admin/awd/*`、
  `/api/admin/events/*/awd/*`、`/api/admin/events/*/awdp/*`），且与源码清单一致。
- `/api/admin/download` 的 8 条授权测试全部通过（§7.6 清单）；任意对象 key 直通已关闭。
- `mise run check` 全绿；`pnpm --filter @floatctf/sdk test` 全绿。

**不做**：不新增表/列；不动 `super_admin`；不改路由。

### Wave 1 — RBAC schema + Permission catalog

**范围**：migration **M1**；`mise run db:gen`；`apps/api/src/core/permission.rs`
（`Permission` enum + `domain()` + `high_risk()` + `ALL`）；`authorization/` 模块骨架；
`define_permissions!` 宏（或等价生成）；单测（§21.1 前 4 行）。

**验收 Gate**：
- M1 在 fresh 与既有 DB 上都 apply 成功；`db:migration:verify` 无 MODIFIED。
- `mise run db:gen` 后 `git diff` 只出现预期的 3 个新实体 + `mod.rs`/`prelude.rs`；
  **二次 `db:gen` 后工作区干净**。
- 约束正确：`(user_id, role_id)` 与 `(role_id, permission)` 复合主键、
  `platform_roles_key_key` unique、3 个 FK 的 `ON DELETE` 行为、CHECK 格式约束。
- 单测：union / `grants_all` / `from_str` 往返与拒绝 / 去重。

**不做**：不插入任何角色行；不改任何 handler；不动 JWT。

### Wave 2 — Account columns + preflight + super_admin → users（M2, M3）

**范围**：migration **M2**、**M3**；`core/system_ids.rs` 增
`PLATFORM_ROLE_SUPER_ADMIN` / `..._KEY`；无业务代码变化。

**验收 Gate**：
- M2/M3 在 fresh（单超管）与既有 DB（≥1 超管）上都成功。
- 复制正确性：password hash / created_at / updated_at **逐字节相同**；
  `nickname` 合成未破坏唯一性；`avatar IS NULL`；`status='active'`；`token_version=0`。
- `user_platform_roles` 中每个超管恰好 1 行指向 `PLATFORM_ROLE_SUPER_ADMIN`。
- 4 类冲突（username / email / email-within / uuid / nickname）各自能**原子失败**
  且 `schema_migrations` 不记录、消息含具体冲突行。
- 幂等：apply 两次结果一致，无重复行。
- **行为不变证明**：`mise run check` 全绿 + `http_auth_contract.rs` 全绿
  （守卫仍读 `super_admin`，所以行为必须完全一致）。

**不做**：不改守卫；不删 `super_admin`；不改 FK。

### Wave 3 — Repoint admin-actor FKs to users（M4）

**范围**：migration **M4**；`mise run db:gen`（实体关系目标变化）。**零 Rust 业务代码改动。**

**验收 Gate**：
- 7 个 FK 的 `pg_constraint.confrelid` 从 `super_admin` 变 `users`，`confdeltype` 不变。
- `db:gen` 后 `announcements.rs` / `awd_score_events.rs` /
  `awd_internal_token_rotations.rs` / `awd_team_bans.rs` / `awd_reset_records.rs` /
  `logs.rs` 的 `belongs_to` 目标变为 `users`；无编译错误。
- `awd_reset_records_requester_check` 仍存在且语义有效（`requested_by_admin` 现在
  指向 users，但 CHECK 不变）。
- **行为不变证明**：管理端动作（发公告 / 封队 / 调分 / 重置 / 轮换令牌）仍 2xx，
  且对应表的新行 `*_by` 列 = 操作者 `users.id`。`mise run check` 全绿。

**不做**：不改 `logs` 列；不删表；不引入 AccountGuard。

### Wave 4 — Authentication v2（TokenPurpose + 单会话 + AccountGuard）

**范围**：
- `core/security/jwt.rs`：`TokenPurpose`；`AuthClaims{sub, purpose, token_version, exp}`；
  **删除 `Role` 与 `AwdJudger`**；`gen_session_token` / `gen_reset_token`。
- `api/extractor/auth.rs`：`AccountGuard`（含 status + token_version 校验）；
  `RequireSuperAdmin`（Wave 4 过渡，语义 = Wave 3 的超级管理员）；
  删除 `UserJwtGuard` / `SuperAdminJwtGuard`。
- `modules/identity/authentication/mod.rs`：`POST /api/session`；
  `/api/users/session`、`/api/admin/session` 作为过渡别名（同一 handler）。
- `modules/identity/user/mod.rs`：`get_me` 返回新 `MeDto`（含 roles/permissions）；
  改密时 `token_version += 1`。
- `modules/identity/administrator/mod.rs`：**删除** `super_admin_login`；
  `/api/admin/super_admin*` CRUD 暂时**保留**（读 `users` 而非 `super_admin`），
  以便默认前端不立刻坏掉——但它改为要求 `RequireSuperAdmin`。
- `modules/event/awd/api/player.rs` 等 SSE：`AccountGuard`。
- 桌面终端：`create_terminal_session` / `terminal_ws` 用 `AccountGuard`
  + （Wave 4）`RequireSuperAdmin`，并在 **ticket 消费时**重新校验账号
  （不只校验 ticket 存在）。
- `apps/api/tests/regression_security_concurrency.rs`：删除 `AwdJudger` 两个用例。

**验收 Gate**：
- **授权语义与 Wave 3 完全等价**：`http_auth_contract.rs` 全绿；
  「超管 token → 全部 admin 路由 2xx/功能码；玩家 token → 全部 admin 路由 403」。
- 单会话：一次登录的 token 同时可用于 `/api/**` 与 `/api/admin/**`。
- 三种 token 用途隔离：`PasswordReset` token 打 `/api/admin/**` → 401。
- `token_version`：改密后旧 token → 401；新登录 → 2xx。
- `status=disabled` → 403（**不是** 401）。
- `AwdJudger` 全仓零命中（含测试）。
- `mise run check` 全绿 + Jeopardy/AWD/AWDP 回归全绿。

**不做**：不引入 per-permission 检查（Wave 5）；不删 `super_admin` 表。

### Wave 5 — Authorization（AuthContext + 权限解析 + 逐路由权限）

**范围**：
- `authorization/` 模块：`AuthContext`、`effective_permissions()`、
  `RequirePermission<P>`（宏生成 29 个）；
  删除 `RequireSuperAdmin`。
- 逐文件替换约 100 个 handler 的守卫为对应 `RequirePermission<P>`
  （映射见 [platform-permissions.md](./platform-permissions.md)）。
- 新增端点：`GET /api/admin/permissions`、`GET/POST/PATCH/DELETE /api/admin/roles*`、
  `GET/PUT /api/admin/users/{id}/roles`。
- 反提权（§20.2）+ last super-admin（§20.1）+ self-lockout（§20.3）。
- `GET /api/admin/users` DTO 增加 `roles: [{key,name}]` 与 `status`。
- 审计动作扩展（§15.3）。

**验收 Gate**：
- `http_auth_contract.rs` 扩展为**表驱动三态**：匿名 401 / 无权限 403 / 有权限非 403，
  对**每条** admin 路由（≥150 条）。
- §21.6 全部 8 个角色场景通过。
- §20.1 的 6 条路径、§20.2 的 6 条操作、§20.3 的 4 条自我锁定规则全部有测试且通过。
- 默认前端在 Wave 5 结束时**功能不回归**（它还带着 adminToken，能打到
  `RequirePermission` —— 因为超管拥有全部权限）。这是 Wave 5 必须保持的兼容性。

**不做**：不改 SDK（Wave 7）；不改前端（Wave 8）；不删 `super_admin` 表。

### Wave 6 — Audit actor unification（M5 + 109 调用点）

**范围**：migration **M5**；`LogService::add_log` / `AuditService::record` 签名；
108 个具体调用点；`logs_dto.rs` / `operations/logs.rs` 的筛选键
（`superadmin_id` → `actor_user_id`）+ 新增 `actor_username` 计算字段；
`mise run db:gen`。

**验收 Gate**：
- `pg_constraint` 无 `superadmin_id`；`logs` 无 `user_id` / `superadmin_id` 列；
  `idx_sys_logs_user_op` 已删、`idx_logs_actor_op` 存在。
- 回填无损：三类历史行分别回填正确（§21.3）。
- 双主体行会导致 M5 原子失败（构造测试）。
- 全仓 `grep -rn 'superadmin_id' apps/api/src --include=*.rs` → 0 命中。
- 管理端日志页与筛选仍可用；`actor_username` 正确。
- `mise run check` 全绿。

**不做**：不删 `super_admin` 表（还有代码在读它）；不改 SDK/前端。

### Wave 7 — SDK contract v2

**范围**：`packages/sdk`（`getToken` + fallback、`onForbidden`、
`FloatCTFError.forbidden`、`FloatCTFTransportScope`、
`floatCTFErrorFromEnvelope` 接线、SSE 401/403 拆分、`MeDto`、
`HIGH_RISK_PERMISSIONS`、roles/permissions/users-roles API、terminal API、
补齐 `logs/{id}` / `scheduled_tasks/{id}` / `users/{id}`）+ `packages/react`
（`useToken`）。

**验收 Gate**：
- §21.4 全部通过。
- `mise run web:typecheck` 与 `mise run web:architecture` 通过。
- **旧 alias 仍工作**（`getUserToken` / `getAdminToken` / `FloatCTFAuthScope`），
  且新增测试证明两者都走单 token 路径。

**不做**：不改 Default 前端逻辑（Wave 8）；不删 alias（Wave 10）。

### Wave 8 — Default Frontend v2

**范围**：`AuthStore` v2（`version: 2` + `migrate` + `partialize`）；
`api/client.ts`（单 token + `onForbidden`）；`api/react.ts`（`useToken`）；
`routes/index.tsx`（调 `/api/session`）；删除 `routes/admin/index.tsx`；
`routes/admin/route.tsx`（权限感知守卫 + 无 `adminIgnoreRoutes`）；
`navigation/admin-navigation.tsx`（`isVisible` + Roles 改名/新增）；
`navigation/sidebar-*`（接线 `visibility`）；
新增 `/admin/roles`、`/admin/roles/{key}`、`/admin/users/{id}`；
`super_admins.tsx` → 删除（其形态并入 Roles 页）；
权限 hook + `MissingPermission` 组件 + 高危标注 + `useConfirm` 二次确认。

**验收 Gate**：
- §21.5 全部通过；§21.6 的角色场景**从 UI 走一遍**（导航可见性 / 按钮态 / 403 提示）。
- persist 迁移测试：旧 `auth-storage` → 新 state 无 `adminToken`。
- `pnpm --filter @floatctf/web build`（`vite build && tsc`）成功；
  既有的 218 个前端单测全绿。
- `frontends/default/**` 内无原生 `alert(`/`confirm(`。

**不做**：不碰 submodule 前端（Wave 9）。

### Wave 9 — External frontends + docs + contract version

**范围**：
- `frontends/floatctf-frontend-xzmu` 与 `frontends/floatctf-frontend-shadcn`
  各自仓库内改造（单 token、`onForbidden`、权限导航），然后**在主仓库 bump submodule 指针**。
- `apps/api/src/core/contract.rs` 与 `packages/frontend-runtime/src/version.ts`：
  `API_CONTRACT_VERSION = "2"`（同步改，否则 `check-architecture.sh` rule #7 失败）。
- 文档：`docs/frontend/AI-FRONTEND-GUIDE.md`、`CAPABILITY-MATRIX.md`、
  `packages/sdk/README.md`、`docs/frontend/ARCHITECTURE.md`（§18.3）；
  `docs/agents/ARCHITECTURE.md:57` 的 identity 模块行更新；
  新增 `docs/agents/BACKUP.md`（修既有 dangling 引用，见 §26 R8）。
- 补齐 `AGENTS.md` 命令速查里的 `db:migration:list|status|verify`（§26 R9）。

**验收 Gate**：
- `mise run web:architecture` 通过（契约版本一致）。
- 两个 submodule 前端 `tsc --noEmit` + `vite build` 成功，并且
  **在 v2 平台上完成一次登录 + 一次管理操作**（各自仓库的验收脚本）。
- 主仓库 `git submodule status` 指向新的提交，且新提交在其仓库中已 push。
- 三个前端的 `CAPABILITY-COVERAGE` 各自的 RBAC 行标记为覆盖。

**不做**：不删 alias（Wave 10）。

### Wave 10 — Cleanup legacy（M6 + 删除全部 alias）

**范围**：
- migration **M6**：删 trigger + `super_admin` 表。
- 删除 `entity/super_admin.rs` / `packages/sdk/src/entity/super_admin.ts`（`db:gen` 自动）。
- 删除 `/api/admin/super_admin*` 端点与 `identity/administrator/` 模块。
- 删除 `POST /api/users/session` / `POST /api/admin/session` 别名。
- 删除 SDK `getUserToken` / `getAdminToken` / `FloatCTFAuthScope` /
  `client.admin.login`；删除 `@floatctf/react` 的 `useUserToken`/`useAdminToken`。
- 删除前端 `adminToken` / `setAdminToken` / `removeAdminToken` 残留。

**验收 Gate（§39 的完整前置证明，缺一不可）**：
1. `grep -rn 'super_admin' apps/api/src --include=*.rs` → 0
2. `grep -rn '/super_admin' apps/api/src --include=*.rs` → 0
3. `grep -rn 'super_admin\|SuperAdmin' packages/sdk/src --include=*.ts` → 0
4. `grep -rn 'super_admins\|super_admin' frontends/ --include=*.ts --include=*.tsx` → 0（三前端）
5. `grep -rn 'getAdminToken\|adminToken\|FloatCTFAuthScope\|getUserToken' packages/ frontends/ apps/` → 0
6. `pg_constraint` 中 `confrelid = 'public.super_admin'` → 0 行
7. `SELECT to_regclass('public.super_admin')` → NULL
8. `mise run check` 全绿；三个前端 build 全绿；`mise run web:architecture` 全绿
9. `mise run db:migration:verify` 无 MODIFIED

**不做**：不引入新的兼容层。

### Wave 11 — Full regression / migration / E2E

**范围**：无新功能。执行：
- fresh install 全流程（含 `merged.sql`）→ 54 条 migration → 平台可用。
- 升级流程：用**上一版**安装器 + 真实数据（含 3 条超管 + 冲突域名）→ 备份 →
  升级 → 验证 §22.2 预检与 §21.3 全部。
- 全量测试矩阵（§21 全部小节）。
- 回滚演练：备份 → 升级 → `restore.sh` 回到备份 → 平台可用且 schema 回到 v1。
- 两个 submodule 前端 + Default 的三方一致性冒烟。

**验收 Gate**：§21 全绿；升级/回滚演练各有一次完整记录（命令 + 输出）。

---

## 24. Per-wave Acceptance Gates（汇总）

| Wave | 一句话 Gate | 禁止偷做 |
|---|---|---|
| 0 | 无任何响应含 password hash；admin 路由目录 ≥150 且覆盖高危面；`check` 全绿 | 不动 schema、不动路由、不改 JWT |
| 1 | M1 apply + `db:gen` 二次干净 + 约束正确 + 权限单测全绿 | 不插角色行、不改 handler |
| 2 | M2/M3 成功且**逐字节保留** hash/时间戳；5 类冲突可原子失败；行为不变 | 不改守卫、不删表、不改 FK |
| 3 | 7 个 FK 目标变 users 且 `confdeltype` 不变；**零代码改动**；行为不变 | 不动 logs、不引 AccountGuard |
| 4 | 单 token 同时通 `/api/**` 与 `/api/admin/**`；**授权语义与 Wave 3 逐条等价**；token 用途隔离；disabled→403 | 不加 per-permission、不删表 |
| 5 | 表驱动三态（401/403/非403）覆盖 ≥150 条；8 角色 E2E；反提权 + 最后超管 + 自我锁定全测 | 不改 SDK/前端 |
| 6 | logs 无旧列/旧索引；回填无损；双主体原子失败；`grep superadmin_id` → 0 | 不删表 |
| 7 | SDK 单 token + 403 区分 + envelope 接线 + 新 DTO/API；旧 alias 仍工作 | 不改前端逻辑、不删 alias |
| 8 | 单登录 + 权限导航 + Roles/Users UI；persist 迁移测试；build + 218 测试全绿 | 不碰 submodule |
| 9 | 契约版本一致（`"2"`）；两个 submodule 前端在 v2 平台跑通并 bump 指针 | 不删 alias |
| 10 | §23 Wave 10 的 9 条前置证明**全部**成立 | 不留任何 alias |
| 11 | §21 全绿 + 升级/回滚演练各一次 | 不加功能 |

**「禁止『后面一起测』」原则**：每个 Wave 必须独立通过自己的 Gate 才能开始下一个。
特别是 Wave 4 的「授权语义等价」Gate —— 它是整个计划里最关键的一道闸门：
如果 Wave 4 让管理端授权弱于 Wave 3，就是**安全回归**；强于 Wave 3 则是**功能回归**。

---

## 25. Commit Plan

按仓库提交规范（中文 message + `feat/fix/chore/docs/refactor/test` 前缀），
**按 Wave 分批**，每批可含多个提交但同一 Wave 内不得跨 Wave。建议：

```text
# Wave 0
fix(api): 移除账户响应中的密码哈希字段
fix(sdk): 删除管理端用户列表的 console 输出
test(api): 补全管理端路由鉴权目录并覆盖 terminal/awd/awdp
docs(security): 明确权限失败必须返回 403 而非 200 业务码

# Wave 1
feat(auth): 新增平台 RBAC 数据模型（roles/assignments/permissions）
feat(auth): 新增权限目录与声明式权限守卫生成宏

# Wave 2
feat(auth): 新增账号 status 与 token_version 列
feat(auth): 迁移超级管理员账号到统一 users 表（含冲突预检）

# Wave 3
refactor(db): 管理动作外键统一指向 users

# Wave 4
refactor(auth): 用 TokenPurpose 替换 Role 并统一会话 JWT
feat(auth): 新增 AccountGuard 与单一登录端点
feat(api): /me 返回账号角色与有效权限
refactor(auth): 删除未使用的 AwdJudger 角色

# Wave 5
feat(auth): 新增 AuthContext 与每请求权限解析
refactor(api): 管理端路由改为声明式权限守卫
feat(api): 新增角色管理与账号角色分配端点
feat(auth): 增加反提权、最后一个超管与自我锁定保护

# Wave 6
refactor(audit): 审计主体统一为 actor_user_id

# Wave 7
refactor(sdk): 统一会话 token 并区分 401/403
feat(sdk): 新增 /me、角色与终端 API
fix(sdk): 接线平台业务失败信封守卫

# Wave 8
refactor(frontend): 切换统一账户会话与权限导航
feat(frontend): 新增角色与账号权限管理界面

# Wave 9
refactor(contract): API 契约版本升至 2 并同步前端运行时
docs(frontend): 更新认证与权限公共契约
chore(frontends): 更新 xzmu / shadcn 前端 submodule 指针
docs(agents): 补充 BACKUP.md 与迁移命令速查

# Wave 10
refactor(auth): 移除 legacy super_admin 身份体系与全部兼容别名
chore(db): 删除 super_admin 表

# Wave 11
test(auth): 补统一账户与 RBAC 端到端覆盖
```

**每个 Wave 收尾必须**：`cargo fmt --all && cargo check -p floatctf`；
前端改动额外 `tsc --noEmit` + `vite build`；`mise run check` 全绿才推送。

---

## 26. Risks

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| R1 | **username / email / nickname 冲突导致升级中断** | 升级失败、需要人工介入 | §13.4 preflight 在**最早**失败并列出具体行；§22.2 提供升级前预检 SQL；**绝不自动 rename** |
| R2 | **`users.nickname` 是 NOT NULL + UNIQUE，超管没有 nickname** | 合成可能冲突 | preflight 断言 `super_admin.username` 不与既有 `users.nickname` 冲突；失败即人工决策（可改名玩家昵称或超管用户名） |
| R3 | **两个 submodule 前端在独立仓库，无法与主仓库原子切换** | 若走原子切换则不可执行 | §12.6 选 Option A（有强制拆除 Wave 的分段切换）；Wave 9 明确要求 submodule 新提交在其仓库先 push |
| R4 | **Wave 4 若把管理端授权放宽为「已认证」** | 严重安全回归（任何玩家可管理平台） | `RequireSuperAdmin` 过渡守卫 + Wave 4 的「授权语义等价」Gate（§24） |
| R5 | **403 被当成 401 处理** | RBAC 一上线，权限不足的用户被反复踢下线 | §11.3 精确定义 401/403；SDK 新增 `onForbidden`；§21.4/§21.5 专门测试 |
| R6 | **并发绕过 last super-admin 检查** | 平台失去唯一可管理账号 → 不可恢复 | 应用层 `SELECT … FOR UPDATE` 序列化 + DB trigger 兜底（§20.1）；并发测试 |
| R7 | **`users.token_version` 被误当权限版本使用** | 引入权限缓存后语义纠缠 | §9.4 明文禁止；§19 不变式 11 |
| R8 | **`docs/agents/BACKUP.md` 被 `backup.sh` / `restore.sh` 引用但不存在** | 升级/回滚时运维找不到文档 | Wave 9 补齐（顺手修既有 dangling 引用） |
| R9 | **`gen_entities.py` 不校验 `sea-orm-cli` 版本** | 误用 2.0.1 会导致全项目编译失败（`rs_type = "Enum"` 不兼容） | Wave 1/2/3/6 每次 `db:gen` 前人工确认 1.1.20；建议 Wave 9 顺手加断言（低成本） |
| R10 | ~~`announcements.publisher_id NOT NULL CASCADE`~~ | ~~删账号会连带删公告~~ | **已解决**：Q2 决定改为 `NULL` + `ON DELETE SET NULL`（纳入 M4，§13.6）。剩余工作量 = entity/DTO/SDK/三前端跟着把 `publisher_id` 变成可空并加「已注销账号」fallback |
| R11 | **`/api/admin/download?key=` 不校验 key 归属**（IDOR 性质：可读私有桶任意对象） | ~~有 `challenges.read` 者可读任意私有对象键~~ | **已在 Wave 0 关闭**：新增 `resolve_private_artifact()` 对象级授权（key → event_id → `events.read`），权限归属修正为 `events.read`（§7.6）。测试见 §7.6 / §21.2 |
| R12 | **权限解析引入 N+1 或慢查询** | 管理端变慢 | 明确最多 2 条窄查询（§10.1）；若成为瓶颈再按 §10.3 的门槛引入缓存 |
| R13 | **HEAD 持续推进导致清单漂移** | FK / 路由清单过期 | 所有清单标注核验 HEAD；实现每个 Wave 前**重跑** §14 末尾的两条扫描命令 |
| R14 | **`AwdJudger` 删除后有人依赖它** | 编译失败（好）或语义缺失（坏） | 已核验全仓无构造点；删除由类型系统强制，测试同步清理 |
| R15 | **审计主体回填有损** | 历史审计归错人 | 源码核验 0 处双主体 + M5 内的 `RAISE EXCEPTION` 断言 + 三类行的 migration 测试 |
| R16 | **前端 persist 未加 `version`/`migrate`** | 旧 `adminToken` 残留、误导性登录态 | §17.1 硬要求 + §21.5 persist 迁移测试 |
| R17 | **`API_CONTRACT_VERSION` 未同步两处** | `check-architecture.sh` rule #7 失败、前端兼容性误判 | Wave 9 同一提交改两处 + Gate |
| R18 | **删账号导致审计主体变 NULL 而丢失可追踪性** | §19 不变式 10 受损 | 推荐用 `status=disabled` 替代删除；`ON DELETE SET NULL` 保证流水不消失 |

---

## 27. Resolved Product Decisions

**Open Questions = 0。** 三个曾需产品决策的问题已全部固化；其余问题均由源码/既有约定
判定，结论直接写在各章正文，不在此重复。

### Q1 — `super-admin` 之外的 5 个模板角色是否内置？ → **已决定：不内置**

```text
只内置：super-admin（protected=true, grants_all=true）
以下全部只是推荐模板，不是 protected built-in，DB 不 seed：
  event-manager
  challenge-manager
  user-manager
  ops-manager
  auditor
```

- **DB 不 seed 这 5 个模板**；`M3` 只插入 `super-admin` 一行。
- Roles UI 提供**「从模板创建」**：以模板权限集为初值创建一个普通角色，
  创建后 `protected = false`，用户可自由增删权限、改名、删除。
- 因此本计划不承担任何「不可变组织角色」的永久兼容义务。
- 依据与取舍见 §8.1 / §8.2；模板权限集见 §8.2 表。

### Q2 — 删除账号时，其平台公告如何处理？ → **已决定：公告保留，`publisher_id` SET NULL**

```text
announcements.publisher_id
  → NULL
  → REFERENCES users(id)
  → ON DELETE SET NULL
```

- 纳入 **M4**（不另开 M7），见 §13.6。
- 理由：删除/清理账号不应删除平台内容；统一 Account 后 publisher 是**历史 actor**
  而非内容 owner；审计与内容历史优先保留。
- 展示：继续使用既有的 `publisher text` 快照列（**不新增** snapshot 字段）；
  `publisher_id IS NULL` 时 fallback 文案为**「已注销账号」**。
- M4 之后 7 个 actor 列的删除语义**统一为 SET NULL**，不再有 `CASCADE` 例外。

### Q3 — 「吊销该账号全部会话」的用户可见入口是否现在做？ → **已决定：后端现在做，UI 不阻塞 v2**

**本次必须完成（后端）**：

```text
改密码            → token_version += 1
disable 账号       → token_version += 1
管理员显式 revoke sessions（若 Wave 5 的账号 API 设计已含该动作）→ token_version += 1
```

**本次不要求**：

```text
普通用户「安全设置 → 吊销全部设备」专门 UI
```

- 理由：`token_version` 的**能力**是安全不变量的必要条件（§19 不变式 11/12），
  必须现在做；而「设备管理」是一个独立的产品功能面，会显著扩大 v2 的 UI 与测试范围。
- 由于改密本身即 `+= 1`，用户**已经有**一条可用的自助吊销途径，不构成安全缺口。
- 除非实现成本极低且不扩大范围，本次不额外做该 UI。

---

## 28. Final Recommendation

对 Prompt §43 的 11 个问题给出明确答复，**不含「都可以，看需求」**。

| # | 问题 | 结论 | 一句话理由 |
|---|---|---|---|
| 1 | 是否统一 `users` 与 `super_admin` | **是** | 管理员是「持有平台角色的账号」，不是「另一种账号」；不统一则 RBAC 必然要维护两套主体、两套守卫、两套审计列 |
| 2 | 是否保留 `/api/admin` | **是** | 它是**授权域**边界而非身份域边界；合并 URL 只放大破坏面，不带来收益 |
| 3 | 是否保留单独 admin login | **否**（Wave 4 新入口，Wave 10 删除旧入口） | 登录是建立会话，不分身份类型；「能不能进后台」由权限决定 |
| 4 | JWT 是否携带 roles/permissions | **否** | 撤权即时性与 `exp`（默认 8h）不可调和；权限只做每请求 DB 解析 |
| 5 | 第一版是否用 Redis permission cache | **否** | 每请求最多 2 条窄查询；正确性优先，并给出可测量的引入门槛（§10.3） |
| 6 | 是否加入 `token_version` | **是** | 改密/禁用/吊销必须让旧会话失效；今天无 logout 端点，不加就是长期缺口。它管**凭据**，与 RBAC revision 分开 |
| 7 | 是否加入 account status | **是**，`active` / `disabled` 两值 | 需要「禁用而不删除」以保住审计主体（删账号会把 `actor` 置 NULL）。不引入更多状态，避免过度设计 |
| 8 | Super Admin 如何建模 | `platform_roles` 一行：`protected=true` + `grants_all=true` | 若靠 permission 行集合，删一行或漏一行就会让最后管理员变砖；`grants_all` 把「含将来新增权限」变成结构性保证 |
| 9 | Permission 用 DB catalog 还是代码 enum/string | **代码 enum + 稳定字符串（DB 只存 text）**；不建 catalog 表 | 代码本来就必须知道每个权限；catalog 表只会引入第二真相源与「每加权限一条 migration」的成本。orphan 由写路径校验 + 启动检测兜住 |
| 10 | 是否需要 `API_CONTRACT_VERSION` major bump | **是，`"1"` → `"2"`** | 删除 `/api/admin/session`、`/api/admin/super_admin*`、改 `/api/users/me` 结构、改 SDK `getUserToken`/`getAdminToken` ——旧前端**会坏**，正是 `contract.rs` 定义的升 major 条件。**不是**因为 DB schema 变了 |
| 11 | 现在做还是 v1.1 做 | **现在做**（并按 §23 的 12 个 Wave 分批落地） | 三个前端都还在同一发布节奏内（两个 submodule 可控），GA 之后外部第三方前端会让同一次破坏性变更的成本成倍上升；且 Wave 0 的三项安全修复（hash 泄漏、console 泄漏、测试盲区）无论如何都要现在做 |

### 28.1 决策 9（Permission 存储）的取舍说明

推荐 **B（代码 enum）**。若未来出现「运维需要在 UI 里新增一个平台不认识的自定义权限」
这种需求（本计划认为**不应该**出现——权限必须由代码执行，DB 里的字符串没有执行者），
再考虑引入 catalog。**判据**：权限是**代码概念**，不是**数据概念**。

### 28.2 决策 10 的边界（重要）

**不要因为内部 DB schema 变化就升 `API_CONTRACT_VERSION`。** 本计划的 M1–M6
全部是内部变化；升 major 的**唯一**原因是公共 API 契约被破坏：

```text
破坏项（导致升 "2"）：
  - POST /api/admin/session        删除
  - POST /api/users/session        语义改（仍是登录，但成为别名）
  - /api/admin/super_admin*        删除（5 个端点）
  - GET /api/users/me              响应结构改（+roles +permissions +status，
                                   且 SDK 类型从 Users 变为 MeDto）
  - SDK getUserToken/getAdminToken 删除
  - SDK FloatCTFAuthScope          改名
  - @floatctf/react useUserToken/useAdminToken  删除

非破坏项（留在 "2" 内，加法式）：
  - 新增 /api/session
  - 新增 /api/admin/roles*、/permissions、/users/{id}/roles
  - 新增 MeDto 的字段（若未来再加）
  - 新增权限（permissions 数组变长）
```

---

## 附录 A — 影响面矩阵（§4）

| Area | Current Design | Target Design | Breaking? | Migration Needed? | Files / Tables |
|---|---|---|---|---|---|
| Database schema | `users` + `super_admin` 双表 | 单 `users` + 3 张 RBAC 表 | 内部变化（非 API breaking） | **是**（M1–M6） | `users`、`super_admin`、新增 3 表 |
| Authentication | 两个登录端点 / 两个 `Role` | `TokenPurpose` + `POST /api/session` | **是** | 否（JWT 无 schema） | `authentication/mod.rs`、`administrator/mod.rs`、`jwt.rs` |
| JWT claims | `{sub, role, exp}` | `{sub, purpose, token_version, exp}` | **是**（token 失效，需重登） | 否 | `core/security/jwt.rs` |
| Guards | `UserJwtGuard` / `SuperAdminJwtGuard` | `AccountGuard` + `RequirePermission<P>` | 否（内部） | 否 | `api/extractor/auth.rs`、`api/prelude.rs`、`api/mod.rs`、~35 文件 |
| Authorization module | 3 行 re-export | `AuthContext` + 解析 + 29 守卫 | 否 | 否 | `modules/identity/authorization/**` |
| Admin APIs | ~100 handler，全超管 | 逐路由 permission | **是**（403 语义） | 否 | 见 [platform-permissions.md](./platform-permissions.md) |
| Player APIs | `UserJwtGuard` | `AccountGuard` | 否（语义不变） | 否 | `service/*`、`event/*/api/player.rs` |
| `/me` | `UsersDto`（含 password） | `MeDto`（+roles +permissions，无 password） | **是** | 否 | `user/dto.rs`、`user/mod.rs` |
| Audit logs | `user_id` + `superadmin_id` | `actor_user_id` | 否（内部） | **是**（M5） | `logs`、`logging.rs`、`audit/service.rs`、109 调用点 |
| Announcements publisher | FK → `super_admin` | FK → `users` | 否 | **是**（M4） | `announcements`、`announcements/api.rs`、`dto.rs` |
| AWD admin actor FKs | 3 个 FK → `super_admin` | → `users` | 否 | **是**（M4） | `awd_score_events`、`awd_team_bans`(×2)、`awd_internal_token_rotations` |
| AWDP admin actor FKs | 无独立列（经 `events` 域） | — | 否 | 否 | `awdp_*`（`awd_reset_records.requested_by_admin` 覆盖重置） |
| Event management | 超管全域 | `events.*` 4 权限 | **是** | 否 | `event/common/api/**` |
| Event membership | `event_users/teams/members` | **不变** | 否 | 否 | 同上（一行不改） |
| Settings | 超管 | `settings.read` / `.manage` | **是** | 否 | `platform/settings/api.rs` |
| Frontend mgmt | 经 `settings` 的 `FRONTEND_ACTIVE` | 同（`settings.*` 覆盖） | 否 | 否 | `platform/frontend/api.rs` |
| Docker / Terminal / Database ops | 超管 | `docker.*` / `terminal.open` / `database.execute` | **是** | 否 | `platform/operations/{docker,terminal,database}.rs` |
| SDK transport | `getUserToken` + `getAdminToken`，`FloatCTFAuthScope` | `getToken`，`FloatCTFTransportScope`，+`onForbidden` | **是** | 否 | `packages/sdk/src/{transport,client,errors}.ts` |
| SDK auth API | `adminAuth.login` + `users.login` | 单 `login` → `/api/session` | **是** | 否 | `api/admin/auth.ts`、`api/service/users.ts` |
| SDK entities | 复用含 `password` 的实体作 DTO | 手写 DTO（`MeDto` 等） | **是**（类型） | 否 | `entity/users.ts`、`entity/super_admin.ts` |
| Default AuthStore | `token` + `adminToken` | 单 `token` + `account` | **是**（需 persist migrate） | 否 | `stores/AuthStore.ts`（+11 个读取点） |
| Default routing | 双登录页 + 单一 presence 守卫 | 单登录 + 权限守卫 | **是** | 否 | `routes/index.tsx`、`routes/admin/route.tsx`、删 `routes/admin/index.tsx` |
| Default capability model | 无（导航全显） | 权限驱动（用既有 `filterNavigationSections`） | 否（纯新增接线） | 否 | `navigation/{admin-navigation,sidebar-types,sidebar-utils}.tsx`、`components/SideBar.tsx` |
| Default admin pages | 56 个路由文件 | +Roles/{key}、+Users/{id}、删 super_admins | **是** | 否 | `routes/admin/**` |
| E2E auth tests | 94 条 admin 路由，有盲区 | ≥150 条 + 三态 | 否 | 否 | `tests/common/routes.rs`、`http_auth_contract.rs` |
| Seed data | baseline 固定 sysadmin（nil UUID） | 同（不改 seed）+ M3 复制 | 否 | **是**（M3） | `20260810121926-initial-data.sql`（不改） |
| installer / bootstrap admin | 无安装器逻辑（靠 seed） | 同 | 否 | 否 | `scripts/install.sh`（不改） |
| backup / restore | `BACKUP_FORMAT_VERSION=1` | 同（备份含迁移后的库） | 否 | 否 | `scripts/{backup,restore}.sh`（不改） |
| migration tooling | forward-only，48 条 | +6 条 = 54 | 否 | — | `apps/api/src/sql/migrations/` |
| release compatibility | `API_CONTRACT_VERSION="1"` | `"2"` | **是** | 否 | `core/contract.rs`、`packages/frontend-runtime/src/version.ts` |
| Docs | 无身份/RBAC 文档 | 本计划 + `platform-permissions.md` + 4 篇更新 | 否 | — | `docs/**` |

### 预计 breaking surfaces 汇总（10 个）

```text
B1  POST /api/admin/session                删除
B2  POST /api/users/session                变别名 → 删除
B3  /api/admin/super_admin*（5 端点）       删除
B4  GET /api/users/me                      响应结构变化
B5  admin 端点新增 403 语义                 行为变化
B6  SDK getUserToken / getAdminToken        删除
B7  SDK FloatCTFAuthScope                   改名
B8  @floatctf/react useUserToken/useAdminToken  删除
B9  Default AuthStore 字段                  变化（localStorage）
B10 API_CONTRACT_VERSION                    1 → 2
```

---

## 附录 B — Case A–F 反向验证（§44）

### Case A — 普通用户被授予 `event-manager`

| 问题 | 答案 | 依据 |
|---|---|---|
| 不需要第二个管理员账号？ | ✅ 同一个 `users` 行 | §4.1 |
| 不需要第二次登录？ | ✅ 登录不做角色判断，`/me` 每请求重新解析权限 | §9.3 |
| 可以参加赛事？ | ✅ `event_users` / `event_teams` 语义不变 | §5、§21.7 |
| 可以进入管理后台？ | ✅ `/me` 返回非空 `permissions` → 导航 `events.*` 可见 | §17.4 |
| 只能管理赛事？ | ✅ effective = `event-manager` 模板的 14 项 | §8.2 |
| 无法打开 terminal/database？ | ✅ `terminal.open` / `database.execute` ∉ 该角色 → 403 | §7.2、§21.6 |

### Case B — 撤销 `event-manager`

**下一次管理 API 请求必须立即 403。**

- 机制：权限**每请求**从 DB 解析（§10.1），**无缓存**（§10.3）。
- 测试：§21.2「permission revoked → 下一请求 403」；§19 不变式 4。

### Case C — 拥有 `event-manager` + `challenge-manager`

- effective = 两角色权限的**并集**（§10.2）。
- 测试：§21.2「multiple roles union」；§21.6 第 7 行。
- 边界：并集**不会**产生任一角色都没有的权限（无隐式传递）。

### Case D — 尝试删除最后一个 super-admin

- **必须被拒绝**，返回 `409 Conflict`。
- 覆盖 4 条路径（删账号 / 禁用 / 移除角色 / 删角色）+ 2 条角色属性修改（§20.1）。
- DB trigger 兜底防并发（R6）。
- 测试：§21.2「last super-admin」4 条 + §21.1 计数口径单测。

### Case E — `event-manager` 尝试给自己增加 `terminal.open`

- **必须被拒绝**，返回 `403 Forbidden`。
- 规则：`target_role_perms ⊆ actor.permissions`（§20.2）；`event-manager` 的
  permissions 不含 `terminal.open`（§8.2），且该角色**不能**改自己的权限集
  （编辑角色还需 `before(R) ⊆ actor ∧ after(R) ⊆ actor`，而 `event-manager` 既不持有
  `roles.manage`，也不拥有 `terminal.open`，两条路径都 403）。
- 两条独立防线：① 没有 `roles.manage` → 连角色编辑 API 都进不去（403）；
  ② 即便临时给了他 `roles.manage`，`target ⊄ actor` 仍会 403。
- 测试：§21.2「escalation」。

### Case F — AwdJudge token 不得变成普通用户 session

- `Role::AwdJudger` 变体**已删除**（§5.2/§9.1），因此不存在能构造它的代码路径。
- AWD 内部认证走 `AwdInternalAuth` + `AwdInternalPrincipal`，**不产出** `AuthContext`
  （§19 不变式 7）。
- 测试：§21.2「internal token 打 `/api/admin/**` → 401」；
  §21.1「`TokenPurpose` 未知值反序列化失败（不默认为 Session）」。
- 若未来真的需要 JWT 化的服务主体：它属于 `TokenPurpose`，**绝不**属于
  `platform_roles`（Prompt §2.5 的红线）。

---

## 附录 C — 实施前必须重跑的核验命令

HEAD 在本计划准备期间已前进两次（`be44fa0` → `02905ef` → `7c382e6`），
清单可能漂移。**每个 Wave 开工前**重跑：

```bash
# 1) FK 清单（§14）
grep -rn "REFERENCES public.super_admin\|REFERENCES super_admin" apps/api/src/sql/migrations/
grep -rn 'belongs_to = "super::super_admin::Entity"' apps/api/src/entity/

# 2) super_admin 代码耦合面
grep -rn "super_admin\|SuperAdmin" apps/api/src --include=*.rs | wc -l
grep -rln "SuperAdminJwtGuard\|UserJwtGuard" apps/api/src --include=*.rs

# 3) 路由与守卫映射（§7 / platform-permissions.md）
grep -rn '^#\[\(get\|post\|patch\|delete\|put\)' apps/api/src/modules --include=*.rs

# 4) SDK / 前端 auth 面
grep -rn "getUserToken\|getAdminToken\|FloatCTFAuthScope\|adminToken" \
     packages/ frontends/ apps/ --include=*.ts --include=*.tsx | grep -v node_modules

# 5) 密码泄漏回归
grep -rn "pub password" apps/api/src/modules/identity/
grep -rn "console.log" packages/sdk/src --include=*.ts

# 6) 审计主体
grep -rn "superadmin_id\|actor_superadmin_id" apps/api/src --include=*.rs

# 7) 契约版本一致性
grep -rn "API_CONTRACT_VERSION" apps/api/src/core/contract.rs \
     packages/frontend-runtime/src/version.ts

# 8) 基线卫生
mise run db:migration:status      # 期望 Applied 48 / Pending 0
mise run db:migration:verify
```

---

## 29. Implementation Status

> 本节是实现期的**唯一权威进度表**，随每个 Wave 的本地提交更新。
> 判定规则：
> - `PASS` = 实现完成 + Gate 满足 + `mise run check` 全绿 + **已本地提交**；
> - `PENDING` = 尚未开始或未完成；
> - `SKIP (env)` = 因环境限制未能执行（**绝不记为 PASS**）。
>
> 全局约束（全程遵守）：不 push / 不 merge / 不 tag / 不 release / 不发布 npm /
> 不 push GHCR（含两个前端子模块，可本地提交不可 push）；不动生产
> （`127.0.0.1:5433` 的 `floatctf-postgres` 与全部生产容器）；不启动开发 API
> （HANDOFF 记载开发 API 会与生产争抢宿主资源并在约 8 秒内删除重建生产 AWDP
> 练习判题容器）；迁移只新增文件，绝不改写既有迁移。

### 29.1 分支与提交

| 项 | 值 |
|---|---|
| 分支 | `feat/identity-authorization-v2` |
| 起始 HEAD（`main`） | `7c382e6b35a39148cbf6ecb581a6611ac61c673d` |
| 计划修订提交 | `20bc592 docs(auth): 修订身份与授权 v2 计划并固化产品决策` |
| Wave 0 提交 | `0f2544b fix(security): 加固身份响应与下载对象级授权基线` |
| Wave 1 提交 | `2a8ef0a feat(auth): 新增平台 RBAC 数据模型与权限目录` |
| Wave 2 提交 | `28b7a73 feat(auth): 迁移管理员身份到统一 users 表` |
| Wave 3 提交 | `2aaa092 feat(auth): 管理动作主体外键统一指向 users`（amend 后的最终 hash） |
| Wave 4 提交 | 见 `git log`（`feat(auth): 认证 v2 —— 令牌只表达身份，授权每请求从 DB 解析`） |
| shadcn 子模块本地提交 | `6756a1d`（Wave 0）、`5e816b4`（Wave 3）——**均未 push** |
| xzmu 子模块本地提交 | 无（Wave 9 起涉及） |

### 29.2 Wave 进度

| Wave | 内容 | 状态 | 关键证据 |
|---|---|---|---|
| 0 | 计划修订 + 安全基线（身份响应去 password、`/api/admin/download` 对象级授权、SDK 去 `console.log`） | **PASS** | 19 文件 +1628/−145；`identity_wave0_security.rs` 6 项测试 |
| 1 | M1 RBAC schema + 权限目录 + 内置角色 reconcile | **PASS** | M1 已 apply；15 单元 + 6 schema + 6 角色/解析测试 |
| 2 | M2 账号列与冲突预检 + M3 `super_admin → users` 迁移 | **PASS** | dev 库实测 hash/`created_at` 逐字节一致；9 项迁移契约测试 |
| 3 | M4 管理动作主体外键重指向 `users`（含 `announcements` SET NULL） | **PASS** | 6 FK 全部 `users` + `confdeltype='n'`；7 项测试 |
| 4 | Auth v2：`TokenPurpose` / 身份-only JWT / `AccountGuard` / `/api/session` / `status`·`token_version` 强制 | **PASS** | 13 JWT 单元 + 22 认证契约测试；`jwt_guard_isolation_matrix_v2` 与 `terminal_session_authz_matrix` **实跑通过**（进程内 `init_service`，未绑定端口） |
| 5 | 授权 v2：`AuthContext` / `RequirePermission<P>` / 角色与权限管理 API / 反提权 / 最后一个超管并发 / 自锁死防护 | **PENDING** | — |
| 6 | M5 `logs.actor_user_id` 合并 + 109 处调用点 | **PENDING** | — |
| 7 | SDK v2（`getToken` / `onForbidden` / `FloatCTFError.forbidden` / SSE 401·403 分流 / `MeDto`） | **PENDING** | — |
| 8 | Default Frontend v2（AuthStore v2 + `version:2` 迁移 / 单登录 / 权限感知导航 / 角色·用户 UI） | **PENDING** | — |
| 9 | 两个外部 Frontend（shadcn / xzmu）+ 子模块 gitlink | **PENDING** | — |
| 10 | M6 删除 `super_admin` 表 + legacy 别名移除 + `API_CONTRACT_VERSION="2"` | **PENDING** | — |
| 11 | 全量回归：fresh DB / upgrade / rollback / Jeopardy / AWD / AWDP | **PENDING** | — |

### 29.3 已完成的 migration

| 版本 | 名称 | 状态 |
|---|---|---|
| `20261009142559` | `add-platform-rbac` | 已 apply（dev 库） |
| `20261009143746` | `identity-account-columns-and-preflight` | 已 apply（dev 库） |
| `20261009143806` | `migrate-super-admin-to-users` | 已 apply（dev 库） |
| `20261009144648` | `repoint-admin-actor-fks-to-users` | 已 apply（dev 库） |

`mise run db:migration:verify` 通过；`db:gen` 连续两次运行零差异。

### 29.4 必须诚实记录的环境限制

- **所有 live-API 测试软跳过**（`http_auth_contract.rs` 的 3 项 live 断言、
  `tests/common` 的 API 依赖用例）：本机禁止启动开发 API（见上），
  因此这些用例以软跳过方式结束。**这不是 PASS**，最终报告中按 SKIP 如实标注。
- Rust 集成测试由 `scripts/test-rust.sh` 建**一次性数据库**运行
  （`[rust-test] PASS: disposable database ... removed`），DB-gated 测试因此
  确实执行而非跳过；但 `SKIP (env)` 仍适用于上面那条。
- 子模块既有 gitlink 漂移（父仓库记录 shadcn `719de86c` / xzmu `c3f3479f`，
  实际检出 shadcn `4ed19ed2`(+2) / xzmu `ec26a842`）是**先于本工作存在**的。
  Wave 9 的 gitlink 更新会一并吸收这些上游提交，届时必须向用户明确说明。

### 29.5 实现期对计划的修正（与 §27 并列）

1. **M3 的「幂等」语义精确化**（§13）：DDL 可重复执行；M3 在未复制状态下可重复
   执行且不重复写入；**M3 成功后重跑会被其自身 preflight 拒绝（fail closed）**，
   而不是静默双写。`migrate.sh` 以 `schema_migrations` 为门禁，永不重跑已应用迁移，
   故实践无影响。该行为由 `identity_migration_contract.rs` 可执行断言锁定。
2. **测试并发模型**：cargo 为每个测试文件起独立进程，`std::sync::Mutex` 不能跨进程
   串行。DB 契约测试改用 Postgres **事务级 advisory lock**（key `1179204949`，
   与 `migrate.sh` 的 `1179204948` 区分）；「预期失败」断言用 `SAVEPOINT` 隔离，
   避免事务进入 aborted（25P02）后拒绝后续语句。此前未这样做会导致
   `mise run check` 因跨进程 DDL 锁竞争被 SIGTERM（退出码 143）而**假失败**。
3. **`jsonwebtoken` 的 `leeway` 默认 60 秒**（Wave 4 发现）：`Validation::new` 的 `leeway`
   默认是 60，因此 `exp = now - 60` 的令牌**不会**被判过期。这在写作测试时会造成
   「过期令牌必须被拒」的断言假通过/假失败。已改为**显式**声明
   `CLOCK_SKEW_LEEWAY_SECONDS = 60` 并补充边界测试（超出 leeway 必须拒绝），
   避免依赖未文档化的库默认值。
4. **`mise run check` 的并行任务会互相掩盖失败**（Wave 4 发现，重要）：
   `fmt` / `lint` / `test` 并行执行，任一任务失败时 mise 会终止兄弟任务。
   这在日志里表现为 `[test] FAIL: tests exited with 143`（SIGTERM）——
   看起来像测试死锁/超时，**真实原因却是 `cargo fmt --check` 未通过**。
   同类现象还有：test 任务在 `migrate.sh` 写临时 SQL 时被杀，导致
   `psql: error: .../.apply.XXXX.sql: No such file or directory`。
   排查顺序应为：**先看是否有别的任务先失败**，再看 143。
5. **`publisher` 展示快照保留**：`announcements.publisher`（NOT NULL）作为发布当时的
   展示名快照保留；`publisher_id` 为空时前端展示快照 + 「已注销」标注，
   而不是只显示「已注销账号」。
