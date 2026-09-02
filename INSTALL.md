# FloatCTF 安装与部署

> 权威的用户安装 / 运维 / 生命周期指南。覆盖：架构概览、环境要求、全新安装、
> 重部署 / 升级、发布构建、systemd 管理、Docker 运维、开发与生产差异、清理、
> 卸载、备份与故障排查。

## 架构概览

FloatCTF 生产部署采用「原生进程 + Docker 容器」的混合架构：

**Host（原生）**
- FloatCTF API（Rust 二进制，systemd 服务）
- Docker（容器运行时）
- nftables（赛事隔离防火墙）
- WireGuard（选手接入隧道）

**Containers（infra，`floatctf-infra`）**
- PostgreSQL（持久化数据）
- RustFS（S3 兼容对象存储）
- nginx（反向代理 + 静态文件，`network_mode: host`）

**Dynamic AWD（赛事运行时）**
- GameBox（选手靶机）
- FlagServer / JudgeServer（赛事基础设施容器）
- 赛事 Docker 网络 + WireGuard 接口 + nftables 表（随赛事动态创建）

**systemd**
- `floatctf-api.service` — 原生 API 进程
- `floatctf-infra.service` — infra 容器（postgres + rustfs + nginx）
- `floatctf.target` — 聚合目标（2 服务 + 1 target，**不是** 3 个独立守护进程）

## 环境要求

| 能力 | 用途 | 缺失时 |
|------|------|--------|
| systemd Linux | 服务托管 | 不支持 |
| Docker | 全部容器 | `init.sh` 报错并退出 |
| Docker Compose（v2 插件） | infra 编排 | `init.sh` 报错并退出 |
| nftables | 赛事隔离防火墙 | `init.sh` 报错并退出 |
| WireGuard / wireguard-tools | 选手接入隧道 | `init.sh` 报错并退出 |
| iproute2 | 接口 / 路由 | `init.sh` 报错并退出 |

Phase 9 真实主机验证要求以下内核设置（`init.sh` 自动检查并持久化）：

- `net.ipv4.ip_forward=1` → `/etc/sysctl.d/99-floatctf.conf`
- `br_netfilter` 模块 → `/etc/modules-load.d/floatctf-br-netfilter.conf`
- `net.bridge.bridge-nf-call-{ip,ip6}tables=1` → `/etc/sysctl.d/99-floatctf.conf`

**已验证平台**：Arch Linux（pacman）真实主机验收全 PASS。Debian/Fedora/RHEL 的
`init.sh` 安装路径未硬编码（避免盲装），需手动安装上述能力后重试 —— 请勿宣称
未经验证的发行版受支持。

## 全新安装（Fresh Host）

```bash
git clone https://github.com/FloatCTF/floatctf.git
cd floatctf

sudo ./scripts/init.sh      # 1. 主机初始化（一次性，root）
./scripts/deploy.sh         # 2. 首次部署（普通用户 + sudo，或直接 root）
```

部署完成后：

```bash
systemctl status floatctf.target
```

> `deploy.sh` 每次成功部署都会把当前源码的 `scripts/uninstall.sh` 安装为
> `/home/floatctf/uninstall.sh`（root:floatctf 0750），供日后卸载/清理使用，
> 无需保留源码签出。

## init.sh —— 主机初始化

**职责（只做这些）**：
- 检查 Linux 环境 / 内核版本（拒绝容器内运行）
- 检查 Docker daemon（含创建/删除临时网络验证）
- 检查 nftables（含创建/删除临时表验证）
- 检查 WireGuard（含创建/删除临时接口验证）
- 检查并启用 IPv4 转发、`br_netfilter`、bridge netfilter sysctls（持久化到 FloatCTF 自有文件）
- 创建 `floatctf` 系统服务用户 + 加入 `docker` 组（`runuser` 实测组生效）
- 创建 `/home/floatctf` 运行布局 + 写 `.initialized` 完成标记

**绝不负责**：
- 不装原生 PostgreSQL / nginx / Rust / Node 运行时
- 不构建 / 部署 FloatCTF
- 不创建数据库、不启动 postgres/nginx 容器
- 不创建 systemd 单元
- 不创建生产赛事网络

