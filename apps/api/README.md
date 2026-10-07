# FloatCTF API Documentation

## Project Structure

本节只是**概览**；模块与文件以代码为准。真实布局（`apps/api/src/`）：

```text
apps/api/src/
├── main.rs                 # 进程入口
├── lib.rs
├── api/                    # HTTP 层：extractor(ReqCtx)、dto、app_error、统一响应
├── bootstrap/              # 启动装配：run、state(AppState)、routes、scheduler
│   └── routes.rs           # ★ 全项目唯一 HTTP 路由聚合点
├── core/                   # config(AppConfig)、contract、secret、security(jwt)
├── entity/                 # SeaORM 实体（脚本生成，勿手改）
├── infrastructure/         # database / docker / redis / storage / settings / realtime / …
├── modules/                # 业务模块（各自的 configure_*_routes 定义精确路径）
│   ├── identity/           # authentication / user / administrator / authorization
│   ├── challenge/          # catalog / build / set / writeup
│   ├── event/              # common + jeopardy + awd + awdp（三个独立赛制引擎）
│   ├── community/          # discussion / comment / like
│   ├── gamebox/            # 题包导入 / 镜像构建 / 库存 / 健康检查
│   ├── platform/           # announcements / files / frontend / settings / operations
│   └── weapon/
├── scheduler/              # 后台任务引擎（engine + handlers + task_key）
└── sql/                    # migrations/ + merged.sql + migrate.sh
```

HTTP 集成测试在 `apps/api/tests/`，说明见 [tests/README.md](tests/README.md)。

## Technology Stack

- **Web Framework**: Actix-web 4
- **ORM**: Sea-ORM 1.1 (PostgreSQL)
- **Authentication**: JWT (HS512 algorithm, 8-hour default expiration)
- **Docker Management**: Bollard 0.19
- **Password Hashing**: Argon2

## Authentication

### JWT Token Structure

Both User and SuperAdmin JWTs contain:
- `sub`: User ID (UUID)
- `role`: Role enum (`User`, `SuperAdmin`, `ResetAccount`, `AwdJudger`)
- `exp`: Expiration timestamp

### Headers

```
Authorization: Bearer <token>
```

### Roles

| Role | Description |
|------|-------------|
| `User` | Regular user access |
| `SuperAdmin` | Admin panel access |
| `ResetAccount` | Password reset token |
| `AwdJudger` | Award judging access |

---

## Routes

**代码是唯一权威。** 本页只给**有代表性、已逐条核对过**的分组概览，不再手工维护一份
"完整端点表" —— 历史版本那份清单已经漂移到会误导人。要精确路径请直接看源码：

| 位置 | 内容 |
|------|------|
| `src/bootstrap/routes.rs` | 全项目唯一路由聚合点：`/api` 选手路由、`/api/admin` 管理路由、AWD/AWDP internal 回调 |
| `src/modules/**` | 每个模块的 `configure_*_routes`（scope 定义）与 handler 上的 `#[get]` / `#[post]` / `#[patch]` / `#[delete]` / `#[put]` 属性 —— **精确路径在这里** |
| `tests/common/routes.rs` | 与路由对齐的测试目录（回归断言路由集合） |

约定：

- 选手端挂在 `/api` 下；管理端挂在 `/api/admin` 下。
- 内部回调（FlagServer / JudgeServer → API）**不在** `/api` 下，直接挂在根路径
  `/internal/...`，走内部令牌鉴权，不对外发布、不经 Caddy 的公开路径。
- 所有 handler 返回 `UniResult<T>`（`{code, message, data}`）；错误用 `AppError` 映射状态码。

### 鉴权与会话（`modules/identity`）

| 路径 | 说明 |
|------|------|
| `POST /api/admin/session` | 超管登录 |
| `POST /api/users/session` | 用户登录 |
| `POST /api/users` | 注册 |
| `POST /api/users/reset_password` · `POST /api/users/reset?token=…` | 密码重置 |
| `GET /api/users/me` · `PATCH /api/users/me` | 当前用户 |
| `GET` / `POST` / `DELETE /api/admin/users` · `GET` / `PATCH /api/admin/users/{user_id}` | 用户管理 |
| `GET` / `POST` / `DELETE /api/admin/super_admin` · `GET` / `POST` / `PATCH /api/admin/super_admin/{super_admin_id}` | 超管管理 |

### 题目 / 题单 / 解题 / Writeup（`modules/challenge`）

