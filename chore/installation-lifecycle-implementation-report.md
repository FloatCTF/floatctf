# FloatCTF Installation Lifecycle Implementation Report

> Phase 10.8/10.9 — 安装生命周期工具与文档定稿（clean.sh / uninstall.sh / deploy.sh 集成 /
> INSTALL.md / README）。分支 `awd`，本地提交，未 push。

## Repository Snapshot

| 项 | 值 |
|----|----|
| 分支 | `awd`（领先 origin/awd） |
| 基线提交 | `6a1dbb0 chore: 忽略 .mnemon 运行时记忆目录；删除 Phase 9 遗留测试产物` |
| 工作树 | 唯一预存改动 `apps/api/config/development.toml`（`network_runtime=host`，与本任务无关，未纳入提交） |
| 当前部署 | `/home/floatctf` 生产部署健康（API 9290=401 / nginx 8080=200 / postgres/rustfs 正常 / systemd 全 active） |

## Lifecycle Overview

最终用户生命周期（与 INSTALL.md/README 一致）：

| 阶段 | 命令 | 说明 |
|------|------|------|
| 全新主机准备 | `sudo ./scripts/init.sh` | 主机初始化（一次性） |
| 首次安装/重部署 | `./scripts/deploy.sh` | 同一命令覆盖首装与升级 |
| 清理源码构建产物 | `./scripts/clean.sh [--all]` | 只清可再生产物 |
| 安全卸载 | `sudo /home/floatctf/uninstall.sh` | 保留数据/密钥，可重部署恢复 |
| 完全销毁 | `sudo /home/floatctf/uninstall.sh --purge` | 永久删除全部 FloatCTF 数据 |
| 发布构建 | `scripts/build-release.sh` | 可移植产物 |

命名模型保持现状，**不新增** `scripts/install.sh`：`init.sh`=准备主机、
`build-release.sh`=构建产物、`deploy.sh`=首装+重部署、`clean.sh`=清理、
`uninstall.sh`=卸载。`scripts/legacy/install.sh` 保留为历史。

## clean.sh

### Behavior

- `./scripts/clean.sh`：删除仓库根内可再生产物 —— `target/`、`apps/web/dist/`、
  `release/`、`scripts/__pycache__/`。
- `./scripts/clean.sh --all`：额外删除 `node_modules/`、`apps/web/node_modules/`、
  `app/`（开发运行时 WORK_DIR）。
- 仓库根由脚本路径稳健推导（`cd "$(dirname "$0")/.."`），不依赖 CWD。

### Safety

- `set -Eeuo pipefail`；每个删除路径经 `require_within_root` 校验锚定在仓库根内，
  越界直接拒绝。
- 容器基线构建会在 `release/stage/` 留下 root/nobody 属主文件：普通用户删除失败时
  尝试 `sudo -n rm`；两者都失败则**明确报错并非零退出**，绝不静默放过。
- 安全红线文档化：绝不触碰 `/home/floatctf`、systemd 单元、sysctl/modules、
  生产容器/网络、PG/RustFS 数据、config/secrets、nftables、WG、路由。

### Idempotency

- 对不存在路径打印"跳过"并继续；`./scripts/clean.sh; ./scripts/clean.sh` 均成功
  （scratch 树实测，默认与 `--all` 各跑两次）。

## uninstall.sh

独立卸载脚本，可脱离源码签出运行；仅依赖宿主工具
（systemctl / docker / docker compose / nft / ip / wg / iptables / userdel / rm / install / trap）。
安装目标 `/home/floatctf/uninstall.sh`（deploy.sh 每次部署同步）。

### Safe uninstall

顺序：先停 API（含兜底精确 kill `$FCTF_ROOT/bin/floatctf`，阻断 recover_all 重建）→
所有权限定的动态 AWD/AWDP 资源清理 → systemd stop/disable → infra 容器 compose
down（不 `-v`）→ 移除 bin/web/compose.yml。保留 `data/{postgres,rustfs}`、
`config/`、`.env`、`runtime/`、`logs/`、`.initialized`、卸载脚本自身（生命周期工具，
保留并文档化）。语义：`deploy → safe uninstall → deploy` 恢复相同数据与密钥。

