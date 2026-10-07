# FloatCTF Phase 13 — Final RC & Release Acceptance

> 本报告由 Phase 13（Final Release Candidate & Release Acceptance）产出。
> **未执行任何发布动作**：没有 push、没有打 tag、没有创建 GitHub Release、没有 npm publish、没有合并分支。
> 所有代码改动均为 `release/v1.0` 上的本地提交。

---

## 1. Verdict

> **更新（Phase 13.1 收口）**：本节的判定是 Phase 13 当时的**原始**结论，予以保留。
> Phase 13.1 已关闭全部代码侧工作（B1 运行时镜像分发 / R2 RustFS 就绪 / R3 tomllib 前置 / 文档），
> 且产品负责人**豁免** B4。B2（独占宿主 AWD/AWDP E2E）与 B3（特权安装/升级/卸载/purge）
> **仍需独占宿主**，本环境不可执行。最终状态见文末 **## Phase 13.1 — GA Closure**。

**BLOCKED — 存在三类环境/前置阻塞，非代码正确性阻塞**

1. **特权闸门无法在本机执行**：agent 沙箱带 `NoNewPrivs=1`，`sudo` 直接拒绝（`The "no new privileges" flag is set`），`unshare -r` 亦被拒绝（`cannot open /proc/self/uid_map: Permission denied`）→ 无法获得 root，因此 **G1 全新安装（`install.sh` 的宿主初始化阶段）/G8 安全卸载→重装/G10 purge→全新安装/G9 正常用户 sudo 路径** 无法端到端执行。已用「真实安装器产物 + 隔离 FATCTF_HOME + 真实 Compose 栈」把可执行部分真实跑通并取证。
2. **AWD/AWDP 宿主级完整 E2E 被明确禁止执行（生产保护）**：平台的 AWD 防火墙是**单一全局 nft 表** `floatctf_awd`（AWDP 练习为 `floatctf_awdp_practice`），且练习网络/容器名固定（`fctf-awdp-practice` / `fctf-awdp-practice-judge`）。本机**正在运行一个重要生产实例**，第二个 API 会删除并重写该全局状态。**实测已复现此风险**（见 §23），因此按用户 §5 规定，破坏性闸门标记为 BLOCKED，不伪造。
3. **无真实域名/DNS/TLS（外部阻塞）**：`REAL-DOMAIN HTTPS: EXTERNAL BLOCKER`。

**代码可控的发布工程已完成并全部实测通过**：备份/恢复（含真实破坏 + 真实恢复演练）、发布工作流安全语义、SHA256SUMS、SDK 分发包与外部消费验证、RC 非发布工作流、版本一致性门禁、安装器升级路径与卸载守卫、文档。发现并修复 **2 个会直接导致 v1.0 装不起来的 GA 阻塞缺陷**（§22 F1/F2）。

依据用户 §43 的定义，"V1.0.0 READY — EXTERNAL HTTPS VALIDATION REQUIRED" **不适用**（因为除 DNS/TLS 外还存在上述环境阻塞，且 AWD/AWDP 与安装/卸载/purge 未被证明）。

---

## 2. Repository Snapshot

| 项 | 值 |
|---|---|
| 分支 | `release/v1.0`（本地创建，未 push） |
| base main | `d6f86d24147ce9fd2422f867e4f2f1c549bbfd11`（= Phase 12/PR #6 的 merge commit） |
| Phase 13 起始 HEAD | `35c04b41abcfbab52697524a03167cae0b0cb508`（`origin/ui`，与 `origin/main` 树一致） |
| 最终本地 HEAD | 见 §24（本报告的本地提交批次） |
| 本地/远端 | 全程 `origin/main` = `d6f86d2`，未推送任何提交；`release/v1.0` 仅存在于本地 |
| 冻结树基线 | `35c04b4`：`mise run check` exit 0（前端 359 项 + 管理器 72 项）、`mise run build` exit 0、`git diff --check` OK |

**Phase 12 已在 main 上的取证**（硬门禁）：`git merge-base` 确认 merge commit 双亲为 `5f3b153`（旧 main）+ `35c04b4`；`origin/main` 上 `packages/sdk`、`packages/react`、`packages/frontend-runtime`、`frontends/default`、`apps/web`、`scripts/frontend.sh` **均存在**；`origin/main` 文件总数 1030。

---

## 3. Release Candidate Artifacts

本机以 release 工作流的**同一路径**构建（`cargo build --locked --release -p floatctf -p floatctf-helper --bins` → `package-web-dist.sh` → `migrate.sh make`）。以下为实际产物（`var/rc13/artifacts/`）：

| artifact | size | sha256（前 16） | validation |
|---|---|---|---|
| `floatctf` | 48M | `aad7cc64db6536c6` | ELF 64-bit LSB pie x86-64；`ldd` not-found=0；容器内 user=65532:65532 |
| `floatctf-helper` | 2.8M | `6c7624c82d8b4f79` | ELF 64-bit LSB pie x86-64；`ldd` not-found=0 |
| `web-dist.tar.gz` | 2.6M | `dc1b5bd71a2b087f` | 顶层 `bootstrap/` + `frontends/default/1.0.0/`；无 `node_modules`/`src`/`.env`（grep=0）；`verify-release-frontend.sh` OK |
| `merged.sql` | 360K | `c898a88a2a2bdd88` | 由 48 个 migration 确定性生成（`migrate.sh make`） |
| `frontend.sh` | 68K | `8babf9c47e435b99` | `bash -n` OK；管理器契约/安全测试 72/72 |
| `install.sh` | 88K | `492c106c2f411e79` | `bash -n` OK；安装器契约测试 50/50 |
| `backup.sh` | 20K | `4f089a9f1c1d2ac2` | `bash -n` OK；真实备份演练通过 |
| `restore.sh` | 24K | `00cd2e5e610d3342` | `bash -n` OK；真实破坏→恢复演练通过 |
| `ops-tools.tar.gz`（CI 生成） | — | `7261664ea7a24151` | 51 成员（`backup.sh`/`restore.sh`/`db/migrate.sh`/48 migrations）；白名单断言；**两次构建字节相同** |
| `floatctf-sdk-1.0.0.tgz` | 108003 | `463d676911bb680d` | 外部消费者实测（npm 安装 + tsc + node 运行） |
| `floatctf-react-1.0.0.tgz` | 29387 | `55eb53ff6f566f61` | 同上；`workspace:*` 已被 pnpm 重写为 `1.0.0` |
| `floatctf-frontend-runtime-1.0.0.tgz` | 43735 | `8facd631f29de649` | 同上 |
| `SHA256SUMS`（CI 生成） | — | — | 覆盖除自身外全部 10 个制品；`sha256sum -c` 全 OK |

归档布局/安全断言：`tar -tzf` 逐成员校验，拒绝绝对路径 / `..` / 符号链接 / 硬链接；`--sort=name --numeric-owner --owner=0 --group=0 --mtime=@0` + `gzip -n` 保证确定性。

---

## 4. Production Architecture Verification

| 要求 | 实测证据 | 结果 |
|---|---|---|
| API 在 Compose | `compose.prod.yml` 服务 `api`；`docker ps` 中 `fctf-rc13-api` | ✅ |
| PostgreSQL/Redis/RustFS/Caddy 在 Compose | 5 服务全部 healthy（`docker inspect .State.Health.Status`） | ✅ |
| 特权宿主操作走 `floatctf-helper` | `[docker] socket_path=/run/floatctf/helper-docker.sock`；容器仅挂 `/run/floatctf:ro` | ✅ |
| API 非 root | `docker inspect -f '{{.Config.User}}'` = `65532:65532` | ✅ |
| API 无 Docker socket | 容器 `Mounts` 不含 `/var/run/docker.sock`（只读挂 `/run/floatctf`） | ✅ |
| API 无 `CAP_NET_ADMIN` | `cap_drop: [ALL]` + `security_opt: no-new-privileges:true` + `read_only: true` | ✅ |
| 宿主网络控制仍由 helper 中介 | RC 配置 `[awd] network_runtime = "noop"`（刻意隔离）；生产模板为 `helper` | ✅ |
| systemd 只拥有 helper + infra/target | 宿主仅 `floatctf-helper.service`（active）；**无** `floatctf-api.service`；`install_systemd` enable `floatctf.target floatctf-infra.service floatctf-helper.service` | ✅ |
| **生产根 = 代码默认** | `scripts/install.sh:44` `FLOATCTF_HOME="${FLOATCTF_HOME:-/var/lib/floatctf}"` | ✅ `/var/lib/floatctf` |
| 未复活 nginx / `/home/floatctf` / 原生 API 服务 | 文档 grep：nginx 0 处（仅 fcmc 单测镜像名）；`/home/floatctf` 仅出现在 uninstall 的历史用户清理 | ✅ |
| 开发入口仍是 `mise run dev` | `mise.toml` `[tasks.dev]` depends `infra:up` → `scripts/dev.sh`；`dev:web` 先 `build:packages` 再起 Default Frontend Vite | ✅ |
| 前端平台结构 | `apps/web` bootstrap（无 React）+ `frontends/default` + 三个 packages + 外部前端支持 | ✅ |

> 说明：本机**真实生产实例**位于非默认根 `/home/fb0sh/floatctf-prod`（用户此前手工部署），且其 `frontends/` 尚不存在（Phase 12 之前的形态）。本阶段所有验证都在**独立 RC 根**内进行，未触碰生产根。

---

## 5. Fresh Install

| 子项 | 结果 |
|---|---|
| 真实 `sudo bash install.sh` 端到端（宿主初始化：包/sysctl/modules/组/systemd enable） | **BLOCKED**：沙箱 `NoNewPrivs=1`，`sudo` 被拒（精确报错见 §1.1）。非代码缺陷。 |
| 安装器**产物装配**是否能真正部署 | ✅ 真实执行：用 release 工作流的同一脚本产出 8 个产物，装配到隔离 `$FLOATCTF_HOME`，`docker build` 出 `floatctf/api:rc13`（user=65532:65532），5 服务全部 healthy |
| 全新数据库 bootstrap | ✅ 空 PGDATA 首次启动由 `merged.sql` 一次跑完 **48 个 migration**；`FRONTEND_ACTIVE=default`、`super_admin` 1 行、`NODE_IP`/`MAIN_URL` 已 seed |
| bootstrap 引导页 + Default Frontend | ✅ `bootstrap/` 落到 `$FLOATCTF_HOME/web`；`frontend.sh install ... --platform --make-current` 安装 `default@1.0.0`，`frontends/registry.json` 生成 |
| 安装后布局 | `compose.prod.yml`、`.env`(0640 root:966)、`config/floatctf.toml`(0640)、`config/caddy/Caddyfile`、`web/`、`frontends/`、`runtime/`、`merged.sql`、`frontend.sh`、`backup.sh`、`restore.sh`、`db/{migrate.sh,migrations/,merged.sql}` |
| **发现的 GA 阻塞缺陷** | 见 §22 F1：生产 Caddy 挂载嵌套在只读 `/srv` 内 → **Caddy 容器无法启动**（已修并实测） |

**结论**：平台「可部署形态」已真实跑通；**只有需要 root 的宿主前置阶段未被执行**。人工在可弃置宿主上执行 `sudo bash install.sh --api-url … --ops-url … --version 1.0.0 && sudo systemctl start floatctf.target` 即可闭环。

---

## 6. Normal User / sudo Install

**BLOCKED（外部原因，非安装器缺陷）**：本机 agent shell 设了 `NoNewPrivs`，`sudo -n true` 返回
`sudo: The "no new privileges" flag is set, which prevents sudo from running as root.`；
`unshare -r id` 亦失败（`cannot open /proc/self/uid_map: Permission denied`），无法构造伪 root。

按用户 §9「Do not weaken installer permissions to work around bad sudo config」，**未**为此放宽任何权限。已做的最强替代：

