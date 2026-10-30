# PLAN — FloatCTF 通用 OpenID Connect Provider

> **项目**：FloatCTF / FloatCTF Identity  
> **目标仓库**：https://github.com/FloatCTF/floatctf  
> **审阅基线**：`main` / `7c382e6b35a39148cbf6ecb581a6611ac61c673d`（2026-10-23 核对；开工前复查 HEAD）  
> **文档状态**：待实施的技术方案；本文不表示已有功能通过验收  
> **优先级**：P0 标准协议闭环 → P1 用户体验与开发者管理 → P2 通用互操作与公开发布  
> **工作性质**：在现有 Rust/Actix Web API 内增加 OIDC Provider；不是把 FloatCTF 改造成 OIDC Client，也不是只对接 LearnOS。

## 1. 背景与代码基线

最新检查确认：

- `apps/api/src/modules/identity/authentication/mod.rs` 已通过 `POST /api/users/session` 使用 Argon2 校验密码，签发 FloatCTF 自有 JWT；`GET /api/users/me` 读取当前玩家资料。
- `apps/api/src/core/security/jwt.rs` 使用 HS512 对称 JWT（当前声明 `sub/role/exp`，默认约 8 小时）。这是现有平台 API 的鉴权凭证，**不能**直接冒充符合 OIDC 的 ID Token。
- `apps/api/src/bootstrap/routes.rs` 负责全站路由注册；`apps/api/src/bootstrap/mod.rs` 负责静态配置与初始化；`apps/api/src/core/config.rs` 是 TOML 配置唯一入口。
- 当前默认前端 `frontends/default/src/stores/AuthStore.ts` 将 JWT 保存到 Zustand persist；这不等于服务器可识别的跨站 OIDC SSO 浏览器会话。
- `apps/api/src/sql/migrations/` 是增量 SQL 迁移目录；现有 `users`、`super_admin` 身份域分离。前端可能存在其他独立项目或 submodule，实施时应按实际启用的默认前端确认入口，不能只实现后台 API 而丢失授权确认界面。
- 仓库已有 `AGENTS.md` 与 `docs/agents/*` 指南：实际实施优先遵守项目现有规范，禁止借 OIDC 任务重写赛事、AWD/AWDP 或既有账号业务。

## 2. 业务目标、非目标、交付边界

### 2.1 目标（MUST）

1. 将 FloatCTF 作为第三方可接入的 **OpenID Provider（OP）** 和 OAuth 2.0 Authorization Server。
2. 通用 OIDC 客户端仅凭 `issuer`、`client_id`、必要的 `client_secret`、授权回调地址和标准 SDK 即可登录，不依赖 FloatCTF 私有接口。
3. 复用 FloatCTF 玩家账号作为身份源；已有 FloatCTF 网页登录态能够复用于第三方授权，实现 SSO（用户无需二次输入密码）。
4. 支持开发者在平台登记、查看、禁用和轮换 OIDC 应用凭据，用户有可读、可拒绝、可撤销的授权记录。
5. 用户身份标识稳定：第三方以 `(iss, sub)` 作为联合唯一身份，不依赖用户名、昵称、邮箱。
6. 保留原有 `POST /api/users/session`、FloatCTF JWT、API 守卫、用户数据与比赛功能，现有用户无需批量重新注册。
7. 使用标准流程和经过维护的 JOSE/加密依赖；不能自行发明签名、JWT、PKCE 算法。
8. 数据迁移可重复部署、不丢用户数据；提供明确配置、诊断工具、接入文档、测试证据和回滚办法。

### 2.2 第一版非目标（MUST NOT 为此扩大工作）

- 不部署 Keycloak、Hydra、Authentik 等独立身份服务器；除非审计发现现有工程条件根本不能满足标准，则在 ADR 中阐明再评估。
- 不要求更换现有 FloatCTF 玩家密码散列算法、完全替换自有 JWT 或重写所有前端登录流程。
- 不做 SCIM、SAML、FIDO2/WebAuthn、社交登录聚合、MFA、FAPI、Device Authorization、Implicit Flow、Resource Owner Password Credentials、Client Credentials Grant。
- 不向第三方暴露 FloatCTF 管理员令牌、战队关系、赛事成绩、比赛权限或平台服务端私钥。
- 不在 V1 自动支持 RFC 7591 动态客户端注册、Refresh Token、跨站统一退出、第三方业务 API 授权；要作为独立里程碑列明。
- 不承诺“退出 FloatCTF 就即时退出已登录第三方网站”：这需要独立的会话撤销与登出通知协议设计。

