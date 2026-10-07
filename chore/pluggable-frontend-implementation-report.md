# FloatCTF Pluggable Frontend Implementation Report

> 本报告对应本地分支 `ui` 上的实现（**未 push**）。
> 架构文档：[docs/frontend/ARCHITECTURE.md](../docs/frontend/ARCHITECTURE.md)、
> [DEVELOPING.md](../docs/frontend/DEVELOPING.md)、[ARTIFACT.md](../docs/frontend/ARTIFACT.md)。

---

## 1. Repository Snapshot

| 项目 | 值 |
|---|---|
| 分支 | `ui`（本地 UI 开发分支，未 push） |
| base main commit | `5f3b1537c892fb17f69d3c82789e34261994c62a`（`git merge-base HEAD main` = `HEAD` 的起点） |
| 起始 HEAD | `5f3b153`（与 `main` 同一提交 → 干净地从当前 main 历史开出） |
| 实现 HEAD | 见 §21（共 6 个本地提交：4 个功能 + 2 个修复 + 1 个文档；`bfd5677` 为最终提交） |
| 工作树 | 干净（除用户既有的 `chore/` 删除与未跟踪的 `.agents/`、`PROJECT-ANALYSIS.html` —— 会话开始时即如此，未触碰） |
| 变更规模 | 387 files changed, +7725 / −1516（相对 `main`，含最终文档提交前） |
| 文件迁移 | 275 renames / 10 deletions / 80 additions |

**目录权威性**：远端不存在 `ui` 分支，本地 `ui` 就是权威；未做任何 reset/rebase/force checkout。

---

## 2. Architecture

```
                      FloatCTF Backend (apps/api)
                       │  REST / SSE / Bearer auth
                       ▼
                @floatctf/sdk                    ← packages/sdk
                框架无关：传输 / 错误模型 / 领域客户端 / DTO / fetch-SSE / 生成实体
                       │
        ┌──────────────┴───────────────┐
        ▼                              ▼
@floatctf/react                  非 React 前端（Vue / Svelte / Solid / 原生 TS）
packages/react                   直接用 sdk，不需要 react 包
可选 headless 绑定（无 UI）
        │
        ▼
Frontend implementation
  ├── frontends/default           官方前端（当前完整 UI，随平台发布，受保护）
  └── 外部仓库                     第三方前端（独立仓库、独立框架、独立路由空间）

@floatctf/frontend-runtime         ← packages/frontend-runtime
制品 / 运行时契约（框架无关；不依赖 sdk / react / 任何 UI 库）
        ▲
        │
apps/web                          极薄 bootstrap 引导页（无 React / 无 Primer / 无路由）
```

边界由 `scripts/check-architecture.sh` 失败即红地强制（7 条规则，见 §16）。

---

## 3. Packages

### `@floatctf/sdk`（`packages/sdk`，134 文件）

- **传输**：`createFloatCTFClient({ baseUrl, getUserToken, getAdminToken, onUnauthorized, onError })`
  —— 认证 token 来源与 401 之后的 UI 反应**由调用方注入**；SDK 不 import router / Zustand /
  localStorage，不导航。
- **错误模型**：`FloatCTFError`（`httpStatus` / `code` / `platformMessage` / `kind`
  (`http|platform|network|unknown`) / `original` / `unauthorized` / `displayMessage`），
  并保留 axios 兼容的 `response` 形态 —— **既有页面的错误处理一行都不用改**。
- **协议**：`UniResponse<T>` / `QueryParams` 原样透传后端 envelope，不发明第二套协议。
- **领域客户端**：选手端 10 个模块 + 管理端 23 个模块 + AWD/AWDP/AWDP-Runs（含类型）。
- **SSE**：fetch-based（Bearer 走 `Authorization` 头、绝不进 URL）、AbortController 生命周期、
  指数退避 + 抖动、401/403 停止、429 处理、`Last-Event-ID`、连接状态、解析器帧边界。
- **类型**：生成的 DB 实体走 `@floatctf/sdk/entity`（与 API DTO 分开入口，避免混淆）；
  从路由页面反向 import 的 14 组 DTO 类型收敛到 `types/`。
- 运行时依赖只有 `axios`。

### `@floatctf/react`（`packages/react`，18 文件）

- 可选 headless 绑定：`createFloatCTFReact({ client, useUserToken, useAdminToken })`
  返回 `useAwdEventStream` / `useAdminAwdEventStream` / `useAwdpEventStream` /
  `useAwdpRunStream` / 4 个 query options 工厂 / `invalidateAwdQueries` + key 常量。
- 注入 token hook（`UseTokenSource`），因此**不绑定任何状态库**。
- 非测试目录**没有 `.tsx`**、没有 CSS、没有 Primer、没有路由、没有页面（门禁强制）。

### `@floatctf/frontend-runtime`（`packages/frontend-runtime`，16 文件）

- `parseFrontendManifest`（严格：未知字段拒绝、schemaVersion、安全 ID、semver、相对路径、
  穿越/scheme 拒绝、整数 major 兼容性）。
- `parseRegistry` / `resolveFrontend`（**显式 `currentVersion` 指针**，绝不按版本号排序挑选）。
- `mount(context)` 契约 + `normalizeFrontendModule`（容忍 default/具名导出）。
- `bootstrapFrontend()`（同源动态 import、样式注入、失败回退、兜底页）+
  `renderBootstrapEmergencyUi`（纯 DOM、真实诊断、无秘密）。
- 零运行时依赖。

---

## 4. Bootstrap

`apps/web` 现在只有一个 14.24 kB（gzip **5.07 kB**）的入口 + 极简 `index.html`：