- 安装器的属主/权限契约逐条核对：`FLOATCTF_HOME` 0750 `root:floatctf`；`.env`/`floatctf.toml`/`Caddyfile` 0640 `root:floatctf`；`frontends` 0755 `root:root`、`registry.json` 0644；`data/postgres` 999:999；`data/rustfs`+`logs/rustfs` 10001:10001 且 `g+rX` + setgid；`runtime` 65532:floatctf；`backup.sh`/`restore.sh`/`frontend.sh` 0755 `root:root`；`uninstall.sh` 0750 `root:floatctf`。
- 在本机 RC 内**按同一契约**用一次性 root 容器设置属主（等价复现安装器的 root 步骤），并验证 API 容器（数值 `65532:<floatctf gid>`）确实能读到 0640 的配置 → 组权限模型自洽。
- `floatctf` 组只建组不建用户（`install.sh:274-293`）；本机 `floatctf:x:966:fb0sh`，`fb0sh` 在组内，helper socket（`srw-rw---- floatctf-helper:floatctf`）可访问（实测 `helper-control`/`helper-docker` 均 CONNECTED）。

**人工必须执行**：以普通用户 + sudo 在可弃置宿主跑一遍 §5 的安装命令，确认 §6 的属主/权限矩阵。
**发现的真实权限缺陷**：见 §22 F3（`registry.json` 模式漂移）。

---

## 7. HTTPS / Domain

**REAL-DOMAIN HTTPS: EXTERNAL BLOCKER**

无操作者提供的真实域名/DNS/TLS 环境，**未**伪造域名、**未**关闭 TLS 校验、**未**以 `curl -k` 充当验收证据。所做的最强本地生产形态 TLS 测试（真实 Caddy 容器 + 真实内嵌 Caddyfile + 真实产物）：

| 检查 | 结果 |
|---|---|
| 证书链校验（`curl --cacert <Caddy 内置 CA root.crt>`，**无 -k**） | `tls=0`（验证通过）；`openssl s_client` → `Verify return code: 0 (ok)`；issuer=`Caddy Local Authority - ECC Intermediate` |
| HTTP → HTTPS | `GET http://127.0.0.1:18080/` → **308** → `https://localhost/` |
| `/` | 200（bootstrap HTML） |
| `/api/*` | `/api/frontend` → 200，返回 `{active_frontend:"default", platform_version:"1.0.0", api_contract_version:"1", frontend_runtime_version:"1", capabilities:[…]}` |
| 注册表 no-store | `/__floatctf/frontends/registry.json` → 200 + `cache-control: no-store` |
| 版本化资产 immutable | `/__floatctf/frontends/default/1.0.0/frontend.json` → 200 + `cache-control: public, max-age=31536000, immutable`, `x-content-type-options: nosniff` |
| 深层路由刷新（SPA） | `/admin/settings`、`/service/events`、`/admin/challenges` 均 200 `text/html` |
| 外部前端 | 真实安装 `rc13-ext@2.0.0` 并经 HTTPS 访问（§11） |
| `?frontend=default` | 破窗路径见 §11 |
| 恢复后的 S3 对象 | `/public/rc13-drill-sentinel.txt` → 200（真实对象经 Caddy 代理） |
| 浏览器混合内容/CORS | 未做浏览器会话（无真实域名）；CORS 见 §27 替代检查 |

**留给人工的确切清单**：① 把真实域名 DNS 指向宿主；② `SITE_ADDRESS` 设为该域名；③ `docker compose up -d` 后确认 Let's Encrypt 证书签发（`data/caddy` 内 ACME 状态）；④ 浏览器确认无 mixed-content/CORS 报错；⑤ 复用本报告 §7 的同一组 URL 断言。

---

## 8. Jeopardy E2E

**BLOCKED（环境隔离，非代码缺陷）**

Jeopardy 的完整生命周期需要「启动实例」——即通过 helper 创建宿主 Docker 容器。而按 §1.2 的结论，本机不能安全运行第二个拥有 helper 的 API（会破坏生产全局 AWD/AWDP 状态），因此 **G3 的实例启动/提交计分链路未在本机执行**。

已真实取证的 Jeopardy 相关部分（在隔离 RC 上，不经过 helper）：

| 项 | 证据 |
|---|---|
| 赛事/题库合同与鉴权路由存在且挂载 | 路由契约目录（`/api/admin/events*`、`/api/submit/flag`、`/api/events/{id}/scoreboard|trend|join|leave`、`/api/instances*`、`/api/solves*`）与仓库既有 E2E 脚本一致 |
| 平台可服务玩家/管理端流量 | `/api/users/me` → 401（未认证正确拒绝）；`/api/frontend` → 200 |
| 前端引导与登录入口 | bootstrap 200、深层路由刷新 200（§7） |
| 窗口守卫/发布态/脱敏 | 未在本轮重跑；Phase 12 前的多轮真实 E2E（HANDOFF §11-§13）已覆盖，且相关修复（hidden 拦截、flag 脱敏、solved_count 口径）已在 `main` 上 |

**人工必须执行**：在**不含生产实例**的可弃置宿主上跑仓库自带 `scripts/test-jeopardy-http-e2e.sh`（该脚本自建隔离 DB/Redis/API，覆盖 practice/competition individual/team 三种模式）与 `scripts/test-jeopardy-modes-e2e.sh`。

---

## 9. AWD Complete E2E

**BLOCKED — 明确的生产保护决定（含实测风险证据）**

**根因（代码取证）**：
- `apps/api/src/modules/event/awd/infrastructure/firewall/render.rs:14` `pub const TABLE_NAME: &str = "floatctf_awd";` → **所有 AWD 事件共用一张全局 nft 表**；`nftables.rs:171,208` 先 `delete table inet floatctf_awd` 再按当前 DB 视图重建。
- `apps/api/src/modules/event/awdp/domain/judge.rs:8,25` `PRACTICE_NETWORK_NAME="fctf-awdp-practice"`、`PRACTICE_JUDGE_CONTAINER_NAME="fctf-awdp-practice-judge"` → 练习资源用**固定名**。
- `apps/api/src/modules/event/awdp/service/practice_judge.rs::ensure_practice_environment` **无任何前置条件**（不检查 DB 是否有练习事件），每 30s 幂等 ensure。

**实测（诚实记录）**：本轮最初尝试让 RC API 与生产共存，结果在 `2026-10-07T10:38:30Z` **生产的 `fctf-awdp-practice-judge` 容器与 `fctf-awdp-practice` 网络被重建**（约 23s 窗口）。生产的 `awdp.practice.judge` cron 随后**完全自愈**：镜像回到 `floatctf/infra/awdp-judgeserver:0.3.3`、子网回到 `10.42.2.0/23`、judge IP 回到 `10.42.2.2`、judge env `PLATFORM_INTERNAL_URL=http://10.42.8.2:9090`、nft 表 `floatctf_awdp_practice` 已按新桥接口名重写且**保留 conntrack 豁免**；生产 5 容器 healthy、API restarts=0、站点 308/401 正常。**无数据丢失**。

**根因定位（我第一轮隔离措施为何失效）**：`core/system_ids.rs` 中 `system.practice.check` 是**固定主键**（`Uuid::from_u128(0)`）的 `startup` 任务，`scheduler/engine.rs::seed_startup_tasks` 按**主键**查找；我用 `gen_random_uuid()` 预插 `enabled=false` 对它无效（而对按 `task_key` 查找的 `awdp.practice.judge` 有效——日志证实只注册未执行）。

**第二轮纵深防御（L1–L5）后结果**：预插固定主键 + task_key 两组 `enabled=false`；把 RC 的 practice 期望值（镜像/子网/judge IP/内部 URL/控制网名/`internal_token_key`）**对齐生产**使 reconcile 成为 no-op；`[awd] network_runtime="noop"`；并做 5s 粒度的生产漂移监控。**实测 120s 内零漂移**，RC 日志明确出现 `[system.practice.check] task is disabled`。

**但纵深防御并非绝对（诚实补充）**：在后续更长的验证轮次中（`2026-10-07T11:02:30Z`，与本轮演练时间窗重合），生产的 `fctf-awdp-practice-judge` 容器**又被重建了一次**。事后核验生产状态正确：镜像回到 `floatctf/infra/awdp-judgeserver:0.3.3`、env 为生产的 `PLATFORM_INTERNAL_URL=http://10.42.8.2:9090`、`EVENT_ID=…0002`、`WORKER_ID=practice-judge-00000000`；练习网络 ID **未变**（`0ef4ab4a…`，子网 `10.42.2.0/23`，judge IP `10.42.2.2`）；`inet floatctf_awdp_practice` 的 conntrack 豁免规则仍在（2 条）；生产 5 容器全部 healthy、API `restarts=0`。
**结论（强化）**：无论怎样对齐配置，第二个拥有 helper 的 API 仍会**触碰**生产的固定名练习资源（重建容器 / 短暂中断）——因此「**同一宿主同一时刻只能有一个 FloatCTF API 操作 Docker + helper**」是硬性运维约束，而不是可调优的配置问题。

**即便如此仍判定 BLOCKED 的理由**：这一套「共存」依赖把生产的 `internal_token_key` 复制进 RC 配置，并依赖对 seed 内部实现的精确理解 —— 对**真实竞赛中的生产环境**这是不可接受的运维前提。AWD 宿主级 E2E（WireGuard + nftables + FlagServer/JudgeServer + 完整 34 项证据）**必须在独占宿主的可弃置环境**执行。

**人工必须执行**：在独占宿主上跑 `scripts/test-awd-business-e2e.sh`（baseline/hardening/reset 三场景）与 `scripts/test-awd-boundaries-e2e.sh`，并按用户 §14 的 34 项清单逐条取证。

---

## 10. AWDP E2E

**BLOCKED — 与 §9 同一根因**

- 练习/训练路径：固定网络名与固定 judge 容器名（`fctf-awdp-practice*`）+ 全局练习 nft 表 → 在共享宿主上与生产互斥。
- 竞赛路径：`[awdp].network_pool`（生产 `10.43.0.0/16`，本机已被生产桥占用 10.43.0–10.43.134）与每赛事判定网络 → 在共享宿主上必然与生产子网冲突。
- 本轮 RC 已把 `network_pool` 改为不重叠的 `10.44.0.0/16`、practice 子网改为 `10.42.30.0/23` 以证明**配置层面可隔离**，但**未**创建任何 AWDP 事件/run。

已真实取证：AWDP 相关镜像/合同/路由存在；`init rustfs failed` 崩溃循环的根因（§23 风险 R2）在本轮被实证。

**人工必须执行**：独占宿主上跑 `scripts/test-awdp-http-e2e.sh`（覆盖 practice/individual、competition/individual、competition/team 三种模式，含真实 JudgeServer 容器与真实 RustFS）。

---

## 11. Frontend Platform E2E

**PARTIAL PASS（文件系统/registry/Caddy 层全部真实通过；浏览器 + 业务数据层未做）**

从**已安装的** `$FLOATCTF_HOME/frontend.sh` 出发（生产运维真实路径，以 root 运行）：

| 项 | 结果/证据 |
|---|---|
| 平台内置前端安装 | `frontend.sh install <web-dist>/frontends/default/1.0.0 --platform --make-current` → `default@1.0.0`，manifest 校验（runtime 1 / api 1）通过 |
| 已安装版本化资产 | `frontends/default/1.0.0/{frontend.json,assets/frontend.js,assets/frontend.css}` 就位；经 HTTPS 返回 immutable 头 |
| 外部（非 React、自带路由）前端安装 | 手写最小合规制品 `rc13-ext@2.0.0`（`frontend.json` + `assets/main.js` 导出 `mount(context)` + 自己的样式），`frontend.sh install <dir> --make-current` 成功 |
| 公开注册表**不含私有源码元数据** | `registry.json` 仅含白名单字段（`id/protected/currentVersion/versions{author,compatibility,description,entry,installedAt,name,styles,version}`）；无仓库路径、无 source/build 字段 |
| 注册表 no-store / 资产 immutable | 见 §7（实测头） |
| FRONTEND_ACTIVE 切换 | `UPDATE settings SET value='rc13-ext'` → `/api/frontend` 反映 active（DB 设置即真相） |
| 破坏性恢复演练中前端状态存活 | 见 §14：`registry.json` sha256、`FRONTEND_ACTIVE`、外部前端 `rc13-ext@2.0.0` 全部在真实破坏+恢复后**逐字节一致** |
| 破窗 `?frontend=default` | **未做浏览器验证**：bootstrap 的 `?frontend=` 覆写逻辑在 `packages/frontend-runtime/src/bootstrap.ts` 中（Phase 12 已实现并有用例），但本阶段无浏览器会话 |
| 完整浏览器加载外部前端 + 调用真实 API | **未做**（需浏览器；且外部前端 `main.js` 已包含调用 `/api/frontend` 的逻辑，未在浏览器中执行） |
| 切换到 Default 回退 | `frontend.sh` 层已覆盖；浏览器层未做 |
| 前端版本不可变 | ✅ 见 §17 |

