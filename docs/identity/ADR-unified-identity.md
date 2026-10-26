# ADR — FloatCTF 统一身份会话、多账号与标准退出

- 状态：已采纳（2026-10-25），分支 `feat/unified-identity`
- 关联：[unified-identity-audit.md](unified-identity-audit.md)（P0 审计、威胁模型、执行矩阵）、
  [ADR-oidc-account-chooser.md](ADR-oidc-account-chooser.md)（选择页状态机，本 ADR 在其上扩展）、
  [ADR-oidc-provider.md](ADR-oidc-provider.md)（OP 总体设计）
- 范围：FloatCTF API（OP + 主站会话）、Default Frontend、`@floatctf/sdk`；LearnOS（RP）见其仓库 `docs/floatctf-unified-logout.md`

## 1. 决策摘要

1. **唯一身份会话源**：`oidc_browser_sessions` 的一行 = 一个浏览器里一个账号的一次认证（独立 `auth_time`、过期、撤销、对外 `sid`）。
   主站与 OP 读写同一行；撤销即两边同时失效。
2. **浏览器账号容器**：浏览器只持有一个 HttpOnly Cookie `__Host-fctf_accounts`（随机 256 bit，库内只存 SHA-256），
   指向 `oidc_browser_containers` 一行；容器内挂多个账号会话（默认上限 5）。Cookie 从不携带用户名、用户 ID 或多个令牌。
3. **主站活跃账号**是容器上的一个指针 `web_session_id`。只有主站自己的登录 / 切换 / 恢复会改它；
   **RP 授权时选择账号不会改动主站当前账号**。
4. **主站认证双栈**：`UserJwtGuard` 优先 Bearer（CLI、第三方前端、旧前端完全不变）；
   请求**没有** `Authorization` 头且部署开启 `unified_web_session` 时，走 Cookie 通道：
   第一方来源 + `X-CSRF-Token`（= SHA-256(域分隔 ‖ 容器秘密)）+ 可选 `X-FloatCTF-Account`（期望账号）。
   `SuperAdminJwtGuard` 不读 Cookie：普通用户的身份会话永远得不到管理员权限。
5. **浏览器不再拿到平台 JWT**：新主站登录接口响应体只有状态，不含令牌；旧 `auth-storage.token` 经一次性迁移换成服务端会话后删除。
6. **三档退出语义**严格分开：退出 LearnOS（只删 Better Auth 会话）/ 退出 FloatCTF（撤销本浏览器该账号的身份会话，不通知 RP）/
   退出所有关联应用（撤销 + 标准 Back-Channel Logout，独立按钮 + 确认）。详见 §5。
7. **三个开关，默认关闭**，按 DB → OP → 主站 → LearnOS → 开关 的顺序上线；关闭即回到旧行为，不删数据：

   | 开关 | 打开后 | 关闭后 |
   |---|---|---|
   | `[oidc].multi_account` | 容器 Cookie、多账号选择页、旧 `__Host-fctf_sso` 自动迁入容器 | 旧版单会话 `__Host-fctf_sso` |
   | `[oidc].unified_web_session`（要求 `multi_account`） | 主站 Cookie 会话、`/api/users/web-session/*` | 状态接口返回 `mode=legacy`，主站继续用 JWT |
   | `[oidc].backchannel_logout` | `end_session_endpoint`、ID Token `sid`、Back-Channel 投递、主站「退出所有关联应用」 | Discovery 不宣告上述能力 |

## 2. 数据模型（迁移 `20261025201938-unified-identity-sessions.sql`）

```text
oidc_browser_containers (新)            oidc_browser_sessions (扩展)
  id                                      id, user_id, auth_time, established_via, expires_at, revoked_at
  secret_hash  ← __Host-fctf_accounts     container_id  → containers.id (SET NULL)
  web_session_id → sessions.id (SET NULL) sid           对外稳定标识（ID Token sid / Logout Token sid）
  web_signed_out_at                       revoked_reason logout | logout_browser | global_logout | replaced | evicted | legacy_logout
  expires_at (滑动 30 天), revoked_at
```

