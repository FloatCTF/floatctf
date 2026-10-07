# FloatCTF v1.0 发布清单（Release Checklist）

本文档是 **v1.0 的人工发布清单**：发布由 CI 自动执行（`.github/workflows/release.yml`），
本清单只规定人在**什么时机**做**什么动作**、以及**看什么证据**才算通过。

> **Phase 13 未执行任何打 tag / 创建 GitHub Release / npm publish 动作。**
> 本清单里的命令都是**待执行**（或事后回滚）的操作说明，不是已经发生的事实。
> 当前仓库没有 `v1.0.0` tag，也没有 v1.0.0 的 GitHub Release。

相关文档：

| 文档 | 用途 |
|---|---|
| [INSTALL.md](./INSTALL.md) | 生产安装、部署、运维、卸载权威说明 |
| [README.md](./README.md) | 项目概览与产物说明 |
| [chore/v1.0.0-release-notes.md](./chore/v1.0.0-release-notes.md) | v1.0.0 发布说明**草稿** |

---

## 0. 本次发布的产物契约（ARTIFACT CONTRACT v1.0）

`push` tag `v*` 时，release workflow 构建并发布**共 11 个产物**；`SHA256SUMS` 覆盖除自身外的全部制品。

| # | 产物 | 说明 |
|---|---|---|
| 1 | `floatctf` | API release 二进制（installer 用它本地构建生产 image） |
| 2 | `floatctf-helper` | 宿主控制面二进制 |
| 3 | `web-dist.tar.gz` | bootstrap 引导页 + 版本化 Default Frontend |
| 4 | `merged.sql` | fresh PostgreSQL bootstrap |
| 5 | `frontend.sh` | 前端管理器（安装到 `$FLOATCTF_HOME/frontend.sh`） |
| 6 | `install.sh` | 安装器（下载入口固定为 `releases/download/v<V>/install.sh`） |
| 7 | `ops-tools.tar.gz` | 顶层 `backup.sh` / `restore.sh` / `db/migrate.sh` / `db/migrations/<48 个 .sql>` |
| 8 | `floatctf-sdk-<V>.tgz` | `@floatctf/sdk` |
| 9 | `floatctf-react-<V>.tgz` | `@floatctf/react` |
| 10 | `floatctf-frontend-runtime-<V>.tgz` | `@floatctf/frontend-runtime` |
| 11 | `SHA256SUMS` | 上述 1–10 的 sha256（**不含自身**） |

`<V>` = tag 去掉前导 `v`（`v1.0.0` → `1.0.0`）。

---

## 1. Pre-release（发布前）

- [ ] `main` 分支 CI 全绿（fmt / lint / test / 前端 `vite build && tsc` / 架构门禁）。
- [ ] 已核对 Phase 13 RC 报告的 **Verdict**（`chore/phase13-final-rc-report.md`）：当前为
      **BLOCKED — 三类环境/前置阻塞，非代码正确性阻塞**（特权安装/卸载/purge 路径未端到端执行；
      AWD/AWDP 宿主级完整 E2E 因生产保护未执行；无真实域名/DNS/TLS）。
      发布决策人必须明确：这些阻塞是**在可弃置独占宿主上补验**，还是**知情接受**后再发布。
- [ ] 工作区干净：`git status --porcelain` 无未预期的改动（**不要在脏树上打 tag**）。
- [ ] 版本一致性对齐（见下表）。
- [ ] 变更日志 / 发布说明已起草（[chore/v1.0.0-release-notes.md](./chore/v1.0.0-release-notes.md)）。
- [ ] 已复核 release workflow 的安全语义（tag 才发布、dispatch 默认只上传 workflow artifact、权限最小化）。
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
- [ ] 明确认知：**tag 就是发布开关** —— 推送 `v*` tag 会把 dispatch 的非发布路径变成发布路径。

---

## 3. Release（发布）

- [ ] 推送 tag：`git push origin v1.0.0`。
- [ ] 确认 `release.yml` 被 `push` tag 触发并跑完。
- [ ] 核对 workflow 真正构建了**全部 11 个产物**（缺 `install.sh` 或 `SHA256SUMS` 即缺陷）。
- [ ] 核对 `SHA256SUMS` 可校验：

  ```bash
  sha256sum -c SHA256SUMS
  ```

- [ ] 核对 GitHub Release 页面包含全部 11 个产物，且 `generate_release_notes` 生成的内容已人工过目。

### 3.1 workflow 语义速查（已实现，供核对）

| 触发 | 行为 |
|---|---|
| `push` tag `v*` | build + verify + 发布 GitHub Release（11 个产物） |
| `workflow_dispatch`（默认） | build + verify + 仅上传 workflow artifact；**不**建 Release、**不**打 tag、**不**发 npm |
| `workflow_dispatch` + `publish=true` | 仅当 ref 是 `v*` tag 才发布；否则 **fail closed** |
| `workflow_dispatch` + `version` 输入 | 分支 dispatch 用于制品命名；缺 `version` 的分支 dispatch **fail closed** |

权限：workflow 与 `build` job 为 `contents: read`；只有 `publish` job 有 `contents: write`，
且 `needs: build` 并带 tag 守卫。另有**非发布** RC workflow `.github/workflows/rc.yml`
（仅 `workflow_dispatch`、`contents: read`、上传 `floatctf-rc-<V>`，永不发布/打 tag/发 npm）。

---

## 4. Post-release（发布后）

- [ ] **从 GitHub Release URL 全新安装冒烟**（不要从源码 checkout 安装）：

  ```bash
  curl -fsSL https://github.com/FloatCTF/floatctf/releases/download/v1.0.0/install.sh -o install.sh
  sudo bash install.sh          # 可用 --version 1.0.0 / --ops-url ... 显式指定
  sudo systemctl start floatctf.target
  ```

- [ ] 在下载目录执行 `sha256sum -c SHA256SUMS` 通过。
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
| 前端版本有问题 | `sudo $FLOATCTF_HOME/frontend.sh set-current <id> <version>` 回退指针；内容有问题的前端用 `remove <id> <version>` |
| 前端把 UI 弄坏（破窗） | 任意页面 URL 追加 `?frontend=default` |
| 数据库升级出错 | **数据库迁移是 forward-only**，无法 downgrade；只能 `sudo $FLOATCTF_HOME/restore.sh <备份归档> --yes` 从备份恢复 |
| Release 产物本身错误 | 删除错误 Release/tag 需谨慎；优先发布修好的新版本，并保留旧 tag 供回退 |

> 回滚前先做备份：`sudo $FLOATCTF_HOME/backup.sh --out <file>`（**明文未加密，注意介质安全**）。

---

## 6. Do NOT（禁止清单）

- ❌ 不要 force-push（尤其已推送的 tag / `main`）。
- ❌ 不要从分支 / PR dispatch 发布（`publish=true` 在非 tag ref 上会 fail closed）。
- ❌ 不要编辑、重命名、squash 任何**已冻结的历史 migration**；Schema 变更只能新增迁移。
- ❌ 不要在既有 Default Frontend 版本号下改动字节（版本**不可变**：同 id+version 必须逐字节一致；要改就升版本号）。
- ❌ 不要用新的 `merged.sql` 覆盖既有数据库（只用于 fresh DB）。
- ❌ 不要在跑着线上赛事的宿主上再起第二套 API/dev stack（AWD 网络状态是**全局单例**，详见发布说明的已知限制）。