**人工必须执行**：浏览器打开 `https://<site>/?frontend=<id>`、观察外部前端挂载并调用真实 API、观察控制台无错、再 `?frontend=default` 破窗并登录管理端恢复 `FRONTEND_ACTIVE=default`。

---

## 12. Upgrade / Redeploy

**PARTIAL PASS（迁移执行器真实跑通；完整平台级升级未端到端跑）**

| 项 | 结果 |
|---|---|
| 升级路径此前是否存在 | ❌ **不存在**：`merged.sql` 只在 PostgreSQL **首次** initdb 时被消费；API 启动不跑迁移；release 也不含 `migrations/` → 既有库无法在**无源码签出**情况下前进。**已作为 GA 阻塞修复（§22 F2）** |
| 修复后的升级路径 | `ops-tools.tar.gz` → `$FLOATCTF_HOME/db/{migrate.sh,migrations/,merged.sql}`；`install.sh` 新增 `apply_migrations`：检测 `data/postgres/PG_VERSION` 存在即「既有集群」→ 仅启 postgres → 等 healthy → 用 **0600 临时 TOML**（`postgres://…@127.0.0.1:<PGPORT>/<DB>`，`migrate.sh` 只从 `FLOATCTF_CONFIG` 读 URL）执行 `migrate.sh apply` → 删除临时 TOML（含失败路径，实测无口令残留）；fresh 集群则跳过（交给 `merged.sql`） |
| 迁移执行器实测 | 在一次性 compose 集群上真实验证：fresh 分支跳过且不建容器；既有集群分支 **Applied 48/48**、已应用跳过、容器保留运行、临时配置删除；错口令分支**清晰失败且平台不启动**、无口令残留 |
| forward-only / 锁 / 不改历史 | `migrate.sh` 自带 advisory lock（`ADVISORY_LOCK_ID=1179204948`）、每迁移同事务写 `schema_migrations`、checksum 校验；本轮**未新增也未修改任何迁移**（`apps/api/src/sql/migrations/` 48 个文件字节未变） |
| 幂等 | `migrate.sh apply` 按 version 跳过已应用；`mise run db:gen` 幂等；`merged.sql` 确定性 |
| 完整平台级「装 RC-N → 装 RC-N+1」 | **未执行**（需 root 的完整安装路径） |
| 持久化状态保留 | Caddy 证书/ACME 状态、`config/`、`.env`、`frontends/`、`runtime/`、PG、RustFS 在**备份/恢复演练**中已被完整保留并逐项核验（§14）——同一组状态正是升级需要保留的状态 |

---

## 13. Safe Uninstall / Reinstall

**BLOCKED（需 root）+ 新增守卫已实现并隔离验证**

- 生成态 `uninstall.sh` 需要 root（`require_root`）；本机无法执行 → 未真实跑 `safe_uninstall`。
- **新增并验证的守卫**（§22 F4）：`safe_uninstall` 在 `stop_api_first` **之前**调用 `active_runtime_guard()`：
  - 活跃 AWD = `awd_events.status NOT IN ('draft','configuring','finished','archived','deploy_failed','verification_failed')`
  - 活跃 AWDP = `awdp_runs.finished_at IS NULL`
  - 命中则打印计数与 id、**非零退出并提示先收尾或 `--force`**；DB 不可查（平台已停/无 compose）→ 打印 warn 后放行（不阻塞已停平台的卸载）。
  - 隔离验证：活跃→rc=1（打印计数+id）；全部 finished→rc=0；postgres 停→warn+rc=0；无 compose 文件→warn+rc=0。
- 语义审计：`safe_uninstall` 会删除全局 `floatctf_awd` nft 表、全部 `fawg_*` 接口、全部 `fctf-awd-*`/`fctf-awdp-*` 网络与相关容器，同时**保留数据库** → 若事件仍在进行，正是「运行时被销毁而 DB 仍标记 running」的不一致态。守卫即为该缺陷的修复。
- purge 路径：`purge_run` 在活跃运行时打印醒目销毁警告（不阻塞，因为 purge 同时销毁 DB，不存在不一致）。

**人工必须执行**：可弃置宿主上开一个活跃 AWD 事件 → `sudo $FLOATCTF_HOME/uninstall.sh` 必须拒绝（rc≠0）；`--force` 才继续；随后重装并核对 sentinel 存活。

---

## 14. Backup / Restore

**PASS（真实破坏 + 真实恢复，15/15 哨兵逐项通过）**

这是本轮的核心交付。演练脚本 `var/rc13/drill-backup-restore.sh`，在隔离 RC 栈上执行；`backup.sh`/`restore.sh` **以 root 运行**（即文档契约 `sudo`）。

### 14.1 流程与证据

| 阶段 | 实测 |
|---|---|
| 0 前置 | 5 服务 healthy；ops 脚本按仓库**当前**版本装配 |
| 1 哨兵 | DB：`settings.RC13_DRILL_SENTINEL`；RustFS：**真实 S3 对象** `floatctf-public/rc13-drill-sentinel.txt`（SigV4 PUT/GET）；第三方前端：`rc13-ext@2.0.0` 经已安装的 `frontend.sh install --make-current`；口径：`schema_migrations=48`、`super_admin=1`、`.env`/`floatctf.toml`/Caddy root CA/`registry.json` 的 sha256 |
| 2 备份 | `backup.sh --out … --quiet` → 2.7M，**权限 `600 root:root`**；`META`：`BACKUP_FORMAT=1`、`TOOL=floatctf-backup`、`PLATFORM_VERSION=rc13`、`POSTGRES_MAJOR=17`、`PG_DUMP_VERSION=pg_dump 17.11`、`MEMBER_COUNT=223`、`SUBSET=all`、`RUSTFS_QUIESCED=yes`、`ARCHIVE_PLAINTEXT=yes`；`MANIFEST` 逐成员 sha256；顶部成员 `env config data frontends web runtime merged.sql compose.prod.yml frontend.sh MANIFEST META` |
| 3 破坏 | `compose down` + 删 redis 卷 + 删 `.env`/`config/floatctf.toml`/`frontends`/`web`/`runtime/*`/`data/{postgres,rustfs,caddy,caddy-config}/*` → 校验 `.env` 与 `frontends` 确已消失 |
| 4 全新 base | 空 PGDATA 重启 postgres → `merged.sql` 一次跑完 48 migration；**哨兵为 `<absent>`**（证明破坏彻底） |
| 5 恢复 | `restore.sh <archive> --yes --force` → sha256 校验通过 → 结构校验（拒绝穿越/链接/设备/setuid）→ `META` 校验 → **MANIFEST 223 成员全通过** → 文件层全部恢复（含属主/权限按安装契约）→ RustFS 数据 → Redis 持久化 → **PostgreSQL `pg_restore`（schema_migrations=48）** → 启动 + 健康检查「全部服务 healthy」 |

### 14.2 哨兵核对（恢复后）

| 哨兵 | 期望 | 实际 |
|---|---|---|
| DB 行 `RC13_DRILL_SENTINEL` | `RC13-DRILL-20261007T105810Z` | ✅ 一致 |
| `schema_migrations` 行数 | 48 | ✅ 48 |
| `super_admin` 行数 | 1 | ✅ 1 |
| `FRONTEND_ACTIVE` | `rc13-ext` | ✅ 一致 |
| `frontends/registry.json` sha256 | `e330492b…` | ✅ 一致 |
| `.env` sha256 | `b9c04dba…` | ✅ 一致 |
| `config/floatctf.toml` sha256 | `cb9b6d74…` | ✅ 一致 |
| **RustFS S3 对象** | `RC13-DRILL-…` | ✅ 一致 |
| Caddy 根 CA sha256 | `c2c60c96…` | ✅ 一致（证书/ACME 状态随归档恢复） |
| 外部前端 `rc13-ext` currentVersion | 2.0.0 | ✅ 一致 |
| 站点 `/`（HTTPS，`--cacert`） | 200 | ✅ 200 |
| `/api/frontend` | 200 | ✅ 200 |
| 注册表 `cache-control` | `no-store` | ✅ `no-store` |
| 恢复后 S3 对象经 Caddy `/public` | 200 | ✅ 200（`RC13-DRILL-…`） |

→ **DRILL_RESULT = PASS**（15/15）。

### 14.3 行为细节与安全属性（实测）

- **PostgreSQL**：使用 `pg_dump --format=custom --no-owner --no-acl`（容器内 pg_dump，版本必然与 server 一致），并断言归档以 `PGDMP` 魔数开头；恢复走 `pg_restore --no-owner --no-acl --exit-on-error`。**未**以裸数据目录复制作为数据库备份。
- **RustFS**：备份时 `compose stop rustfs` → 打包 `data/rustfs` → `start`（受控维护窗口，`RUSTFS_QUIESCED=yes`）；`--no-quiesce` 会明确警告「不保证一致性」。**未**声称崩溃一致。
- **确定性**：文件层与成员清单确定性；整包**不**字节确定（`pg_dump` custom 头部内嵌导出时间戳）——如实标注 `DETERMINISTIC_BACKUP=no(pg_dump header timestamp)`，不做虚报。
- **权限**：归档 `0600 root:root`；`.env` 在归档内 `0600`；日志从不打印密钥内容；报告只记录哈希。
- **明文声明**：`META` 内 `ARCHIVE_PLAINTEXT=yes`，且安装器/脚本帮助与文档均明确「明文、仅靠文件权限保护、请放受控介质」——**未**发明未正确实现的静态加密。
- **恢复安全**：默认要求显式意图（`--yes` 或交互输入 `RESTORE FLOATCTF`）；已存在安装时**默认拒绝覆盖**（需 `--force`）；先校验归档 sha256 与结构再解压；拒绝路径穿越/`..`/符号链接/硬链接/设备/FIFO/setuid；校验 `BACKUP_FORMAT` 受支持与 `TOOL` 名；PG 主版本不一致默认拒绝（`--allow-version-mismatch` 才放行）；只停本安装根的 compose project（**从不** `down -v`）；恢复后逐服务健康检查。
- **依赖**：GNU `tar`/`find`/`coreutils`（`--sort`/`--mtime`/`-printf`）——脚本已在入口做能力探测并对 BusyBox 明确报错。

### 14.4 本轮修掉的自身缺陷（诚实记录）

| 缺陷 | 现象 | 修复 |
|---|---|---|
| `restore.sh` 未传 `--env-file` | compose 无法解析 `${POSTGRES_PASSWORD:?}` → 恢复必然失败 | 引入 `RC_ENV`（`down` 阶段用归档内 `env/.env`，落盘后切回目标 `.env`），全部 compose 调用带 `--env-file` |
| `restore.sh` 非 root / 无组条目时静默跳过属主修正 | 配置回落 `root:root 0640` → API 容器读不到 → 平台起不来 | 新增 `FLOATCTF_GROUP_GID` 显式覆盖；无 GID 时**明确告警**而非静默 |
| `backup.sh` staging 清理会失败并让整包退非零 | 前端资产为 0555/0444 不可变树，非 root `rm -rf` 失败 → 归档已写好却报失败 | `drop_stage()` 先 `chmod -R u+w` 再删，且**永不失败** |
| `backup.sh` 复制失败信息无原因 | 仅「复制失败」 | 带出 `tar` 真实原因并提示用 `sudo` |
| 演练哨兵曾直接往 RustFS 数据目录写裸文件 | RustFS 把 `/data` 顶层当 bucket → bucket 元数据损坏 → 服务 `service error` | 改为**真实 S3 对象**（SigV4 PUT/GET），并据此发现 §23 R2 |