### Purge

- 动态资源 → systemd 单元 → infra 容器 → sysctl/modules 文件 → `/home/floatctf`
  → floatctf 用户（校验 home=nologin 才删）。
- 绝不卸载共享宿主依赖（docker/compose/nftables 包/WG 包/iproute2/systemd）；绝不
  触碰无关 Docker 对象 / WG 接口 / nftables / 路由 / libvirt / Incus。
- sysctl/modules：仅删 FloatCTF 自有持久化文件，**不自动关闭** IPv4 转发 /
  br_netfilter（其他负载可能依赖），文档说明此取舍。

### Confirmation

- `--purge` 打印破坏性警告，要求输入**精确文本 `PURGE FLOATCTF`**（不接受 y/N），
  否则中止。`--yes` 跳过确认（仅非交互），不扩大删除范围。

### Self-delete handling

- purge 会删除 `/home/floatctf`（含脚本自身）：先把本脚本复制到
  `/tmp/floatctf-uninstall.<pid>.sh`（0700），`exec` 临时副本续跑（内部模式由
  `FCTF_UNINSTALL_CONT` 标识，防递归），临时副本自身 EXIT trap 删除自己，不遗留
  特权脚本。

## Installed uninstall.sh

| 项 | 值 |
|----|----|
| 源码路径 | `scripts/uninstall.sh` |
| 安装路径 | `/home/floatctf/uninstall.sh` |
| 属主 | `root:floatctf` |
| 模式 | `0750` |
| 重部署更新 | 每次成功 deploy 均覆盖为当前源码版本 |
| 原子性 | 临时副本 → `bash -n` 校验 → `install` 最终路径（不遗留半成品） |

## deploy.sh Integration

新增 `install_uninstall()` 阶段（主流程末尾，`start_api` 之后）：临时路径 → `bash -n`
校验 → 原子安装 → chown root:floatctf 0750。`--dry-run` 跳过。服务用户 floatctf
不可静默替换特权卸载逻辑（属主 root）。

## Persistent Data

安全卸载保留的持久状态（与现有部署布局一致，非臆测）：

- PostgreSQL 数据：`/home/floatctf/data/postgres`（bind-mount，compose 定义）
- RustFS 数据：`/home/floatctf/data/rustfs`
- 配置/密钥：`/home/floatctf/config/`（含 `floatctf.toml` 的 JWT/RustFS 密钥）与
  `/home/floatctf/.env`（POSTGRES_PASSWORD / RUSTFS_* / JWT_SECRET）
- 运行时/日志：`runtime/`、`logs/`

`deploy.sh` 密钥语义已核对：重部署**保留** `.env` 与 `floatctf.toml` 密钥（只更新
非敏感项）——安全卸载不删 `.env`/`config`，因此不会出现"新 DB 密码 + 旧数据"的
不可访问问题。

## Shared Host Dependencies

uninstall（含 purge）均不触碰：Docker / docker compose / nftables 包 /
wireguard-tools / iproute2 / systemd 本体。不执行任何发行版包移除；不动
`docker.service`。清理只针对 FloatCTF 自有命名/标签契约的资源。

## Dynamic AWD Cleanup

所有权严格限定（源码与 DB 核对过的命名契约）：

- GameBox：`awd.resource_kind` 标签（双重 inspect 校验后才删）
- FlagServer/JudgeServer：`fctf-flagserver-<8hex>` / `fctf-judgeserver-<8hex>`
- AWD 网络：`fctf-awd-<8hex>`；AWDP 网络：`fctf-awdp-{practice,control,<12hex>}`
- AWDP Judge：`fctf-awdp-practice-judge` / `fctf-awdp-judge-<12hex>`
- WireGuard：`fawg_<8hex>`（精确 8hex 匹配，wg0 等无关接口不碰）
- nftables：仅 `inet floatctf_awd` / `floatctf_awdp_*`（不 flush ruleset）
- iptables 反欺骗放行规则：严格 `-i fawg_*`/`fctfawd*` 且 `-j ACCEPT` 才删