### 2.3 名词与身份边界

- **OP/Issuer**：FloatCTF，管理认证会话、OIDC 应用、授权、签发 ID Token、提供 UserInfo。
- **RP/Client**：LearnOS 或任何第三方标准 OIDC 应用，维护自身会话与权限。
- **SSO Session**：FloatCTF 服务端管理、浏览器通过 Cookie 持有的登录会话，区别于 FloatCTF 原有 API Bearer JWT。
- **ID Token**：面向特定 `aud=client_id` 的已签名身份断言；不是 FloatCTF API 通行证。
- **Access Token**：V1 使用高熵不透明令牌，仅用于 OIDC UserInfo；不是 ID Token，不向 RP 提供 FloatCTF 原业务 API 权限。
- **User**：仅普通玩家 `users` 可以成为 OP 最终用户；`super_admin` 权限域独立。

## 3. 遵循的标准与互操作范围

以标准原文为契约，不以示例 JSON 当标准：

| 规范 | V1 用途 | 参考 |
| --- | --- | --- |
| OpenID Connect Core 1.0（Errata Set 2） | Authorization Code Flow、ID Token、UserInfo、claims、错误响应 | https://openid.net/specs/openid-connect-core-1_0.html |
| OpenID Connect Discovery 1.0（Errata Set 2） | OP Metadata / Discovery | https://openid.net/specs/openid-connect-discovery-1_0.html |
| OAuth 2.0 Authorization Framework (RFC 6749) | 授权端点、令牌端点、客户端认证、错误码 | https://www.rfc-editor.org/rfc/rfc6749 |
| PKCE (RFC 7636) | `S256` code_challenge/code_verifier | https://www.rfc-editor.org/rfc/rfc7636 |
| OAuth 2.0 Security BCP (RFC 9700) | 精确 redirect URI、PKCE、mix-up、令牌保护 | https://www.rfc-editor.org/rfc/rfc9700 |
| Bearer Token Usage (RFC 6750) | UserInfo Bearer 认证与错误 | https://www.rfc-editor.org/rfc/rfc6750 |
| JWT (RFC 7519), JWS (RFC 7515), JWK (RFC 7517) | ID Token / 公钥目录 | https://www.rfc-editor.org/rfc/rfc7519 |
| OAuth 2.0 Token Revocation (RFC 7009) | 可选 V1.1 撤销端点 | https://www.rfc-editor.org/rfc/rfc7009 |
| OAuth 2.0 Authorization Server Metadata (RFC 8414) | 进一步通用 OAuth 客户端发现 | https://www.rfc-editor.org/rfc/rfc8414 |

**V1 宣告支持**：`response_type=code`、`grant_type=authorization_code`、`scope=openid [profile] [email]`、`PKCE=S256`、`RS256 ID Token`、`subject_types_supported=[public]`、`client_secret_basic`（机密客户端）、`client_secret_post`（互操作备用）、`none`（公开客户端）。未通过实现和测试的能力不得出现在 Discovery Metadata 中。需要根据实际能力确认 `prompt_values_supported`、`claims_supported` 等可选字段。

## 4. 总体架构

```text
Browser / Third-party RP (LearnOS, apps, SDKs)
               |
               v
        HTTPS Reverse Proxy
               |
          FloatCTF API
   +----------------------------+
   | /oauth/authorize           |<--- First-party browser SSO session
   | /oauth/token               |<--- Confidential/Public client + PKCE
   | /oauth/userinfo            |<--- Opaque Access Token
   | /.well-known/...           |<--- Discovery / JWKS
   | /api/oidc/apps             |<--- Developer client management
   | /api/oidc/consents         |<--- User grant management
   +----------------------------+
     |             |          |
 PostgreSQL   Signing keys  Existing users
     |
 No change to existing FloatCTF competition data
```