---

## 15. Purge / Fresh Reinstall

**BLOCKED（需 root）**

- 未执行 `uninstall.sh --purge`（需 root；且本机 purge 会删除全局 `floatctf_awd` nft 表等生产共享资源 → 按 §5 禁止）。
- 未执行破坏性 purge 前的宿主资源快照对比（有快照，但无 purge 可比）。
- **审计结论**（代码级）：`purge_run` 的清理是**所有权限定**的——nft 只删 `floatctf_awd` / `floatctf_awdp_*`（`cleanup_nftables` 对其它表明确 `warn 跳过`）；WG 只删 `^fawg_[0-9a-f]{8}$`（注释明确「无关接口 wg0 等未触碰」）；网络只删 `fctf-platform-control`/`fctf-awd-*`/`fctf-awdp-*` 前缀；systemd 只动 `floatctf.target`/`floatctf-infra.service`/`floatctf-helper.service`（+ 历史 `floatctf-api.service` 兼容清理）；sysctl/modules 只删自己的两个文件；用户只删 `floatctf`/`floatctf-helper`。**未见** `docker system/network/volume prune`、`nft flush ruleset`、全局 sysctl 归零、卸载 Docker/nft/WG/systemd/iproute。
- 本机已存在大量**非本阶段产生**的 FloatCTF 历史残留（248 容器 / 144 网络，实测快照 `var/rc13/snapshot/`），本轮**未清理**它们（按 §5「不得触碰无关资源」，它们属于生产/历史，不属于本次 RC 环境）。

**人工必须执行**：可弃置宿主上 `sudo $FLOATCTF_HOME/uninstall.sh --purge`（走交互确认），purge 前后对比 `docker ps -a`/`network ls`/`volume ls`/`ip link`/`ip route`/`wg show`/`nft list tables`/`systemctl --failed`，确认无关资源不变，随后全新安装并确认 healthy。

---

## 16. SDK Package Distribution

**PASS**

| 项 | 结果 |
|---|---|
| 三包审计（version/license/exports/types/files/peerDeps/publishConfig） | 全部 `1.0.0`、`AGPL-3.0-only`、`sideEffects:false`、`main`/`types` 指向存在的 `dist`、`files:["dist","README.md"]`、`publishConfig.access=public`。**未发现真实分发缺陷** |
| `pnpm pack` 是否重写 `workspace:*` | ✅ **是**：`@floatctf/react` 的 tarball 内 `dependencies["@floatctf/sdk"] = "1.0.0"`（精确版本）；三个 tarball 中 `workspace:` 出现 0 次 |
| tarball 内容 | 无 `src/`、无测试、无 `node_modules`、无 `.env`/密钥、无仓库绝对路径；README + LICENSE 均在包内 |
| 确定性 | 两次 pack **字节相同**（gzip mtime=0、成员顺序固定） |
| 外部全新工程消费 | ✅ `scripts/test-sdk-dist.sh` → **51/51**：三个 `@floatctf/*` 全部从**绝对 .tgz 路径**安装；lockfile 记录 `resolved: file:…tgz`、**无 `link`**；`grep workspace:` = 0；`node_modules/@floatctf` 下**无任何符号链接**且 realpath 全在消费者工程内（无一指向 FloatCTF 源码树）；`dist/index.js` sha256 与 tarball 成员一致；`tsc --noEmit` 通过；真实 `node` import + API 断言通过；`node --experimental-strip-types` 通过 |
| 离线路径 | ✅ registry 不可达时打印 `[SKIP] 公共 registry 不可达 …` 并仍完成 33 项 tarball 校验、退出 0 |
| 确定性命名 | `floatctf-sdk-<V>.tgz` / `floatctf-react-<V>.tgz` / `floatctf-frontend-runtime-<V>.tgz`；`SDK-SHA256SUMS` 随附并 `sha256sum -c` 通过 |
| 附加到 GitHub Release | ✅ `release.yml` 上传三个 `.tgz` + `SHA256SUMS`（覆盖它们） |
| npm 发布 | **未执行**（用户禁止）。`npm publish`/`pnpm publish` 未调用；不需要 `NPM_TOKEN` 即可通过 RC |
| ⚠️ 实测限制（如实记录） | 三个**未发布**的互依赖 tarball 用 **pnpm** 一起安装会 `ERR_PNPM_FETCH_404`（对 `@floatctf/sdk`），`pnpm.overrides`（`file:`/`link:`）亦无效；用两个一次性 `@faketest/*` 包复现 → 属 **pnpm 对未发布互依赖本地 tarball 的固有限制**，非包缺陷（**npm 可正确解析**，已实测）。已在三个 README 中说明：pnpm 消费者需要 registry 发布 |

---

## 17. Release Workflow Safety

**PASS（含安全语义修复）**

**修复前的风险**：`release.yml` = `workflow_dispatch` + 无条件 `softprops/action-gh-release` → 对**非 tag ref 手工 dispatch 会创建 GitHub Release**。

**修复后的语义（实测 8 种组合全部符合预期）**：

| 事件 / ref | inputs | 结果 |
|---|---|---|
| push tag `v1.0.0` | — | 构建 + 校验 + **发布 Release** |
| dispatch on `v*` tag | `publish=false`（默认） | 构建 + 校验 + **仅上传 workflow artifacts** |
| dispatch on `v*` tag | `publish=true` | 发布 Release（tag 守卫三重） |
| dispatch on branch | 无 `version` | **fail closed**（`无法确定平台版本 VERSION`） |
| dispatch on branch | `version=1.0.0` | 仅 workflow artifacts，**不发布** |
| dispatch on branch | `publish=true` | **fail closed**（`publish=true 只允许在 v* tag ref`） |
| dispatch on 非 v tag（`api-v1.0.0`） | `publish=true` | **fail closed** |
| 任意 | `version=latest` | **fail closed** |

- `publish` 输入 `default: false`；发布动作只存在于 `publish` job，且该 job `needs: build` + `if` 要求 `publish == 'true'`，另有步骤级 `github.ref_type == 'tag' && startsWith(github.ref_name,'v')` 与 publish job 内**重新断言** tag 守卫 → 三重独立 tag 门。
- 权限最小化：workflow + `build` job = `contents: read`；只有 `publish` job = `contents: write`。
- `SHA256SUMS` 覆盖除自身外全部制品、`LC_ALL=C` 按名排序、写完立即 `sha256sum -c` 自校验。
- 新增**非发布 RC 工作流** `.github/workflows/rc.yml`：仅 `workflow_dispatch`、`contents: read`、构建全量制品 + 跑三个校验脚本 + 上传 `floatctf-rc-<V>`；静态断言确认**无** `action-gh-release`、**无** `npm/pnpm publish`、**无** tag/push 触发、**无** `contents: write`。与 `release.yml` 共用 `scripts/release-checksums.sh`（不重复大段逻辑）。
- 静态/行为测试：`scripts/test-release-workflow.sh` → **84/84**；Release 制品清单 11 项逐项断言（三个 tgz 以 `floatctf-sdk-${{ needs.build.outputs.version }}.tgz` 形式断言，证明版本来自守卫而非硬编码）。
- 本阶段**未**触发任何发布动作。

> 未在本机验证（需真实 GitHub Actions）：`inputs.publish` 在 `push` 事件下的解析、步骤 `if` 中的 `github.ref_type/ref_name`、`upload-artifact@v4`→`download-artifact@v4` 的名称往返、softprops 对 `files:` 块中标量表达式的插值、GH 自身 YAML 解析。已用 `uv run --with pyyaml` 确认两个 workflow 可被 YAML 解析。

---

## 18. Security Revalidation

| 面 | 检查 | 结果 |
|---|---|---|
| HOST | API 无 `docker.sock` | ✅ 容器仅挂 `config/floatctf.toml:ro`、`runtime`、`/run/floatctf:ro` |
| HOST | API 无 `CAP_NET_ADMIN` | ✅ `cap_drop: [ALL]` + `no-new-privileges:true` + `read_only:true` + `tmpfs /tmp:noexec,nosuid,nodev` |
| HOST | helper 协议受限 | ✅ 仅两个 Unix socket；`/run/floatctf` 0750 `floatctf-helper:floatctf`，socket `srw-rw----`；API 以数值 gid 获得访问 |
| HOST | 无特权容器回归 | ✅ 无 `privileged`/`--net=host`/host PID |
| NETWORK | FloatCTF nft 作用域 | ✅ 代码层：AWD 只操作 `floatctf_awd`；purge 对非 FloatCTF 表 `warn 跳过`；**未**出现 `nft flush ruleset`（CI 安全 grep 亦断言） |
| NETWORK | WireGuard 所有权隔离 | ✅ 只删 `^fawg_[0-9a-f]{8}$`；`wg0` 等未触碰 |
| NETWORK | Docker 网络所有权 | ✅ 仅 `fctf-platform-control` / `fctf-awd-*` / `fctf-awdp-*` |
| NETWORK | AWD 的 Internet 拒绝规则 | ⚠️ 未在宿主级复验（§9 BLOCKED）；渲染层回归用例存在（`render.rs` 断言 own-subnet 无 accept、跨队 drop） |
| FRONTEND | 无任意远程 JS import | ✅ bootstrap 只加载同源 `/__floatctf/frontends/<id>/<version>/...`；manifest 校验 entry 为相对路径 |
| FRONTEND | 注册表仅公开字段 | ✅ 实测 `registry.json` 仅白名单字段；`sanitize_registry` + fail-closed 未知键校验 |
| FRONTEND | manifest 严格 / 归档穿越拒绝 | ✅ 管理器负向测试覆盖（拒绝绝对路径/`..`/符号链接/特殊文件）72/72 |
| FRONTEND | 构建容器非 root / 无 docker.sock / 非特权 | ✅（Phase 12.1 已实现并测试：`resolve_build_identity` 永不为 0，非 root 容器构建） |
| FRONTEND | Default 受保护 + 版本不可变 | ✅ `--platform` 才允许动 `default`；同版本不同字节**拒绝**（§17 及管理器测试） |
| AUTH | 用户/管理员 token 作用域 | ✅ `/api/users/me` 未认证 401；管理端路由走 `SuperAdminJwtGuard` |
| AUTH | 无 URL token 泄漏 | ✅ SSE 走 `Authorization: Bearer`，不把 token 放 URL（Phase 12.1 修复） |
| AUTH | 公开 bootstrap 不泄漏密钥设置 | ✅ `/api/frontend` 只返回 `active_frontend`/版本号/`capabilities`（实测响应体已取证） |
| SECRETS | JWT / `awd_root_key` / `internal_token_key` / DB / RustFS | ✅ 均在 `.env`+TOML，模式 0640 `root:floatctf`；Debug 输出 `Secret(***)`（实测启动日志） |
| SECRETS | 备份权限 | ✅ 归档 `0600 root:root`（实测）；`META` 声明明文 |
| RELEASE | 制品无密钥 / 日志无密钥 / workflow 无 token | ✅ `web-dist` 无 `.env`/`src`；备份/恢复日志只打印哈希；workflow 无内嵌凭据、权限最小化 |
| RELEASE | 镜像 label | ✅ `org.opencontainers.image.version` = 构建 ARG；`io.floatctf.managed=true` |

---

## 19. Restart / Recovery

**PARTIAL PASS（容器级真实；宿主 reboot 未做）**

