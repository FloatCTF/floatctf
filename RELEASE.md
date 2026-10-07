# FloatCTF v1.0 发布清单（Release Checklist）

本文档是 **v1.0 的人工发布清单**：发布由 CI 自动执行（`.github/workflows/release.yml`），
本清单只规定人在**什么时机**做**什么动作**、以及**看什么证据**才算通过。

> **尚未发布：Phase 13 / 13.1 没有打任何 tag、没有创建 GitHub Release、没有 npm publish、
> 也没有向 GHCR 推送任何镜像。**
> 本清单里的命令都是**待执行**（或事后回滚）的操作说明，不是已经发生的事实。
> 当前仓库没有 `v1.0.0` tag，也没有 v1.0.0 的 GitHub Release / GHCR image。

> ⚠️ **单控制面不变量（R1）—— 同一宿主/helper/Docker daemon 同一时刻只允许一个 FloatCTF 控制面（API）。**
> 第二套 API —— 包括开发栈、RC/测试栈或第二份生产安装 —— 会 reconcile 全局/固定命名的宿主资源
> （AWD 防火墙是单一全局 nft 表 `floatctf_awd`；AWDP 练习网络与容器名固定为 `fctf-awdp-practice` /
> `fctf-awdp-practice-judge`），从而干扰正在运行的实例。**这在 Phase 13 实测发生过**（第二个 API
> 重建了线上练习判题容器与网络一次），不是理论风险。
> **绝不要在承载生产实例的宿主上启动开发 / RC / 测试 API。**
> 发布后的冒烟验证必须在一台**独占**宿主上进行。

相关文档：

| 文档 | 用途 |
|---|---|
| [INSTALL.md](./INSTALL.md) | 生产安装、部署、运维、卸载权威说明 |
| [README.md](./README.md) | 项目概览与产物说明 |
| [chore/v1.0.0-release-notes.md](./chore/v1.0.0-release-notes.md) | v1.0.0 发布说明**草稿** |

---

## 0. 本次发布的产物契约（ARTIFACT CONTRACT v1.0）

v1.0 有**两条产物通道**，发布时必须**两条都验证**：

### 0.1 通道 A —— GitHub Release 文件产物（11 个）

| # | 产物 | 说明 |
|---|---|---|
| 1 | `floatctf` | API release 二进制（installer 用它本地构建生产 image） |
| 2 | `floatctf-helper` | 宿主控制面二进制 |
| 3 | `web-dist.tar.gz` | bootstrap 引导页 + 版本化 Default Frontend 制品 |
| 4 | `merged.sql` | fresh PostgreSQL bootstrap |
| 5 | `frontend.sh` | 前端管理器（安装到 `$FLOATCTF_HOME/frontend.sh`） |
| 6 | `install.sh` | 安装器（下载入口固定为 `releases/download/v<V>/install.sh`） |
| 7 | `ops-tools.tar.gz` | `backup.sh` / `restore.sh` / `db/migrate.sh` / `db/migrations/*.sql` |
| 8 | `floatctf-sdk-<V>.tgz` | `@floatctf/sdk` |
| 9 | `floatctf-react-<V>.tgz` | `@floatctf/react` |
| 10 | `floatctf-frontend-runtime-<V>.tgz` | `@floatctf/frontend-runtime` |
| 11 | `SHA256SUMS` | **覆盖 1–10 全部文件产物**的 sha256（**不含自身**） |

### 0.2 通道 B —— GHCR OCI 运行时镜像（3 个）

AWD / AWDP 的运行时服务镜像不再"随 release 文件分发"；**GHCR 是规范在线分发通道**：

```text
ghcr.io/floatctf/awd-flagserver:<V>
ghcr.io/floatctf/awd-judgeserver:<V>
ghcr.io/floatctf/awdp-judgeserver:<V>      # 已扁平化；旧名 floatctf/infra/awdp-judgeserver 不再使用
```

- 由 tag 触发的 `release.yml` 推送（`runtime-images` job 复用
  `scripts/build-runtime-images.sh --registry ghcr.io/floatctf --push`）；
  `packages: write` **只**出现在 tag 发布路径上（PR / 分支 dispatch / RC **永不**推送镜像）。
- 本地/开发构建（不接 registry）仍使用本地历史名 `floatctf/awd-flagserver:<tag>`、
  `floatctf/awd-judgeserver:<tag>`、`floatctf/infra/awdp-judgeserver:<tag>`（awdp 的
  `infra/` 段只为向后兼容保留）。
