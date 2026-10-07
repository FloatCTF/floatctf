# FloatCTF 安装与部署

本文档是 FloatCTF **生产安装、容器化部署、systemd 运维、权限模型和生命周期管理**的权威说明。本地源码开发见 [DEVELOPMENT.md](./DEVELOPMENT.md)。

---

## 0. 动手之前：单控制面不变量（R1，必读）

> ⚠️ **单控制面不变量（R1）—— 同一宿主/helper/Docker daemon 同一时刻只允许一个 FloatCTF 控制面（API）。**
> 第二套 API —— 包括开发栈、RC/测试栈或第二份生产安装 —— 会 reconcile 全局/固定命名的宿主资源
> （AWD 防火墙是单一全局 nft 表 `floatctf_awd`；AWDP 练习网络与容器名固定为 `fctf-awdp-practice` /
> `fctf-awdp-practice-judge`），从而干扰正在运行的实例。**这在 Phase 13 实测发生过**（第二个 API
> 重建了线上练习判题容器与网络一次），不是理论风险。
> **绝不要在承载生产实例的宿主上启动开发 / RC / 测试 API。**

实践含义：

- 迁移/移植到新宿主时，确认**旧**宿主上的 API 已经停机，不要"两台并行跑一段时间"。
- 在一台宿主上做升级、验收、备份恢复或故障演练时，**不要**另起 `mise run dev`、
  RC stack 或第二份安装。
- 需要独占宿主做宿主级验证（AWD/AWDP E2E、privileged 安装/卸载/purge）时，用一台专用机器。

## 1. 生产拓扑

生产环境把普通应用面全部收进 Docker Compose，宿主只保留一个高权限控制面 `floatctf-helper`：

```text
Internet
   │
   ▼
Caddy container :80/:443
   ├── Web static
   ├── /api/* ───────────────► API container :9090
   └── object paths ─────────► RustFS :9000

Compose default network
   ├── API
   ├── PostgreSQL
   ├── Redis
   ├── RustFS
   └── Caddy

API container
   ├── non-root numeric floatctf UID:GID
   ├── cap_drop=ALL
   ├── no-new-privileges
   ├── read-only root filesystem
   ├── /run/floatctf/helper-control.sock ─────────► Host RPC
   └── /run/floatctf/helper-docker.sock ─► Docker policy proxy
                                                │
                                                ▼
                                      floatctf-helper.service
                                      ├── docker group → Docker Engine
                                      └── CAP_NET_ADMIN
                                          ├── WireGuard
                                          ├── nftables
                                          ├── conntrack
                                          └── Docker FORWARD
```

API **不发布宿主 9090 端口**。公网入口只有 Caddy 的 HTTP/HTTPS。Caddy 在 Compose default network 内直接访问 `api:9090`。

AWD/AWDP 内部服务使用额外的 control plane：

```text
fctf-platform-control
subnet   : 10.42.8.0/24
internal : true
API      : 10.42.8.2

API ───────────────┐
FlagServer ────────┼── fctf-platform-control
JudgeServer ───────┘

GameBox ───────────── 不加入该网络
```

因此 FlagServer/JudgeServer 可以回调 `http://10.42.8.2:9090`，同时不需要把 API 端口暴露到宿主公网接口。赛事 data network、GameBox 隔离和 WireGuard 仍按各赛制自己的网络模型运行。

---

## 2. systemd 与 Compose 的职责

生产 systemd 只管理两个 service 和一个 target：

| 单元 | 身份 | 用途 |
|---|---|---|
| `floatctf-helper.service` | `floatctf-helper` + docker group + `CAP_NET_ADMIN` | 唯一宿主高权限控制面 |
| `floatctf-infra.service` | systemd/root | 执行生产 `docker compose up/down` |
| `floatctf.target` | systemd target | 聚合 helper + Compose stack |

`floatctf-api.service` 已退出当前架构。安装器会清理旧版 native API unit，避免同一台机器出现两份 API。

Compose 管理：

```text
floatctf-api
floatctf-postgres
floatctf-redis
floatctf-rustfs
floatctf-caddy
```

`floatctf-infra.service` 依赖 `floatctf-helper.service`，确保 API 启动前 helper socket 已经可用。

---

## 3. API 容器安全边界

生产 API service 使用以下约束：

```yaml
user: "${FLOATCTF_UID:-65532}:${FLOATCTF_GID}"
cap_drop:
  - ALL
security_opt:
  - no-new-privileges:true
read_only: true
tmpfs:
  - /tmp:rw,noexec,nosuid,nodev,size=64m
```

只挂载：

```text
config/floatctf.toml -> /etc/floatctf/floatctf.toml 只读
runtime/             -> /var/lib/floatctf/runtime 可写
/run/floatctf/       -> /run/floatctf 只读目录挂载
```

API 容器不会挂载：

```text
/var/run/docker.sock
/
/dev
/proc host namespace
宿主任意目录
```

宿主侧只创建 `floatctf` **组**，不创建 `floatctf` 用户：API 在容器内以数值 UID/GID 运行（`FLOATCTF_UID` 默认 `65532`，与 API 镜像的 `USER 65532:65532` 对齐；`FLOATCTF_GID` 是 `floatctf` 组的 GID），这样 `/run/floatctf/helper*.sock` 的 `floatctf` group 权限无需在容器内创建同名组也能正确生效。

---

## 4. helper 的两个接口

### 4.1 Host RPC

```text
/run/floatctf/helper-control.sock
owner: floatctf-helper
 group: floatctf
 mode : 0660
```

协议由 `crates/helper-protocol` 定义。当前 Host RPC 覆盖：

```text
WireGuard interface / peer
nftables FloatCTF-owned tables
conntrack flush
host route observation
Docker FORWARD compatibility rules
health ping
```

Host RPC 只接受结构化操作，不提供任意 shell/command RPC。

### 4.2 Docker policy proxy

```text
/run/floatctf/helper-docker.sock
owner: floatctf-helper
 group: floatctf
 mode : 0660
```

API 的 Bollard client 把它当作 Docker-compatible Unix socket。helper 再访问真实 `/var/run/docker.sock`。

