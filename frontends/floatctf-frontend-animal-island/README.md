# FloatCTF · Animal Island

一个 **FloatCTF 可插拔前端**（选手端 + 管理端），视觉与组件建立在
[`animal-island-ui`](https://github.com/guokaigdg/animal-island-ui) v2.2.1（MIT，39 个组件）之上。

- Frontend id: `animal-island`
- 包名：`@floatctf/frontend-animal-island`
- API 契约：**major 2**（Wave 10 之后的平台契约；`compatibility.apiContract` 必须是 `"2"`）
- 技术栈：React 19 + TypeScript + Vite 6 + react-router v7 + TanStack Query v5 + zustand v5

## 当前状态（**请先读这一节**）

**覆盖口径：`complete（选手端 + 管理端）`。**

- **37 个页面，0 个骨架页**；42 条路由。
- 覆盖 `CAPABILITY-MATRIX.md` 的**全部 93 条 `required`**（逐条机械核对，方法见下）。
- 三大赛制齐备：Jeopardy（解题 / 排行 / 题解 / 讨论）、**AWD**（选手 + 管理）、
  **AWDP**（选手 + 练习 + 管理）。
- 验证：`tsc --noEmit` 0 错误；`vite build` 通过；制品与源码 manifest 均通过
  **平台自己的校验器**；`nav.ts` 的 17 条权限串逐条对照后端 `permission.rs` 无漂移；
  设计法机械扫描 0 违规。

逐页清单、判定证据与**已知缺口**见 [CAPABILITY-COVERAGE.md](./CAPABILITY-COVERAGE.md)。

### 交付前仍需补的一步（如实说明）

[AGENTS.md](./AGENTS.md) 的 DoD 要求在 FloatCTF 实例上完成安装与端到端验收
（`frontend.sh install` → 激活 → 刷新 → 真实数据 → `?frontend=default` 回退）。
**这一步尚未执行** —— 开发期间的约束禁止启动 dev API。
因此上述验证全部是**静态 + 构建期**的，运行时端到端需要在安全实例上补做。

## 命令

```bash
# 在 FloatCTF monorepo 内（pnpm workspace 成员）
cd <monorepo>
mise exec -- pnpm --filter @floatctf/frontend-animal-island typecheck
mise exec -- pnpm --filter @floatctf/frontend-animal-island build     # vite build && tsc --noEmit
mise exec -- pnpm --filter @floatctf/frontend-animal-island dev       # :13500，自带 /api 代理到 :7780
```

**交付门禁**：`tsc --noEmit` + `vite build`（本仓无测试运行器，因此"漏声明权限"这类
不变式用**类型**保证，见 `src/app/nav.ts` 的 `AdminNavItem`）。

## 制品

构建产物 `dist/`：

| 文件 | 说明 |
|---|---|
| `assets/frontend.js` | 入口，导出 `mount(context)` |
| `assets/frontend.css` | 全站唯一样式（含库样式与字体，**完全自包含**） |
| `frontend.json` | 运行时 manifest；由 Vite 插件生成并用 `parseFrontendManifest` **自检** |

**已知取舍**：`assets/frontend.css` 约 4.88MB（gzip 3.60MB）。原因是库的 CSS 用相对
`url("files/...")` 引用 9 个 woff2（其中 Noto Sans SC 的 CJK 子集约 3.8MB），而 Vite
会把这些字体 **base64 内联**进 CSS——实测 `build.assetsInlineLimit: 0` 对 CSS 内的字体引用
**不生效**。收益是产物零外部依赖（不会出现字体 404）；若要瘦身，删掉 CSS 里 `noto-sans-sc`
的 `@font-face` 块即可，字体栈已有 `PingFang SC / Microsoft YaHei` 兜底。

## 设计底线

见 [AGENTS.md](./AGENTS.md) 末尾的《animal-island-ui 设计法》。
最容易踩的几条：**Card 没有 box-shadow**、**图标只能用 `naive-icons`**（禁 emoji /
Unicode 符号 / 手写 SVG）、**焦点环输入类用黄 `#ffcc00` 其余用青 `#19c8b9`**（禁冷蓝）、
**UI 文本不用等宽字体**、**不要用 className/style 覆盖库组件的颜色/圆角/阴影**。

## 认证

- 唯一登录入口 `POST /api/session`；令牌存 `localStorage`（由本前端自己拥有）。
- **401 与 403 严格分流**：401 清会话并引导重新登录；**403 只提示，绝不清会话**
  （重新登录解决不了权限问题）。见 `src/api/client.ts`。
- 前端守卫只判断"有没有会话"与"入口显不显示"；**权限判定永远在服务端**。