| 操作 | 结果 |
|---|---|
| 重建整套 RC 栈（`docker compose down` + 清空数据 + 重新 initdb/initbuckets + `up -d`） | ✅ 5 服务 healthy，0 restarts（多次执行） |
| 恢复演练中的「停全部 → 恢复 → 启全部」 | ✅ 全部 healthy（§14） |
| API 容器重启 | ✅ 健康；重启过程中 `restarts` 计数归零后稳定 |
| 备份期间 RustFS 停/启 | ✅ `stop` → 打包 → `start`，随后 healthy；异常路径有 trap 还原 |
| **宿主 reboot** | **未执行**（会中断本机正在运行的生产实例与其它服务）→ 不得声称通过 |

**取证到的真实恢复缺陷（§23 R2）**：API 启动时 `init rustfs failed: service error` → **panic（致命）**，进入 crash-loop，直到 RustFS 的 S3 层真正可用；根因是 compose 的 rustfs healthcheck 只做 TCP 探测（`nc -z 127.0.0.1 9000`），端口打开 ≠ S3 就绪。`restart: unless-stopped` 最终自愈，但会产生一段失败重启窗口。

---

## 20. Logs / Diagnostics Review

在 RC 生产形态栈上采集并审阅（`docker logs`）：

| 类别 | 观察 |
|---|---|
| panic / stack trace | 仅 **RustFS 初始化竞态**导致（§23 R2），非稳态下不出现 |
| 5xx | 未见 |
| SQL 错误 | 未见；`seed_default_settings` 正常 |
| 迁移警告 | 无（fresh 走 `merged.sql`，未触发 `apply_migrations` 路径） |
| helper 权限错误 | 未出现（Docker/RustFS 经 helper 正常；`Rustfs connected OK`） |
| Caddy 路由错误 | 未出现；4 类路由（`/`、`/api/*`、注册表、版本化资产、深层路由）均 200/308 |
| 凭据/令牌泄漏 | 启动日志 `database_url="Secret(***)"`、`redis_url="Secret(***)"`、`jwt_secret="Secret(***)"` → **无泄漏** |
| 重复重连风暴 | 未见 |
| nft/WG 清理失败 | 本轮未触发 AWD/AWDP 清理路径 |
| 预期负向错误的区分 | `GET /api/users/me` 的 401 `unauthorized` 属**健康检查的预期负向**（healthcheck 只看 curl 退出码），已在报告中区分，不计为缺陷 |
| 生产实例日志（只读，未改动） | 生产 API 健康、restarts=0；无新增 5xx |

---

## 21. Documentation Audit

| 文档 | 处理 |
|---|---|
| `RELEASE.md`（新增） | 人工发布清单（pre-release / tagging / release / post-release / rollback / do-not），明确标注本阶段未执行任何发布动作 |
| `chore/v1.0.0-release-notes.md`（新增，草稿） | 按实际交付范围撰写发行说明草稿 + 诚实的 Known limitations；不含未经验证的市场宣传 |
| `INSTALL.md` | 修正：产物数量与清单（补 `frontend.sh`/`install.sh`/`ops-tools.tar.gz`/SDK tgz）、新 flag（`--ops-url`/`--skip-migrations`）、安装后布局（补 `backup.sh`/`restore.sh`/`db/`）、卸载章节补**活跃运行时拒绝**与 `--force`、新增升级子节与备份/恢复指引 |
| `README.md` | 修正：产物数量与清单陈旧（补缺失项） |
| `packages/{sdk,react,frontend-runtime}/README.md` | 新增「在 monorepo 之外消费（v1.0）」章节：从 release tarball 安装、react 的 peer deps、pnpm 限制、`skipLibCheck` 注意事项、明确「v1.0 未做 npm 发布」 |
| 陈旧引用审计（全量） | **已确认无** `nginx`、无 `/home/floatctf`、无「apps/web 是完整 UI」、无旧 TS 实体路径、无 `--reinstall`、无 `floatctf-dev-infra.service`/A-B 双轨、无「API 有 docker.sock/CAP_NET_ADMIN」、无「`mise run dev:api` 非 watch」、无「fresh clone 必须先 merged.sql」 |
| 已修正的真实文档漂移（审计发现） | `README.md` 4 产物→正确清单；`README.md` 仓库树把 `apps/web` 描述为 React 前端（实为 bootstrap，且缺 `frontends/`/`packages/`）；`docs/agents/DATABASE.md` 关于生产 compose 不挂 `merged.sql` 的错误断言；`docs/agents/FIX-BUG.md` 开发 Web 端口 3000→13000；`docs/agents/ARCHITECTURE.md` dev compose 服务清单（补 redis、去 registry）；`docs/deployment/portability.md` 四条 pre-containerization 描述（API 权限模型 / Caddy 网络模式 / AWD 网络能力归属 / 生产 API 端口）；`DEVELOPMENT.md` 创建 `floatctf` 用户（实为只建组）、生产外部入口、API UID 表述；`INSTALL.md` `id floatctf`（无该用户） |
| §31 要求的陈旧引用终检 | `nginx` **0 命中**；`/home/floatctf` **0 命中**；`--reinstall` 仅剩 `docs/frontend/DEVELOPING.md` 的**否定句**（「没有 `--reinstall` 这类例外」）；`floatctf-dev-infra` 仅剩 `AGENTS.md` 的**否定句**（「不存在 A/B 双轨与 `floatctf-dev-infra.service`」）；`floatctf-api.service` 仅剩 4 处**明确的「已移除/不要用」**说明（`INSTALL.md`/`AGENTS.md`/`docs/agents/ARCHITECTURE.md`/`DEVELOPMENT.md`）；无「apps/web 是 React 前端」 |
| 仍未处理（如实标注） | ① `apps/api/tests/README.md` 默认端口 8080 vs dev 9090；② `infra/config/floatctf.prod.toml` 参考副本缺两个派生根；③ `chore/pluggable-frontend-implementation-report.md` 仍描述已移除的 `--reinstall`（历史报告，位于 gitignore 的 `chore/`，本阶段仅以 `-f` 单独收录 Phase 13 报告与发行说明草稿）；④ `docs/frontend/ARCHITECTURE.md` 小节编号（9 下的 5.x）；⑤ `apps/api/README.md` 路由清单结构性陈旧；⑥ `infra/caddy/Caddyfile.prod` 是参考副本且缺 `/__floatctf/frontends/*` 两段路由（权威模板在 `install.sh` 内嵌；已在本报告与 `install.sh` 注释中点明，未改副本） |

---

## 22. Release Blockers Fixed

每一项回答「这解除了哪个发布闸门」。

### F1 — 【GA 阻塞】生产 Caddy 因嵌套只读挂载**根本无法启动**
- **问题**：`install.sh` 的生产 compose 把 `web/` 与 `frontends/` 挂在 `${FLOATCTF_HOME}/runtime:/srv:ro` **内部**（`/srv/web`、`/srv/frontends`）。Docker 需先在容器内创建挂载点目录，而 `/srv` 已只读。
- **根因**：挂载点必须位于可写（尚未被只读挂载覆盖）的路径；嵌套在只读挂载之下必然失败。
- **证据（实测复现）**：`create mountpoint for /srv/frontends mount: create next directory component failed: mkdirat(… "srv" … "frontends" …): Read-only file system (os error 30)`。修后同样命令下 Caddy 立即 healthy。CI 只 build API 镜像、从不起生产栈 → 该路径此前**从未被执行**，所以逃过了所有门禁。
- **修复**：两个只读静态根移到 `/srv` 之外的兄弟路径 `/srv-web`、`/srv-frontends`，Caddyfile 的 `root` 同步（2 处 frontends + 1 处 web）；并加注释禁止改回嵌套。
- **回归**：`scripts/test-installer-contract.sh`（50 项）断言 compose 挂载与 Caddyfile root 的一致性；RC 栈实测 Caddy healthy + 4 类路由 200/308。
- **解除闸门**：G1 全新安装（否则站点整体不可用）。

### F2 — 【GA 阻塞】既有安装没有可用的升级（迁移）路径
- **问题**：`merged.sql` 仅在 PostgreSQL **首次** initdb 被消费；API 启动不跑迁移；release 不含 `migrations/` 与 `migrate.sh`。→ 无源码签出时**无法**把既有库前进到新版本。
- **根因**：安装器只实现了 fresh bootstrap，升级被留成 TODO（`install.sh` 原文「只做全新安装；已有数据的升级后续单独实现」）。
- **修复**：新增 release 制品 `ops-tools.tar.gz`（`backup.sh` + `restore.sh` + `db/migrate.sh` + 48 个 migrations，白名单断言 + 确定性归档）；安装器新增 `--ops-url` / `--only-ops` 安装到 `$FLOATCTF_HOME/db/`；新增 `apply_migrations`：检测 `data/postgres/PG_VERSION` → 既有集群则仅启 postgres、等 healthy、用 0600 临时 TOML（host 可达 URL）跑 `migrate.sh apply`、无论成败删除临时文件；fresh 集群则跳过。`--skip-migrations` / `FLOATCTF_SKIP_MIGRATIONS=1` 可绕过。
- **证据**：真实 `--ops-tools` 归档经安装器 `download→validate→install` 全链路验证（48 migrations、`migrate.sh list` 从安装位置可用）；在一次性集群上真实跑通 fresh 跳过 / 既有集群 Applied 48/48 / 错口令清晰失败且**无口令残留**。
- **回归**：`test-installer-contract.sh` 断言 `apply_migrations` 存在且在无 `PG_VERSION` 时跳过；`test-release-workflow.sh` 断言 `ops-tools.tar.gz` 在发布清单内。
- **解除闸门**：G7 升级 / G18 平台升级与迁移运行器。

### F3 — 【中】`frontend.sh` 每次运维写注册表都把模式退回 0600
- **问题**：`install.sh` 明确把 `frontends/registry.json` 设为 **0644**（公开静态树契约），但 `frontend.sh` 的 `atomic_write` 用 `tempfile.mkstemp`（0600）+ `os.replace` → 之后**每次** `install`/`remove`/`set-current` 都退回 0600。
- **影响（实测）**：非 root 的 `backup.sh` 因读不到 `registry.json` 而**整体失败**（`tar: ./registry.json: Cannot open: Permission denied`）→ 与「公共静态树」契约及备份可运维性冲突。
- **修复**：`atomic_write` 在 `os.replace` 前 `os.chmod(tmp, 0o644)`。
- **回归**：管理器契约/安全测试 72/72 保持绿；`backup.sh` 在非 root 下不再因此失败（并新增错误原因透出）。
- **解除闸门**：G9 备份可运维性 / G16 文档一致性 / G1 安装契约一致性。

### F4 — 【中】安全卸载会在活跃赛事中途销毁运行时并留下不一致 DB
- **问题**：`safe_uninstall` 会删全局 `floatctf_awd` nft 表、全部 `fawg_*` 接口、全部 `fctf-awd-*`/`fctf-awd-*` 网络与相关容器，同时**刻意保留数据库** → 若赛事进行中，DB 仍标记 running 而运行时已消失。
- **修复**：新增 `active_runtime_guard()`（活跃 AWD = `awd_events.status NOT IN (6 个非活跃值)`；活跃 AWDP = `awdp_runs.finished_at IS NULL`），在 `stop_api_first` **之前**调用；命中即打印计数+id 并非零退出，提示先收尾或 `--force`；DB 不可查时 warn 放行；`purge_run` 打印醒目警告（purge 同时销毁 DB，无不一致）。
- **证据**：隔离验证 rc=1（活跃，打印计数+id）/ rc=0（全 finished）/ warn+0（postgres 停）/ warn+0（无 compose）。
- **回归**：`test-installer-contract.sh` 断言生成的 `uninstall.sh` 含 `active_runtime_guard`/`--force`/`awd_events`/`awdp_runs`，且**抽取内嵌脚本后 `bash -n` 通过**（这是 CI 此前唯一完全没有语法门禁的脚本）。
- **解除闸门**：G8 安全卸载 / G19 生命周期安全。