### 4.1 路由约定

采用根 Issuer（正式地址示例：`https://ctf.example.com`）。代理必须将以下路径路由到 FloatCTF API，而不是返回前端 SPA 的 index.html。Issuer 从可信静态配置读取，**不能**由未校验的 `Host`/`X-Forwarded-*` 拼接。

| 方法/路径 | 角色 | 优先级 |
| --- | --- | --- |
| `GET /.well-known/openid-configuration` | OIDC Discovery JSON | P0 |
| `GET /.well-known/jwks.json` | 发布有效 RS256 公钥（仅公钥） | P0 |
| `GET /oauth/authorize` | 校验 client、redirect、scope、state、nonce、PKCE；引导登录/授权 | P0 |
| `POST /oauth/authorize/consent` | 同意/拒绝当前已验证授权事务 | P1（P0 流程必须先有等效同意页） |
| `POST /oauth/token` | `application/x-www-form-urlencoded`，授权码原子兑换令牌 | P0 |
| `GET /oauth/userinfo` | Bearer token 获得已授权 claims，`sub` 必含 | P0 |
| `POST /oauth/revoke` | 撤销不透明令牌；实现后才加入 Discovery | P1 |
| `GET /oauth/login` / `POST /oauth/login` | 第一方登录网页流程，复用现有账号校验 | P0 |
| `POST /oauth/logout` | 清除 FloatCTF 本地 SSO 会话，不宣称实现 OIDC 全局登出 | P1 |
| `/api/oidc/clients/*` | 开发者应用管理/管理员审核 | P1 |
| `/api/oidc/authorizations/*` | 用户查看/撤回授权 | P1 |

URL 不是死规定；但必须在 `apps/api/src/bootstrap/routes.rs` 集中注册，在 Discovery 中公布实际、可访问、与 Issuer 匹配的 URL。反向代理中统一支持 HTTPS、转发与无缓存策略。注意 `/.well-known` 不能无意识落到 UI 静态前端。

### 4.2 推荐源代码边界

```text
apps/api/src/modules/identity/oidc/
  mod.rs
  api/{discovery,authorize,token,userinfo,consent,clients,grants}.rs
  domain/{claims,client,grant,scope,authorization}.rs
  service/{authorization_service,token_service,session_service,client_service}.rs
  infrastructure/{repository,signing_keys}.rs
apps/api/src/sql/migrations/<new_timestamp>-oidc-provider.sql
apps/api/tests/oidc_{discovery,flow,clients,security,interop}.rs
frontends/default/...       # 在真实启用的前端增加授权/应用/授权管理 UI

docs/identity/oidc-provider.md
docs/identity/oidc-client-integration.md
docs/identity/oidc-operations.md
```

文件名仅为组织建议。按项目现有模块目录和路由风格落地，不制造无意义抽象层。

## 5. 协议契约

### 5.1 Discovery（P0）

至少准确包含：`issuer`、`authorization_endpoint`、`token_endpoint`、`userinfo_endpoint`、`jwks_uri`、`response_types_supported`、`subject_types_supported`、`id_token_signing_alg_values_supported`、`scopes_supported`、`grant_types_supported`、`token_endpoint_auth_methods_supported`、`code_challenge_methods_supported`。HTTP 头 `Content-Type: application/json`，可按合理 TTL 缓存。`issuer` 必须与所签发 ID Token 的 `iss` **字节级一致**。

示例（仅声明已实现者）：

```json
{
  "issuer": "https://ctf.example.com",
  "authorization_endpoint": "https://ctf.example.com/oauth/authorize",
  "token_endpoint": "https://ctf.example.com/oauth/token",
  "userinfo_endpoint": "https://ctf.example.com/oauth/userinfo",
  "jwks_uri": "https://ctf.example.com/.well-known/jwks.json",
  "response_types_supported": ["code"],
  "grant_types_supported": ["authorization_code"],
  "subject_types_supported": ["public"],
  "id_token_signing_alg_values_supported": ["RS256"],
  "scopes_supported": ["openid", "profile", "email"],
  "token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post", "none"],
  "code_challenge_methods_supported": ["S256"]
}
```

