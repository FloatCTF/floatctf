# FloatCTF API 测试说明

## 现状分层

| 层级 | 命令 | 依赖 |
|------|------|------|
| **单元** | `cargo test jwt_roundtrip` / `cargo test dynamic_score` | 无 |
| **路由目录** | `cargo test --test http_auth_contract catalog_sizes` | 无 |
| **鉴权契约** | `cargo test --test http_auth_contract` | 运行中的 API |
| **业务冒烟** | `cargo test --test http_flow` | 运行中的 API + 可选账号 |

运行 API 测试：`cargo test -p floatctf`

## HTTP 测试如何跑

1. 启动完整栈（Postgres / Docker / RustFS + floatctf）
2. 默认请求 `http://127.0.0.1:8080`，可改：
   ```bash
   export FLOATCTF_API_BASE=http://127.0.0.1:8080
   ```
3. 可选登录账号（用于 GET 列表与 EventMode 冒烟）：
   ```bash
   export FLOATCTF_TEST_USER=...
   export FLOATCTF_TEST_PASS=...
   export FLOATCTF_TEST_ADMIN=...
   export FLOATCTF_TEST_ADMIN_PASS=...
   ```
4. 若希望「API 未启动就失败」：
   ```bash
   export FLOATCTF_API_REQUIRE=1
   ```

## 覆盖范围

- `tests/common/routes.rs`：与 `service` / `admin` 配置对齐的**全路由目录**（100+）
- **无 Token**：所有 `UserRequired` / `SuperAdminRequired` 必须 401/403
- **有 User Token**：常见列表 `code==0`；user 不能进 admin
- **有 Admin Token**：admin GET 列表 `code==0`，且不出现 500
- **EventMode**：若库内有 Event，探测 `/scoreboard` `/trend` 不 500

## 做不到 / 未自动保证的

- Docker 开实例、题包 import/build、AWD 全链路：依赖真实环境与副作用，需人工或专用 e2e
- 未启动 API 时 HTTP 测试**默认 soft-skip**（避免 CI 无栈直接红）

业务语义断言请优先补在 `strategies/event/*` 单元测试（如 DynamicScore）。
