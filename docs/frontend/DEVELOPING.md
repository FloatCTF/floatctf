# 前端开发指南（DEVELOPING.md）

> 三种角色，各看各的：
> - **在 FloatCTF 仓库里改官方前端** → §1
> - **在别的仓库里写第三方前端** → §2、§3
> - **把前端装到服务器上** → §4
> - **用 AI 代理从零创建一个新前端** → [AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md) +
>   [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md)
>
> 契约细节见 [ARTIFACT.md](./ARTIFACT.md)，架构见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

> [!IMPORTANT]
> **本文 §1 的"仿照既有页面"约定只约束 `frontends/default/`（官方前端）。**
> 创建新的可插拔 Frontend 时，视觉语言 / 信息架构 / 导航 / 路由 / 布局 / 组件 / 设计系统 /
> CSS 策略 / 状态管理 / 框架**由你的前端自主决定**，本节不构成约束。
> 新前端的权威作业手册是 [AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md)（能力清单：
> [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md)）；判型规则见仓库根 [AGENTS.md](../../AGENTS.md)
> 的「前端任务先判型」。

## 1. 在 FloatCTF 仓库里开发

### 1.1 唯一开发入口

```bash
mise run setup     # 新机器一次性：工具链 + 宿主能力 + helper
mise run dev       # 唯一开发入口：infra + migrations + API watch + 前端 Vite
```

`mise run dev` 的前端部分做两件事（`mise.toml` 的 `dev:web`）：

1. `pnpm run build:packages` —— 构建 `packages/*` 的 `dist`（前端按 `package.json`
   的 `exports` 解析 `@floatctf/*`，所以这是**真实产物**，不是源码别名）；
2. 在 `frontends/default` 启动 Vite（**:13000**，HMR）。

浏览器访问的是 **`http://<host>:7780`**（开发 Caddy）：`/` → Vite，`/api/*` → API，
`/__floatctf/frontends/*` → Default Frontend 的 Vite（它按**真实 schema** 提供一份
只含 `default` 的开发注册表，因此管理端的"前端选择器"在 dev 下走的是与生产完全相同的
代码路径）。

> 开发模式**只有这一个前端**（Default Frontend 的源码）。要看第三方前端，用 §2 的
> 独立 dev server，或在 §6 的"生产形态验证"里装到本地再跑。

### 1.2 改动落在哪个包？

| 你要改的东西 | 位置 |
|--------------|------|
| 页面、路由、布局、导航、Primer 样式、UX | `frontends/default/src/**` |
| 领域 API 调用、DTO 类型、传输/401 行为、SSE 传输 | `packages/sdk/src/**` |
| query 工厂、SSE→React Query 失效、headless hook | `packages/react/src/**` |
| manifest/注册表校验、bootstrap 加载与回退 | `packages/frontend-runtime/src/**` |
| 引导页本身 | `apps/web/src/main.ts` |

改动 `packages/*` 后需要重建 dist 才会被前端看到：

```bash
pnpm run build:packages          # 一次性
mise run dev:packages            # 或：tsc --watch 持续重建
```

### 1.3 常用门禁

```bash
mise run web:typecheck      # 五个包的 tsc --noEmit
mise run web:architecture   # 架构边界 + 契约版本一致性（失败即红）
mise run test               # Rust + 全部前端包测试
mise run lint               # clippy + biome + 类型检查 + 架构门禁
mise run build              # API + packages + bootstrap + Default Frontend
mise run check              # fmt + lint + test
```

### 1.4 前端开发者必须知道的两条约束

1. **制品运行在纯浏览器环境**：没有 `process`、没有 `require`、没有 Node 全局。
   第三方依赖（styled-components / mermaid 等）里的 `process.env.*` 必须在
   **构建期**替换掉——官方前端的做法见 `frontends/default/vite.config.ts` 的 `define`
   与 `src/node-shim.ts`。漏掉会表现为"整页起不来"，且由 bootstrap 的兜底页如实报错。
2. **不要 import 其它包的源码路径**：只能用 `@floatctf/sdk` / `@floatctf/react` /
   `@floatctf/frontend-runtime` 的公开导出。`scripts/check-architecture.sh` 会拦截
   `@floatctf/sdk/src/...`、`../apps/web`、仓库私有别名等写法。
3. **本节的视觉约定只对官方前端生效**：§1 讲的"先找同域参照页 / 复用 `components/`
   既有组件 / 与参照页保持一致"是 **`frontends/default/` 内部**的规矩（见
   [RULES.md](../agents/RULES.md)）。第三方前端**不受**这些视觉约定约束——上述第 1、2 条
   是**运行时与依赖边界**约束，对所有前端一视同仁。

## 2. 在独立仓库里开发第三方前端

### 2.1 前提

- 可以拿到平台发布的包：`@floatctf/sdk`、`@floatctf/frontend-runtime`
  （可选 `@floatctf/react`，仅 React 前端需要）。
