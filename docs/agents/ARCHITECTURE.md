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
│   │       ├── core/            # 跨模块核心：config(AppConfig)、contract(契约版本)、secret、security(jwt)
│   │       ├── entity/          # SeaORM 实体（脚本生成，勿手改）
│   │       ├── infrastructure/  # 适配器：database、docker、redis、storage、logging、realtime、audit、settings、package、ratelimit、script_runner、helper
│   │       ├── modules/         # 业务模块（见 §2）；platform/frontend = GET /api/frontend
│   │       ├── scheduler/       # 后台任务引擎（engine + handlers + task_key）
│   │       └── sql/             # SQL 迁移（migrations/ + merged.sql + migrate.sh）
│   └── web/                     # **Web bootstrap**：解析并挂载已安装前端（无 React / 无 UI）
├── frontends/
│   └── default/                 # **官方前端**：当前完整 UI（React + TanStack Router + Primer）
├── packages/
│   ├── sdk/                     # @floatctf/sdk：传输/错误/领域 API/SSE/DTO/生成实体
│   ├── react/                   # @floatctf/react：可选 headless React 绑定（无 UI）
│   └── frontend-runtime/        # @floatctf/frontend-runtime：制品与运行时契约 + bootstrap
├── crates/
│   ├── fcmc/                    # 容器管理 / 出题工具 CLI
│   ├── awd-flagserver/          # AWD FlagServer 独立服务
│   ├── awd-judgeserver/         # AWD JudgeServer 独立服务
│   ├── awdp-judgeserver/        # AWDP（攻防+补丁）JudgeServer 独立服务
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
| `identity` | 登录注册、JWT、用户、管理员与授权 | `authentication/`、`user/`、`administrator/`、`authorization/` |
| `challenge` | 题目 CRUD、构建、题单、Writeup | `catalog/`、`build/`、`set/`、`writeup/` |
| `community` | 讨论区、评论、点赞 | `discussion/`、`comment/`、`like/` |
| `gamebox` | GameBox 包导入、镜像构建、库存、健康检查 | `import.rs`、`library.rs`、`package.rs`、`healthcheck.rs` |
| `platform` | 系统运营 | `announcements/`、`files/`、`settings/`、`operations/`（`dashboard`/`database`/`docker`/`logs`/`runtime_instances`/`scheduled_tasks`/`system`/`terminal`） |
| `weapon` | 工具库（武器） | `api.rs`、`application.rs`、`dto.rs` |
| `event` | 赛事（三引擎 + 公共） | 见下 |

`event` 是最大模块，按赛制拆成**三个互相独立的引擎**（`common` + `jeopardy` + `awd` + `awdp`）：

- `event/common/` — 赛事公共：events/teams/users/challenges/writeup 的 API 与应用层，以及三维模式值对象 `modules/event/common/domain/event_mode.rs`（`EventFamily × EventPurpose × ParticipantMode`，只允许 7 种组合）
- `event/jeopardy/` — 解题赛引擎：
  - `api/`（handlers）、`application/`（use cases + `context.rs`）、`domain/`（`policy.rs` 承载 practice / 个人竞赛 / 战队竞赛三种模式策略，`scoring.rs` 积分衰减、`scoreboard.rs`、`solve.rs`、`instance.rs`、`trend.rs`）、`infrastructure/`（容器运行时）
- `event/awd/` — AWD 攻防赛引擎：
  - `api/`（`player.rs` / `admin.rs` / `internal.rs`）、`domain/`（`flag.rs`、`score.rs`、`network.rs`、`timing.rs`、`firewall_state.rs`、`round_ext.rs`、`execution.rs` 等纯逻辑）、`service/`（deploy / reset / wireguard / judge / firewall / archive）、`infrastructure/`（`firewall/`、wireguard 密钥与持久化）、`repo/`、`scheduler/`、`system/`（helper 侧命令执行：`command.rs` / `conntrack.rs` / `wireguard.rs`）、`websocket.rs`、`crypto.rs`（加密，进程级 OnceLock 注入）
