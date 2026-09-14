# DEVELOPMENT.md — FloatCTF 开发指南

FloatCTF 只有一套开发环境：**完整能力开发模式**。Jeopardy、AWD、AWDP、Docker、WireGuard、nftables、PostgreSQL、Redis、RustFS 和 Caddy 都在这套环境中开发和验证。

开发与生产共享同一宿主权限边界：API 不直接持有 Docker daemon 或 `CAP_NET_ADMIN` 权限，所有宿主高权限操作统一经过 `floatctf-helper`。开发由 `mise` 编排原生 API/Vite 和基础设施；生产 API 已容器化，由 Compose 管理 API/PostgreSQL/Redis/RustFS/Caddy，systemd 只负责 helper 与整套 Compose 生命周期。

Redis 是开发 API 的必需依赖。`mise run dev` 会先等待 `floatctf-dev-redis` healthy；API 读取 `[redis].url = "redis://127.0.0.1:6379/"` 后还会执行一次真实 `PING`，不可达时 fail-fast。不要为了单机开发增加无 Redis 运行模式。

生产安装与运维见 [INSTALL.md](./INSTALL.md)，模块架构见 [docs/agents/ARCHITECTURE.md](./docs/agents/ARCHITECTURE.md)。

---

## 1. 最终权限模型

### 1.1 宿主控制面

```text
floatctf API
    │
    ├── /run/floatctf/helper-control.sock
    │       结构化 Host RPC
    │       WireGuard / nftables / conntrack / Docker FORWARD
    │
    └── /run/floatctf/helper-docker.sock
            受策略限制的 Docker-compatible API
            │
            ▼
       floatctf-helper
       ├── User=floatctf-helper
       ├── Group=floatctf
       ├── SupplementaryGroups=docker
       └── CAP_NET_ADMIN
            │
            ├── /var/run/docker.sock
            ├── WireGuard
            ├── nftables
            └── conntrack
```

`helper-protocol` 是 API 与 helper 共用的 Rust 类型 crate，只定义结构化 Host RPC 的 `Request` / `Response`，自身没有进程、端口或权限。

`floatctf-helper` 是唯一宿主高权限守护进程。它提供两个 `0660` Unix socket，主组都是 `floatctf`：

```text
/run/floatctf/helper-control.sock
/run/floatctf/helper-docker.sock
```

Docker proxy 会在 helper 内执行策略检查。当前关键约束包括：禁止 privileged、host/container network、host bind/mount、Devices、DeviceRequests、CapAdd、host PID/IPC/UTS、任意 sysctl；FloatCTF 创建的容器和网络会写入 helper ownership label；对已有容器/网络的变更要求 FloatCTF ownership。

### 1.2 开发身份

```text
开发者用户
├── git / mise / cargo / pnpm / watchexec
├── Docker Compose CLI（开发基础设施）
└── Vite

API 子进程
├── UID = 当前开发者
├── GID = floatctf
├── supplementary groups = 仅 floatctf
├── docker group = 无
├── CAP_* = 无
├── NoNewPrivileges = yes
└── Docker/网络操作 → floatctf-helper
```

开发者仍加入 `docker` 组，原因只有一个：`mise run infra:*` / Docker Compose 需要管理 PostgreSQL、Redis、RustFS、Caddy。`scripts/dev-api-run.sh` 在每次启动 API 时通过 `setpriv` 显式丢弃 `docker` 等附加组，因此 API 自身无法直接打开 `/var/run/docker.sock`。

源码、`target/`、`node_modules/` 和开发日志继续由开发者拥有，Rust 编译也始终由开发者执行，不会产生 root-owned 构建产物。

### 1.3 生产身份

```text
API container
├── user = 宿主 floatctf numeric UID:GID
├── docker group = 无
├── capabilities = 全部 drop
├── NoNewPrivileges = yes
├── rootfs = read-only
├── /var/run/docker.sock = 未挂载
└── helper sockets = /run/floatctf 只读目录挂载

floatctf-helper（宿主 systemd）
├── docker group
├── CAP_NET_ADMIN
└── 宿主控制面
```

生产已经没有 `floatctf-api.service`。`floatctf-infra.service` 负责 `docker compose up/down`，Compose 内运行 `floatctf-api`、PostgreSQL、Redis、RustFS 和 Caddy。

---

## 2. 首次准备

自动宿主初始化当前验证于 **Arch Linux + systemd**。开发者需要普通用户账号、sudo 权限和网络连接。

先安装并激活 mise：

```bash
curl https://mise.run | sh
# 按 mise 输出把 activate 行加入 shell 配置
mise --version
```

然后：

```bash
git clone https://github.com/FloatCTF/floatctf.git
cd floatctf
mise run setup
```

`mise run setup` 会完成：

