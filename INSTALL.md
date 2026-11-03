# FloatCTF 安装与部署

单机生产安装：**systemd 托管一套 Docker Compose 栈**。本地源码开发见 [DEVELOPMENT.md](./DEVELOPMENT.md)。

> ⚠️ **单控制面不变量（R1）**：同一宿主 / 同一个 Docker daemon 同时只允许一个 FloatCTF API。
> 不要在生产宿主上再跑开发栈、RC 栈或第二份安装——它们会 reconcile 全局资源
> （`floatctf_awd` nft 表、固定命名的 AWDP 网络与容器），干扰线上实例。

---

## 0. 前置条件

| 项 | 要求 |
|---|---|
| 系统 | Linux + systemd；Arch / Debian / Ubuntu 的包名已适配 |
| Docker | Docker Engine + compose v2 插件（`docker compose`，不是 `docker-compose`） |
| Python | 3.11+（宿主侧 `db/migrate.sh` 用 stdlib `tomllib`；安装器会先做能力探测） |
| 网络 | `SITE_ADDRESS` 的 80/443 可从公网到达（Caddy 签证书）；出网可达 Docker Hub 与 GitHub |
| 权限 | root（`sudo`） |

---

## 1. 安装

```bash
curl -fsSL https://github.com/FloatCTF/floatctf/releases/latest/download/install.sh -o install.sh
sudo env SITE_ADDRESS=ctf.example.com bash install.sh
sudo systemctl start floatctf.target
```

安装器依次做：装宿主包 / 建组 → 下载 5 个 release 产物 → 按版本拉 7 个部署模板 →
拉 4 个镜像 → 渲染配置 → 对既有库补迁移 → 写并 enable 三个 systemd 单元（**不启动**容器）。

首次 `systemctl start` 时 Postgres 用 `merged.sql` 初始化空库。

### 常用参数

| 参数 / 环境变量 | 作用 |
|---|---|
| `--version <V>` / `FLOATCTF_VERSION` | 指定版本；默认跟随 latest release |
| `FLOATCTF_HOME` | 安装根，默认 `/var/lib/floatctf` |
| `FLOATCTF_RUNTIME_IMAGE_REGISTRY` | 运行时镜像 registry 前缀，默认 `docker.io/floatctf`（可指向镜像站） |
| `FLOATCTF_TEMPLATE_BASE` | 部署模板来源，默认 `https://raw.githubusercontent.com/FloatCTF/floatctf` |
| `--skip-migrations` / `FLOATCTF_SKIP_MIGRATIONS=1` | 跳过升级时的迁移 |
| `--skip-runtime-images` / `FLOATCTF_SKIP_RUNTIME_IMAGES=1` | 跳过 AWD/AWDP 镜像校验（只跑 Jeopardy） |

---

## 2. 装了什么

```text
宿主（systemd）
└── floatctf-helper.service        唯一非容器进程：docker 组 + CAP_NET_ADMIN
        ├── /run/floatctf/helper-control.sock    nftables / WireGuard / conntrack
        └── /run/floatctf/helper-docker.sock     受限 Docker 代理

Compose（compose.prod.yml）
├── api          floatctf-api:<V>，非 root 65532、cap_drop ALL、no-new-privileges、read-only
├── postgres     postgres:17
├── redis        redis:7-alpine
├── rustfs       rustfs/rustfs（S3 对象存储）
└── caddy        caddy:2-alpine（80/443，反代 API 与前端）
```

| systemd 单元 | 作用 |
|---|---|
| `floatctf-helper.service` | 宿主控制面 |
| `floatctf-infra.service` | `docker compose -f compose.prod.yml up -d --wait` |
| `floatctf.target` | 聚合上面两个；`systemctl start/stop floatctf.target` 即整栈启停 |

| 端口 | 暴露 |
|---|---|
| 80 / 443（Caddy） | 公网 |
| 5433 / 6380 / 9000 / 9001 | 仅 `127.0.0.1` |
| API 9090 | 仅 Compose 网络内（不发布到宿主） |

### 目录布局（`$FLOATCTF_HOME`）

```text
compose.prod.yml  .env          # 栈定义 + 部署元数据/密钥
config/floatctf.toml            # 业务配置（API 只读这个文件）
config/caddy/Caddyfile          # 站点配置
config/oidc/                    # OIDC 签名私钥（[oidc].signing_keys 引用）
data/{postgres,rustfs,caddy}    # 持久数据
logs/rustfs  runtime  web/      # 日志 / 运行时可写目录 / 引导页
frontends/  frontend.sh         # 前端存储 + 前端管理器
db/migrate.sh  db/migrations/   # 迁移器
backup.sh  restore.sh           # 备份 / 恢复
uninstall.sh                    # 卸载
```

---

## 3. 配置

| 文件 | 内容 | 改完 |
|---|---|---|
| `.env` | 部署元数据：`VERSION`、`FLOATCTF_HOME`、`FLOATCTF_UID/GID`、端口、数据库与 RustFS 凭据、JWT / AWD / internal 密钥 | 重启整栈 |
| `config/floatctf.toml` | 业务配置：`[application]`、`[server]`、`[database]`、`[redis]`、`[storage]`、`[awd]`、`[awdp]`、`[oidc]` | 重启整栈 |

```bash
sudo systemctl restart floatctf.target
```

- 平台动态设置（站点地址、OIDC 开关、出网代理等）走**管理端**，改完即时生效，不必重启。
- 安装器不会覆盖你改过的 `config/floatctf.toml` / `.env` 值。
- OIDC 私钥：`sudo scripts/oidc-keygen.sh $FLOATCTF_HOME/config/oidc`（详见 [docs/identity/oidc-operations.md](./docs/identity/oidc-operations.md)）。

