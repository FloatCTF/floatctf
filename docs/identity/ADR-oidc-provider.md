# ADR — FloatCTF 作为通用 OpenID Connect Provider

- 状态：已采纳（Phase 0，2026-10-23）
- 需求契约：[docs/plans/floatctf-oidc-provider.md](../plans/floatctf-oidc-provider.md)
- 基线：`main` @ `7c382e6b35a39148cbf6ecb581a6611ac61c673d`，实施分支 `oidc`
- 规范：OpenID Connect Core 1.0 / Discovery 1.0（Errata Set 2）、RFC 6749、RFC 6750、RFC 7636、RFC 7519/7515/7517、RFC 8414、RFC 9207、RFC 9700

## 1. 审计结论（现状代码链路）

### 1.1 现有认证链路

```
POST /api/users/session                         modules/identity/authentication/mod.rs::user_login
  → users WHERE username = ?                    entity/users.rs（id UUID / username / nickname / email / avatar）
  → Argon2::verify_password(users.password)
  → core/security/jwt.rs::gen_jwt_token(id, Role::User, None)   HS512，claims = {sub, role, exp(+8h)}
  → UniResponse<String>（JWT 明文在 JSON body）
前端 frontends/default/src/routes/index.tsx
  → AuthStore.setToken(jwt)                     Zustand persist → localStorage["auth-storage"]
  → @floatctf/sdk transport 注入 Authorization: Bearer <jwt>
API 守卫 api/extractor/auth.rs
  → UserJwtGuard：validate_jwt + role==User + users 存在
  → SuperAdminJwtGuard：validate_jwt + role==SuperAdmin + super_admin 存在
```

- 登出：`components/Header.tsx` 只调用 `authStore.removeToken()`，**服务端无会话、无撤销**。
- 管理员：`POST /api/admin/session` → `super_admin` 表，独立 `Role::SuperAdmin` JWT。
- 结论：浏览器里 FloatCTF 的"已登录"只是 localStorage 中的 JWT。顶层导航到 `/oauth/authorize` 时
  浏览器**不会**携带任何服务器可识别的凭据——这正是 PROMPT 要求消除的设计障碍（§4 解决）。

### 1.2 路由与反向代理

- 全站路由唯一聚合点：`apps/api/src/bootstrap/routes.rs::configure_all_routes`。
  当前只有 `/api/*` 与 AWD/AWDP internal 路由，**无** `/.well-known`、`/oauth`。
- 开发入口：`infra/caddy/Caddyfile.dev`（:7780）只把 `/api/*` 反代到 API（:9090），其余进 Vite（:13000）。
- 生产入口：`scripts/install.sh` 内嵌 Caddyfile 模板（镜像 `infra/caddy/Caddyfile.prod`），同样只反代 `/api/*`，
  其余进前端制品 → **`/.well-known/*` 与 `/oauth/*` 默认会落到 SPA index.html**，必须新增反代规则。
- CORS：`bootstrap/mod.rs::build_cors` 用 `[cors].allowed_origins` 白名单 + `supports_credentials`；
  第三方浏览器端 RP 直接调用 token/userinfo/discovery/jwks 会被拒，需要对这几个路径单独放开（§7）。

### 1.3 配置、数据库、调度

- 配置：`core/config.rs` 的 `TomlConfig` → `AppConfig`，唯一来源 TOML，敏感值用 `Secret`。
- 迁移：`apps/api/src/sql/migrations/` forward-only，新增迁移 + `db:gen` 生成实体（sea-orm-cli 1.1.20）。
- 后台任务：`scheduler/`（`TaskKey` + handler + `scheduled_tasks` 种子），用于过期数据清理。
- 限流：`infrastructure/ratelimit.rs`（Redis ZSET + Lua，fail-closed）只服务 AWD 的固定 scope。
- 请求日志：`LogService::add_log` 写 `logs` 表；`get_client_ip` 直接信任 `X-Forwarded-For` 首段
  （直连 API 时可伪造）——OIDC 限流不能只按 IP 计数。

### 1.4 前端

- 实际启用的官方前端：`frontends/default`（React + TanStack Router + Primer）。
  `frontends/floatctf-frontend-{shadcn,xzmu}` 是 git submodule，本仓库工作树中**未检出（空目录）**，
  不在本次修改范围内；它们通过公共 SDK 获得新能力（§8）。
- 登录页 `routes/index.tsx`；选手区守卫 `routes/service/route.tsx`（只看 localStorage token）。

### 1.5 回归基线（G0）

在隔离 PostgreSQL（独立容器 + 一次性数据库，非开发库）上执行 `scripts/test-rust.sh`：

```
passed=795 failed=6 ignored=9
```