### 5.2 Authorization Endpoint（P0）

1. 验证 `client_id`、client 启用/审批状态、`response_type=code`、`scope` 包含 `openid`、预注册 `redirect_uri`、`code_challenge`、`code_challenge_method=S256`、可选 `state` 和 `nonce`，以及其他主动声明支持的参数。请求参数不合法时必须返回标准 OAuth 错误。
2. `redirect_uri` 采用精确字符串匹配；无通配符、无任意域名。V1 仅允许 HTTPS Web 回调；本地开发或 Native loopback 可作为单独受控配置，不能把 `http://任意域名` 当成生产回调。
3. **先校验回调再重定向**。如果 client/redirect 无效，显示 FloatCTF 自己的错误页，绝不跳回未知 URL。
4. OIDC 交易内容写入短期、服务端可验证的授权事务，绑定浏览器会话、client、redirect、scope、state、nonce、PKCE challenge；不能以未签名可篡改的 query/hidden input 作为信任源。
5. 未认证：引导第一方登录；已认证：检查用户是否授权过。第一次或增加 Scope 显示清晰的客户端身份、权限和拒绝按钮。`prompt=none` 如受支持，应在需登录或需同意时返回 `login_required` / `consent_required`，不能静默弹页面。
6. 同意后仅向已经验证的 callback 以 `?code=...&state=...` 返回短时、单次授权码；拒绝通过同一合法 callback 返回 `access_denied`。
7. 仅在安全登录会话下授予权限；禁止通过 `prompt`、`login_hint` 或自带 `sub` 任意代替身份。

### 5.3 Token Endpoint（P0）

- `POST`、`application/x-www-form-urlencoded`；不接受 GET 兑换、不把 token 放在 URL。
- `grant_type=authorization_code`；使用 `client_secret_basic` 或按配置允许 `client_secret_post`；公开客户端 `none` 必须强制 PKCE，不存 client secret；机密客户端同样要求 PKCE。
- 验证一次性随机 Code 的哈希、client_id、redirect_uri、code_verifier 的 S256、TTL、撤销状态和用户/应用状态。数据库**原子消费** Code，并防止并发重复换取 token；不允许 PKCE downgrade。
- 成功响应含 `access_token`、`token_type=Bearer`、`expires_in`、`id_token`、`scope`（按实际返回）；必要时包含标准错误 `invalid_grant/invalid_client/unsupported_grant_type`。
- 返回 `Cache-Control: no-store` 和 `Pragma: no-cache`；敏感值不得记录日志。

### 5.4 ID Token（P0）

- 使用独立的非对称密钥（推荐 RS256），JWS Header 有有效 `kid`，JWKS 同步发布公钥；**不得复用 FloatCTF API 的 HS512 `jwt_secret`**。
- 正确包含 `iss`、`sub`、`aud`、`exp`、`iat`；如果 auth request 携带 `nonce`，必须原样反映并绑定原事务；按 OIDC Core 在 `max_age` 或相应场景下提供 `auth_time`，对未支持的请求明确返回标准错误，不能谎称支持。
- `sub` 为不可再分配给其他人的稳定字符串标识，建议当前 `users.id` 的 UUID 字符串；`super_admin` 不能作为普通用户默认登录。
- `aud` 指向具体 `client_id`；发行、受众、时间戳均按 OIDC Core 测试。禁止把旧 FloatCTF `role` 当作标准客户端权限自动附加。
- 明确 ID Token 只供 RP 验证用户身份，**不能**作为 `/api/*` 的 FloatCTF Bearer JWT。

### 5.5 Access Token / UserInfo（P0）

- V1 推荐高熵不透明 Access Token，数据库仅存 `SHA-256(token)` 哈希和 token/client/user/scope/TTL/撤销信息；不要把 Token JSON 明文存库。令牌用途限制为 UserInfo。
- `GET /oauth/userinfo` 使用 `Authorization: Bearer <token>`，校验 Token 状态、到期、client 是否禁用、用户是否启用、授权是否撤回；返回 JSON，`sub` 必须与 ID Token 的 `sub` 相同。
- `openid`：`sub`；`profile`：最小限度昵称/用户名/头像，采用标准 claim 名称如 `preferred_username`、`nickname`、`picture`；`email`：`email`、有依据才设 `email_verified=true`。当前账号注册不等于已校验邮箱，不能伪造邮箱验证状态。
- 未授予的 Scope 不应泄露对应 claim；不能返回密码散列、手机号、管理员角色、赛事内部信息或全部数据库列。

