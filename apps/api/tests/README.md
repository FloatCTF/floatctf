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

1. 启动完整栈：`mise run dev`（Postgres / Redis / RustFS / Docker + floatctf API）。
2. **必须导出正确的 API base**，否则测试会**静默 soft-skip**（看起来"全绿"，其实一个用例都没跑）。

   端口事实（写文档时以仓库为准）：

   | 来源 | 值 |
   |------|-----|
   | 测试 harness 的**默认** base（`tests/common/mod.rs` 的 `base_url()`） | `http://127.0.0.1:8080` |
   | `mise run dev` 实际监听的 API（`apps/api/config/development.toml` 的 `[server].listen_port`） | `0.0.0.0:9090`，本机直连 `http://127.0.0.1:9090` |
   | 开发统一入口（开发 Caddy 反代 `/api/*`） | `http://127.0.0.1:7780` |

   默认的 `8080` **没有任何服务在听**，所以对 `mise run dev` 起的 API 必须显式导出：

   ```bash
   # 直连开发 API（推荐）
   export FLOATCTF_API_BASE=http://127.0.0.1:9090

   # 或经开发 Caddy 统一入口（同样能代理 /api/*）
   export FLOATCTF_API_BASE=http://127.0.0.1:7780
   ```

3. 可选登录账号（用于 GET 列表与 EventMode 冒烟）：
   ```bash
   export FLOATCTF_TEST_USER=...
   export FLOATCTF_TEST_PASS=...
   export FLOATCTF_TEST_ADMIN=...
   export FLOATCTF_TEST_ADMIN_PASS=...
   ```
4. 若希望「API 未启动/端口写错就失败」而不是静默 soft-skip：
   ```bash
   export FLOATCTF_API_REQUIRE=1
   ```

   设了它以后 base 不可达会直接 panic（`FLOATCTF_API_REQUIRE=1 but API not reachable at …`），
   是发现端口写错的最快方式。

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
