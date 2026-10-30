# 统一身份（FloatCTF × LearnOS）P0 审计、威胁模型与执行矩阵

- 日期：2026-10-25
- 审计基线：FloatCTF `upstream/main` = `64cb4bb`（1.0.2，已含账号选择 PR #9）；LearnOS `upstream/main` = `edde3aa`（0.1.2，已含 PR #1）
- 工作分支：两仓库均为 `feat/unified-identity`（从上述 HEAD 切出，工作区干净，无用户未提交改动）
- 关联：[ADR-unified-identity.md](ADR-unified-identity.md)（决策与设计）、[ADR-oidc-account-chooser.md](ADR-oidc-account-chooser.md)（已上线的选择页，**不重做**）

本文是 G0 门禁产物：问题清单、真实 origin/issuer 图、存储与迁移风险、威胁模型、兼容矩阵、A01–A20 执行矩阵。
所有结论都给出源码位置；没有源码依据的部署事实标注为「部署记录」（来自 LearnOS `deploy/prod/README.md`）。

## 1. 已完成、必须保留的能力（避免重复开发）

| 能力 | 位置 | 处理 |
|---|---|---|
| `prompt=select_account` 独立选择页 + 状态机（选择 → 认证 → 同意 → 发码） | `oidc/flow.rs::decide/evaluate/advance`、`api/protocol.rs::select_account_*`、迁移 `20261025145704-oidc-account-chooser.sql` | 保留状态机与全部断言；在其上扩展多账号候选与「退出账号」动作 |
| 「使用其他账号」强制输入密码、旧 Cookie 不得短路 | `flow::usable_session`、`store::request_account_switch` | 保留 |
| 并发/双击只确认一次、CSRF 轮换、浏览器绑定 `__Host-fctf_bid` | `store::confirm_account_selection`、`find_live_transaction` | 保留，多账号版本沿用同一条件 UPDATE 模式 |
| LearnOS 交互登录传 `prompt=select_account`，LearnOS 退出只结束 Better Auth 会话 | LearnOS `lib/auth/floatctf-request.ts`、`lib/auth/sign-out.ts` | 保留；新增的「退出所有关联应用」是**独立**入口 |
| 真实跨仓库 E2E | LearnOS `tests/e2e/floatctf-account-chooser.spec.ts`、`floatctf-sso.spec.ts`；FloatCTF `apps/api/tests/oidc_account_chooser.rs` | 不删、不降级断言 |

## 2. 问题清单（P0 实证）

