# 前端制品 / manifest / 注册表（ARTIFACT.md）

> 契约的**权威校验实现**在 `@floatctf/frontend-runtime`
> （`src/manifest.ts` / `src/registry.ts` / `src/paths.ts`，测试见同目录 `__tests__/`）。
> 本文说明字段、规则与版本策略；实现与文档冲突时以**代码 + 测试**为准，并请修文档。
>
> 架构背景见 [ARCHITECTURE.md](./ARCHITECTURE.md)，开发流程见 [DEVELOPING.md](./DEVELOPING.md)。

## 1. 两个 manifest 不要混淆

| 文件 | 用途 | 谁读它 |
|------|------|--------|
| `floatctf.frontend.json` | **源码仓库** manifest：标识前端、声明兼容性、定义构建期望 | `scripts/frontend.sh`（构建前） |
| `frontend.json` | **构建产物** manifest：运行时契约（入口、样式、兼容性） | 注册表生成、浏览器解析、发布验证 |

制品 manifest 里**不允许**出现源码专用的 `build` 段（严格校验会拒绝未知字段）。
构建脚本负责把源码 manifest 转成制品 manifest（剥掉 `build`），
`scripts/frontend.sh` 也能在源码 manifest 缺失产物版时做同样的转换。

## 2. `frontend.json`（制品 manifest）

```json
{
  "schemaVersion": 1,
  "id": "my-frontend",
  "name": "My FloatCTF Frontend",
  "version": "1.2.0",
  "description": "可选",
  "author": "可选",
  "compatibility": { "frontendRuntime": "1", "apiContract": "1", "sdk": "1.0.0" },
  "entry": "assets/frontend.js",
  "styles": ["assets/frontend.css"]
}
```

| 字段 | 必填 | 规则 |
|------|------|------|
| `schemaVersion` | ✅ | 必须等于 `FRONTEND_MANIFEST_SCHEMA_VERSION`（当前 `1`） |
| `id` | ✅ | `[a-z0-9][a-z0-9._-]*`，≤64 字符（同时是目录名） |
| `name` | ✅ | 非空字符串，≤128 |
| `version` | ✅ | 严格 semver（含预发布，如 `1.2.0-rc.1`） |
| `description` / `author` | ❌ | 字符串（≤1024 / ≤256） |
| `compatibility.frontendRuntime` | ✅ | 整数 major 字符串，必须与平台一致 |
| `compatibility.apiContract` | ✅ | 整数 major 字符串，必须与平台一致 |
| `compatibility.sdk` | ❌ | 字符串，**信息性**（前端自带依赖，运行时不强制） |
| `entry` | ✅ | 相对路径，`server` 会动态 import 它；必须导出 `mount` |
| `styles` | ❌ | 相对路径数组（≤16），由 bootstrap 注入 `<link>` |

**严格性**：未知顶层字段直接**拒绝**（fail-closed）。静默忽略未知字段会产生
"看起来装了但实际少了一件事"的故障，极难排查。

**`compatibility` 只支持整数 major**（如 `"1"`，不支持 `>=1 <2` 这类 range）：
v1 刻意选择语义确定、可测、不会因 range 解析器差异产生分歧的约束形式。
未来的兼容范围需求再单独设计并升 `frontendRuntime` major。

### 路径安全规则（`entry` 与 `styles` 共用）

必须**相对**，且拒绝：

- 绝对路径（`/x`、`C:\x`）
- URL scheme（`https:`、`data:`、`file:`、`javascript:`）
- `..` 穿越与 `.` 段
- 反斜杠、空段（`a//b`）、前后空白、控制字符
- 非 `[A-Za-z0-9._@+-]` 的单段字符

## 3. `registry.json`（本地已安装注册表）

位置：`$FLOATCTF_HOME/frontends/registry.json`（浏览器通过
`/__floatctf/frontends/registry.json` 读取，`Cache-Control: no-store`）。

```json
{
  "schemaVersion": 1,
  "updatedAt": "2026-10-07T06:44:19Z",
  "frontends": {
    "default": {
      "id": "default",
      "currentVersion": "1.0.1",
      "protected": true,
      "versions": {
        "1.0.0": { "version": "1.0.0", "name": "…", "compatibility": { … },
                   "entry": "assets/frontend.js", "styles": ["assets/frontend.css"],
                   "installedAt": "…" },
        "1.0.1": { … }
      }
    }
  }
}
```

| 字段 | 规则 |
|------|------|
| `schemaVersion` | 必须等于 `FRONTEND_REGISTRY_SCHEMA_VERSION`（当前 `1`） |
| `frontends.<id>.id` | 若出现必须与键一致；ID 必须安全 |
| `frontends.<id>.currentVersion` | **必须**是该 ID 已安装的版本 |
| `frontends.<id>.protected` | `true` 表示常规前端管理不得移除（`default`） |
| `versions.<v>` | 键必须是 semver；条目内 `version` 必须与键一致；`entry`/`styles` 走同一套路径规则；`installedAt` 必填 |