### 5.6 SSO Session（P0）

- 保留既有 JWT API；新增第一方、服务端管理的浏览器 SSO Session：Cookie `HttpOnly`、`Secure`（生产）、`SameSite=Lax`、正确 Path 与期限；服务端存随机 Session ID 的摘要并支持撤销/过期。
- FloatCTF 当前 SPA 主要持有本地 JWT，因此必须打通登录行为：在兼容原有登录接口与前端情况下，让网页登录成功同时建立 SSO Cookie；或在授权流程中提供第一方网页登录页，成功建立 SSO Cookie。二者需要 E2E 验证“先在 FloatCTF 登录 → 打开第三方登录 → 无密码授权”，不能只靠 API Token 证明 SSO。
- 同意/拒绝等状态变更端点有 CSRF 防护；`prompt=login`/`prompt=none` 等处理符合宣告的能力；SSO 会话撤销不依赖 JS localStorage 清理。
- 检查跨域 Cookie、代理 TLS 终止、多个域名/子域名的 SameSite 限制；不要为了“全域共享”把 Cookie 暴露给全部第三方站点。

## 6. 客户端注册 / 开发者中心（P1）

### 6.1 应用模型

每个 Client 至少包含：`client_id`（不可预测、全局唯一）、`owner_user_id`、显示名称、主页、图标、应用类型（confidential/public）、允许的 redirect URI 列表、允许 scopes、token endpoint auth method、状态（pending/active/disabled）、创建与修改时间。机密客户端的 Secret 仅创建/轮换时显示一次，数据库存 Argon2 等单向校验值，不可查询明文；支持失效旧 Secret 与有限重叠轮换窗口。

客户端登记由已登录 FloatCTF 玩家发起，V1 推荐管理员审核激活；如果允许免审，仅开放受限开发环境并设置风险控制。开发者只能查看和修改**自己**的应用；管理员有独立审核/禁用权限。避免直接复用 `super_admin` JWT 为普通用户 OIDC 身份。

### 6.2 开发者界面

- 「我的应用」：应用名、状态、Client ID、授权 Scope、回调 URL。
- 「创建应用」：机密/公开客户端类型、名称、网站、callback、Scopes、接入须知。
- 「应用详情」：查看 Discovery、复制配置、添加/删除回调、轮换 Secret、暂停与删除（应考虑仍有授权）。
- 「管理员审核」：批准/拒绝/禁用应用与审计日志。
- 「我的授权」：用户查看第三方名称、已授权 Scope、最近使用和撤销操作。
- 「授权确认」：明确列出将分享的信息，允许取消，不默认分享 email。

### 6.3 客户端身份与权限边界

- `client_id` 不是秘密，不承担客户端认证；`client_secret` 不得下发前端公开源码。公开应用不配置 Secret。
- 不允许客户端自己设置高权限 Scope；V1 只有 `openid/profile/email`。
- 所有管理接口必须验证调用者、应用归属和状态；UI 不构成权限保护，后端必须执法。
- OAuth Client 的授权不能自动提升成 FloatCTF 或 LearnOS 管理员权限。

## 7. 数据模型与迁移计划

建议新增独立、前缀统一的表，不修改 `users.id` 和原 JWT 业务表：

