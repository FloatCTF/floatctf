# Phase 10 Deployment Foundation — Design（部署地基设计）

> 分支 `awd`，2026-09-01。Phase 9 判定 PASS 后进入本阶段。
> 本阶段仅产出：部署审计、`scripts/init.sh`（主机初始化）、compose/systemd 设计。
> 不实现 `deploy.sh`（无足够运行时事实前不盲写）。

## 1. 目标架构

```
HOST（Arch Linux，系统管理 Docker/nftables/WireGuard）
├── systemd: floatctf-api.service   ← FloatCTF API 原生进程（本机二进制）
├── docker (host-managed daemon)
│   ├── floatctf-postgres（PostgreSQL 容器，仅回环私有端口）
│   ├── floatctf-nginx（nginx 容器，80/443）
│   └── （AWD 动态运行时：GameBox / FlagServer / JudgeServer 按赛事创建）
├── nftables（host-managed，FloatCTF 表 + 系统表共存）
└── WireGuard（host-managed，赛事接口由 API 经 HostNetworkRuntime 管理）
```

API 不容器化（本阶段明确不做）。PostgreSQL / nginx 容器化以避免与宿主已有
PostgreSQL/nginx 实例冲突、避免发行版相关原生安装。

## 2. 观察到的仓库事实（Phase 10.1 审计，均为实测值，不猜测）

| 项 | 实测事实 |
|---|---|
| API crate | `apps/api/Cargo.toml`：`name = "floatctf"`，v0.3.3，单二进制 `floatctf` |
| API 监听 | `[server] listen_ip`（dev 默认 `0.0.0.0`）/ `listen_port`（dev 默认 `9090`），`bootstrap/mod.rs:229` |
| 配置格式 | TOML，经 `FLOATCTF_CONFIG` 指定（AGENTS.md 铁律 1：配置只从 TOML 读取） |
| 运行根 | `[server] work_dir`（相对启动 CWD 解析）：dev `../../app`；phase9 `../../app-phase9`；`{{WORK_DIR}}/challenges`、`{{WORK_DIR}}/logs/<svc>` 以此为锚 |
| 数据库 | `[database] url`：dev `postgres://postgres:postgres@127.0.0.1:5432/floatctf_db` |
| 迁移 | `apps/api/src/sql/migrate.sh`：forward-only、advisory lock `0x464C4154`、`schema_migrations` 由脚本独占、`merged.sql`（337KB）为 fresh bootstrap；mise 任务 `db:migration:*` 包装 |
| 对象存储 | **必须**：`[rustfs]`（endpoint_url/region/ak/sk）；dev 容器 `rustfs/rustfs:latest` 回环 `127.0.0.1:9000/9001` |
| 前端 | `apps/web`：`vite build && tsc` → `apps/web/dist/`（实测 8.9MB：assets + favicon + FloatCTF.png），由 nginx 静态托管 |
| nginx dev | `nginx:1.26-bookworm`，`:7780→80`，挂载 `infra/nginx/nginx.dev.conf` |
| SSE 代理 | dev conf 已含：`proxy_buffering off; proxy_request_buffering off; proxy_read_timeout 300s; proxy_send_timeout 300s;` + WS Upgrade 头 + `proxy_http_version 1.1` |
| nginx→host API | `extra_hosts: host.docker.internal:host-gateway` + 静态 upstream `api_backend`（nginx 用 /etc/hosts 解析，不依赖 Docker 内嵌 DNS） |
| nginx→RustFS | Docker 内嵌 DNS `resolver 127.0.0.11` + 变量 proxy_pass |
| AWD 镜像 | `floatctf/awd-flagserver:latest` / `floatctf/awd-judgeserver:latest`；Dockerfile 在 `chore/awd-phase9-e2e/`（aliyun 镜像容器 + host rustup 构建，glibc 2.34） |
| AWD 运行时 | `[awd] network_runtime` 默认 `"noop"`，`"host"` → `HostNetworkRuntime`（真实 nftables + WireGuard + docker_forward）；`platform_internal_url` 为模板，host 由 `derive_event_internal_platform_url` 派生 |
| mise 任务 | `build/check/db:*/dev*/fmt/infra:*/install/lint/test`（`~/.local/bin/mise`） |
| 现有脚本 | `scripts/init.sh`（**旧版发布安装器辅助**，260 行）+ `scripts/install.sh`（**旧版全量安装器**，730 行：原生 apt 装 postgres、源码编 nginx、原生 rustfs、systemd 单元）→ Phase 10 目标架构取代之 |
| 宿主前置 | Phase 9 实测必须：Docker daemon（29，nftables 后端）、`nft`、`wg`、`ip`、`net.ipv4.ip_forward=1`、`br_netfilter` 模块 + `bridge-nf-call-{ip,ip6}tables=1`（API reconcile 时 best-effort modprobe/sysctl，生产应在 init.sh 预检） |

