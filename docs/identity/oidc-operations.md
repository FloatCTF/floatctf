# FloatCTF OIDC Provider —— 部署与运维手册

适用于 `oidc` 分支引入的 OpenID Connect Provider。默认 **关闭**（`[oidc].enabled = false`），关闭时平台行为与之前完全一致。

## 1. 开启前检查清单

- [ ] 站点已经是 **HTTPS**，并且有固定域名（issuer 不能是 IP、不能随 `Host` 变化）。
- [ ] 已执行包含迁移 `20261023162115-oidc-provider.sql` 的版本（`mise run db:migration:status` 显示 APPLIED）。
- [ ] Caddy 已把 `/oauth/*` 与三个 `/.well-known/*` 文档反代到 API（安装器模板与 `infra/caddy/*` 已包含；自定义代理需自行添加）。
- [ ] Redis 正常（登录 / token 限流 fail-closed，Redis 故障时 OIDC 登录会被拒绝）。
- [ ] 已准备好签名私钥并决定 `kid`。

## 2. 生成签名 key

```bash
# 生产（安装器已创建 $FLOATCTF_HOME/config/oidc，容器内只读挂载为 /etc/floatctf/oidc）
sudo scripts/oidc-keygen.sh /var/lib/floatctf/config/oidc oidc-2026-10-a
sudo chown root:floatctf /var/lib/floatctf/config/oidc/oidc-2026-10-a.pem
sudo chmod 0640 /var/lib/floatctf/config/oidc/oidc-2026-10-a.pem

# 本地开发（/app 已被 gitignore）
scripts/oidc-keygen.sh app/secrets/oidc dev-key-1
```

- RSA 3072 位 PKCS#8 PEM；脚本拒绝覆盖已有文件，并打印公钥指纹用于登记。
- 私钥**不要**提交 Git、不要写进数据库、不要放进 TOML 正文。备份时与其它密钥同等保护。
- API 容器以 `65532:floatctf` 运行，私钥需对 `floatctf` 组可读（0640）。权限对其他用户可读时 API 会在日志告警。

## 3. 配置

```toml
[oidc]
enabled = true
issuer = "https://ctf.example.com"          # 必须与浏览器看到的 origin 逐字节一致，无尾斜杠、无路径
require_client_approval = true              # 新应用需管理员审批（建议保持）
# account_chooser = true                    # prompt=select_account 显示账号选择页（默认）；false 仅为临时回滚
# allow_loopback_redirect_uris = false      # 仅开发 / 原生应用测试时开启
# session_ttl_secs = 28800                  # SSO 会话 8h
# authorization_code_ttl_secs = 60          # ≤ 120
# access_token_ttl_secs = 3600
# id_token_ttl_secs = 3600
# transaction_ttl_secs = 600
# secret_rotation_grace_secs = 86400        # client secret 轮换宽限期

[[oidc.signing_keys]]
kid = "oidc-2026-10-a"
private_key_file = "/etc/floatctf/oidc/oidc-2026-10-a.pem"   # 相对路径以 TOML 所在目录为基准
active = true
```

启动期严格校验（任一失败 → API 拒绝启动，日志 `OIDC provider initialization failed` / `configuration error`）：
issuer 形态、恰好一把 `active` key、`kid` 唯一且只含 `[A-Za-z0-9._-]`、私钥可读且为 2048–4096 位 RSA、TTL 范围。
**不会**临时生成 key。

运行期可调（设置表，管理端「Settings」或 SQL，缓存 60 秒）：

| Key | 默认 | 含义 |
|-----|------|------|
| `OIDC_LOGIN_RATE_PER_USER` | 10 | 同一用户名 5 分钟内 OIDC 登录尝试次数 |
| `OIDC_LOGIN_RATE_PER_IP` | 50 | 同一来源 IP 5 分钟内 OIDC 登录尝试次数 |

## 4. 上线自检

```bash
ISSUER=https://ctf.example.com
curl -fsS $ISSUER/.well-known/openid-configuration | jq '.issuer, .authorization_endpoint, .jwks_uri'
curl -fsS $ISSUER/.well-known/jwks.json | jq '.keys[] | {kid, kty, alg, use, has_private: has("d")}'   # has_private 必须为 false
curl -s -o /dev/null -w '%{http_code} %{content_type}\n' "$ISSUER/oauth/authorize"                 # 400 text/html（不是 SPA 的 200）
curl -s -o /dev/null -w '%{http_code}\n' -X POST $ISSUER/oauth/token                                # 400（JSON 错误）
```

- `issuer` 字段必须与 `$ISSUER` 完全一致；所有端点 URL 以它开头。
- 如果 `/.well-known/openid-configuration` 返回 HTML，说明反代把它交给了前端 SPA，检查 Caddy 规则顺序。
- 管理员在「Admin → Access → OIDC Clients」审批应用；玩家在「Account → Developer Apps / Authorized Apps」管理。

