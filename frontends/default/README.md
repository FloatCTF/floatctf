# 官方 Default 前端（FloatCTF frontend `default`）

FloatCTF 的**官方完整前端**，也是平台内置的前端实现。它实现
[前端运行时契约](../../docs/frontend/ARCHITECTURE.md)：`apps/web` 只做引导（解析注册表 →
注入样式 → 动态 import → 调用 `mount(context)`），**其余全部在本包内** —— React、路由、
数据层、组件库与页面。

- **id**：`default`（注册表中标记为 `protected`，**不可卸载**）
- **版本**：`1.0.0`（随平台发布，与平台版本同号）
- **破窗**：任何前端出问题时可用 `?frontend=default` 强制回落到本前端
  （见 [可插拔前端架构](../../docs/frontend/ARCHITECTURE.md)）
- **技术栈**：React 19 + TanStack Router + TanStack Query + Primer React + Tailwind +
  styled-components + zustand + axios

> 本 README 面向**维护 Default 的人**。想新建一个不同风格的前端请看
> [AI-FRONTEND-GUIDE.md](../../docs/frontend/AI-FRONTEND-GUIDE.md) —— 那里说明
> Default 只是**语义参照**，不是视觉模板。

---

## 1. 快速开始

```bash
# 在 monorepo 根执行
mise exec -- pnpm install
mise exec -- pnpm --filter @floatctf/frontend-default dev        # Vite :13000
```

`mise run dev` 会一并起基础设施（PostgreSQL / Redis / RustFS / Caddy）与 API，并且
**Caddy 在 `:7780` 反向代理到本前端的 `:13000`** —— 开发时请访问
**<http://127.0.0.1:7780>**（而不是 13000），因为 `/api` 与 `/__floatctf/*` 都由 Caddy 提供。

```bash
mise exec -- pnpm --filter @floatctf/frontend-default typecheck  # tsc --noEmit
mise exec -- pnpm --filter @floatctf/frontend-default build      # vite build && tsc --noEmit
mise exec -- pnpm --filter @floatctf/frontend-default test       # vitest run（15 个文件 / 173 项）
```

构建产出的 `dist/` 即前端制品，其中 `frontend.json` 由 `vite.config.ts` 的
`floatctf:emit-frontend-manifest` 插件写出，并用 `@floatctf/frontend-runtime` 的**真实校验器**
自检（不通过则构建失败）。

## 2. 界面截图

截图在**真实 API** 下采集（本机开发环境，全新数据库）。原图存于 [`docs/images/`](./docs/images/)。

### 登录

![登录页](./docs/images/login.png)

> 本前端的登录页挂在根路径 `/`（未登录时），而不是 `/login`。

### 选手端

| 天梯 Top | 赛事 | 题库 |
| :------: | :--: | :--: |
| ![Top](./docs/images/top.png) | ![赛事](./docs/images/events.png) | ![题库](./docs/images/challenges.png) |

| 题集 | GameBox | 我的实例 |
| :--: | :-----: | :------: |
| ![题集](./docs/images/challenge-sets.png) | ![GameBox](./docs/images/gameboxes.png) | ![我的实例](./docs/images/instances.png) |

| 解题流水 | 公告 | 讨论区 |
| :------: | :--: | :----: |
| ![解题流水](./docs/images/solves.png) | ![公告](./docs/images/announcements.png) | ![讨论区](./docs/images/discussions.png) |

| 题解 | 武器库 | 我的资料 |
| :--: | :----: | :------: |
| ![题解](./docs/images/writeups.png) | ![武器库](./docs/images/weapons.png) | ![我的资料](./docs/images/profile.png) |

### 管理端

| 概览 | 赛事 | 挑战 |
| :--: | :--: | :--: |
| ![概览](./docs/images/admin-dashboard.png) | ![赛事](./docs/images/admin-events.png) | ![挑战](./docs/images/admin-challenges.png) |