```
GET /api/frontend                         → active_frontend / 版本 / capabilities
GET /__floatctf/frontends/registry.json   → 本地已安装前端（no-store）
?frontend=<id>                            → 破窗：只接受注册表里已安装的安全 ID
→ 兼容性校验（runtime / API 契约 major）
→ 注入样式（失败即移除）
→ 动态 import 同源 ESM
→ frontend.mount(context)
→ 失败回退 default → 再失败渲染内置兜底页
```

产物断言：`scripts/check-architecture.sh` 检查 `apps/web` 源码与 `dist` 都不含
`@floatctf/frontend-default` / React；实测 grep 为 0。

**开发模式**：`mise run dev` → `pnpm run build:packages` + `frontends/default` 的 Vite（:13000，HMR）。
DEV 路径显式（`frontends/default/src/dev.tsx` + `index.html` 注释说明），生产 bootstrap
**完全不 import** 默认前端，因此不存在"不小心把默认前端打进引导产物"的可能。

---

## 5. Default Frontend Migration

**结论：UI/UX 保持等价。** 迁移是"整目录 `git mv` + import 路径重写"，页面/组件/样式/导航/
Primer 用法未做任何重设计：

- `apps/web/src/**`（286 文件）→ `frontends/default/src/**`；
  页面代码只改 import 来源（`@/api/axios`→`@floatctf/sdk`、`@/entity`→`@floatctf/sdk/entity`、
  `@/hooks/useAwdEventStream`→`@/api/react`），`@/` 相对别名语义不变。
- UUID 路由、文件式路由树（TanStack Router）、布局、AWD/AWDP UX、Zustand 登录态
  （含既有 `auth-storage` localStorage 兼容）全部保留；**没有任何用户被登出**。
- `main.tsx` 拆成 `router.tsx`（路由实例，前端自己拥有）+ `entry.tsx`（`mount(context)`），
  渲染树、Provider 顺序、单例 QueryClient、StrictMode、styled-components
  `shouldForwardProp` 过滤器逐字保留。

**真实浏览器实测**（生产形态 Caddy + 真实 API + 真实数据，见 §18）暴露并修掉了一个真实回归：

- **症状**：默认前端整页起不来，bootstrap 兜底页显示 `default: process is not defined`。
- **根因**：Vite **lib 模式**刻意不替换第三方依赖（styled-components / mermaid）里的
  `process.env.*`（默认假设消费者会处理），而浏览器没有 `process`；旧 `apps/web` 是
  普通 app 构建，所以从未暴露。
- **修复**：`vite.config.ts` 的 `define` 显式替换 `process.env.NODE_ENV` / `process.env`，
  并加 `src/node-shim.ts`（`entry.tsx` 的第一个 import）兜底 Node 全局。
  副作用是 **包体积下降**：共享 chunk 5.03 MB → 4.32 MB（gzip 1.23 → **1.08 MB**），
  因为 `NODE_ENV=production` 让开发分支被 DCE 掉。
- 另外修掉选择器文案里漏出的 markdown `**`（已在构建产物中确认 0 处）。
- **`process.env` 的替换必须只作用于构建**：`frontends/default/vite.config.ts` 与
  vitest 共用同一份配置，如果无条件下 `define`，测试环境里的 React 会被解析成
  **production** 构建，而 `React.act` 只在 development 构建里导出 → 66 个组件测试
  全部报 `React.act is not a function`。已改为按 `process.env.VITEST` 分支：
  测试跑 development React，构建产物仍无 `process.env`（两条都已在真实产物/测试里确认）。

---

## 6. Backend Bootstrap API

- `GET /api/frontend`（**未认证**，登录前必须可用 —— 登录 UX 属于所选前端）。
  响应（实测）：

```json
{"code":0,"message":"OK","data":{
  "active_frontend":"default",
  "platform_version":"1.0.0",
  "api_contract_version":"1",
  "frontend_runtime_version":"1",
  "capabilities":["jeopardy","awd","awdp","discussions","writeups","web_terminal"]
},"meta":null}
```

- 只暴露 5 个公开字段；**不含**任意设置项、密钥、文件系统路径、注册表内容、资产路径。
- 非法 `FRONTEND_ACTIVE`（未过安全 ID 校验）一律回落 `default`，绝不回显未校验值。
- 能力标记来自 `apps/api/src/core/contract.rs` 的**白名单**（有单测断言其为小写标识符）。
- 实现位置 `apps/api/src/modules/platform/frontend/`（`api.rs` / `domain.rs` / `dto.rs`）。
- 后端**没有**新增 `loginPage` / `sidebar` / `eventLayout` / `routePrefix` 这类概念。

---

## 7. FRONTEND_ACTIVE

- 属于**既有动态设置体系**（`settings` 表 + `get_setting` + Redis 缓存失效），
  **不是**进程静态 TOML，**不是**环境变量。**无 schema 变更、无新迁移**。
- seed 默认 `default`，`protected = true`（**可编辑、不可删除**）。
  seed 结构为此补了 `protected` 字段，其余键保持 `false`（零行为变化）。
- 写入校验：create/patch 时若 key 为 `FRONTEND_ACTIVE`，值必须是安全前端 ID，否则 400。
- 实测（隔离库）：

| 操作 | 结果 |
|---|---|
| `PATCH {value:"cyberpunk"}` | `200`，`GET /api/frontend` 立即反映 |
| `PATCH {value:"../../etc/passwd"}` | `400 FRONTEND_ACTIVE 必须是安全前端 ID…` |
| `DELETE {id_list:[FRONTEND_ACTIVE]}` | `400 protected setting can not be deleted` |
| `PATCH {value:"default"}` | `200`，回落验证通过 |

---

## 8. Frontend Registry

生产文件系统布局（`FLOATCTF_HOME` 默认 `/var/lib/floatctf`）：