---

## 4. 日常运维

```bash
# 状态
systemctl status floatctf.target floatctf-helper
sudo docker compose -f /var/lib/floatctf/compose.prod.yml ps

# 日志
journalctl -fu floatctf.target floatctf-infra
sudo docker compose -f /var/lib/floatctf/compose.prod.yml logs -f api caddy

# API 存活（401 = 服务在跑且鉴权生效）
curl -sS -o /dev/null -w '%{http_code}\n' https://$SITE_ADDRESS/api/users/me

# 重启单服务 / 整栈
sudo docker compose -f /var/lib/floatctf/compose.prod.yml restart api
sudo systemctl restart floatctf.target
```

镜像只有 4 个，全部来自 Docker Hub：
`docker.io/floatctf/{floatctf-api,awd-flagserver,awd-judgeserver,awdp-judgeserver}:<V>`。
离线机：在有网机器 `docker save`，目标机 `docker load`，再重跑安装器。

前端管理：

```bash
sudo /var/lib/floatctf/frontend.sh list | info <version> | verify <version> | install <pkg> | set-current <version> | remove <version>
```

---

## 5. 升级

```bash
sudo env SITE_ADDRESS=ctf.example.com bash install.sh    # 同一入口，幂等
sudo systemctl restart floatctf.target
```

迁移是 **forward-only**：`db/migrate.sh` 按 `schema_migrations` 补齐新增版本，不回滚、不改历史文件。

```bash
sudo FLOATCTF_CONFIG=/var/lib/floatctf/config/floatctf.toml /var/lib/floatctf/db/migrate.sh status
sudo FLOATCTF_CONFIG=/var/lib/floatctf/config/floatctf.toml /var/lib/floatctf/db/migrate.sh apply
```

---

## 6. 备份与恢复

```bash
sudo /var/lib/floatctf/backup.sh                      # 产出 ./floatctf-backup-<UTC>.tar.gz
sudo /var/lib/floatctf/restore.sh <归档> --yes         # 恢复
```

归档包含 `.env` 密钥、`config/`、数据库 dump、RustFS 数据、前端集合与版本元信息。

---

## 7. 卸载

```bash
sudo /var/lib/floatctf/uninstall.sh                  # 安全卸载：保留数据、配置、密钥、前端
sudo /var/lib/floatctf/uninstall.sh --force          # 跳过「活跃 AWD/AWDP 运行时」守卫
sudo /var/lib/floatctf/uninstall.sh --purge          # 连数据一起删（需输入 PURGE FLOATCTF）
sudo /var/lib/floatctf/uninstall.sh --purge --yes    # 非交互永久删除
```

安全卸载 = 移除 systemd 单元、容器/赛事运行时、API 镜像、静态产物，**保留** `data/`、`config/`、`.env`、`frontends/`。
重新跑安装器即可回到同一套用户与赛事数据。

---

## 8. 网络与安全边界

- API 容器**没有** `/var/run/docker.sock`、没有 `CAP_NET_ADMIN`、rootfs 只读；Docker 与宿主网络操作只能经 helper 的两个 Unix socket。
- `fctf-platform-control`：Compose 声明为 `external` 的 internal bridge（`10.42.8.0/24`），只给 API 与 FlagServer/JudgeServer 内部回调；GameBox 不加入。
- 若同 subnet 已被其它 Docker 网络占用（例如历史遗留的 `fctf-awdp-control`），安装器直接报错，先手动清理再重跑。
- 宿主上只创建 `floatctf` 组（不创建同名用户），API 以数值 `65532:$FLOATCTF_GID` 运行。

---

## 9. 故障排查

| 症状 | 检查 | 常见原因 |
|---|---|---|
| 安装器报缺 `tomllib` | `python3 -c 'import tomllib'` | Python < 3.11，装新版 python3 |
| 启动失败，API 反复重启 | `docker compose logs api` | 配置写错 / 数据库未就绪 / helper socket 不在 |
| `helper-control.sock` 不存在 | `systemctl status floatctf-helper` | helper 未启动，看 `journalctl -u floatctf-helper` |
| 拉镜像 401/超时 | `docker pull <ref>` | 网络或镜像站问题；用 `FLOATCTF_RUNTIME_IMAGE_REGISTRY` 换源或离线 `docker load` |
| 证书签发失败 | `docker compose logs caddy` | DNS 未指向本机 / 80-443 不可达 |
| 容器健康检查失败 | `compose ps` + 单服务日志 | 依赖未 healthy 时 API 会等待 |
| 端口被占 | `ss -lntp \| grep -E ':(80\|443\|5433\|6380\|9000)'` | 已有服务占用，改 `.env` 端口或释放 |
| 装旧版本报模板缺失 | 版本与安装器是否配套 | 用目标版本自带的 `install.sh` |

---

## 10. 产物与版本

一次 `v*` tag 发布两条通道：

- **GitHub Release**：11 个文件（`install.sh`、`floatctf`、`floatctf-helper`、`web-dist.tar.gz`、`merged.sql`、`frontend.sh`、`ops-tools.tar.gz`、三个 npm 包、`SHA256SUMS`）。
- **Docker Hub**：4 个镜像，tag = 版本号。

安装器默认装 latest；`--version 1.0.6` 钉版本。部署模板按版本从仓库 tag 拉取，
所以**不要用新安装器去装没有模板的旧版本**（会 fail closed 并提示）。

发布流程见 [RELEASE.md](./RELEASE.md)。