| # | 问题 | 证据 | 影响的场景 |
|---|---|---|---|
| I1 | 主站登录态 = `localStorage["auth-storage"].token`（8h HS512 JWT），路由守卫只看它 | `frontends/default/src/stores/AuthStore.ts:31`、`routes/service/route.tsx:20`、`api/client.ts:25`、`api/react.ts:19` | U01/U03/U12：OP 会话与主站互不认识 |
| I2 | OIDC 登录页建立的 SSO（`SessionOrigin::OidcLogin`）不会反向建立主站登录 | `api/protocol.rs::login_submit` 只下发 `__Host-fctf_sso`；主站无任何接口读取它 | U03：LearnOS 登录后打开主站仍要求密码 |
| I3 | 主站登出 `void serviceApi.oidc.endSso().catch(() => undefined)`，不等待、吞错 | `components/Header.tsx:129` | U07/A06：主站显示已退出，SSO 可能仍有效 |
| I4 | JWT 无服务端撤销：登出后 8h 内令牌仍然有效 | `core/security/jwt.rs::validate_jwt`、`api/extractor/auth.rs` | U12：退出后旧令牌仍可用 |
| I5 | 一个浏览器只有一个 SSO 会话；登录 B 会撤销 A | `api/session.rs::establish_after_password_login`、`protocol.rs:508` | U11：无多账号 |
| I6 | 选择页无「退出当前 FloatCTF 账号」 | `templates/select_account.html` | U07 |
| I7 | 品牌是 CSS 蓝色方块 `.brand .dot`，不是官方 `/float.png` | `templates/layout.html` | U10 |
| I8 | 选择页头像直接引用用户数据的 `/public/images/...`；失败时显示破图 | `protocol.rs::same_origin_avatar` + `<img src="{{ avatar }}">` | U10/A15 |
| I9 | **破图根因（部署）**：内网 issuer `https://10.66.66.7:8443` 的 Caddy 站点把**所有路径**反代给 API（`reverse_proxy 127.0.0.1:9092`），`/public/*`、`/float.png` 都落到 API → 404 | LearnOS `deploy/prod/floatctf-Caddyfile` 末尾 `https://10.66.66.7:8443` 块 | U10/A15/A17 |
| I10 | 没有标准 `end_session_endpoint`（RP-Initiated Logout），没有 Back-Channel Logout，ID Token 无 `sid` | `domain/metadata.rs`（测试断言 `end_session_endpoint` 不存在）、`domain/claims.rs::IdTokenClaims` | U08/A12–A14 |
| I11 | **Better Auth 1.7.12 的 `signOut` 会自动构造 RP-Initiated Logout URL**，只要 Discovery 出现 `end_session_endpoint` | LearnOS `node_modules/better-auth/dist/api/routes/sign-out.mjs:55-90`、`plugins/generic-oauth/index.mjs:159` | U04/U05：OP 一旦宣告端点，LearnOS 普通退出就可能变成全局退出 → LearnOS 必须先设 `disableProviderLogout: true` 再开启 OP 端点 |
| I12 | 主站 origin 与 issuer 不同源时（10.66 入口：主站 `https://10.66.66.7`，issuer `https://10.66.66.7:8443`），现有 `same_origin()` 用 issuer 比较 `Origin`，主站发起的 `DELETE /api/users/sso` 会 403 | `protocol.rs::same_origin`；部署记录 | U07 在 10.66 入口失效 |
| I13 | 平台登录接口 `/api/users/session` 无登录限流（OIDC 登录页有） | `authentication/mod.rs::user_login` | 新的 Cookie 登录入口须补限流 |
| I14 | 头像上传扩展名取自用户文件名，任意扩展名写入公开桶 | `platform/files/upload.rs::gen_image_path` | 选择页不得直接信任该路径与 MIME（本轮由安全头像端点白名单 + 魔数校验兜住；上传侧问题记录为残留风险，不在本轮范围） |

## 3. 认证面清单（P0-2）

### 3.1 浏览器侧

| 读写点 | 内容 |
|---|---|
| `stores/AuthStore.ts` | Zustand persist → `localStorage["auth-storage"]` = `{token, adminToken, username, nickname}` |
| `routes/index.tsx` | 登录成功 `setToken(jwt)` + `setUsername` |
| `routes/service/route.tsx` | `beforeLoad` 只看 `token`；`useSsoBridge` 调 `POST /api/users/sso` |
| `routes/admin/route.tsx`、`routes/admin/index.tsx` | 只看 `adminToken`（超管，独立） |
| `api/client.ts` | `getUserToken/getAdminToken`；401 → 清 token 跳转 |
| `api/react.ts` | `useUserToken` 作为 SSE 连接开关 |
| `components/Header.tsx` | 登出：`endSso()` 不等待 + `removeToken()` |
| `routes/service/awdp/runs.$runId/writeup.tsx` | 读 `nickname`（展示） |

### 3.2 SDK

- `packages/sdk/src/transport.ts::attachInterceptors`：`Authorization: Bearer <getUserToken()>`；401 → `onUnauthorized`。
- `packages/sdk/src/sse/connectSse.ts::buildHeaders`：fetch-based SSE，同样只发 Bearer。
- `packages/sdk/src/api/service/oidc.ts`：`establishSso` / `endSso`。

### 3.3 服务端

| 守卫/处理器 | 机制 |
|---|---|
| `api/extractor/auth.rs::UserJwtGuard` | 仅 `Authorization: Bearer`，`Role::User`，查 `users` |
| `api/extractor/auth.rs::SuperAdminJwtGuard` | 仅 Bearer，`Role::SuperAdmin`，查 `super_admin`（**本轮不改**） |
| `modules/event/awd/api/auth.rs`、`awdp/api/internal.rs` | 裁判机内部令牌（与用户会话无关） |
| `POST /api/users/session` | 口令 → JWT；OIDC 启用时附带 `__Host-fctf_sso` |
| `POST/DELETE /api/users/sso` | JWT → SSO 桥接 / 撤销 SSO |
| `POST /oauth/login` | 口令 → SSO（`oidc_login`） |
| `POST /oauth/logout` | 撤销 SSO（非标准，保留兼容） |

