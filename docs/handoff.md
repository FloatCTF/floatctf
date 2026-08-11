# FloatCTF 源码接手教程

> **基线**：分支 `awd` · HEAD `75110ab6f83dac74c8e15a496d6689fb257aefba`（2026-08-11）  
> **性质**：基于当前源码的教学材料，不是 changelog / 审计报告。  
> **读者**：会 Rust / Web / SQL / Docker，第一次系统读 FloatCTF。

---

## 0. 如何使用这份教程

| 时间 | 读什么 |
|------|--------|
| 15 分钟 | §1 全景 + 文末「一张图」 |
| 1 小时 | §53 第一轮 10 文件 |
| 半天 | §1–§11 + §15 五条调用链 |
| 动手改功能 | §16 Where Do I Modify X? + §22 Decision Tree |

**原则**：

1. 先整体，再局部。不要一上来搜 200 个文件。
2. 每个概念先问 **WHY DOES THIS EXIST?**
3. 结论以 **当前 HEAD 源码** 为准；若与旧 README/文档冲突，以源码为准并在文中标注。
4. 改功能先定位 **Family / Purpose / Participant** 哪一层，再动代码。

**本仓库命令速记**（详见 `AGENTS.md`）：

```bash
mise run dev:api / dev:web / dev
mise run db:migration:new|apply|gen|merge
cargo check -p floatctf
cd apps/web && pnpm exec tsc --noEmit
```

开发入口：API `localhost:9090` · Web `localhost:3000` · Nginx `localhost:7780`。

---

## 1. 15 分钟看懂 FloatCTF

### 一句话

FloatCTF 是 **Jeopardy + AWD 双赛制** 的 CTF 实训/竞赛平台：Rust（Actix + SeaORM + PostgreSQL + Docker + RustFS）后端 + React 前端，monorepo。

### 心智模型（先背这张）

```text
                         events  ← 一切比赛/练习的根
                    family × purpose × participant_mode
                         │
              ┌──────────┴──────────┐
              │                     │
         family=jeopardy       family=awd
              │                     │
      Jeopardy Engine          AWD Engine
   Purpose×Participant      awd_events 扩展行
              │                     │
   Challenge → Instance        GameBox → Round
        → Solve → Score        → Judge/Flag/Network
              │                     │
              └──────────┬──────────┘
                         │
              PostgreSQL + Docker + Scheduler
```

### 三个正交维度（替代旧 EventType）

| 维度 | 回答的问题 | 取值 |
|------|------------|------|
| **EventFamily** | 用哪个引擎？ | `jeopardy` / `awd` |
| **EventPurpose** | 练习还是正式赛？ | `practice` / `competition` |
| **ParticipantMode** | 谁是参赛主体？ | `individual` / `team` |

合法组合（Rust `EventMode::validate` ≡ DB `events_mode_combination_check`）：

1. Jeopardy + Practice + Individual（**系统托管**，非管理员创建）
2. Jeopardy + Competition + Individual
3. Jeopardy + Competition + Team
4. Awd + Competition + Team

**没有** `EventType::JeopardySingle` 了。Individual ≠ 旧 Single 类型名；引擎里叫 Individual。

### 两个引擎，不是一个 Registry

- **Jeopardy**：`modules/event/jeopardy` —— 统一 application 用例 + `JeopardyPolicy`
- **AWD**：`modules/event/awd` —— 独立 service/repo/scheduler/network
- **Common**：`modules/event/common` —— 报名、战队、公告、管理 CRUD、EventMode

> 已删除：`EventModuleRegistry`、三套 `Jeopardy*Services`、`modes/practice|single|team`。

### Practice 是什么

- **不是**「无 event 的题库旁路」
- **是** 一行系统 `events`：`system_key = practice:jeopardy`，固定 UUID `…0001`
- 题库 `/api/instances`、无 `event_id` 的 submit/launch 都解析到这行
- 管理员 **不能** 创建/PATCH/删除它（`system_key` 保护）

### 数据主链（各记一条）

**Jeopardy**：`challenges` →（竞赛）`jeopardy_event_challenges` → `challenge_instances` → `jeopardy_challenge_solves`

**AWD**：`events` + `awd_events` → `awd_event_gameboxes` → `awd_gamebox_instances` → `awd_rounds` → judge/flag/score

### 你 15 分钟后应能回答

- Family 选引擎；Purpose/Participant 在引擎内改规则，不另起引擎
- Practice 是系统 Event，不是 EventType
- AWD 有 `awd_events` 扩展表；Jeopardy **没有** `jeopardy_events` 表

---

## 2. Repository 地图

```text
floatctf/
├── apps/api/          # 主后端 crate floatctf
│   ├── src/
│   │   ├── main.rs            # 入口 → bootstrap::run
│   │   ├── bootstrap/         # 配置、状态、路由、调度器接线
│   │   ├── core/              # AppConfig、JWT、system_ids
│   │   ├── entity/            # ★ SeaORM 生成，禁止手改
│   │   ├── infrastructure/    # DB/Docker/S3/realtime/settings
│   │   ├── modules/           # 业务域
│   │   │   ├── event/{common,jeopardy,awd}
│   │   │   ├── challenge/ identity/ platform/ community/ weapon/
│   │   ├── scheduler/         # 任务引擎 + 平台 handlers
│   │   └── sql/migrations/    # 只前进的 SQL
│   └── config/development.toml
├── apps/web/          # React + TanStack Router + Primer
├── crates/
│   ├── fcmc/          # 包构建/校验 + 容器 runtime 抽象
│   ├── awd-flagserver/
│   └── awd-judgeserver/
├── docs/              # 本文档等
├── AGENTS.md          # AI/开发铁律
└── mise.toml          # 任务入口
```

**边界**：

| 包 | 职责 |
|----|------|
| `floatctf` | HTTP、领域、调度、持久化 |
| `fcmc` | Challenge/GameBox 包契约 + Docker runtime trait |
| `awd-flagserver` / `awd-judgeserver` | AWD 侧车进程（Flag 下发 / 裁判执行） |

---