| 表名建议 | 关键字段 / 索引 | 作用 |
| --- | --- | --- |
| `oidc_clients` | `client_id` UNIQUE、`owner_user_id` FK、`type`、`redirect_uris`、`scopes`、`auth_method`、`secret_hash`、`status` | 应用注册与状态 |
| `oidc_browser_sessions` | `session_hash` UNIQUE、`user_id` FK、`auth_time`、`expires_at`、`revoked_at` | 第一方 SSO |
| `oidc_auth_transactions` | `transaction_id`、`session/user_id`、client、redirect、PKCE、nonce、scope、TTL | 登录/授权页跨重定向事务 |
| `oidc_authorization_codes` | `code_hash` UNIQUE、`client_id`、`user_id`、redirect、PKCE、nonce、`expires_at`、`consumed_at` | 授权码一次性兑换 |
| `oidc_consents` | `(user_id, client_id)` UNIQUE、`granted_scopes`、`revoked_at`、时间戳 | 用户授权审计 |
| `oidc_access_tokens` | `token_hash` UNIQUE、client/user、scopes、`expires_at`、`revoked_at` | 不透明 token 验证/撤销 |
| `oidc_signing_keys`（可选元数据） | `kid` UNIQUE、`alg`、公钥、密钥引用、激活/退役时间 | 轮换与 JWKS |

签名私钥优先存受限访问的配置密钥库/持久化只读密钥文件或规范的秘密管理系统，**不要**以普通明文数据库列存私钥；数据库最多记录元数据/秘密引用。部署多副本时使用一致的持久化密钥版本，不能每次启动随机生成新 Key 破坏第三方验签。

迁移要求：新建增量 SQL，不修改旧已应用迁移；创建唯一约束、FK、TTL 索引和最小权限；敏感字段不打印；清理过期 codes/transactions/tokens/session 用现有调度体系或轻量定期任务，避免无限增长。迁移回退不能自动删除已有正常用户与赛事数据。

## 8. 安全不变量（上线硬门禁）

1. 授权码短期（建议 ≤120 秒）、随机高熵、一次性、原子消费；即使两请求并发也只准成功一次。
2. PKCE `S256` 强制执行；阻断降级、错误 verifier、错误 client、错误 redirect、错误 grant。
3. 生产 redirect URI 精确匹配 HTTPS；未知 client 或 callback 不重定向；绝不允许开放重定向。
4. 签名 ID Token 只能用专用非对称私钥，JWKS 只有公钥；禁用 `alg=none` 及算法混淆。
5. SSO Cookie 服务端验证、HttpOnly/Secure/SameSite，状态变更有 CSRF；注意与 SPA localStorage JWT 的过渡。
6. 所有授权流程确认 client 状态、用户状态、Scope、审批结果；第三方不能越权或代替另一个用户。
7. Token/code/client_secret/password 不打印到请求日志、错误日志、审计事件；异常响应不带敏感内容。
8. 用户撤销授权、应用禁用后，既有 Access Token 对 UserInfo 的访问立即失效；**已交付的短期 ID Token 不能追溯删除，RP 自己的会话也不会自动退出**，UI 必须如实解释。
9. 应用 API 按 owner 隔离；用户授权页必须展示真实注册应用，阻断假冒系统/官方身份。
10. 对登录、授权、Token、开发者创建/轮换设置请求大小、超时、速率限制与错误保护；代理真实 IP 从可信代理获取。
11. HTTPS 终止与 `issuer` 配置严格一致；代理路径、缓存头和 JWKS 轮换完整验证。
12. `email_verified` 不得凭空声明；ID Token `sub`、UserInfo `sub`、账号状态在整个链路一致。

## 9. 分阶段实施与验收

### Phase 0 — 审计与 ADR（门禁 G0）

任务：阅读 `AGENTS.md`、`docs/agents/*`、auth/routes/config/migrations/前端登录代码；列出真实路由链和反代路径；检查可复用 Rust JOSE/OAuth 包，避免手写协议基础设施；写 `docs/identity/ADR-oidc-provider.md`，确定 issuer、客户端类型、签名 Key 管理、Access Token 表示、SSO 方案、前端入口、是否允许开发者公开登记。

**G0**：给出具体代码链路、数据流与风险清单；现有 JWT/登陆回归基线通过；未确认关键设计不得编造“标准已支持”。

### Phase 1 — 配置、数据库、签名与 Discovery（门禁 G1）

任务：增量迁移、配置开关（默认禁用待配置）、Issuer 校验、持久化 Key ring、RS256 签名/验证、公钥 JWKS、Discovery、client 的后端注册模型、短期事务与令牌随机生成工具。