### 3.4 Cookie

| Cookie | 属性 | 来源 |
|---|---|---|
| `__Host-fctf_sso`（http loopback 为 `fctf_sso`） | HttpOnly、Secure、SameSite=Lax、Path=/、无 Domain、Max-Age=session_ttl | `protocol.rs::build_cookie` |
| `__Host-fctf_bid` | 同上，30 天；**只做事务浏览器绑定，从不作为认证凭据** | 同上 |

`secure_cookies()` = issuer 以 `https://` 开头。Cookie 不区分端口：`10.66.66.7:443` 与 `10.66.66.7:8443` 共享同一 host-only Cookie。

## 4. 部署 origin / issuer 图（P0-3）

```text
                       浏览器 origin                         API 实例      issuer（ID Token iss）
FloatCTF 主站 (120)    https://120.95.81.151                9091          https://120.95.81.151        ← 同源
FloatCTF 主站 (10.66)  https://10.66.66.7                   9092          https://10.66.66.7:8443      ← 同主机、不同端口
OP 页面    (10.66)     https://10.66.66.7:8443  (Caddy 全量反代到 9092；/public、/float.png 404)
LearnOS    (120)       https://120.95.81.151:8080 → :3001   FLOATCTF_OIDC_ISSUER=https://120.95.81.151
LearnOS    (10.66)     https://10.66.66.7:8080   → :3000    FLOATCTF_OIDC_ISSUER=https://10.66.66.7:8443（部署记录）
数据库                 floatctf_db、learnos 由两套实例共享
```

- 安装器内嵌 Caddy（`scripts/install.sh::write_caddy_template`，镜像 `infra/caddy/Caddyfile.prod`）：`/api/*`、OIDC 路径 → API；`/public/*` → RustFS `floatctf-public`；其余 → `/srv-web`（`/float.png` 来自 `apps/web/public/float.png`）。单站点部署下 OP 页面与主站同源，`/float.png`、`/public/*` 可用。
- 生产实际运行的是 LearnOS 仓库 `deploy/prod/floatctf-Caddyfile`（手工替换，见 LearnOS 部署记录），**不是**安装器模板。
- 双 issuer：两个实例共享一个 FloatCTF 用户库，`sub` = 用户 UUID 在两侧相同；LearnOS 以 `(providerId='floatctf', accountId=sub)` 识别外部账号，两侧映射到同一 LearnOS 用户（部署记录 + `lib/auth/better.ts`）。不同主机名的 Cookie 天然隔离：120 与 10.66 之间登录态不共享是浏览器同源机制，不是缺陷。

**结论**：统一主站会话要求「主站 origin 与 OP 处于同一主机」（Cookie 才能共享）。120 入口满足（同源）；10.66 入口同主机不同端口，满足 Cookie 共享，但须把 `https://10.66.66.7` 显式登记为第一方 origin（新配置 `[oidc].first_party_origins`）。本轮**不改任何生产 issuer**。

## 5. 数据与迁移风险（P0-4）

| 表 | 现状 | 本轮变更 | 风险/对策 |
|---|---|---|---|
| `oidc_browser_sessions` | 单会话，`session_hash` 唯一 | 只加可空/默认列：`container_id`、`sid`（默认随机）、`revoked_reason` | 旧行自动获得 `sid`；旧 API 不读新列 → 回滚安全 |
| （新）`oidc_browser_containers` | — | 浏览器账号容器，`__Host-fctf_accounts` 摘要 | 新表，回滚时闲置 |
| `oidc_auth_transactions` | 已有选择列 | 不改结构 | — |
| `oidc_clients` | — | 加 `post_logout_redirect_uris jsonb default '[]'`、`backchannel_logout_uri text null` | 旧 API 忽略新列 |
| （新）`oidc_session_clients`、`oidc_logout_requests`、`oidc_backchannel_deliveries` | — | sid ↔ RP 绑定、end-session 事务、投递 outbox | 新表 |
| LearnOS `session`（Better Auth） | `token` 唯一 | 不改 | — |
| （新）LearnOS `floatctf_oidc_sessions`、`floatctf_logout_jti` | — | Better Auth 会话 ↔ `(iss, sid, sub)`；`jti` 防重放 | `ON DELETE CASCADE` 跟随会话删除 |