## 3. 程序从哪里启动

### 调用链（真实）

```text
apps/api/src/main.rs
  main()
    → floatctf::bootstrap::run()          # bootstrap/mod.rs
         → AppConfig::from_file(FLOATCTF_CONFIG | development.toml)
         → chdir(work_dir) + init_logging
         → jwt::configure_jwt_secret + AwdCrypto::configure_secret
         → database::connect / docker::connect / storage::connect
         → seed_default_settings
         → realtime hub
         → AWD network/firewall runtime (host | noop)
         → AwdCrypto::from_secret_bytes  # 失败 fail-fast
         → awd::recovery_service::recover_all
         → scheduler::build_task_scheduler + init_and_recover + start_polling
         → AppState::new + AwdDependencies
         → HttpServer::new(App...configure_all_routes).bind.listen
```

### 逐步说明

| 步骤 | 文件 | 创建了什么 | 下一步 |
|------|------|----------|--------|
| 1 | `main.rs` | 进程入口 | `bootstrap::run` |
| 2 | `bootstrap/mod.rs` `run` | 配置、日志、基础设施 | 调度器 |
| 3 | `bootstrap/scheduler.rs` | 注册全部 TaskHandler | 后台 poll |
| 4 | `bootstrap/state.rs` | `AppState` + `AwdDependencies` | HTTP |
| 5 | `bootstrap/routes.rs` | `/api` 路由树 | 请求进入 modules |

**WHY bootstrap 独立**：集成测试与二进制共用同一 `run`/`configure_routes`，避免 main 与 test 双份接线。

**配置铁律**：进程静态配置只读 TOML（`FLOATCTF_CONFIG`）；动态项走 DB `settings` + `get_setting`。禁止新增环境变量业务配置。

Source: `apps/api/src/main.rs` · `apps/api/src/bootstrap/mod.rs`

---

## 4. AppState 专章

Source: `apps/api/src/bootstrap/state.rs`

### AppState 字段

| 字段 | 类别 | 谁创建 | 谁读 | WHY 全局 |
|------|------|--------|------|----------|
| `config` | Config | bootstrap | 全站 | 进程静态 |
| `db` | Database | bootstrap | 几乎所有 handler/service | 连接池 |
| `docker` | Runtime | bootstrap | Jeopardy 实例、清理、部分平台运维 | Docker 客户端 |
| `storage` | S3 | bootstrap | 附件/writeup/上传 | 对象存储 |
| `log` / `audit` | Observability | bootstrap | 审计、日志 | 跨请求 |
| `publisher` | Realtime | bootstrap | AWD SSE/WS、事件推送 | 广播中枢 |
| `scheduler` | Scheduler | bootstrap | 平台运维、AWD 任务注册 | 后台任务 |

### AwdDependencies（与 AppState 分离）

| 字段 | 用途 |
|------|------|
| `crypto` | AWD token/flag 材料加密 |
| `containers` | `fcmc::AwdContainerRuntime` |
| `network` | WireGuard/conntrack host runtime |
| `firewall` | nftables Desired 态 |
| `rate_limiter` | AWD 限流 |
| `audit` / `publisher` | 与 AppState 共享 |

**WHY 分离**：非 AWD handler 不该被迫依赖 WG/防火墙。AWD 路由注入 `web::Data<AwdDependencies>`。

### 刻意不在 AppState 的东西

- **Jeopardy application**：无状态自由函数（`launch_instance` / `submit_flag` / `get_scoreboard`…），按请求构造 `EventContext`
- **旧 EventModuleRegistry / 三套 Services bag**：已删除
- **SeaORM Entity**：生成类型，不是运行时服务

**判断是否放 AppState**：需要进程级单例、跨请求共享、有连接/密钥/后台循环 → 放；纯业务规则/一次请求算完 → 自由函数或短生命周期 Context。

---

## 5. HTTP / Route 注册图

Source: `apps/api/src/bootstrap/routes.rs`

```text
/api
├── session / users…                 # identity
├── weapons
├── announcements / uploads          # platform player
├── discussions/**
├── submit/**                        # Jeopardy flag/writeup  # jeopardy/api
├── challenges / challenge_sets / writeups / solves
├── instances/**                     # Jeopardy practice-oriented instance API
├── events/**                        # common player + AWD player（同 scope 注册！）
│     ├── GET/POST join, teams, scoreboard, trend, writeup…
│     └── {event_id}/awd/**          # AWD player
└── admin/
      ├── platform ops / settings / docker / tasks…
      ├── users / discussions / weapons / challenges…
      ├── events/**                  # common admin CRUD + nested + AWD admin events
      │     └── {event_id}/awd/**    # configure/start/reset/gameboxes…
      └── awd/**                     # 平台级 AWD（网络池、gamebox 库）

# 另：configure internal_routes —— FlagServer/JudgeServer 回调（非 /api 选手路径）
```

### 重点标记

| 类别 | 位置 |
|------|------|
| Common Event | `modules/event/common/api` |
| Jeopardy Instance/Submit | `modules/event/jeopardy/api`（`/api/instances`、`/api/submit`） |
| AWD | `modules/event/awd/api`（挂在 events scope + admin/awd） |
| Practice | 无独立 `/practice` 前缀；缺 `event_id` 时 resolve `practice:jeopardy` |

**Actix 坑**：AWD player/admin **必须** 与 common 的 `scope("/events")` **同组** `.configure`，否则同前缀 scope 会吞路由。注释写在 `awd/api/mod.rs` 与 `routes.rs`。

### 怎么追一条请求

```text
1. rg 路由宏或 path
   rg -n 'launch|#\[post|/instances' apps/api/src/modules
2. Handler 里找 application 调用
3. application 找 domain policy / participant
4. 再找 repo / Entity / Docker
```

示例：

```bash
rg -n "launch_instance" apps/api/src
# api/instances.rs → application/instance.rs → common::launch_instance → container_runtime
```

---

## 6. Event 是整个系统的根

### WHY `events` 存在

几乎所有比赛态数据最终 FK 到 `events.id`：

