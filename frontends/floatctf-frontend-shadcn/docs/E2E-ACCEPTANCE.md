# 端到端验收记录（真实 API）

本文记录 `shadcn` 前端在**本机开发环境**上对着**真实 FloatCTF API** 完成的端到端验收，
对应 [AI-FRONTEND-GUIDE §20](../../../docs/frontend/AI-FRONTEND-GUIDE.md) 的 13 步流程。

- 日期：2026-10-08
- 前端版本：`shadcn@0.1.0`
- 平台：本机开发栈（Caddy `:7780` → API `:9090`）+ 真实 `apps/web/dist` 引导页（同源 harness `:13500`）
- 数据库：本机开发库；验收数据由**公共 API** 播种（见 §7），无任何桩数据
- 账号：选手 `e2e-alpha` / `e2e-beta`（脚本注册），管理端 `sysadmin`

## 1. 构建与制品

```
pnpm build            → vite build + tsc --noEmit          ✅ 0 类型错误（2,347 modules）
dist/frontend.json    → id=shadcn  version=0.1.0  entry=assets/frontend.js  styles=assets/frontend.css
dist/assets/frontend.js   2,883,956 B │ gzip 638.42 kB
dist/assets/frontend.css    107,295 B │ gzip  17.99 kB
scripts/frontend.sh verify  → 制品校验通过：shadcn@0.1.0（entry/assets/stylesheet/契约 runtime1 api1） ✅
scripts/frontend.sh install → 安装到 $FLOATCTF_HOME/frontends/shadcn/0.1.0 并写入 registry.json         ✅
scripts/frontend.sh list    → default 1.0.0 [protected] / shadcn 0.1.0                                  ✅
scripts/frontend.sh info    → current=yes，兼容性 frontendRuntime=1 / apiContract=1                      ✅
mise run web:typecheck      → ✅ exit 0（本前端已并入仓库 Web 类型门禁）
mise run web:architecture   → ✅ [check-architecture] OK

**版本不可变（实测）**：把内容已变化的同一版本再装一次会被硬拒绝 ——
`[FAIL] 已存在同 ID 同版本但内容不同（资产不可变，拒绝覆盖）`. 这正是 ARTIFACT.md 要求的语义；
发布/升级必须递增版本号（本次验收用 `remove` + 重新 `install` 到隔离的临时 home，未污染任何真实安装）。
```

## 2. 两条加载路径

| 路径 | 方式 | 结果 |
|---|---|---|
| 开发直挂 | `pnpm dev`（`:13400`，`dev.tsx` 直接 `mount(context)`） | ✅ 登录、赛事、题库、实例、积分榜、AWD 驾驶舱全部走真实 API |
| 生产引导链 | 真实 `apps/web/dist` + 同源 harness（`:13500`，复刻生产 Caddy 三条映射） | ✅ bootstrap 读取 `/api/frontend`（`active_frontend=shadcn`）→ `registry.json` → `/__floatctf/frontends/shadcn/0.1.0/assets/frontend.js` → `mount(context)` |

引导链实测证据：打开 `http://<lan>:13500/` 得到**本前端**的登录页
（`document.title = 登录 · FloatCTF`，正文含「选手登录 / 使用平台账号登录，进入你的比赛工作区」），
而不是 Default 的 `Login | FloatCTF`。

## 3. 鉴权与边界

| 场景 | 结果 |
|---|---|
| 选手登录（真实 API） | ✅ 表单 → `POST /users/session` → token 入 store → 进入总览 |
| 未登录访问受保护路由 | ✅ 重定向 `/login?next=%2Fevents…`（带 next，登录后回原处） |
| 401 统一处理 | ✅ 清对应作用域 token + 跳登录（`onUnauthorized`，SDK 不导航） |
| 管理端独立作用域 | ✅ 管理端登录不影响选手会话（两个 localStorage 键、两个 SDK token 源） |
| 退出登录 | ✅ 清 token + 清 query cache + 跳登录页 |
| 未参赛 + 赛事已开始 | ✅ 显示「你尚未加入本赛事」门禁（题目/积分榜/实例不可见，与后端一致） |
| 不可见 / 不存在赛事 | ✅ 可见错误「加载赛事失败 · 未找到该赛事」+ 技术细节 + 重试（不白屏） |
| token 不进 URL | ✅ 全部请求走 `Authorization` 头；SSE 同（SDK fetch-based stream） |
| 深链直接刷新 | ✅ 引导链路径直接打开 `/events/<id>?tab=scoreboard` 正常渲染真实积分榜 |

## 3.1 路由走查（真实 API，程序化）

用 `var/shadcn-e2e/route-walk.sh` 逐条打开路由并读取 `document.title` / 主体文本长度 / `[role=alert]`：