- `event/awdp/` — AWD Plus 引擎（补丁/fix 语义）：`domain/`（`config.rs`、`phase.rs`、`judge.rs`、`score.rs`、`timing.rs`、`flag.rs`）、`service/`（break/fix 补丁与评测）、`repo/`、`scheduler.rs`、`realtime.rs`

> 历史遗留命名：旧版文档写的 `event/awd_team/` 已重命名为 `event/awd/`；`event/registry.rs` / `EventModuleRegistry` 这层"按模式分发"已删除，改为路由层分发（见 §4）。

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
    pub server: ServerConfig,        // listen_ip/port、work_dir
    pub database: DatabaseConfig,    // url（Secret 包装）
    pub docker: DockerConfig,
    pub storage: StorageConfig,      // RustFS endpoint/keys（Secret）
    pub auth: AuthConfig,            // jwt_secret（必填 ≥16）+ awd_root_key / internal_token_key（可选，缺省回落）
    pub cors: CorsConfig,
    pub paths: PathConfig,           // changelog_path、challenges_dir
    pub awd: AwdStaticConfig,        // network_runtime、flagserver/judgeserver_image
    pub awdp: AwdpStaticConfig,      // practice_judgeserver_image、network_pool、event_netmask
    pub registry: RegistryConfig,    // image_prefix、push、server_address
    pub features: FeatureFlags,      // web_terminal、unsafe_sql_admin
    pub redis: RedisConfig,          // url（必需，Secret）
    pub realtime: RealtimeConfig,    // channel
    pub logging: LoggingConfig,      // filter、timezone（IANA，空=系统时区）
    pub main_url: String,            // [application] main_url，作为 MAIN_URL 设置的 seed
}
```

- 新增配置项流程：`ApplicationToml` 等 struct 加字段 → `AppConfig::from_file` 映射 → 开发者在 development.toml 填值。真正必需的基础设施配置（如 `[redis].url`）不要加 `#[serde(default)]`；可选行为/有安全默认值的字段才使用 default。
- 敏感字段用 `core::secret::Secret` 包装（Debug 脱敏，提供 `as_bytes()`）。
- 历史字段已移除：`AppConfig.challenge`（`ChallengeConfig`）与 `AppConfig.timezone` 都不存在了 —— 计分衰减等改为 **settings 表**动态项（`EVENT_SCORE_DECAY` / `EVENT_SCORE_MIN_PERCENT`），时区移到 `[logging].timezone`。

### AppState（bootstrap/state.rs）
`web::Data<AppState>` 是全局共享状态：`config: Arc<AppConfig>`、`db`、`docker`、`storage`、`redis`、`log`、`audit`、`publisher: Arc<dyn EventPublisher>`（本地 hub + Redis 扇出）、`scheduler: Arc<TaskScheduler>`、`terminal_tickets: Arc<TerminalTicketStore>`。

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

生产 API 已进入生产 Compose（`install.sh` 内嵌模板；安装后落在 `$FLOATCTF_HOME/compose.prod.yml`，默认根 `$FLOATCTF_HOME=/var/lib/floatctf`），宿主没有 `floatctf-api.service`。Compose 内包括 API/PostgreSQL/Redis/RustFS/Caddy；systemd 只保留 `floatctf-helper.service` 与负责 Compose 生命周期的 `floatctf-infra.service`/`floatctf.target`。

API container 以数值 `65532:<floatctf GID>`（`.env` 的 `FLOATCTF_UID`/`FLOATCTF_GID`）运行——宿主不创建 `floatctf` 用户，只需要 `floatctf` 组（helper socket 权限）。`cap_drop=ALL`、`no-new-privileges`、read-only rootfs，只读挂载 `/run/floatctf`，不挂 `/var/run/docker.sock`。API 的 9090 不发布到宿主；Caddy 通过 Compose DNS `api:9090` 访问。