- 报名 `event_users` / 战队 `event_teams`
- Jeopardy 实例与解题
- AWD 扩展行与轮次/网络（多数经 `awd_events` 再挂）

没有「无 event 的正式竞赛上下文」。Practice 也占一行 event，以便 **同一套表约束与引擎代码**。

### 三维度详解

**Family —— 用哪个引擎？**

- `jeopardy` → Challenge/Instance/Solve 引擎
- `awd` → GameBox/Round/Judge/Network 引擎
- 前端路由：`family === awd` → `/events/awd.$id`，否则 jeopardy

**Purpose —— 练习还是正式赛？**

- `practice`：零官方分、无官方榜、可不挂 `jeopardy_event_challenges`、`end_time` 可为 NULL
- `competition`：报名+进行中门禁、官方分/榜、题目必须挂载、`end_time` 必填

**ParticipantMode —— 谁是主体？**

- `individual`：solve/instance 的 owner 是 user；`team_id` 必须 NULL
- `team`：owner 是 team；提交者仍是 user；`team_id` 必须 NOT NULL

Source: `common/domain/event_mode.rs` · `common/domain/capability.rs` · `common/domain/event.rs`

---

## 7. EventMode

```text
events row
  ├ family
  ├ purpose
  └ participant_mode
         ↓
    EventMode::new → validate
         ↓
  capabilities / JeopardyPolicy / 业务门禁
```

**WHY 不是旧 EventType**：

旧扁平枚举 `jeopardy_practice|single|team|awd_team` 把三个问题焊死成四个标签，扩展（如 AWD Practice）要新枚举值+全链路分支。正交后扩展是 **放开组合 + 引擎内加规则**。

**WHY 不是 class 层次**：Rust 里三个 enum 字段 + validate 比 `JeopardyIndividualCompetitionEvent` 类型爆炸更合适；行为差异用 Policy/match，不是子类。

Source: `EventMode::validate` · migration `events_mode_combination_check` · trigger `events_identity_immutable`

**创建后不可变**：family/purpose/participant_mode（及 system_key）DB trigger 拒绝 UPDATE；管理端 createOnly，PATCH 会剥掉这些字段。

---

## 8. Family 如何选择 Engine

```text
                   Event
                     │
               event.family
              /            \
       Jeopardy              AWD
          │                   │
          ▼                   ▼
 modules/event/jeopardy   modules/event/awd
```

**两种进入方式**：

1. **路由已绑定 Family**  
   - `/api/events/{id}/awd/...` → AWD handler → 再 `event.family == Awd` 校验  
   - `/api/instances`、`/api/submit` → Jeopardy → `JeopardyPolicy::require_jeopardy_family`

2. **Common API 按能力分支**  
   - scoreboard/trend/instances：player_service 调 Jeopardy application（AWD 会 UnsupportedForFamily 或走 AWD 自己的接口）  
   - `EventCapabilities::for_mode` 给前端开关

**没有 `EventEngineFactory` / 插件 Registry**。  
Family 只有 2 个引擎，用 **模块边界 + 路由 + 显式 require_family** 足够。

Source: `jeopardy/domain/policy.rs::require_jeopardy_family` · `awd/api/admin.rs` family 检查 · `capability.rs`

---

## 9. Purpose × Participant 在 Jeopardy 如何组合

```text
family = Jeopardy
       │
       ▼
Jeopardy Engine（一套 application）
       │
       ├── purpose
       │      ├ Practice     → 零分、无官方榜、可不挂载题、并发 1
       │      └ Competition  → 报名+进行中、挂载题、计分、榜
       │
       └── participant_mode
              ├ Individual → SolveSubject::User, team_id NULL
              └ Team       → SolveSubject::Team, 解析 membership
```

| 规则 | 主要决定维 | Source |
|------|------------|--------|
| `requires_event_challenge` | Purpose | `JeopardyPolicy` |
| `contributes_to_official_score` | Purpose | 同上 |
| `supports_official_scoreboard` | Purpose | scoreboard/trend 入口 |
| `max_concurrent_instances` | Purpose×Participant | policy + launch |
| `resolve_participant` | Participant | `application/participant.rs` |
| scoreboard 行维度 | Participant | `assemble_individual` / `assemble_team` |

**不是** Strategy 对象替换；是 **一个 Policy VO + application 内 if/match**。

---

## 10. 为什么没有 Strategy Factory

当前形状：

```text
EventMode → JeopardyPolicy → application match purpose/participant
```

**WHY 合理**：

- 差异是数据规则（并发数、是否计分），不是整段算法可插拔
- 三套 unit struct Services 已证明是命名噪音（无字段、player 甚至每次 `Default`）
- Rust enum exhaustiveness 比 `Box<dyn Trait>` 更安全

**何时才考虑 Strategy**：某条路径出现 **第三种完全不同的计分管线** 且与现有 Competition 分叉 >~200 行且测试独立时——仍优先先扩 Policy 方法。

> Note: `allows_retraining_after_solve` 在 submit 里被读取但结果未分支（练习路径本身允许复开环境）；属轻量残留，不是第二套架构。

---

## 11. Jeopardy Engine 全景

### 真实 tree

```text
modules/event/jeopardy/
├── api/            instances.rs, submit.rs, dto
├── application/    instance, submit, scoreboard, trend, writeup,
│                   participant, context, submission_service, instance_service
├── domain/         policy, solve, scoring, scoreboard, trend, instance
└── infrastructure/ container_runtime, instance_repository, solve_repository
```

**已不存在**：`modes/practice|single|team`、`JeopardyPracticeServices`、`JeopardySingleServices`、`JeopardyTeamServices`、`EventModuleRegistry`。

### Application 模块职责

