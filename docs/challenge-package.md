# Challenge Package Format (v1)

可移植 Challenge 包格式与平台导入管线说明。

## 目录结构

```
<challenge>/
├── meta.toml
├── src/                 # 唯一 Docker build context
│   ├── Dockerfile
│   ├── entrypoint.sh
│   ├── flag
│   └── index.php
└── attachment/          # 可选；绝不进入 Docker build context
    └── src.zip
```

- `src/` 由 fcmc 生成/维护，是 **唯一** Docker Build Context（attachment/、meta.toml 不进镜像）。
- `attachment/` 属于 Revision 元数据（随版本不可变），用于下载附件。

## meta.toml

公共字段与 GameBox 完全一致，content id = 目录名；source of truth 是
[floatctf-content](https://github.com/FloatCTF/floatctf-content) 的
`scripts/content.py`。

```toml
name = "Test"
version = "1.0.0"

author = "your_email@example.com"
category = "web"
difficulty = "easy"
tags = ["web"]
description = "Challenge description"

# Optional：缺省由 **目录 ID**（content id）派生，派生失败时必须显式提供
# safe_name = "test"

# Optional
# attachment = "attachment/src.zip"

[flag]                       # 可选；缺失 = 运行时不注入 FLAG
type = "dynamic"
# type = "static"
# value = "flag{}"

[docker]                     # 可选；port 也可省略
port = 80

[docker.recommended_resources]   # 可选，partial：出现即须 > 0
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
```

静态 Flag：

```toml
[flag]
type = "static"
value = "flag{example}"
```

### 禁止字段

- 未知的**顶层**扩展字段会被忽略（与 floatctf-content 一致，不报错）；
  但 FCMC 拥有的严格段（`[flag]` / `[gamebox]` / `[judge]` / `[awdp]`）出现未知字段会报错
- `[flag] env_var`、`value = ""`（空串不再表示 dynamic）
- `port = "80/tcp"` 字符串与 `port = 0`（一律整数、隐式 TCP）
- 容器运行时字段：`container_id` / `host_port` / `network` / `privileged` / `cap_add` 等

### 规则摘要

| 项 | 规则 |
|----|------|
| `name` / `author` / `category` / `description` | 必填，strip 后非空 |
| `difficulty` | 必填：`unknown` / `beginner` / `easy` / `medium` / `hard` / `expert` |
| `tags` | 必填：字符串数组，允许 `[]`，每项 strip 后非空 |
| `version` | 严格 `x.y.z`（`^\d+\.\d+\.\d+$`）；拒绝 prerelease / build metadata |
| `safe_name` | 可选；缺省由 **content id（目录名）** 派生；无法派生时必须显式提供 |
| 幂等导入 | 同 `safe_name`+`version`+相同 `package_digest` → 幂等 |
| 版本冲突 | 只允许严格递增；等于/更低 → `VERSION_GATE_REJECTED` |
| flag | 可选；`dynamic` 禁止 value；`static` 必须 value |
| dynamic flag | 平台运行时生成，注入固定 `FLAG` env；entrypoint 写 `/flag` 后同 shell `unset` |
| container 判定 | 只看 `src/Dockerfile` 是否存在（不看 `[docker]`） |
| image | 平台生成：`<registry.image_prefix>/<safe_name>:challenge-v<version>` |
| build context | 仅 `src/` |
| 附件 | optional；必须在 `attachment/` 下；sha256 记录 |

## 导入 API

`POST /api/admin/challenges/import`  
multipart 字段：`package_zip`

流程（容器内容）：safe extract → validate → upsert identity(building) → fcmc build(/push) → tag 规范 ref → pin → ready。

流程（static / attachment-only）：safe extract → validate → upsert identity → mirror 到
`CHALLENGES_DIR/<safe_name>` → **ready（不构建）**。

### static / attachment-only 题目（无 `src/Dockerfile`）

floatctf-content 的 28 道官方题里有 16 道没有 `src/Dockerfile`（纯附件题 / 静态
flag 题）。它们**同样可以直接从 admin import**：

| 项 | 容器内容（有 `src/Dockerfile`） | static / attachment-only（无 Dockerfile） |
|----|--------------------------------|------------------------------------------|
| 包布局要求 | `meta.toml` + `src/Dockerfile` | 只需 `meta.toml`（`attachment/` 可选） |
| `image_ref` / `image_id` | 规范 ref + 本地镜像 | 均为 NULL（没有镜像） |
| `container_port` | `[docker].port`（可为空） | 恒为 NULL（**忽略** `[docker].port`） |
| `build_status` | building → ready / failed | 直接 ready（无构建步骤） |
| flag | `dynamic`（注入 `FLAG`）或 `static` | 只能是 `static`（无容器可注入 `FLAG`） |
| 玩家侧 | Launch 起容器 | Start 创建无容器实例，flag 由服务端比对 |
| `[flag]` 缺失 | 可以导入 | 可以导入，但**不可开局**（开局需要 flag_type） |

- 判定规则与 floatctf-content 一致：**只看 `src/Dockerfile` 是否存在**，
  不看 `[docker]` 段（`static_with_docker` 这类"声明了 port 但没有 Dockerfile"
  的包按 static 处理）。
- 附件下载：`GET /static/challenges/<safe_name>/attachment/<path>`。Caddy 只暴露
  `attachment/` 子树，因此 `meta.toml`（含 static flag 明文）与 `src/` **不会**被
  静态服务。
- 校验包布局的接口分层：`require_meta_toml`（两者）与 `require_src_dockerfile`
  （GameBox 与"是否容器"判定）。
- GameBox **仍然**要求 `src/Dockerfile`：没有镜像就没有可运行的靶机。

## Runtime pin

`EventChallenge` 钉住 `challenge_revision_id`（加入赛事时取 latest ready）。
Instance 创建 / Reset / Recovery 使用：

1. `image_repo_digest`（`repo@sha256:…`，push 模式）
2. 否则 `image_id`（LocalOnly `sha256:…`）

Ready Revision **不 rebuild**；本地镜像丢失时按 RepoDigest `pull`。禁止用可变 tag 作为 Runtime identity。

## Registry 配置

`apps/api/config/*.toml`：

```toml
[registry]
image_prefix = "floatctf"
push = false              # true = 必须 push 并解析 RepoDigest
build_timeout_secs = 600
# username / password / server_address
```

## Challenge vs GameBox

| | Challenge | GameBox |
|--|-----------|---------|
| Flag | `[flag] type=dynamic\|static` | 无（AWD 平台生成） |
| 附件 | optional `attachment/` | 无 |
| 端口 | 单端口 `[docker].port` + TCP readiness | 多 healthchecks（HTTP/TCP） |
| 用户名 | 无 | `[gamebox].username` |
| Judge | 无 | `[judge].script` |
| image | `<prefix>/<safe_name>:challenge-v<version>` | `<prefix>/<safe_name>:gamebox-v<version>` |
| 共享 | name / optional safe_name / version / revision / fcmc image pipeline / Registry / RepoDigest | 同左 |

## 示例

见 `crates/fcmc/tests/fixtures/challenges/<content-id>/`、`crates/fcmc/tests/fixtures/content_contract/`（floatctf-content 官方 fixture 副本）与 fcmc scaffold（`fcmc gen -n xxx`）。