AWD/AWDP 的 FlagServer/JudgeServer 通过 external internal network `fctf-platform-control` 回调 API：subnet `10.42.8.0/24`，API 固定 `10.42.8.2`，动态地址范围 `10.42.8.128/25`。GameBox 不加入该网络。`AwdStaticConfig.platform_internal_network` 为空时保持开发模式的“按赛事 infra gateway 派生 host”逻辑；生产设置该字段后使用固定 `platform_internal_url` 并把基础设施容器额外接入 control network。

### EventContext（event/jeopardy/application/context.rs）
Jeopardy 请求级上下文（`db`、`docker`、`event`、`user`、`team`、`config: Option<Arc<AppConfig>>`），通过 `EventContextBuilder` 构造；launch 路径会注入 config（实例数量限制等）。

## 4. 请求数据流（以提交 flag 为例）

```
POST /api/submit/flag                     # bootstrap/routes.rs 里 scope("/submit") 挂载
  → jeopardy::api::submit::submit_flag    # 参数: UserJwtGuard + ReqCtx
       省略 event_id 时以实例归属反查赛事（否则竞赛实例会被当练习记 0 分）
  → EventContextBuilder::new().db(..).docker(..).event(..).user(..).config(..).build()
  → jeopardy::application::submit::submit_flag(&ctx, req)
  → domain/scoring.rs 计算动态分（EVENT_SCORE_DECAY / EVENT_SCORE_MIN_PERCENT 来自 settings 表）
  → SeaORM entity（event_challenge_solves / event_challenge_instance）
  → AppState.publisher（Arc<dyn EventPublisher>）广播 score.changed
  → UniResponse::ok(...) 统一响应包装
```

> **按模式分发不在业务层**：`bootstrap/routes.rs`（约 100 行，全项目唯一路由聚合点）分别挂载 `jeopardy` / `awd` / `awdp` 各自的 `api::*_routes`；引擎内部再用 `if event.family != EventFamily::Xxx` 之类的守卫拒绝跨赛制调用。旧文档里的 `EventModuleRegistry` / `JeopardySingleServices` 已不存在。

统一响应：所有 handler 返回 `UniResult<T>`（`{code, message, data}` 包装），错误用 `AppError`（thiserror）映射 HTTP 状态码。

## 5. 配置体系（三层）

1. **静态 TOML**（`apps/api/config/development.toml`）— 进程级、启动时固定。读法：`ctx.config`。
2. **动态 DB 设置表**（`settings` 表，infrastructure/settings.rs）— 管理员可在管理端编辑。`seed_default_settings` 启动时播种一组**内置默认值**（`INSTANCE_DESTROY_DELAY`、`EVENT_SCORE_DECAY`、`EVENT_SCORE_MIN_PERCENT`、`HTTP_PREFIX`、`NODE_IP`、`FLAG_PREFIX` 等，其中 `WORK_DIR`/`MAIN_URL` 取自 `config.server.work_dir` / `config.main_url`），`ON CONFLICT DO NOTHING`，**不会覆盖已有值**；运行时用 `get_setting(&db, key)` 读取，无此键报错 "Setting not found:<key>"。
3. **基础设施**（`infra/compose/compose.dev.yml` / `compose.prod.yml`）— 端口、卷、容器网络和部署载体。生产 Compose 的 `VERSION/FLOATCTF_HOME/FLOATCTF_UID/FLOATCTF_GID` 属于部署元数据，不是应用业务配置；API 仍只读 TOML。

判断用哪层：**进程级静态不变 → TOML；管理员可改 → settings 表**。不要在 TOML 里放可运营修改项，也不要在 settings 里放进程级安全配置（如 secret）。

### 密钥拆分：`auth.jwt_secret` / `auth.awd_root_key` / `auth.internal_token_key`

历史上同一个 `auth.jwt_secret` 被用于三件事（风险清单 #7），现已拆开：

