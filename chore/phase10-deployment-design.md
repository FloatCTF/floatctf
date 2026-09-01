# Phase 10 Deployment Foundation — Design（部署地基设计）

> 分支 `awd`，2026-09-01（Phase 9 PASS 后进入）。Phase 9.2 于同日封存（SEALED）。
> 本阶段产出：部署审计、`scripts/init.sh`（主机初始化）、`scripts/build-release.sh`
> （可移植发布产物）、生产 compose/systemd、`scripts/deploy.sh`（部署器）。
> 设计文档随实现迭代；实现时以本文定稿为准，未决项在 §9 如实标注。

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
| 对象存储 | **必须（生产必需容器）**：`[rustfs]`（endpoint_url/region/ak/sk）；dev 容器 `rustfs/rustfs:latest` 回环 `127.0.0.1:9000/9001`。API 启动即连 RustFS，缺失直接 panic（bootstrap 实测）→ 生产 compose 必含 rustfs 服务 |
| 前端 | `apps/web`：`vite build && tsc` → `apps/web/dist/`（实测 8.9MB：assets + favicon + FloatCTF.png），由 nginx 静态托管 |
| nginx dev | `nginx:1.26-bookworm`，`:7780→80`，挂载 `infra/nginx/nginx.dev.conf` |
| SSE 代理 | dev conf 已含：`proxy_buffering off; proxy_request_buffering off; proxy_read_timeout 300s; proxy_send_timeout 300s;` + WS Upgrade 头 + `proxy_http_version 1.1` |
| nginx→host API | **生产决策：`network_mode: host` + `127.0.0.1:<API_PORT>`**（见 §5）。nginx 容器与宿主同网络命名空间，proxy_pass 走宿主回环直连 API 进程。不再依赖 `host.docker.internal` 与静态 upstream。dev 的 bridge+extra_hosts 仅作为 dev 兼容保留 |
| API 监听 | **必须 `0.0.0.0`**（dev/phase9 均如此，Phase 9 实测）：AWD FlagServer/JudgeServer 经赛事 infra 网关（docker bridge IP）回连平台（`derive_event_internal_platform_url`），仅回环监听会导致 AWD 回连失败。对外暴露面由宿主 nftables 限制，不在 API listen_ip 收紧 |
| nginx→RustFS | 生产走 `network_mode: host` 下 `127.0.0.1:9000` 直连（宿主回环，Docker DNS 在 host 模式下不可用，故不用变量 proxy_pass + resolver） |
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
│   ├── floatctf.toml     # 生产配置（含密钥；root:floatctf 640）
│   ├── nginx/nginx.conf  # 生产 nginx 配置（host 网络模式）
│   └── nginx/keys/       # TLS 证书挂载预留（运维外置）
├── data/
│   ├── postgres/   # PostgreSQL 数据目录（compose bind-mount）
│   └── rustfs/     # RustFS 数据目录（compose bind-mount）
├── logs/           # api / nginx / rustfs
├── runtime/        # work_dir 指向此处：challenges/、api 等运行时数据
├── gameboxes/      # AWD GameBox zip / 镜像构建产物
├── .env            # 部署变量（端口/密钥来源引用，不存密钥本体）
├── compose.yml     # postgres + rustfs + nginx 容器编排
└── .initialized    # init.sh 完成标记（幂等判定）
```

决策依据：API 的 `work_dir` 已承担 `logs/`、`challenges/` 等相对路径锚点（§2 实测），
故 `runtime/` 作为 `work_dir` 绝对目标；`web/` 与 nginx 静态挂载分离。

## 4. PostgreSQL 容器设计（Phase 10.4）

- 镜像：`postgres:17`（与 dev compose `postgres:17` 一致，锁主版本）。
- 网络：**不发布公共端口**；仅回环 `127.0.0.1:<私有高位端口> → 5432`（默认 `5433`，
  避开宿主 5432 冲突；端口可配置）。`POSTGRES_USER/PASSWORD/DB` 由部署 .env 注入。
- **存储模型（决策定稿）：bind-mount `/home/floatctf/data/postgres` → `/var/lib/postgresql/data`**。
  理由：数据落 `/home/floatctf` 统一布局，运维可见、备份/迁移直接打包目录即可，
  不依赖 Docker named volume 的 `/var/lib/docker/volumes` 隐藏路径；同时避免裸目录
  权限/初始化分叉。`data/postgres` 目录由 init.sh 创建并 `chown 999:999`（postgres 容器
  内 UID），compose 挂载后由 postgres 镜像 initdb 填充。
- 健康检查：`pg_isready -U $POSTGRES_USER -d $POSTGRES_DB`。
- 迁移：**不使用** dev 式 `merged.sql` initdb 自动初始化（生产禁止隐式重建）；
  由部署流程在容器就绪后显式执行 `apps/api/src/sql/migrate.sh`（forward-only +
  advisory lock，幂等，绝不破坏性重建数据库）。`schema_migrations` 由 migrate.sh 独占。
- 禁止：`0.0.0.0:5432`、自动 DROP DATABASE/CREATE DATABASE、公开暴露。

## 5. nginx 容器设计（Phase 10.5）

- 镜像：`nginx:1.26-bookworm`（与 dev 一致）。
- **网络模式（决策定稿）：`network_mode: host`**。nginx 与宿主共享网络命名空间，
  `listen 80/443` 直绑宿主端口；`/api/` 与 `/public/`、`/private/` 均 `proxy_pass`
  到 `127.0.0.1`（nginx→API 走宿主回环；API 本身监听 `0.0.0.0` 以服务 AWD 容器
  经 infra 网关的回连，见 §2 表；RustFS 容器仅回环发布，宿主回环可达）。
  依据：API 是原生 systemd 进程而非容器，Docker 桥接/`host.docker.internal` 方案在
  生产引入不必要的网关依赖；host 网络下 nginx 是唯一对外暴露面，直接由宿主 nftables
  管辖，语义最清晰。dev 的 bridge+extra_hosts 仅作为 dev 兼容保留，生产不复用。
- **不授予** `privileged` / `NET_ADMIN` / docker.sock（host 网络已足够，最小权限）。
- 职责：前端静态资产（挂载 `/home/floatctf/web/` → `/usr/share/nginx/html:ro`）+
  反向代理 `/api/` → `127.0.0.1:<API_PORT>` + `/public/`、`/private/` → `127.0.0.1:9000`。
- SSE：继承 dev conf 已证的设置（`proxy_buffering off` + 300s 读写超时 + WS Upgrade）。
  注：Phase 9 观测到经 nginx 的长驻 SSE 空闲后 `net::ERR_ABORTED`（D 类记录）；
  Phase 9.2 A1 已在 dev 链路验证前端 fetch-SSE 断线重连 + 轮询回退自愈；生产部署后
  需复测（§I 验收包含 SSE through prod nginx）。
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

单元形态（**就绪语义：`--wait`，容器 healthcheck 通过才算 infra ready**）：

```
floatctf-infra.service   # ExecStart=docker compose -f /home/floatctf/compose.yml up -d --wait
                         # （--wait：等到所有服务 healthcheck 通过才返回，非"已启动"）
                         # ExecStop=docker compose down（保留 data/postgres 数据目录）
