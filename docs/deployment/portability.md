# FloatCTF — 主机可移植性指南（Portability）

> Phase 10。本文件说明 FloatCTF 生产部署对宿主机的依赖，以及在“标准 Linux 部署主机”
> 上迁移/移植所需的先决条件。目标：**任何基于 systemd 的 Linux（Arch/Debian/Fedora/RHEL）
> + Docker + nftables + WireGuard + iproute2 的主机均可承载本平台**，不绑定单一发行版。

> ⚠️ **单控制面不变量（R1）—— 同一宿主/helper/Docker daemon 同一时刻只允许一个 FloatCTF 控制面（API）。**
> 第二套 API —— 包括开发栈、RC/测试栈或第二份生产安装 —— 会 reconcile 全局/固定命名的宿主资源
> （AWD 防火墙是单一全局 nft 表 `floatctf_awd`；AWDP 练习网络与容器名固定为 `fctf-awdp-practice` /
> `fctf-awdp-practice-judge`），从而干扰正在运行的实例。**这在 Phase 13 实测发生过**（第二个 API
> 重建了线上练习判题容器与网络一次），不是理论风险。
> **绝不要在承载生产实例的宿主上启动开发 / RC / 测试 API。**
> 迁移（§7）时必须**先停旧宿主上的 API**，不要两台并行。

## 1. 模块→宿主能力映射

| FloatCTF 组件 | 运行形态 | 宿主能力需求 |
|---|---|---|
| API（`floatctf` 二进制） | Docker 容器（生产 Compose） | 数值 `65532:<floatctf 组 GID>`、`cap_drop=ALL`、`no-new-privileges`、read-only rootfs、可写 `runtime/`；宿主能力经 helper socket |
| PostgreSQL | Docker 容器 | Docker daemon（任意后端）、`127.0.0.1` 回环端口 |
| RustFS | Docker 容器 | 同上（仅回环 9000/9001）；healthcheck 用镜像内 BusyBox `nc` 做 **HTTP `/health` 探针** |
| Caddy | Docker 容器（Compose default bridge + 端口映射） | 映射宿主 80/443（HTTP/HTTPS）；API/RustFS 端口只在 Compose 网络内 |
| AWD/AWDP 运行时镜像（FlagServer / JudgeServer） | Docker 镜像 + 赛事动态容器 | 从 **GHCR** 拉取 `ghcr.io/floatctf/{awd-flagserver,awd-judgeserver,awdp-judgeserver}:<V>`，或离线本地构建/`docker load` |
| AWD/AWDP 动态资源 | 容器 / 网络 / nft / WireGuard | helper 的 `CAP_NET_ADMIN` + docker 组（经 `helper-control.sock` / `helper-docker.sock` 创建子网/桥/nftables/WireGuard） |

> 注：RustFS 的 HTTP `/health` 探针（`timeout 10s` / `interval 10s` / `retries 12` /
> `start_period 30s`）与 API 的对象存储 bucket 初始化**有界重试**属于 R2 修复；
> 在 R2 落地前的安装器里 healthcheck 仍是 TCP 探针（`nc -z 127.0.0.1 9000`）。
> 以 `docker inspect floatctf-rustfs --format '{{json .Config.Healthcheck}}'` 为准。

## 2. 宿主必需软件（feature-check，非发行版清单）

| 能力 | 命令 | 用途 | 缺失时 |
|---|---|---|---|
| Docker | `docker info` | 全部容器 | `install.sh` 报错并退出 |
| Python（stdlib `tomllib`） | `python3 -c 'import tomllib'`（Python ≥ 3.11） | `migrate.sh` 解析配置；`frontend.sh` 解析 JSON | `install.sh` 在**任何改动前**报错并退出（能力检查，**不** `pip install`） |
| nftables | `nft` | 赛事隔离防火墙 | `install.sh` 报错并退出 |
| WireGuard | `wg` | 选手接入隧道 | `install.sh` 报错并退出 |
| iproute2 | `ip` | 网络接口/路由 | `install.sh` 报错并退出 |
| sysctl | `sysctl` | 内核参数 | `install.sh` 报错并退出 |
| modprobe | `modprobe` | 内核模块（br_netfilter） | `install.sh` 报错并退出 |

`install.sh` **按功能检测**而非按发行版包名：提供这些命令的主机即可初始化，不假设
“Arch 有什么包”。已知发行版安装命令作为提示（Arch `pacman -S`），Debian/Fedora
未硬编码（避免盲装）。

## 3. 必需内核参数与模块（install.sh 自动持久化）

写入 `/etc/sysctl.d/99-floatctf.conf`（重启后生效）：

```
net.ipv4.ip_forward=1
net.bridge.bridge-nf-call-iptables=1
net.bridge.bridge-nf-call-ip6tables=1
```

写入 `/etc/modules-load.d/floatctf-br-netfilter.conf`：

```
br_netfilter
```

