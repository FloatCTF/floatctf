# 用 FloatCTF 账号登录你的应用（OIDC 接入指南）

本文面向第三方开发者。FloatCTF 是标准的 OpenID Connect Provider，你**不需要**任何 FloatCTF 私有 SDK，
用所在语言成熟的 OIDC 客户端库即可。端点与字段细节见 [oidc-provider.md](./oidc-provider.md)。

## 1. 你需要的四样东西

| 项 | 从哪里来 |
|----|----------|
| Issuer | 平台地址，例如 `https://ctf.example.com`（Discovery：`<issuer>/.well-known/openid-configuration`） |
| Client ID | 开发者中心创建应用后显示 |
| Client Secret | 仅机密客户端；**只在创建 / 轮换时显示一次** |
| Redirect URI | 你登记的回调地址（逐字节匹配，必须 https） |

## 2. 登记应用

1. 用 FloatCTF 账号登录平台，进入 **Account → Developer Apps → 新增**。
2. 选择类型：
   - **机密客户端**：有服务端、能安全保存 secret（Web 后端、BFF）。认证方式默认 `client_secret_basic`，也可选 `client_secret_post`。
   - **公开客户端**：纯前端 SPA / 移动端，没有 secret，必须使用 PKCE。
3. 填写回调地址（每行一个，**完全一致**匹配：协议、主机、端口、路径、查询串都要相同，不支持通配符）。
4. 勾选需要的用户信息：`openid`（必需）、`profile`、`email`。
5. 保存弹窗中的 Client Secret——关闭后无法再看到，只能轮换。
6. 默认需要平台管理员审批，状态变为「已启用」后才能登录。

## 3. 协议流程（Authorization Code + PKCE）

```
浏览器 ──(1) 302 /oauth/authorize?response_type=code&client_id&redirect_uri&scope=openid profile
                 &state&nonce&code_challenge&code_challenge_method=S256──▶ FloatCTF
FloatCTF ── 已登录则跳过密码；首次需用户同意 ──(2) 302 redirect_uri?code&state&iss──▶ 你的回调
你的后端 ──(3) POST /oauth/token（code + redirect_uri + code_verifier + 客户端认证）──▶ FloatCTF
         ◀── access_token、id_token、scope
你的后端 ──(4) 验证 id_token，（可选）GET /oauth/userinfo ──▶ 建立你自己的会话
```

**必须做的校验**（成熟库默认都会做）：

1. 回调中的 `state` 等于你发出的值；`iss` 参数等于 issuer（RFC 9207，防 mix-up）。
2. ID Token 签名：用 `jwks_uri` 中 `kid` 对应的 RS256 公钥验证；拒绝 `alg=none` 或其它算法。
3. `iss` == issuer（逐字节）、`aud` 包含你的 `client_id`、`exp` 未过期、`nonce` 等于你发出的值。
4. 用 **`(iss, sub)`** 识别用户。不要用用户名、昵称或邮箱作为主键——它们可能改变。
5. ID Token 只含身份信息（`sub`、`auth_time` 等）；用户名、昵称、头像、邮箱请用 access token 调用 UserInfo 获取。
6. `email_verified` 恒为 `false`：FloatCTF 没有验证邮箱，**不要**据此自动合并已有账号。

## 4. 示例（均已在测试中对 FloatCTF 实际跑通）

### Node.js — openid-client v6（公开或机密客户端）

```js
import * as client from 'openid-client';

const config = await client.discovery(new URL('https://ctf.example.com'), CLIENT_ID, CLIENT_SECRET);
// 公开客户端：client.discovery(issuer, CLIENT_ID, undefined, client.None())

const code_verifier = client.randomPKCECodeVerifier();
const state = client.randomState(), nonce = client.randomNonce();
const url = client.buildAuthorizationUrl(config, {
  redirect_uri: 'https://app.example.com/callback', scope: 'openid profile',
  code_challenge: await client.calculatePKCECodeChallenge(code_verifier),
  code_challenge_method: 'S256', state, nonce,
});
// …重定向用户到 url，回调时：
const tokens = await client.authorizationCodeGrant(config, currentUrl, {
  pkceCodeVerifier: code_verifier, expectedState: state, expectedNonce: nonce, idTokenExpected: true,
});
const { iss, sub } = tokens.claims();
const profile = await client.fetchUserInfo(config, tokens.access_token, sub);
```

### Python — Authlib（机密客户端，client_secret_basic）