floatctf-api.service     # ExecStart=/home/floatctf/bin/floatctf
                         # User/Group=floatctf  SupplementaryGroups=docker
                         # WorkingDirectory=/home/floatctf/runtime
                         # EnvironmentFile=/home/floatctf/.env（FLOATCTF_CONFIG=/home/floatctf/config/floatctf.toml）
                         # AmbientCapabilities=CAP_NET_ADMIN  CapabilityBoundingSet=CAP_NET_ADMIN
                         # （AWD host runtime 需要 CAP_NET_ADMIN 管理 nftables/WireGuard/Docker 转发；
                         #  绝不 PrivateNetwork —— API 必须操作宿主网络）
                         # After=network-online.target floatctf-infra.service
                         # 软依赖 db/rustfs（infra --wait 已保证就绪，api 不重复等待）
floatctf.target          # 聚合上述两个单元（Wants/After）
```

API 能力最小化原则：仅 `CAP_NET_ADMIN`（HostNetworkRuntime 底线）。不授予
`CAP_NET_RAW`/`CAP_SYS_ADMIN` 等；不 `PrivateNetwork`、不 `PrivateTmp` 冲突项（以实测为准）。

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
  - `net.ipv4.ip_forward=1`（读取/写入，写入需 root）——**持久化到
    `/etc/sysctl.d/99-floatctf.conf`**（FloatCTF 自有文件，重启后仍生效）
  - `br_netfilter`：`modprobe br_netfilter` + `bridge-nf-call-{ip,ip6}tables=1`
    ——**持久化**：模块写 `/etc/modules-load.d/floatctf-br-netfilter.conf`，
    内核参数写 `99-floatctf.conf`（幂等；Phase 9 实测必须，同桥隔离依赖）
  - `floatctf` 服务用户（`id floatctf` 不存在则 `useradd --system --shell nologin`，
    并加入 docker 组 `usermod -aG docker floatctf`；docker 组验证用 `runuser -u floatctf -- docker info`）
  - `/home/floatctf` 布局 mkdir（§3）：`config/` 属 root 640，`runtime/logs/web/bin/data`
    属 floatctf
  - 完成标记：`/home/floatctf/.initialized`（原子写入；已存在则幂等跳过主体检查，
    仍复跑预检）
- 明确**不做**：装 PostgreSQL/nginx/Rust/Node、构建 FloatCTF、部署、建库、
  起 postgres/nginx 容器、创建 systemd 单元、创建生产赛事网络。
- 破坏性网络检查一律临时唯一命名资源 + `trap ... EXIT` 清理；失败即退出（fail-closed）。

## 8. 生产迁移工作流

```
docker compose up -d db --wait → pg_isready 健康 → ./apps/api/src/sql/migrate.sh \
  <DATABASE_URL> → 校验 schema_migrations → 启动 API