| 模块 | 职责 | 主 caller | 关键表 |
|------|------|-----------|--------|
| `context` | 请求级 Event+User+DB+Docker；时间/报名门禁 | api / player_service | events, event_users |
| `participant` | Individual/Team → `ResolvedParticipant` | instance/submit/writeup | event_team_members |
| `instance` | launch/destroy/list/solve_status | api instances, player | challenge_instances, j_e_challenges |
| `submit` | Purpose 分流 practice/competition | api submit | solves |
| `submission_service` | 验 flag、事务计分、唯一约束 | submit | jeopardy_challenge_solves, event_users/teams |
| `scoreboard`/`trend` | 官方榜/趋势；Practice 拒绝 | player_service | solves + event_users/teams |
| `writeup` | 本人/本队 writeup URL | player_service | event_writeup |
| `instance_service` | 底层 launch/cleanup Docker | instance, scheduler | challenge_instances |

### JeopardyPolicy（运行时权威）

| 方法 | Purpose/Participant | 调用方 |
|------|---------------------|--------|
| `require_jeopardy_family` | Family 门禁 | 各 use case 入口 |
| `requires_event_challenge` | Purpose | launch |
| `contributes_to_official_score` | Purpose | submit 分流 |
| `supports_official_scoreboard` | Purpose | scoreboard/trend |
| `allows_retraining_after_solve` | Purpose | submit（语义标注） |
| `max_concurrent_instances` | 两者 | launch |
| `is_team` / `is_individual` | Participant | assert / 分支 |

Source: `jeopardy/domain/policy.rs`

### SolveSubject / Participant

```text
Team A = {User1, User2}
User1 提交正确 flag：

  acting user  = User1          # 请求 JWT / submitter 列
  solve owner  = Team A         # jeopardy_challenge_solves.team_id
  score owner  = Team A         # event_teams.points
  instance 作用域 = Team A      # team_id 过滤并发
```

Individual 时 owner = User，`team_id = NULL`（DB trigger 强制）。

**WHY SolveSubject 存在**：把「谁点的按钮」和「分记在谁头上」拆开，避免 Team 模式误用 user 唯一索引。

---

## 12. Jeopardy 完整请求流

### 12.1 Launch instance

```text
POST /api/instances/launch  { challenge_id, event_id? }
  → jeopardy/api/instances.rs::launch_instance
      event_id 缺省 → require_practice_jeopardy_event
      EventContextBuilder { event, user, db, docker }.build()
  → application/instance::launch_instance
      require_jeopardy_family + JeopardyPolicy::from_event
      Competition? should_user_joined + should_ongoing
      resolve_participant
      requires_event_challenge? 查 jeopardy_event_challenges
      数 running 实例 vs max_concurrent_instances
      已有同 challenge running → 复用
      identifier: JP- / JS- / JT-
      common::launch_instance → Docker + INSERT challenge_instances
```

| 模式 | 挂载题 | 并发 | identifier | team_id |
|------|--------|------|------------|---------|
| Practice Individual | 否 | 1 | `JP-{user}-{chal}` | NULL |
| Comp Individual | 是 | 2 | `JS-{event}-{user}-{chal}` | NULL |
| Comp Team | 是 | members×2 (≥2) | `JT-{event}-{team}-{chal}` | Some |

### 12.2 Submit flag

```text
POST /api/submit/… 
  → submit::submit_flag
      !contributes_to_official_score → submit_practice
           验 flag → 无则 insert solve points=0 → destroy instance
      else Competition
           join + ongoing → resolve_participant
           JeopardySubmissionService::submit
             事务：验 flag → 算分 → award user/team points
             → insert_solve（唯一冲突 → 不加分）
             → destroy instance
```

**Team 两人同题只加一次分**：

1. **DB**：部分唯一索引 `jeopardy_challenge_solves_team_uidx (event_id, challenge_id, team_id)`
2. **应用**：`insert_solve` 遇 unique violation 当已解，不再加分  
Source: `submission_service.rs` · migration orthogonalization

### 12.3 Scoreboard / Trend

- 入口检查 `supports_official_scoreboard`（Practice → UnsupportedForPurpose）
- **共享**：load challenges、load solves、拼 cell
- **分叉**：`assemble_individual` 读 `event_users`；`assemble_team` 读 `event_teams`  
不是两个 Engine，是两个 loader。

---

## 13. AWD Engine 全景

### 一句话差异

| | Jeopardy | AWD |
|--|----------|-----|
| 核心循环 | 开题环境 → 交 flag → 解题分 | 部署靶机 → 轮次 → 攻防/裁判 → 分 |
| 逻辑对象 | Challenge Instance | GameBox Instance |
| 时间结构 | 赛事 start/end | Round + Phase |
| 网络 | 端口映射为主 | WG + 子网 + nftables |

### Tree（服务层）

```text
modules/event/awd/
├── api/           player, admin, internal, gamebox_admin, network_admin
├── service/       event, config, deploy, reset, round, judge, flag,
│                  submission, score, network, wireguard, firewall,
│                  precheck, recovery, ban, archive, gamebox*
├── repo/          各表仓储
├── domain/        flag/score/network/gamebox_ext/round_ext…
├── infrastructure/ network, firewall, persistence
├── scheduler/     AWD TaskHandlers
├── system/        wg/conntrack 命令封装
└── crypto.rs
```

### events vs awd_events

```text
events                     # 身份 + 时间窗 + 报名/战队（common）
  │ family=awd, purpose=competition, participant_mode=team
  ▼
awd_events  (UNIQUE event_id)
  │ status/phase/config generation/round 参数/verified…
  ├── awd_rounds
  ├── awd_event_gameboxes → awd_gamebox_instances
  ├── awd_event_networks / team_networks / wireguard_peers
  ├── awd_flag_issues / flag_submissions
  ├── awd_judge_batches / tasks
  └── awd_score_events …
```

**WHY awd_events 存在**：AWD 运行态字段很多（phase、verified_generation、judge 参数…），塞进通用 `events` 会污染 Jeopardy。  
**WHY 没有 jeopardy_events**：Jeopardy 运行态主要落在 instance/solve/event_challenges，通用列已够。

**当前合法 EventMode**：仅 Awd+Competition+Team。  
这是 **产品开放面**，不是「AWD 数学上等于 Team Competition」。未来 AWD Practice = 放宽 validate + AWD 内 Purpose 分支，不必改 Event 根模型。

### Family Guard（真实做法）

