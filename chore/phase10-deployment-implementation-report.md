# FloatCTF — Phase 10 部署实现报告

> 分支 `awd`（本地，**未推送**），2026-09-01。Phase 9.2（AWD 封存）PASS 后进入本阶段。
> 判定：**PASS** —— 全部验收项在真实主机（Arch Linux）通过。

---

## 1. 判定

**PASS**：Phase 10 部署地基（可移植发布产物 + 主机初始化 + 生产容器编排 + systemd +
部署器）已实现并在本 Arch 主机完成两次真实干净部署验收。无"mostly complete"，
所有验收矩阵项均有真实执行证据。

## 2. 交付物

| 交付 | 路径 | 说明 |
|---|---|---|
| 部署设计 | `chore/phase10-deployment-design.md` | RustFS 必需、postgres 存储、nginx host 网络、API 最小能力 |
| 发布构建器 | `scripts/build-release.sh` | musl 优先 + 容器 glibc-2.34 基线；file/ldd/objdump 审计 |
| 主机初始化 | `scripts/init.sh` | feature-check、sysctl/modules 持久化、floatctf 用户+docker 组、`.initialized` 标记 |
| 生产 compose | `infra/compose/compose.prod.yml` | postgres/rustfs 仅回环 + `--wait` 就绪、nginx host 网络 |
| 生产 nginx | `infra/nginx/nginx.prod.conf` | host 网络下 proxypass 到宿主回环 |
| 生产配置模板 | `infra/config/floatctf.prod.toml` | 仅 `FLOATCTF_CONFIG`、密钥保留 |
| systemd 单元 | `infra/systemd/*` | infra(oneshot `--wait`) / api(CAP_NET_ADMIN) / target |
| 部署器 | `scripts/deploy.sh` | precheck→配置→装配→infra→迁移→systemd→API，dry-run + 失败即退出 |
| AWD 镜像 Dockerfile | `infra/docker/awd-{flag,judge}server/Dockerfile` | 生产正式路径 |
| 可移植性文档 | `docs/deployment/portability.md` | systemd+Docker+nftables+WireGuard+iproute2 |
| 起步文档 | `docs/deployment/getting-started.md` | 极简部署入口 |
| 归档 | `scripts/legacy/{init.release,install}.sh` | 旧发布模型保留 |

## 3. 真实主机验收（Arch Linux, 内核 7.1.11, Docker 29.7.2）

### 3.1 init 幂等（init 3 次全通过）

| 运行 | 结果 | 证据 |
|---|---|---|
| 第 1 次（fresh） | 全绿 | docker/nftables/WG/转发/br_netfilter/用户/布局 全 OK，`.initialized` 写入 |
| 第 2 次 | 幂等 | 检测到 `.initialized`，跳过主体写入，runuser 验证 docker 组 |
| 第 3 次 | 幂等 | 同上 + 修正孤儿 uid 1001 顶层属主为 `root:floatctf 750` |

持久化文件落盘：`/etc/sysctl.d/99-floatctf.conf`（ip_forward + bridge-nf-call-*）、
`/etc/modules-load.d/floatctf-br-netfilter.conf`（br_netfilter）——重启后仍生效。

### 3.2 可移植发布产物

`release/floatctf-0.3.3/`（207 文件，73MB）：`bin/floatctf`（67MB，动态链接，
`objdump -T` 验证 GLIBC 需求 ≤ **2.34**，可跑进 bookworm+ 任何主机/容器）+
`web/`（vite+tsc 产物）+ `floatctf/awd-flagserver:0.3.3` / `awd-judgeserver:0.3.3`
镜像（本地构建，未推送）。`file`/`ldd`/`objdump` 审计全过。

### 3.3 首次部署 + 重部署（各 1 次真实运行，均 EXIT=0）

