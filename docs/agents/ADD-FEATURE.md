# 添加新功能（ADD-FEATURE.md）

> 本文件是"从需求到上线"的完整操作手册。动手前先读 [ARCHITECTURE.md](./ARCHITECTURE.md) 建立心智模型；涉及数据库先读 [DATABASE.md](./DATABASE.md)；写测试前读 [TESTING.md](./TESTING.md)。

## 总览：新功能生命周期

```
0. 理解需求 → 1. 定位模块 → 2. 数据模型 → 3. 配置 → 4. 后端实现 → 5. 前端 → 6. 测试 → 7. 验证 → 8. 提交
```

## 步骤 0：理解需求

- 列出功能涉及的**输入/输出**（请求、响应、副作用），明确属于哪个模块（identity/challenge/community/platform/weapon/event）。
- 明确是**玩家侧**（`/api/...`）还是**管理侧**（`/api/admin/...`）路由。
- 明确数据归属：**需要持久化 → 先做步骤 2（DB）**；进程级静态参数 → 步骤 3（TOML）；管理员可编辑 → settings 表。

## 步骤 1：定位模块

按 §2 的模块表找到归属。参考现有最相似的 handler/service 作为模板（例如新赛事功能参考 `modules/event/common/`，新 AWD 功能参考 `modules/event/awd_team/`）。

## 步骤 2：数据模型（如需要）

严格按 [DATABASE.md](./DATABASE.md)：

1. `mise run db:migration:new <feature-name>` 生成时间戳迁移文件
2. 写幂等 SQL（`IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS`），**补中文 `COMMENT ON TABLE/COLUMN`**（注释是硬性要求，管理员后台依赖它）
3. `mise run db:migration:merge` 重新生成 merged.sql
4. 应用到开发库（见 DATABASE.md 第 3 步）
5. `mise run db:gen` 重新生成 Rust 实体 + TS 类型（前提：sea-orm-cli 为 1.1.20）

> 新表才需要建 entity；新列会自动进入已生成实体。**改完 Schema 后必须 `db:gen`，否则编译或运行时三处不一致。**

## 步骤 3：配置（如需要）

| 类型 | 放哪 | 怎么读 |
|------|------|--------|
| 进程级静态 | `core/config.rs` 对应 struct + development.toml | `ctx.config.xxx` |
| 管理员可改 | settings 表（seed_default_settings 播种） | `get_setting(&db, key)` |

新增静态配置步骤：
1. 在 `config.rs` 找到对应 Toml struct（如 `ChallengeToml`），加字段：`#[serde(default = "default_fn")] xxx: T`（注意 `Default` impl 同步）
2. `AppConfig::from_file` 映射到 `ChallengeConfig` 等
3. 非敏感则加入 `log_source_summary()`；敏感用 `Secret` 包装（**严禁**日志打印明文）
4. development.toml 填入示例值 + 中文注释

## 步骤 4：后端实现

按分层顺序写（`domain → infrastructure → application → api → 路由`）：