## 3. 部署布局决策

`/home/floatctf/`（与旧版发布模型 `/app` 并列但不混淆；旧模型保留不动）：

```
/home/floatctf/
├── bin/            # floatctf API 二进制（release 构建，musl/glibc 视部署目标）
├── web/            # apps/web/dist 产物（由发布流程拷入）
├── config/
│   ├── floatctf.toml
│   └── nginx/nginx.conf
├── data/postgres/  # PostgreSQL named volume 由 compose 管理（不直接落裸目录）
├── logs/           # api / nginx
├── runtime/        # work_dir 指向此处：challenges/、api 等运行时数据
├── gameboxes/      # AWD GameBox zip / 镜像构建产物
├── .env            # 部署变量（端口/密钥来源引用，不存密钥本体）
└── compose.yml     # postgres + nginx（+ rustfs）容器编排
```

决策依据：API 的 `work_dir` 已承担 `logs/`、`challenges/` 等相对路径锚点（§2 实测），
故 `runtime/` 作为 `work_dir` 绝对目标；`web/` 与 nginx 静态挂载分离；Postgres 数据
走 compose named volume（`data/postgres` 仅为运维可见性注释）。

## 4. PostgreSQL 容器设计（Phase 10.4）

- 镜像：`postgres:17`（与 dev compose `postgres:17` 一致，锁主版本）。
- 网络：**不发布公共端口**；仅回环 `127.0.0.1:<私有高位端口> → 5432`（默认 `5433`，
  避开宿主 5432 冲突；端口可配置）。`POSTGRES_USER/PASSWORD/DB` 由部署 .env 注入。
- 持久化：named volume `floatctf-postgres-data`（compose 管理，避免裸目录权限/备份混淆）。
- 健康检查：`pg_isready -U $POSTGRES_USER -d $POSTGRES_DB`。
- 迁移：**不使用** dev 式 `merged.sql` initdb 自动初始化（生产禁止隐式重建）；
  由部署流程在容器就绪后显式执行 `apps/api/src/sql/migrate.sh`（forward-only +
  advisory lock，幂等，绝不破坏性重建数据库）。`schema_migrations` 由 migrate.sh 独占。
- 禁止：`0.0.0.0:5432`、自动 DROP DATABASE/CREATE DATABASE、公开暴露。

## 5. nginx 容器设计（Phase 10.5）

- 镜像：`nginx:1.26-bookworm`（与 dev 一致）。
- 职责：前端静态资产（挂载 `$HOME/floatctf/web/` → `/usr/share/nginx/html:ro`）+
  反向代理 `/api/` → host API + `/public/` → RustFS（容器内 DNS）。
- **host API 可达机制（决策）**：`extra_hosts: "host.docker.internal:host-gateway"` +
  静态 upstream（dev 已实测有效的同款模式）。**不用** host 网络模式、**不授予**
  `privileged` / `NET_ADMIN` / docker.sock。
- SSE：继承 dev conf 已证的设置（`proxy_buffering off` + 300s 读写超时 + WS Upgrade）。
  注：Phase 9 观测到经 nginx 的长驻 SSE 空闲后 `net::ERR_ABORTED`（D 类记录），
  部署前应复测；缓解方向：提高 `proxy_read_timeout` 或 SSE 心跳。
- 端口：80/443 可配置（`HTTP_PORT`/`HTTPS_PORT`）；**部署前检测端口占用**，
  发现冲突时明确报错并退出（绝不自动停用无关进程）。
- TLS 结构预留：`keys/` 挂载目录 + 443 server 块模板（证书由运维提供，init.sh 不生成）。

## 6. systemd 设计（Phase 10.6）