- Admin AWD：`create_awd_event` / 操作前 `event.family != Awd` → 错  
- DB：`assert_event_family('awd')` 挂在 `awd_events` 等子表  
- **没有** 单独的 `AwdEventContext` 类型；模式是 `event_repo::find_by_event_id` + service 参数塞 db/network/firewall/crypto

### Configure 两步

```text
1) common create_event(family=awd, participant=team, purpose=competition 强制)
2) POST/PATCH .../awd  config_service
     首次 → INSERT awd_events
     之后 → 仅可配置状态更新，并使 Verified 失效
```

**WHY 两步**：身份在 common；运行参数是 AWD 专属且影响 precheck generation。

### GameBox（单版本模型）

> 当前 **无** `gamebox_revisions` 运行路径（迁移 `20260810235621-single-version-challenge-gamebox` 已折回 identity 列）。

```text
gameboxes（库身份 + 镜像钉扎）
   ↓ admin 选入赛事
awd_event_gameboxes（event 内模板 + host_offset）
   ↓ deploy
awd_gamebox_instances（每队每题逻辑实例 + container 元数据）
   ↓ Docker
container
```

逻辑 Instance ≠ Docker 容器：reset 会换容器/世代，逻辑行可保留。

### Round

```text
start_event → 首轮
scheduler: AwdRoundEnd / GraceEnd / RoundStart
  → round_service 结束当前 → 开下一轮
  → conntrack flush → judge_service.create_batch/dispatch → publish
```

Source: `service/round_service.rs` 文件头流程图 · `TaskKey::AwdRound*`

### Flag / Judge / Score

| 概念 | 表/模块 | 含义 |
|------|---------|------|
| Flag issue | `awd_flag_issues` | 某轮某实例发出的 flag 材料 |
| Flag submission | `awd_flag_submissions` | 攻击提交 |
| Score event | `awd_score_events` | 得分流水（含 reset 惩罚等），幂等键 |
| Judge batch | `awd_judge_batches` | 一轮一次批 |
| Judge task | `awd_judge_tasks` | 每队×模板 |
| Judge server | crate `awd-judgeserver` | 执行脚本，回调 internal API |

Flag 密码学细节见 `awd/crypto.rs` / `domain/flag.rs`——**不要在日志打印 secret**。

### Network

| 对象 | 职责 |
|------|------|
| `awd_network_settings` | 平台全局池 |
| `awd_network_allocations` | 分配记录；FK → **events**（平台级，可早于 awd_events configure）+ family trigger |
| `awd_event_networks` | 赛事 Docker/WG 身份 |
| `awd_team_networks` | 队子网 |
| `awd_wireguard_peers` | 用户 peer |

**WHY allocations → events**：允许在完整 `awd_events` 配置前做平台网络规划；仍用 `assert_event_family('awd')` 防止挂到 Jeopardy。

### Reset 链

```text
Player/Admin reset API
  → reset_service::execute_reset(ResetActor)
      校验 ban/ownership/protection/count
      记 awd_reset_records；超额 → score 惩罚
      gamebox_service 重建容器
      更新 awd_gamebox_instances 状态/元数据
```

---

## 14. 数据库地图

### ER 概览（业务）

```text
users ─┬─ event_users ── events ── event_teams ── event_team_members
       │                  │  │
       │     jeopardy_*   │  └── awd_events ── awd_*
       │                  │
challenge_instances       jeopardy_event_challenges ── challenges
jeopardy_challenge_solves
```

### 谁拥有谁（核心）

| Table | Owner | 随谁删 | 主要读写 |
|-------|-------|--------|----------|
| events | 平台/管理员 | 根 | common admin/player |
| event_users / event_teams | event | CASCADE event | join/team |
| jeopardy_event_challenges | event | CASCADE | 竞赛挂题 |
| challenge_instances | event + participant | CASCADE | launch |
| jeopardy_challenge_solves | event + owner | CASCADE | submit |
| awd_events | event (1:1) | CASCADE | AWD config |
| awd_rounds / gamebox_instances / … | awd_events/event | CASCADE | AWD runtime |
| awd_network_allocations | event（平台） | CASCADE events | 网络池 |
| challenges / gameboxes | 平台库 | 独立 | catalog |

### 关键 Invariants（业务含义）

| 机制 | 作用 |
|------|------|
| `events_mode_combination_check` | 非法三维组合插不进 |
| `events_identity_immutable` | 身份字段（含 system_key 任何变更）不可改 |
| `events_system_key_uidx` | 系统赛事语义键唯一 |
| `assert_event_family` | 子表不能跨引擎挂靠 |
| `assert_jeopardy_participant_ownership` | individual⇔team_id NULL 对称 |
| `event_teams (event_id,id) UNIQUE` + 复合 FK | instance/solve 的 team 必须属于同 event |
| solves 部分唯一索引 | 个人/战队每题一分记录 |
| AWD 多处 UNIQUE | 一轮一 active、一队一实例、flag/score 幂等 |

### SeaORM 工作流

```text
mise run db:migration:new name
  → 写幂等 SQL（无 BEGIN/COMMIT）+ 中文 COMMENT
mise run db:migration:validate
mise run db:migration:apply
mise run db:gen          # 再生 entity/ + web types；禁止手改 entity/
mise run db:migration:merge  # merged.sql 生成物
```

**铁律**：已存在 migration 文件绝不改；计算字段用手工 DTO（如 settings `resolved_value`）。

---

## 15. Docker Runtime

### 抽象位置

| 用途 | 位置 |
|------|------|
| Jeopardy 容器 | `jeopardy/infrastructure/container_runtime.rs` → `fcmc::ContainerRuntime` |
| AWD 容器/网络 | `fcmc::AwdContainerRuntime` / `DockerRuntime`，经 `AwdDependencies.containers` |
| 包构建/校验 | crate `fcmc` CLI + application |

Jeopardy：**单版本** Challenge 行钉扎镜像（`image_repo_digest` > `image_id`），动态 FLAG 环境变量注入。

### Instance vs Container

