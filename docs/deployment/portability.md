# FloatCTF — 主机可移植性指南（Portability）

> Phase 10。本文件说明 FloatCTF 生产部署对宿主机的依赖，以及在“标准 Linux 部署主机”
> 上迁移/移植所需的先决条件。目标：**任何基于 systemd 的 Linux（Arch/Debian/Fedora/RHEL）
> + Docker + nftables + WireGuard + iproute2 的主机均可承载本平台**，不绑定单一发行版。

## 1. 模块→宿主能力映射

| FloatCTF 组件 | 运行形态 | 宿主能力需求 |
|---|---|---|
| API（`floatctf` 二进制） | 原生 systemd 进程 | `floatctf` 用户、`CAP_NET_ADMIN`、`docker` 组访问、可写 `work_dir` |
| PostgreSQL | Docker 容器 | Docker daemon（任意后端）、`127.0.0.1` 回环端口 |
| RustFS | Docker 容器 | 同上（仅回环 9000/9001） |
| nginx | Docker 容器（`network_mode: host`） | **宿主端口 80/443**（无端口映射，直接绑宿主） |
| AWD 运行时 | Docker 容器（赛事动态创建） | `CAP_NET_ADMIN`（API 创建子网/桥/nftables/WireGuard） |

## 2. 宿主必需软件（feature-check，非发行版清单）

| 能力 | 命令 | 用途 | 缺失时 |
|---|---|---|---|
| Docker | `docker info` | 全部容器 | `install.sh` 报错并退出 |
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

- 系统用户 `floatctf`（`useradd --system --shell nologin`），UID 自动分配。
- 加入 `docker` 组（`usermod -aG docker floatctf`）以操作容器。
- `install.sh` 用 `runuser -u floatctf -- docker info` 验证组真实生效。

## 5. 目录布局（`/home/floatctf`）

```
bin/  web/  config/  data/{postgres,rustfs}  logs/{api,nginx,rustfs}  runtime/  gameboxes/
```

`config/` 属 `root:floatctf`（含密钥）；运行数据属 `floatctf`；容器数据目录
分别归容器 uid（postgres 999 / rustfs 10001）。

## 6. 端口约定

| 服务 | 默认 | 说明 |
|---|---|---|
| API | 9090 | 监听 `0.0.0.0`（AWD 容器回连需要），对外暴露由 nftables 限制 |
| PostgreSQL | 5433（回环） | compose 映射 `127.0.0.1:5433` |
| RustFS | 9000/9001（回环） | 仅 `127.0.0.1` |
| nginx | 80/443 | host 网络直绑宿主端口 |

所有端口可经 `/home/floatctf/.env`（或环境变量）调整，`install.sh` 部署前检测冲突。

## 7. 迁移（porting）到另一台主机

1. 新主机：`sudo ./scripts/install.sh`（下载 release tarball + 建用户/布局/内核参数/权限 + 部署）。
2. 数据迁移：`data/postgres/` 与 `data/rustfs/` 物理拷贝（原主机停容器后拷贝
   最安全），或逻辑导出/导入。密钥如需延续，原样拷贝 `/home/floatctf/.env` 与
   `config/floatctf.toml`。

## 8. 故障排查锚点

- `journalctl -u floatctf-api -f` —— API 启动/崩溃日志。
- `docker compose -f /home/floatctf/compose.prod.yml ps` —— infra 健康状态。
- `nft list table inet floatctf_awd` —— 赛事防火墙表（`managed-by=floatctf`）。
- `docker network ls | grep fctf-awd` —— AWD 赛事子网。