- **动手前先读** [AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md)（AI 代理作业手册：鉴权 /
  mount / 路由 / SSE / 完整性口径 / 验收流程）与 [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md)
  （能力清单，用来判断"完整前端"意味着覆盖哪些能力）。
- 本地开发时若包尚未发布到 npm，用 **packed tarball**（见 §2.4）。
- Node 基线：与平台一致的大版本（平台开发基线 `node 26.x`；源码构建默认用
  `node:26-bookworm` 镜像）。

### 2.2 最小仓库骨架

```
my-floatctf-frontend/
├── package.json
├── floatctf.frontend.json      # 源码 manifest（含 build 段）
├── vite.config.ts
├── index.html                  # 仅 dev server 用
├── README.md
└── src/
    ├── mount.ts                # 导出 mount(context)；导出什么框架由你决定
    └── …
```

```jsonc
// package.json
{
  "name": "my-floatctf-frontend",
  "private": true,
  "type": "module",
  "packageManager": "pnpm@11.20.0",
  "scripts": { "dev": "vite --port 14000", "build": "vite build" },
  "dependencies": {
    "@floatctf/sdk": "^1.0.0",
    "@floatctf/frontend-runtime": "^1.0.0"
  }
}
```

```jsonc
// floatctf.frontend.json
{
  "schemaVersion": 1,
  "id": "my-frontend",
  "name": "My FloatCTF Frontend",
  "version": "0.1.0",
  "compatibility": { "frontendRuntime": "1", "apiContract": "1" },
  "entry": "assets/app.js",
  "styles": ["assets/app.css"],
  "build": { "packageManager": "auto", "script": "build", "outputDir": "dist" }
}
```

### 2.3 入口与 API 基址

```ts
// src/mount.ts
import { createFloatCTFClient } from "@floatctf/sdk";
import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";

let token: string | null = null;   // 存哪里完全由你决定

export function mount(context: FloatCTFMountContext) {
  const client = createFloatCTFClient({
    baseUrl: context.apiBaseUrl,           // 生产同源 → "/api"
    getUserToken: () => token,
    onUnauthorized: () => { token = null; /* 你的登录页怎么走由你决定 */ },
  });
  // 建你自己的框架/路由/样式，渲染进 context.root
}
```

**框架自由**：`mount(context)` 只要求"给一个宿主元素，把界面挂上去"。
Vue / Svelte / Solid / 原生 TypeScript 都可以；`@floatctf/react` 是可选的。
`context` 里**没有** Router、Query、Primer 或"当前页面"的概念——路由是你的。

### 2.4 本地开发（跨源）

```bash
# 1) 让 API 允许你的 dev server 源（平台唯一支持的方式：TOML，不是环境变量）
#    编辑 apps/api/config/development.toml：
#      [cors]
#      allowed_origins = ["http://localhost:14000", "http://127.0.0.1:14000", ...]

# 2) 用你自己的构建期变量指向 API（这是**前端自己的** Vite 变量，不是平台契约）
VITE_API_BASE_URL=http://127.0.0.1:7780/api pnpm dev
```

用未发布的本地包时，先打包再引用（**不要**用 `workspace:*`，那是仓库内协议）：

```bash
# 在 FloatCTF 仓库里
pnpm --filter @floatctf/frontend-runtime --filter @floatctf/sdk pack --pack-destination /tmp/pkgs
# 在你的前端仓库里
pnpm add file:/tmp/pkgs/floatctf-sdk-1.0.0.tgz file:/tmp/pkgs/floatctf-frontend-runtime-1.0.0.tgz
```

### 2.5 构建产物

`pnpm build` 必须在 `dist/` 里产出 **制品 manifest** `frontend.json`（字段见
[ARTIFACT.md](./ARTIFACT.md)）：它比源码 manifest 少一个 `build` 段。
官方前端的做法是 Vite 插件在 `closeBundle` 里生成并用 `@floatctf/frontend-runtime`
的校验器自检（`frontends/default/vite.config.ts`），可以直接照抄思路。

构建产物必须**自包含**：可以自带框架（v1 不做共享 React 单例/模块联邦）。

## 3. 三方前端能用什么、不能用什么

| | |
|---|---|
| ✅ 用 | `@floatctf/sdk`、`@floatctf/frontend-runtime`、可选 `@floatctf/react`；任意 UI 框架；任意状态库；任意样式方案 |
| ❌ 不用 | `apps/web/*`、`frontends/default/*`、`@/routes` 这类仓库私有别名、任何 FloatCTF 仓库内的源码路径 |

## 4. 安装、激活、回滚