版本条目里允许多余字段（例如 `source`），运行时会忽略它们（工具链可自由附加元数据）；
但**必须**满足上表的必填与格式要求。注册表整体校验失败时，bootstrap 视为"注册表不可用"
并走回退链（最终兜底页），绝不会"跳过坏条目继续加载"。

## 4. 文件系统布局

```
$FLOATCTF_HOME/                             # 默认 /var/lib/floatctf
├── frontend.sh                             # 前端管理器（随 release 发布，root:root 0755）
├── web/                                    # bootstrap 引导页（apps/web 产物）
│   ├── index.html
│   └── assets/bootstrap-<hash>.js
└── frontends/                              # Caddy 以只读挂载到 /srv/frontends
    ├── registry.json                       # 0644，原子写入，no-store
    ├── .registry.lock                      # 写入锁（file_server 隐藏）
    ├── default/                            # 平台内置（受保护）
    │   ├── 1.0.0/                          # 全局只读；旧版本保留 → 可回滚
    │   │   ├── frontend.json
    │   │   └── assets/{frontend.js,frontend.css,<chunks>.js}
    │   └── 1.0.1/
    └── my-frontend/                        # 第三方
        ├── 0.2.0/
        └── 1.0.0/
```

Caddy 映射：

| 请求 | 行为 |
|------|------|
| `/__floatctf/frontends/registry.json` | 直接提供，`Cache-Control: no-store` |
| `/__floatctf/frontends/<id>/<version>/**` | 直接提供，`Cache-Control: public, max-age=31536000, immutable` |
| `/api/**` | 反向代理 API（先匹配，**不被**前端资产遮蔽） |
| 其它任意路径 | SPA 回退到 bootstrap `index.html`（前端自己处理路由） |

## 5. 制品归档格式

推荐扩展名：`*.tar.gz`（`frontends/<id>/<version>/…` 或直接含 `frontend.json` 的目录）。

硬性要求：

1. 归档**自包含**：含 `frontend.json` 与它声明的 `entry` / `styles`；
   不依赖源码、`node_modules`、pnpm 或 FloatCTF 仓库。
2. 归档**不得**包含符号链接、硬链接、设备/管道等特殊成员（解包前校验）。
3. 归档成员不得是绝对路径或含 `..`。
4. 发布产物 `web-dist.tar.gz` 的布局固定为：

```
bootstrap/                     ← 引导页
  index.html
  assets/…
frontends/
  default/
    <version>/
      frontend.json
      assets/…
```

`scripts/verify-release-frontend.sh` 会在 release 前断言：布局正确、manifest 通过
**真实校验器**、entry/styles 真实存在、无源码/依赖残留、归档成员安全、脚本语法有效。

## 6. 源码构建（`frontend.sh install <源码目录|Git URL>`）

构建期望来自源码 manifest 的 `build` 段：

```json
"build": { "packageManager": "auto", "script": "build", "outputDir": "dist" }
```

| 键 | 默认 | 说明 |
|----|------|------|
| `packageManager` | `auto` | `auto` 时按 lockfile 判定：`pnpm-lock.yaml` → pnpm，`yarn.lock` → yarn，`package-lock.json` → npm，否则 npm |
| `script` | `build` | **package.json 里的脚本名**（不是 shell 命令） |
| `outputDir` | `dist`（回落 `build`） | 构建产物目录 |

安全约束：**不**把 JSON 里的字符串当命令执行；构建在**隔离容器**内进行
（详见 [ARCHITECTURE.md §7/§8](./ARCHITECTURE.md)）；Node 基线 = `node:26-bookworm`
（可用 `--node-image` 覆盖），pnpm 基线 = 仓库 `packageManager`（`11.20.0`），
源码若声明 `packageManager: pnpm@x.y.z` 则以它为准。

## 7. 版本与指针语义

- 安装某版本**不会**自动改变当前指针；需要时显式 `--make-current` 或 `frontend.sh set-current`。
- `frontend.sh remove <id> <version>` 删除的若是当前版本，会把指针**显式回退**到剩余版本
  并打印出来（绝不留下悬空指针）。
- 同 ID 同版本、内容不同：默认拒绝（资产不可变）；`--platform --reinstall` 用于平台重部署。
- 同 ID 同版本、内容相同：幂等跳过（重部署安全）。

## 8. 契约版本策略

见 [ARCHITECTURE.md §6](./ARCHITECTURE.md)。要点：**加法式变更不升 major**；
破坏性变更才升，并且必须同步
`packages/frontend-runtime/src/version.ts` 与 `apps/api/src/core/contract.rs`
（`scripts/check-architecture.sh` 会断言两者一致）。
