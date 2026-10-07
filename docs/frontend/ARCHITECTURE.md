# FloatCTF 可插拔前端平台（架构）

> 本文是**前端平台**的权威架构文档。
> 相关：[DEVELOPING.md](./DEVELOPING.md)（开发与外部仓库流程）、[ARTIFACT.md](./ARTIFACT.md)（制品 / manifest / 注册表 / 版本策略）。
> **从零创建一个新前端**：[AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md)（AI 代理作业手册）、[CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md)（能力矩阵——完整性口径由它定义，而非路由对齐）。
> 后端整体架构见 [../agents/ARCHITECTURE.md](../agents/ARCHITECTURE.md)。

## 1. Frontend ≠ Theme

| 概念 | 定义 |
|------|------|
| **Frontend（前端）** | 一个**完整的、可替换的浏览器应用**：自己的框架、路由、路由路径、页面层级、布局、导航、登录 UX、选手 UX、管理端 UX、赛事/Jeopardy/AWD/AWDP UX、组件、设计系统、样式、动画、UI 状态与信息架构。 |
| **Theme（主题）** | 某个前端**自己内部**可选实现的东西（例如 light/dark）。平台层不存在"主题"这个契约。 |

因此不同前端**不要求**有相同的页面、布局、路由路径或交互流程。
例如官方前端用 `/service/events/awd/<id>/scoreboard`，另一个前端完全可以用
`/workspace/<id>` —— 后端不关心浏览器路由，`frontend-runtime` 也不关心。

## 2. 依赖方向（唯一允许的方向）

```
                      FloatCTF Backend (apps/api)
                       │  REST / SSE / Bearer
                       ▼
                @floatctf/sdk                        packages/sdk
                框架无关：传输 / 错误模型 / 领域客户端 / DTO / SSE
                       │
        ┌──────────────┴───────────────┐
        ▼                              ▼
@floatctf/react                  （非 React 前端直接使用 sdk）
packages/react                   Vue / Svelte / Solid / 原生 TS …
可选 headless 绑定
        │
        ▼
Frontend implementation
  ├── frontends/default             官方前端（当前完整 UI，随平台发布）
  └── 外部仓库                       第三方前端（完全独立，可换成任意框架）

                @floatctf/frontend-runtime        packages/frontend-runtime
                制品 / 运行时契约（框架无关，不依赖上面任何 UI 包）
                       ▲
                       │ 被 apps/web bootstrap 与所有前端共同使用
                apps/web（bootstrap 引导页，无框架）
```

硬性规则（由 `scripts/check-architecture.sh` 在 CI 中**失败即红**地强制）：

1. `@floatctf/sdk` **不得**依赖 React / 路由 / 状态库 / Primer / Tailwind / 任何应用页面。
2. `@floatctf/react` **不得**包含任何 UI（组件、Primer、Tailwind、CSS、路由、页面、JSX）。
3. `@floatctf/frontend-runtime` **不得**依赖任何 UI 框架或 SDK。
4. `frontends/default` **不得** import `apps/web` 或逃出自身包的仓库路径。
5. `apps/web`（bootstrap）**不得** import 默认前端或 React（生产产物同样会被断言）。
6. `packages/*` **不得**反向 import `apps/*` 或 `frontends/*`。
7. 后端 `API_CONTRACT_VERSION` 必须与前端运行时声明一致（防止版本漂移）。

> 历史教训：迁移前 `apps/web/src/api/*` 反向 import 路由页面里的 DTO 类型
> （`api → routes` 的倒挂）。这些类型现在住在 `@floatctf/sdk` 的 `types/`，
> 页面改为从 SDK 引入，方向被掰正。

## 3. 代码位置