| 路径 | 说明 |
|------|------|
| `GET /api/challenges` · `GET /api/challenges/{challenge_id}` | 题目目录 / 详情 |
| `GET /api/challenges/{challenge_id}/instance` | 题目实例 |
| `GET` / `POST /api/challenges/{challenge_id}/my_writeup` | 我的 Writeup |
| `GET /api/challenges/{challenge_id}/writeups` | 题解列表 |
| `GET /api/challenge_sets` · `GET /api/challenge_sets/{challenge_set_id}` | 题单 |
| `GET /api/solves` · `GET /api/solves/top15users` | 解题记录（scope 为 `/solves`） |
| `GET /api/writeups` · `GET /api/writeups/{writeup_id}` | Writeup 汇总 |
| 管理端 `/api/admin/challenges`、`/api/admin/challenge_sets`（含 `import` / `build` / `check` / `scan`） | 题目与题单 CRUD |

### 提交（`event/jeopardy`）

| 路径 | 说明 |
|------|------|
| `POST /api/submit/flag` | 提交 Flag |
| `POST /api/submit/writeup` | 提交 Writeup |

### 实例（`event/jeopardy`）

| 路径 | 说明 |
|------|------|
| `GET /api/instances` · `GET /api/instances/{instance_id}` | 我的实例 |
| `POST /api/instances/launch` · `DELETE /api/instances/{instance_id}` | 启动 / 销毁 |
| `GET /api/admin/events/{event_id}/instances` | 管理端按赛事的实例列表（family 无关） |

### 赛事公共服务（`event/common`，前缀 `/api/events`）

- 选手：`GET /api/events` · `GET /api/events/{event_id}` · `…/capabilities` · `…/challenges` ·
  `…/instances` · `…/challenges/{challenge_id}/instance` · `…/scoreboard` · `…/trend` ·
  `…/announcements` · `…/own_wp`
- 组队：`POST …/join` · `DELETE …/leave` · `POST …/team` · `POST …/team/{team_id}/join` ·
  `POST …/team/{team_id}/leave` · `DELETE …/team/{team_id}`
- 管理端（`/api/admin/events`）：`GET` / `POST` / `DELETE ""` · `GET` / `PATCH /{event_id}` ·
  `GET /{event_id}/data` · `GET /{event_id}/report` · 以及 `{event_id}/users*`、`{event_id}/teams*`、
  `{event_id}/challenges*`、`{event_id}/announcements*`、`{event_id}/writeups`、`{event_id}/logs`

### AWD —— 选手（`modules/event/awd`，前缀 `/api/events`）

`GET {event_id}/awd/status` · `GET {event_id}/awd/scores` · `GET {event_id}/awd/stream` ·
`GET {event_id}/awd/gameboxes` · `GET {event_id}/awd/wireguard/config` · `GET {event_id}/awd/ssh-config` ·
`POST {event_id}/awd/submissions` · `POST {event_id}/awd/gameboxes/{instance_id}/reset`

### AWD —— 管理（前缀 `/api/admin`）

- 赛事级（`/api/admin/events`）：`POST /api/admin/events/awd` 新建；`GET` / `PATCH
  {event_id}/awd` · `{event_id}/awd/start` · `pause` · `resume` · `finish` · `archive` · `deploy` ·
  `precheck`（POST）· `prechecks`（GET）· `judge` · `network`（GET/PUT）· `network/reallocate` ·
  `gameboxes`（GET/POST）· `gameboxes/{event_gamebox_id}`（PATCH/DELETE）·
  `gameboxes/{instance_id}/reset` · `scores` · `stream` · `score/adjust` · `tokens/rotate` ·
  `teams/{team_id}/ban`（POST/DELETE）
- 平台级（`/api/admin/awd/*`）：`GET` / `DELETE awd/gameboxes` ·
  `awd/gameboxes/import` · `check` · `build` · `scan` ·
  `PATCH awd/gameboxes/{gamebox_id}` · `awd/gameboxes/{gamebox_id}/hide` ·
  `GET` / `PATCH awd/network` · `GET awd/network/allocations` · `GET awd/network/health`

### AWDP —— 选手与练习（`modules/event/awdp`）

- 赛事内（`/api/events`）：`GET {event_id}/awdp` · `{event_id}/awdp/scores` · `trend` · `scoreboard` ·
  `stream` · `rounds` · `evaluations` · `GET {event_id}/awdp/gameboxes/{eg_id}/instance` ·
  `POST {event_id}/awdp/gameboxes/{eg_id}/instance` · `…/instance/stop` · `…/instance/reset` ·
  `…/break` · `…/patch` · `…/test-check` · `GET {event_id}/awdp/gameboxes/{eg_id}/source`
