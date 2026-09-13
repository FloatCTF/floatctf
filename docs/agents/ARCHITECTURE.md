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
│   │       ├── infrastructure/  # 适配器：database、docker、storage、logging、realtime、audit、settings、helper
│   │       ├── modules/         # 业务模块（见 §2）
│   │       ├── scheduler/       # 后台任务引擎（engine + handlers + task_key）
│   │       └── sql/             # SQL 迁移（migrations/ + merged.sql + migrate.sh）
│   └── web/                     # 前端（React + TanStack Query + Tailwind）
├── crates/
│   ├── fcmc/                    # 容器管理 / 出题工具 CLI
│   ├── awd-flagserver/          # AWD FlagServer 独立服务
│   ├── awd-judgeserver/         # AWD JudgeServer 独立服务
│   ├── helper-protocol/       # API ↔ helper 结构化 Host RPC 协议
│   └── floatctf-helper/       # Docker + CAP_NET_ADMIN 宿主控制面守护进程
├── infra/
│   ├── compose/                 # compose.dev.yml（db/rustfs/Caddy/registry）
│   └── caddy/                   # Caddyfile.dev / Caddyfile.prod
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
    pub awd: AwdStaticConfig,      // network_runtime、flagserver_image、judgeserver_image
    pub features: FeatureFlags,    // web_terminal、unsafe_sql_admin
    pub redis: RedisConfig,         // url（必需，Secret）
    pub realtime: RealtimeConfig,  // channel
    pub logging: LoggingConfig,    // filter
    pub challenge: ChallengeConfig,// 计分衰减、实例限制等
    pub timezone: String,          // IANA 时区，空=系统时区
}
```

- 新增配置项流程：`ApplicationToml` 等 struct 加字段 → `AppConfig::from_file` 映射 → 开发者在 development.toml 填值。真正必需的基础设施配置（如 `[redis].url`）不要加 `#[serde(default)]`；可选行为/有安全默认值的字段才使用 default。
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

### floatctf-helper（宿主高权限控制面）

开发与生产的 API 都以无宿主高权限身份运行。helper 提供两个 Unix socket：

```text
floatctf API
    ├── helper-control.sock
    │     JSON line / helper-protocol
    │     WireGuard / nftables / conntrack / Docker FORWARD
    │
    └── helper-docker.sock
          Docker-compatible policy proxy
                 │
                 ▼
          floatctf-helper
          ├── SupplementaryGroups=docker → Docker Engine
          └── CAP_NET_ADMIN → host networking
```

`helper-protocol` 只定义结构化 Host RPC 类型。Docker 继续使用 Bollard，但 `DockerConfig.socket_path` 固定指向 helper policy proxy，生产代码禁止直接连接 `/var/run/docker.sock`。

`floatctf-helper` 内部按宿主能力拆分，避免特权逻辑堆在入口文件：

```text
crates/floatctf-helper/src/
├── main.rs                 # 仅启动 control + Docker proxy
├── control.rs              # helper-control.sock server + RPC dispatch
├── command.rs              # 受控外部命令执行
├── validation.rs           # 资源命名/CIDR/ownership 校验
├── docker_proxy.rs         # Docker-compatible policy proxy
└── network/
    ├── wireguard.rs
    ├── nftables.rs
    ├── conntrack.rs
    ├── routes.rs
    └── docker_forward.rs
```

helper 对 Host RPC 做资源所有权校验：WireGuard interface 必须是 `fawg_<8hex>`，Docker FORWARD bridge 必须是 `fctfawd<8hex>`，nftables table 只能是 `floatctf_awd` / `floatctf_awdp_*`；`ApplyNftTable` 只接受单一 declarative table body，并拒绝 `add/delete/flush/include/define/...` 等 batch command/directive，防止借 `nft -f` 越权操作其他宿主对象。Docker proxy 拒绝 privileged、host bind/mount、host/container network、Devices、CapAdd 等宿主逃逸能力，并限制 mutating operations 到 FloatCTF-owned 资源。

正常配置使用 `awd.network_runtime = "helper"`；`noop` 只用于测试/mock。

### 生产进程载体与 control network

生产 API 已进入 `infra/compose/compose.prod.yml`，宿主没有 `floatctf-api.service`。Compose 内包括 API/PostgreSQL/Redis/RustFS/Caddy；systemd 只保留 `floatctf-helper.service` 与负责 Compose 生命周期的 `floatctf-infra.service`/`floatctf.target`。

API container 以宿主 `floatctf` numeric UID/GID 运行，`cap_drop=ALL`、`no-new-privileges`、read-only rootfs，只读挂载 `/run/floatctf`，不挂 `/var/run/docker.sock`。API 的 9090 不发布到宿主；Caddy 通过 Compose DNS `api:9090` 访问。

AWD/AWDP 的 FlagServer/JudgeServer 通过 external internal network `fctf-platform-control` 回调 API：subnet `10.42.8.0/24`，API 固定 `10.42.8.2`，动态地址范围 `10.42.8.128/25`。GameBox 不加入该网络。`AwdStaticConfig.platform_internal_network` 为空时保持开发模式的“按赛事 infra gateway 派生 host”逻辑；生产设置该字段后使用固定 `platform_internal_url` 并把基础设施容器额外接入 control network。

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
3. **基础设施**（`infra/compose/compose.dev.yml` / `compose.prod.yml`）— 端口、卷、容器网络和部署载体。生产 Compose 的 `VERSION/FLOATCTF_HOME/FLOATCTF_UID/FLOATCTF_GID` 属于部署元数据，不是应用业务配置；API 仍只读 TOML。