### F5 — 【中】release 从不发布 `install.sh` 本身
- **问题**：`README.md`/`INSTALL.md` 指示用户从 `releases/download/<tag>/install.sh` 下载安装器，但 `release.yml` 只上传 5 个具名制品，**从不包含 `install.sh`** → 文档指向 404，全新安装无入口。
- **修复**：上传清单加入 `install.sh`（并新增 `ops-tools.tar.gz`、`SHA256SUMS`、三个 SDK tgz）。
- **回归**：`test-release-workflow.sh` 逐项断言 11 个制品名出现在发布 `files:` 块。
- **解除闸门**：G1 全新安装 / G11 制品可安装性。

### F6 — 【低/安全】发布工作流手工 dispatch 可误发布
- 见 §17。**解除闸门**：G15 发布工作流安全。

### F7 — 【低】AWD/AWDP 运行镜像不在发布流程内 → 全新宿主 AWD/AWDP 部署必然失败
- **问题**：生产配置引用 `floatctf/awd-flagserver:<V>`、`floatctf/awd-judgeserver:<V>`、`floatctf/infra/awdp-judgeserver:<V>`，但 `release.yml` 不产出它们，`build-runtime-images.sh` 只打 `:latest`（标签与配置**不匹配**），`install.sh` 原文标为 TODO。
- **本轮处理（部分修复 + 明确留作 GA 阻塞）**：
  - 已修：`build-runtime-images.sh` 新增 `--tag <版本>`（+ 可重复 `--extra-tag`，env `FLOATCTF_RUNTIME_IMAGE_TAG`），同时保留 `:latest` 以兼容既有宿主；`check_image` 对带 tag 的镜像做 `ldd` 校验。这使「本地构建 → 标签与配置一致」成为可用路径。
  - 已修：`install.sh` 新增 `check_runtime_images()` 在 `validate_compose_config` 之后**fail-loud 警告**（不 `die`），列出缺失镜像与确切修复命令，并在安装收尾醒目重复一次 → 缺失镜像必须在**安装期**暴露，而不是在比赛进行中。
  - **仍为 GA 阻塞**：镜像的**分发渠道**（release 附 `docker save` 包 vs 镜像仓库）是产品/架构决策，按用户 §0「不要静默扩大范围」**未擅自实现**。
- **解除闸门**：部分解除 G1（安装期显式化）；AWD/AWDP 全新宿主可部署性仍是 GA 阻塞。

### F8 — 【低】备份脚本的确定性与健壮性缺陷
- 见 §14.4（4 项：`rm -rf` 清理失败致整包退非零、复制失败无原因、GNU 工具前置检查缺失、归档确定性过度声明）。**解除闸门**：G9 备份/恢复。

---

## 23. Remaining Risks

### GA blockers（代码/交付可控，必须在 GA 前闭环）
| # | 风险 | 证据 | 建议 |
|---|---|---|---|
| B1 | **AWD/AWDP 运行镜像无分发渠道** | `install.sh` 配置引用 `:<V>`；`release.yml` 不产出；`build-runtime-images.sh` 只 `:latest` | 二选一：release 附 `docker save` 的确定性镜像包（安装器 `docker load` + 重打 `:<V>`），或建立镜像仓库并让安装器拉取。属发布工程决定，需用户确认 |
| B2 | **AWD/AWDP 宿主级 E2E 未验证** | §9/§10；全局 `floatctf_awd` 表 + 固定练习名使共享宿主互斥 | 在**独占宿主**上跑 `test-awd-business-e2e.sh`、`test-awd-boundaries-e2e.sh`、`test-awdp-http-e2e.sh` 并逐条取证 |
| B3 | **特权安装/卸载/purge 未端到端执行** | §1.1/§5/§6/§13/§15 | 可弃置宿主上以普通用户 + sudo 跑完整 `install.sh` → 启停 → 升级 → 安全卸载（含活跃赛事拒绝）→ purge → 重装 |
| B4 | **真实域名 HTTPS 未验证** | §7 | 提供域名 DNS + 让 Caddy 签发真证书，复用 §7 断言 |

### EXTERNAL blockers（本阶段唯一被用户允许的「非代码」阻塞）
- B4（真实域名/DNS/TLS）。
- 本机 agent 沙箱无法提权（`NoNewPrivs`），导致 B3 无法由 agent 完成。

### 运维级真实发现（非代码缺陷，但会影响生产）
| # | 发现 | 证据 | 建议 |
|---|---|---|---|
| R1 | **同一宿主只能有一个 FloatCTF API 操作 Docker + helper** | `system.practice.check`（固定主键 startup 任务）与 `awdp.practice.judge`（按 task_key 的 30s cron）无条件 reconcile **固定名**资源；本轮**两次**实测到第二实例触碰生产练习资源：第一次重建 judge+网络（约 23s），第二次（配置已完全对齐后）仍重建了 judge 容器（网络与生产 API 未变）；两次生产均自愈且状态正确 | 生产文档中显式禁止在同一宿主运行第二套 API/开发栈；开发机与生产机分离。**注意**：`scheduled_tasks.enabled=false` 对**固定主键 startup 任务**无效（预插随机 UUID 不会抑制它），必须以主键屏蔽 |
| R2 | **RustFS healthcheck 过弱 → API 首个启动可能 crash-loop** | compose healthcheck 仅 `nc -z 127.0.0.1 9000`；实测 API 在 RustFS S3 层就绪前 `init rustfs failed: service error` → panic（`bootstrap/mod.rs:131`），`restart: unless-stopped` 最终自愈 | 把 rustfs healthcheck 换成真实的 S3 层探测（或给 `ensure_buckets` 加有界重试）。本轮**未**改 healthcheck（镜像内无 curl，改动无法在本机充分验证） |
| R3 | `apps/api/src/sql/migrate.sh` 需要宿主 `python3` **带 `tomllib`（≥3.11）**，而安装器只要求 `python3` | `migrate.sh` 用 `tomllib` 解析 `FLOATCTF_CONFIG` | 在 `install.sh` 增加 `python3 -c 'import tomllib'` 前置检查（Ubuntu 22.04 = 3.10 会在升级路径 ImportError 失败） |
| R4 | 宿主存在大量历史 FloatCTF 残留（248 容器 / 144 网络） | `var/rc13/snapshot/{docker-ps-a,docker-network}.txt` | 按所有权护栏（`io.floatctf.managed` 标签 + 库内 id 交集为空 + 保护 `fctf-awdp-practice*`/`fctf-platform-control`）分批清理；本轮**未**执行 |
| R5 | 默认前端 bundle ~4.3MB（gzip ~1.08MB） | 构建产物 | v1.1 优化候选（用户 §40 明确：非阻塞） |

### post-v1.0 non-blockers（用户 §41）
前端 Marketplace 与签名包、无刷新热切换前端、广泛 UI 代码分割、新赛制、大规模架构重写、任意插件生态、移动端 App、clippy 告警清理（~108 条，非门禁）、`packages/sdk` 的 `./api/*` 通配子路径补全（`admin`/`service` 目录）、`packages/react/dist/index.d.ts` 需要 `skipLibCheck: true` 的上游问题、SDK 源码 map 未带 `sourcesContent`。

---

## 24. Commits

`release/v1.0` 上的本地提交（**未 push**、未打 tag、未发布）。基线 `35c04b4`（= `origin/main` 的树）。

| # | commit | message | 内容 |
|---|---|---|---|
| 1 | `7773bda` | `feat(ops): 新增生产备份/恢复工作流（backup.sh + restore.sh）` | §14/§20；F8 |
| 2 | `9ddc063` | `fix(frontend): 注册表模式对齐公开静态树契约，并修备份可运维性` | F3 |
| 3 | `415db65` | `feat(release): 收紧发布语义、补齐制品清单与 SHA256SUMS，新增非发布 RC 工作流` | F5/F6；`release-checksums.sh`、`test-release-workflow.sh`、`rc.yml` |
| 4 | `184acdb` | `fix(deploy): 修 Caddy 无法启动的 GA 阻塞，补齐升级路径/卸载守卫/镜像前置检查` | **F1**/**F2**/F4/F7；`test-installer-contract.sh` |
| 5 | `1246c92` | `feat(sdk): 三个平台包的分发打包 + 外部消费真实验证` | §16 |
| 6 | `c750288` | `chore(ci): fast-web 接入发布工作流/安装器契约/SDK 分发三套新测试` | §30 |
| 7 | `68af931` | `docs(release): 新增 RELEASE.md 与 v1.0.0 发行说明草稿，并纠正文档漂移` | §21/§31/§32/§33 |
| 8 | 本报告提交 | `docs(rc): Phase 13 最终 RC 与发布验收报告` | 本文件 |

提交规范：中文 message + 前缀；按角度分批；**未**改写 Phase 12 历史；**未** push。
（`chore/` 在 `.gitignore` 中，故发行说明草稿与本报告用 `git add -f` 单独收录 —— 与 Phase 12 报告的先例一致；用户既有的 25 个 `chore/*.md` 删除与未跟踪的 `.agents/`、`PROJECT-ANALYSIS.html` **未被触碰、未被提交**。）

## 25. Final Release Checklist

（详细版见 `RELEASE.md`；此处为执行摘要，**本阶段全部未执行**）

- [ ] `main` CI 绿；本报告判定为可发布
- [ ] 工作树干净；版本一致性检查通过（平台 + 全部 JS 包 + 默认前端 = 目标版本）
- [ ] GA 阻塞 B1–B4 全部闭环（尤其 B1 镜像分发渠道、B2 AWD/AWDP 独占宿主 E2E、B3 特权安装路径）
- [ ] 发行说明定稿；`RELEASE.md` 复核
- [ ] 打 tag `v1.0.0`（annotated/signed，按项目策略）→ 触发 `release.yml`
- [ ] 核对 11 个制品 + `SHA256SUMS` 校验通过 + Release 页面内容正确
- [ ] 用 GitHub Release URL 做全新安装冒烟（**不依赖源码签出**）
- [ ] SDK tarball 消费冒烟；按需（可选）npm 发布
- [ ] 监控首次真实安装
- [ ] 回滚预案就绪（前一 tag / 前一安装器版本 / `frontend.sh set-current` 回退前端 / 数据库 forward-only → 降级需从备份恢复）

---

## 26. Final Verdict

### **BLOCKED — 见 §1 的三类环境/前置阻塞；代码可控的发布工程已全部完成并实测**

按用户 §43 的规则：`V1.0.0 READY` 与 `V1.0.0 READY — EXTERNAL HTTPS VALIDATION REQUIRED` **均不适用**——后者仅允许「除操作者提供的真实 DNS/域名/TLS 之外，所有代码可控闸门全部通过」，而本阶段除 HTTPS 外还有：特权安装/卸载/purge 路径未端到端执行（环境无 root）、AWD/AWDP 宿主级完整 E2E 因生产保护而禁止执行（且已实测复现风险）。这些属于**未被证明**，不是「通过」。

**已确证的部分**（可直接支撑 GA 决策）：可部署形态真实跑通（隔离 RC 栈、5 服务 healthy、真实 TLS 链、前端平台路由全绿）；备份/恢复真实破坏演练 15/15；SDK 外部消费 51/51；发布工作流安全语义 8/8 + 静态 84/84；安装器契约 50/50；前端管理器 72/72；发现并修复 2 个「一装就崩」的 GA 阻塞缺陷（F1/F2）与 5 个中低缺陷（F3–F8）。

**下一步（唯一让判定转绿的路径）**：在一台**独占**的可弃置 Linux 宿主上，以普通用户 + sudo 执行 §25 清单，重点闭环 B1（运行镜像分发渠道）、B2（AWD/AWDP 三套 E2E）、B3（安装/升级/卸载/purge 端到端）与 B4（真实域名 HTTPS）。


---

## Phase 13.1 — GA Closure