| 用户 | 动态设置 | 容器运维 |
| :--: | :------: | :------: |
| ![用户](./docs/images/admin-users.png) | ![动态设置](./docs/images/admin-settings.png) | ![容器运维](./docs/images/admin-docker.png) |

| AWD 网络 | 操作日志 |
| :------: | :------: |
| ![AWD 网络](./docs/images/admin-awd-network.png) | ![操作日志](./docs/images/admin-logs.png) |

## 3. 路由一览

浏览器路由**由前端自己拥有**，平台不做要求。本前端的划分：

| 区域 | 前缀 | 说明 |
|---|---|---|
| 选手端 | `/service/*` | `top` / `events` / `challenges` / `challenge_sets` / `gameboxes` / `instances` / `solves` / `announcements` / `discussions` / `writeups` / `weapons` / `profile`；赛事与 AWD / AWDP 子页在 `service/events/<family>/<id>/*` |
| 管理端 | `/admin/*` | `dashboard` / `events`（含 Jeopardy / AWD / AWDP 子页）/ `challenges` / `super_admins` / `users` / `awd/*` / `weapons` / `announcements` / `discussions` / `terminal` / `docker` / `database` / `logs` / `scheduled_tasks` / `settings` / `version` |
| 账号 | `/register` `/reset` `/reset_password` | 注册与找回 |

路由文件位于 `src/routes/`，由 TanStack Router 的 `@tanstack/router-plugin` 生成
`src/routeTree.gen.ts`（**生成物，不要手改**）。

## 4. 目录结构

```
src/
├── entry.tsx              # 制品入口：实现 mount(context)（bootstrap 调它）
├── dev.tsx                # 开发入口（直接挂载，等价于 bootstrap 的 context）
├── node-shim.ts           # 必须最先求值的 Node 全局兜底
├── router.tsx             # TanStack Router 装配
├── routeTree.gen.ts       # 生成物
├── config.ts / util.ts / reportWebVitals.ts
├── api/                   # SDK client 装配与封装
├── stores/                # zustand（AuthStore 等，persist 到 localStorage `auth-storage`）
├── integrations/          # 外部集成
├── components/            # 复用组件（GenericTable / Dialog / FilterBar / MsgBanner …）
├── navigation/            # 导航配置
├── routes/                # 页面：service/*（选手）+ admin/*（管理）
└── style.css              # Tailwind 入口与全局样式
```

## 5. 与平台的关系

- **始终存在**：本前端随平台发布，注册表条目标记 `protected: true`，
  `scripts/frontend.sh remove default` 会被拒绝 —— 它是最后一道可用 UI。
- **激活开关**：动态设置 `FRONTEND_ACTIVE`（管理端 →「前端管理」页，或
  `PATCH /api/admin/settings/<id>`）。值为其它前端 id 时本前端不再被加载，但**文件仍在**。
- **破窗**：`?frontend=default` 可绕过 `FRONTEND_ACTIVE` 强制加载本前端。
- **不可依赖**：其它前端**不得** import 本包源码。Default 是行为参照，不是 API。

## 6. 文档

| 文档 | 内容 |
|---|---|
| [docs/frontend/ARCHITECTURE.md](../../docs/frontend/ARCHITECTURE.md) | 可插拔前端平台：mount 契约、注册表、信任模型 |
| [docs/frontend/CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md) | 前端能力矩阵（每项能力的公共 SDK 面 / Default 参照） |
| [docs/agents/RULES.md](../../docs/agents/RULES.md) | **改 Default 必读**：§1–§4 是本前端专属的形态细则 |
| [docs/frontend/AI-FRONTEND-GUIDE.md](../../docs/frontend/AI-FRONTEND-GUIDE.md) | 新建前端（不受本 README 的视觉约定约束） |

## 7. 许可

AGPL-3.0-only（与 FloatCTF 平台一致）。