```
$FLOATCTF_HOME/
├── frontend.sh                         # 前端管理器（root:root 0755，随 release 发布）
├── web/                                # bootstrap 引导页（Caddy root /srv/web）
│   ├── index.html
│   └── assets/bootstrap-<hash>.js
└── frontends/                          # Caddy 只读挂载 → /srv/frontends
    ├── registry.json                   # 0644，原子写入，no-store
    ├── .registry.lock                  # 写入锁（file_server hide）
    ├── default/                        # 平台内置、受保护
    │   ├── 1.0.0/                      # 旧版本保留 → 可回滚
    │   │   ├── frontend.json
    │   │   └── assets/{frontend.js,frontend.css,<chunks>.js}
    │   └── 1.0.1/
    └── workspace/                      # 第三方（本次外部仓库验证）
        ├── 0.1.0/
        └── 0.2.0/
```

注册表要点：

- `schemaVersion` 版本化；每个 ID 一份 `versions` + **显式** `currentVersion` 指针；
  `protected` 标记（`default`）。
- **原子更新**：`flock` 串行化 + 同目录 tmp + `fsync` + `rename`。
- **多 ID 多版本共存**；**拒绝覆盖**同 ID 同版本（资产不可变）；同版本同内容 = 幂等跳过。
- 升级 = 装新版本 + 移指针；回滚 = 移指针回旧版本（实机验证见 §17）。
- `default` 由平台发布并受保护；只有 `install.sh` 的内部 `--platform` 可覆盖它。
- 生成的 `registry.json` 被 `@floatctf/frontend-runtime` 的**权威解析器**验证通过
  （每次浏览器加载都会重验；失败即回退并给出真实原因）。

---

## 9. Frontend Manager

`scripts/frontend.sh`（同时安装到 `$FLOATCTF_HOME/frontend.sh`，生产无需源码签出）：

```bash
frontend.sh help
frontend.sh list                                    # 列出前端与版本（* = 当前版本）
frontend.sh info <id> [version]
frontend.sh verify <artifact.tar.gz>                # 只校验，不安装
frontend.sh install <本地目录|Git URL|制品.tar.gz> [--ref][--node-image][--no-build][--make-current][--dry-run]
frontend.sh install <预构建目录> --platform --reinstall --make-current   # 平台重部署（内部）
frontend.sh remove <id> [version]                   # default 不可移除
frontend.sh set-current <id> <version>              # 回滚/固定当前版本
```

实测（真实脚本文本 + 真实制品）：

| 场景 | 结果 |
|---|---|
| `install` 发布制品 tar.gz（`--platform --make-current`） | 安装到 `default/1.0.0`，注册表生成 |
| `install` 预构建目录（release 解包后的 `frontends/default/<v>/`） | 正常安装 |
| 幂等重部署（同版本同内容） | "跳过资产复制（幂等重装）"，只更新注册表 |
| 同版本不同内容（无 `--reinstall`） | **拒绝**，提示"发布新版本号 / `--platform --reinstall`" |
| `--platform --reinstall` | 警告 + 同目录 rename **原子替换**（无 `.staging-*` 残留） |
| 第三方 `--reinstall` | **拒绝**（"第三方前端资产不可覆盖"） |
| `list` / `info` | 版本、指针、protected、entry/styles、契约、installedAt 全部真实 |
| `set-current` 旧版本 | 指针移动成功（回滚路径） |
| `remove` 第三方 | 删资产 + 删注册表条目（指针显式回退并打印） |
| `remove default` | **拒绝**（保护） |
| 恶意归档：`../evil.js` | **拒绝**（含 `..` 的归档成员） |
| 恶意归档：`/tmp/evil.js` | **拒绝**（绝对路径成员） |
| 恶意归档：符号链接 → `/etc/passwd` | **拒绝**（符号链接/硬链接） |
| manifest `id` = `../../evil` | **拒绝**（安全 ID） |
| manifest `entry` = `../escape.js` | **拒绝**（相对路径规则） |
| 契约不兼容（runtime 9） | **拒绝**（契约不兼容） |
| `entry` 声明但不存在 | **拒绝**（制品必须自包含） |

**源码构建隔离**（`install <源码目录|Git URL>`）：隔离 Docker 构建容器，
`--cap-drop ALL`、`--security-opt no-new-privileges`、`--pids-limit`、
以**调用者身份**运行（而非 root）、只读源码挂载 + 临时产物挂载、无 Docker socket、
不共享宿主 PID/网络命名空间；只复制源码（排除 `node_modules`/`dist`/`.git`）；
按 lockfile 识别 pnpm/npm/yarn 并把包管理器装进可写前缀；**只执行约定的 `build` 脚本**
（脚本名，不是 JSON 里的 shell 命令）。
实测：外部仓库（`pnpm` + `packageManager: pnpm@11.20.0`）→ 容器构建 → 制品校验 → 安装成功，
耗时约 16s。

---

## 10. External Repository Contract

外部仓库只需：

```
my-floatctf-frontend/
├── package.json                 # 依赖 @floatctf/sdk (+ 可选 @floatctf/react/@floatctf/frontend-runtime)
├── floatctf.frontend.json       # 源码 manifest（标识 + 兼容性 + build 期望）
├── vite.config.ts               # 自己的构建（产出 dist/frontend.json）
├── index.html                   # 仅 dev server
└── src/mount.ts                 # 导出 mount(context)；框架自由
```

- **不得** import `apps/web/*`、`frontends/default/*`、`@/routes` 等仓库私有路径
  （`scripts/check-architecture.sh` 的规则 4/6 会拦截）。
- **可以**依赖已发布的包；本地未发布时用 `pnpm pack` 的 tarball，**不要用 `workspace:*`**。
- 跨源开发用**既有 TOML** `[cors].allowed_origins`（不新增环境变量配置）。
- 源码 manifest 的 `build` 段**不会被执行主机命令**：只有受约束的脚本名 + 输出目录，
  且只在隔离容器内执行。

---

## 11. Artifact Format