**G1**：Discovery 与实际路由完全一致；已签 ID Token 可使用 JWKS 验证；JWKS 永不含私钥；issuer 变化/无签名 Key 应明确失败，不默默生成不兼容 Key；原登录/赛事路由不受影响。

### Phase 2 — Authorization Code + PKCE + UserInfo（门禁 G2）

任务：`/oauth/authorize` 及错误协议、Authorization Code 存储、Token client auth、PKCE、RS256 ID Token、Opaque Access Token、UserInfo、原子消费与有效期限制。先通过测试 client 端到端跑通。

**G2**：规范成功闭环 + 反例矩阵全部通过；跨 Client/redirect、并发重放、篡改/到期/伪造令牌全部拒绝；标准客户端可直接配置 Issuer 完成登录。

### Phase 3 — 浏览器 SSO、授权 UI、开发者中心（门禁 G3）

任务：打通原 FloatCTF 登录与 SSO Session；授权确认/拒绝；开发者客户端注册与审核；用户查看/撤销授权；机密客户端 Secret 创建显示一次/轮换；前端亮暗主题与移动端适配；审计日志。

**G3**：用户先在 FloatCTF 登录后，第三方点击登录不重复输密码；首次授权有提示、二次授权正常跳转、拒绝与撤销生效；所有开发者管理接口权限隔离。

### Phase 4 — 标准互操作与生产发布（门禁 G4）

任务：本地及 TLS staging 跑至少三种标准 OIDC RP（推荐 LearnOS/Supabase Auth、普通 Web 服务端客户端、公开 PKCE 客户端）；执行 OpenID Foundation Conformance Suite 相关 OP 测试；负向安全测试、稳定性测试、密钥轮换演练；补齐完整接入与运维文档。

**G4**：第三方仅靠 Issuer + Client ID/Secret + callback 正常配置；测试过程、通过/失败、被跳过项目真实记录；生产 Proxy/HTTPS/issuer/JWKS 可用；不声称尚未获得的 OpenID 官方认证。

### Phase 5 — 可选增强（另行授权，不阻塞 V1）

Refresh Token + Rotation、RFC 7009 revoke endpoint、RFC 7662 introspection、RP-Initiated Logout/Back-Channel Logout、RFC 7591 动态注册、pairwise subject、OAuth 2.0 资源访问 scopes、PAR/JAR/FAPI、第三方行为监控。实施前出 ADR 与单独验收定义。

## 10. 测试矩阵

### 10.1 单元测试

- Claims 必选字段、`aud`/`iss`/`nonce`/`iat`/`exp`、无效 alg、错误 kid。
- JWKS 公钥与私钥分离、旧/新 kid、轮换窗口与签名验证。
- PKCE S256 长度、字符集、base64url 无填充、边界与错误 verifier。
- Redirect URL 校验：同前缀/子域/端口/fragment/大小写/百分号编码/未知域/loopback 配置。
- Scope 并集与授权减集、email_verified、sub 稳定性、scope 限制。
- Client Secret 哈希/认证方法与公开客户端无 Secret；无效 client 状态。

### 10.2 Rust API + PostgreSQL 集成测试

- 标准 code flow 机密客户端及公开 PKCE 客户端。
- 授权码只消费一次（含两请求并发）；TTL 到期；跨客户端或跨 redirect 交换；PKCE 错误或缺失；secret 错误。
- 同一账号不同 app 的 `aud` 不同、`sub` 一致（public subject）；两个账户 `sub` 不相同。
- 有/无 profile/email scope 时 UserInfo 数据最小披露；邮箱未验证不冒充已验证。
- 授权拒绝、Scope 更新、禁用应用、撤销授权、失效令牌、用户注销。
- 旧 `/api/users/session`、JWT API 鉴权、`super_admin` 与 `users` 身份隔离、赛事核心 API 回归。
- 多实例与 Key ring 一致性/密钥切换、公钥缓存过渡。

### 10.3 浏览器 E2E / 互操作