| 路径 | 角色 |
|------|------|
| `apps/web` | **bootstrap 引导页**：解析已安装前端 → 动态加载 → 回退 → 兜底页。无 React、无 Primer、无路由（产物约 14 KB / gzip 5 KB）。 |
| `frontends/default` | **官方前端**：迁移前后的当前完整 UI（React + TanStack Router + Primer + Tailwind）。它是**普通前端**，走与第三方完全相同的制品/运行时契约。 |
| `packages/sdk` | `@floatctf/sdk`：传输（axios + Bearer 注入 + 错误归一化 + 401 回调）、领域 API 客户端、DTO、fetch-based SSE、生成实体（`/entity` 子路径）。**客户端按实例隔离**：没有模块级"当前 transport"，`createFloatCTFClient()` 返回的每个实例各自拥有 base URL / token 来源 / 领域门面。 |
| `packages/react` | `@floatctf/react`：**可选** headless React 绑定（query 工厂、SSE hook、失效工具）。 |
| `packages/frontend-runtime` | `@floatctf/frontend-runtime`：manifest 校验、注册表校验与解析、`mount(context)` 契约、bootstrap 加载器与兜底 UI。 |
| `scripts/frontend.sh` | 前端管理器（安装 / 列举 / 检查 / 移除 / 切换当前版本）。生产安装到 `$FLOATCTF_HOME/frontend.sh`。 |
| `scripts/package-web-dist.sh` / `scripts/verify-release-frontend.sh` | 发布产物组装与验证。 |
| `scripts/check-architecture.sh` | 架构边界与契约版本门禁。 |

## 4. 运行时契约

### 4.1 制品 manifest（`frontend.json`）

见 [ARTIFACT.md](./ARTIFACT.md)。要点：**字段很少**，只声明"是什么、吃什么版本、从哪加载"。
**不声明**路由、页面、框架、布局或登录页。

### 4.2 挂载契约（`mount(context)`）

```ts
import type { FloatCTFFrontendModule } from "@floatctf/frontend-runtime";

export function mount(context: FloatCTFMountContext): void | (() => void) {
  // 建自己的框架/路由/状态/样式，渲染进 context.root
}
```

`FloatCTFMountContext` 刻意很小，且**只有平台能力**：

| 字段 | 含义 |
|------|------|
| `root` | 渲染宿主（bootstrap 保证已清空） |
| `apiBaseUrl` | API 基址（同源，例如 `/api`） |
| `assetBaseUrl` | 本前端制品根目录的同源路径（加载自己的图片/字体用） |
| `frontendId` / `frontendVersion` | 当前前端标识与版本（诊断） |
| `platformVersion` | 平台版本（诊断） |
| `apiContractVersion` / `frontendRuntimeVersion` | 两个契约 major |
| `capabilities` | 平台真实且稳定的能力标记（白名单在后端 `core/contract.rs`） |

**不得**出现在 context 里：Router、React Query、Primer、当前页面/路由概念、任何状态库。
前端要读 URL 就自己读 `window.location`（路由是它自己的事）。

### 4.3 实时（SSE）与 base URL

`@floatctf/react` 的四个实时 hook **不接受** URL 参数，而是从传入的 `client` 派生：

| hook | 传输 | 最终 URL |
|------|------|----------|
| `useAwdEventStream` | `client.sse.connect` | `${client.baseUrl}/events/<id>/awd/stream` |
| `useAdminAwdEventStream` | `client.sse.connectAdmin` | `${client.adminBaseUrl}/events/<id>/awd/stream` |
| `useAwdpEventStream` | `client.sse.connect` | `${client.baseUrl}/events/<id>/awdp/stream` |
| `useAwdpRunStream` | `client.sse.connect` | `${client.baseUrl}/service/awdp/runs/<id>/stream` |

因此 REST 与 SSE 永远走**同一个**已配置地址（跨源/自建 base URL 的外部前端不会出现
"REST 正确、实时通道打错源"）。Bearer 仍走 `Authorization` 头，管理端用 admin token。

### 4.4 加载流程

```
Browser
  │
  ▼
apps/web bootstrap（index.html + 一个入口 chunk）
  │  1. GET /api/frontend                      → active_frontend / 版本 / capabilities
  │  2. GET /__floatctf/frontends/registry.json → 本地已安装前端（no-store）
  │  3. ?frontend=<id> 只接受注册表里**已安装**的安全 ID（破窗）
  │  4. 客户端再次校验兼容性（runtime / API 契约 major）
  │  5. 注入该前端的样式（<link>，失败即移除）
  │  6. 动态 import 同源 ESM（/__floatctf/frontends/<id>/<version>/...）
  │  7. frontend.mount(context)
  │  8. 任一步失败 → 回退 default → 再失败 → 内置兜底页（纯 DOM + 真实诊断）
  ▼
Frontend（default 或第三方，任意框架）
```

安全不变量：**永不**动态 import 任意远端 URL；只 import 由"已校验相对路径 + 同源前缀"
拼出的地址。破窗参数只做白名单解析，绝不直接拼进路径。

## 5. 注册表与版本解析