- 构建产物目录必须自包含：`frontend.json` + 它声明的 `entry` / `styles`；不含源码、
  `node_modules`、pnpm 或 FloatCTF 仓库任何东西（前端的 JS 自带框架依赖；v1 **不**要求
  共享 React 单例/模块联邦）。
- 发布归档推荐 `*.tar.gz`；`web-dist.tar.gz` 布局固定为：

```
bootstrap/{index.html,assets/…}
frontends/default/<version>/{frontend.json,assets/…}
```

- `scripts/package-web-dist.sh`：确定性归档（固定 mtime/属主/排序），只含构建产物。
- `scripts/verify-release-frontend.sh`：断言 bootstrap 存在、manifest 通过**真实校验器**、
  entry/styles 真实存在、无源码/依赖残留、归档成员无穿越/绝对路径/特殊类型、
  三个脚本语法有效。实测 `OK`。

---

## 12. Caddy Integration

生产内嵌模板（`scripts/install.sh` 的 `write_caddy_template`）新增：

```caddyfile
@frontend_registry path /__floatctf/frontends/registry.json
handle @frontend_registry {
    root * /srv/frontends
    header Cache-Control "no-store"
    header Content-Type "application/json"
    rewrite * /registry.json
    file_server
}

handle_path /__floatctf/frontends/* {
    root * /srv/frontends
    header Cache-Control "public, max-age=31536000, immutable"
    header X-Content-Type-Options nosniff
    file_server { hide .registry.lock .staging-* }
}
```

Compose 里 Caddy 新增只读挂载 `${FLOATCTF_HOME}/frontends:/srv/frontends:ro`；
开发 Caddy 把 `/__floatctf/frontends/*` 显式指向 Default Frontend 的 Vite
（按真实 schema 提供只含 `default` 的开发注册表）。

实测（用 `install.sh` 内嵌模板提取出的**真实 Caddyfile** 起 Caddy，只把 API 上游改成宿主）：

| 请求 | 结果 |
|---|---|
| `GET /` | `200 text/html`（bootstrap，含 `bootstrap-*.js`） |
| `GET /api/frontend` | `200`，字段正确 |
| `GET /__floatctf/frontends/registry.json` | `200` + `Cache-Control: no-store` |
| `GET /__floatctf/frontends/default/1.0.1/assets/frontend.js` | `200` + `cache-control: public, max-age=31536000, immutable` + `text/javascript` |
| `GET .../assets/frontend.css` | `200` + immutable（1.2 MB） |
| 深路由 `/service/events/awd/<id>/scoreboard`、`/admin/settings`、`/workspace/xyz` | 全部 `200 text/html`（SPA 回退到 bootstrap） |
| `/__floatctf/frontends/../../etc/passwd`、`registry.json/../../Caddyfile` | `200 text/html`（返回**引导页**，未泄露文件；实测 body 无 `root:x:`） |
| `/__floatctf/frontends/.registry.lock`、`/default/.staging-*/frontend.json` | `404`（隐藏） |
| 嵌套 chunk（entry 的相对 import） | `200 text/javascript` |

---

## 13. Install / Upgrade / Uninstall Integration

**首次安装**（`scripts/install.sh`）：

1. 下载 **5 个** release 产物（新增 `frontend.sh`，`--frontend-manager-url` /
   `FLOATCTF_FRONTEND_MANAGER_URL` 与既有覆盖约定一致）；
2. 安装 `frontend.sh` 到 `$FLOATCTF_HOME/frontend.sh`（root:root 0755，`bash -n` 校验）；
3. `bootstrap/` 铺到 `$FLOATCTF_HOME/web`；
4. release 内 `frontends/<id>/<version>/` 经 `frontend.sh install … --platform --reinstall`
   安装（`default` 移动 current 指针）；
5. 校验注册表已生成并设权限；
6. python3 列入宿主前置依赖（前端管理器的 JSON 依赖；Arch 包列表已加）。

**重部署/升级**：更新 bootstrap；更新/安装新的 Default Frontend 版本；
**保留**第三方前端、其版本与其注册表指针；**不动** `FRONTEND_ACTIVE`；更新前端管理器。
实测：用安装器里**真实函数文本**跑首次安装 / 幂等重部署 / 「加入第三方前端后升级」
三种场景，第三方条目与版本列表前后完全一致。

**卸载语义**：

