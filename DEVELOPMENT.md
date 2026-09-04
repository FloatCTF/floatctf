# DEVELOPMENT.md — 开发指南

本文档是 FloatCTF **本地开发**的权威说明，面向两类读者：

- **人工开发者**：在自己机器上跑起来、改代码、调试。
- **AI Agent**（Claude Code / Cursor / Pi Coding Agent 等）：在本仓库内协同开发时的工作约定。

生产安装 / 运维请见 [INSTALL.md](./INSTALL.md)；架构与功能开发流程见 [docs/agents/](./docs/agents/) 系列。

---

## 0. 两种场景，先分清（本文档只管「开发」）

| | 生产安装 | 本地开发（本文档） |
|---|---|---|
| 起点 | **一行流下载 install.sh**，无需 clone 仓库 | **clone 仓库**后，用仓库里的 `install.sh --develop` |
| 命令 | `curl -fsSL <install.sh URL> -o install.sh && sudo bash install.sh` | `git clone … && cd floatctf && sudo ./scripts/install.sh --develop` |
| 产物 | 下载 release 产物（API 二进制 + web dist + merged.sql） | 本地源码编译（cargo run + vite），三产物不下载 |
| 文档 | [INSTALL.md](./INSTALL.md) | **本文档** |

**本文档只讲「开发」**：clone 仓库 → `sudo ./scripts/install.sh --develop` 起环境 →
`sudo mise run dev:api` + `mise run dev:web` 开发。

---

## 1. 前置：开发环境总览

FloatCTF 开发环境 = **主机网络能力 + dev 容器 + 本地编译的 API + Vite 前端**：

```
浏览器
  ↓
nginx（dev 容器，127.0.0.1:7780）
  ├── /api/  → 127.0.0.1:9090   （本地 cargo run 的 API 进程）
  └── /      → 127.0.0.1:3000   （本地 Vite dev server）
```

配套 dev 容器（`infra/compose/compose.dev.yml`）：

| 容器 | 用途 | 端口 |
|------|------|------|
| `floatctf-dev-db` | PostgreSQL 17，首次启动用 merged.sql 自动初始化 | 5432 |
| `floatctf-dev-rustfs` | S3 兼容对象存储 | 9000 / 9001 |
| `floatctf-dev-nginx` | 反向代理（`/api/`→9090，`/`→3000） | 7780 |

> 关键：开发与生产**主机初始化完全一致**（`network_runtime = host`，nftables +
> WireGuard + 转发 + br_netfilter + floatctf 用户/布局）。区别只在「产物来源」：
> 开发用源码本地编译 + dev compose；生产下载 release 产物 + systemd。

---

## 2. mise 环境（为什么会 `command not found`）