不修改、不删除任何已有迁移；不清空旧会话；不改 LearnOS `account`、`profiles`、进度/积分/笔记表。

## 6. 威胁模型（P0-5）

| 威胁 | 场景 | 对策（实现位置见 ADR） | 验收 |
|---|---|---|---|
| 账号混淆 | 选择页提交的账号与渲染时不同；多标签切换 | 表单提交明确的会话 id，服务端校验属于本浏览器容器且有效；主站请求带 `X-FloatCTF-Account`，与服务端活跃主体不一致返回 401 | A07/A08 |
| 会话固定 | 攻击者预置容器 Cookie | 每次口令认证轮换容器 secret；容器只引用本浏览器认证得到的会话 | A08/A20 |
| Login CSRF | 跨站把受害者登录到攻击者账号 | Cookie 登录接口要求同源 `Origin`/`Sec-Fetch-Site` + 自定义头（触发预检，CORS 不放行） | A20 |
| CSRF | 跨站利用 Cookie 调用写接口 | Cookie 认证的**所有**请求要求 `X-CSRF-Token` = H(容器 secret)，且 `Origin` ∈ 第一方 origin；Bearer 请求不变 | A20 |
| SSRF | 恶意应用登记内网 `backchannel_logout_uri` | 登记校验（https、无 userinfo/fragment）；投递时自建 DNS 解析器拒绝私网/环回/链路本地/保留地址，除非 `host:port` 在运维 TOML 白名单；禁止重定向；超时与限流 | A20 |
| Cookie scope | 子域/他站读取 | `__Host-` 前缀、HttpOnly、Secure、SameSite=Lax、Path=/、无 Domain | A20 |
| 跨用户撤销 | 退出 A 误伤 B / 其他设备 | 撤销粒度 = 会话行；全局退出只针对本浏览器选中的会话 `sid`；Logout Token 带 `sid`，LearnOS 只删映射到该 `(iss, sid)` 的会话 | A09/A13 |
| 恶意 RP 发起登出 | 第三方用 end-session 退出他人 | 只对本浏览器 Cookie 绑定的会话生效，`id_token_hint` 必须验签且与会话 `sid`/`sub` 一致，并要求用户在 OP 页面确认（CSRF + 一次性事务） | A12 |
| 开放重定向 | `post_logout_redirect_uri` 任意跳转 | 必须与应用登记值逐字相等，且应用由 `id_token_hint.aud` 或 `client_id` 确定 | A12 |
| Logout Token 伪造/重放 | 伪造、过期、错 aud、重复 jti | LearnOS：JWKS 验签（RS256）、`typ=logout+jwt`、iss ∈ 受信 issuer、aud = client_id、`iat`/`exp` 窗口、`events` 正确、无 `nonce`、`jti` 唯一表 | A12 |
| 双 issuer 合并 | 不同 issuer 身份被静默合并 | 不改 issuer；LearnOS 映射表按 `(iss, sid)`；受信 issuer 列表显式配置 | A17 |
| 权限提升 | 用户 Cookie 访问管理端 | `SuperAdminJwtGuard` 不读 Cookie；Cookie 只能得到 `Role::User` 等价主体 | A19 |
| 头像 XSS/SSRF | 头像路径指向外站、`javascript:`、SVG | 选择页只引用 OP 自己的头像端点；端点只读 RustFS 公开桶 `images/` 下白名单文件名，魔数校验 PNG/JPEG/GIF/WebP，`nosniff` + `default-src 'none'` | A15 |
| Cookie Cache 绕过 | LearnOS 删除会话后缓存仍放行 | LearnOS 未开启 `session.cookieCache`；撤销后用 `disableCookieCache` 复核 + E2E 断言 | A12/A13 |

## 7. 兼容矩阵