> 目标：关闭 GA 前全部代码侧工作。**未执行**任何发布动作（无 push / 无 tag / 无 GitHub Release / 无 npm publish / **无 GHCR 镜像推送** / 未合并分支）。
> 原 Phase 13 判定保留为 `BLOCKED`；本节的最终状态以其为准。

### B1 Runtime Image Distribution

**最终规范引用（GHCR 为规范在线通道）**

| 用途 | ref |
|---|---|
| AWD FlagServer | `ghcr.io/floatctf/awd-flagserver:<V>` |
| AWD JudgeServer | `ghcr.io/floatctf/awd-judgeserver:<V>` |
| AWDP JudgeServer | `ghcr.io/floatctf/awdp-judgeserver:<V>`（**已扁平化**；旧名 `floatctf/infra/awdp-judgeserver` 不再作为默认） |

`<V>` = 平台版本（tag 去前导 `v`，GA 为 `1.0.0`）。本地/开发构建不带 registry 时仍是历史本地名（`floatctf/awd-flagserver:<tag>` 等），据此保持既有本地 E2E 脚本可用。

**工作流行为（`.github/workflows/release.yml`）**：新增 tag 守卫的 `runtime-images` job（`needs.build.outputs.publish == 'true' && github.ref_type == 'tag' && startsWith(github.ref_name,'v')`，并在 job 内再次断言 tag 守卫），`permissions: contents: read + packages: write`（全仓唯一 `packages: write` 与唯一 `docker login`），用 `secrets.GITHUB_TOKEN` 登录 `ghcr.io`，调用 `scripts/build-runtime-images.sh --registry ghcr.io/floatctf --tag "$VERSION" --label ... --push`。
- **不可能**在 `pull_request` / 分支 push / `workflow_dispatch`（`publish=false`）/ RC 工作流上推送；分支/非 tag ref 的 `publish=true` **fail closed**。
- OCI 标签由 workflow 上下文注入（**非硬编码**）：`org.opencontainers.image.source=https://github.com/FloatCTF/floatctf`、`.version=<V>`、`.revision=${{ github.sha }}`、`.created=<UTC 构建时刻>`；标签内不含任何密钥。
- `rc.yml` 只**构建并检查**运行时镜像（同样调用该脚本，不带 `--registry`/`--push`），静态断言确认其无 login / 无 push / 无 `packages: write` / 无 gh-release。

**安装器行为（`scripts/install.sh`）**：原 warn-only 的 `check_runtime_images` 被 `ensure_runtime_images` 取代，在 `run_deploy` 中于 `prepare_env` **之前**执行 → 逐个 `docker image inspect`，缺失则 `docker pull` 精确 `<V>` ref，仍缺失即 **`die`**（不再"先装好、等比赛时才炸"）。失败信息列出缺失 ref、精确 `docker pull` 命令、以及 `docker save`/`docker load` 离线逃生口。
`--skip-runtime-images`（`FLOATCTF_SKIP_RUNTIME_IMAGES=1`）是唯一显式降级开关（降为醒目 WARN 并在收尾再次提示），供只跑 Jeopardy 的宿主机使用。默认硬失败。
Registry 前缀为单一事实来源常量 `RUNTIME_IMAGE_REGISTRY`（默认 `ghcr.io/floatctf`，可 env 覆盖）。

**配置默认值与升级语义（§2.5）**：镜像 ref 只来自 TOML（`[awd] flagserver_image`/`judgeserver_image`、`[awdp] practice_judgeserver_image`），Rust 侧默认值同步改为规范 GHCR ref（`apps/api/src/core/config.rs`，含新单测 `runtime_image_defaults_match_canonical_ghcr_refs`）。**无需数据库迁移**（不是动态设置）。升级时 `prepare_configs` 新增 `preserve_custom_runtime_images`：
- 已存在值是**已知 stock 模式**（历史 `floatctf/awd-*: *`、`floatctf/infra/awdp-judgeserver:*`、三个规范 GHCR ref）→ 迁移到新规范默认；
- 其它值视为**管理员自定义** → 原样保留 + `warn`；
- `--reset-runtime-images` 强制规范值、`--keep-runtime-images` 强制保留；两者同时给出 **fail closed**。

**本地镜像 smoke（未推送）**：`FLOATCTF_BUILD_*` 注入后 `scripts/build-runtime-images.sh --tag 1.0.0` exit 0；三个镜像均通过脚本内 `ldd` 检查，容器可启动并绑定 HTTP 监听（`/` 返回 404 证明进程健康），`Config.Env` 无密钥。见下方「Final Artifacts → 本地运行时镜像」。

**测试**：新增 `scripts/test-runtime-images.sh` → **76/76**（含 stub-docker 行为验证：默认**不** push；`--push` 时只推版本 tag、绝不推 `:latest`；rc/分支/非发布路径无 login/push；OCI 标签名与三个规范 ref 存在；`packages: write` 唯一性）。`scripts/test-installer-contract.sh` 扩到 **106/106**（含 `ensure_runtime_images` 失败路径、`--skip-runtime-images` 降级、渲染出的规范 ref、升级保留三种模式）。

### R2 RustFS Readiness

**原故障（Phase 13 实测）**：compose 的 rustfs healthcheck 仅 `nc -z 127.0.0.1 9000`（TCP 开放 ≠ S3/HTTP 层可用）；API 在 `depends_on: service_healthy` 下启动并调用 `ensure_buckets()`（**无重试**）→ `init rustfs failed: service error` → `bootstrap/mod.rs:131` panic → crash-loop（`restart: unless-stopped` 最终自愈，但有失败重启窗口）。

**修复（两层，均已落地）**
1. **Compose 边界**：rustfs healthcheck 改为**真实 HTTP 就绪探测**——镜像内只有 BusyBox `nc`（无 curl），故用 `nc` 发真实 HTTP 请求读 RustFS 的 `/health`（实测该端点在此 pinned 镜像返回 **200**；`/healthz` 返回 503 故不使用），`{ printf 'GET /health HTTP/1.1\r\n...'; sleep 1; } | nc -w 3 127.0.0.1 9000 | head -1 | grep -q ' 200 '`；因探针最多约 4s，参数为 `interval 10s / timeout 10s / retries 12 / start_period 30s`（原 `timeout 3s` 会把探针掐死）。**未**新增镜像包、**未**发明端点。
   - 独立验证：从 `docker compose config --format json` 取出**渲染后**的探针字符串，作为真实 `--health-cmd` 跑 RustFS：`starting → healthy`（约 12s，`exit=0`）；TCP-only 假服务上探针 **fail closed**（1s 内失败）。
   - 开发 compose（`infra/compose/compose.dev.yml`）同步为同一探针（参数按 dev 节奏 5s/10s/24/10s），并已对**运行中的真实 dev rustfs 容器**验证 PASS。
2. **API 侧有界重试 + 错误分类**（`apps/api/src/infrastructure/storage.rs`）：`RetryPolicy` = `max_attempts=12`、`total_deadline=90s`、`initial_backoff=1s`、`max_backoff=8s`、`jitter 0..250ms`（退避 1/2/4/8… 累计 71s < 90s，且 `elapsed+delay >= deadline` 提前收敛 → **绝不无限重试**）。
   - 分类器只读取 SDK 的**状态码与错误 code**（不读 message/body，避免 RustFS 在消息里回显 access key 片段）：`DispatchFailure`/`TimeoutError`/`ConstructionFailure`/5xx/非 401-403 的 4xx（含 404 NoSuchBucket、408、429、无状态裸 `service error`）/`Unknown` → **transient 重试**；HTTP **401/403** 或 code ∈ {InvalidAccessKeyId, SignatureDoesNotMatch, AccessDenied, InvalidAccessKey, InvalidSecurity, InvalidToken, ExpiredToken, TokenRefreshRequired, AccountProblem, AuthorizationHeaderMalformed} → **permanent 立即失败**（不烧满预算）。
   - 逐次日志：`RustFS bucket initialization failed attempt=N max_attempts=12 class="transient" error=…` + `retrying … delay_ms=…`；耗尽错误：`RustFS did not become usable within the bounded retry window (12 attempts / 90s, elapsed 93s); last classified error: …`；永久错误：`RustFS storage configuration error (permanent, not retryable): …`。**无密钥泄漏**。
   - `bootstrap/mod.rs` **有意未改**（保留 panic-on-Err）：现在只在永久配置错误或有界耗尽时触发。
   - 单测 20 项（分类器 + 策略边界）：`cargo test -p floatctf --lib storage` → **20 passed / 0 failed**。

**重启次数证据（真实 Docker，`scripts/test-rustfs-readiness.sh`，隔离网络 + 自建 PostgreSQL/Redis + 自有 48 迁移 DB + 宿主侧只答 `/_ping` 的 fake Docker socket，全程**不触碰宿主 Docker/helper 与既有 `fctf*` 资源**）**：**PASS=24 FAIL=0 SKIP=0**
| 场景 | 观测 |
|---|---|
| 1 healthcheck 契约 | TCP-only 假服务：旧 `nc -z` 会判 healthy，新 `/health` 探针 1s 内 **fail closed** |
| 2 RustFS 迟到（API 先起） | API 未退出；日志出现 `class="transient"` 与退避重试；RustFS 就绪后 API **2s 内 healthy**；**RestartCount = 0**（无 crash-loop churn）；日志 `Rustfs connected OK` |
| 3 RustFS 永不到达 | API 在 **95s** 后以 exit code 101 有界失败（不无限挂起），日志含确切耗尽消息 |

清理佐证：容器 306→306、网络 181→181（与基线**逐字节相同**），`fctf*` 172/174 前后不变，无 `r2test*` 残留镜像/卷。

### R3 Python / tomllib Precheck

- **精确检查**（能力探测，而非版本字符串）：`python3 -c 'import tomllib' >/dev/null 2>&1`，置于 `main()` **最前**（早于 `require_root`、包安装、`mkdir`、下载、`docker build`、迁移），并在 `check_commands` 与 `precheck` 再做纵深防御。**无任何 pip**（测试断言不存在 `pip install`）。
- **失败信息**（真实运行取证）：`[FAIL] Python 缺少 stdlib tomllib（需要 Python 3 且带标准库 tomllib，即 **Python ≥3.11**；检测到: Python 3.10.13）。release 的 db/migrate.sh 用它解析 TOML 配置，**升级/迁移路径**必须有它才能运行。请用宿主包管理器升级 python3 到 ≥3.11（tomllib 只随标准库提供：不要用 pip 往宿主装，也不要用 sudo pip）后重新运行本安装器。`
- **负向测试（证明"改动前失败"）**：`test-installer-contract.sh` 真的以 fake `python3`（`import tomllib` 失败、`-V` 报 `3.10.13`）运行 `scripts/install.sh --version 1.0.0`，断言 ① rc≠0；② stderr 为上述精确消息（含 `Python ≥3.11`、检测到的 `3.10.13`、迁移路径）；③ stderr **不含** `需要 root`（证明执行确实走到该检查而不是更早退出）；④ 临时 `FLOATCTF_HOME` **从未被创建**（零改动）。另有静态顺序断言（`check_python_tomllib` 早于 `run_init`，而 `run_init` 含 `require_root`）。
- 正向对照：本机 Python 3.14.7 → `[ OK ] Python tomllib 可用`，随后按预期停在 `[FAIL] 需要 root`（无写入）。
- 文档前置条件已在 `INSTALL.md` / `README.md` / `RELEASE.md` 同步（§4.1 要求）。

### B2 Exclusive Host E2E

**未执行 — BLOCKED（exclusive disposable host required）**

**宿主安全门禁（§5）证据**（`var/rc131/host-gate-evidence.txt`）：本机是**共享宿主且承载重要生产实例**——
- `floatctf-helper.service` active；helper 两 socket 就位（`helper-control.sock` / `helper-docker.sock`）
- compose 项目 `floatctf` 5 容器全部 healthy（`floatctf-api:1.0.0`、caddy、postgres、redis、rustfs）
- `fctf*` 容器 **172** 个、`fctf*` 网络 **174** 个、`floatctf_awd*`/`floatctf_awdp*` nft 表 **172** 张
- 生产 `FLOATCTF_HOME` 存在（`/home/fb0sh/floatctf-prod`）
- 且 agent **无 root**：`sudo -n true` → `The "no new privileges" flag is set`；`NoNewPrivs: 1`；`unshare -r` 被拒