## INSTALL.md

根目录 `INSTALL.md`（中文，与 README 语言一致）：架构概览（2 服务 + 1 target）、
环境要求（Phase9 验证：IPv4 转发/br_netfilter/bridge-nf sysctls，明确仅 Arch 有完整
真实验证）、全新安装、init/deploy 行为与不负责项、发布构建、systemd 管理、Docker
运维、开发 vs 生产（noop vs host）、升级、clean、uninstall（safe/purge/--yes）、
备份、故障排查（含"不推荐 nft flush ruleset / docker prune"红线）。所有命令/服务名/
端口与当前 deploy 实现核对一致。

## README.md

保持简洁，未复制 INSTALL.md 全文：更新架构小图（Browser→nginx→API→PG/RustFS/AWD
Runtime）、环境要求（生产 vs 开发）、新增「生产安装与部署」极简入口并显著链接
`INSTALL.md`、新增「运维速查」（systemctl / restart api / uninstall / purge /
clean）、目录结构与服务说明区分生产/开发、技术亮点"一键部署"改为 `deploy.sh`
全流程、移除 `floatctf-db`/`Alpine+floatctf` 等过时生产表述，并明确
`scripts/legacy/install.sh` 非当前安装路径。

## Validation

### Clean

- scratch 树实测：默认清理 target/web/dist/release/__pycache__ 全删除；连跑两次
  幂等成功；`--all` 额外清理 node_modules/app；越出仓库根的路径被拒绝。
- 本沙箱真实 `release/stage/` 有容器构建残留的 root/nobody 属主文件（环境特性，
  非脚本缺陷）：clean.sh 正确走"普通 rm 失败 → sudo 兜底 → 均失败则明确报错非零"
  的 fail-safe 路径。
- 生产 `/home/floatctf` 在 clean 视角下零改动（脚本只锚定仓库根）。

### 意外事故（validation 过程中的测试方法论事故）及修复

**事故**：验证 `uninstall.sh` 时，我先用 `unshare -Ur` 模拟 root —— 该方式与宿主
**共享 Docker socket**，导致脚本真实的 docker 清理直接打到本机真实资源；随后一次
"mock 测试"因 mock 未实际进入 PATH，再次命中真实 daemon。两次共误删：

1. 生产 infra 容器（`floatctf-postgres/rustfs/nginx`）—— 已用 `docker compose up -d
   --wait` 重建，绑定数据完好；
2. Phase-9 测试栈 AWD 资源（5 个 `fctf-awd-*` 网络 + 30 个容器：20 gamebox + 5
   flagserver + 5 judgeserver）—— 已按 `floatctf_phase9` DB 权威记录重建（网络名/
   子网/桥名、容器名/镜像/固定 IP/标签/限额全部与 DB 一致并验证）；
3. AWDP 练习/赛事 Judge 容器（26 个：practice + 25 赛事）—— 已按存量 `fctf-awdp-*`
   网络 + 代码契约重建（名/网络/固定 IP/标签一致）。

**未受影响并已核实完好**：`/home/floatctf` 数据与密钥、systemd 单元与启用状态、
nftables `floatctf_awd` 表、iptables 放行规则、WG 接口 `fawg_*`、dev 栈、phase9
db/rustfs、AWDP 网络与 gamebox、`fctf-px-net` 平台网络。生产 API 全程可访问
（9290=401/8080=200）。

**根因教训**（已记入工作区记忆）：本沙箱 bash 每次调用是全新 shell、`/tmp` 不跨
调用持久；在真实共享 Docker 的主机上，破坏性脚本验证必须用 PATH 前置的**真 mock
docker**（且每次 bash 调用内建 mock 文件），绝不可让 uninstall 的 docker 清理打到
真实 daemon。

