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

## 后台登录接口测试凭证

AI 在本地开发环境中测试后台登录接口时，使用以下账号：

- 用户名：`sysadmin`
- 密码：`FloatCTF@2025`

> 该凭证仅用于本地开发与测试，禁止用于生产环境或写入测试日志。

## 用户端接口测试账号

AI 测试用户端接口时，不使用预置的固定账号，应先调用注册接口 `POST /api/users` 自行注册。示例：

```bash
curl -X POST http://localhost:9090/api/users \
  -H 'Content-Type: application/json' \
  -d '{
    "username": "ai_test_user_2025",
    "nickname": "AI 测试用户",
    "password": "FloatCTF@Test2025",
    "email": "ai-test-2025@example.com"
  }'
```

注册成功后，使用相同的 `username` 和 `password` 调用 `POST /api/users/session` 登录。若示例用户名或邮箱已存在，应添加时间戳或随机后缀生成唯一值，不要依赖已有用户数据。

## 现有测试分布（参考示例）

- `core/config.rs` — TOML 加载（无环境变量）、数据库 Secret 脱敏
- `core/security/jwt.rs` — JWT 往返（显式注入测试 Secret，不读环境变量）
- `core/secret.rs` — Secret 包装（隐藏值、as_bytes）
- `infrastructure/realtime/publisher.rs` — `RecordingEventPublisher` 记录事件（零依赖异步测试）
- `modules/event/awd/domain/*` — flag/score/network/timing 纯逻辑
- `modules/event/jeopardy/domain/scoring.rs` — 积分公式
- `modules/event/awdp/domain/*` — 阶段/配置/评测分数纯逻辑
- `modules/event/awd/crypto.rs`、`modules/event/awd/infrastructure/firewall/*`、`modules/event/awd/domain/firewall_state.rs` 等 — 加密/防火墙规则

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

## 真实宿主 AWD E2E（helper + Docker + WireGuard + nftables）

业务级验收脚本只应在准备好的 Linux AWD 开发机上执行：它们使用隔离的 PostgreSQL/Redis/API
端口，但会真实创建容器、WireGuard 接口与 nftables 表（经 `floatctf-helper`），结束时自行清理。

```bash
scripts/test-awd-business-e2e.sh                            # baseline：2x30s 完整比赛 + 精确计分账本
AWD_E2E_SCENARIO=hardening scripts/test-awd-business-e2e.sh # 40s 硬化期 + 暂停/恢复边界
AWD_E2E_SCENARIO=reset scripts/test-awd-business-e2e.sh     # 靶机重置：免费/罚分/归属/暂停/结算边界
scripts/test-awd-boundaries-e2e.sh                          # 顺序执行 hardening + reset 两个场景
```

前置条件（脚本 preflight 会逐项检查并说明失败原因）：`floatctf-helper.service` active、
`floatctf/awd-flagserver:latest` 与 `floatctf/awd-judgeserver:latest` 镜像存在、开发库无
active AWD 赛事、无残留 `fctf-awd-*` 网络 / `fawg_*` 接口 / AWD 容器。

## 宿主代理（HTTP_PROXY）与内网直连

平台内部探针/评测都直连 Docker 内网（`10.42.x`、`10.43.x`）。宿主一旦设置
`HTTP(S)_PROXY`（国内开发机为 cargo/docker 构建设代理很常见），`reqwest`
默认会把内网请求转给代理由代理转发，而代理无法回连这些地址 → 健康检查恒失败、
AWD/AWDP 评测测试大面积报 `ServiceDown`。

- 测试入口 `scripts/test-rust.sh` 已自动导出 `NO_PROXY`（含 `10.0.0.0/8`、
  `172.16.0.0/12`、`192.168.0.0/16`，reqwest 的 `NO_PROXY` 支持 CIDR）。
- 业务代码里访问内网的 reqwest 客户端必须显式 `.no_proxy()`
  （见 `modules/gamebox/healthcheck.rs` 与三个 judgeserver/flagserver）。
- 回归用例：`cargo test -p floatctf --test gamebox_healthcheck_proxy`。

## 测试禁忌

- ❌ 测试里设置/读取真实环境变量（历史教训：`SECRET`/`REALTIME_REDIS_URL` 的测试代码已全部移除）。
  唯一例外：进程内注入**受控**变量、`Drop` 时还原、且该测试文件只含这一个测试，并自带对照断言
  （例：`gamebox_healthcheck_proxy.rs` 注入「死代理」证明探针确实关闭了系统代理继承）。
- ❌ 单元测试连真实数据库/Docker（属于集成测试层级，且要显式标出）
- ❌ 只测 happy path 不加边界值（空输入、越界、非法状态）
- ❌ 提交编译不过或测试失败的代码（保持 `cargo test -p floatctf` 绿色）