幂等：重复运行安全；`.initialized` 已存在时跳过主体写入。

## deploy.sh —— 首次安装与重部署/升级

```
precheck → .env/configs → 装配产物 → infra(--wait) → 迁移(forward-only)
         → systemd → 启动 API → 安装 uninstall.sh
```

**首部署**：生成密钥（DB 密码 / RustFS 密钥 / JWT secret）写入
`/home/floatctf/.env` 与 `config/floatctf.toml`，启动 infra，跑前向迁移，装
systemd 单元，启动 API，并安装 `/home/floatctf/uninstall.sh`。

**重部署 / 升级**：`./scripts/deploy.sh` 保留既有数据与密钥（只更新非敏感值与新
发布产物），跑前向迁移，更新 systemd 单元与卸载脚本。**数据与 secrets 均保留**。

升级流程：

```bash
git pull
./scripts/deploy.sh
```

**不承诺降级支持**（迁移 forward-only）。

## build-release.sh —— 发布构建

```bash
scripts/build-release.sh                # 自动：musl 可用则 musl，否则容器 glibc-2.34 基线
scripts/build-release.sh --container    # 强制容器 glibc 基线
scripts/build-release.sh --musl         # 强制 musl（需 rustup target + musl-gcc）
```

产物：`release/floatctf-<version>/`（`bin/floatctf` + `web/` + AWD 服务镜像 +
`checksums.txt`）。AWD 服务镜像 `floatctf/awd-flagserver` / `awd-judgeserver`
本地构建不推送。可移植性：musl 静态或 bookworm glibc-2.34 基线（Phase 9 实测）。

### crates.io 发布（出题工具 / 平台二进制分发）

`fcmc`（出题 / 容器管理工具）已发布到 crates.io，可直接安装：

```bash
cargo install fcmc
```

平台后端 crate `floatctf` 亦已具备发布元数据。发布顺序须**先 `fcmc` 后 `floatctf`**
（后者依赖前者）。发布流程与命令见 `chore/crates-io-publish-guide.md`。

> crates.io 发布与「`scripts/build-release.sh` 本地发布构建」是两条独立渠道：
> 前者分发 Rust crate（经 `cargo install`），后者产出平台部署用的自包含产物
> （bin + web + AWD 镜像），供 `deploy.sh` 使用。

## systemd 管理

**整平台**：

```bash
sudo systemctl start floatctf.target
sudo systemctl stop floatctf.target
sudo systemctl restart floatctf.target
systemctl status floatctf.target
```

**仅 API**：

```bash
sudo systemctl restart floatctf-api
systemctl status floatctf-api
journalctl -fu floatctf-api
```

**仅基础设施**：

```bash
sudo systemctl restart floatctf-infra
systemctl status floatctf-infra
```

- `floatctf-infra`：postgres + rustfs + nginx 容器（`docker compose up -d --wait`）
- `floatctf-api`：原生 API 进程（`After=floatctf-infra`）
- `floatctf.target`：聚合目标

## Docker 运维（infra 容器）

```bash
docker compose -f /home/floatctf/compose.yml ps
docker compose -f /home/floatctf/compose.yml logs
docker compose -f /home/floatctf/compose.yml logs -f postgres
docker compose -f /home/floatctf/compose.yml logs -f rustfs
docker compose -f /home/floatctf/compose.yml logs -f nginx
```

## 开发 vs 生产

| 场景 | `network_runtime` | 说明 |
|------|-------------------|------|
| 开发（普通 API/UI） | `noop` | 不建 nftables/WireGuard |
| 生产 / 真实 AWD | `host` | HostNetworkRuntime + NftablesFirewallRuntime |

生产流量模型：

```
Player → WireGuard → nftables → Docker 网络 → GameBox
```

## clean.sh —— 清理源码构建产物

```bash
./scripts/clean.sh        # 清理可再生构建产物
./scripts/clean.sh --all  # 额外清理依赖安装与开发运行时数据
```