- 离线宿主仍可用本地逃生通道：在源码签出上
  `sudo bash scripts/build-runtime-images.sh --tag <V>`（本地历史名）或
  `--registry ghcr.io/floatctf --tag <V>`（规范名），再 `docker save` / `docker load`。
- 安装器渲染 `config/floatctf.toml` 与检查镜像都使用**同一个** `FLOATCTF_RUNTIME_IMAGE_REGISTRY`
  （默认 `ghcr.io/floatctf`），是运行时镜像 ref 的单一事实来源。

### 0.3 版本占位符

`<V>` = tag 去掉前导 `v`（`v1.0.0` → `1.0.0`）。它是**平台版本**，同时用于文件产物命名
与 GHCR image tag。安装器渲染生产配置时用同一个值（`${VERSION}`）。

---

## 1. Pre-release（发布前）

- [ ] `release/v1.0` 合入后的 `main` 分支 CI 全绿（fmt / lint / test / 前端 `vite build && tsc` / 架构门禁）。
- [ ] **两条产物通道都有核对方案**：文件产物由 `release.yml` 产出；3 个 GHCR 镜像由 tag 路径推送
      （见 §3）。缺任何一条都**不是**完成的发布。
- [ ] 已核对 Phase 13 RC 报告的 **Verdict**（`chore/phase13-final-rc-report.md`）：
      Phase 13 初判为 **BLOCKED**（无 root → 特权安装/升级/卸载/purge 未执行；AWD/AWDP 宿主级 E2E 因
      唯一可用宿主已在跑重要生产实例而刻意未执行；无真实域名 HTTPS）。
      **Phase 13.1 关闭了全部代码可控工作（B1 / R2 / R3 / 文档），产品负责人已豁免 B4。**
- [ ] **B2 / B3 必须在独占宿主上作为发布前步骤执行**（Phase 13 / 13.1 **没有**执行）：
      - **B2**：独占宿主上的 Jeopardy / AWD / AWDP 完整宿主级 E2E（含 WireGuard / nftables）。
      - **B3**：普通用户 + `sudo` 的安装 / 升级 / 卸载 / purge / 重装全流程。
      未完成 B2/B3 之前不得声称 v1.0 已通过宿主级验证。
- [ ] 已确认 **R3 前置条件**：安装器要求 `python3` **带 stdlib `tomllib`**（Python ≥ 3.11），
      因为 `apps/api/src/sql/migrate.sh` 用 `tomllib` 解析配置。安装器在**任何改动之前**做能力检查
      （`python3 -c 'import tomllib'`），缺失即 fail-fast；**不**走 pip 安装。
- [ ] 工作区干净：`git status --porcelain` 无未预期的改动（**不要在脏树上打 tag**）。
- [ ] 版本一致性对齐（见下表）。
- [ ] 变更日志 / 发布说明已起草（[chore/v1.0.0-release-notes.md](./chore/v1.0.0-release-notes.md)）。
- [ ] 已复核 release workflow 的安全语义（tag 才发布、dispatch 默认只上传 workflow artifact、
      权限最小化、`packages: write` 只在 tag 发布路径）。
- [ ] 确认 CI 会产出 `SHA256SUMS` 与 3 个 SDK tarball（无需人工上传）。

### 1.1 版本一致性

| 对象 | 期望值 | 检查方式 |
|---|---|---|
| API / 平台 | `1.0.0` | `apps/api/Cargo.toml`、`GET /api/admin/system/version` |
| 根 `package.json` | `1.0.0` | — |
| `apps/web` / `frontends/default` | `1.0.0` | — |
| `packages/{sdk,react,frontend-runtime}` | `1.0.0` | — |
| Default Frontend 安装版本 | `1.0.0`（不可变目录 `frontends/default/1.0.0`） | `frontends/default/package.json` |
| Rust workspace crates | **各自版本，刻意不强制 1.0.0** | `floatctf-helper`/`helper-protocol`/`awd-flagserver`/`awd-judgeserver`/`awdp-judgeserver` = `0.1.0`；`fcmc` = `1.2.0` |

> 契约/Schema 版本是**另一套概念**，不要与平台版本混淆：
> `API_CONTRACT_VERSION = "1"`、`FRONTEND_RUNTIME_CONTRACT_VERSION = "1"`、
> frontend manifest `schemaVersion = 1`、frontend registry `schemaVersion = 1`、backup format `1`。

---

## 2. Tagging（打 tag）