- 练习（Training Ground，前缀 `/api/service`）：`GET /api/service/gameboxes?capability=awdp` ·
  `POST /api/service/gameboxes/{gamebox_id}/awdp/runs` ·
  `GET /api/service/awdp/runs/{run_id}` ·
  `POST /api/service/awdp/runs/{run_id}/{start|stop|reset|end|phase|restart-training}` ·
  `POST /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/break` ·
  `GET /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/source`

### AWDP —— 管理（前缀 `/api/admin/events`）

`GET` / `PATCH /api/admin/events/{event_id}/awdp` · `{event_id}/awdp/start` · `{event_id}/awdp/break-to-fix` ·
`{event_id}/awdp/finish` · `{event_id}/awdp/data` · `{event_id}/awdp/gameboxes`（GET/POST） ·
`{event_id}/awdp/instances` · `{event_id}/awdp/runs` · `{event_id}/awdp/scores`

### 公告（`modules/platform/announcements`）

- 选手：`GET /api/announcements`；赛事内 `GET /api/events/{event_id}/announcements`
- 管理端：`GET` / `POST` / `DELETE /api/admin/announcements` ·
  `PATCH /api/admin/announcements/{announcement_id}` ·
  赛事级 `GET` / `POST` / `DELETE /api/admin/events/{event_id}/announcements` ·
  `GET` / `PATCH /api/admin/events/{event_id}/announcements/{announcement_id}`

### 讨论区（`modules/community`，前缀 `/api/discussions`）

- 选手：`GET` / `POST ""` · `GET` / `PATCH` / `DELETE /{discussion_id}` ·
  `GET` / `POST /{discussion_id}/comments` · `PATCH` / `DELETE /{discussion_id}/comments/{comment_id}` ·
  `POST` / `DELETE /{discussion_id}/like`
- 管理端（`/api/admin/discussions`）：`GET ""` · `DELETE ""` · `GET /{discussion_id}` ·
  `GET /{discussion_id}/comments` · `DELETE /{discussion_id}/comments/{comment_id}`

### 上传与文件（`modules/platform/files`）

`POST /api/uploads/image` · `PATCH /api/uploads/avatar` · `GET /api/admin/download?key=…`

### 工具库（`modules/weapon`）

`GET /api/weapons`；管理端 `GET` / `POST` / `DELETE /api/admin/weapons` ·
`PATCH /api/admin/weapons/{weapon_id}` · `POST /api/admin/weapons/{weapon_id}/upload`

### 平台 / 系统 / 设置 / 前端引导（`modules/platform`）

| 路径 | 说明 |
|------|------|
| `GET /api/frontend` | **未认证**的公开前端引导元数据（登录前必须可用） |
| `GET /api/admin/system/monitor` · `GET /api/admin/system/version` | 系统信息 / 版本 |
| `POST /api/admin/database/exec_sql` | SQL 控制台（受 `features.unsafe_sql_admin` 约束） |
| `GET` / `POST` / `DELETE /api/admin/settings` · `PATCH /api/admin/settings/{setting_id}` | 动态设置 |
| `GET /api/admin/dashboard/summary` | 管理端概览 |
| `GET` / `POST /api/admin/scheduled_tasks` · `GET` / `PATCH` / `DELETE /api/admin/scheduled_tasks/{task_id}` · `POST /api/admin/scheduled_tasks/{task_id}/run` | 定时任务 |
| `GET /api/admin/logs` · `GET /api/admin/logs/{log_id}` | 日志 |
| `GET /api/admin/docker/containers` · `POST …/{container_id}/start` / `stop` · `DELETE …/{container_id}` | Docker（经 helper 策略代理） |
| `GET /api/admin/docker/images` · `DELETE /api/admin/docker/images/{image_id}` | 镜像 |
| `GET` / `POST /api/admin/docker/networks` · `DELETE /api/admin/docker/networks/{network_id}` | 网络 |
| `POST /api/admin/terminal/session` · `GET /api/admin/terminal/ws` | Web Terminal（一次性 ticket） |

### 内部回调（`/internal/*`，不对外发布）

- AWD（FlagServer / JudgeServer，内部令牌）：
  `POST /internal/awd/events/{event_id}/flags/issue` ·
  `POST /internal/awd/events/{event_id}/judge/claim` ·
  `POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/heartbeat` ·
  `POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/result` ·
  `GET /internal/awd/events/{event_id}/health`