- 注册表文件：`$FLOATCTF_HOME/frontends/registry.json`，由 `scripts/frontend.sh`
  **原子**写入（tmp + fsync + rename，`flock` 串行化）。
- 每个前端 ID 一份 `versions` 映射 + 一个**显式** `currentVersion` 指针。
- 解析规则：
  - 用 `currentVersion`，**绝不**按文件名/版本号排序挑"最高版本"
    （`9.0.0` 与 `10.0.0` 的字典序会随机换前端）。
  - `currentVersion` 始终指向一个已安装版本（删除时显式回退并打印）。
  - 兼容性按 **major** 判定（v1 的契约只支持整数 major，语义确定、可测）。
- 语义：
  - **安装新版本** = 新目录 `<id>/<version>/` + 注册表新增条目（不动指针，除非显式要求）。
  - **升级** = 安装新版本 + 移动指针。
  - **回滚** = 指针移回旧版本（旧版本目录仍在）。
  - **资产不可变**：同 ID 同版本已存在且内容不同 → 拒绝覆盖；`default` 由平台发布，
    重部署也只能安装**新的前端版本**（`--platform` 允许操作 `default`，但不允许改写
    已存在的版本目录）。
- `FRONTEND_ACTIVE`（数据库动态设置）只存**前端 ID**；"用哪个版本"由注册表指针决定。
  两层职责不重叠。

## 6. 版本策略（三个独立概念，禁止合并）

| 版本 | 常量 | 何时 +1 |
|------|------|---------|
| HTTP API 契约 | `API_CONTRACT_VERSION`（`apps/api/src/core/contract.rs` + `packages/frontend-runtime/src/version.ts`，由门禁断言一致） | 前端可见的**破坏性** API 变更 |
| 前端运行时契约 | `FRONTEND_RUNTIME_CONTRACT_VERSION` / `FRONTEND_RUNTIME_VERSION` | 破坏 `frontend.json` 或 `mount(context)` |
| 注册表 schema | `FRONTEND_REGISTRY_SCHEMA_VERSION` | 注册表结构不兼容变更 |
| 平台自身版本 | `Cargo.toml` / `package.json` | 按仓库既有发版策略（**与上面三者无关**） |

**加法式变更不升 major**：新增可选字段、新增端点、新增响应字段都留在当前 major。
只有"旧前端会因此坏掉"的变更才升 major，并同步两边声明（门禁会红）。

## 7. 信任模型（必读）

> **第三方 FloatCTF 前端是可信应用代码（trusted application code）。**

它运行在用户的浏览器里、位于 **FloatCTF 源**下，会通过正常 API 调用接触到用户/管理员的
凭据与令牌。因此：

- 安装一个第三方前端 **等同于安装可信应用代码**；
- 构建隔离（容器）降低的是**宿主**风险，**不会**让产物变得可信；
- 平台**不**把"任意前端安装"宣传成沙箱；
- **不**在运行时直接加载任意远端 JavaScript URL；
- 生产前端必须**先安装到本地**，再由 FloatCTF 源提供（同源）。

（本任务范围内**不**实现前端市场，也**不**实现浏览器端安装。）

## 8. 安全审查清单（本实现逐项对照）