按 §5「If this is a shared/production host: DO NOT RUN B2/B3」，**未执行**任何 B2/B3 测试，**未**尝试 namespace 技巧或第二 API 共存，**未**复制生产 `internal_token_key` 进 RC，**未**再触碰生产运行时资源。
Jeopardy / AWD / AWDP 的宿主级 E2E **全部待执行**（需独占宿主，命令见 §7）。

### B3 Privileged Lifecycle

**未执行 — BLOCKED（需独占宿主 + 真实 root）**

`sudo` 不可用使"普通用户 + sudo"的安装/升级/安全卸载/purge/重装链路在本机**不可能**执行；且本机 purge 会删除生产共享的全局 AWD nft 表等资源，按 §5 禁止。
已完成的替代（非端到端，仅代码/隔离级）：Phase 13 的安装器契约（现 106 项，含 `apply_migrations` 真集群 48/48、活跃运行时守卫四分支、R3 负向测试、Caddyfile/TOML 参考文件零漂移）与真实备份/恢复演练（15/15）。**未**声称 B3 通过。

### B4 Accepted Limitation

- 产品负责人**明确豁免**真实公网域名 / 公共 DNS / Let's Encrypt 签发验证：**B4 不是 v1.0 的 GA 阻塞**。
- **保留真实证据、不夸大**：本地已验证生产配置下的 Caddy + 其**受信任本地 CA**——证书链校验通过（**未使用 `curl -k`**，`tls=0`、`Verify return code: 0`）、HTTP→HTTPS 308、`/` 200、`/api/frontend` 200、registry `no-store`、版本化资产 `immutable`、深层路由 200、恢复后 S3 对象经 `/public` 200。
- **未**删除任何证据；文档统一表述为「真实公网 TLS 未验证，属已接受的 v1.0 运维限制；操作者须在安装后自行验证其域名」。文档中**没有**"真实 HTTPS 已完整验证"这类表述。

### Documentation Closure

| 文档 | 处理 |
|---|---|
| `infra/caddy/Caddyfile.prod` | 与 `install.sh` 内嵌 `CADDY_TMPL_EOF` **正文逐字节一致**（仅多一个"勿单独编辑"头部）；新增契约测试断言，杜绝静默漂移 |
| `infra/config/floatctf.prod.toml` | 补 `[auth] awd_root_key`/`internal_token_key`；运行时镜像改为 `${RUNTIME_IMAGE_REGISTRY}/…` 规范值；无任何密钥字面量；新增"参考副本"说明；新增契约测试断言覆盖安装器模板全部键值 |
| `apps/api/tests/README.md` | 修正默认端口误导（harness 默认 `8080`，而 `mise run dev` 是 `9090`，须显式导出否则静默 soft-skip） |
| `docs/frontend/ARCHITECTURE.md` | 修正 9 节下误编为 5.1/5.2/5.3 的子节编号 |
| `apps/api/README.md` | 用"源码为权威 + 代表性模块/路由总览"替换上千行易腐的端点清单 |
| `INSTALL.md` | R1 警告、Python/tomllib 前置、两条产物通道、`--ops-url`/`--skip-migrations`/`--skip-runtime-images`/`--reset|--keep-runtime-images`、升级子节、镜像获取与 `docker load` 逃生口、健康检查、B4 已接受限制、重装 |
| `RELEASE.md` | 两条通道（GitHub Release 文件制品 vs GHCR OCI）与实际名字；tag 路径的 GHCR 登录+推送步骤；R3 前置；B2/B3 必须执行但**本阶段未执行**；R1 警告；明确"未执行任何发布动作" |
| `README.md` | 架构/部署模型、GHCR 运行时镜像、外部前端、备份/恢复、最低前置（含 Python ≥3.11 + `tomllib`）、当前产物、单控制面警告、指向 INSTALL/RELEASE 而非重复细节 |
| `DEVELOPMENT.md` | 顶部 R1 警告 + 「绝不在生产宿主启动开发栈」专节 |
| `docs/agents/ARCHITECTURE.md`、`docs/deployment/portability.md` | R1、GHCR 引用、运行时镜像分发、修正 Caddy "host network" 等陈旧描述 |
| `infra/compose/compose.dev.yml` | dev rustfs healthcheck 与生产同构（同一 `/health` 探针），已对真实 dev 容器验证 |
| `chore/v1.0.0-release-notes.md` | 运行时镜像改为 GHCR 分发（移除"无分发渠道"的阻塞表述）；已知限制更新为：公网 HTTPS 未验证（已接受）、单控制面/宿主、bundle 体积、无热切换、无 Marketplace/签名、B2/B3 待独占宿主；**未**把已修项列为当前阻塞 |

**R1 单控制面不变量**（统一措辞，见 6 个文件）：同一宿主/helper/Docker daemon 同时刻只允许一个 FloatCTF 控制面（API）；第二套 API（开发/RC/测试栈或第二份生产安装）会 reconcile 全局/固定命名的宿主资源（`floatctf_awd` 单一全局 nft 表；`fctf-awdp-practice` / `fctf-awdp-practice-judge` 固定名）→ **Phase 13 实测发生过**，非理论风险；**绝不在承载生产实例的宿主上启动开发/RC/测试 API**。文档未包含任何生产主机名/IP/路径/token。

### Final Artifacts

**文件制品（`release-checksums.sh --assemble … 1.0.0` 于干净构建态装配；11 项；`sha256sum -c SHA256SUMS` → 10/10 OK）**

| artifact | size | SHA256 |
|---|---|---|
| `floatctf` | 48M | `b0318af1f8c16ea6ae799b8439b993efef5cec3dd7bbc8978ab736dedb8da719` |
| `floatctf-helper` | 2.8M | `6c7624c82d8b4f792c9a5b7575b5ce24b4a67a345a08b760cb6ef178a8decb3c` |
| `web-dist.tar.gz` | 2.6M | `dc1b5bd71a2b087f25bb289365fa7f262d03dfcaa9d209ecf9bea838a35f36ce` |
| `merged.sql` | 360K | `c898a88a2a2bdd88327a23b6f566ab5d4d04ca66478cbcd39f42b5b1d6346e1a` |
| `frontend.sh` | 68K | `8babf9c47e435b99f31292cd37d112849cbb9e679ba7d276a1bcc63354b41345` |
| `install.sh` | 124K | `9adb76ea8ca62593dec8dfbd342c3e985fb4b23f7519a8ae663aad652f1c6564` |
| `ops-tools.tar.gz` | 84K | `1084b0dddb75e1084c8501a07c8277a4232130e08ff454fd987a8759e32b24f2` |
| `floatctf-sdk-1.0.0.tgz` | 108K | `a75c13ed70284de998a85e20fb02f134776f5c7e3e9b35d87ac33ec1f0919ef5` |
| `floatctf-react-1.0.0.tgz` | 32K | `12004c37803ca8fc53c5a984a9091e241bf385ff59450fb80ea7c0fef20206f6` |
| `floatctf-frontend-runtime-1.0.0.tgz` | 44K | `dc55c72c35847d57dd11ceaa0d6db48ad574cbf8e9ef7e2db919fa0887d2e913` |
| `SHA256SUMS` | 4.0K | `d9ed141801be0075cd01f85f7486a9db243765cea8f487a196e5045a549fb475` |

**本地运行时镜像 — LOCAL TEST DIGEST（**未**推送；**不是** GHCR 已发布 digest）**
构建命令：`FLOATCTF_BUILD_{SOURCE,VERSION,REVISION,CREATED}=… bash scripts/build-runtime-images.sh --tag 1.0.0`；构建时 revision = `df81f58`（Phase 13.1 代码改动之前的 HEAD；GA 由 CI 在 release tag 上重建，revision 标签届时为 tag 指向的 commit）。

| ref | LOCAL TEST DIGEST | size | Arch | User | Entrypoint |
|---|---|---|---|---|---|
| `ghcr.io/floatctf/awd-flagserver:1.0.0` | `sha256:e3cb344ad7e2c46e8717a6bb3b32e8c14ec56ccaf0c26247aab0aefd9d5b2781` | 188.2 MiB | amd64/linux | unset (root) | `/usr/local/bin/awd_flagserver` |
| `ghcr.io/floatctf/awd-judgeserver:1.0.0` | `sha256:4957dff7599d1302e0d5b1e6f493ffb846a4194ec560373d4bc5d749f69ca60c` | 188.6 MiB | amd64/linux | unset (root) | `/usr/local/bin/awd_judgeserver` |
| `ghcr.io/floatctf/awdp-judgeserver:1.0.0` | `sha256:fe4fbf4a4d2c7754026ac2d58a35835e07e2bfd82c1233c410676e7c1f6b0a85` | 188.9 MiB | amd64/linux | unset (root) | `/usr/local/bin/awdp_judgeserver` |

三镜像 OCI 标签：`source=https://github.com/FloatCTF/floatctf`、`version=1.0.0`、`revision=df81f587…`、`created=2026-10-07T11:21:03Z`；无密钥标签，`Config.Env` 无凭据。
**PUBLISHED DIGEST = not yet available**（GA 首次 tag 发布后才存在；本阶段**未**推送任何镜像）。

### Remaining Risks

**GA blockers（仍需闭环，均为"须在独占宿主执行/决策"，非代码缺陷）**
| # | 阻塞 | 说明 |
|---|---|---|
| B2 | 独占宿主 Jeopardy/AWD/AWDP 宿主级 E2E 未执行 | §5 门禁：本机是共享宿主且有生产实例 → 禁止；命令见 Phase 13 §13/§14/§15 |
| B3 | 普通用户 + sudo 的安装/升级/安全卸载/purge/重装未端到端执行 | 无 root；须可弃置宿主 |
| — | **真实 GHCR 推送未执行** | workflow 仅静态+行为验证；首次真实 `v*` tag 需人工确认 `packages: write`、镜像可拉取、digest 记录 |

**accepted v1.0 limitations（产品负责人已接受 / 设计取舍）**
- 真实公网域名 HTTPS 未验证（B4，**已豁免**）；操作者须在安装后自行验证域名。
- 单控制面/宿主不变量（R1）：同一宿主只能有一套 FloatCTF API；不得在生产宿主跑开发/RC/测试栈。
- 三个运行时服务镜像容器内 `User` 未设置（=root）——沿用既有基础镜像行为，未在本次收口中改动。
- Default Frontend bundle 约 4.3 MB（gzip ~1.08 MB）。
- 前端切换需页面重载；无 Marketplace/包签名。

**post-v1.0 items**：前端 Marketplace 与签名、无刷新热切换、UI 代码分割、`packages/sdk` 的 `./api/*` 通配子路径补全、`packages/react` `skipLibCheck` 上游问题、源码 map `sourcesContent`、clippy 告警清理、宿主历史残留（172 容器/174 网络/172 nft 表）按护栏分批清理、运行时镜像容器内非 root 化、`awd`/`awdp` 服务镜像的 `--registry`+digest 固定（pin by digest）。

### Final Verdict

**READY FOR EXCLUSIVE-HOST GA VALIDATION — NOT YET V1.0.0 READY**

代码侧 GA 工作已全部完成并通过全部门禁（B1 运行时镜像分发、R2 RustFS 就绪、R3 tomllib 前置、文档收口、发布工作流安全、备份/恢复、前端平台、质量门禁全绿）；B4 已由产品负责人豁免。
**唯一未完成项是 B2/B3 在独占宿主上的执行**——本环境是承载重要生产实例的共享宿主且无可用的 root，按 §5 明确禁止执行、按 §19 亦不足以判定 `V1.0.0 READY`。**不存在代码侧 GA 阻塞**，故不判定 `BLOCKED`。
