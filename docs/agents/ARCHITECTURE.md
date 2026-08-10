# 架构速览（AI 必读）

> 目标：让 AI 在 5 分钟内建立对仓库的心智模型，知道每个东西在哪里、如何流动、改哪里。
> 阅读顺序建议：本文 → [DATABASE.md](./DATABASE.md) → [TESTING.md](./TESTING.md)，动手前再看 [ADD-FEATURE.md](./ADD-FEATURE.md) 或 [FIX-BUG.md](./FIX-BUG.md)。

## 1. 仓库布局

```
floatctf/
├── apps/
│   ├── api/                     # 后端 API（Rust / Actix Web），包名 floatctf
│   │   ├── config/              # TOML 配置文件（development.toml）
│   │   └── src/
│   │       ├── api/             # HTTP 层：extractor(ReqCtx)、dto、app_error
│   │       ├── bootstrap/       # 启动装配：mod(run)、state(AppState)、routes、scheduler
│   │       ├── core/            # 跨模块核心：config(AppConfig)、secret、security(jwt)
│   │       ├── entity/          # SeaORM 实体（脚本生成，勿手改）
│   │       ├── infrastructure/  # 适配器：database、docker、storage、logging、realtime、audit、settings
│   │       ├── modules/         # 业务模块（见 §2）
│   │       ├── scheduler/       # 后台任务引擎（engine + handlers + task_key）
│   │       └── sql/             # SQL 迁移（migrations/ + merged.sql + migrate.sh）
│   └── web/                     # 前端（React + TanStack Query + Tailwind）
├── crates/
│   ├── fcmc/                    # 容器管理 / 出题工具 CLI
│   ├── awd-flagserver/          # AWD FlagServer 独立服务
│   └── awd-judgeserver/         # AWD JudgeServer 独立服务
├── infra/
│   ├── compose/                 # compose.dev.yml（db/rustfs/nginx/registry）
│   └── nginx/                   # nginx.dev.conf（静态 upstream）
├── scripts/                     # gen_entities.py、gen_web_types.py、infra-up.sh、dev.sh
├── mise.toml                    # 全部开发任务入口
└── AGENTS.md                    # AI 工作手册索引
```

## 2. 业务模块（apps/api/src/modules/）

| 模块 | 职责 | 关键子目录 |
|------|------|-----------|
| `identity` | 登录注册、JWT、管理员 | `authentication/` |
| `challenge` | 题目 CRUD、构建、题单、Writeup | `catalog/`、`build/`、`set/`、`writeup/`、`metadata/` |
| `community` | 讨论区、评论 | `discussion/`、`comment/` |
| `platform` | 系统运营 | `announcements/`、`files/`、`operations/`(system/database/terminal)、`settings/` |
| `weapon` | 工具库（武器） | `dto/` |
| `event` | 赛事（两大模式 + 公共） | 见下 |

`event` 是最大模块：

- `event/common/` — 赛事公共：events/teams/users/challenges/writeup 的 API 与应用层
- `event/jeopardy/` — 解题赛模式：
  - `api/`（handlers）、`application/`（use cases + context）、`domain/`（策略/积分/排行榜）、`infrastructure/`（容器运行时）、`modes/`（practice / single / team 三种模式策略）
- `event/awd_team/` — AWD 攻防赛模式：
  - `api/`（player/admin/internal 路由）、`domain/`（flag/score/network 纯逻辑）、`service/`（deploy/reset/wireguard/judge）、`infrastructure/`（wireguard 密钥/持久化）、`repo/`、`scheduler/`、`system/`（防火墙）、`crypto.rs`（加密，进程级 OnceLock 注入）
- `event/registry.rs` — `EventModuleRegistry`：按模式分发 launch/submit/get_instances/destroy

### 模块内分层约定

```
api/            HTTP handlers + DTO（薄，只做参数解析与错误映射）
  └─> application/  用例（编排领域逻辑与持久化）
        └─> domain/     纯领域逻辑（无 I/O，最值得写单元测试）
        └─> infrastructure/ 外部适配器（Docker/DB/WireGuard 等）
        └─> repo/        SeaORM 持久化
```

规则：
- **handler 不写业务逻辑**；复杂逻辑放 `application/` 或 `domain/`。
- **domain/ 禁止 I/O**（不 import sea_orm / bollard），方便纯单元测试。
- 新模块必须在 `modules/mod.rs` 声明，路由必须在 `bootstrap/routes.rs` 注册（全项目唯一路由聚合点）。

## 3. 关键类型与依赖注入

### AppConfig（core/config.rs）
进程级静态配置，全部来自 TOML（`FLOATCTF_CONFIG` 指向 `apps/api/config/development.toml`），启动时 `AppConfig::from_file` 加载一次，**fail-fast**：任何时刻代码都不要直接读环境变量。

```rust
pub struct AppConfig {
    pub server: ServerConfig,      // listen_ip/port、work_dir、log_dir
    pub database: DatabaseConfig,  // url（Secret 包装）
    pub docker: DockerConfig,
    pub storage: StorageConfig,    // RustFS endpoint/keys（Secret）
    pub auth: AuthConfig,          // jwt_secret（Secret，≥16 字符）
    pub cors: CorsConfig,
    pub paths: PathConfig,         // changelog_path、challenges_dir
    pub awd: AwdStaticConfig,      // host_network、flagserver_image、judgeserver_image
    pub features: FeatureFlags,    // web_terminal、unsafe_sql_admin
    pub realtime: RealtimeConfig,  // redis_url/channel（可选）
    pub logging: LoggingConfig,    // filter
    pub challenge: ChallengeConfig,// 计分衰减、实例限制等
    pub timezone: String,          // IANA 时区，空=系统时区
}
```