| 范围 | 结果 |
|---|---|
| 选手端 25 条（总览 / 赛事 + 6 个标签 / 赛事题目 / 题库 / 题目 / 题集 / 我的实例 / 训练场 / 排行 / 解题流水 / 公告 / 讨论 + 详情 + 我的 / 武器库 / 题解 / 资料 / AWD 驾驶舱 / AWDP 工作台） | **25/25 正常**，0 条误报错误块，全部渲染真实数据 |
| 管理端 30 条（总览 / 赛事 + 10 个标签 / AWD 运维 / AWDP 运维 / 题库 / 题集 / GameBox 库 / 靶场网络 / 用户 / 超管 / 日志 / 设置 / 前端 / 计划任务 / 公告 / 讨论 / 武器库 / Docker / SQL / 终端 / 版本） | **29/30 正常**；唯一例外是「前端选择器」在 **dev（:13400）** 下按设计显示「读取本地前端注册表失败：HTTP 404」+ 重试（dev 没有注册表静态文件），同一页在**引导链（:13500）**下正确列出 `default`（平台内置）与 `shadcn`（生效中）+ 契约兼容性 ✅ |

> 也就是说：dev 下的那一条**不是缺陷**，而是 AI-FRONTEND-GUIDE 要求的行为（注册表不可用时
> 必须给出具体错误，绝不能伪装成「空注册表」）；真实注册表读取路径已在引导链上验证通过。

## 4. 真实数据（非桩）

播种数据（`var/shadcn-e2e/seed.py`，全部通过公共 API）：

| 对象 | 值 |
|---|---|
| 题目 | `base64`（静态，crypto）、`comment`（容器，web）——均由 zip 包真实导入，`build_status=ready` |
| 赛事 | 「shadcn 验收赛 · Jeopardy 单人」`0148cf10-03ef-49a7-925b-93144cf28df3`（jeopardy/individual/competition，进行中，2 题已发布，300 分） |
| 选手 | `e2e-alpha`、`e2e-beta`（真实注册并加入赛事） |
| 解题 | `e2e-alpha` 启动 base64 实例 → 提交真实 flag → **计分 300**；积分榜 `#1 E2E Alpha 300.00 / 1 题`、`#2 E2E Beta 0.00` |
| AWD 赛事 | 「shadcn 验收赛 · AWD 战队赛（未部署）」`011c8971-7c5f-4077-9b8a-912fa1508e99`（awd/team，配置 `status=configuring` / `phase=hardening`）+ 战队「shadcn 验收战队」（2 名真实成员） |
| AWDP 赛事 | 「shadcn 验收赛 · AWDP 正式赛」`6ad16455-a686-429b-b97c-0253d372bfe0`（awdp/individual/competition，`phase=pending`，时长/分值与后端一致） |
| 讨论 | 1 条真实讨论 + 1 条真实评论（`author_id` 过滤已用接口核实） |

**平台模式约束（实测，前端据此决定参与面板形态）**：`family=awd` 只允许 `participant_mode=team`；
`family=awdp` 只允许 `individual`；`purpose=practice` 的 AWDP 赛事不支持 `join/team`
（`UnsupportedForPurpose`），练习入口在 `/training`。

页面读数（节选，均为接口返回值渲染）：

- `/events`：列出上述赛事，状态「进行中」、家族「Jeopardy 解题」、参赛「已加入」，时间窗与后端一致。
- `/events/:id?tab=challenges`：题目数量与分值来自 `GET /events/{id}/challenges`。
- `/events/:id?tab=scoreboard`：`#1 E2E Alpha 300.00 / 1 题`、`#2 E2E Beta 0.00`（30s 轮询）。
- `/events/:id?tab=trend`：`TrendChart` 渲染真实 `TrendItem`（E2E Alpha 的 base64 300 分折线）。
- `/challenges`：两道真实题目，`base64` 显示「已解出」（后端 `solved` 字段）。
- `/events/:id`（AWD 战队赛）：状态「未开始」、模式「战队赛」、战队成员与加入时间均来自后端。
- `/arena/:eventId`（AWD 驾驶舱）：状态卡「配置中 / 加固期 / 队伍正常 / 最终结算否」、
  **实时已连接**（SSE `GET /events/{id}/awd/stream`）、实时积分榜含「我的队伍」高亮、
  GameBox 空态说明「赛事可能尚未部署」。网络凭据在**未分配网络**时给出明确的未分配/失败状态与重试。
- 全站公告 / 解题流水 / Top15 等社区页在空库下显示**空态**而不是假数据。

## 5. 错误可见性

| 场景 | 表现 |
|---|---|
| 后端平台业务失败（HTTP 200 + `code != 0`） | 由 transport 拦截器转成 `FloatCTFError`，页面显示**后端原文**（例如 `路径参数格式错误`、`event network …`）+ 技术细节 + 重试 |
| 404 资源 | `ErrorBlock`「资源不存在」/`NotFoundBlock`（题目不在赛事中、页面不存在） |
| 未授权（401） | 清 token → 跳登录（带 `next`） |
| 权限不足（403） | `PermissionDeniedBlock` / 可见错误，不伪装成空列表 |
| 实时 `auth_error` | `RealtimePill` 显示「实时鉴权失败（需重新登录）」，停止重连并保留轮询降级 |