| 风险 | 处理 |
|------|------|
| 制品解包路径穿越 / 绝对路径 | 解包**前**逐条校验 tar 成员，拒绝 `..`、`/` 开头、反斜杠与特殊文件类型 |
| 目录形态制品夹带符号链接/特殊文件 | **任何来源**（归档 / 源码构建 / 预构建目录 / 安装暂存树）在安装前都跑 `validate_artifact_tree`（`lstat` 语义）：拒绝符号链接、硬链接、FIFO、socket、设备文件；`frontend.json` / `entry` / `styles` 必须是普通文件 |
| 公开注册表泄露运维元数据 | 注册表是**公开静态树**的一部分：`registry.json` 只允许白名单字段（无 `source`/路径/Git URL/凭据），CLI 每次写盘都 sanitize 并自检（`check-public`），运行时解析器对未知字段 fail-closed |
| 源码构建以 root 运行 | 构建容器**绝不**使用 UID 0：sudo 场景用 `SUDO_UID/SUDO_GID`，普通用户用其自身，root 直调时用专用非特权身份（默认 `65534:65534`）；`docker run` 前断言 `uid != 0` |
| tar 符号链接逃逸 | 拒绝符号链接与硬链接成员 |
| 恶意前端 ID | ID 必须匹配 `[a-z0-9][a-z0-9._-]*`（≤64），后端与 CLI 都校验 |
| 覆盖 default 前端 | `default` 受保护：常规 `remove`/`install` 拒绝；仅 install.sh 的 `--platform` 可动 |
| 注册表非原子 / 并发写 | `flock` + tmp/fsync/rename |
| 构建容器宿主挂载 | 只读源码 + 临时产物目录；`cap-drop ALL`；`no-new-privileges`；不加额外 capabilities |
| Docker socket 暴露 | 构建容器**不**挂载任何 socket |
| 宿主 root 命令执行 | 只执行**约定的** `<pm> run build`；不从 JSON 里取任意命令 |
| 远端 JS 运行时 import | 只 import 注册表推导出的同源路径；破窗值走白名单 |
| Caddy 资产穿越 | `handle_path` + 根目录限定；`..` 被规范化后落入 SPA 回退（返回引导页而非文件） |
| 内部文件外泄 | `file_server hide` 隐藏 `.registry.lock` / `.staging-*` |
| 设置更新鉴权 | `FRONTEND_ACTIVE` 走既有 `PATCH /api/admin/settings/{id}`（SuperAdmin） |
| 公开 bootstrap 泄露 | `/api/frontend` 只回 5 个公开字段；非法设置值回落 `default`，不回显 |
| 第三方前端信任说明 | 见 §7 与 [DEVELOPING.md](./DEVELOPING.md) |

其它已落实的细节：

- 前端资产安装后设为**全局只读**（`chmod -R a-w`），运行期不可写；
- 管理端选择器**只**列出本地注册表里已安装、且契约兼容的前端；
- 契约不兼容的候选在下拉里禁用并说明原因（而不是让管理员选出一个必然回退的前端）。

## 9. 生命周期

### 9.1 `$FLOATCTF_HOME/frontends` 是**公开**静态树

Caddy 以只读方式把它挂在 `/srv/frontends` 下并**无鉴权**提供
（`/__floatctf/frontends/...`）。因此：

- **任何私密信息都不得写入该目录**（安装来源、本地路径、Git URL、凭据、运维用户名、
  构建工作目录、临时路径、内部部署元数据……）；
- 需要溯源信息时，把它放在**这个树之外**（例如数据库 / 运维记录），不要放在这里；
- 注册表的字段白名单是**公开契约**的一部分，见 [ARTIFACT.md](./ARTIFACT.md)。

### 9.2 版本不可变

前端资产 URL 带 `Cache-Control: immutable` 长缓存，因此 **前端 ID + 版本 = 一组不可变字节**：

- 同 ID + 同版本 + 同内容 → 幂等成功；
- 同 ID + 同版本 + **不同内容 → 硬失败**（包括 `default` 与平台重部署路径）；
- Default UI 变了就必须**升它的前端版本号**（平台版本与前端版本独立演进）。

### 9.3 生命周期

| 动作 | 谁来执行 | 说明 |
|------|----------|------|
| 安装 / 升级 / 回滚版本 | 运维 CLI：`sudo $FLOATCTF_HOME/frontend.sh install …` | 可在无源码签出的机器上运行 |
| 选择生效前端 | 管理端 → 设置 → 前端（写 `FRONTEND_ACTIVE`） | 只从"已安装"里选；不安装、不构建 |
| 破窗恢复 | 任意用户在 URL 加 `?frontend=default` | 只影响当前这次加载，不改设置，不需登录 |
| 升级平台 | `scripts/install.sh`（重新部署） | 更新 bootstrap 与 release 内前端；**保留**第三方前端、其版本与其指针；不动 `FRONTEND_ACTIVE` |
| 安全卸载 | `sudo $FCTF_ROOT/uninstall.sh` | 移除可运行应用，但**保留** `frontends/`（含注册表）与 `frontend.sh` |
| 彻底删除 | `sudo $FCTF_ROOT/uninstall.sh --purge` | 连已安装前端与注册表一并删除 |

## 10. 非目标（明确不做）

- 前端市场 / 浏览器内安装 / Git URL 安装入口；
- 运行时加载远端任意 JS；
- 把前端安装包装成沙箱；
- 共享 React 单例 / 模块联邦（v1 每个前端自带依赖，换独立性与简单）；
- 用环境变量做前端配置（平台配置仍只从 TOML / settings 表读取）。