1. **domain/**（可选）：纯逻辑 + `#[cfg(test)]` 单元测试（如积分公式、状态机）
2. **infrastructure/** 或 **repo/**：持久化（SeaORM `Entity::find/insert/update`）或外部适配器
3. **application/**：用例编排。需要 DB/Docker → 从 `ReqCtx` / `EventContext` 取
4. **api/**：handler。签名模式：
   ```rust
   #[get("/api/xxx")]                      // 或 post/delete/scope
   pub async fn my_handler(
       user: UserJwtGuard,                 // 或 SuperAdminJwtGuard
       ctx: ReqCtx,                        // 需要配置/DB/Docker 时
       path: Path<Uuid>,                   // 路径参数
       body: web::Json<MyRequest>,         // 请求体
   ) -> UniResult<MyDto> { ... }
   ```
   - 错误返回 `Err(AppError::NotFound(...))` / `BadRequest` / `Forbidden` 等
   - 业务操作记日志：`ctx.log.add_log(...)`；敏感审计用 `ctx.audit`
   - DTO 放 `api/dto/` 或模块内 `dto.rs`，用 `serde` 派生
5. **路由注册**：模块内 `configure_player_routes`（或 admin/internal）→ 在 `bootstrap/routes.rs` 的 `configure_all_routes` 挂上（**唯一聚合点，漏了就 404**）

### 需要 ReqCtx 配置时

```rust
// 从 ReqCtx 拿配置
let max = ctx.config.challenge.instance_max_per_user.parse::<u64>().unwrap_or(2);
```

### 需要事件广播时

```rust
// ReqCtx 不含 publisher；从 AppState 获取（或经 AwdDependencies 注入）
let state = web::Data::<AppState>::from_request(&req, &mut payload);
state.publisher.publish(RealtimeEvent::new(entity_id, "score.changed", json!({...})));
```

`EventPublisher` trait 见 `infrastructure/realtime/`；测试用 `RecordingEventPublisher`。

## 步骤 5：前端（apps/web）

> 新增**数据页面**前必读 [DATA-FETCHING.md](./DATA-FETCHING.md)：缓存分级、keepPreviousData、queryKey 失效等硬性规则。

1. `src/entity/*.ts` 由 `db:gen` 生成，新表自动有类型；**不要手改**；改 Schema 后同步页面字段并用 `pnpm exec tsc --noEmit` 校验
2. 页面组件：参考现有相似页面；数据请求用 TanStack Query（`useQuery`），路由用 TanStack Router
3. 数据页面性能基线（详见 DATA-FETCHING.md）：用 useQuery、低频数据覆盖 `staleTime`、列表加 `keepPreviousData`、mutation 成功后 invalidate 对应 key、实时数据用 `refetchInterval`/`useAwdEventStream`
4. 管理端页面若有 API 权限要求，使用 admin 守卫
5. 前端类型与后端 DTO 不一致时，以后端为准（必要时同步改 TS 接口）

## 步骤 6：测试（必做）

按 [TESTING.md](./TESTING.md) 的规范：

- **纯逻辑**（积分、状态、加解密、校验）→ domain 内单元测试，`cargo test -p floatctf <name>` 可单跑
- **无外部依赖的用例** → 直接在源码文件 `#[cfg(test)] mod tests`
- **HTTP 契约** → `apps/api/tests/` 集成测试
- **禁用**：读真实环境变量（配置来自 TOML）、依赖真实 DB/Docker 的用例进 `cargo test` 默认路径（需要单独标记/跳过）

每个新功能的**最小测试清单**：
- [ ] domain 纯逻辑（如有）单元测试，含边界值
- [ ] 配置解析测试（如新增 TOML 字段）：无环境变量也能加载
- [ ] Secret 脱敏测试（如新增敏感字段）：Debug 输出不含明文
- [ ] 权限分支测试（未登录/普通用户/管理员）

## 步骤 7：验证

```bash
cargo fmt --all
cargo check -p floatctf            # 编译零错误
cargo test -p floatctf             # 单元测试通过
# 功能冒烟（可选）：
mise run infra:up                  # 已启动可跳过
mise run dev:api                   # 起服务，curl 验证新端点
```

## 步骤 8：提交

- **中文 message**，Conventional Commits 前缀（feat/fix/chore/docs/refactor）
- 按角度分批次：功能代码与格式化分开提交、与文档分开提交（参考 `git log` 既有风格）
- 提交前 `git status` 检查没有误入文件（锁文件、生成物、敏感配置不入库）

## 检查清单（提交前过一遍）

- [ ] `entity/` 与 Schema 一致（改过 Schema 一定跑过 `db:gen`）
- [ ] 无新增环境变量读取
- [ ] 敏感值走 `Secret`，日志无明文
- [ ] 路由已注册且前缀正确（/api 玩家侧 vs /api/admin 管理侧）
- [ ] 新配置项有默认值与文档（development.toml 示例）
- [ ] 前端数据页面遵循 DATA-FETCHING.md（useQuery + staleTime 分级 + keepPreviousData + invalidate）
- [ ] 注释齐全：SQL 中文 COMMENT、Rust doc comment
- [ ] fmt + check + 相关测试全绿