## 5. 签名 key 常规轮换（零停机）

ID Token 默认 1 小时有效，JWKS 允许 RP 缓存 5 分钟。按下列步骤，旧令牌在过期前始终可验：

1. **预发布**新 key：生成 `oidc-2027-01-a`，在 TOML 中追加为 `active = false`，滚动重启所有 API 副本。
   JWKS 现在同时包含新旧公钥；签发仍用旧 key。
2. 等待 ≥ JWKS 缓存时间（5 分钟，若 RP 缓存更久则按其最长值）。
3. **切换**：新 key `active = true`、旧 key `active = false`，滚动重启。新 ID Token 的 `kid` 变为新 key；旧令牌仍可用旧公钥验证。
4. 等待 ≥ `id_token_ttl_secs` + JWKS 缓存时间。
5. **退役**：从 TOML 删除旧 key，重启；归档或销毁旧私钥文件。

多副本必须读取同一组 key 文件、同一份 `[[oidc.signing_keys]]` 配置；任一副本的 JWKS 不一致都会导致 RP 间歇性验签失败。

## 6. 紧急轮换（私钥疑似泄露）

1. 立即生成新 key，TOML 中 **删除** 泄露的 key（不要保留为 inactive），新 key `active = true`，重启所有副本。
2. 泄露 key 签发的 ID Token 将因 `kid` 不在 JWKS 中而验签失败（RP 刷新 JWKS 后生效，最长约 5 分钟）。
3. 通知接入方：要求用户重新登录、检查期间可疑登录。access token 不依赖签名 key，如需一并作废，
   可在数据库执行 `UPDATE oidc_access_tokens SET revoked_at = now() WHERE revoked_at IS NULL;`。
4. 记录事件、复盘私钥存放与访问控制。

平台 API 的 HS512 `auth.jwt_secret` 与 OIDC key 完全独立；轮换任何一方都不影响另一方。

## 7. 日志与审计

- 管理端「Logs」中 `category = OIDC`：`OIDC_LOGIN` / `OIDC_LOGIN_FAILED` / `OIDC_CONSENT_GRANT` / `OIDC_CONSENT_DENY` /
  `OIDC_CONSENT_REVOKE` / `OIDC_TOKEN` / `OIDC_TOKEN_REJECTED` / `OIDC_CLIENT_*` / `OIDC_SECRET_ROTATE`；
  账号选择：`OIDC_ACCOUNT_SELECTED` / `OIDC_ACCOUNT_SWITCH` / `OIDC_ACCOUNT_SELECT_CANCEL` / `OIDC_LOGIN_HINT_MISMATCH`。
- 日志只含 `client_id`、scope、错误码等元数据；不记录 code、token、secret、密码、PKCE verifier。
- 授权码重放会额外打出 `OIDC authorization code replay detected` 警告。

## 8. 数据清理与容量

- 系统定时任务 `identity.oidc.cleanup`（每 10 分钟，管理端「Scheduled Tasks」可见）删除过期 1 天以上的
  授权事务、授权码、access token，以及过期或撤销 7 天以上的 SSO 会话。
- 表：`oidc_clients`、`oidc_consents`（长期）；`oidc_browser_sessions`、`oidc_auth_transactions`、
  `oidc_authorization_codes`、`oidc_access_tokens`（短期）。

## 9. 回滚

1. 首选：`[oidc].enabled = false` 并重启。协议端点立即 404，平台登录不再写 SSO Cookie，其它功能不变。
   RP 现有登录会话由 RP 自己管理，不受影响；access token 在关闭期间无法使用。
2. 数据保留，不回滚迁移；重新开启后应用与授权记录仍在。
3. 停止某个应用：管理员「封禁」，或所有者「暂停」。

### 9.1 账号选择（`prompt=select_account`）的上线与回滚

- 依赖迁移 `20261025145704-oidc-account-chooser.sql`（`oidc_auth_transactions` 新增 4 个可空列，只前进，不改已有迁移）。
  部署前 `pg_dump` 备份；`migrate.sh apply` 后再启动新版 API。旧版 API 读写该表不受新列影响，因此迁移可先于代码上线。
- **同一数据库上的所有 FloatCTF API（例如两个 issuer 共用库的拓扑）都要升级到新版本**，否则旧版实例收到
  `select_account` 时仍导向同意页（体验退化，不越权）。每个 issuer 的 Cookie 只属于自己的主机，选择页只会看到本 issuer 的会话。