- 新增配置项流程：`ApplicationToml` 等 struct 加字段（带 `#[serde(default)]`）→ `AppConfig::from_file` 映射 → 开发者在 development.toml 填值。
- 敏感字段用 `core::secret::Secret` 包装（Debug 脱敏，提供 `as_bytes()`）。

### AppState（bootstrap/state.rs）
`web::Data<AppState>` 是全局共享状态：`config: Arc<AppConfig>`、`db`、`docker`、`storage`、`log`、`audit`、realtime hub、事件注册表。

### ReqCtx（api/extractor/request_context.rs）
Handler 的参数注入器（实现 `FromRequest`），每个请求自动构造：

```rust
pub struct ReqCtx {
    pub config: Arc<AppConfig>,  // 静态配置
    pub db: WebDb,               // web::Data<DbConn>
    pub docker: WebDocker,       // web::Data<Docker>
    pub rustfs: WebRustfs,       // web::Data<S3Client>
    pub log: WebLog,
    pub req: HttpRequest,
}
```

**规则：handler 需要配置/DB/Docker 时，一律声明 `ctx: ReqCtx` 参数，从 `ctx.xxx` 取**，不要自行从环境变量或全局单例取。

### EventContext（event/jeopardy/application/context.rs）
Jeopardy 请求级上下文（`db`、`docker`、`event`、`user`、`team`、`config: Option<Arc<AppConfig>>`），通过 `EventContextBuilder` 构造；launch 路径会注入 config（实例数量限制等）。

## 4. 请求数据流（以提交 flag 为例）

```
POST /api/events/{id}/challenges/{cid}/submit
  → handler（jeopardy/api/submit.rs）参数: UserJwtGuard + ReqCtx
  → EventContextBuilder::new().db(...).docker(...).config(ctx.config.clone()).build()
  → EventModuleRegistry::submit_flag(&event_ctx, req)     // 按模式分发
  → JeopardySingleServices::submit_flag(ctx, instance_id, flag)
  → core::jeopardy_submit(...) → submission_service（积分规则）
  → SeaORM entity（event_challenge_solves / event_instances）
  → 通过 RealtimeEventPublisher 广播 score.changed
  → UniResponse::ok(...) 统一响应包装
```

统一响应：所有 handler 返回 `UniResult<T>`（`{code, message, data}` 包装），错误用 `AppError`（thiserror）映射 HTTP 状态码。

## 5. 配置体系（三层）

1. **静态 TOML**（`apps/api/config/development.toml`）— 进程级、启动时固定。读法：`ctx.config`。
2. **动态 DB 设置表**（`settings` 表，infrastructure/settings.rs）— 管理员可在管理端编辑。`seed_default_settings` 启动时从 AppConfig.challenge 播种（ON CONFLICT DO NOTHING，**不会覆盖已有值**），运行时用 `get_setting(&db, key)` 读取，无此键报错 "Setting not found:<key>"。
3. **基础设施**（infra/compose/compose.dev.yml）— 端口、卷、容器。

判断用哪层：**进程级静态不变 → TOML；管理员可改 → settings 表**。不要在 TOML 里放可运营修改项，也不要在 settings 里放进程级安全配置（如 secret）。

## 6. 后台调度器（apps/api/src/scheduler/）

- `engine.rs` — 轮询 `scheduled_tasks` 表的任务执行引擎（锁、重试、心跳）
- `handlers/` — 具体任务处理器（如 AWD 轮次推进）
- `task_key.rs` — 任务键常量
- 新定时任务：加 task_key → 在 handlers 实现 → 在 bootstrap/scheduler.rs 注册

## 7. Realtime（infrastructure/realtime/）

事件发布抽象：本地广播 + 可选 Redis fan-out（TOML `[realtime]` 配置，feature `realtime-redis`）。测试用 `RecordingEventPublisher`（记录事件，零依赖）。

## 8. mise 任务速查

```bash
mise run install                  # 安装依赖
mise run infra:up / down / logs   # 基础设施（Postgres/RustFS/Nginx）
mise run dev:api / dev:web / dev  # 启动开发服务
mise run db:migration:new <名称>  # 新建 SQL 迁移（文件内无 BEGIN/COMMIT）
mise run db:migration:apply       # 应用未执行迁移（开发库已 baseline）
mise run db:migration:merge       # 合并迁移 → merged.sql（fresh DB bootstrap）
mise run db:gen                   # 从 DB 重新生成 Rust 实体 + TS 类型
mise run fmt / lint / test / check / build
```

## 9. 常见陷阱

- **sea-orm-cli 版本必须 1.1.20**（与运行时 sea-orm 1.1.20 匹配）。2.0.1 生成的 `rs_type = "Enum"` 语法在 1.x 编译失败（E0425）。
- **实体是生成的**：手改 `entity/` 会被下次 `db:gen` 覆盖；改 Schema 走迁移，改完重新生成。
- **不要新增环境变量读取**：配置一律从 TOML（`ctx.config`）或 settings 表获取。
- **entity/代码/DB Schema 三者必须一致**（详见 DATABASE.md 的"三处一致"原则）。
- **前端导航必须走 TanStack Router**（`Link` 或 `navigate`），禁止裸 `<a href>`：裸 anchor 点击会整页刷新白屏并清空 QueryClient 缓存。SideBar 已有 onClick 拦截实现，新侧栏/导航组件照抄；回归测试见 `apps/web/src/components/SideBar.test.tsx`。