主要策略：

```text
容器 create
  deny privileged
  deny host bind/mount
  deny Devices / DeviceRequests
  deny CapAdd
  deny host/container network
  deny host PID/IPC/UTS/Cgroup/User namespace
  deny arbitrary sysctl
  named network 只允许 fctf-* / floatctf-*
  inject floatctf.managed=true

网络 create
  bridge only
  name 必须 fctf-* / floatctf-*
  inject floatctf.managed=true

已有容器/网络 mutation
  require FloatCTF ownership

image tag / push / delete
  require FloatCTF managed image label
```

Docker group 本身具有极高宿主控制能力，因此 `floatctf-helper` 是高信任控制面。API 只拥有 helper 暴露的受限能力。

---

## 5. 环境要求

自动安装路径当前验证于 **Arch Linux + systemd**。需要：

- Docker Engine + Docker Compose v2
- nftables
- wireguard-tools
- iproute2
- conntrack tools
- iptables（Docker FORWARD 兼容规则）
- PostgreSQL client
- **`python3` 且带 stdlib `tomllib`（即 Python ≥ 3.11）** —— 见下
- curl / tar / openssl
- systemd
- 出网（拉取 release 产物与 GHCR 运行时镜像；离线见 §6.2）

### 5.1 Python / `tomllib` 前置条件（R3）

`apps/api/src/sql/migrate.sh` 用 **`tomllib`** 解析配置，因此安装器要求 `python3` **自带**该
stdlib 模块。检查方式是**能力检查**，在任何改动宿主之前运行：

```bash
python3 -c 'import tomllib'    # 安装器内部执行的等价检查
```

- 缺失（例如系统自带 Python 3.10 或更早）→ 安装器 **fail-fast**，不会做任何变更。
- **不会**、也不需要 `pip install`：要求的模块必须在标准库里。
- 请用发行版包管理器安装/升级 `python3`（Python 3.11+），不要创建 venv 或改 `PYTHONPATH`。

> 说明：`frontend.sh` 另需 `python3` 做 JSON 解析与原子注册表更新；同一个解释器满足两者。

### 5.2 宿主网络参数

需要的宿主网络参数：

```text
net.ipv4.ip_forward=1
br_netfilter
net.bridge.bridge-nf-call-iptables=1
net.bridge.bridge-nf-call-ip6tables=1
```

安装器会持久化 FloatCTF 需要的 sysctl/modules 配置。

---

## 6. Release 产物与 API / 运行时镜像

