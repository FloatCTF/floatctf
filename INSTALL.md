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
| Docker | 全部容器 | `install.sh` 报错并退出 |
| Docker Compose（v2 插件） | infra 编排 | `install.sh` 报错并退出 |
| nftables | 赛事隔离防火墙 | `install.sh` 报错并退出 |
| WireGuard / wireguard-tools | 选手接入隧道 | `install.sh` 报错并退出 |
| iproute2 | 接口 / 路由 | `install.sh` 报错并退出 |

Phase 9 真实主机验证要求以下内核设置（`install.sh` 自动检查并持久化）：

- `net.ipv4.ip_forward=1` → `/etc/sysctl.d/99-floatctf.conf`
- `br_netfilter` 模块 → `/etc/modules-load.d/floatctf-br-netfilter.conf`
- `net.bridge.bridge-nf-call-{ip,ip6}tables=1` → `/etc/sysctl.d/99-floatctf.conf`

**已验证平台**：Arch Linux（pacman）真实主机验收全 PASS。Debian/Fedora/RHEL 的
`install.sh` 安装路径未硬编码（避免盲装），需手动安装上述能力后重试 —— 请勿宣称
未经验证的发行版受支持。

## 全新安装（Fresh Host）

```bash
git clone https://github.com/FloatCTF/floatctf.git
cd floatctf

sudo ./scripts/install.sh    # 一键：下载 3 产物 + 主机初始化(幂等) + 部署（仅全新安装）
```

部署完成后：

```bash
systemctl status floatctf.target
```

> `install.sh` 是**单文件自包含**安装器：内嵌所有模板，部署时内嵌生成
> `uninstall.sh` 到 `$FLOATCTF_HOME/uninstall.sh`（root:floatctf 0750）。
> 安装根默认 `/home/floatctf`，可用 `FLOATCTF_HOME` 环境变量覆盖。

## install.sh —— 一键安装（单文件自包含）

`install.sh` 合并了主机初始化 + 下载 + 部署，**不依赖仓库其他文件**（模板全部内嵌）。
三阶段：

```
1. 主机初始化（幂等） → 2. 下载 3 产物 → 3. 部署
   （docker/nftables/WG/       （API 二进制 +       （渲染配置 → 装配 →
    转发/br_netfilter/           前端 dist +          infra(--wait) → psql 初始化
    用户/布局，已存在即 skip）    merged.sql）         merged.sql → systemd → API）
```

**主机初始化（幂等，逐项补齐）**：
- 检查 Linux 环境 / 内核版本（拒绝容器内运行）
- 检查 Docker daemon（含创建/删除临时网络验证）
- 检查 nftables（含创建/删除临时表验证）
- 检查 WireGuard（含创建/删除临时接口验证）
- 检查并启用 IPv4 转发、`br_netfilter`、bridge netfilter sysctls（持久化到 FloatCTF 自有文件）
- 创建 `floatctf` 系统服务用户 + 加入 `docker` 组（`runuser` 实测组生效）
- 创建 `$FLOATCTF_HOME` 运行布局 + 写 `.initialized` 完成标记

以上每项都幂等：**已存在/已做过就跳过**（不依赖 `.initialized` 单点标记），
重跑安全。

**安装根**：默认 `/home/floatctf`，可经环境变量覆盖（所有路径相对它）：

```bash
FLOATCTF_HOME=/opt/floatctf sudo ./scripts/install.sh
```

**下载 3 个 release 产物**：默认 fake 占位地址，可经 `--*-url` 或环境变量覆盖：

```bash
sudo ./scripts/install.sh \
  --api-url <bin-url> --web-url <dist-url> --migrate-url <sql-url>
# 或环境变量：FLOATCTF_API_URL / FLOATCTF_WEB_URL / FLOATCTF_MIGRATE_URL
```

**跳过下载（用本地产物）**：跳过下载，改用本地 `release/floatctf-*` 产物目录
（`bin/floatctf` + `web/` + `merged.sql`），init 与部署照走：

```bash
sudo ./scripts/install.sh --skip-download
```

**部署（仅全新安装）**：首部署生成密钥（DB 密码 / RustFS 密钥 / JWT secret）写入
`$FLOATCTF_HOME/.env` 与 `config/floatctf.toml`，装配 bin/web/merged.sql，起 infra，
用 `psql` 应用 `merged.sql` 初始化空库（库已有表则跳过），装 systemd 单元，启动
API，并内嵌生成 `$FLOATCTF_HOME/uninstall.sh`。

> **只做全新安装**：`merged.sql` 是 fresh-DB bootstrap，只适用于空库。
> 已有数据的升级（forward-only 迁移）后续单独实现。
>
> AWD 服务镜像（`floatctf/awd-flagserver` / `awd-judgeserver`）暂不在 install.sh
> 构建，需另行准备（TODO：registry 拉取或本地 docker build）。

## 发布构建与 crates.io

CI 打 `v*` tag 触发 `.github/workflows/release.yml`，产出 3 个产物：
`floatctf`（API 二进制）、`web-dist.tar.gz`（前端）、`merged.sql`（数据库初始化，
由 `mise run db:migration:merge` 生成），供 `install.sh` 下载部署。

### crates.io 发布（出题工具 / 平台二进制分发）

`fcmc`（出题 / 容器管理工具）已发布到 crates.io，可直接安装：

```bash
cargo install fcmc
```

平台后端 crate `floatctf` 亦已具备发布元数据。发布顺序须**先 `fcmc` 后 `floatctf`**
（后者依赖前者）。发布流程与命令见 `chore/crates-io-publish-guide.md`。

> crates.io 发布与「GitHub Release 产物」是两条独立渠道：
> 前者分发 Rust crate（经 `cargo install`），后者产出平台部署用的 3 个产物
> （bin + web + merged.sql），供 `install.sh` 使用。

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
docker compose -f /home/floatctf/compose.prod.yml ps
docker compose -f /home/floatctf/compose.prod.yml logs
docker compose -f /home/floatctf/compose.prod.yml logs -f postgres
docker compose -f /home/floatctf/compose.prod.yml logs -f rustfs
docker compose -f /home/floatctf/compose.prod.yml logs -f nginx
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

docker compose -f /home/floatctf/compose.prod.yml ps
docker compose -f /home/floatctf/compose.prod.yml logs

docker info
wg show
nft list table inet floatctf_awd
```

> **安全红线**：切勿运行 `nft flush ruleset`、`docker system prune`、
> `docker network prune` —— 会破坏无关资源。

## 术语速查

| 术语 | 含义 |
|------|------|
| `install.sh` | 一键安装（下载 release tarball + 主机初始化(幂等) + 部署/升级） |
| `clean.sh` | 清理源码签出里的再生构建产物 |
| `uninstall.sh` | 移除已部署的 FloatCTF（safe / purge） |