- SAFE UNINSTALL：移除 systemd/容器/API image/**bootstrap web**，但**保留**
  `$FCTF_ROOT/frontends`（含注册表，第三方前端与版本一并保留）与 `frontend.sh`
  —— 重新部署可恢复同一套前端集合；用户可见提示已更新列出这两项。
- PURGE：删除安装根（含 `frontends/` 与 `frontend.sh`）；提示里明确写出"已删除全部已安装
  前端与注册表以及前端管理器"。

---

## 14. Admin Frontend Selector

管理端 → 设置 页面顶部新增「前端（Frontend）」区块（`components/admin/FrontendSelector.tsx`）：

- 从 `/__floatctf/frontends/registry.json` 读**已安装**前端，并用
  `@floatctf/frontend-runtime` 的 `parseRegistry` 解析 —— 与 bootstrap 用**同一个**校验器，
  "页面能选"与"浏览器能加载"不会分叉；
- 读既有动态设置 `FRONTEND_ACTIVE`，保存走既有 `PATCH /api/admin/settings/{id}`（SuperAdmin）；
- 展示 id / 名称 / 版本 / 安装时间 / 入口 / 契约兼容性 / "平台内置"标记；
  **契约不兼容的候选在下拉里禁用并说明原因**；
- 保存成功后刷新 `["Settings"]` 缓存、显示 Primer `MsgBanner`（success）并启用"刷新页面"；
- 注册表读不到/不合法、设置接口失败 → **真实错误文案**，不造假数据；
- 文案明确写出破窗路径 `?frontend=default`；
- **不提供**任何安装能力（无 Git URL 输入、无构建、无上传、无"让后端去 clone"）；
- 无原生 `alert(`/`confirm(`（沿用 `useMsgBanner` / Primer 组件）。

实测（真实浏览器 + 真实数据）：选择 `workspace` → 保存 → 出现成功横幅
「已切换到前端「workspace」…请刷新页面使其生效」→ `GET /api/frontend` 立即变为
`active_frontend=workspace` → 点击"刷新页面" → 第三方前端接管界面。反向同样成功。

截图（本次验证实拍）：
- 管理端设置 + 前端选择器：`/tmp/fcft-ui-e2e/shot-admin-settings.png`
- 第三方前端（workspace，完全不同的界面）：`/tmp/fcft-ui-e2e/shot-external-frontend.png`

---

## 15. Break-glass Recovery

- 任意页面加 `?frontend=default`：
  - 只影响**当前这次页面加载**；
  - **不修改**后端 `FRONTEND_ACTIVE`（实测：破窗后 `GET /api/frontend` 仍为 `workspace`）；
  - **不需要认证**；
  - 值必须命中本地注册表的已安装 ID（未安装/不安全值被忽略并记入诊断）。
- 实测完整恢复链：`FRONTEND_ACTIVE=workspace` → `?frontend=default` 打开官方 UI →
  管理端（SuperAdmin 已登录）→ 设置 → 前端选择器 → 选 `default` → 保存并切换 →
  `GET /api/frontend` 回到 `default` → 正常加载官方前端。
- 连 Default 都失败时：内置兜底页（纯 DOM，无框架），显示平台版本 / 契约版本 /
  `FRONTEND_ACTIVE` / 破窗参数 / 已尝试的前端 / **真实失败原因**，并给"重试"按钮。
  本次验证**真的触发过**它（`default: process is not defined`，见 §5），行为符合预期。

---

## 16. Security Review

逐项对照（实现细节见 [ARCHITECTURE.md §8](../docs/frontend/ARCHITECTURE.md)）：

| 项目 | 结论 |
|---|---|
| 制品解包路径穿越 / 绝对路径 | ✅ 解包**前**逐条校验 tar 成员；实测 `../evil.js`、`/tmp/evil.js` 均被拒 |
| tar 符号链接/硬链接逃逸 | ✅ 拒绝 `l`/`h` 及特殊类型；实测 symlink→`/etc/passwd` 被拒 |
| 恶意前端 ID | ✅ `[a-z0-9][a-z0-9._-]*`（≤64），后端 + CLI + 运行时三处校验 |
| 覆盖 / 删除 Default 前端 | ✅ `protected`；CLI 拒绝 `install`/`remove`；仅 install.sh `--platform` |
| 注册表原子性 | ✅ `flock` + tmp + `fsync` + `rename`；只读方永远看到完整文件 |
| 构建容器宿主挂载 | ✅ 只读源码 + 临时产物；调用者身份；无额外 capabilities |
| Docker socket 暴露 | ✅ 构建容器不挂载任何 socket |
| 宿主 root 命令执行 | ✅ 只跑 `<pm> run <约定脚本名>`；不从 JSON 取任意命令 |
| 远端 JS 运行时 import | ✅ 只 import 注册表推导出的同源路径；破窗值走白名单 |
| Caddy 资产穿越 | ✅ `handle_path` + 根限定；`..` 规范化后落入 SPA 回退（返回引导页，无文件泄露） |
| 内部文件外泄 | ✅ `file_server hide` 隐藏 `.registry.lock` / `.staging-*`（实测 404） |
| 设置更新鉴权 | ✅ 沿用 `PATCH /api/admin/settings/{id}`（SuperAdmin 守卫） |
| 公开 bootstrap 泄露 | ✅ 只回 5 个公开字段；非法设置值回落 `default`，不回显 |
| 第三方前端信任说明 | ✅ 三份文档 + 信任模型小节明确写出"= 安装可信应用代码，不是沙箱" |
| 浏览器端安装副作用 | ✅ Admin UI 无安装/克隆/构建入口；安装只属运维 CLI |
| 无原生弹窗 | ✅ `grep -rn 'alert(\|confirm(' frontends/default/src` = 0 |

补充：前端资产安装后 `chmod -R a-w`（运行期不可写，删除时先恢复可写）；
`getUserToken`/`getAdminToken`/`onUnauthorized` 全为注入回调（SDK 不导航、不写存储）；
`FloatCTFError` 不含任何凭据内容（`toJSON` 只导出状态/码/文案）。

---

## 17. External Repo Validation

**位置**：`/tmp/fcft-external-frontend`（**仓库之外**；未 push 到任何远端，也未在仓库内留副本）。

**独立性证据**：

```
$ grep -rnE "apps/web|frontends/default|@/routes|@/components|@floatctf/sdk/src|\.\./\.\./\.\./floatctf" src/ vite.config.ts index.html
无任何主仓库路径引用 ✓

$ python3 -c "import json;d=json.load(open('package.json'));print(d['dependencies'])"
{'@floatctf/frontend-runtime': 'file:./packages/floatctf-frontend-runtime-1.0.0.tgz',
 '@floatctf/sdk': 'file:./packages/floatctf-sdk-1.0.0.tgz'}
```

- 依赖 = `pnpm pack` 出来的**本地 tarball**（`workspace:` 协议已被替换为真实版本 `1.0.0`）；
  未使用 `workspace:*`。
- 无 React、无 `@floatctf/react`、无 Primer、无 Tailwind（**纯 TypeScript + DOM**）——
  同时证明 `@floatctf/react` 可选、运行时契约与框架无关。
- 结构**刻意与官方前端不同**：自己的路由空间 `/workspace` 与 `/workspace/<event-id>`、
  自己的登录 UX（内联表单）、自己的会话存储（`sessionStorage`，官方是 localStorage）、
  自己的手写深色样式。

**装的包只含 dist**：三个 tarball 均无 `src/`、无 `node_modules`，含 README，
`workspace:` 已替换。

**端到端链路（全部实测通过）**：

```
临时外部仓库
  │  frontend.sh install（源码目录）
  ▼
隔离容器构建（cap-drop ALL / no-new-privileges / 非 root / 只读源码 / 无 socket）
  │  产物：dist/{frontend.json,assets/app.js,assets/app.css}
  ▼
制品校验（manifest 契约 / 安全 ID / 相对路径 / entry+styles 存在）
  ▼
版本化安装（frontends/workspace/0.1.0 → 后来 0.2.0）
  ▼
注册表原子更新（权威解析器验证通过）
  ▼
FRONTEND_ACTIVE=workspace（**通过真实 Admin UI**）
  ▼
浏览器加载第三方前端：自有登录 UX → 登录成功 → 赛事列表（真实数据）→
AWD 状态只读读取（`awdPlayerApi.status`）返回真实 JSON → 深路由 `/workspace/<id>` 整页刷新正常
  ▼
恢复 FRONTEND_ACTIVE=default（**通过真实 Admin UI**）→ 官方前端恢复
```

**过程中发现并修掉的真实缺陷**（外部前端自身）：路由器未处理"不属于自己的路径"
（服务端 SPA 回退把 `/admin/settings` 交给它 → 被当作 eventId → `GET /api/events/admin` 400）。
修复为"非自有路径空间 → 归正到自己的首页"，升级为 0.2.0 后复验通过
（同时实测了第三方前端的**升级路径**：0.1.0 与 0.2.0 共存、指针前移）。

**未留下临时凭据**：外部仓库只用本地测试账号（`1000000/testuser`、
`sysadmin/FloatCTF@2025`，均为仓库既有文档化开发凭据），未写入任何文件。

---

## 18. Default Frontend Regression Validation

环境：生产形态（`install.sh` 内嵌 Caddyfile 提取的真实配置 + 真实 Caddy 容器 +
真实 API + 隔离数据库 + 真实事件/题目/队伍数据），真实浏览器（bsk/Edge）实测。

| 流程 | 结果 |
|---|---|
| 公开落地页 / 登录页 | ✅ `Sign in to FloatCTF`、学号占位、注册/忘记密码链接齐全 |
| 用户登录 | ✅ `1000000/testuser` 登录成功，进入 Top 页，导航完整 |
| 用户登出 | ✅ 回到登录页 |
| 管理端登录 | ✅ `sysadmin` → 管理端仪表盘（4 Users / 4 Events / 4 Challenges） |
| 赛事列表 | ✅ 真实表格 + 分页 + FilterBar |
| 赛事详情（Jeopardy） | ✅ 标题、Repository 导航、倒计时（`Ends in 5h 59m …`） |
| Jeopardy 挑战页 | ✅ 题目卡片 + 分值/解出数/已解出排序 + 分类筛选 |
| 题目详情 | ✅ 对话框：Current Score 300.00 / Solved ❌ / 内容 / Start 按钮 |
| 计分榜 | ✅ 真实列（排名/名称/Score/已解出/各题列） |
| 当前 AWD 选手页 | ✅ 事件导航（Overview/GameBoxes/Scoreboard/WireGuard/SSH）+ 真实积分表（蓝队） |
| AWD 实时行为 | ✅ `GET /api/events/<id>/awd/stream` → **200**（fetch-SSE + Bearer 头） |
| AWD 实时重连 | ✅ 停 API：页面不崩、轮询回退（502 重试）；恢复 API 后**自动重连**新 `awd/stream` **200** |
| 管理端赛事页 | ✅ AWD 管理页：标题+#id、Configuring、进度条、完整事件导航 |
| AWD 管理操作 | ✅ 生命周期控件真实存在（部署/预检/轮换内部令牌/分数调整/排行榜）——**只读未点击** |
| 设置页 | ✅ 侧栏 + 设置表（含新 `FRONTEND_ACTIVE` 行）+ 前端选择器 |
| 深路由刷新 | ✅ `/service/events/jeopardy/<id>/scoreboard`、`/admin/settings` 等整页刷新后由 bootstrap → 默认前端接管 |
| 浏览器控制台 | ✅ 无应用级错误（仅浏览器扩展自身的 `chrome-extension://invalid` 噪声） |
| SSE 重连语义（单元） | ✅ 15 个 `connectSse` 用例 + 3 个 hook 用例（迁移后逐字保留） |

**不造假**：以上全部来自真实接口与真实数据；本次为验证在**隔离库**中创建了
1 个解题赛（挂 4 道现有题并发布）、1 个攻防赛（仅配置，**未** deploy/start）

- 1 支队伍，并把选手加入两者（`FRONTEND_ACTIVE` 前后均恢复为 `default`）。

---

## 19. CI / Release Changes

**迁移后的门禁对齐**（迁移会"移动文件"，因此必须同步所有**引用前端源码路径**的门禁）：

| 门禁 | 处理 |
|---|---|
| `apps/api/tests/awdp_practice_judge.rs`（源码边界测试） | 前端页面路径 → `frontends/default/src/routes/admin/events/awdp.$id/judge.tsx`；`awdp.ts` → `packages/sdk/src/api/awdp.ts`（测试更严格：同时覆盖前端页面与 SDK 模块） |
| `scripts/gen_web_types.py` | `OUTPUT_DIR` → `packages/sdk/src/entity`（实体属 SDK，前端从 `@floatctf/sdk/entity` 引用）；注释/报错文案同步；并让生成器输出**显式 `.js` 扩展名**（SDK 以 ESM 发布，tsc 不改写相对导入，缺少扩展名时 `@floatctf/sdk/entity` 在 Node ESM 下无法解析） |
| biome | 三个新包各加 `biome.json`（tab + double quote，与仓库既有风格一致；仅保留 `recommended` 规则，不放松），并修掉迁移代码里 14 处 lint 诊断（unused template literal / useImportType / 与 axios 对齐的 `any` 显式豁免） |
| `scripts/check-architecture.sh` | 新增（7 条边界规则 + 契约版本一致性） |

**顺带修掉一处既有漂移**：按任务要求"经官方任务重新生成并确认无意外漂移"地跑
`mise run db:gen:ts` 后，生成器报出 `awd_reset_records` 少一个字段
（`requested_by_admin`）。核对确认这是**迁移前就存在**的漂移：列存在
（migration `20260914120000-awd-reset-requester-admin.sql`）、Rust 实体有该字段、
但 `main` 上的 `apps/web/src/entity/awd_reset_records.ts` 缺它（当时没有重跑生成器）。
本次按铁律 #4（Schema / 生成实体 / 代码三处一致）把它修正，并且现在
`mise run db:gen:ts` 的输出与提交内容**字节一致**（幂等）。

**`.github/workflows/ci.yml`（`fast-web`）**：安装依赖 → `pnpm run build:packages`
→ `scripts/check-architecture.sh`（架构边界 + 契约版本一致性）→ 五个包 biome lint
→ `pnpm run test:web`（**300** 个测试）→ `pnpm run build:web`（bootstrap + Default Frontend）
→ `package-web-dist.sh` + `verify-release-frontend.sh`（发布形态验证）。
Rust/安全 grep/集成测试作业**未削弱**。

**`.github/workflows/release.yml`**：前端构建改为「先构建 packages 再构建 web」；
发布 **5** 个产物（新增 `frontend.sh`）；`web-dist.tar.gz` 由
`scripts/package-web-dist.sh` 按新布局组装；新增
"Verify frontend release artifacts" 步骤（`verify-release-frontend.sh`）；
API/helper 产物语义不变。

---

## 20. Files Moved / Deleted

**移动（275 renames，`git mv` 保留历史）**

| 从 | 到 |
|---|---|
| `apps/web/src/{components,routes,navigation,stores,integrations}`（含 `routeTree.gen.ts` 生成物不进 git） | `frontends/default/src/…` |
| `apps/web/src/{config.ts,logo.svg,style.css,util.ts,reportWebVitals.ts}` | `frontends/default/src/…` |
| `apps/web/src/entity/**`（61 文件） | `packages/sdk/src/entity/**` |
| `apps/web/src/lib/sse/**`（5 文件，含测试） | `packages/sdk/src/sse/**` |
| `apps/web/src/api/{admin,service}/**`、`awd.ts`、`awdp.ts`、`awdpRuns.ts`（57 文件） | `packages/sdk/src/api/**` |
| `apps/web/src/types/challengeDto.ts` | `packages/sdk/src/types/challengeDto.ts` |
| `apps/web/src/hooks/{awdInvalidation,useAwdEventStream,useAdminAwdEventStream,useAwdpEventStream,useAwdpRunStream}.ts` + 测试 | `packages/react/src/**` |
| `apps/web/src/api/queries/**`（4 文件 + 测试） | `packages/react/src/queries/**` + 测试 |
| `apps/web/src/api/index.ts` | `frontends/default/src/api/index.ts`（改为 SDK 门面 + 本前端接线） |
| `apps/web/.cta.json`（脚手架标记，描述的是前端而非引导页） | `frontends/default/.cta.json` |

**删除（10）**

```
apps/web/src/api/axios.ts                              → packages/sdk/src/transport.ts
apps/web/src/api/__tests__/axiosInterceptors.test.ts   → packages/sdk/src/__tests__/transport.test.ts（按新契约重写）
apps/web/src/main.tsx                                  → frontends/default/src/{router.tsx,entry.tsx}
apps/web/src/api/admin/index.ts                        → packages/sdk/src/api/index.ts（聚合移入 SDK）
apps/web/src/api/service/index.ts                      → packages/sdk/src/api/index.ts
apps/web/src/api/queries/index.ts                      → packages/react/src/queries/index.ts
apps/web/src/api/queries/__tests__/query-keys.test.ts  → packages/react/src/__tests__/query-keys.test.ts
apps/web/src/entity/index.ts                           → packages/sdk/src/entity/index.ts（重生成）
apps/web/src/lib/sse/index.ts                          → packages/sdk/src/sse/index.ts（重生成）
```

**新增**：`packages/{sdk,react,frontend-runtime}`、`frontends/default`、
`apps/web/src/main.ts`、`apps/api/src/{core/contract.rs,modules/platform/frontend/*}`、
`scripts/{frontend.sh,package-web-dist.sh,verify-release-frontend.sh,check-architecture.sh}`、
`docs/frontend/{ARCHITECTURE,DEVELOPING,ARTIFACT}.md`。
**没有任何永久兼容别名**（旧 `@/api/axios`、`apps/web/src/*` 的转发层一律不存在）。

---

## 21. Commits

| # | commit | 内容 |
|---|---|---|
| 1 | `240567a` | `refactor(frontend)`: 抽出 `@floatctf/{sdk,react,frontend-runtime}`，当前 UI 迁入 `frontends/default`；`apps/web` 变 bootstrap；新增架构门禁与契约常量 |
| 2 | `b0b52b1` | `feat(api)`: 未认证 `GET /api/frontend` + `FRONTEND_ACTIVE` 动态设置（protected、写入校验） |
| 3 | `afd82df` | `feat(deploy)`: `scripts/frontend.sh` + 注册表 + Caddy/install.sh/clean.sh/CI/release 全面接入 |
| 4 | `97249a9` | `feat(admin)`: 设置页「已安装前端」选择器（只选择，不安装） |
| 5 | （本报告） | `docs(frontend)`: 三份前端文档 + README/AGENTS/INSTALL/agents 文档同步 + 实现报告 |
| 5b | `ee1ca31` | `fix(frontend)`: 产物 `process` 依赖修复 + 迁移后门禁对齐（biome 配置、14 处 lint、源码边界测试、类型生成目录与幂等、mise 任务顺序） |
| 5c | `552ae2f` | `fix(deploy)`: 隔离容器构建加固（调用者身份 / cap-drop ALL / npm 前缀装包管理器 / Node 基线对齐） |
| 6 | `bfd5677` | `docs(frontend)`: 三份前端文档 + README/INSTALL/AGENTS/agents 同步 + 本报告 |

全部为**本地提交，未 push**。

---

## 22. Remaining Risks

1. **默认前端 JS 体积**：共享 chunk 4.32 MB（gzip **1.08 MB**）。迁移本身让 gzip 从 1.23 MB
   降到 1.08 MB（`NODE_ENV` 替换带来的 DCE），但**首屏仍然偏大**（mermaid/katex/xterm 等在
   静态图内）。这是既有问题（HANDOFF 早已记载），未在本任务范围内做 code-split 优化。
2. **同一版本替换资产对浏览器不可见**：版本化资产是 `immutable` 长缓存，`--reinstall`
   同版本换内容时浏览器可能仍用旧缓存（本次验证就撞到过，靠升版本号解决）。
   生产语义正确（升级 = 新版本号），但运维需知道"同版本替换不等于用户能看到"。
3. **`frontend.sh` 依赖宿主 `python3`**：用于 JSON 解析与原子注册表更新。
   生产安装器已把它列入前置依赖与 Arch 包列表；极简主机需自行安装。
4. **外部前端构建需要网络**：容器内通过 npm 安装 pnpm 并下载依赖；离线宿主需自备镜像/代理。
   构建容器默认镜像 `node:26-bookworm`（首次会拉取）。
5. **单客户端约束**：SDK 的 API 模块通过"当前绑定的 HTTP handle"工作，一个页面只支持一个
   绑定客户端（bootstrap 每次只挂载一个前端，因此符合现实）。同时创建两个不同认证上下文的
   客户端不受支持（已在 `client.ts` 明确记录）。同浏览器标签内热切换前端也**不**支持
   （设计上要求刷新页面）。
6. **`registry.json` 版本条目允许附加字段**：工具链可写 `source` 等元数据，运行时会忽略；
   这意味着"注册表条目的额外字段"不是契约的一部分（有意为之，已在 ARTIFACT.md 说明）。
7. **前端签名/来源校验未实现**：制品没有签名链，信任完全建立在"运维安装 + 同源提供"
   之上（与信任模型一致，但值得未来考虑）。
8. **浏览器验证使用隔离库与本地 Caddy**：未在生产 Compose / 真实域名 + HTTPS 下跑一遍
   （本机同时跑着生产栈，按 HANDOFF 的告诫未启动 `mise run dev` 以免与生产争抢宿主资源）。
   生产形态的静态路由、缓存头、SPA 回退、registry/资产服务均已用**真实 Caddyfile** 验证。

---

## 23. Final Verdict

**最终门禁证据**（本分支 HEAD 上实测）：

| 门禁 | 结果 |
|---|---|
| `CI=true pnpm install --frozen-lockfile` | ✅ lockfile 一致（676 entries 通过供应链策略校验） |
| `bash -n` × 6 个脚本（frontend/install/clean/package-web-dist/verify-release-frontend/check-architecture） | ✅ 全部通过 |
| `mise run check` | ✅ **exit 0**（cargo fmt + clippy --workspace --all-targets --all-features + 5 包 biome lint + web:typecheck + web:architecture + Rust 全量测试 + web 测试） |
| `mise run build` | ✅ **exit 0**（`cargo build --workspace --release` + packages + bootstrap + Default Frontend 制品；`target/release/floatctf` 47.5 MB） |
| Web 测试 | ✅ 25 个文件 / **300** 个用例全绿（frontend-runtime 66、sdk 51、react 10、frontends/default 173） |
| Rust 测试 | ✅ 61 个 suite 全绿（含源码边界测试 `push_flow_removed_from_source`） |
| `mise run db:gen:ts` | ✅ 幂等（生成结果与提交内容字节一致） |
| `scripts/check-architecture.sh` | ✅ OK（7 条边界 + 契约版本一致性） |
| `scripts/verify-release-frontend.sh` | ✅ OK（真实校验器验证发布制品） |
| `git diff --check` | ✅ 无空白/冲突标记问题 |
| 未 push | ✅ `git log main..HEAD` 的 6 个提交全部只在本地 |

**PASS**

- 三个包（`@floatctf/sdk` / `@floatctf/react` / `@floatctf/frontend-runtime`）边界由自动化门禁强制，
  测试 300 个（前端）+ Rust 全量门禁通过；依赖方向被掰正（不再有 `api → routes` 倒挂）。
- 当前 UI 以 `frontends/default` 形式迁移，**UI/UX 等价**（真实浏览器逐流程验证），
  迁移暴露的唯一真实回归（lib 模式 `process` 未替换）已定位、修复、复验。
- `apps/web` 成为 14 KB / gzip 5 KB 的纯引导页（无 React），
  生产加载链（`/api/frontend` → 本地注册表 → 同源版本化 ESM → `mount`）在真实 Caddy 下逐项验证。
- `FRONTEND_ACTIVE` 动态设置 + 未认证引导端点 + 文件系统注册表 + 隔离构建的前端管理器 +
  Caddy/安装器/卸载/CI/release 全部落地并实测。
- 第三方前端的**外部仓库端到端验证**完成（独立仓库、非 React、自己的路由空间、
  packed 包、隔离容器构建、版本化安装、注册表更新、Admin UI 切换、破窗恢复）；
  过程中发现的外部前端缺陷已修复并复验。
