# FloatCTF 安装与部署

本文档是 FloatCTF **生产安装、容器化部署、systemd 运维、权限模型和生命周期管理**的权威说明。本地源码开发见 [DEVELOPMENT.md](./DEVELOPMENT.md)。

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
user: "${FLOATCTF_UID}:${FLOATCTF_GID}"
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

`floatctf` 系统用户仍会在宿主创建。它的 numeric UID/GID 被写进 `.env`，Compose 用这组数字运行 API 容器。这样 `/run/floatctf/helper*.sock` 的 `floatctf` group 权限无需在容器内创建同名组，也能正确生效。

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
- curl / tar / openssl
- systemd

需要的宿主网络参数：

```text
net.ipv4.ip_forward=1
br_netfilter
net.bridge.bridge-nf-call-iptables=1
net.bridge.bridge-nf-call-ip6tables=1
```

安装器会持久化 FloatCTF 需要的 sysctl/modules 配置。

---

## 6. Release 产物与 API image

`v*` tag 触发 `.github/workflows/release.yml`，发布四个部署产物：

```text
floatctf            API release binary
floatctf-helper     host control plane binary
web-dist.tar.gz     Web static dist
merged.sql          fresh PostgreSQL bootstrap
```

仍然发布原始 API binary，是为了让安装器无需依赖外部容器 registry。部署阶段会使用：

```text
infra/docker/api/Dockerfile
        +
release floatctf binary
        ↓
floatctf/api:<version>
```

Release CI 固定 Ubuntu 24.04 runner，并实际构建一次 production runtime image，校验 image 默认用户为非 root。目标机 installer 再以同一 Dockerfile 本地构建对应版本 image。

`merged.sql` 由 migrations 确定性生成，只用于 fresh production database。

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
  --migrate-url <merged.sql-url>
```

等价环境变量：

```text
FLOATCTF_API_URL
FLOATCTF_HELPER_URL
FLOATCTF_WEB_URL
FLOATCTF_MIGRATE_URL
FLOATCTF_VERSION
```

默认安装根：

```text
/home/floatctf
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
create floatctf + floatctf-helper users/groups
        ↓
floatctf-helper joins docker group
        ↓
download 4 release artifacts
        ↓
render TOML / Caddyfile / compose
        ↓
install root-owned helper
        ↓
build floatctf/api:<version> locally
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

---

## 8. 安装位置

默认布局：

```text
/home/floatctf/
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
│   ├── redis/
│   ├── rustfs/
│   ├── caddy/
│   └── caddy-config/
├── logs/
├── runtime/
├── gameboxes/
├── compose.prod.yml
├── merged.sql
├── .env
└── uninstall.sh
```

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

[redis]
url = "redis://redis:6379/"

[realtime]
channel = "floatctf:realtime"

[awd]
network_runtime = "helper"
platform_internal_url = "http://10.42.8.2:9090"
platform_internal_network = "fctf-platform-control"

[awdp]
platform_internal_url = "http://10.42.8.2:9090"
```

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
cd /home/floatctf
docker compose -f compose.prod.yml ps
```

日志：

```bash
journalctl -fu floatctf-helper floatctf-infra

docker compose -f /home/floatctf/compose.prod.yml logs -f api
docker compose -f /home/floatctf/compose.prod.yml logs -f caddy
docker compose -f /home/floatctf/compose.prod.yml logs -f postgres redis rustfs
```

重启 API：

```bash
docker compose -f /home/floatctf/compose.prod.yml restart api
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
| API UID | 当前开发者 | 宿主 `floatctf` numeric UID |
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
sudo /home/floatctf/uninstall.sh
```

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
uninstall.sh
```

卸载器保留旧 `floatctf-api.service` 和 `fctf-awdp-control` 的清理分支，仅用于迁移旧安装。

---

## 18. Purge

永久删除：

```bash
sudo /home/floatctf/uninstall.sh --purge
```

非交互：

```bash
sudo /home/floatctf/uninstall.sh --purge --yes
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

## 19. 备份

至少备份：

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

生产升级前先完成可恢复备份。

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
| PostgreSQL 失败 | `docker logs floatctf-postgres` |
| API 因 Redis 启动失败 | `docker logs floatctf-redis`、`docker exec floatctf-redis redis-cli ping`；Redis 必须 healthy 且 API bootstrap PING 成功 |
| Caddy/TLS 失败 | `docker logs floatctf-caddy`，检查 DNS / 80 / 443 |
| fresh DB 初始化失败 | PostgreSQL logs + release `merged.sql` |
| external control network missing | 重新执行 installer 的部署阶段，或检查 `docker network inspect fctf-platform-control` |

生产验收标准：**API container 为非 root、无 capabilities、无真实 Docker socket；`floatctf-helper` 是唯一拥有 Docker group/CAP_NET_ADMIN 的 FloatCTF 进程；PostgreSQL/Redis/RustFS 均为必需基础设施，其中 Redis 还必须通过 API bootstrap PING 门禁。**