- AWDP（练习 JudgeServer 回调，内部令牌）：
  `POST /internal/awdp/judge/jobs/claim` · `POST /internal/awdp/judge/jobs/{id}/heartbeat` ·
  `POST /internal/awdp/judge/jobs/{id}/result` · `POST /internal/awdp/flag/resolve` ·
  `POST /internal/awdp/proof/consume`

---

## Data Models

### challenges::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| name | String | Challenge name (unique) |
| safe_name | String | URL-safe name (unique) |
| category | String | Challenge category |
| description | String | Challenge description |
| attachment | Option<String> | Attachment filename |
| hidden | bool | Whether challenge is hidden |
| toml_str | String | Challenge configuration TOML |
| created_at | DateTimeWithTimeZone | Creation timestamp |
| updated_at | DateTimeWithTimeZone | Last update timestamp |

### events::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| family | EventFamily | `jeopardy` / `awd` |
| purpose | EventPurpose | `practice` / `competition` |
| participant_mode | ParticipantMode | `individual` / `team` |
| system_key | Option<String> | System-managed key (e.g. `practice:jeopardy`); NULL for ordinary events |
| title | String | Event title |
| description | Option<String> | Event description |
| hidden | bool | Whether event is hidden |
| allow_join | bool | Whether users can join |
| rules | String | Event rules |
| start_time | DateTimeWithTimeZone | Start time |
| end_time | Option<DateTimeWithTimeZone> | End time (`NULL` for Practice) |
| flag_prefix | Option<String> | Custom flag prefix |

### EventMode (Family × Purpose × ParticipantMode)

Allowed combinations:

| family | purpose | participant_mode |
|--------|---------|------------------|
| jeopardy | practice | individual |
| jeopardy | competition | individual |
| jeopardy | competition | team |
| awd | competition | team |

Practice is system-managed via `system_key = practice:jeopardy` (not created by admin).

### challenge_instances::Model

| Field | Type | Description |
|-------|------|-------------|
| id | Uuid | Primary key |
| challenge_id | Uuid | Related challenge |
| user_id | Uuid | Launcher / requester |
| event_id | Uuid | Owning event (Practice uses system Practice event) |
| team_id | Option<Uuid> | Team owner when participant_mode=team; NULL for individual |
| status | InstanceStatus | Running/Stopped/Error |
| flag | String | Instance flag |
| identifier | String | Container/instance identifier |
| created_at | DateTimeWithTimeZone | Creation timestamp |
| updated_at | DateTimeWithTimeZone | Last update timestamp |

### InstanceStatus Enum

| Value | Description |
|-------|-------------|
| `Running` | Instance is running |
| `Stopped` | Instance is stopped |
| `Error` | Instance error state |

---

## Query Parameters

All list endpoints support pagination and filtering:

### Pagination

| Parameter | Type | Description |
|-----------|------|-------------|
| page | usize | Page number (1-indexed) |
| limit | usize | Items per page |

### Filtering

| Parameter | Type | Description |
|-----------|------|-------------|
| filter | JSON string | Filter expression |

### Filter Format

```json
{
  "key": "value",
  "key2": "value2"
}
```

Common filter keys:
- `id`: UUID exact match
- `name`: String contains
- `category`: String contains
- `type`: Enum value
- `hidden`: Boolean
- `allow_join`: Boolean

---

## Response Format

All responses follow `UniResponse<T>` format:

```json
{
  "data": T,
  "meta": {
    "page": 1,
    "limit": 10,
    "total": 100
  }
}
```

For endpoints returning no data (`UniResponse<()>`):
```json
{
  "data": null
}
```

---

## Error Responses

Errors return `UniError` with HTTP status codes:

| Status | Description |
|--------|-------------|
| 400 | Bad Request |
| 401 | Unauthorized (AuthError) |
| 404 | Not Found |
| 500 | Internal Server Error |

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable message"
  }
}
```

---

## Dynamic Score Calculation

Events use dynamic scoring that decays based on solve count:

```
score = min_points + (base_points - min_points) * sqrt(decay / (decay + solves))
```

Where:
- `min_points = base_points * event_score_min_percent`
- `decay` and `event_score_min_percent` are system settings


## License
本项目以 [GNU AGPLv3](LICENSE) 协议发布。
Copyright (C) 2025-2026 fb0sh@outlook.com