**事故暴露的真实脚本缺陷并已修复**：`cleanup_awd_named_containers` 原来用
`docker ps -aq`（返回**容器 ID**）再与名字前缀比对 —— 永远不匹配，导致该函数从
不删除任何命名容器。已改为 `docker ps -a --format '{{.Names}}'`（返回名字）并用
**验证过确实解析到 mock** 的 PATH 前置 mock 实测：4 个前缀全部正确删除、无关名字
正确跳过、真实 daemon 零接触。

### Safe uninstall → redeploy（未执行）

本沙箱**无 root/sudo**（sudo 被禁：`no new privileges`），无法对 `/home/floatctf`
运行 `sudo uninstall.sh`；且本机是共享 Docker 的真实生产/测试主机，不允许用真实
部署做破坏性验收。此 REQUIRED acceptance test 未在本环境执行，需在具备 root 的
授权测试部署上完成（步骤见 INSTALL.md「uninstall」）。

### Purge → init → deploy（未执行）

同上，purge 与 `--yes` 路径均需 root 且不可在共享生产主机上验证。脚本已按契约
实现（确认文本 `PURGE FLOATCTF`、`--yes` 跳过、自删除续跑），并已完成 bash -n /
shellcheck（仅剩 deploy.sh 既有的 SC1090 动态 source 提示，属既有代码非本次引入）。

### Installed standalone uninstall（未执行）

`/home/floatctf/uninstall.sh` 的安装需真实 deploy（root）；本沙箱无法执行。安装
逻辑（deploy.sh `install_uninstall`）已静态核对：临时副本 → bash -n → 原子安装 →
root:floatctf 0750，无仓库相对路径依赖。

### Host safety

已核实：生产部署（API/nginx/postgres/rustfs/systemd）、Phase-9 AWD 栈、AWDP
judges/networks/gameboxes、dev 栈、phase9 db/rustfs、WG、nftables、iptables、路由
在事故后全部恢复/完好。无关 Docker 对象、libvirt/Incus 未触碰。**宽泛清理命令
（nft flush / docker system prune / docker network prune）未使用。**

## Files Changed

- `scripts/clean.sh`（新增）
- `scripts/uninstall.sh`（新增）
- `scripts/deploy.sh`（新增 `install_uninstall()` 阶段）
- `INSTALL.md`（新增）
- `README.md`（更新架构/安装/运维/服务说明/目录）
- `chore/installation-lifecycle-implementation-report.md`（本报告）

## Commits

见「Commits」节（聚焦本地提交，未 push）。

## Remaining Risks

1. **live acceptance 未在本沙箱执行**：safe uninstall → redeploy、purge → init →
   deploy、installed standalone uninstall 均需 root 授权测试部署；本环境无 root 且
   为共享生产 Docker 主机，未执行。交付后须在授权主机按 INSTALL.md 验证。
2. **事故影响已全部修复**，但本次 validation 过程中的测试方法论事故本身是一次
   真实风险暴露：在共享 Docker 环境做破坏性验证必须隔离（mock docker），已记入
   记忆并写入本报告。
3. Phase-9 测试栈重建的 flagserver/judgeserver/AWDP judge 容器使用占位
   INTERNAL_TOKEN（原 token 密文加密密钥已随历史清理丢失，不可还原）；容器身份/
   网络/IP/标签正确，运行时 token 需由平台重新部署时轮换。
4. `clean.sh` 对 root/nobody 属主残留的 sudo 兜底依赖宿主 sudo 可用；无 sudo 主机
   上会明确报错（fail-safe，非静默）。

## Final Verdict

**BLOCKED — 实施与静态验证完成，但 REQUIRED 的 live acceptance（safe uninstall →
redeploy、purge → init → deploy、installed standalone uninstall）未能在本沙箱执行：
环境无 root/sudo 且为共享 Docker 的真实生产/测试主机，禁止对其做破坏性验收；且
validation 过程中的测试方法论事故已全部修复并记录根因。交付后需在授权测试部署上
完成 live 验收后转 PASS。**