> 已知契约观察：选手端 `EventInfo` 运行时不返回顶层 `id`（只有 `event.id`），本前端统一用
> `eventIdOf(info)`；这属于平台侧类型/响应漂移，已记录在 README §7。

## 5.1 实时（SSE）

| 通道 | 结果 |
|---|---|
| AWD 选手 `/events/<id>/awd/stream` | ✅ `RealtimePill` 显示「实时已连接」（真实赛事，未部署也建流成功） |
| AWDP 选手 `/events/<id>/awdp/stream` | ✅ 显示「实时已连接」，阶段/分值随接口真实值渲染 |
| AWDP 练习 `/service/awdp/runs/<id>/stream` | ✅ hook 已接线（dev 库无 run，页面为真实空态） |
| 降级 | 非 connected 状态由 `@floatctf/react` 自动降级轮询；`auth_error` 在 UI 上显式可见 |

## 6. 破窗回退

`http://<lan>:13500/?frontend=default` → 加载 **Default Frontend**（`Login | FloatCTF`），
`GET /api/frontend` 的 `active_frontend` **仍为 `shadcn`**（未修改设置、不需要登录）。✅

## 7. 复现步骤

```bash
# 1) 平台与开发栈（本机已有 mise run dev 时跳过）
mise run infra:up && mise run db:migration:apply && mise run dev

# 2) 前端的真实数据播种（只用公共 API；幂等，可重复执行）
python3 var/shadcn-e2e/seed.py        # Jeopardy：2 题 + 1 赛事 + 2 选手 + 1 真实 solve
python3 var/shadcn-e2e/seed-awd.py    # AWD：1 战队赛（仅配置，不 deploy）

# 3) 开发直挂路径
mise exec -- pnpm --filter @floatctf/frontend-shadcn dev     # http://<lan>:13400

# 4) 生产引导链路径（真实 apps/web dist + 本地制品）
mise exec -- pnpm --filter @floatctf/frontend-shadcn build
tar -czf /tmp/shadcn-0.1.0.tar.gz -C frontends/floatctf-frontend-shadcn/dist .
export FLOATCTF_HOME=/tmp/shadcn-bootstrap-home
./scripts/frontend.sh install frontends/default/dist --platform --make-current
./scripts/frontend.sh install /tmp/shadcn-0.1.0.tar.gz
FLOATCTF_HOME=$FLOATCTF_HOME python3 var/shadcn-e2e/bootstrap_server.py 13500
# 管理端设置 FRONTEND_ACTIVE=shadcn（或用管理 UI），然后打开 http://<lan>:13500/
```

## 7.1 截图（真实 API）

`docs/images/` 下为本次验收采集的原始截图（Edge + 真实接口数据）：

| 文件 | 内容 |
|---|---|
| `player-dashboard.png` | 选手总览（真实用户 `E2E Alpha`） |
| `player-event-scoreboard.png` | 赛事工作区 · 积分榜（`#1 E2E Alpha 300.00`） |
| `player-challenges.png` | 题库（2 道真实题目，`base64` 已解出） |
| `player-awd-arena.png` | AWD 驾驶舱（`configuring/hardening`、实时已连接、真实战队与积分榜） |
| `player-rank.png` | Top15 排行 |
| `player-discussions.png` | 讨论区 |
| `admin-dashboard.png` | 控制台总览（真实统计 4 用户 / 6 赛事 / 2 题目 / 1 实例，`需要关注：一切正常`） |
| `admin-event-console.png` | 赛事控制台 · 数据大屏 |
| `admin-awd-ops.png` | AWD 运维（配置态） |

> 刻意未收录：宿主 Docker 容器清单（容器名 / 镜像 / 端口属运维内部信息）与含 flag 明文的页面。
> 采集过程中发现并修复了一个真实缺陷：`SecretValue` 旁边并排的明文 `CopyText` 抵消了「默认模糊」，
> 现已改为**只复制不回显**（含隐藏态 `title` 也不携带明文）。

## 8. 范围声明（诚实性）

1. **AWD 容器级未验收**：本机同时运行生产栈，`awd deploy` 会在共享宿主创建容器与网络，
   因此 AWD 验收覆盖到「配置 / 状态 / 成员 / 战队 / 网络分配前」的真实读取与完整前端状态机；
   GameBox 实例面板、重置、判题以**真实空态 / 未部署态**呈现。
2. AWDP 赛事与练习工作台同样未在宿主上部署靶机容器；其页面读取真实接口（总览/阶段/配置/轮次/
   评测/积分/目录），容器生命周期以真实空态呈现。
3. 上述两项都是**平台侧资源动作**，不是前端缺口；前端对这些状态的处理已按要求实现（可见、可恢复）。