```python
from authlib.integrations.requests_client import OAuth2Session
from authlib.oidc.core import CodeIDToken
from authlib.jose import JsonWebKey, jwt
import requests

meta = requests.get(ISSUER + "/.well-known/openid-configuration").json()
s = OAuth2Session(CLIENT_ID, CLIENT_SECRET, scope="openid profile email",
                  redirect_uri=REDIRECT_URI, code_challenge_method="S256")
auth_url, state = s.create_authorization_url(meta["authorization_endpoint"], nonce=nonce, code_verifier=verifier)
# 回调：
token = s.fetch_token(meta["token_endpoint"], authorization_response=callback_url, code_verifier=verifier)
claims = jwt.decode(token["id_token"], JsonWebKey.import_key_set(requests.get(meta["jwks_uri"]).json()),
                    claims_cls=CodeIDToken,
                    claims_options={"iss": {"essential": True, "value": ISSUER}},
                    claims_params={"nonce": nonce, "client_id": CLIENT_ID, "access_token": token["access_token"]})
claims.validate()
```

### Go — coreos/go-oidc v3 + golang.org/x/oauth2（机密客户端，client_secret_post）

```go
provider, _ := oidc.NewProvider(ctx, "https://ctf.example.com")
conf := oauth2.Config{ClientID: id, ClientSecret: secret, RedirectURL: cb,
    Endpoint: provider.Endpoint(), Scopes: []string{oidc.ScopeOpenID, "profile"}}
// client_secret_post：conf.Endpoint.AuthStyle = oauth2.AuthStyleInParams
verifier := oauth2.GenerateVerifier()
url := conf.AuthCodeURL(state, oidc.Nonce(nonce), oauth2.S256ChallengeOption(verifier))
// 回调：
tok, _ := conf.Exchange(ctx, code, oauth2.VerifierOption(verifier))
idt, _ := provider.Verifier(&oidc.Config{ClientID: id}).Verify(ctx, tok.Extra("id_token").(string))
// 检查 idt.Nonce == nonce；用 idt.Issuer + idt.Subject 识别用户
```

### 其他平台（例如 Supabase Auth / LearnOS）

任何支持「自定义 OIDC Provider」的平台，按下列方式填写即可：Issuer / Discovery URL = 上文；
Client ID / Secret = 开发者中心的值；回调地址 = 该平台给出的 callback（需登记到 FloatCTF）；
Scopes = `openid profile email`。注意该平台对 `email_verified=false` 的处理（部分平台会因此拒绝自动关联账号）。
**本仓库未对 Supabase Auth 做实测**，接入前请在你的环境验证。

## 5. 常见错误

| 现象 | 原因 |
|------|------|
| FloatCTF 页面显示「回调地址与该应用登记的地址不一致」 | `redirect_uri` 与登记值不完全一致（末尾斜杠、端口、查询串都算） |
| 「该应用当前未启用」 | 仍待审批、已被你暂停或被管理员封禁 |
| 回调带 `error=invalid_request`、描述 `code_challenge … required` | 没有使用 PKCE S256 |
| Token 端点 `invalid_client` | secret 错误，或认证方式与登记不符（例如登记 basic 却用了 post） |
| Token 端点 `invalid_grant` | 授权码过期（60 秒）、已用过、`redirect_uri` 不一致、`code_verifier` 不匹配 |
| UserInfo 401 `invalid_token` | access token 过期（1 小时）、用户撤销了授权、应用被停用 |
| 回调带 `error=login_required` / `consent_required` | 你用了 `prompt=none`，而用户未登录 FloatCTF / 尚未同意 |
| 回调带 `error=access_denied` | 用户在 FloatCTF 登录、账号选择或同意页点了「取消 / 拒绝」；不要为此建立登录会话 |
| 用户退出你的应用后再点登录，被直接带回且没有机会换账号 | 你的应用退出只结束了自己的会话，FloatCTF SSO 仍有效（正常现象）。在用户主动点击的登录按钮上传 `prompt=select_account`，FloatCTF 会显示账号选择页 |

## 6. 用户撤销与登出

- 用户可以在 FloatCTF「Authorized Apps」中撤销授权：你持有的 access token 立即失效，下次登录需重新同意。
- FloatCTF **不会**通知你，也不能退出你应用内的会话；需要时请缩短你自己的会话或定期用 `prompt=none` 重新确认。
- 当前不支持 Refresh Token 与 RP-Initiated Logout。
- 你的应用「退出」**不会**退出 FloatCTF（浏览器也不允许你清除 FloatCTF 的 Cookie）。请在退出后的界面如实说明，
  并在用户主动点击的「使用 FloatCTF 登录 / 绑定 FloatCTF」按钮上传 `prompt=select_account`：有 SSO 时用户会看到
  FloatCTF 的账号选择页，可免密码继续当前账号或输入密码使用其他账号。后台静默续期等非用户主动场景不要传它。
- 账号绑定（把 FloatCTF 身份关联到你应用里已有的账号）同样应传 `prompt=select_account`，避免浏览器里残留的
  其他 FloatCTF 账号被静默绑定；始终以 `(iss, sub)` 关联，不要按邮箱自动合并。