| | Jeopardy | AWD |
|--|----------|-----|
| 逻辑对象 | `challenge_instances` | `awd_gamebox_instances` |
| Docker | container name ≈ identifier | container_name 字段 |
| 生命周期 | launch → running → destroy/cleanup | deploy/reset/round 影响 |
| Reset | 基本是毁再建实例 | 一等公民 reset_service + 惩罚 |

**不要**用 Docker name 反推 Domain mode；mode 只看 `events` 三字段。

---

## 16. Scheduler

### 启动

`bootstrap::run` → `build_task_scheduler` → `init_and_recover` → `start_polling` 后台任务。

### TaskKey（真实）

| Key | 作用 |
|-----|------|
| `CHECK_PRACTICE_EVENT` | `ensure_practice_jeopardy_event` |
| `CLEAN_INSTANCES` | 清理残留实例 |
| `CLEAN_RUSTFS` | 对象存储回收 |
| `awd.event.auto_precheck` | 自动预检 |
| `awd.event.start` | 计划开赛 |
| `awd.round.start/end/grace_end` | 轮次推进 |
| `awd.archive.cleanup` | 归档清理 |
| `awd.team.unban` | 到期解封 |

平台种子 UUID：`core/system_ids.rs`（与 Practice 事件 UUID 分表编号）。

**WHY 不能随便双开相同任务**：DB 对部分 AWD task_key 有 **active unique**（`group_id + task_key`）；业务用 task_key 幂等。RoundStart 带 `round_number` 防 retry 双轮。

Source: `scheduler/task_key.rs` · `bootstrap/scheduler.rs` · `awd/scheduler`

---

## 17. Frontend（接手要点）

```text
apps/web/src/
├── routes/{admin,service}/
│   └── events/{index, jeopardy.$id/*, awd.$id/*}
├── api/ + api/queries/     # queryKey 必须稳定
├── entity/                 # 生成类型，勿手改
├── components/             # GenericTable, EventStatusBadge…
└── navigation/             # AppLink 协调器
```

- 列表按 `event.family` 进 Jeopardy 或 AWD 详情  
- `participant_mode` 决定 Users vs Teams 子页  
- Practice UI 走题库/instances；**不必**也不应写死 Practice UUID，后端 resolve `practice:jeopardy`  
- Admin 创建：只选 family + participant（Awd 强制 Team）；identity createOnly

### 五页对照

| 前端 | API | Backend |
|------|-----|---------|
| `/admin/events` | admin events CRUD | `common/application/admin_service` |
| `/admin/events/jeopardy.$id` | admin nested + challenges | common api nested |
| `/admin/events/awd.$id` | `/admin/events/{id}/awd` | awd api admin |
| `/service/events/jeopardy.$id` | `/api/events/{id}/…` | player_service + jeopardy app |
| `/service/challenges/$id` | instances/submit 无 event | practice resolve |

---

## 18. 五条端到端调用链

### A. 创建 Jeopardy Individual Competition

```text
Admin UI POST
 → admin_service::create_event
    purpose 强制 Competition
    EventMode::new(Jeopardy, Competition, Individual)
    system_key=None, end_time=Some
 → INSERT events
 →（可选）挂 jeopardy_event_challenges
```

### B. Practice 用户启动 Challenge

```text
POST /api/instances/launch {challenge_id}  # 无 event_id
 → require_practice_jeopardy_event
 → launch_instance：不查 event_challenges，max=1，JP- identifier
 → Docker + challenge_instances(event_id=Practice)
```

### C. Team Competition 提交 Flag

```text
POST submit + event_id
 → submit_flag → JeopardySubmissionService
    subject=Team，team_id from membership
    事务加 event_teams.points + insert_solve
    第二队员再交 → unique → 不加分
```

### D. 创建/配置 AWD

```text
create_event(Awd, Competition, Team)
 → create_awd_event / configure → awd_events 行
 → 选 gameboxes、网络、precheck → Verified
 → start_event → 首轮 + 调度
```

### E. AWD Round 推进（代表 runtime）

```text
scheduled_tasks AwdRoundEnd
 → AwdRoundEndHandler
 → round_service::end_round / grace / start_round
 → judge_service batch+dispatch → judgeserver
 → score_events + websocket publish
```

---

## 19. Where Do I Modify X?

| 想改什么 | 第一入口 | 还要看 | 不要误改 |
|----------|----------|--------|----------|
| Practice 并发实例数 | `JeopardyPolicy::max_concurrent_instances` | `instance::launch_instance` | 硬编码回 launch |
| Competition 个人并发 | 同上 | 测试 policy | 旧 Single 命名文件（已无） |
| Team 并发公式 | 同上 | participant member count | Docker 层 |
| 是否必须挂题 | `requires_event_challenge` | launch | catalog 全局 |
| 练习是否计分 | `contributes_to_official_score` + `submit_practice` | solves 唯一 | AWD score |
| 官方榜算法 | `application/scoreboard.rs` | domain/scoring | FE 假数据 |
| Team 榜维度 | `assemble_team` | event_teams | 复制整份 individual |
| 动态分公式 | `domain/scoring.rs` | submission_service | |
| Flag 校验 | submission_service | instance.flag | |
| 允许重复 solve 行 | **默认不允许**；改唯一索引+应用 | migration | 只改应用 |
| Event 合法组合 | `EventMode::validate` | migration CHECK + FE create | 只改一端 |
| 禁止改 identity | 已有 trigger+admin | FE createOnly | entity 手改 |
| Practice 系统行 | `practice_event.rs` + `system_ids` | scheduler CheckPractice | 管理员 CRUD |
| 报名规则 | `player_service::join_event` | purpose competition | |
| 组队规则 | `require_team_mode` / team_service | | |
| AWD 开赛条件 | `event_service::start_event` | precheck generation | |
| AWD 轮长 | `config_service` | round_service | |
| AWD Reset 惩罚 | `reset_service` | score_service | |
| AWD 裁判并发 | config + `judge_service` | judgeserver | |
| WG 分配 | `wireguard_service` | network runtime | |
| nftables | `firewall_service` + infrastructure/firewall | host 权限 | |
| 镜像钉扎策略 | fcmc + challenge/gamebox 行 | container_runtime | |
| 新 HTTP 路由 | `bootstrap/routes.rs` + module api | | 复制第二套 scope |
| 前端赛事跳转 | `routes/**/events/index` | family | |
| query 缓存 | `api/queries/*` queryKey | DATA-FETCHING.md | 乱改 key |
| 系统 UUID | `core/system_ids.rs` | 迁移/seed | 两处各写各的 |
| 调度新任务 | `TaskKey` + handler + build_task_scheduler | DB unique | 只注册不写 key |
| DB 新列 | migration new → apply → db:gen | 业务 DTO | 手改 entity/ |
| 设置项 | settings 表 + get_setting | 非新环境变量 | |