```bash
# 安装（本地目录 / Git 仓库 / 预构建归档；在 FloatCTF 宿主上执行）
sudo /var/lib/floatctf/frontend.sh install /path/to/my-floatctf-frontend
sudo /var/lib/floatctf/frontend.sh install https://github.com/me/my-frontend --ref v0.1.0
sudo /var/lib/floatctf/frontend.sh install ./my-frontend-0.1.0.tar.gz

# 看装了哪些 / 详情 / 只校验归档
sudo /var/lib/floatctf/frontend.sh list
sudo /var/lib/floatctf/frontend.sh info my-frontend
sudo /var/lib/floatctf/frontend.sh verify ./my-frontend-0.1.0.tar.gz

# 回滚某个 ID 的当前版本（旧版本仍在）
sudo /var/lib/floatctf/frontend.sh set-current my-frontend 0.0.9

# 移除（default 不可移除）
sudo /var/lib/floatctf/frontend.sh remove my-frontend 0.0.9
```

**激活**（把 `FRONTEND_ACTIVE` 指到它）是**应用设置**，在管理端操作：
管理端 → 设置 → 前端 → 选择 → 保存并切换 → 刷新页面。
CLI **不会**替你改 `FRONTEND_ACTIVE`。

**破窗恢复**：任何页面加 `?frontend=default` 即用内置前端打开，
只影响当前这次加载、不修改设置、不需要登录。

源码安装会在**隔离容器**里跑依赖安装与 `build` 脚本：

- `cap-drop ALL`、`no-new-privileges`、`--pids-limit`、只读源码挂载、无 Docker socket；
- **构建身份永不为 UID 0**：`sudo` 调用用 `SUDO_UID/SUDO_GID`；普通用户用其自身；
  root 直调时用专用非特权身份（默认 `65534:65534`）。源码与输出目录会被暂存并
  `chown` 给该身份，因此**不会**为了构建去改你原仓库的权限；
- 制品目录会逐个文件用 `lstat` 校验（拒绝符号链接/硬链接/FIFO/socket/设备文件），
  `frontend.json` / `entry` / `styles` 必须是普通文件 —— 归档、源码构建、预构建目录
  走**同一套**边界。

但请注意：**隔离降低的是宿主风险，不会让浏览器 JS 变得可信**（见
[ARCHITECTURE.md §7](./ARCHITECTURE.md)）。

`build` 段的三个字段是**真实生效**的契约（不是文档摆设）：

```jsonc
"build": { "packageManager": "auto", "script": "build:floatctf", "outputDir": "out-ui" }
```

- `packageManager`: `auto` 按 lockfile 判定，也可显式 `pnpm|npm|yarn`；
- `script`: package.json 里的**脚本名**（形如 `[A-Za-z0-9:_-]+`，绝不接受 shell 片段）；
- `outputDir`: 安全相对目录；构建容器只复制这个目录。

若构建没有产出 `frontend.json`，管理器会从源码 manifest **生成**一个只含运行时字段的
制品 manifest（剥掉 `build`），并按制品契约复核后才安装。

## 5. 常见问题

| 现象 | 原因 / 处理 |
|------|-------------|
| 整页显示"FloatCTF 界面加载失败"并给出原因 | bootstrap 的兜底页：按页面上的失败原因修（最常见：制品路径写错、契约 major 不匹配、bundle 引用了 `process`） |
| 改了制品但刷新没变化 | 版本化资产是 `immutable` 长缓存：**发布新版本号**（同版本替换只用于平台重部署，浏览器仍可能拿旧缓存） |
| 管理端选择器里看不到我的前端 | 注册表里没有它：确认 `frontend.sh install` 成功、`frontend.sh list` 能看到 |
| 选择器里我的前端被禁用 | 契约不兼容（`compatibility.frontendRuntime` / `apiContract` 与平台不一致） |
| `frontend.sh install` 报 "已存在同 ID 同版本但内容不同" | 资产不可变（immutable 长缓存）：**发新版本号**；没有 `--reinstall` 这类例外 |
| `frontend.sh remove <id> <version>` 报 "该版本是 currentVersion" | 显式指针：先 `frontend.sh set-current <id> <其它版本>` 再删除 |
| 跨源请求被浏览器拦 | 把 dev server 的源加入后端 TOML 的 `[cors].allowed_origins` |

## 6. 生产形态验证（不启动整套生产）

想在本地验证"引导页 + 注册表 + 版本化资产 + Caddy 路由"的真实行为：

```bash
pnpm run build:packages && pnpm run build:web     # 构建 bootstrap 与 Default Frontend
./scripts/package-web-dist.sh /tmp/web-dist.tar.gz
./scripts/verify-release-frontend.sh /tmp/web-dist.tar.gz

export FLOATCTF_HOME=/tmp/fcft-home && mkdir -p "$FLOATCTF_HOME"
cp apps/web/dist -r "$FLOATCTF_HOME/web"
./scripts/frontend.sh install /tmp/web-dist.tar.gz --platform --make-current
# 然后用与 scripts/install.sh 内嵌模板一致的 Caddyfile 起一个 Caddy，
# 把 $FLOATCTF_HOME/{web,frontends} 只读挂载进去即可。
```