**只影响当前源码签出**的再生构建产物：

- 默认：`target/`、`apps/web/dist/`、`release/`、`scripts/__pycache__/`
- `--all` 额外：`node_modules/`、`apps/web/node_modules/`、`app/`（开发 WORK_DIR）

**绝不影响**：运行中的 FloatCTF、数据库、RustFS 数据、配置、systemd、生产容器、
nftables、WireGuard、宿主路由、`/home/floatctf`。幂等，重复运行安全。

## uninstall.sh —— 卸载

```bash
sudo /home/floatctf/uninstall.sh              # SAFE UNINSTALL
sudo /home/floatctf/uninstall.sh --purge      # 永久删除全部 FloatCTF 数据
sudo /home/floatctf/uninstall.sh --purge --yes  # 跳过确认（非交互）
```

### 安全卸载（保留可恢复状态）

移除：systemd 单元、infra 与赛事容器/网络、API 二进制、web 资产、可再生产物。

**保留**：`data/postgres`、`data/rustfs`、`config/`、`.env`（密钥）、`runtime/`、
`logs/`、`.initialized`、`uninstall.sh` 本身（生命周期/恢复工具，保留并文档化）。

语义：`deploy → safe uninstall → deploy` 恢复相同数据与密钥（用户/赛事/数据仍在；
API 启动时 `recover_all` 自动重建 AWD 动态资源）。

### 永久删除（--purge）

**不可撤销**。删除：PostgreSQL/RustFS 数据、config/secrets、runtime、日志、API
二进制、web、compose、systemd 单元、动态赛事资源、sysctl/modules 文件、
`floatctf` 服务用户、`/home/floatctf`（含本脚本）。

默认要求输入 `PURGE FLOATCTF`（不接受简单 y/N）；`--yes` 跳过确认（仅非交互）。

> **强烈警告**：`--purge` 永久销毁 PostgreSQL / RustFS / 配置 / 密钥，不可恢复。

**共享宿主依赖永不卸载**：Docker / docker compose / nftables 包 / wireguard-tools /
iproute2 / systemd。绝不触碰无关 Docker 对象、WG 接口、nftables 状态、路由、
libvirt、Incus、其他应用。

**sysctl/modules 说明**：purge 仅移除 FloatCTF 自有持久化文件
（`/etc/sysctl.d/99-floatctf.conf`、`/etc/modules-load.d/floatctf-br-netfilter.conf`），
**不自动关闭** IPv4 转发 / `br_netfilter`（可能被其他负载依赖）；如需关闭请手动评估。

## 备份

`--purge` 或大版本升级前请先备份。平台**不提供自动备份**（不要假设存在）。

最小备份集：

- **PostgreSQL**：`/home/floatctf/data/postgres`（建议 `pg_dump`，见下）
- **RustFS**：`/home/floatctf/data/rustfs`
- **配置/密钥**：`/home/floatctf/config/` 与 `/home/floatctf/.env`

```bash
# PostgreSQL 逻辑备份示例（容器内）
docker exec floatctf-postgres pg_dump -U postgres -d floatctf_db -Fc \
  > floatctf-$(date +%F).dump
```

## 故障排查

```bash
systemctl status floatctf.target
systemctl status floatctf-api
systemctl status floatctf-infra

journalctl -fu floatctf-api
journalctl -fu floatctf-infra

docker compose -f /home/floatctf/compose.yml ps
docker compose -f /home/floatctf/compose.yml logs

docker info
wg show
nft list table inet floatctf_awd
```

> **安全红线**：切勿运行 `nft flush ruleset`、`docker system prune`、
> `docker network prune` —— 会破坏无关资源。

## 术语速查

| 术语 | 含义 |
|------|------|
| `init.sh` | 主机初始化（一次性准备） |
| `build-release.sh` | 构建可移植发布产物 |
| `deploy.sh` | 首次安装 **和** 后续重部署/升级 |
| `clean.sh` | 清理源码签出里的再生构建产物 |
| `uninstall.sh` | 移除已部署的 FloatCTF（safe / purge） |