**决策：拓扑 A** —— `floatctf-api.service` + `floatctf-infra.service` + `floatctf.target`。

对比：

| | A: api + infra + target | B: api + postgres + nginx + target |
|---|---|---|
| 单元数 | 3 | 4 |
| 容器编排 | compose 单入口（postgres/nginx/rustfs 顺序由 compose depends_on + restart 管） | 每容器一个单元，重复实现 compose 的排序/重启/健康语义 |
| 可靠性 | infra 单元 = `docker compose up -d`，API `After=floatctf-infra.service`；compose 自带 restart 策略 | 单元间需手写 BindsTo/After 链 |
| 生命周期语义 | 清晰：infra 容器生命周期与 API 解耦（容器崩了 compose 拉起，不影响 API 单元） | 过度耦合 |

单元形态：

```
floatctf-infra.service   # ExecStart=docker compose -f /home/floatctf/compose.yml up -d
                         # ExecStop=docker compose down（保留 volume）
floatctf-api.service     # ExecStart=/home/floatctf/bin/floatctf
                         # WorkingDirectory=/home/floatctf
                         # EnvironmentFile=/home/floatctf/.env（FLOATCTF_CONFIG 指向 config/floatctf.toml）
                         # After=network-online.target floatctf-infra.service
floatctf.target          # 聚合上述两个单元（Wants/After）
```

旧版发布模型的三个原生单元（api/nginx/rustfs）被本设计取代：nginx/rustfs 进容器、
postgres 进容器、API 保持原生。

## 7. init.sh 设计（Phase 10.2，HOST INITIALIZATION ONLY）

- 位置：`scripts/init.sh`（替换旧版发布安装器辅助脚本；旧脚本 `git mv` 到
  `scripts/legacy/init.release.sh` 保留，`install.sh` 不引用它，无调用方破坏）。
- 职责（只检查/准备，不安装应用）：
  - 受支持 Linux（内核 ≥ 5.x 且非容器环境检测）
  - 发行版检测：`pacman`（Arch 路径实现）/ `apt-get` / `dnf`（仅检测，明确报错不实现，
    避免在未确认包名时盲装）
  - Docker：`docker info` + 临时网络 `fctf-init-$$` 创建/删除（trap 清理）验证可用性
  - nftables：`nft` 存在 + 临时表 `inet fctf_init_$$` 添加/删除（trap）
  - WireGuard：`wg` 存在 + 临时 `ip link add fctf-init-$$ type wireguard` 删除（trap）
  - iproute2：`ip` 存在
  - `net.ipv4.ip_forward=1`（读取/写入，写入需 root）
  - `br_netfilter`：`modprobe br_netfilter` + `bridge-nf-call-{ip,ip6}tables=1`
    （幂等；Phase 9 实测必须，同桥隔离依赖）
  - `floatctf` 服务用户（`id floatctf` 不存在则 `useradd --system`，不设 shell）
  - `/home/floatctf` 布局 mkdir（§3）
- 明确**不做**：装 PostgreSQL/nginx/Rust/Node、构建 FloatCTF、部署、建库、
  起 postgres/nginx 容器、创建 systemd 单元、创建生产赛事网络。
- 破坏性网络检查一律临时唯一命名资源 + `trap ... EXIT` 清理；失败即退出（fail-closed）。

## 8. 生产迁移工作流

```
docker compose up -d db → pg_isready 健康 → ./apps/api/src/sql/migrate.sh \
  <DATABASE_URL> → 校验 schema_migrations → 启动 API
```
（`migrate.sh` 支持 DATABASE_URL 传参——以实际脚本参数为准，部署时确认；绝不自动重建。）

## 9. 遗留风险 / 未决项（记录，不阻塞本阶段）

1. nginx SSE 空闲 ERR_ABORTED（Phase 9 D 类）——部署前复测，可能需心跳或更长超时。
2. AWD 镜像构建流程（aliyun 容器 + host rustup）应沉淀为正式构建脚本（当前仅在
   `chore/awd-phase9-e2e/` 有 Dockerfile）。
3. RustFS 生产容器化：与 postgres/nginx 同 compose（dev 已容器化，直接沿用模式）。
4. `scripts/install.sh`（旧发布模型）与 Phase 10 并存策略待产品决策（本阶段不动它）。