```
（`migrate.sh` 支持 DATABASE_URL 传参——以实际脚本参数为准，部署时确认；绝不自动重建。）

## 9. 遗留风险 / 未决项（记录，不阻塞本阶段）

1. ~~nginx SSE 空闲 ERR_ABORTED~~（Phase 9 D 类）——Phase 9.2 A1 已在 dev 链路验证
   前端 fetch-SSE 断线重连 + 轮询回退自愈；生产 host 网络模式 + AWD 复测在 §I 验收执行。
2. AWD 镜像构建流程（aliyun 容器 + host rustup）→ **本阶段沉淀为
   `scripts/build-release.sh`**（见 §10），Dockerfile 迁移出 `chore/awd-phase9-e2e/`。
3. ~~RustFS 生产容器化~~ —— 已定稿：与 postgres/nginx 同 compose，且为**必需**服务
   （API 启动连不上 RustFS 即 panic，bootstrap 实测）。
4. `scripts/install.sh`（旧发布模型）与 Phase 10 并存策略待产品决策（本阶段归档
   为 `scripts/legacy/install.sh`，不删除历史）。
5. TLS 证书供应链：本阶段只留结构（443 server 块 + keys/ 挂载），证书由运维外置。

## 10. 发布产物设计（Phase 10.3，scripts/build-release.sh）

- 目标：单主机可移植发布包，含四类产物：
  1. **API 二进制**：`cargo build --release`（crate `floatctf`，单二进制）。
     **musl 优先**（`x86_64-unknown-linux-musl`，静态链接，任意 glibc 主机可运行）；
     musl 不可用/构建失败时回退**容器内 glibc-2.34 基线构建**（沿用 Phase 9 实测：
     aliyun 镜像 + host rustup，`objdump -T` 验证 GLIBC 需求 ≤ 2.34）。
  2. **Web dist**：`apps/web` → `pnpm build`（vite build && tsc）→ `dist/`。
  3. **FlagServer 镜像**：`floatctf/awd-flagserver:<tag>`。
  4. **JudgeServer 镜像**：`floatctf/awd-judgeserver:<tag>`。
- Dockerfile（flagserver/judgeserver）从 `chore/awd-phase9-e2e/` 迁移到
  `infra/docker/awd-flagserver/` 与 `infra/docker/awd-judgeserver/`（生产正式路径）。
- 产物目录：`release/floatctf-<version>/`（bin/ web/ images/ checksums.txt）。
- **不推送镜像**（本阶段本地构建，不 `docker push`；tag 用 `floatctf/awd-*:<version>`，
  部署时 `docker load` 或本地 daemon 直用）。
- 审计：`file`（ELF 静态/动态）、`ldd`（动态依赖）、`readelf -d`（NEEDED）验证可移植性。