1. 安装固定版本 Rust / Node / pnpm / Python / watchexec；
2. 执行 `pnpm install`、`cargo fetch`；
3. 编译 `floatctf-helper`；
4. 通过 sudo 检查/安装 Docker、nftables、WireGuard、iproute2 等宿主能力；
5. 配置 IPv4 forwarding、`br_netfilter` 与 bridge netfilter sysctl；
6. 创建 `floatctf` 与 `floatctf-helper` 系统身份；
7. 把 `floatctf-helper` 加入 `docker` 组；
8. 把开发者加入 `docker` 与 `floatctf` 组；
9. 安装 root-owned `/usr/local/libexec/floatctf-helper`；
10. 安装并启动 `floatctf-helper.service`；
11. 等待两个 helper socket 就绪后返回成功。

首次新增组权限后注销并重新登录一次，然后检查：

```bash
groups | tr ' ' '\n' | grep -E '^(docker|floatctf)$'
systemctl status floatctf-helper --no-pager
ls -l /run/floatctf/helper-control.sock /run/floatctf/helper-docker.sock
```

修改 `crates/floatctf-helper/` 或 `crates/helper-protocol/` 后重新执行：

```bash
mise run setup
```

它会重新编译、安装并启动最新 helper。

---

## 3. 日常开发

唯一入口：

```bash
mise run dev
```

启动顺序：

```text
Docker Compose up --wait
        ↓
PostgreSQL / Redis / RustFS / Caddy healthy
        ↓
migrate.sh apply
        ↓
检查 helper.service + 两个 Unix socket
        ↓
watchexec
        ↓
cargo build（开发者身份）
        ↓
setpriv 启动 API（丢弃 docker 组和 capabilities）
        ↓
Vite HMR
```

访问地址：

| 服务 | 地址 |
|---|---|
| 统一入口 | `http://0.0.0.0:7780`（局域网使用宿主机 IP） |
| API（监听） | `0.0.0.0:9090`（本机直连仍用 `http://127.0.0.1:9090`） |
| Vite（监听） | `0.0.0.0:13000`（本机直连仍用 `http://127.0.0.1:13000`） |
| PostgreSQL | `127.0.0.1:5432` |
| Redis | `127.0.0.1:6379` |
| RustFS API | `127.0.0.1:9000` |
| RustFS Console | `127.0.0.1:9001` |

开发端口默认只绑定回环地址。

Rust/TOML 修改后，`watchexec` 会重新编译并以受限组重新启动 API。前端由 Vite HMR 更新。

常用生命周期命令：

```bash
mise run dev
mise run dev:down
mise run dev:reset
mise run dev:logs
```

helper 日志：

```bash
journalctl -fu floatctf-helper
```

---

## 4. Docker 控制策略

API 的 Bollard client 连接 TOML 中的 helper socket：

```toml
[docker]
socket_path = "/run/floatctf/helper-docker.sock"

平台 API 始终固定走 helper。`fcmc` 自身的 CLI/SDK 则有独立 fallback：helper socket 不存在时可直连本机 Docker；socket 已存在但故障时不会绕过 helper。
```

API 不应出现新的 `/var/run/docker.sock` 或 `Docker::connect_with_local_defaults()` 生产代码。

helper Docker proxy 当前允许 FloatCTF 需要的容器、网络、镜像和 build API，并在高风险边界做约束：

```text
容器 create
├── 禁 privileged
├── 禁 bind/mount host path
├── 禁 Devices / DeviceRequests
├── 禁 CapAdd
├── 禁 host/container network mode
├── 禁 host PID/IPC/UTS/Cgroup/User namespace 模式
├── 只允许默认网络或 fctf-* / floatctf-* 网络
└── 自动写 floatctf.managed=true

网络 create
├── 只允许 bridge driver
├── 名字必须 fctf-* / floatctf-*
└── 自动写 floatctf.managed=true

已有容器/网络 mutation
└── 必须通过 FloatCTF ownership 校验

image tag / push / delete
└── 目标 image 必须带 FloatCTF managed label
```

管理后台的 Docker 页面仍可以读取宿主 Docker 状态；对非 FloatCTF-owned 容器/网络的修改会被 helper 拒绝。

---

## 5. 数据库迁移

`apps/api/src/sql/migrations/` 是 Schema source of truth。开发 fresh PostgreSQL 直接从 migration #1 执行，不依赖 `merged.sql`。

日常 Schema 变更：

```bash
mise run db:migration:new <名称>
# 只编辑刚创建的新 migration
mise run db:migration:validate
mise run db:migration:apply
mise run db:gen
```

已有 migration 永远不可改。`schema_migrations` 由 `migrate.sh` 独占维护。

`merged.sql` 只用于 release / 生产 fresh install：

