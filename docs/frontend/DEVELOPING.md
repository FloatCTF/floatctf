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
# ① 在 FloatCTF 仓库里打包（用平台自带脚本，它会先构建并做 fail-closed 校验）
pnpm run build:packages
./scripts/package-sdk-dist.sh /tmp/pkgs

# ② 在你的前端仓库里指向这些 tarball
./scripts/use-local-packages.sh /tmp/pkgs    # 可照抄官方前端仓库里那份
pnpm install
```

**不要用 `pnpm add file:...`**（本节的旧写法）。在 pnpm 11 上它会失败：
`pnpm add` 会**先解析已有的** `^1.0.0` 依赖，而这三个包不在 npm 上 → 直接 404，
根本走不到"添加本地 tarball"那一步。正确做法是**改 overrides**，而不是追加依赖。

**为什么必须用 overrides 而不是只改直接依赖**：`@floatctf/react` 自身依赖
`@floatctf/sdk@1.0.0`。只把直接依赖指到 tarball 的话，这条**传递依赖**仍会去 registry 找。

**不要用裸 `pnpm pack`**：三个包的 `files` 只有 `dist` + `README.md`，
**不先构建就会打出只有 `package.json`/`README.md`/`LICENSE` 的空壳 tarball**
（约 13–15KB）。`package-sdk-dist.sh` 正是为拦这个而写的，它还会校验三包版本一致、
拒绝 `workspace:` 残留、并附 `SDK-SHA256SUMS`。

#### pnpm 11 的两个静默坑

1. **`package.json` 里的 `pnpm` 字段不再被读取**（pnpm 11 起）。原来放
   `pnpm.overrides` 的配置会被**静默忽略**（只打一行 WARN），必须写到
   `pnpm-workspace.yaml` 的 `overrides:`。
2. **构建脚本默认被拦**。要显式放行，且旧名 `onlyBuiltDependencies` **已失效**
   —— 正确写法是 `pnpm-workspace.yaml` 里的 `allowBuilds` **map + 显式布尔**：

   ```yaml
   allowBuilds:
     esbuild: true
   ```

   否则 `pnpm install` 会以 `ERR_PNPM_IGNORED_BUILDS` 失败（依赖其实装上了，
   但 esbuild 的 postinstall 被拦，`vite build` 起不来）。

### 2.5 构建产物

`pnpm build` 必须在 `dist/` 里产出 **制品 manifest** `frontend.json`（字段见
[ARTIFACT.md](./ARTIFACT.md)）：它比源码 manifest 少一个 `build` 段。
官方前端的做法是 Vite 插件在 `closeBundle` 里生成并用 `@floatctf/frontend-runtime`
的校验器自检（`frontends/default/vite.config.ts`），可以直接照抄思路。

构建产物必须**自包含**：可以自带框架（v1 不做共享 React 单例/模块联邦）。

### 2.6 发布：制品 CI（GitHub Release）

**强烈建议**给前端仓库配一条发布 CI：把上面那份 `dist/` 打成制品并挂到 GitHub Release 上。
这样**消费方只下载、不构建** —— 宿主不需要 Node，也不执行你仓库里的任何代码。

这与平台既有的训练站同步完全同构（见 `apps/api/src/scheduler/handlers/training_handlers.rs`
的开头注释）。三个官方前端仓库（`FloatCTF/floatctf-frontend-{animal-island,shadcn,xzmu}`）
的 `.github/workflows/release.yml` 就是照这个形状写的，可以直接拿去改。

#### 产出的三个资产

| 资产 | 用途 |
|---|---|
| `frontend-dist.tar.gz` | 制品本体。**`frontend.json` 必须在归档根部**（`tar -C dist .` 的形态），且**不含 `build` 段** |
| `frontend-dist.tar.gz.sha256` | 校验和；消费方 `sha256sum -c` |
| `BUILD_INFO.json` | 构建元数据。其中 **`frontend` 字段是 `dist/frontend.json` 的原样副本** |

**为什么要有 `BUILD_INFO.json` 里的 `frontend` 字段**：前端市场
（`GET /api/admin/frontend-market`）要展示每个前端的 id / version / 兼容契约，
而这些字段在 tar.gz 内部。市场只读这个几 KB 的小文件就能拿到，
**不必为了显示元数据去下载整个制品**。所以**不要把 `frontend` 字段裁成部分字段**。

#### 发布通道

| 触发 | 产出 |
|---|---|
| push 到 `main` | **滚动** release，tag 固定为 `frontend-latest`（每次覆盖三个资产） |
| push tag `v*` | **版本化** release，永久保留，用于固定版本部署 |
| `workflow_dispatch` | 手动重跑 |

稳定的下载地址（消费方不需要知道具体提交）：

```bash
gh release download frontend-latest -R <owner>/<repo> -p 'frontend-dist.tar.gz*'
sha256sum -c frontend-dist.tar.gz.sha256
sudo /var/lib/floatctf/frontend.sh install frontend-dist.tar.gz
```

#### 打包时的两个硬要求

1. **`frontend.json` 必须在归档根部**，且 `entry` / `styles` 指向的文件真实存在。
   `frontend.sh install` 会做 `normalize_artifact_root` → `validate_artifact_tree` →
   `validate_manifest_file`，任一步不过就拒装。用
   `tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - -C dist . | gzip -9n`
   可以得到**确定性**的归档（同样内容得到同样字节）。
2. **归档里不能有符号链接 / 特殊文件** —— 制品树校验会拒绝。
   建议在打包前就 `find dist -type l` 检查并 fail。

#### 契约对齐（最容易踩的坑）

制品的 `compatibility.frontendRuntime` / `apiContract` 必须与**构建它时用的
`@floatctf/frontend-runtime`** 里的常量一致。前端的 Vite 插件会在 `closeBundle` 里
用 `parseFrontendManifest` **自检**，不一致时**构建直接失败**（这是好事：不会产出装不上的制品）。

反过来，一个声明 `apiContract: "1"` 的制品，会被契约 major 为 2 的平台在
`frontend.sh` 侧**拒绝**。所以升级平台契约时，前端要**同时**升级自己的声明并重新发布。

#### 依赖从哪来（当前状态）

`@floatctf/sdk` / `@floatctf/frontend-runtime` / `@floatctf/react` **尚未发布到 npm**，
而 `workspace:*` 是仓库内协议、独立仓库解析不了。因此**当前**的 CI 做法是：

1. `git clone` 平台 monorepo（`vars.FLOATCTF_MONOREPO_REF`，默认 `main`）；
2. 用平台自带的 **`scripts/package-sdk-dist.sh <dir>`** 打出三个 tarball ——
   它**先构建**（否则 `files` 里只有 `dist` + `README.md`，打出来是**空壳**），
   校验三包版本一致、**fail-closed 拒绝 `workspace:` 残留**，并产出 `SDK-SHA256SUMS`；
3. `scripts/use-local-packages.sh <dir>` 把 `pnpm-workspace.yaml` 的 `overrides`
   指向这些 tarball（**必须用 overrides**：`@floatctf/react` 自身依赖
   `@floatctf/sdk`，只改直接依赖的话这条**传递依赖**仍会去 registry 找而 404）；
4. `pnpm install && pnpm build`。

`FLOATCTF_MONOREPO_REF` 这个仓库变量是为**平台契约刚升级、尚未并入 `main`** 的场景准备的：
把 CI 指向一个已含该升级的 ref，就能在不动 `main` 的前提下产出制品。

> 这三个包一旦发布到 npm，上面 4 步可以整体删掉，改成普通的 `pnpm install`。
> 在那之前，每个前端仓库的 CI 都被「借 monorepo」绑住，也就必然受契约版本影响。

#### 让市场发现你

前端市场按 **`<owner>` + 仓库名前缀 `floatctf-frontend-`** 发现候选
（默认 owner = `FloatCTF`，可用 `[frontend_market]` 配置改）。所以：

- 仓库名要以 `floatctf-frontend-` 开头才会被列出；
- 有 `frontend-latest` release 且含上述三个资产，市场才会显示版本 / 契约 / 可复制的安装命令。

市场**只做发现，不安装** —— 安装始终是宿主的 `frontend.sh`（原因见
[ARCHITECTURE.md](./ARCHITECTURE.md) 与 `apps/api/src/modules/platform/frontend/market.rs` 的边界说明）。

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
| CI 报 `apiContract "2" is incompatible with API contract major 1` | 你声明的契约与**构建时用的 `@floatctf/frontend-runtime`** 不一致。若平台契约刚升级、尚未并入 `main`，把仓库变量 `FLOATCTF_MONOREPO_REF` 指向已含该升级的 ref；**不要**为了过 CI 而下调声明（那是伪造兼容性，制品装不上） |
| CI 产出的 tarball 只有 13–15KB；`frontend.sh` 报缺 `dist` | **空壳 tarball**：打包前没有构建（三个包的 `files` 只有 `dist`）。改用 `scripts/package-sdk-dist.sh`，它会 fail-closed |
| `pnpm install` 报 `ERR_PNPM_FETCH_404 @floatctf/sdk` | 独立仓库不能用 `workspace:*`（仓库内协议）；且必须用 `pnpm-workspace.yaml` 的 `overrides` 才能覆盖**传递**依赖。见 §2.4 |
| `pnpm install` 报 `ERR_PNPM_IGNORED_BUILDS` | pnpm 11 默认拦构建脚本：加 `allowBuilds: { esbuild: true }`（旧名 `onlyBuiltDependencies` 已失效） |
| 配了 `pnpm.overrides` 却"没生效" | pnpm 11 起 `package.json` 的 `pnpm` 字段**不再被读取**，只打一行 WARN。写进 `pnpm-workspace.yaml` |
| 前端市场里看不到我的前端 | 仓库名必须以 `floatctf-frontend-` 开头，且要有含 `frontend-dist.tar.gz` 的 release |

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