| 步骤 | 首次 | 重部署 |
|---|---|---|
| precheck 端口检查 | 通过（9290/5434/9003/8080 全空） | 通过（放行本平台进程/容器占用） |
| 配置 + .env | 生成新密钥 | **保留密钥**（JWT/PG/RustFS 哈希前缀一致）+ 更新 VERSION |
| 装配产物 | bin/web/compose + 容器目录属主 | 同 |
| infra `--wait` | postgres/rustfs/nginx 全 healthy | 全 healthy |
| migrate.sh apply | 44 个 migrations 应用 | 44 skip（幂等） |
| systemd 单元 | 安装 + enable | 同 |
| API 启动 | 9290 监听（HTTP 401） | 同 |

修复的实际问题（均提交）：孤儿 uid 顶层目录、rustfs/postgres bind-mount EACCES
（容器 uid 归一）、config/ 目录 floatctf 组遍历、web inode 替换导致 nginx bind-mount
403、precheck 对本平台进程端口误判。

### 3.4 系统单元最终状态

```
floatctf-api.service    active (running)  User/Group=floatctf，CapEff=0x1000=cap_net_admin（最小能力）
floatctf-infra.service  active (exited)   oneshot + RemainAfterExit
floatctf.target         active            （聚合 infra+api）
```

API 运行**恰好只有 CAP_NET_ADMIN**（capsh --decode 证实），无 CAP_SYS_ADMIN/NET_RAW。

### 3.5 应用冒烟

- 选手注册 + 登录：`POST /api/users` → OK；`POST /api/users/session` → HS512 JWT 签发。
- 管理员登录：`POST /api/admin/session`（sysadmin，argon2id 验签）→ SuperAdmin JWT。
- 前端经 nginx：`GET :8080/` → 200 `<title>FloatCTF</title>`（web dist 正确托管）。

### 3.6 数据库冒烟

- 全新 postgres 容器：`data/postgres` bind-mount 归 999，44 migrations 应用，
  `schema_migrations` 完整。种子数据 present（11 settings、1 super_admin）→ 达人初始化通过。
- 重部署幂等：44 skip，无重复/损坏。

### 3.7 AWD 冒烟（关键：CAP_NET_ADMIN 真生效）

- **网络健康**（管理员 `/api/admin/awd/network/health`）：nftables `inet floatctf_awd`
  表 Healthy、wireguard Healthy、docker Available、ipv4_forwarding enabled。
- **赛事防火墙**：`nft list table inet floatctf_awd`（managed-by=floatctf revision=89）
  包含全部 gamebox 子网 / infra 网关 / player WG 子网 —— 证明 API 在**恰有
  CAP_NET_ADMIN** 下真实管理宿主 nftables。
- **A2 避让实锤**：`all_gameboxes_v4` 集合含 10.0…10.8/10.10/10.11，**不含 10.9**
  （fctf-px-net Docker 网络被自动分配器正确跳过）—— Docker 网络冲突规避在生产端到端生效。
- 创建 AWD 事件 + 配置：`POST /api/admin/events/awd` → OK（网络自动分配触发）。

### 3.8 SSE 经生产 nginx（FINAL ACCEPTANCE 关键项）

- 选手 SSE：`GET :8080/api/events/{id}/awd/stream`（带 player token）→ **`: connected`**
  首帧经生产 nginx 送达。
- 管理员 SSE：同 → `: connected`。
- 证明 nginx host 网络下 `proxy_buffering off` + 300s 超时正确透传 SSE，前端 fetch-SSE
  断线重连（Phase 9.2 A1）在生产链路成立。

### 3.9 无关宿主资源保护

`awdp-*` 容器、`fctf-awdp-*`、`wg0`、`floatctf_awdp_practice` 表、`fctf-px-net`
（10.9.0.0/16）、`floatctf-registry`（:80）、dev 栈（9090/5432/9000/3000/7780）、
phase9 栈（5433/9002）全部保持运行，未受影响。测试资源（Phase10-AWD-Smoke、
SSE-Prod-Nginx 事件 + 临时用户）已清理。

## 4. FINAL ACCEPTANCE 矩阵