| 键 | 用途 | 泄露后果 |
|---|---|---|
| `auth.jwt_secret`（必填，≥16 字符） | JWT 签名（HS512） | 可伪造任意用户/超管令牌 |
| `auth.awd_root_key`（可选） | AWD/AWDP flag、实例密钥的 HKDF 根（`AwdCrypto`） | 可伪造 AWD/AWDP flag |
| `auth.internal_token_key`（可选） | AWDP 判题容器 `INTERNAL_TOKEN` 的派生根（会下发进容器） | 可冒充判题容器回调平台 |

- **可选键未配置时回落 `jwt_secret`**（`AuthConfig::awd_root_key()/internal_token_key()`），bootstrap 会打一条 warn
  提醒"仍在共用主密钥"；这样既有部署升级不会被动轮换 flag。
- 三个键现在可以**独立轮换**：改 `jwt_secret` 只失效 JWT；改 `awd_root_key` 只改变 flag 派生
  （进行中的 AWD 赛事会因此换 flag）；改 `internal_token_key` 只需重建判题容器（`awdp.practice.judge`
  cron 检测 env drift 会自动重建）。
- 全新安装由 `scripts/install.sh` 生成三个互不相同的随机值；**既有安装在 `.env` / TOML 里显式填值
  才会启用**（不填 = 保持旧行为）。
- 三处都走 `Secret` 包装：Debug/日志自动脱敏。不要把其中任何一个写进日志或返回给前端。

另：`seed_default_settings` 现在也补上了此前"被读取但从未 seed"的 5 个键（`AWD_RATE_SUBMIT_PER_MIN`/`AWD_RATE_RESET_PER_HOUR`/`AWD_RATE_INTERNAL_PER_MIN`/`AWD_NETWORK_REVISION`/`AWDP_DATA_PLANE_EXEC`，风险清单 #14）。其中 **`AWDP_DATA_PLANE_EXEC` 在 production 必须是 `true`**（API 只挂控制网，进程内探测必然失败）；默认播种 false 是为了不改变既有 dev/测试语义，请在管理端设置页确认生产值。

## 6. 后台调度器（apps/api/src/scheduler/）

- `engine.rs` — 轮询 `scheduled_tasks` 表的任务执行引擎（锁、重试、心跳）
- `handlers/` — 具体任务处理器（如 AWD 轮次推进）
- `task_key.rs` — 任务键常量
- `wake.rs` — Redis pub/sub 即时唤醒（`floatctf:scheduler:wake`，5s DB 轮询为兜底）

⚠️ 读 `scheduled_tasks` 时的两个反直觉点（风险清单 #9/#10）：

- `awd.round.start` 与 `awd.archive.cleanup` **有 handler、没有生产者**：没有任何代码写出这两行
  （轮次推进是 `round_service::end_round` 进程内直接调 `start_round`；归档没有周期清理）。
  它们只是"可手工触发的入口"。
- `platform.rustfs.clean`（`CleanUnusedRustFSFilesHandler`）是**未实现的空操作**：不列举也不删除
  任何对象，只会打一条 warn。它的成功记录不代表对象存储被清理过。
- `scheduled_tasks.enabled = false` **不生效**：引擎用内存中的 cron 状态，改库不会停任务（实测）。
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
- **前端导航必须走 TanStack Router**（`Link` 或 `navigate`），禁止裸 `<a href>`：裸 anchor 点击会整页刷新白屏并清空 QueryClient 缓存。SideBar 已有 onClick 拦截实现，新侧栏/导航组件照抄；回归测试见 `frontends/default/src/components/SideBar.test.tsx`。
- **前端制品跑在纯浏览器环境**：没有 `process` / `require` / Node 全局。第三方依赖里的 `process.env.*` 必须在构建期替换（见 `frontends/default/vite.config.ts` 的 `define`）。漏掉会导致整页起不来。
- **不要跨包 import 源码路径**：只能用 `@floatctf/sdk`、`@floatctf/react`、`@floatctf/frontend-runtime` 的公开导出；`mise run web:architecture` 会拦截。
