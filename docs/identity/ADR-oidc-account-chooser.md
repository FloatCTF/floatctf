# ADR — OIDC 账号选择（`prompt=select_account`）与安全切换账号

- 状态：已采纳（2026-10-25）
- 关联：[ADR-oidc-provider.md](ADR-oidc-provider.md)（OP 总体设计）、[oidc-provider.md](oidc-provider.md)（参考手册）
- 范围：FloatCTF OP（本仓库）+ LearnOS RP（`FloatCTF/LearnOS`，登录按钮传 `prompt=select_account`）

## 1. 背景与根因（G0 实证）

用户只在 LearnOS 点过「使用 FloatCTF 登录」，在 FloatCTF `/oauth/login` 输入一次密码；退出 LearnOS 后再点登录，
会被直接带回 LearnOS，没有确认账号的机会。

代码路径：

1. `POST /oauth/login`（`api/protocol.rs::login_submit`）认证成功后 `store::create_session(.., SessionOrigin::OidcLogin)`
   并下发 HttpOnly `__Host-fctf_sso`。这是 FloatCTF **OP 自己的 SSO 会话**，与 FloatCTF SPA 的 JWT（localStorage）
   和 LearnOS 的 Better Auth 会话互相独立。
2. LearnOS 退出（`lib/auth/actions.ts::signOutAction`）只删除 Better Auth 会话；浏览器同源策略也不允许它清除 FloatCTF Cookie。
3. 再次登录时 `GET /oauth/authorize` → `flow::advance`：有效 SSO + 已有覆盖本次 scope 的 consent → 直接 `issue_code`。
   LearnOS 当时没有传 `prompt`，所以这是**规范内的正常 SSO**，不是凭据绕过。
4. 旧实现即使收到 `prompt=select_account`，也只是把它并入 `needs_interaction` 导向 **Consent 页**（"切换账号"是 consent 页上的
   一个链接）——账号选择和授权同意混为一页，且已同意的用户会被迫重复同意。

首次登录与再次登录不同的原因：首次没有 SSO → 登录页（输入密码本身即选择身份）；再次有 SSO → 旧实现静默发码。

## 2. 决策

### 2.1 三个独立阶段

授权事务按 **账号选择 → 认证（登录 / 重新认证）→ 授权同意 → 发码** 推进，由纯函数 `flow::decide` 给出下一步，
`flow::advance` 只负责执行（绑定事务、发码、回错误）。决策优先级：

1. `id_token_hint` 指向其他用户 → 当前 SSO 视为不可用。
2. 本事务内用户已明确选择「使用其他账号」且尚未在本事务内输入新密码 → 当前 SSO 视为不可用（不得被旧 Cookie 短路）。
3. 有可用 SSO 且 `prompt` 含 `select_account`、本事务尚未确认该会话 → **账号选择页**（`prompt=none` 不可能与之共存；
   防御性地返回 `account_selection_required`）。
4. 无可用 SSO，或 `prompt=login` / `max_age` 要求重新认证 → 登录页（`prompt=none` → `login_required`）。
   无 SSO 时输入密码即完成本次选择，不会再多弹一个空的选择页。
5. `prompt=consent` 或已有授权不覆盖本次 scope → 同意页（`prompt=none` → `consent_required`）。
   **`select_account` 不再触发同意页**。
6. 否则发码。

| prompt | 有 A 的 SSO 时的页面序列 |
| --- | --- |
| （无） | 已同意 → 直接回调（与旧版逐字一致，保护其他 RP） |
| `select_account` | 选择页 →（继续 A）→ 已同意则直接回调 |
| `select_account consent` | 选择页 → 同意页（两页独立） |
| `select_account login` | 选择页 → 重新认证页（预填 A 用户名，仍必须输入密码） |
| `login` | 重新认证页（不显示选择页） |
| `consent` | 同意页（不显示选择页） |
| `none` | 永不渲染 UI：满足条件发码，否则 `login_required` / `consent_required` |

### 2.2 路由

新增第一方页面（MiniJinja 服务端渲染，与登录/同意页同一 CSP、`no-store`、`Referrer-Policy: same-origin`）：

```
GET  /oauth/select-account?txn=<事务 id>   纯渲染，从不改变选择、从不发码
POST /oauth/select-account                 action = continue | other | cancel
```

已有 `/oauth/authorize|login|consent|token|userinfo|logout` 契约不变；Discovery 不新增字段（这是 OP 内部交互页）。
`/oauth/*` 已整体由 Caddy 反代到 API，无需改代理配置。