| 客户端 | 行为 |
|---|---|
| 旧 Default Frontend（已缓存） | `/api/users/session` 仍返回 JWT；Bearer 继续可用；`/api/users/sso` 兼容 |
| 新 Default Frontend，`unified_web_session = false` | 状态接口返回 `mode=legacy`，走原 JWT 流程 |
| 新 Default Frontend，`unified_web_session = true` | Cookie + CSRF；首次加载把旧 `auth-storage.token` 经 `/api/users/web-session/migrate` 换成服务端会话后再删除 |
| 第三方可插拔前端 / CLI | Bearer 不变（`API_CONTRACT_VERSION` 仍为 1，新增接口均为增量） |
| 超管后台 | 不变（独立 `adminToken`） |
| 未传 `prompt` 的 RP | 单账号浏览器：静默 SSO 与旧版一致；多账号浏览器：显示选择页（`prompt=none` → `account_selection_required`） |
| LearnOS 0.1.2（未升级） | OP 未开启 `backchannel_logout` 时完全不受影响；开启前必须先上线带 `disableProviderLogout` 的 LearnOS（见 I11） |

## 8. 备份与回滚（P0-6）

- 生产数据库与签名密钥位于 `10.66.66.7` / `120.95.81.151`，本工作站**无访问权限**：生产备份/恢复演练 = **BLOCKED（需运维执行）**，执行命令写在 ADR §部署。
- 本地在一次性 PostgreSQL 17 容器上验证：迁移前 `pg_dump` → 应用新迁移 → `pg_restore` 回到迁移前快照可用（证据见测试报告 §9）。
- 每个阶段一个开关：`[oidc].multi_account`、`[oidc].unified_web_session`、`[oidc].backchannel_logout`，默认 **关闭**；关闭即回到旧行为，不删数据。

## 9. A01–A20 执行矩阵

状态在 [oidc-test-report.md §9](oidc-test-report.md) 维护（PASS / FAIL / BLOCKED + 证据路径）。本表只记录每项的验证手段。

| ID | 验证手段 |
|---|---|
| A01 | 浏览器 E2E：主站密码登录 → LearnOS「使用 FloatCTF 登录」→ 选择页点账号 → LearnOS 已登录，无密码框 |
| A02 | 浏览器 E2E：LearnOS 经 OP 登录页输入密码 → 打开主站 → 自动恢复 |
| A03 | 浏览器 E2E：LearnOS 退出 → 再点 SSO 出现选择页 |
| A04 | 浏览器 E2E + 集成：主站普通退出后 OP 会话撤销，LearnOS 页面仍可用 |
| A05 | 浏览器 E2E：A04 之后再点 SSO 出现登录页 |
| A06 | 集成 + 前端单测：注入撤销失败（500/断网），UI 显示「未完成退出」，状态未改 |
| A07 | 集成 + E2E：同浏览器 A、B 两账号；选择页只列本浏览器有效账号；授权记录按用户独立 |
| A08 | 集成：并发 continue / 跨标签切换 |
| A09 | 集成 + E2E：移除单账号、退出本浏览器全部账号 |
| A10 | 集成（既有矩阵保留）+ 新增多账号组合 |
| A11 | 集成：`max_age=0` / `id_token_hint` 正反向 |
| A12 | LearnOS 单测/集成：伪造/过期/错 aud/重复 jti/含 nonce/错 typ；OP 端 end-session 参数校验 |
| A13 | 集成 + E2E：全局退出 A，只撤销 A 的 sid 绑定会话 |
| A14 | 集成：投递目标不可达 → outbox 记 pending/failed、UI 报「未完全同步」、重试 |
| A15 | 集成 + 浏览器：`/oauth/assets/float.png`、头像 200/404/非图片 → 首字母 |
| A16 | 浏览器截图：亮/暗 × 320/390/768/1440/1920 |
| A17 | 文档 + 配置测试：canonical issuer 方案、`first_party_origins` 校验；生产双 issuer 实测 **BLOCKED** |
| A18 | LearnOS 既有单测/集成 + E2E 学习流程回归 |
| A19 | 集成：用户 Cookie 访问 `/api/admin/*` 401；Bearer 兼容 |
| A20 | 集成：跨站 Origin、缺 CSRF 头、伪造 CSRF、SSRF 地址被拒 |