- 只新增表与列；旧会话行自动获得随机 `sid`；旧二进制不读新列，可直接回滚。
- 清理：OIDC 清理任务额外删除过期/撤销满 7 天的容器。

## 3. 主站会话接口（`/api/users/web-session`）

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `` | 状态：`mode`、`authenticated`、`user`、`csrf_token`、本浏览器真实有效的 `accounts`、`auto_restore`、`max_accounts`、`global_logout`。从不改变状态 |
| POST | `/login` | 口令登录；账号加入本浏览器并成为主站当前账号；共用 OIDC 登录限流（Redis，fail-closed） |
| POST | `/restore` | 恰好一个可恢复账号时恢复主站登录；多个 → 409（必须由用户选择） |
| POST | `/select` | 切换到本浏览器中的另一个账号（用户明确点击） |
| POST | `/logout` | `scope=account`（默认当前账号）/ `browser`（本浏览器全部账号）；撤销成功才返回 200 |
| POST | `/migrate` | 只接受 Bearer 平台 JWT：把旧 localStorage 令牌换成服务端会话（`auth_time` = JWT 签发时间） |

所有 POST：第一方 `Origin`（issuer 或 `[oidc].first_party_origins`）/ `Sec-Fetch-Site: same-origin` + `X-CSRF-Token`。
浏览器尚无容器时（首次登录/迁移）只要求存在该自定义头——跨站页面无法不经 CORS 预检发送它，CORS 只放行运维配置的 origin。

**自动恢复规则**（U03/U12）：`web_signed_out_at` 记录主站最近一次明确退出；只有在它之后建立的会话才算「可恢复」，
且只有恰好一个时才自动恢复。所以：在 LearnOS 的 FloatCTF 登录页输入密码 → 打开主站自动恢复；
在主站退出 A 后，浏览器里早已存在的 B 不会被悄悄恢复，必须由用户点选。

**多标签**：主站每个请求携带 `X-FloatCTF-Account`；另一标签页切换账号后，旧标签页的请求返回 401，前端重新读取状态、清空查询缓存；
`BroadcastChannel` 负责即时通知其他标签页。

## 4. OIDC 授权事务（在账号选择状态机上扩展）

- `flow::evaluate` 输入从「当前一个 SSO 会话」变为「本浏览器的账号列表」：
  先按 `id_token_hint`、「使用其他账号」过滤出候选；本事务已确认/刚输入密码的会话优先；
  只有一个候选 → 与旧版完全一致；多个且 `login_hint` 恰好匹配其一 → 它；否则 **ambiguous**。
- `decide` 新增 `ambiguous`：任何交互 prompt 都先进选择页；`prompt=none` → `account_selection_required`。
  多账号时绝不按最近使用/创建顺序替用户挑选。
- 选择页：每个账号是整行可点击的表单按钮，提交 `action=continue&account=<会话 id>`；服务端校验它属于本浏览器、仍有效、未被 hint 排除，
  再用条件 UPDATE 一次性确认（并发只成功一次）。上线前渲染、没有 `account` 字段的旧页面沿用「展示时的会话」校验。
- 新动作：`logout`（退出本浏览器中的某个账号，同时退出主站中的该账号）、`logout_all`；都**不**通知 RP，也不发码；
  之后仍有账号 → 重新显示选择页（由用户点选），否则 → 登录页。GET 选择页即使状态机已可发码也只渲染，从不发码。
- OIDC 登录页输入密码：多账号模式下把账号加入容器（同一用户旧会话 `replaced`、超上限淘汰最久未用 `evicted`），
  **每次口令认证都轮换容器秘密**（防会话固定）。

## 5. 退出语义