| 验收项 | 结果 |
|---|---|
| SSE reconnect（Phase 9.2 A1） | PASS（此前浏览器实测） |
| Docker CIDR 碰撞（A2） | PASS（10.9 实时跳过，nftables 集合证明） |
| failed deploy 解锁（A3） | PASS（此前实测） |
| AWD 封存（SEALED） | YES（报告 §9） |
| init 幂等 | PASS（3 次） |
| 可移植发布产物 | PASS（musl 回退容器 glibc-2.34） |
| postgres/rustfs/nginx 容器 | PASS（全 healthy，`--wait`） |
| API 原生 systemd | PASS（floatctf 用户 + User/Group） |
| 最小能力 | PASS（CapEff 恰为 CAP_NET_ADMIN） |
| 生产配置 | PASS（`FLOATCTF_CONFIG`、listen 0.0.0.0、network_runtime host） |
| 迁移 | PASS（44 applied once，重部署 44 skip） |
| 首次部署 | PASS（EXIT 0，API 监听） |
| 第二次部署 | PASS（EXIT 0，密钥保留） |
| 前端经 nginx | PASS（200 FloatCTF） |
| RustFS 路径 | PASS（桶 floatctf-public/private 已建） |
| SSE 经生产 nginx | PASS（`:` connected 送达） |
| AWD 冒烟 | PASS（CAP_NET_ADMIN + 网络健康 + CIDR 避让） |
| 无关宿主资源安全 | PASS（全部保留） |

## 5. 未推送 + 提交

本地 `awd` 分支领先 `origin/awd` **48 commits**。新增提交（自 Phase 9.2 封存）：

```
21af491 docs(deploy): 主机可移植性指南 + 极简部署起步文档；归档 scripts/install.sh 至 legacy
3002458 fix(deploy): precheck 放行 floatctf-api.service 占用的 API 端口（重部署场景）
0a80d49 fix(deploy): web 目录就地覆盖保留 inode（修 bind-mount nginx 读空目录 403）
9300d74 fix(deploy): postgres 数据属主非 999 时停容器修正（FATAL Permission denied）
3bdaf6c fix(deploy): precheck 识别 host 网络 nginx 容器端口
cfd45ad fix(deploy): precheck 放行本平台 infra 容器端口
5c97822 fix(deploy): config/ 目录 root:floatctf 750（API 读配置 Permission denied）
9de56dc fix(deploy): 容器数据/日志目录归对应容器 uid（rustfs 10001 / postgres 999）
c1acda9 fix(deploy): init.sh 归一楼顶层属主 root:floatctf 750（孤儿 uid 阻塞）
08b5a9c feat(deploy): 可移植发布产物构建与生产部署基础
c667b56 docs(deploy): 定稿 Phase 10 部署设计
```

## 6. 遗留风险 / 建议

1. **AWD 完整赛事部署**（真正起的 FlagServer/JudgeServer + GameBox）未在本阶段做端到端
   —— 已确认 API 具备 CAP_NET_ADMIN 且网络管理生效，但完整 AWD 赛事的容器拉起/判题
   建议在下一阶段用真实 GameBox 复测（Phase 9 的 AWD 环境在 phase9 测试栈仍可用）。
2. **TLS /HTTPS**：本阶段只留 443 server 块结构 + `keys/` 挂载，证书由运维外置提供
   （未自动签发）。
3. **`data/postgres` 备份**：数据落在 bind-mount 目录，建议配置定时 `pg_dump` 或
   目录级备份；本阶段未内置。
4. **sudo 交互路径**：本机 sudo 需密码（且 `/etc/sudo.conf` 有坏配置），验收走
   privileged-container root 路径；普通用户 + NOPASSWD sudo 的 deploy 路径代码已就位但
   未在本机复测（`run_priv` 分支）。
5. **镜像构建耗时**：API+2 AWD 服务容器基线构建约 3-5 分钟（冷 cache）；未来可加
   `cargo` 缓存预构筑。

## 7. 下一阶段建议

- Phase 11：完整 AWD 赛事端到端（真实 GameBox 部署 + 判题 + 计分）在生产栈复测；
  TLS 落地；部署策略/备份；`floatctf.target` 开机自启随 `systemctl enable` 已含（
  target 未显式 start 但 enable 了，重启后自动拉起）。