---

## 20. 不要从这里开始改

| 区域 | 原因 |
|------|------|
| `apps/api/src/entity/**` | 生成物，下次 db:gen 覆盖 |
| `apps/web/src/entity/**`、`routeTree.gen.ts` | 同上/生成 |
| 已 apply 的 `sql/migrations/*` | 只前进 |
| `merged.sql` | 生成物 |
| bollard 直调（业务里） | 应走 fcmc / container_runtime |
| 复制 scoreboard 整文件 | 应扩展 loader |
| 为 Product Rule 只改 SQL 不改 `EventMode`/`JeopardyPolicy` | 双源真相 |
| `.github/workflows/ci.yml`（若非任务） | 常被本地误改 |

---

## 21. 容易混淆的概念

| 对 | 澄清 |
|----|------|
| Event vs awd_events | Event=身份根；awd_events=AWD 运行扩展 1:1 |
| Event vs EventMode | 行 vs 三维校验值对象 |
| User vs Participant | User=账号；Participant=本赛事作用域（人/队） |
| User vs SolveSubject | 点击者 vs 得分归属 |
| Team Member vs Solve Owner | 成员关系 vs solves.team_id |
| Challenge vs Event Challenge | 题库 vs 本赛挂载（分/上架） |
| ChallengeInstance vs Container | DB 行 vs Docker 对象 |
| GameBox vs Instance vs Container | 库定义 / 赛中逻辑实例 / 容器 |
| Purpose vs Family | 练习|竞赛 vs 引擎 |
| Policy vs Strategy | 规则 VO+查询 vs 可替换算法对象（后者基本不用） |
| Application vs Domain | 用例编排 vs 纯规则/值 |
| Repository vs Entity | 查询封装 vs 生成表映射 |

---

## 22. 架构设计为什么这样

| 决策 | 原因 |
|------|------|
| Family = Engine | 运行时模型本质不同（解题 vs 攻防） |
| Purpose ≠ Engine | 练习/竞赛共享表与流程，差在规则 |
| Participant ≠ Engine | 只影响 owner/维度，不换引擎 |
| 无三 Jeopardy Services | 正交后重复代码合并，Policy 统一 |
| AWD 独立目录 | 网络/轮次/裁判体量与 Jeopardy 不共享 |
| 有 awd_events | 扩展列隔离 |
| 无 jeopardy_events | Jeopardy 态不必第二头表 |
| 无硬塞 Strategy | 差异不足以支付 dyn 与 Registry 成本 |
| enum/match | 穷尽匹配 + 简单 |

---

## 23. 未来扩展模拟（只教学，不实现）

### 若增加 AWD Practice

**要动**：`EventMode::validate` + DB CHECK；AWD 内 Purpose 门禁（是否 round/judge）；或许系统 `practice:awd`；FE 入口；capabilities。  
**不动**：Event 根三列模型、Jeopardy 引擎、Family 路由骨架。

### 若增加 Jeopardy Organization 参赛

**要动**：新 `ParticipantMode` 值 + validate + ownership 规则 + scoreboard loader + FE。  
**说明**：Participant 独立维度的价值——不必新 Family。

---

## 24. 推荐阅读顺序

### 第一轮 10 文件（1 小时）

1. `apps/api/src/main.rs` — 入口  
2. `apps/api/src/bootstrap/mod.rs` — 启动全景  
3. `apps/api/src/bootstrap/state.rs` — 全局依赖  
4. `apps/api/src/bootstrap/routes.rs` — HTTP 地图  
5. `modules/event/mod.rs` + `common/domain/event_mode.rs` — 根模型  
6. `common/domain/practice_event.rs` + `core/system_ids.rs` — Practice  
7. `jeopardy/domain/policy.rs` — 规则权威  
8. `jeopardy/application/instance.rs` + `submit.rs` — 主路径  
9. `awd/service/config_service.rs` + `event_service.rs` — AWD 入口  
10. `scheduler/task_key.rs` + `bootstrap/scheduler.rs` — 后台  

### 第二轮（半天）

- Jeopardy：`context` `participant` `submission_service` `scoreboard` `container_runtime`  
- AWD：`round_service` `judge_service` `reset_service` `wireguard_service` `api/mod`  
- DB：`20260811112131-event-domain-orthogonalization.sql` 后半 + `20260811122136-event-ownership…`  
- FE：`routes/admin/events/index.tsx` `routes/service/events/index.tsx`  
- 测试：`apps/api/tests/event_domain_invariants.rs`

### 最多 30 文件导航

在上述基础上加：`admin_service` `player_service` `capability` `time_state` `awd/api/admin` `flag_service` `deploy_service` `infrastructure/network/runtime` `firewall/mod` `fcmc` runtime 入口 `apps/web/hooks/useAwdEventStream.ts`。

---

## 25. Debugging

```text
API 500
  → 日志 WORK_DIR/logs/api/（开发常在 app/logs/api/）
  → 对一下路由是否挂对 scope（AWD 吞路由经典坑）
  → handler 返回 AppError / AwdError 文案
  → application anyhow 上下文
  → DB：约束名（assert_event_family / unique / ownership）
  → Docker：docker ps / logs；identifier 是否 JP/JS/JT
```

常用：