| 操作 | 撤销 | 通知 RP | 入口 |
|---|---|---|---|
| 退出 LearnOS | Better Auth 会话 | 否 | LearnOS「退出」（Better Auth `disableProviderLogout: true`） |
| 退出 FloatCTF（本账号） | 本浏览器该账号的身份会话；主站同时退出 | 否；LearnOS 已有会话保留，之后再经 OIDC 需重新认证 | 主站账号菜单、选择页「退出账号」 |
| 退出本浏览器全部账号 | 容器及其全部会话 | 否 | 主站账号菜单、选择页 |
| 退出所有关联应用 | 本浏览器该账号的身份会话 | **是**：对拿到过该 `sid` 令牌、且登记了 `backchannel_logout_uri` 的应用投递 Logout Token | 主站独立按钮 + 确认；RP 发起的 `end_session_endpoint` + OP 确认页 |

「退出所有关联应用」的 RP 范围、投递、SSRF 防护、重试与诚实报告见 §6（P4）。

## 6. 标准退出（P4）

见本文[附录 A](#附录-a标准退出rp-initiated-logout-10--back-channel-logout-10)。

## 7. 部署与 Issuer

- 统一主站会话要求主站 origin 与 OP 位于**同一主机**（host-only Cookie 只在同主机间共享；端口不隔离 Cookie）。
  120 入口同源；10.66 入口主站 `https://10.66.66.7`、issuer `https://10.66.66.7:8443`，需要
  `first_party_origins = ["https://10.66.66.7"]`。配置校验拒绝不同主机。
- 本轮不改任何生产 issuer；canonical issuer 方案、双 issuer 兼容与上线/回滚步骤见 [oidc-operations.md](oidc-operations.md) §统一身份。
- 已知风险：同一主机不同端口的其他应用（例如同 IP 的 LearnOS:8080）会收到 FloatCTF 的 Cookie（浏览器 Cookie 不按端口隔离）。
  这不是本次引入的问题（旧 `__Host-fctf_sso` 同样如此），正式消除需要为 FloatCTF 与 LearnOS 使用不同主机名。

## 8. 被否决的方案

- **主站直接读 Bearer + 把 JWT 放 Cookie**：JWT 无法服务端撤销（退出后 8h 内仍有效），违背「可撤销的唯一会话源」。
- **主站作为 OP 的 RP（自己走一遍 OIDC）**：同源场景下多一次重定向与令牌兑换却没有安全收益；仍需要第一方会话存放 RP 结果。
- **每个账号一个 Cookie**（`fctf_sso_0..n`）：Cookie 数量随账号增长、无法原子轮换、JS 外的列表管理困难。
- **共享 Cookie 给 LearnOS**：违背「共享的是 OP 身份会话，由各 RP 通过标准 OIDC 换取自己的会话」。

## 附录 A：标准退出（RP-Initiated Logout 1.0 + Back-Channel Logout 1.0）

迁移 `20261025211040-oidc-standard-logout.sql`（只新增表与可空/默认列）：

| 表 / 列 | 用途 |
|---|---|
| `oidc_clients.post_logout_redirect_uris jsonb default '[]'` | RP-Initiated Logout 回跳地址（逐字匹配） |
| `oidc_clients.backchannel_logout_uri text null` | Back-Channel 接收地址（登记期：https 绝对 URL、无 userinfo / fragment） |
| `oidc_session_clients (session_id, oidc_client_id)` | 某个身份会话（`sid`）向哪些应用签发过令牌——全局退出的通知范围 |
| `oidc_logout_requests` | end_session 的一次性确认事务（10 分钟、浏览器绑定、CSRF） |
| `oidc_backchannel_deliveries` | 投递 outbox：每个 (应用, sid) 一行，`pending → delivered / failed`，`attempts`、`next_attempt_at`、`last_error`、`last_jti` |

**`sid`**：开关打开后，ID Token 带 `sid`（= 身份会话的 `sid` 列），Discovery 宣告
`end_session_endpoint`、`backchannel_logout_supported = true`、`backchannel_logout_session_supported = true`，并在 `claims_supported` 中加入 `sid`
（不实现 Front-Channel Logout，不宣告该能力）。
LearnOS 用 `(iss, sid)` 把自己的 Better Auth 会话映射到 FloatCTF 身份会话。

**RP-Initiated Logout（`GET|POST /oauth/end-session`）**

1. `id_token_hint` 必须是本 OP 签发的 ID Token（允许已过期）；同时给出 `client_id` 时必须等于其 `aud`。
2. `post_logout_redirect_uri` 必须与应用登记值逐字相等，否则不回跳（只显示结果页）。
3. 只作用于**本浏览器** Cookie 中与 hint 的 `sid`/`sub` 匹配的会话；没有 hint 时作用于本浏览器全部账号（确认页逐个列出）。
   第三方站点无法借此退出别人的身份，也无法跳过 OP 确认页。
4. 确认页（`end_session.html`）：「取消，保持登录」/「退出」；一次性事务 + 浏览器绑定 + CSRF；GET 从不改变状态。
5. 退出后：所有关联应用都确认收到通知 → 直接回到登记的 `post_logout_redirect_uri`（带原 `state`）；
   有应用未确认或不支持标准退出 → 停在结果页逐个如实列出，再给出「返回应用」。
6. 旧的 `POST /oauth/logout`、`DELETE /api/users/sso` 保留兼容，**不是**标准端点，也不触发通知。

**Back-Channel Logout（OP → RP，服务端到服务端）**

- 只在用户明确选择「退出所有关联应用」（主站按钮 + 确认，或 end_session 确认）时触发；普通退出从不通知 RP。
- 范围：被撤销的那个 `sid` 在 `oidc_session_clients` 中关联、且仍 active、登记了 `backchannel_logout_uri` 的应用；
  拿到过令牌但没登记接收地址的应用记入 `without_backchannel`，界面明确提示「可能仍保持登录」。
- Logout Token（RS256，`typ=logout+jwt`）：`iss`、`aud=client_id`、`iat`、`exp`（默认 120 秒）、`jti`、`sid`、`sub`、
  `events = {"http://schemas.openid.net/event/backchannel-logout": {}}`，**没有** `nonce`。
- 投递：`application/x-www-form-urlencoded` 的 `logout_token`；2xx = delivered；400 = RP 拒绝（failed，不重试）；
  其他状态 / 网络错误 = pending，按 30s、2m、10m、30m、2h 退避，最多 6 次后 failed。
  后台 worker 每 30 秒领取到期行（`FOR UPDATE SKIP LOCKED` + 120 秒租约，多实例不重复投递）；只投递本实例 issuer 的行。
- 出站安全（SSRF）：自建 DNS 解析器只放行公网地址并直接用解析结果连接（防 DNS 重绑定）；IP 字面量同样校验；
  内网 RP 只能由运维在 `[oidc].backchannel_trusted_hosts`（精确 `host:port`）放行，应用自己无法声明；
  禁止重定向、不走代理、连接/总超时（`backchannel_timeout_secs`）、只读状态码。被策略拒绝的目标记 failed（「非公网地址」），不重试。
- 结果如实返回：主站「退出所有关联应用」对话框与 OP 结果页逐个显示 已确认退出 / 未送达，将自动重试 / 未能确认。

**LearnOS 侧**（其仓库 `docs/floatctf-unified-logout.md`）：`POST /api/oidc/backchannel-logout` 用 JWKS 验签并校验
`typ`、受信 issuer、`aud`、`iat/exp`、`events`、无 `nonce`、`sid` 或 `sub`、`jti` 防重放，只删除映射到 `(iss, sid)`
（只有 `sub` 时为 `(iss, sub)`）的 Better Auth 会话；Better Auth 未开启 cookie cache，删除即刻生效。

**上线顺序约束（I11）**：Better Auth 只要在 Discovery 里看到 `end_session_endpoint` 就会把普通 `signOut` 变成 RP-Initiated Logout。
所以必须**先**上线带 `disableProviderLogout: true` 的 LearnOS，**再**打开 OP 的 `backchannel_logout`。