- 未登录第三方授权 → FloatCTF 登录 → 授权 → 回跳第三方 → 获取已验证资料。
- 已在 FloatCTF 登录 → LearnOS 发起授权 → 无需再次输入密码。
- 首次授权与重复授权、明确拒绝、`prompt=none` 情况（如声明支持）、多标签/过期登录事务。
- 开发者创建 app/复制 client ID/一次性 Secret/轮换/禁用；其他用户不能更改。
- 个人中心撤销授权后 UserInfo token 失效；不能声称撤销已登录第三方的本地会话。
- HTTPS 反代域名、移动端 Cookie、亮暗主题、代理路径一致性；浏览器网络中无明文 Secret 或私钥。
- OpenID Foundation Conformance Suite 对应 OP profile。报告具体 suite 版本、配置、通过率与剩余失败，禁止将“运行过工具”当成“已认证”。

## 11. 非功能性要求

- 可观测性：只记录 `client_id`、请求关联 ID、授权状态和错误类别等非秘密元数据；可以按 client 统计成功率和错误率，不记录 token、code、PKCE verifier、密码。
- 容错：DB 失败时令牌和授权 **fail closed**；签名密钥不可用时拒绝签发并明确告警；不能采用内存默认值悄悄降级。
- 运维：新配置从 TOML/秘密存储引入，配置检查和健康检查明确；提供受控数据库迁移、密钥管理与轮换、日志查询、关闭开关和故障回退。
- 密钥与 Session：持久化、多副本兼容，支持撤销和合理保留周期。
- 兼容：既有赛事和用户账号无破坏性更新；普通用户和管理员认证边界严格分开。
- 资源控制：短期事务的定期清理、索引及限流；对 token/code 输入设置长度边界。

## 12. 完成定义（Definition of Done）

必须**全部**有真实证据：

- [ ] 不需要 FloatCTF 私有 SDK，通用 OIDC Client 从 Issuer Discovery 自动接入。
- [ ] 第三方能登记自己的 Client、回调地址及 scope，并获得对应凭据。
- [ ] 标准 Authorization Code + PKCE S256 流程，包含授权确认与拒绝。
- [ ] 合法 ID Token 的 `iss/aud/sub/exp/iat/nonce/kid` 正确，JWKS 可验证。
- [ ] UserInfo 按授权范围返回，不返回敏感业务数据。
- [ ] 浏览器真正 SSO：已经登录 FloatCTF 后第三方无需重新输入密码。
- [ ] OAuth 交易对 code 重放、跨 client/redirect、错误 PKCE 和过期状态 fail closed。
- [ ] 用户可以撤销第三方访问；开发者只能管理自己的应用；管理员可审核/禁用。
- [ ] 旧登录/JWT/用户/赛事核心功能无回归。
- [ ] Rust unit/integration/HTTP/浏览器 E2E + 至少两种外部客户端互操作；有客观测试日志。
- [ ] OIDF Conformance Suite 适用项目运行并记录结果，未通过项清楚标记。
- [ ] 完整接入文档、API 错误与示例、部署和 TLS 配置、Key rotation runbook、故障处理与回滚指南。

## 13. 发布策略与回滚

1. 先在受控开发环境启用 OIDC，保证 `enabled=false` 时旧 FloatCTF 完全正常。
2. 在真实 HTTPS staging 绑定固定 Issuer，部署标准测试客户端；不使用 IP 或不受信任 `Host` 推导 Issuer。
3. 开启仅管理员/受邀开发者可登记 App；完善审核、审计、限流；通过门禁后再开放公众登记。
4. 回滚首选关闭新的 OIDC 注册/授权入口，保留新表和用户资料，不回滚已存在的数据结构；对现有已发 Token 定义明确到期/撤销策略。
5. 发生签名私钥泄露执行紧急轮换并评估已发令牌；常规轮换应保留旧公钥直至旧 Token 过期及缓存宽限结束。

## 14. Agent 汇报与证据格式

每个 Phase 至少提交：

```text
Phase / Git SHA / 完成项 / 具体文件 / 数据库迁移
协议端点与 Metadata 差异
测试命令 / 测试数量 / 结果 / 关键日志或报告路径
未完成 / 已知风险 / 下阶段依赖
是否修改或影响现有 API 与账号会话
```

任何无法运行的测试必须说明为什么；不得以 mock、README 勾选或静态 lint 代替实测 PostgreSQL 和浏览器登录端到端验证。