6 个失败全部来自 `tests/awdp_training.rs`，原因是本机缺少本地镜像 `floatctf/test-g:gamebox-v1.0.3`
（`inspect local image ...: image not found`），属于既有环境依赖，与 OIDC 无关。后续阶段以此为回归对照。

## 2. 决策总览

| 议题 | 决策 |
|------|------|
| 实现方式 | 在现有 API 内新增 `modules/identity/oidc/`，不引入 Keycloak/Hydra 等独立 IdP |
| Issuer | `[oidc].issuer` 静态配置，**只允许 origin**（`scheme://host[:port]`，无 path/query/fragment/尾斜杠）；生产必须 `https`；`http` 只允许 loopback（开发）。不从 `Host`/`X-Forwarded-*` 推导 |
| 签名 | RS256；私钥 PEM 文件由配置的 key ring 引用（不进数据库、不进 TOML 正文）；`jsonwebtoken`（底层 `ring`）签名；`ring` 解析公钥导出 JWK |
| ID Token | 独立 RS256 key，`kid` 必带；与 API 的 HS512 `jwt_secret` 完全隔离 |
| Access Token | 256-bit 随机不透明令牌，库内只存 SHA-256；用途仅 UserInfo |
| 授权码 | 256-bit 随机，只存 SHA-256，TTL 默认 60s（上限 120s），条件 UPDATE 原子消费 |
| 客户端认证 | `client_secret_basic`、`client_secret_post`、`none`（公开客户端）；每个 client 注册一种方法，不匹配即 `invalid_client` |
| PKCE | 仅 `S256`；公开客户端强制；机密客户端默认强制，**仅管理员**可对单个机密客户端关闭，关闭后授权请求必须带 `nonce`（RFC 9700 §2.1.1） |
| SSO 会话 | 服务端 `oidc_browser_sessions` + HttpOnly Cookie；由现有登录接口、OIDC 第一方登录页、以及 SPA JWT 桥接三种方式建立 |
| 交互页面 | 登录/同意/错误页由 API 服务端渲染（与任何可插拔前端无关），自带亮暗色与移动端样式 |
| 开发者中心 | `/api/oidc/*`（玩家 JWT）+ `/api/admin/oidc/*`（超管 JWT）+ Default Frontend 页面 + SDK 方法 |
| 客户端审批 | 默认需要管理员审批（`require_client_approval = true`）；`enabled = false` 默认关闭 |
| Refresh Token / 动态注册 / 全局登出 | V1 不实现，不在 Discovery 宣告 |

## 3. Issuer 与路由