- `continue`：服务端从 SSO Cookie 解析当前会话；必须**等于 GET 时展示给用户的那个会话**（展示时写入事务）。
  表单不携带任何用户 ID。条件 UPDATE（未完成、未过期、未请求切换、展示会话一致、CSRF 摘要一致）原子写入选择并轮换 CSRF，
  所以重复点击 / 并发 POST 只有一个能推进；之后发码仍受 `complete_transaction` 一次性约束。
  会话在 GET 与 POST 之间变化（另一标签切换了账号）→ 不选择任何人，重新渲染选择页并提示「账号已变化，请重新确认」。
- `other`：条件 UPDATE 标记「本事务要求切换账号」并清空任何已有选择，跳转登录页。之后旧 SSO 在本事务内不可用，
  直到用户在登录页输入（任意）账号密码；新身份的 consent 按其自身记录计算。
- `cancel`：结束事务，302 到已验证的 `redirect_uri`，携带 `error=access_denied`、原始 `state` 和 `iss`。

`POST /oauth/consent` 的旧 `action=switch`（已渲染的旧页面上的「切换账号」）改为走同一个「切换账号」状态转移，
不再直接跳登录页绕过新状态机。同意页只负责授权。

### 2.3 数据模型（forward-only 迁移 `oidc-account-chooser`）

`oidc_auth_transactions` 增加 4 个可空列，旧行 / 旧代码均兼容（全部 NULL = 未选择、未切换）：

| 列 | 含义 |
| --- | --- |
| `account_choice_session_id` | 账号选择页最近一次展示的 SSO 会话（`continue` 必须与之相同） |
| `account_selected_at` | 用户在本事务内确认账号的时间（选择页「继续」或本事务内输入密码） |
| `account_selected_session_id` | 被确认的 SSO 会话；只认会话 id，用户由会话行决定 |
| `account_switch_requested_at` | 用户选择「使用其他账号」的时间；之后旧 SSO 在本事务内不可短路 |

外键 `ON DELETE SET NULL` 指向 `oidc_browser_sessions`。不改已有迁移、不改 `merged.sql`。

### 2.4 功能开关与回滚

`[oidc].account_chooser`（TOML，默认 `true`）。设为 `false` 时 `select_account` 回到旧行为（导向同意页），
**仅作为明确标注的临时兼容模式**；其余流程不受影响。迁移列在开关关闭时保持 NULL，可以留在库里。

### 2.5 会话边界（不变）

- 三种会话互不推断：FloatCTF SPA JWT、FloatCTF OP SSO Cookie、RP（LearnOS Better Auth）会话。
- `POST /oauth/logout` 仍只撤销本地 SSO；本次**不实现** RP-Initiated / Back-Channel / Front-Channel Logout，
  不伪造全局退出。
- 登录页成功后仍撤销同一浏览器的旧 SSO 会话（单当前会话模型）；因此切换到 B 后，A 需要再次输入密码。

## 3. 安全不变量

- 所有选择页动作先验证：事务存在、未完成、未过期、同一浏览器绑定、CSRF、`Origin`/`Sec-Fetch-Site`、client 仍为 active
  （`find_live_transaction` 联表取 client 后在 `advance`/发码路径中使用；禁用的 client 在 `/oauth/authorize` 即被拒）。
- 只有有效 SSO（未过期、未撤销、用户仍存在）才会被展示为可继续的账号；被删除的用户不会出现。
- 选择页只显示当前浏览器自己的 SSO 用户（昵称、@用户名、同源头像），不展示邮箱、其他账号或「最近使用账号」列表。
- `id_token_hint` 指定 A 时，用 B 的密码登录**不会**建立 B 的 SSO、也不会发码，登录页提示需使用指定账号。
- 同意记录维度仍是 `(user, client)`；切换账号不继承旧账号 scope。
- 回调只带 `code|error`、`state`、`iss`，不带用户名、UID、邮箱或任何秘密。

## 4. 明确不做（非目标）

- 同一浏览器同时保持多个可免密切换的已登录账号（Google 式多会话）。这需要多会话记录、浏览器不可读的账户索引、
  移除/撤销、共享设备提示与独立安全评审，作为后续单独提案（PLAN P5）。**选择页只会出现一个账号**，不伪造账号列表。
- One Tap、自动登录、统一退出、社交头像同步。

## 5. 后果

- LearnOS 主动点击登录 → 有 SSO 时一定先看到 FloatCTF 的选择页；继续当前账号免密码、已同意时免重复同意。
- 未传 `select_account` 的第三方 RP 行为不变（集成测试 `legacy_rp_without_prompt_keeps_silent_sso` 守护）。
- 旧 LearnOS（不传 prompt）在新 OP 上照常工作；新 LearnOS 在旧 OP 上只会看到旧的同意页（体验退化，不越权）。