`install.sh` 同时做运行时 `sysctl -w` / `modprobe`（幂等），并持久化到
FloatCTF 自有文件（不污染系统默认配置）。

## 4. 运行账号

- 系统组 `floatctf`（`groupadd --system`）：`/run/floatctf` 与两个 helper socket 的访问凭据。
- 系统用户 `floatctf-helper`（`useradd --system --no-create-home`，主组 `floatctf`），加入 `docker` 组，
  由 `floatctf-helper.service` 以 `CAP_NET_ADMIN` 运行；Docker 与宿主网络的权限只授予它。
- **不创建 `floatctf` 用户**：API 在容器内以数值 `65532:<floatctf GID>` 运行，宿主上没有任何进程
  以该身份运行（宿主历史上曾以 `User=floatctf` 跑 native API service，该用途已随容器化取消）。

## 5. 目录布局（`/var/lib/floatctf`）

```
web/  config/{floatctf.toml,caddy/}  data/{postgres,rustfs,caddy,caddy-config}  logs/rustfs/  runtime/{challenges,gameboxes,logs/api}
```

`config/` 属 `root:floatctf`（含密钥）；运行数据（`data/`、`logs/`、`runtime/`）属
API 容器的数值身份 `65532:floatctf`；容器数据目录分别归容器 uid（postgres 999 /
rustfs 10001）。Redis 数据在 Compose named volume `floatctf-redis-data`。

## 6. 端口约定

| 服务 | 默认 | 说明 |
|---|---|---|
| API | 9090 | 容器内监听；**不发布宿主端口**（生产经 Compose DNS `api:9090` 与 internal 网络 `10.42.8.2:9090`；仅开发监听 `0.0.0.0:9090`） |
| Redis | 6380（回环） | compose 映射 `127.0.0.1:6380` |
| PostgreSQL | 5433（回环） | compose 映射 `127.0.0.1:5433` |
| RustFS | 9000/9001（回环） | 仅 `127.0.0.1` |
| Caddy | 80/443 | **Compose default bridge 网络 + 端口映射**（`${HTTP_PORT}:${HTTP_PORT}` / `${HTTPS_PORT}:${HTTPS_PORT}`，含 443/UDP）；不是 host network。`SITE_ADDRESS` 自动 HTTPS |

所有端口可经 `/var/lib/floatctf/.env`（或环境变量）调整，`install.sh` 部署前检测冲突。

## 7. 迁移（porting）到另一台主机

> ⚠️ 先确认**旧宿主上的 API / helper 已经停机**（R1 不变量，见文首）：AWD 防火墙表、
> `fawg_*` 接口与 AWDP 练习网络/容器都是全局/固定命名资源，两台宿主不能同时管理同一批
> 动态资源；也不要"并行跑一段时间"。

1. 新主机：`sudo env SITE_ADDRESS=ctf.example.com ./scripts/install.sh`（下载 release tarball + 建用户/布局/内核参数/权限 + 部署）。
   需要 `python3` ≥ 3.11（stdlib `tomllib`）。
2. 运行时镜像：新主机需能拉取 `ghcr.io/floatctf/*:<V>`；离线/内网则按
   [INSTALL.md §6.2](../../INSTALL.md) 在源码机上 `scripts/build-runtime-images.sh --tag <V>`
   后 `docker save` / `docker load`，并把 TOML 的镜像 ref 指向本地名。
3. 数据迁移：`data/postgres/`、`data/rustfs/`、`data/caddy/` 与 `data/caddy-config/` 物理拷贝（原主机停容器后拷贝
   最安全），或逻辑导出/导入。密钥如需延续，原样拷贝 `/var/lib/floatctf/.env` 与
   `config/floatctf.toml`。
4. 迁移后：`sudo systemctl start floatctf.target`，按 [INSTALL.md §11–§12](../../INSTALL.md)
   做健康检查与验收；**真实域名 TLS 由运维方自行验证**（B4，见 INSTALL.md §14.1）。

## 8. 故障排查锚点

- `docker compose -f /var/lib/floatctf/compose.prod.yml logs -f api` —— API 启动/崩溃日志。
- `docker compose -f /var/lib/floatctf/compose.prod.yml ps` —— infra 健康状态。
- `docker inspect floatctf-rustfs --format '{{json .State.Health}}'` —— 对象存储 readiness（R2）。
- `nft list table inet floatctf_awd` —— AWD 赛事防火墙表（单一全局表，R1）。
- `docker network ls | grep -E 'fctf-awd|fctf-awdp-practice'` —— AWD / AWDP 赛事子网。
- `docker ps -a --filter 'name=^fctf-awdp-practice'` —— AWDP 练习容器（**固定命名**，R1）。
- `docker image ls --format '{{.Repository}}:{{.Tag}}' | grep -E '^(ghcr\.io/floatctf|floatctf)/'`
  —— 运行时镜像是否齐备（AWD/AWDP 必需，见 §7 第 2 步 / INSTALL.md §6.2）。