```bash
rg -n "符号" apps/api/src
RUST_LOG=info,floatctf=debug  # 以 development.toml logging.filter 为准
mise run db:migration:status
psql postgres://postgres:postgres@127.0.0.1:5432/floatctf_db
```

**注意**：`mise run dev:api` **不是** watch；改后端需重启 9090 进程。

---

## 26. 新需求 Decision Tree

```text
需求
 │
 ├─ 跨解题/攻防两套完全不同运行时？ ──是──► Family / 新引擎模块
 │
 ├─ 练习 vs 正式赛规则差异？ ──是──► Purpose + Policy/AWD 门禁
 │
 ├─ 个人 vs 队归属/榜维度？ ──是──► ParticipantMode + participant/scoreboard
 │
 ├─ 仅 Jeopardy 流程细节？ ──► jeopardy/application 对应用例
 ├─ 仅 AWD 流程细节？ ──► awd/service 对应服务
 └─ 平台账号/题库/社区？ ──► modules/{identity,challenge,community,…}
```

---

## 27. 项目特定 Code Smell（看到要警觉）

- 重新引入 `JeopardySingle*` / `EventType` 作为业务真相  
- 空的 unit struct「Services」+ Registry 分发  
- Policy 写了但 launch 仍写死魔法数  
- Individual/Team scoreboard 整文件复制漂移  
- 用 Docker identifier 前缀当权限/mode 判断  
- Practice 走 `event_id IS NULL` 旁路（已废除）  
- 手改 `entity/` 或改历史 migration  
- AWD 路由另起 `/events` scope 导致 404  
- Admin 可编辑 family/participant  
- 前端假数据糊弄赛事状态  

---

## 28. Glossary

| 术语 | 含义 |
|------|------|
| Event | `events` 行，比赛/练习根身份 |
| EventMode | 已校验的 family×purpose×participant |
| Family | 引擎选择 |
| Purpose | 练习/竞赛 |
| ParticipantMode | 个人/战队主体 |
| Practice | 系统 Jeopardy 练习 Event |
| Competition | 正式赛 Purpose |
| Challenge | 题库身份 |
| Event Challenge | 赛题挂载 `jeopardy_event_challenges` |
| Challenge Instance | Jeopardy 运行实例行 |
| Solve | `jeopardy_challenge_solves` |
| SolveSubject | User/Team 归属枚举 |
| GameBox | AWD 靶机库身份 |
| GameBoxInstance | 赛中逻辑实例 |
| Round | AWD 轮次 |
| Judge batch/task | 裁判批/单任务 |
| Flag issue/submission | 发 flag / 交 flag |
| Score event | AWD 得分流水 |
| EventContext | Jeopardy 请求上下文 |
| JeopardyPolicy | Jeopardy 规则 VO |
| system_key | 系统对象语义键 |
| TaskKey | 调度任务稳定标识 |
| AwdDependencies | AWD 专用 DI |
| fcmc | 包与容器 runtime crate |

---

## 29. Self Test（20 题 + 答案）

1. **main 从哪启动？** → `main.rs` → `bootstrap::run`  
2. **AppState 有 scheduler 吗？** → 有；AWD 网络在 `AwdDependencies`  
3. **如何找 launch handler？** → `rg launch_instance` → api/instances  
4. **Event 是什么？** → 比赛根行，三维身份  
5. **EventMode 是什么？** → 三维校验 VO  
6. **Family 怎么选引擎？** → 路由模块 + require_family，无 Factory  
7. **Purpose 影响？** → 计分/挂题/榜/并发等 Policy  
8. **Participant 影响？** → owner、team_id、榜维度  
9. **Practice 为何不是 EventType？** → 是 Purpose+系统行  
10. **launch 步骤？** → 见 §12.1  
11. **submit 步骤？** → 见 §12.2  
12. **Team 提交 owner？** → team；user 是 submitter  
13. **为何 awd_events？** → AWD 运行扩展  
14. **GameBox 与 Container？** → 逻辑实例 vs Docker  
15. **Round 谁推？** → scheduler + round_service  
16. **Judge 怎么跑？** → batch → judgeserver → callback  
17. **网络配置在哪？** → platform settings + event_network + wireguard services  
18. **Scheduler 启动？** → bootstrap 内 build + poll  
19. **entity 从哪来？** → migration apply + `mise run db:gen`  
20. **改功能从哪？** → §19 表 + Decision Tree  

### 进阶

- AWD handler 收到 Jeopardy event_id？→ admin/service family 检查 + DB family trigger  
- Practice launch 为何不查 event_challenges？→ `requires_event_challenge == false`  
- 为何不能 PATCH participant_mode？→ identity immutable trigger + 产品不变量  
- 两人同队同题两交？→ 一解一分；unique + 应用忽略重复  

---

## 30. 一张图记住 FloatCTF

```text
                    ┌──────── AppConfig (TOML) ────────┐
                    │  bootstrap → AppState + AwdDeps  │
                    │  scheduler poll · HttpServer /api │
                    └────────────────┬─────────────────┘
                                     │
                              events (root)
                     family × purpose × participant_mode
                          /                    \
                   Jeopardy                     AWD
              JeopardyPolicy               awd_events
           instance/submit/board         round/judge/flag
           challenge_instances           gamebox_instances
           jeopardy_challenge_solves     network/wg/nftables
                          \                    /
                           PostgreSQL + Docker
                        system_ids · practice:jeopardy
```

---

## 31. 文档元信息

| 项 | 值 |
|----|----|
| 生成目标 | 源码接手教程（非审计） |
| HEAD | `75110ab6f83dac74c8e15a496d6689fb257aefba` |
| 分支 | `awd` |
| 输出路径 | `docs/handoff.md` |
| 代码修改 | 无（本文件为文档） |

**近期相关提交（便于对照历史）**：

- `abe753d` EventType → Family×Purpose×ParticipantMode  
- `16c46b8`/`da6f40d` ownership / system invariants + 测试  
- `3e0214a` Jeopardy Application 正交化  
- `d36a1e4`/`31da319` Practice 固定 UUID + system_ids  

若后续 HEAD 前进，以源码为准更新本教程中的路径与符号。