v1.0 有**两条产物通道**（与 [RELEASE.md](./RELEASE.md#0-本次发布的产物契约artifact-contract-v10) 一致）。

### 6.1 通道 A —— GitHub Release 文件产物（11 个）

`v*` tag 触发 `.github/workflows/release.yml`，发布 **11 个文件产物**（ARTIFACT CONTRACT v1.0）：

```text
floatctf                            API release binary
floatctf-helper                     host control plane binary
web-dist.tar.gz                     bootstrap 引导页 + 版本化 Default Frontend 制品
merged.sql                          fresh PostgreSQL bootstrap
frontend.sh                         前端管理器（安装到 $FLOATCTF_HOME/frontend.sh）
install.sh                          安装器（releases/download/v<V>/install.sh）
ops-tools.tar.gz                    backup.sh / restore.sh / db/migrate.sh / db/migrations/*.sql
floatctf-sdk-<V>.tgz                @floatctf/sdk
floatctf-react-<V>.tgz              @floatctf/react
floatctf-frontend-runtime-<V>.tgz   @floatctf/frontend-runtime
SHA256SUMS                          上面 1-10 的 sha256（不含自身）
```

`<V>` = tag 去掉前导 `v`（`v1.0.0` → `1.0.0`），即**平台版本**；`SHA256SUMS` 覆盖除自身外的
全部文件产物。其中安装器实际下载的是 6 个部署产物：`floatctf` / `floatctf-helper` /
`web-dist.tar.gz` / `merged.sql` / `frontend.sh` / `ops-tools.tar.gz`。
人工发布清单见 [RELEASE.md](./RELEASE.md)。

### 6.2 通道 B —— GHCR 运行时镜像（3 个，AWD/AWDP 必需）

AWD / AWDP 需要三个运行时服务镜像。**GHCR 是规范在线分发通道**，由 tag 触发的 release
workflow 以 `packages: write` 推送（**仅** tag 发布路径，PR / 分支 dispatch / RC 永不推送）：

```text
ghcr.io/floatctf/awd-flagserver:<V>
ghcr.io/floatctf/awd-judgeserver:<V>
ghcr.io/floatctf/awdp-judgeserver:<V>      # 已扁平化
```

> 旧名 `floatctf/infra/awdp-judgeserver` 只是**历史引用**，规范 GHCR ref 已扁平化为
> `ghcr.io/floatctf/awdp-judgeserver:<V>`。

安装器的行为：

- **硬失败**：部署前用 `docker image inspect` 检查上述**精确 `<V>` ref**，缺失则 `docker pull`；
  仍无法获取时安装器**直接 `die`**（不是警告后就继续）。旧版"只告警"的行为已删除。
- `--skip-runtime-images`（或环境变量 `FLOATCTF_SKIP_RUNTIME_IMAGES=1`）把硬失败降级为醒目告警，
  供 **Jeopardy-only** 宿主使用。此时 AWD/AWDP 赛事在补齐镜像前无法部署。
- registry 前缀可覆盖：`FLOATCTF_RUNTIME_IMAGE_REGISTRY`（默认 `ghcr.io/floatctf`）。它是
  **单一事实来源**，同时用于渲染 `config/floatctf.toml` 与安装器的镜像检查，避免"模板写 A、安装器查 B"。

#### 离线 / 内网宿主的本地逃生通道

不接 registry 时仍可自备镜像：

```bash
# ① 在**有仓库源码**的机器上构建（需要 Rust/Docker 构建环境）
#    不带 --registry = 沿用本地历史命名（awdp 仍带 infra/ 段，向后兼容）
sudo bash scripts/build-runtime-images.sh --tag 1.0.0
#    或直接产出规范名（awdp 扁平化）：
sudo bash scripts/build-runtime-images.sh --registry ghcr.io/floatctf --tag 1.0.0

# ② 导出并搬到目标机（下例对应"不带 --registry"的本地历史名；
#    若用 --registry 构建，请把 awdp 的 ref 换成 <prefix>/awdp-judgeserver:1.0.0）
docker save floatctf/awd-flagserver:1.0.0 floatctf/awd-judgeserver:1.0.0 \
            floatctf/infra/awdp-judgeserver:1.0.0 | gzip > runtime-images-1.0.0.tar.gz
# 目标机：
docker load < runtime-images-1.0.0.tar.gz
# 然后重新运行安装器（已 load 的镜像会被 docker image inspect 命中，不再尝试拉取）
```

命名规则（已核对 `bash scripts/build-runtime-images.sh --help`）：

| 调用方式 | 产出的镜像名 |
|---|---|
| `--tag <V>`（无 `--registry`） | `floatctf/awd-flagserver:<V>`、`floatctf/awd-judgeserver:<V>`、**`floatctf/infra/awdp-judgeserver:<V>`**（历史名，向后兼容） |
| `--registry <prefix>` | `<prefix>/awd-flagserver:<V>`、`<prefix>/awd-judgeserver:<V>`、`<prefix>/awdp-judgeserver:<V>`（**不再有 `infra/` 段**） |
| `--registry ghcr.io/floatctf` | 即规范 GHCR ref（= release 推送到 GHCR 的同一组） |

`--push` 会推送**版本 tag**（绝不推 `:latest`），推送前需自行 `docker login <registry>`；
release 的 GHCR 推送由 `release.yml` 的 `runtime-images` job 完成。
`--extra-tag <tag>`（可重复）与 `--label k=v`（可重复）同样可用。

不带 `--registry` 时产出的是**本地历史名**。安装器默认检查的是规范 GHCR ref，因此离线宿主需要
把 `config/floatctf.toml` 的 `[awd].flagserver_image` / `[awd].judgeserver_image` /
`[awdp].practice_judgeserver_image` 指向本地名，或用 `docker tag` 把它们打成 GHCR ref
（把 `FLOATCTF_RUNTIME_IMAGE_REGISTRY` 指到本地前缀也可以）。升级重跑安装器时，
**管理员自定义过的这些值会被保留**（见 §7.1）。参考取值见
[infra/config/floatctf.prod.toml](infra/config/floatctf.prod.toml)。

### 6.3 API runtime image 仍在目标机构建

仍然发布原始 API binary，是为了让安装器用同一份 Dockerfile 在目标机本地构建 API image
（`floatctf/api:<V>`，默认用户非 root），从而**不依赖外部容器 registry 也能部署应用面**；
运行时服务镜像（§6.2）才走 GHCR：

```text
infra/docker/api/Dockerfile
        +
release floatctf binary
        ↓
floatctf/api:<V>
```

Release CI 固定 Ubuntu 24.04 runner，并实际构建一次 production runtime image，校验 image 默认用户为非 root。

`merged.sql` 由 migrations 确定性生成，只用于 fresh production database。

`web-dist.tar.gz` 的布局是固定契约（由 `scripts/package-web-dist.sh` 组装、
`scripts/verify-release-frontend.sh` 在发布前断言）：

```text
bootstrap/                          # 引导页（无 React / 无 UI）
  index.html
  assets/…
frontends/
  default/
    <version>/                      # 版本化不可变制品
      frontend.json
      assets/{frontend.js,frontend.css,<chunks>.js}
```

安装器把 `bootstrap/` 铺到 `$FLOATCTF_HOME/web`，把 `frontends/` 交给前端管理器安装
（`$FLOATCTF_HOME/frontend.sh install … --platform --make-current`）：
**升级只更新 release 里的前端，第三方已安装的前端、其版本与注册表指针一律保留**，
`FRONTEND_ACTIVE` 设置也不会被改动。详见
[docs/frontend/ARCHITECTURE.md](docs/frontend/ARCHITECTURE.md)。

---

## 7. Fresh install

生产机无需 clone 仓库：

```bash
curl -fsSL \
  https://github.com/FloatCTF/floatctf/releases/download/<tag>/install.sh \
  -o install.sh

sudo env SITE_ADDRESS=ctf.example.com bash install.sh
```

显式指定 release 产物：

```bash
sudo env SITE_ADDRESS=ctf.example.com bash install.sh \
  --version <版本> \
  --api-url <floatctf-url> \
  --helper-url <floatctf-helper-url> \
  --web-url <web-dist.tar.gz-url> \
  --migrate-url <merged.sql-url> \
  --frontend-manager-url <frontend.sh-url> \
  --ops-url <ops-tools.tar.gz-url> \
  --skip-migrations
```

等价环境变量：

```text
FLOATCTF_API_URL
FLOATCTF_HELPER_URL
FLOATCTF_WEB_URL
FLOATCTF_MIGRATE_URL
FLOATCTF_FRONTEND_MANAGER_URL
FLOATCTF_OPS_URL
FLOATCTF_VERSION
FLOATCTF_SKIP_MIGRATIONS=1
```

运行时镜像相关（AWD/AWDP，见 §6.2）：

```text
--skip-runtime-images                    # 或 FLOATCTF_SKIP_RUNTIME_IMAGES=1
--reset-runtime-images                   # 升级时强制回规范 GHCR ref
--keep-runtime-images                    # 升级时强制保留现有镜像设置
FLOATCTF_RUNTIME_IMAGE_REGISTRY          # 覆盖 registry 前缀（默认 ghcr.io/floatctf）
```

已核对的开关全集（从 `scripts/install.sh` 的参数解析逐个核对；`--help` 文本覆盖其中大部分）：

```text
--api-url <url>         --helper-url <url>        --helper-bin <path>
--web-url <url>         --migrate-url <url>       --frontend-manager-url <url>
--ops-url <url>         --version <V>             --skip-migrations
--skip-runtime-images   --reset-runtime-images    --keep-runtime-images
--develop               -h | --help
```

> `--version <V>` 是真实解析的开关（自定义产物 URL 时必须显式给，否则无法推导版本 tag），
> 但当前 `--help` 文本里没有单列它；`--skip-migrations` / `--skip-runtime-images` /
> `--reset-runtime-images` / `--keep-runtime-images` / `--develop` / `--helper-bin` 都在
> `--help` 文本里有说明。

环境变量：`FLOATCTF_API_URL` / `FLOATCTF_HELPER_URL` / `FLOATCTF_WEB_URL` /
`FLOATCTF_MIGRATE_URL` / `FLOATCTF_FRONTEND_MANAGER_URL` / `FLOATCTF_OPS_URL` /
`FLOATCTF_VERSION` / `FLOATCTF_SKIP_MIGRATIONS` / `FLOATCTF_SKIP_RUNTIME_IMAGES` /
`FLOATCTF_RUNTIME_IMAGE_REGISTRY`。

默认安装根：

```text
/var/lib/floatctf
```

覆盖：

```bash
sudo env \
  FLOATCTF_HOME=/opt/floatctf \
  SITE_ADDRESS=ctf.example.com \
  bash install.sh
```

安装流程：

```text
host precheck / initialization
        ↓
python3 + tomllib 能力检查（R3；失败即中止，不做任何变更）
        ↓
create floatctf group + floatctf-helper user
（API 容器用数值 uid，不再创建 floatctf 用户）
        ↓
floatctf-helper joins docker group
        ↓
download 6 release artifacts
        ↓
render TOML / Caddyfile / compose
        ↓
install root-owned helper
        ↓
build floatctf/api:<version> locally
        ↓
ensure runtime images（docker image inspect → docker pull 规范 GHCR ref；
        缺失且无法拉取 → 硬失败，除非 --skip-runtime-images）
        ↓
create/validate fctf-platform-control
        ↓
validate production Compose
        ↓
write helper + infra + target systemd units
        ↓
enable units（不启动整个平台）
```

安装器会移除旧版 `/etc/systemd/system/floatctf-api.service`，避免 native API 与新 API container 冲突。

安装完成后启动与验收见 §11 / §12；备份见 §19。**真实域名 TLS 由运维方在安装后自行验证**（§14）。

### 7.1 升级既有安装

升级就是**用同一入口重跑安装器**（幂等）：

```bash
sudo bash install.sh --version <新版本> --ops-url <ops-tools.tar.gz-url> ...
```

- 既有 PostgreSQL 集群上，安装器会先启动 postgres，再用 `$FLOATCTF_HOME/db/migrate.sh apply`
  （带 `FLOATCTF_CONFIG`）应用 **forward-only** 迁移：已应用版本由 `schema_migrations` 跳过，
  不做任何破坏性回滚。
- `--skip-migrations`（或 `FLOATCTF_SKIP_MIGRATIONS=1`）跳过迁移步骤；跳过时平台可能因缺少
  新表/新列而启动失败。
- **fresh** 数据库仍由 `merged.sql`（48 个迁移）一次性 bootstrap，不属于升级路径。
- 升级前先做可恢复备份：`sudo $FLOATCTF_HOME/backup.sh --out <file>`。
- 升级同样会做 **`python3` + `tomllib`** 能力检查（R3）与运行时镜像获取检查（§6.2）。

运行时的三个镜像设置（`config/floatctf.toml` 的 `[awd].flagserver_image` /
`[awd].judgeserver_image` / `[awdp].practice_judgeserver_image`）在升级时：

- **管理员自定义过的值被保留**（例如离线宿主的本地镜像名；安装器会告警）；
- 只有命中**已知 stock 形态**的值才会被迁移到新的规范 GHCR 默认值；
- `--reset-runtime-images` 强制写回规范 GHCR ref；
- `--keep-runtime-images` 强制保留现值。

（目标行为与 `bash scripts/install.sh --help` 中"升级既有安装"一节的描述一致。）

---

## 8. 安装位置

默认布局：

```text
/var/lib/floatctf/
├── image/
│   └── api/
│       ├── Dockerfile
│       └── floatctf          # image build input
├── web/
├── config/
│   ├── floatctf.toml
│   └── caddy/Caddyfile
├── data/
│   ├── postgres/
│   ├── rustfs/
│   ├── caddy/
│   └── caddy-config/
├── logs/
│   └── rustfs/
├── runtime/                  # API work_dir；容器内同名 /var/lib/floatctf/runtime
│   ├── challenges/           # CHALLENGES_DIR = {{WORK_DIR}}/challenges（Caddy 附件根挂到 /srv）
│   ├── gameboxes/            # GAMEBOXES_DIR
│   └── logs/api/             # API 日志（bootstrap 固定 WORK_DIR/logs/api）
├── compose.prod.yml
├── merged.sql
├── backup.sh                 # 运维备份（来自 ops-tools，root:root 0755）
├── restore.sh                # 运维恢复（来自 ops-tools，root:root 0755）
├── db/
│   ├── migrate.sh            # forward-only 迁移器（升级用）
│   ├── migrations/           # release 全部 .sql 迁移（48 个）
│   └── merged.sql            # merged.sql 的副本（fresh bootstrap 语义）
├── .env
├── web/                      # bootstrap 引导页（Caddy root /srv/web）
├── frontends/                # 已安装前端（Caddy 只读挂载到 /srv/frontends）
│   ├── registry.json         # 本地注册表（no-store）
│   └── default/<version>/    # 平台内置前端（受保护、版本化）
├── frontend.sh               # 前端管理器（root:root 0755；无需源码签出）
└── uninstall.sh
```

前端生命周期：安装/升级/回滚用 `sudo $FLOATCTF_HOME/frontend.sh install|set-current|remove`；
**激活**在管理端 → 设置 → 前端（写动态设置 `FRONTEND_ACTIVE`）；破窗恢复在任意页面加
`?frontend=default`。安全卸载（不带 `--purge`）**保留** `frontends/` 与 `frontend.sh`，
因此重新部署能恢复同一套前端；`--purge` 才会一并删除。

### 8.1 前端管理器命令（`frontend.sh`，已核对 `--help`）

```bash
frontend.sh list                                # 列出已安装前端与版本（* 标记当前版本）
frontend.sh info <id> [version]                 # 某前端/某版本的详细信息
frontend.sh verify <artifact.tar.gz>            # 只校验预构建制品，不安装
frontend.sh install <目录|Git URL|制品.tar.gz>   # 安装（可选 --ref/--node-image/--no-build/
                                                #   --make-current/--platform/--dry-run）
frontend.sh remove <id> [version]               # 移除（不给 version 则移除该 ID 全部版本）
frontend.sh set-current <id> <version>          # 回滚/切换该 ID 的当前版本指针
frontend.sh help
```

关键语义（详见 [docs/frontend/ARCHITECTURE.md](docs/frontend/ARCHITECTURE.md)）：

- **前端版本不可变**：同 ID + 同版本 + 不同内容 = 硬失败（包括 `default`）；要改就升前端版本号。
- `frontend.sh` **不修改** `FRONTEND_ACTIVE`；激活只在管理端设置页。
- 源码安装一律在隔离 Docker 构建容器内进行，不会在宿主直接跑 pnpm/npm/yarn。
- 回滚 = `set-current` 指回旧版本（旧版本目录安装时保留）；破窗 = URL 加 `?frontend=default`。

说明：`data/`、`logs/`、`runtime/` 属主是 API 容器的数值身份 `65532:floatctf`；
根目录本身是 `root:floatctf 0750`。Redis 的数据落在 Compose named volume
`floatctf-redis-data`（不是 `data/redis/`）。

helper 单独安装：

```text
/usr/local/libexec/floatctf-helper
owner: root:root
mode : 0755
```

生产 API binary 只作为 Docker build context 输入使用，不直接作为宿主 service 执行。

---

## 9. `.env` 与 TOML

应用配置仍然以 TOML 为唯一业务配置入口：

```text
$FLOATCTF_HOME/config/floatctf.toml
```

`.env` 由 installer 维护两类部署数据：

1. 用来渲染 TOML/Caddy 的 secrets/部署参数；
2. 仅供 Compose 使用的 `VERSION`、`FLOATCTF_HOME`、`FLOATCTF_UID`、`FLOATCTF_GID`。

API 容器不会把整份 `.env` 注入进进程；API 进程只收到 `FLOATCTF_CONFIG=/etc/floatctf/floatctf.toml`。

生产 TOML 关键值：

```toml
[server]
work_dir = "/var/lib/floatctf/runtime"
listen_ip = "0.0.0.0"
listen_port = 9090

[docker]
socket_path = "/run/floatctf/helper-docker.sock"

[database]
url = "postgres://...@postgres:5432/..."

[rustfs]
endpoint_url = "http://rustfs:9000"

[auth]
jwt_secret = "..."            # installer 生成
awd_root_key = "..."          # 可选；空 = 回落 jwt_secret（启动时告警）
internal_token_key = "..."    # 可选；空 = 回落 jwt_secret（启动时告警）

[redis]
url = "redis://redis:6379/"

[realtime]
channel = "floatctf:realtime"

[awd]
network_runtime = "helper"
flagserver_image = "ghcr.io/floatctf/awd-flagserver:1.0.0"
judgeserver_image = "ghcr.io/floatctf/awd-judgeserver:1.0.0"
platform_internal_url = "http://10.42.8.2:9090"
platform_internal_network = "fctf-platform-control"

[awdp]
practice_judgeserver_image = "ghcr.io/floatctf/awdp-judgeserver:1.0.0"
platform_internal_url = "http://10.42.8.2:9090"
```

- **三个 auth 密钥互相独立**：`jwt_secret` 必填；另两个可选，未配置时回落 `jwt_secret`
  并打 warn（既有部署升级不会被动轮换 flag）。全新安装由 installer 生成三个不同的随机值。
- 三个运行时镜像键在**升级重跑安装器**时的保留/迁移语义见 §7.1。
- 完整参考副本（含 installer 的全部键与注释）见
  [infra/config/floatctf.prod.toml](infra/config/floatctf.prod.toml)。

`network_runtime = "noop"` 只用于 test/mock。

---

## 10. 生产网络边界

### 10.1 Compose default network

API、Caddy、PostgreSQL、Redis、RustFS 使用 Compose DNS：

```text
api:9090
postgres:5432
redis:6379
rustfs:9000
```

Caddy 的 `/api/*` 直接代理 `api:9090`，不经过宿主 9090。RustFS 使用 path-style S3，并保持 `RUSTFS_SERVER_DOMAINS` 未设置；启用 virtual-host domains 会让 Compose 主机名 `rustfs:9000` 被误判为 bucket 名，导致 API 初始化 bucket 失败。

Redis 是 API **必需基础设施**：生产 Compose 会等待 `redis` healthcheck 通过后再启动 API；API bootstrap 随后主动 `PING` Redis，连接失败时 fail-fast，不进入 HTTP serving。它承担 realtime 跨节点扇出/全局 sequence、AWD 分布式限流、Web Terminal 一次性 ticket、scheduler 即时唤醒与 settings 热点缓存。

### 10.2 运维端口

默认仍将以下服务只发布到 loopback，便于宿主备份/诊断：

```text
PostgreSQL 127.0.0.1:5433
Redis      127.0.0.1:6380
RustFS     127.0.0.1:9000/9001
```

### 10.3 `fctf-platform-control`

installer 创建：

```text
name      fctf-platform-control
driver    bridge
internal  true
subnet    10.42.8.0/24
ip-range  10.42.8.128/25
label     io.floatctf.managed=true
```

`.2` 保留给 API container。自动地址分配从 `.128/25` 进行，避免 JudgeServer 动态 IP 与 API 固定地址竞争。

该网络声明为 Compose `external`。这样赛事 FlagServer/JudgeServer 可以在 Compose 生命周期之外加入它，`docker compose down` 也不会误删仍被动态业务容器使用的 control network。

旧版使用 `fctf-awdp-control` 占用同一 `10.42.8.0/24`。installer 在新网络不存在时会识别旧网络；仅当它是 internal、subnet/managed label 匹配且已经没有连接容器时自动删除旧网络并创建 `fctf-platform-control`。如果旧 Judge 仍连接其中，部署会 fail-fast，要求先停止旧 Judge，避免在线迁移时强行断网。

---

## 11. 启动与运维

安装完成后：

```bash
sudo systemctl start floatctf.target
```

查看 systemd：

```bash
systemctl status floatctf.target
systemctl status floatctf-helper
systemctl status floatctf-infra
```

查看 Compose：

```bash
cd /var/lib/floatctf
docker compose -f compose.prod.yml ps
```

日志：

```bash
journalctl -fu floatctf-helper floatctf-infra

docker compose -f /var/lib/floatctf/compose.prod.yml logs -f api
docker compose -f /var/lib/floatctf/compose.prod.yml logs -f caddy
docker compose -f /var/lib/floatctf/compose.prod.yml logs -f postgres redis rustfs
```

重启 API：

```bash
docker compose -f /var/lib/floatctf/compose.prod.yml restart api
```

重启整个 Compose 应用面：

```bash
sudo systemctl restart floatctf-infra
```

重启 helper：

```bash
sudo systemctl restart floatctf-helper
```

整个平台：

```bash
sudo systemctl restart floatctf.target
```

### 11.1 健康与状态速查

```bash
# systemd 层
systemctl status floatctf.target floatctf-helper floatctf-infra --no-pager

# 容器层（五项都应 Up / healthy；postgres/redis/rustfs/api 有 healthcheck）
docker compose -f /var/lib/floatctf/compose.prod.yml ps

# API 存活（容器内 healthcheck 用的就是这个端点；预期 401 = 服务在跑且鉴权生效）
docker compose -f /var/lib/floatctf/compose.prod.yml exec api \
  curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9090/api/users/me

# 对外入口（真实域名，见 §14.1）
curl -fsS -o /dev/null -w '%{http_code}\n' "https://$SITE_ADDRESS/"

# 运行时镜像是否齐备（AWD/AWDP 必需）
docker image ls --format '{{.Repository}}:{{.Tag}}' | grep -E '^(ghcr\.io/floatctf|floatctf)/'
```

> 说明：R2 的目标行为是 RustFS 的 compose healthcheck 用**真实 HTTP 探针**（探测 RustFS
> `/health` 返回 HTTP 200，`timeout: 10s` / `interval: 10s` / `retries: 12` /
> `start_period: 30s`），不再是"端口能连就算健康"。**API 侧已经落地**：
> `apps/api/src/infrastructure/storage.rs` 的 `ensure_buckets_with_retry` 对对象存储 bucket
> 初始化做**有界次数 + 有界总时长 + 指数退避**的重试，并把永久性凭据/配置错误与暂时不可用
> 分类处理（永久错误立即失败，暂时错误耗尽窗口后才让启动失败）。因此**对象存储暂未就绪不再
> 导致崩溃重启循环**。
>
> ⚠️ **compose 侧的 rustfs healthcheck 尚未落地**：本工作树里仍是旧的 TCP 探针
> （`nc -z 127.0.0.1 9000`，`interval: 5s` / `retries: 10` / `start_period: 10s`）。
> 落地后以 `docker inspect floatctf-rustfs --format '{{json .Config.Healthcheck}}'` 为准。

---

## 12. 启动验收

helper：

```bash
id floatctf
id floatctf-helper

systemctl show floatctf-helper \
  -p User \
  -p Group \
  -p SupplementaryGroups \
  -p AmbientCapabilities \
  -p CapabilityBoundingSet

ls -l /run/floatctf/helper-control.sock /run/floatctf/helper-docker.sock
```

目标：

```text
floatctf:
  docker group absent

floatctf-helper:
  primary/shared group = floatctf
  supplementary docker group present
  CAP_NET_ADMIN present

helper sockets:
  group = floatctf
  mode = 0660
```

API container：

```bash
docker inspect floatctf-api \
  --format 'User={{.Config.User}} CapAdd={{json .HostConfig.CapAdd}} CapDrop={{json .HostConfig.CapDrop}} Readonly={{.HostConfig.ReadonlyRootfs}} SecurityOpt={{json .HostConfig.SecurityOpt}}'

docker inspect floatctf-api \
  --format '{{range $k,$v := .NetworkSettings.Networks}}{{$k}}={{$v.IPAddress}} {{end}}'
```

期望看到：

```text
CapDrop=[ALL]
Readonly=true
SecurityOpt 包含 no-new-privileges:true
fctf-platform-control = 10.42.8.2
```

确认 API 没有宿主 Docker socket mount：

```bash
docker inspect floatctf-api --format '{{json .Mounts}}' | grep '/var/run/docker.sock' && echo ERROR || echo OK
```

---

## 13. Fresh database

空 PostgreSQL data dir 第一次启动时：

```text
$FLOATCTF_HOME/merged.sql
```

挂载到：

```text
/docker-entrypoint-initdb.d/00-init.sql
```

fresh install 一次性建立 schema、seed 和 migration history。

已有生产数据库只能通过 forward-only migration 升级。禁止修改历史 migration、手写 `schema_migrations` 或用新的 `merged.sql` 覆盖已有数据库。

数据库规则见 [docs/agents/DATABASE.md](./docs/agents/DATABASE.md)。

---

## 14. Caddy / 域名

安装必须提供：

```bash
SITE_ADDRESS=ctf.example.com
```

Caddy 是唯一公网应用入口：

```text
HTTP/HTTPS
Web static
/api reverse proxy → api:9090
RustFS object routes → rustfs:9000
challenge attachments
```

更换域名后重跑安装器重新渲染 Caddy/TOML，并检查动态 setting `MAIN_URL`。

### 14.1 真实域名 TLS 是运维方的责任（B4，已接受限制）

> **B4（已接受限制，不是 GA 阻塞）**：真实公网域名 / 公共 DNS / Let's Encrypt 签发**未被验证**。
> 已（本地）验证的是**生产配置下的 Caddy + 其受信任本地 CA**：证书链校验通过
> （**无需 `curl -k`**）、HTTP→HTTPS 跳转正确、站点 / API / registry / immutable-asset /
> deep-route 行为正确。产品负责人已接受该限制。
> **不要把它表述为"真实 HTTPS 已完整验证"。**

因此安装后**由运维方自行验证真实域名**：

```bash
# 1) DNS 已把 SITE_ADDRESS 指向本机公网地址
dig +short "$SITE_ADDRESS"

# 2) 80/443 可从公网到达（安全组/防火墙/云厂商放行）
# 3) 证书签发成功且链可校验（故意不用 -k）
curl -fsS -o /dev/null -w '%{http_code}\n' "https://$SITE_ADDRESS/"

# 4) HTTP 跳转到 HTTPS
curl -fsS -o /dev/null -w '%{http_code} -> %{redirect_url}\n' "http://$SITE_ADDRESS/"

# 5) 证书签发失败的根因看 Caddy 日志
docker compose -f /var/lib/floatctf/compose.prod.yml logs -f caddy
```

常见签发失败原因：DNS 未生效、80/443 被占用或未放行、`SITE_ADDRESS` 写成 IP/`localhost`
（此时 Caddy 只能用本地 CA，浏览器会不信任）、同一域名被另一台机器占用。

---

## 15. helper Docker ownership

helper 对新创建的 Docker 资源写入 ownership label：

```text
floatctf.managed=true
```

生产 API runtime image 还带：

```text
io.floatctf.managed=true
```

FloatCTF 业务网络名称保留：

```text
fctf-*
floatctf-*
```

helper 会拒绝通过应用 API 修改普通宿主容器和普通 Docker 网络。这项约束同样适用于管理员 Docker 页面。

---

## 16. 开发与生产对照

| 项目 | 开发 | 生产 |
|---|---|---|
| 主入口 | `mise run dev` | `systemctl start floatctf.target` |
| API 载体 | native `watchexec + setpriv` | Docker Compose container |
| API UID | 当前开发者 | `65532`（`$FLOATCTF_UID`，容器内数值身份） |
| API docker group | 启动时显式丢弃 | 无 |
| API capabilities | 全部丢弃 | `cap_drop=ALL` |
| API NoNewPrivileges | yes | yes |
| API rootfs | host process | read-only container rootfs |
| 宿主控制面 | helper systemd | helper systemd |
| Host RPC | `/run/floatctf/helper-control.sock` | 同左 |
| Docker proxy | `/run/floatctf/helper-docker.sock` | 同左 |
| PostgreSQL/Redis/RustFS/Caddy | Compose | Compose |
| API build | debug + watch | release binary → local runtime image |
| Web | Vite HMR | Caddy static dist |
| fresh DB | migrations | `merged.sql` |
| API host port | `127.0.0.1:9090` | 不发布 |

两种环境保持同一**权限边界**，只选择不同的进程载体来优化各自目标：开发追求热重载，生产追求隔离和可复制部署。

---

## 17. 安全卸载

安装器生成：

```text
$FLOATCTF_HOME/uninstall.sh
```

安全卸载：

```bash
sudo /var/lib/floatctf/uninstall.sh
```

**活跃运行时守卫**：若数据库仍可查询且存在进行中的 AWD 赛事或未结束的 AWDP run，
安全卸载会**拒绝执行并以非零退出**（列出受影响的 id），因为运行时会被销毁而数据库被保留，
会留下「赛事仍 running / run 未结束，但 runtime 已消失」的不一致状态。处理方式：先在管理端
结束/归档这些赛事或 run，再重新运行；确认放弃该场次时用 `--force` 跳过守卫：

```bash
sudo /var/lib/floatctf/uninstall.sh --force   # 数据库仍保留；进行中的赛事 runtime 会被销毁
```

> ⚠️ **`--force` 会跳过守卫**：正在进行中的 AWD 赛事 / AWDP run 的 FlagServer、JudgeServer、
> GameBox、赛事网络、`fawg_*` WireGuard 接口与 `floatctf_awd*` nft 表都会被销毁，而数据库
> （含成绩与赛事状态）仍保留 → 留下「赛事仍 running，但 runtime 已消失」的不一致状态。
> 只有在确认放弃该场次时才用。

`--purge` 会连数据库一起删除，因此不做该守卫（但会打印销毁内容警告）。

它会停止并清理：

```text
production Compose containers
FloatCTF managed API runtime images
helper service + helper binary
GameBox / FlagServer / JudgeServer
fctf-platform-control 与赛事网络
WireGuard interfaces
nftables tables
Docker FORWARD compatibility rules
可再生 web/image build context/compose/merged.sql
```

同时保留：

```text
PostgreSQL data
RustFS data
config
.env
runtime
logs
已安装前端 frontends/ 与 registry.json
frontend.sh
uninstall.sh
```

卸载器保留旧 `floatctf-api.service` 和 `fctf-awdp-control` 的清理分支，仅用于迁移旧安装。

### 17.1 卸载后重新安装（fresh reinstall）

安全卸载是**刻意可恢复**的：数据库、`.env`、`config/`、`runtime/`、`frontends/` 与
`frontend.sh` 都保留。因此"重装"和"全新安装"走**同一个入口**：

```bash
sudo /var/lib/floatctf/uninstall.sh                 # 停平台 + 清运行时/动态资源（保数据）
sudo bash install.sh --version 1.0.0                # 重新装配（同版本 = 幂等恢复）
sudo systemctl start floatctf.target
```

- 想真正从零开始（丢弃全部数据）→ 先 `--purge`，再按 §7 全新安装。
- 重装**不会**重置 `FRONTEND_ACTIVE`，也**不会**动第三方前端（见 §8）。
- ⚠️ 重装前确认这台宿主上**没有**第二套 API / dev stack 在跑（R1 不变量，见 §0）：
  卸载器会删除全局 nft 表 `floatctf_awd`、`fawg_*` 接口与固定命名的 AWDP 练习网络/容器，
  同一时刻只应有一个控制面在管理它们。

---

## 18. Purge

永久删除：

```bash
sudo /var/lib/floatctf/uninstall.sh --purge
```

非交互：

```bash
sudo /var/lib/floatctf/uninstall.sh --purge --yes
```

Purge 还删除：

```text
floatctf user/group
floatctf-helper user
FloatCTF systemd units
FloatCTF sysctl/modules-load files
$FLOATCTF_HOME
```

清理逻辑只匹配 FloatCTF naming/label contract，不执行全局 `nft flush ruleset`，也不会删除无关 Docker/WireGuard/nftables 资源。

---

## 19. 备份与恢复

首选随 release 安装的运维工具（root 身份运行）：

```bash
sudo $FLOATCTF_HOME/backup.sh --out <file>          # 默认 FLOATCTF_HOME=/var/lib/floatctf
sudo $FLOATCTF_HOME/restore.sh <archive> --yes      # 覆盖既有安装需额外 --force
```

`backup.sh` 产出确定性归档（除 `pg_dump` 成员的 custom 格式头带时间戳外），包含 `.env` secrets、
`config/`、`frontends/`、`web/`、`runtime/`（challenges + gameboxes）、Caddy 证书/ACME 状态、Redis
持久化、`pg_dump --format=custom` 数据库转储与静默后的 RustFS 数据 tarball，以及 `META`/`MANIFEST`；
输出权限 `0600`。**归档是明文（未加密）的，务必保护介质。** 备份期间 RustFS 会短暂停止。

`restore.sh` 会校验归档 sha256 与逐成员 MANIFEST，拒绝路径穿越/符号链接/设备/setuid，未加
`--force` 时拒绝覆盖既有安装，PostgreSQL 主版本不一致时拒绝恢复（`--allow-version-mismatch` 放行），
只停本安装的 Compose project（绝不 `-v`），用 `pg_restore` 恢复后对五个服务做健康检查。

恢复流程（默认拒绝危险操作，必须显式表达意图）：

```bash
sudo $FLOATCTF_HOME/restore.sh floatctf-backup-<UTC>.tar.gz --dry-run   # 先看计划，不改状态
sudo $FLOATCTF_HOME/restore.sh floatctf-backup-<UTC>.tar.gz --yes       # 目标为空安装
sudo $FLOATCTF_HOME/restore.sh floatctf-backup-<UTC>.tar.gz --yes --force   # 覆盖既有安装
```

已核对的开关（`--help`）：

| 工具 | 开关 |
|---|---|
| `backup.sh` | `--home DIR` · `--out FILE` · `--compose-file F` · `--no-quiesce` · `--offline` · `--only LIST` · `--force` · `--quiet` · `-h\|--help` |
| `restore.sh` | `--home DIR` · `--compose-file F` · `--yes` · `--force` · `--dry-run` · `--no-start` · `--sha256 HEX` · `--only LIST` · `--allow-version-mismatch` · `-h\|--help` |

`--only` 子集：`env,config,merge,frontends,web,runtime,postgres,rustfs,redis,caddy`。
`backup.sh` 退出码：0 成功 / 1 用法或前置错误 / 2 数据面失败（已清理半成品并恢复 RustFS）。

如不想用工具，至少手动备份：

```text
$FLOATCTF_HOME/data/postgres
$FLOATCTF_HOME/data/rustfs
$FLOATCTF_HOME/config
$FLOATCTF_HOME/.env
```

PostgreSQL 逻辑备份示例：

```bash
docker exec floatctf-postgres \
  pg_dump -U postgres -d floatctf_db \
  > floatctf-db-$(date +%F).sql
```

生产升级前先完成可恢复备份。人工发布清单见 [RELEASE.md](./RELEASE.md)。

---

## 20. 故障排查

| 现象 | 检查 |
|---|---|
| API container 启动时报 helper unavailable | `systemctl status floatctf-helper`、检查 `/run/floatctf` mount 和 socket GID |
| API Docker ping 失败 | helper 日志、`helper-docker.sock`、helper docker group |
| API Docker 返回 403 | Docker 操作超出 helper policy 或对象无 FloatCTF ownership |
| API 容器直接拥有 Docker socket | 属于部署错误；检查 `docker inspect floatctf-api` mounts |
| AWD Flag/Judge 回调 API 失败 | 检查 `fctf-platform-control`、API `.2` 地址、infra 容器是否加入该网络 |
| AWDP Judge 回调 API 失败 | 同上，并检查 `PLATFORM_INTERNAL_URL` |
| AWD WG/nft 失败 | helper 日志、`ip_forward`、`br_netfilter`、`CAP_NET_ADMIN` |
| Caddy 502 | `docker compose ... ps` 与 `docker compose ... logs api caddy` |
| 安装器报缺少 `python3` / `tomllib` | 发行版 Python < 3.11；升级到 3.11+（要求 stdlib 自带 `tomllib`，不要 `pip install`） |
| 安装器报缺少运行时镜像 | 检查 `docker images \| grep ghcr.io/floatctf`；需出网拉取，或按 §6.2 离线构建/`docker load` 后改 TOML ref。Jeopardy-only 宿主可用 `--skip-runtime-images` |
| AWD/AWDP 行为异常、容器/网络被"莫名"重建 | 疑似同宿主存在**第二套 API**（dev/RC/第二份安装）—— 见 §0 的 R1 不变量；同一时刻只允许一个控制面 |
| PostgreSQL 失败 | `docker logs floatctf-postgres` |
| API 因 Redis 启动失败 | `docker logs floatctf-redis`、`docker exec floatctf-redis redis-cli ping`；Redis 必须 healthy 且 API bootstrap PING 成功 |
| Caddy/TLS 失败 | `docker logs floatctf-caddy`，检查 DNS / 80 / 443 |
| fresh DB 初始化失败 | PostgreSQL logs + release `merged.sql` |
| external control network missing | 重新执行 installer 的部署阶段，或检查 `docker network inspect fctf-platform-control` |

生产验收标准：**API container 为非 root、无 capabilities、无真实 Docker socket；`floatctf-helper` 是唯一拥有 Docker group/CAP_NET_ADMIN 的 FloatCTF 进程；PostgreSQL/Redis/RustFS 均为必需基础设施，其中 Redis 还必须通过 API bootstrap PING 门禁。**