- [ ] 确认要发布的 commit（通常是 `release/v1.0` 合入后的 `main` HEAD）。
- [ ] 按项目策略创建**注解 tag**（或签名 tag）：

  ```bash
  git tag -a v1.0.0 -m "FloatCTF v1.0.0"
  # 或签名：git tag -s v1.0.0 -m "FloatCTF v1.0.0"
  ```

- [ ] **不要**在 dirty tree 上打 tag；tag 必须指向已通过 CI 的那个 commit。
- [ ] 明确认知：**tag 就是发布开关** —— 推送 `v*` tag 会同时打开文件产物发布**和** GHCR 镜像推送。

---

## 3. Release（发布）

- [ ] 推送 tag：`git push origin v1.0.0`。
- [ ] 确认 `release.yml` 被 `push` tag 触发并跑完。

### 3.1 通道 A —— 文件产物

- [ ] 核对 workflow 真正构建了**全部 11 个产物**（缺 `install.sh`、`ops-tools.tar.gz` 或
      `SHA256SUMS` 即缺陷）。
- [ ] 核对 `SHA256SUMS` 可校验，且**只**覆盖文件产物：

  ```bash
  sha256sum -c SHA256SUMS
  ```

- [ ] 核对 `ops-tools.tar.gz` 内含 `backup.sh`、`restore.sh`、`db/migrate.sh`、`db/migrations/*.sql`。
- [ ] 核对 GitHub Release 页面包含全部 11 个产物，且 `generate_release_notes` 生成的内容已人工过目。

### 3.2 通道 B —— GHCR 运行时镜像（tag 路径专属）

- [ ] 确认 workflow 在 tag 发布路径上做了 GHCR 登录（`GITHUB_TOKEN` + `packages: write`）并
      **推送 3 个镜像**。实现方式（供核对）：`release.yml` 的 `runtime-images` job 唯一持有
      `packages: write`，只在 tag 守卫的 publish 路径上执行 `docker login ghcr.io`，然后复用
      本地/RC 的同一契约脚本：

      ```bash
      bash scripts/build-runtime-images.sh \
        --registry ghcr.io/floatctf --tag 1.0.0 ... --push
      ```

      产出：

      ```text
      ghcr.io/floatctf/awd-flagserver:1.0.0
      ghcr.io/floatctf/awd-judgeserver:1.0.0
      ghcr.io/floatctf/awdp-judgeserver:1.0.0      # awdp 已扁平化（无 infra/ 段）
      ```

- [ ] 确认 `--push` **只推版本 tag、绝不推 `:latest`**；并确认 `packages: write` **没有**
      出现在 PR / 分支 dispatch / RC 路径上（RC 只应上传 workflow artifact，永不推送镜像）。
- [ ] 从一台干净的宿主核实 3 个 ref 真的可拉取且 tag 正确：

  ```bash
  docker pull ghcr.io/floatctf/awd-flagserver:1.0.0
  docker pull ghcr.io/floatctf/awd-judgeserver:1.0.0
  docker pull ghcr.io/floatctf/awdp-judgeserver:1.0.0
  ```

- [ ] 若 GHCR 包可见性需要设置（public / 与仓库关联），在 Release 页面发布后确认匿名 `docker pull`
      可用；否则只能靠 `docker login ghcr.io`。
- [ ] 确认安装器的镜像检查与 GHCR ref **同源**：安装器渲染 `config/floatctf.toml` 与检查镜像都用
      `FLOATCTF_RUNTIME_IMAGE_REGISTRY`（默认 `ghcr.io/floatctf`），避免"模板写 A、安装器查 B"。
      缺镜像时安装器 `die`；`--skip-runtime-images` 才降级为告警。

### 3.3 workflow 语义速查（已实现，供核对）

| 触发 | 行为 |
|---|---|
| `push` tag `v*` | build + verify + 发布 GitHub Release（11 个文件产物）+ 推送 3 个 GHCR 镜像 |
| `workflow_dispatch`（默认） | build + verify + 仅上传 workflow artifact；**不**建 Release、**不**打 tag、**不**发 npm、**不**推镜像 |
| `workflow_dispatch` + `publish=true` | 仅当 ref 是 `v*` tag 才发布；否则 **fail closed** |
| `workflow_dispatch` + `version` 输入 | 分支 dispatch 用于制品命名；缺 `version` 的分支 dispatch **fail closed** |

权限：workflow 与 `build` job 为 `contents: read`；只有 tag 发布路径有 `contents: write` 与
`packages: write`，且带 tag 守卫。另有**非发布** RC workflow `.github/workflows/rc.yml`
（仅 `workflow_dispatch`、`contents: read`、上传 `floatctf-rc-<V>`，永不发布/打 tag/发 npm/推镜像）。