- 部署顺序：**先 FloatCTF（OP），再 RP**（例如 LearnOS 开始发送 `prompt=select_account`）。不传 prompt 的 RP 不受影响。
- 回滚：`[oidc].account_chooser = false` 并重启 —— `select_account` 回到旧行为（同意页），其余 OIDC 不变；
  这是临时兼容模式。需要回退代码时直接部署上一版本，新增列保留在库中即可（全部可空，旧代码忽略）。
- 升级时进行中的授权事务（默认 10 分钟有效）继续可用：新列为空即「尚未选择」，带 `select_account` 且停在旧同意页的事务，
  提交后会先被带到账号选择页再继续；不带 prompt 的事务行为不变。

## 10. 故障排查

| 症状 | 排查 |
|------|------|
| API 启动失败 `OIDC provider initialization failed` | 私钥路径（容器内路径）、文件权限、PEM 是否加密、kid 是否匹配 |
| RP 报 `issuer mismatch` | TOML issuer 与 RP 配置 / 浏览器 origin 不一致（http/https、端口、尾斜杠） |
| 登录页提交后显示「请求来源无效」 | 反代改写了 `Origin`，或页面不是从 issuer origin 打开 |
| 登录总提示「尝试次数过多」 | 限流命中；检查是否有撞库，必要时调高设置或等待 5 分钟 |
| 登录页提示「服务暂时不可用」 | Redis 不可达（限流 fail-closed） |
| 已登录 FloatCTF 仍要求输入密码 | 浏览器没有 SSO Cookie：确认平台与 `/oauth/*` 同源；旧会话在进入选手区时会自动桥接 |
| 授权页提示「授权请求已过期或无效」 | 事务超过 10 分钟、已完成，或在另一个浏览器中继续 |

## 11. 统一身份（主站 Cookie 会话、多账号、标准退出）

设计见 [ADR-unified-identity.md](ADR-unified-identity.md)，审计与威胁模型见 [unified-identity-audit.md](unified-identity-audit.md)，
测试证据见 [oidc-test-report.md](oidc-test-report.md) §9。三个开关默认全部关闭；**关闭任一开关即回到对应的旧行为，不删任何数据**。

### 11.1 前提

- **主站与 OP 同一主机**：主站 Cookie 会话就是 OP 身份会话，host-only Cookie 只在同主机间共享（端口不隔离 Cookie）。
  主站 origin 与 issuer 完全相同时无需额外配置；同主机不同端口（例如主站 `https://10.66.66.7`、issuer `https://10.66.66.7:8443`）
  时把主站 origin 写进 `first_party_origins`。不同主机名的入口之间登录态不共享——这是浏览器同源机制，不是缺陷。
- **不改生产 issuer**。RP 以 `(iss, sub)` 识别外部账号；改 `iss` 会让 RP 把同一个人当成新用户。
  推荐的长期形态是一个 canonical HTTPS 身份域名（内外网经分流 DNS / 反代访问**同一个** origin 与 issuer），
  迁移到它需要 RP 侧先为每个用户补齐新 issuer 的关联，属于单独的变更，不在本轮。
- **双 issuer 共用一个数据库**（两个 API 实例各自 issuer）时：两个实例都必须升级；每个实例只投递自己 issuer 的退出通知；
  LearnOS 的 `FLOATCTF_OIDC_TRUSTED_ISSUERS` 列出它接受 Logout Token 的 issuer（缺省只信任 `FLOATCTF_OIDC_ISSUER`），
  映射表按 `(iss, sid)` 区分，不会把两个 issuer 的会话混为一谈。
- OP 页面（登录 / 选择 / 同意 / 退出）的 Logo、样式、脚本、头像都由 API 自己的 `/oauth/assets/*`、`/oauth/accounts/{id}/avatar`
  提供，因此「issuer 站点把所有路径都反代给 API」的部署（无 `/public`、`/float.png`）也不会再出现破图。

### 11.2 备份（上线前必做，必须能恢复）

```bash
sudo $FLOATCTF_HOME/backup.sh --out floatctf-before-unified-$(date +%F).tar.gz   # 安装器部署：库 + config（含 OIDC 私钥）+ RustFS
# 手工部署至少：
docker exec floatctf-postgres pg_dump -U postgres -d floatctf_db --format=custom > floatctf_db-$(date +%F).dump
cp -a $FLOATCTF_HOME/config/oidc oidc-keys-$(date +%F)                               # 签名私钥（0640 root:floatctf）
pg_dump "$LEARNOS_DB_URL" --format=custom > learnos-$(date +%F).dump                 # LearnOS
```

在一次性库上演练恢复：`createdb restore_check && pg_restore -d restore_check floatctf_db-*.dump`，确认 `users`、`oidc_*` 行数一致。

### 11.3 上线顺序