判断用哪层：**进程级静态不变 → TOML；管理员可改 → settings 表**。不要在 TOML 里放可运营修改项，也不要在 settings 里放进程级安全配置（如 secret）。

## 6. 后台调度器（apps/api/src/scheduler/）

- `engine.rs` — 轮询 `scheduled_tasks` 表的任务执行引擎（锁、重试、心跳）
- `handlers/` — 具体任务处理器（如 AWD 轮次推进）
- `task_key.rs` — 任务键常量
- 新定时任务：加 task_key → 在 handlers 实现 → 在 bootstrap/scheduler.rs 注册

## 7. Realtime / Redis（infrastructure/）

Redis 是 **API 必需基础设施**。TOML 必须提供 `[redis].url`；bootstrap 在初始化早期建立连接并执行 `PING`，失败即 fail-fast。Redis crate 始终编译进 API，不再存在 `realtime-redis` Cargo feature。`[realtime].channel` 只负责 realtime 频道名。测试里的 `RecordingEventPublisher` / 显式 in-memory terminal backend 仅用于隔离单元测试，不形成生产无 Redis 路径。

当前挂载在 Redis 上的业务如下：

| 消费方 | Redis 机制 / key | 业务语义 | 启动后 Redis 短暂故障 |
|--------|------------------|----------|------------------------|
| realtime（publisher.rs） | pub/sub `floatctf:realtime` + `:sequence` INCR | AWD/AWDP 阶段、比分、攻击/flag、ban、patch、round/recovery 等实时事件跨 API 节点扇出并分配全局 sequence | 同节点 local broadcast 继续，远端 fan-out 等重连恢复 |
| AWD 限流（ratelimit.rs） | `floatctf:ratelimit:{scope}:{key}` ZSET + Lua | flag submit / GameBox reset / internal API 的跨节点共享滑动窗口配额 | **fail-closed**，防止降级为单机计数后绕过限流 |
| Web Terminal（operations/terminal.rs） | `floatctf:terminal-ticket:*`，`SET NX EX` + `GETDEL` | 管理员 session → WebSocket 的 60s 一次性 ticket | **fail-closed**，不退回进程内 ticket |
| Scheduler wake（scheduler/wake.rs） | pub/sub `floatctf:scheduler:wake` | AWD/AWDP deadline、round/judge、管理员任务 create/edit/run-once 等排程立即唤醒 | subscriber 自动重连，5s DB polling 保持任务正确性 |
| settings cache（infrastructure/settings.rs） | `floatctf:settings:map`，整表 JSON + 60s TTL | `get_setting` 热路径（尤其限流）与模板解析的数据库减压 | 读取回源 DB；写后失效失败由 TTL 兜底 |

因此“Redis 必需”指部署和 API 启动契约；各业务仍按风险选择运行期故障语义。安全相关的限流和 terminal ticket 采用 fail-closed，缓存/调度/realtime 保留明确的连续性路径。

## 8. mise 任务速查

```bash
mise run setup                           # 新开发宿主一次性安装/初始化 + helper
mise run dev                             # 唯一开发入口：infra + migration + API watch + Vite
mise run dev:down / dev:reset / dev:logs # 停止 / 清库重建 / 日志
mise run db:migration:new <名称>          # 新建 SQL 迁移（文件内无 BEGIN/COMMIT）
mise run db:migration:apply              # 应用未执行迁移（fresh DB 从 #1 开始）
mise run db:migration:merge              # release/fresh-production 生成 merged.sql
mise run db:gen                          # 从 DB 重新生成 Rust 实体 + TS 类型
mise run fmt / lint / test / check / build
```

开发不存在第二条 infra 轨道；API watch 每次启动会丢弃 docker 等附加组/capabilities，`floatctf-helper` 由 systemd 作为唯一宿主高权限控制面常驻。生产使用同一个 helper 权限边界，但 API 改为 Compose container；不要为了“环境一致”把 helper 容器化或把生产 API 改回 native systemd service。

## 9. 常见陷阱

- **sea-orm-cli 版本必须 1.1.20**（与运行时 sea-orm 1.1.20 匹配）。2.0.1 生成的 `rs_type = "Enum"` 语法在 1.x 编译失败（E0425）。
- **Migrations 只前进**：`apps/api/src/sql/migrations/` 下已有文件**无论如何都不可直接修改/删除/重写**（含 baseline）；改 Schema 只能 `db:migration:new` 追加（详见 DATABASE.md「绝对禁令」与 AGENTS.md 铁律 2）。
- **实体是生成的**：手改 `entity/` 会被下次 `db:gen` 覆盖；改 Schema 走迁移，改完重新生成。
- **不要新增环境变量读取**：配置一律从 TOML（`ctx.config`）或 settings 表获取。
- **API 禁止直连 Docker daemon**：生产 API container 不挂 Docker socket、不带 capabilities；Bollard 必须连接 `[docker].socket_path = "/run/floatctf/helper-docker.sock"`。
- **entity/代码/DB Schema 三者必须一致**（详见 DATABASE.md 的"三处一致"原则）。
- **前端导航必须走 TanStack Router**（`Link` 或 `navigate`），禁止裸 `<a href>`：裸 anchor 点击会整页刷新白屏并清空 QueryClient 缓存。SideBar 已有 onClick 拦截实现，新侧栏/导航组件照抄；回归测试见 `apps/web/src/components/SideBar.test.tsx`。
