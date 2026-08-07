# 测试规范（TESTING.md）

> 测试是功能提交的门禁。目标：纯逻辑全覆盖、配置解析有测试、外部依赖不进默认测试路径。

## 测试层级

| 层级 | 位置 | 依赖 | 命令 |
|------|------|------|------|
| 单元测试 | 源码内 `#[cfg(test)] mod tests` | 无（禁 I/O） | `cargo test -p floatctf <关键词>` |
| 路由目录测试 | `apps/api/tests/` 下模块 | 无 | `cargo test --test http_auth_contract catalog_sizes` |
| 鉴权契约 | `apps/api/tests/http_auth_contract.rs` | 运行中的 API | `cargo test --test http_auth_contract` |
| 业务冒烟 | `apps/api/tests/http_flow.rs` | 运行中的 API + 可选账号 | `cargo test --test http_flow` |

全部跑：`cargo test -p floatctf`（mise 任务 `mise run test` 跑 workspace + 前端）。

## 现有测试分布（参考示例）

- `core/config.rs` — TOML 加载（无环境变量）、数据库 Secret 脱敏
- `core/security/jwt.rs` — JWT 往返（显式注入测试 Secret，不读环境变量）
- `core/secret.rs` — Secret 包装（隐藏值、as_bytes）
- `infrastructure/realtime/publisher.rs` — `RecordingEventPublisher` 记录事件（零依赖异步测试）
- `modules/event/awd_team/domain/*` — flag/score/network 纯逻辑
- `modules/event/jeopardy/domain/scoring.rs` — 积分公式
- `modules/event/awd_team/crypto.rs`、`system/firewall.rs` 等 — 加密/防火墙规则

## 写测试的硬性规则

1. **纯逻辑测试**：优先放在 `domain/` 或对应模块文件的 `#[cfg(test)] mod tests`，只测函数、不连 DB/Docker。
2. **禁止读取真实环境变量**：配置一律来自 TOML 或显式参数。JWT/加密测试用显式 Secret：
   ```rust
   jwt::configure_jwt_secret(Secret::new("test-secret-16chars-min".into()));
   ```
   不要 `std::env::set_var("SECRET", ...)`。
3. **异步测试**：`#[tokio::test]`；外部依赖用注入替代（如 `RecordingEventPublisher` 替真实 Redis）。
4. **Secret 脱敏测试**：新增含敏感字段的配置/结构体时，加一条"Debug 输出不含明文"的测试：
   ```rust
   let out = format!("{:?}", cfg);
   assert!(!out.contains("postgres:postgres"));
   ```
5. **配置解析测试**：新增 TOML 字段时，扩展 `toml_config_loads_without_environment_variables` 这类测试，验证默认值与解析。
6. **回归测试（修 bug 时）**：先写复现失败的测试 → 修复 → 转绿。
7. **不依赖网络/容器的用例**必须能离线跑通 `cargo test -p floatctf`。

## 常用命令速查

```bash
cargo test -p floatctf                    # 全部单元测试
cargo test -p floatctf core::config       # 指定模块
cargo test -p floatctf toml_config        # 关键词过滤
cargo test --test http_auth_contract      # API 契约（需 API 运行）
cargo fmt --all && cargo check -p floatctf  # 提交前必跑
```

## 测试禁忌

- ❌ 测试里设置/读取真实环境变量（历史教训：`SECRET`/`REALTIME_REDIS_URL` 的测试代码已全部移除）
- ❌ 单元测试连真实数据库/Docker（属于集成测试层级，且要显式标出）
- ❌ 只测 happy path 不加边界值（空输入、越界、非法状态）
- ❌ 提交编译不过或测试失败的代码（保持 `cargo test -p floatctf` 绿色）