```bash
mise run db:migration:merge
```

Release workflow 会确定性重新生成它。

---

## 6. 开发与生产对照

| 项目 | 开发 | 生产 |
|---|---|---|
| 主入口 | `mise run dev` | `systemctl start floatctf.target` |
| API 载体 | native `watchexec + setpriv` | Docker Compose container |
| API UID | 当前开发者 | 宿主 `floatctf` numeric UID |
| API docker group | 启动时显式丢弃 | 无 |
| API capabilities | 全部丢弃 | `cap_drop=ALL` |
| API NoNewPrivileges | yes | yes |
| API rootfs | 宿主源码进程 | read-only container rootfs |
| 宿主控制面 | `floatctf-helper.service` | `floatctf-helper.service` |
| helper 用户 | `floatctf-helper` | `floatctf-helper` |
| helper docker group | 有 | 有 |
| helper capability | `CAP_NET_ADMIN` | `CAP_NET_ADMIN` |
| Host RPC | `/run/floatctf/helper-control.sock` | 同左 |
| Docker policy proxy | `/run/floatctf/helper-docker.sock` | 同左 |
| PostgreSQL/Redis/RustFS/Caddy | Docker Compose | Docker Compose |
| API 产物 | debug + watchexec | release binary → local runtime image |
| Web | Vite HMR | Caddy static dist |
| fresh DB | migrations | `merged.sql` |
| API listen | `0.0.0.0:9090`（仅开发；供 Caddy 容器经 host-gateway 访问） | 不发布 |
| 外部入口 | `0.0.0.0:7780` | Caddy HTTP（开发模式，对所有宿主 IPv4 接口开放） |

---

## 7. 配置

开发配置：

```text
apps/api/config/development.toml
```

`mise.toml` 设置：

```text
FLOATCTF_CONFIG=<repo>/apps/api/config/development.toml
```

关键宿主控制配置：

```toml
[docker]
socket_path = "/run/floatctf/helper-docker.sock"

[awd]
network_runtime = "helper"
```

`network_runtime = "noop"` 只用于 unit test / mock。

Host RPC socket `/run/floatctf/helper-control.sock` 是架构常量，由 `helper-protocol` 统一定义。

开发 `[awd].platform_internal_url` 只提供 scheme + port，赛事部署时 host 会替换为该赛事 infra bridge gateway；生产则设置 `platform_internal_network = "fctf-platform-control"`，API/FlagServer/JudgeServer 通过独立 internal control network 互通。这样开发 API 可以保持原生热重载，生产 API 也无需发布宿主 9090。

---

## 8. 提交前检查

```bash
mise run check
```

Release 构建：

```bash
mise run build
```

helper 单独测试：

```bash
cargo test -p floatctf-helper
```

---

## 9. 常见故障

| 症状 | 处理 |
|---|---|
| `docker info` permission denied | setup 后重新登录，确认开发者在 `docker` 组；该权限只供 Compose 使用 |
| helper socket permission denied | 确认当前用户在 `floatctf` 组并重新登录 |
| helper 未运行 | `systemctl status floatctf-helper`、`journalctl -u floatctf-helper` |
| API 启动时报 helper unavailable | 检查两个 `/run/floatctf/helper*.sock` |
| API Docker 返回 403 | helper policy 拒绝了超出 FloatCTF ownership/安全策略的 Docker 操作 |
| API 意外能打开 `/var/run/docker.sock` | 检查 `scripts/dev-api-run.sh` 的 setpriv 组收敛是否生效 |
| PostgreSQL 5432 被占用 | 停止旧开发容器/占用进程后重试 `mise run dev` |
| Caddy 502 | 检查 API 9090 与 Vite 13000 |
| migration history mismatch | 新增修复 migration，禁止改历史 migration |
| helper 源码更新后行为没变化 | 重新执行 `mise run setup` |

---

## 10. 真实宿主验证

源码重构、unit test、编译检查可以直接执行。以下验证会实际修改宿主 Docker / WireGuard / nftables 状态，需要在明确的测试步骤中执行：

```text
setup → helper identity/socket → Docker proxy policy → API 权限 → Jeopardy → AWD → AWDP → cleanup/recovery
```

验证过程中不要使用 `nft flush ruleset`、删除无关 Docker 对象或改动无关 WireGuard 接口。FloatCTF 动态资源必须限定在自身 naming/label contract 内。

---

## 11. Agent 协同

Agent 开始工作前读取 [AGENTS.md](./AGENTS.md) 和根目录 `HANDOFF.md`（存在时）。数据库、测试、前端等规范继续以 `docs/agents/` 为准。

开发环境统一使用 `mise run dev`，禁止恢复第二套开发轨道或让 API 重新获得 docker group / `CAP_NET_ADMIN`。