- 采用根 Issuer：`https://<平台域名>`，与现有平台同域；所有协议端点挂在根路径：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/.well-known/openid-configuration` | GET | OIDC Discovery |
| `/.well-known/oauth-authorization-server` | GET | RFC 8414（同一份元数据的 OAuth 视图） |
| `/.well-known/jwks.json` | GET | 仅公钥 |
| `/oauth/authorize` | GET, POST | 授权端点（OIDC Core 3.1.2.1 要求 GET 与 POST） |
| `/oauth/login` | GET, POST | 第一方登录页（复用 Argon2 校验） |
| `/oauth/consent` | GET, POST | 授权确认页（同意/拒绝） |
| `/oauth/token` | POST | `application/x-www-form-urlencoded` |
| `/oauth/userinfo` | GET, POST | Bearer |
| `/oauth/logout` | POST | 只清除 FloatCTF 本地 SSO 会话 |

- Issuer 校验在启动时完成：`enabled = true` 而 issuer 非法、无可用签名 key、key 文件不可读/非 RSA/小于 2048 位、
  `kid` 重复、active key 不是恰好一个 → **启动失败**（fail closed），绝不生成临时 key。
- `enabled = false`：全部协议端点返回 404，登录接口不设置 SSO Cookie，原有行为逐字不变。
- Caddy（dev 与 prod 模板）新增 `handle /.well-known/openid-configuration`、`/.well-known/oauth-authorization-server`、
  `/.well-known/jwks.json`、`/oauth/*` → API，必须排在前端兜底 `handle` 之前。

## 4. 浏览器 SSO 方案（消除"SPA 已登录但授权服务器不认"）

`oidc_browser_sessions(session_hash, user_id, auth_time, expires_at, revoked_at, ...)` + Cookie：

- Cookie：`__Host-fctf_sso`（issuer 为 https 时；要求 `Secure`、`Path=/`、无 `Domain`），开发 http loopback 下降级为
  `fctf_sso`；`HttpOnly`、`SameSite=Lax`、`Max-Age = session_ttl`。值为 256-bit 随机数，库内只存 SHA-256。
- 建立途径（三条，都只针对 `users`，超管不参与）：
  1. **现有 `POST /api/users/session`**：登录成功时在保持 JSON 响应体不变的前提下额外 `Set-Cookie`。
     SPA 通过同源 `/api` 调用（dev Caddy :7780 / prod Caddy），Cookie 落在与 `/oauth/*` 相同的 origin。
  2. **OIDC 第一方登录页 `/oauth/login`**：未登录时引导至此，成功后建立同一种会话。
  3. **JWT 桥接 `POST /api/users/sso`**（需 `Authorization: Bearer <user JWT>`）：给本功能上线前已持有 JWT 的
     SPA 会话补发 Cookie；已有同用户有效会话时幂等返回。Bearer 认证不受 CSRF 影响。
     `auth_time` 取 JWT 新增的 `iat`（旧 JWT 无 `iat` 时取 `exp - 8h`，即签发时间），不冒充"刚刚认证"。
- 撤销：`POST /oauth/logout`（同源校验）把会话标记 `revoked_at` 并清 Cookie；Default Frontend 登出时调用。
  不依赖 localStorage 清理；不承诺第三方 RP 本地会话随之退出。
- `prompt=login` / `max_age` 超时 / `id_token_hint` 与当前会话用户不一致 → 重新输入密码，生成新的 `auth_time`。
- 只在同一 origin 使用 Cookie；不设置 `Domain`，不向子域或第三方共享。

## 5. 授权事务与 CSRF

- `GET|POST /oauth/authorize`：先校验 `client_id`（存在且 `active`）与 `redirect_uri`（与注册值逐字节相等）；
  任一失败 → FloatCTF 自己的 400 错误页，**绝不重定向**。之后的参数错误按 RFC 6749 §4.1.2.1 携带
  `error`、`error_description`、`state`、`iss`（RFC 9207）重定向回已验证的回调。
- 重复参数 → `invalid_request`；`request` / `request_uri` / `registration` → `request_not_supported` /
  `request_uri_not_supported` / `registration_not_supported`；`response_mode` 仅支持 `query`。
- 合法请求写入 `oidc_auth_transactions`（TTL 默认 10 分钟），保存 client、redirect、scope、state、nonce、PKCE、prompt、
  max_age 及**浏览器绑定值的哈希**。登录/同意页只携带事务 id，不信任任何 hidden 字段中的协议参数。
- 浏览器绑定：长期随机 Cookie `__Host-fctf_bid`（http 开发下 `fctf_bid`），同一浏览器的多个标签页共享，
  因此多标签并行授权互不覆盖；事务只在同一浏览器内可继续。
- 状态变更表单（`/oauth/login`、`/oauth/consent`）三重防护：事务内的一次性 CSRF token（存哈希）+ 浏览器绑定 Cookie +
  `Origin`（若存在）必须等于 issuer origin。事务在发码后立即标记完成，不可重放。

## 6. 令牌与 Claims

- 授权码：绑定 client、user、redirect_uri、scope、nonce、code_challenge、auth_time、session。
  兑换时先校验绑定再条件 UPDATE（`consumed_at IS NULL AND expires_at > now()`）；0 行 = 重放或并发失败 → `invalid_grant`，
  并撤销该授权码已签发的 access token（RFC 6749 §4.1.2）。
- PKCE 降级防护：发码时有 `code_challenge` 则兑换必须带正确 `code_verifier`；发码时没有则兑换时出现 `code_verifier` 也拒绝
  （RFC 9700 §2.1.1）。
- ID Token（RS256，header 含 `kid`、`typ=JWT`）：`iss`、`sub`（`users.id` UUID 字符串）、`aud`（client_id）、`azp`、`exp`、`iat`、
  `auth_time`（始终提供）、`nonce`（请求带了就原样回显）、`at_hash`。profile / email 等资料 claims **只经 UserInfo 返回**
  （OIDC Core §5.4；Phase 4 OIDF 一致性测试后由「ID Token 也携带」修订为最小披露）。
  不包含 `role`、管理员身份、战队或赛事信息。
- UserInfo（`GET`/`POST`，Bearer header；`POST` 也接受 form `access_token`）：
  `openid` → `sub`；`profile` → `preferred_username`、`nickname`、`name`、`picture`（有头像时）、`updated_at`；
  `email` → `email`、`email_verified: false`（平台没有邮箱确认流程，**不伪造 true**；邮箱为空时不返回）。
- 撤销授权 / 禁用应用 / 删除应用 / 用户被删除 → UserInfo 立即 401（查询时联表校验状态）；已签发的 ID Token 无法追溯撤回，
  RP 本地会话不会自动退出（UI 与文档如实说明）。

## 7. 客户端模型

- `client_id`：`fctf_` + 128-bit 随机；`client_type`：`confidential` | `public`；
  `token_endpoint_auth_method`：机密 `client_secret_basic`（默认）/`client_secret_post`，公开固定 `none`。
- Secret：`fcs_` + 256-bit 随机，仅创建/轮换时返回一次，库内存 Argon2 PHC 串；轮换保留旧 Secret 至宽限期结束
  （`secret_rotation_grace_secs`，默认 24h，可选立即作废）。
- redirect_uri：精确字符串匹配；必须是绝对 URL、无 fragment、无 userinfo；`https` 必需；
  `http://127.0.0.1|[::1]|localhost` 仅当 `allow_loopback_redirect_uris = true`（开发/原生应用受控开关）。
- 状态机：`pending → active | rejected`（管理员）、`active ⇄ disabled`（所有者暂停/恢复）、
  `* → suspended`（管理员封禁，所有者不可解除）、`suspended → active`（管理员恢复）。
- 所有者只能操作自己的应用（服务端按 `owner_user_id` 过滤，越权一律 404）；审批与封禁只走 `SuperAdminJwtGuard`。
- CORS：`/.well-known/*`、`/oauth/token`、`/oauth/userinfo` 允许任意 Origin（这些端点不读取 Cookie），
  `/oauth/authorize|login|consent|logout` 不开放跨域。

## 8. 前端入口

- 协议交互页（登录、同意、错误）由 API 渲染：无论当前激活的是哪个可插拔前端，OIDC 都能工作；
  页面使用 `prefers-color-scheme` 亮暗主题与移动端布局，不引入前端依赖。
- 开发者中心（我的应用 / 创建 / 详情 / 轮换 / 暂停 / 删除）、我的授权（查看 / 撤销）、管理员审核页放在
  `frontends/default`，遵循 Default 规则（GenericTable、Primer、`useConfirm`、无原生弹窗）。
- `@floatctf/sdk` 新增 `service.oidc`（应用、授权、SSO 桥接、登出）与 `admin.oidc_clients`，供其他前端复用。
- Default Frontend：登录成功后 Cookie 由服务端自动下发；应用启动时若持有 JWT 则调用一次桥接；登出时调用 `/oauth/logout`。

## 9. 存储

新增迁移（只新增表，不修改 `users` 与既有表）：

| 表 | 作用 |
|----|------|
| `oidc_clients` | 应用注册、状态、redirect_uris、scopes、认证方式、Secret 哈希（含轮换中的旧 Secret） |
| `oidc_browser_sessions` | 第一方 SSO 会话（只存哈希） |
| `oidc_auth_transactions` | 授权事务（登录/同意跨重定向） |
| `oidc_authorization_codes` | 授权码（只存哈希，一次性） |
| `oidc_consents` | `(user_id, client_id)` 唯一的授权记录 |
| `oidc_access_tokens` | 不透明 access token（只存哈希） |

签名 key 不建表：key ring 由 TOML 引用私钥文件，所有副本读取同一组文件即可保证一致；`kid` 与文件一一对应。
过期数据由新的定时任务 `oidc.cleanup` 清理。

## 10. 风险清单

| # | 风险 | 缓解 |
|---|------|------|
| R1 | 生产反代未加 `/oauth`、`/.well-known` 规则，Discovery 返回 SPA HTML | Caddy dev/prod/install.sh 模板同步修改；运维文档提供 curl 自检 |
| R2 | 签名私钥泄露 | 私钥文件 0600、只读挂载；紧急轮换 runbook；JWKS 只含公钥并有单测 |
| R3 | 多副本 key 不一致 | key ring 从共享文件加载，无启动期随机生成 |
| R4 | 登录暴力破解 | Redis 限流（用户名 + 客户端 IP 两个维度），fail-closed |
| R5 | `X-Forwarded-For` 可伪造 | 生产 API 只经 Caddy 访问（Caddy 覆盖 XFF）；限流同时按用户名计数 |
| R6 | SPA JWT 与 SSO Cookie 生命周期不一致 | 两者 TTL 同为 8h；登出同时撤销；桥接按 JWT 签发时间记录 `auth_time` |
| R7 | 机密客户端关闭 PKCE | 仅管理员可关；关闭后强制 `nonce`；公开客户端不可关 |
| R8 | 用户撤销后 RP 仍持有 ID Token | 文档与 UI 明确说明；UserInfo 立即失效 |
| R9 | 钓鱼应用冒充官方 | 默认需审批；同意页展示真实注册名称、主页域名与"第三方应用"标识 |
| R10 | 邮箱未验证 | `email_verified` 恒为 `false` |

## 11. 回滚

1. 首选：`[oidc].enabled = false` 并重启 API —— 协议端点 404，登录不再写 Cookie，平台其他行为不变。
2. 数据：新表保留，不回滚迁移；需要时由管理员禁用/删除应用。
3. Key 泄露：按 `docs/identity/oidc-operations.md` 紧急轮换（移除泄露 key，立即生效；已签 ID Token 由 RP 侧因 `kid` 缺失验签失败）。