1. **数据库迁移**（只新增表/列，旧版 API 不受影响）：`migrate.sh apply`，包含
   `20261025201938-unified-identity-sessions.sql`、`20261025211040-oidc-standard-logout.sql`；`migrate.sh status` 确认无 pending。
2. **FloatCTF API**（OP + 主站接口）升级到新版本，三个开关保持关闭；共用库的每个实例都升级。
3. **主站前端**（Default Frontend）升级。开关关闭时状态接口返回 `mode=legacy`，前端继续用平台 JWT。
4. **LearnOS** 升级：Drizzle 迁移 `0001_floatctf_backchannel`（新表），Better Auth `disableProviderLogout: true`。
   **必须在第 7 步之前完成**，否则 LearnOS 的普通「退出」会变成全局退出（ADR 附录 A，I11）。
5. 开 `multi_account = true` → 重启 → 验证 11.4 的 1–2。旧的单会话 `__Host-fctf_sso` 会在下次访问时迁入账号容器。
6. 开 `unified_web_session = true`（同主机不同端口的入口同时配置 `first_party_origins`）→ 重启 → 验证 11.4 的 3–4。
   已登录用户的旧 localStorage JWT 在首次打开主站时换成服务端会话后删除，不需要重新登录。
7. 在开发者中心为 LearnOS 登记 `post_logout_redirect_uris = ["<LearnOS>/auth/signed-out"]` 与
   `backchannel_logout_uri = "<LearnOS>/api/oidc/backchannel-logout"`；RP 在内网时把 `host:port` 写进
   `backchannel_trusted_hosts`，自签证书时配置 `backchannel_extra_ca_file`；开 `backchannel_logout = true` → 重启 → 验证 11.4 的 5–6。

### 11.4 上线自检

```bash
ISS=https://ctf.example.com
curl -s $ISS/api/users/web-session | jq '.data | {mode, authenticated, max_accounts, global_logout}'   # mode=cookie
curl -s $ISS/.well-known/openid-configuration | jq '{end_session_endpoint, backchannel_logout_supported}'
```

1. 浏览器主站登录 → DevTools 只看到 HttpOnly 的 `__Host-fctf_accounts`，`localStorage["auth-storage"]` 没有 `token`。
2. 主站「添加账号」登录第二个账号 → 账号菜单可切换；LearnOS「使用 FloatCTF 登录」→ 选择页列出这两个账号。
3. 新浏览器先在 LearnOS 经 FloatCTF 输入密码 → 打开主站自动登录同一账号。
4. 主站「退出当前账号」→ LearnOS 已有会话仍可用；LearnOS 再次 SSO 需要密码。
5. 主站「退出所有关联应用…」→ 结果对话框显示 LearnOS「已确认退出」，LearnOS 页面刷新后回到登录页。
6. 查看投递：`select status, attempts, last_error from oidc_backchannel_deliveries order by created_at desc limit 5;`

### 11.5 回滚

| 现象 / 目的 | 操作 | 结果 |
|---|---|---|
| 标准退出有问题 | `backchannel_logout = false`，重启 | Discovery 不再宣告 `end_session_endpoint`，ID Token 不带 `sid`，主站不显示「退出所有关联应用」；未投递的 outbox 行不再处理 |
| 主站 Cookie 会话有问题 | `unified_web_session = false`，重启 | 状态接口 `mode=legacy`；主站回到密码登录 + 平台 JWT（用户需重新登录一次）；OIDC 不受影响 |
| 多账号有问题 | `multi_account = false`（同时关闭上一项），重启 | 回到单会话 `__Host-fctf_sso`；容器 Cookie 被忽略，用户在 OIDC 页面重新登录一次 |
| 回退代码 | 部署上一版本 API 与前端 | 新表/新列保留在库中，旧代码不读取；**不要**回滚或删除迁移 |
| 数据损坏（最后手段） | `restore.sh` / `pg_restore` 到 11.2 的备份 | 备份之后的业务数据会丢失，需要单独评估 |

LearnOS 回滚：部署上一版本即可（新表 `floatctf_oidc_sessions`、`floatctf_logout_jti` 闲置）；
**OP 的 `backchannel_logout` 必须先关闭**，再回滚到没有 `disableProviderLogout` 的 LearnOS 版本。

### 11.6 限流与测试环境

主站 `/api/users/web-session/login` 与 OIDC 登录页共用限流：每用户名 5 分钟 `OIDC_LOGIN_RATE_PER_USER`（默认 10）次、
每 IP `OIDC_LOGIN_RATE_PER_IP`（默认 50）次，Redis 不可达时拒绝登录（fail-closed）。
跨应用 E2E 在同一 IP 上连续登录上百次，只在**隔离测试库**里调高这两个设置；生产保持默认。