仓库所有开发命令由 [mise](https://mise.jdx.dev) 管理（固定版 Rust / Node / pnpm），
见 `mise.toml`。mise 通过 shell 初始化脚本注入 PATH：

```bash
# .bashrc 里（zsh 同理）
eval "$(/home/fb0sh/.local/bin/mise activate bash)"
```

所以平时 `mise`、`cargo`、`node` 能找到，是因为它们被加进了**你的登录 shell 的 PATH**。

**坑：`sudo` 会重置环境**。`sudo <cmd>` 跑的是干净 root 环境：
- PATH 被换成系统默认，`mise`/`cargo`/`node` 都找不到（`command not found`）；
- 不读你的 `.bashrc`。

**解决**：用 mise 的绝对路径 + 保留环境：

```bash
sudo -E env PATH="$PATH" /home/fb0sh/.local/bin/mise run dev:api
```

> 你的机器上 mise 绝对路径是 `/home/fb0sh/.local/bin/mise`（`which mise` 找不到时用这个）。

---

## 3. 启动开发环境（两种方式）

### 方式 A：一键（推荐）

```bash
git clone https://github.com/FloatCTF/floatctf.git
cd floatctf
mise run install                          # 装 Rust/Node/pnpm + pnpm install + cargo fetch
sudo ./scripts/install.sh --develop       # 完整主机初始化 + 起 dev 容器 + merged.sql 初始化
```

`--develop` 做的事：
1. 检测是否在源码目录（缺 `Cargo.toml`/`apps/`/`infra/` 则报错）；
2. 检查 `apps/api/src/sql/merged.sql` 存在（否则先 `mise run db:migration:merge`）；
3. 完整主机初始化（与生产一致，含 nftables/WireGuard/host，**需要 root**）；
4. 起 dev 容器（db 自动 initdb merged.sql + nginx 反代）；
5. 打印下一步提示。

然后**手动**起开发服务（两个终端）：

```bash
sudo mise run dev:api     # API → 127.0.0.1:9090
mise run dev:web          # Vite → 127.0.0.1:3000
```

统一入口 **http://127.0.0.1:7780** 。

### 方式 B：手动分步（等价于 --develop 的展开）

```bash
mise run install
mise run infra:up          # 起 dev 容器（db/rustfs/nginx）
# 两个终端：
sudo mise run dev:api
mise run dev:web
```

> `mise run dev` 会同时起 api + web（但 API 无 root 时 AWD host 网络会失败，见 §4）。

---

## 4. 为什么 API 要 `sudo`？—— CAP_NET_ADMIN

AWD 运行时需要操作宿主网络（建 WireGuard 隧道、写 nftables 规则），这需要 Linux 的
**CAP_NET_ADMIN** 能力（一张「网络管理员许可证」）。

- root 天生有这张证；普通用户没有。
- `mise run dev:api` 底层是 `cargo run`，以**你的普通账号**跑 API → 没有许可证 →
  一旦 AWD 建隧道/写防火墙就报权限错误。

**两条路**：

| 方案 | 命令 | 特点 |
|------|------|------|
| sudo 跑 | `sudo mise run dev:api` | 简单；但 sudo 重置环境（见 §2），且 root 权限过大 |
| 给二进制贴证 | `sudo setcap cap_net_admin+ep target/debug/floatctf` | 之后可不用 sudo；但每次 `cargo build` 重编译后证书丢失，要重贴 |

> 实用建议：日常开发用 `sudo -E env PATH="$PATH" ... mise run dev:api`；如果频繁
> 重启 API，考虑仓库里加一个包装脚本 `scripts/dev-api.sh` 自动处理 sudo + PATH。

**纯 Jeopardy 开发（不碰 AWD）**可以不 sudo：普通 `mise run dev:api` 即可，AWD 相关
功能才会因缺 CAP_NET_ADMIN 失败。

---

## 5. 数据库迁移

开发库（`floatctf-dev-db`，5432）的 Schema 变更走 SQL 迁移：

```bash
mise run db:migration:new <名称>     # 新建迁移文件（migrations/ 下，时间戳前缀）
mise run db:migration:validate       # 校验（不连库）
mise run db:migration:apply          # 应用到开发库
mise run db:gen                      # 重新生成 Rust 实体 + 前端 TS 类型
mise run db:migration:merge          # 重新生成 merged.sql（dev 容器 initdb 用）
```

铁律见 [docs/agents/DATABASE.md](./docs/agents/DATABASE.md)：migrations **只前进**，
已有迁移文件绝不改动；实体是生成的，手改会被覆盖。

---

## 6. 人工开发工作流

### 日常改代码

```bash
mise run dev:web      # 前端：Vite 热更新，改完即生效
sudo mise run dev:api # 后端：改完 Rust 代码需重启（cargo run 不是 watch）
```

> **后端不是 watch 进程**：`mise run dev:api` 改代码后必须手动重启，否则旧进程继续
> 提供旧行为（曾导致验证失效/误判 bug）。

### 质量门禁（提交前）

```bash
mise run fmt     # cargo fmt 检查
mise run lint    # clippy + web lint
mise run test    # cargo test + web test
mise run check   # fmt + lint + test 全绿
# 前端额外（CI 跑 vite build && tsc）
pnpm --filter @floatctf/web exec tsc --noEmit
pnpm --filter @floatctf/web build
```

### 常见命令速查

```bash
mise run infra:logs      # dev 容器日志
mise run infra:down      # 停 dev 容器
mise run infra:reset     # 重建 dev 容器（down -v + up，会清空 db 数据）
./scripts/clean.sh --all # 清理构建产物 + 开发运行时数据
```

---

## 7. Agent 协同开发约定

Agent 在本仓库工作时，请遵守：

1. **先读 [AGENTS.md](./AGENTS.md)**（必读手册 + 铁律）和本文档。
2. **先读 `HANDOFF.md`**：若根目录存在该文件，先完整读取并记住其中的上下文、决策、
   未完成事项、约束（它是开发者私有的会话交接记忆，已被 gitignore，不提交）。
3. **工作过程中**，若有新增关键决策/进展，及时写回 `HANDOFF.md`，供下次会话延续。
4. **改代码前先诊断**：涉及行为/数据变更，先向用户说明方案获批后再动手（AGENTS.md 铁律 7）。
5. **提交规范**：中文 message（feat/fix/chore/docs/refactor 前缀），按角度分批提交；
   提交前 `cargo fmt --all && cargo check -p floatctf` 与相关测试全绿。
6. **数据库**：改 Schema 只能追加新 migration（`db:migration:new`），绝不改已有迁移文件；
   改完 `db:migration:validate` → `apply` → `db:gen` → `merge`。
7. **前端**：仿照既有页面与 `components/` 组件，不另起炉灶；数据必须来自真实接口。
8. **环境事实**：
   - `sudo mise ...` 会丢环境（见 §2），脚本化时用 `sudo -E env PATH="$PATH" ...`。
   - AWD 需要 CAP_NET_ADMIN；纯 Jeopardy 不需要。
   - 后端 `mise run dev:api` 非 watch，改完要重启。

### Agent 可用命令（不 sudo）

Agent 运行验证时，多数命令无需 root：

```bash
mise run fmt / lint / test / check / build
cargo test -p <crate> <关键词>
mise run db:migration:validate / status   # 只读，不连库
```

涉及 AWD host 网络、容器、nftables/WG 的**破坏性验证**，务必先与用户确认，不要对
真实共享 Docker/生产资源动手。

---

## 8. 目录与参考

| 文档 | 用途 |
|------|------|
| [INSTALL.md](./INSTALL.md) | 生产安装 / 部署 / 卸载 / 运维 |
| [docs/agents/ARCHITECTURE.md](./docs/agents/ARCHITECTURE.md) | 架构速览 |
| [docs/agents/ADD-FEATURE.md](./docs/agents/ADD-FEATURE.md) | 新功能开发流程 |
| [docs/agents/FIX-BUG.md](./docs/agents/FIX-BUG.md) | 修 bug 流程 |
| [docs/agents/DATABASE.md](./docs/agents/DATABASE.md) | 数据库迁移 / 实体生成 |
| [docs/agents/TESTING.md](./docs/agents/TESTING.md) | 测试规范 |
| [docs/agents/RULES.md](./docs/agents/RULES.md) | 用户反复强调的规则与返工教训 |
