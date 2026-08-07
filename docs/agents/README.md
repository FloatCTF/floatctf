# AI 工作手册（docs/agents/）

> 这套文档帮助 AI 理解 FloatCTF 项目结构、按规范开发新功能与修复 bug。

## 文档索引

| 文档 | 内容 | 什么时候读 |
|------|------|-----------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | 仓库布局、模块分层、关键类型（AppConfig/ReqCtx/AppState）、数据流、配置体系、常见陷阱 | **任何任务前必读** |
| [ADD-FEATURE.md](./ADD-FEATURE.md) | 新功能从需求到上线的完整步骤与测试清单 | 开发新功能 |
| [FIX-BUG.md](./FIX-BUG.md) | bug 复现、定位、根因分析（三处不一致等陷阱）、最小修复、回归 | 修 bug |
| [DATABASE.md](./DATABASE.md) | Schema 变更流程：迁移 → 合并 → 应用 → 实体/类型再生成 → 验证 | 涉及数据库 |
| [TESTING.md](./TESTING.md) | 测试层级、写测试规范与禁忌、命令 | 写/跑测试 |
| [DATA-FETCHING.md](./DATA-FETCHING.md) | 前端数据请求规范：缓存分级、keepPreviousData、queryKey 失效、实时数据 | **新增/修改前端数据页面** |

## 阅读顺序建议

```
新任务
  └─> ARCHITECTURE.md（5 分钟建立心智模型）
        ├─ 涉及数据库  → DATABASE.md
        ├─ 写测试      → TESTING.md
        ├─ 前端数据页面 → DATA-FETCHING.md
        ├─ 新功能      → ADD-FEATURE.md
        └─ 修 bug      → FIX-BUG.md
```

## 三条铁律（全项目通用）

1. **配置只从 TOML / settings 表取**，禁止新增环境变量读取
2. **实体是生成的**，Schema 变更必须走迁移 + `mise run db:gen`（sea-orm-cli 1.1.20）
3. **提交前**：`cargo fmt` + `cargo check` + 相关测试全绿；中文提交 message，按角度分批