---

## 4. Post-release（发布后）

> 以下冒烟必须在一台**独占**宿主上进行（见开头的 R1 不变量）。

- [ ] **从 GitHub Release URL 全新安装冒烟**（不要从源码 checkout 安装）：

  ```bash
  curl -fsSL https://github.com/FloatCTF/floatctf/releases/download/v1.0.0/install.sh -o install.sh
  sudo bash install.sh          # 可用 --version 1.0.0 / --ops-url ... 显式指定
  sudo systemctl start floatctf.target
  ```

- [ ] 在下载目录执行 `sha256sum -c SHA256SUMS` 通过。
- [ ] 安装器**成功获取 3 个运行时镜像**（`docker image inspect` 命中
      `ghcr.io/floatctf/*:<V>` 或 `docker pull` 成功）；Jeopardy-only 宿主可用
      `--skip-runtime-images`（env `FLOATCTF_SKIP_RUNTIME_IMAGES=1`）降级为告警。
- [ ] 确认安装器在改动前完成了 **`python3` + `tomllib`** 能力检查（Python ≥ 3.11）。
- [ ] 在真实域名上验证 HTTPS（**B4：真实公网域名 / 公共 DNS / Let's Encrypt 签发未被 RC 验证，
      由运维方在安装后自行验证**；不要声称"真实 HTTPS 已完整验证"）。
- [ ] SDK tarball 消费冒烟（**npm** 安装三个 tarball 可工作；pnpm 见下方已知限制）。
- [ ] `@floatctf/react` 消费方需要 peer deps：`react@^19`、`react-dom@^19`、`@tanstack/react-query@^5.66.5`；
      `@floatctf/sdk` 依赖 `axios ^1.11.0`；`@floatctf/frontend-runtime` 无依赖。
- [ ] **可选**：若项目 owner 决定发布 npm，则执行 npm 发布（v1.0 **尚未**执行，也非必须；pnpm 消费者需要它）。
- [ ] 核对文档中记录的下载命令与 release 页面 URL 一致。
- [ ] 观察首次真实安装，记录任何异常。

---

## 5. Rollback（回滚）

| 场景 | 动作 |
|---|---|
| 只是装错了版本 | 用上一个 tag 的安装器重装：`sudo bash install.sh --version <上一版本> ...` |
| 运行时镜像有问题 | 改 `config/floatctf.toml` 的 `[awd]` / `[awdp]` 镜像 ref 指向已知可用 tag（或 `--reset-runtime-images` / `--keep-runtime-images` 控制升级行为），再重跑安装器 |
| 前端版本有问题 | `sudo $FLOATCTF_HOME/frontend.sh set-current <id> <version>` 回退指针；内容有问题的前端用 `remove <id> <version>` |
| 前端把 UI 弄坏（破窗） | 任意页面 URL 追加 `?frontend=default` |
| 数据库升级出错 | **数据库迁移是 forward-only**，无法 downgrade；只能 `sudo $FLOATCTF_HOME/restore.sh <备份归档> --yes` 从备份恢复 |
| Release 产物本身错误 | 删除错误 Release/tag 需谨慎；优先发布修好的新版本，并保留旧 tag 供回退 |

> 回滚前先做备份：`sudo $FLOATCTF_HOME/backup.sh --out <file>`（**明文未加密，注意介质安全**）。

---

## 6. Do NOT（禁止清单）

- ❌ 不要 force-push（尤其已推送的 tag / `main`）。
- ❌ 不要从分支 / PR dispatch 发布（`publish=true` 在非 tag ref 上会 fail closed）。
- ❌ 不要让 PR / 分支 / RC 路径获得 `packages: write` 或推送 GHCR 镜像。
- ❌ 不要编辑、重命名、squash 任何**已冻结的历史 migration**；Schema 变更只能新增迁移。
- ❌ 不要在既有 Default Frontend 版本号下改动字节（版本**不可变**：同 id+version 必须逐字节一致；要改就升版本号）。
- ❌ 不要用新的 `merged.sql` 覆盖既有数据库（只用于 fresh DB）。
- ❌ 不要在跑着线上赛事的宿主上再起第二套 API/dev stack（见开头 R1 不变量：AWD 网络状态是**全局单例**）。
- ❌ 不要把 B4（真实域名 HTTPS 未验证）当成 GA 阻塞，也不要反过来声称它已经验证过。
