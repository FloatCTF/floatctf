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

```toml
name = "Test"
version = "1.0.0"

author = "your_email@example.com"
category = "web"
description = "Challenge description"

# Optional
# safe_name = "test"

# Optional
# attachment = "attachment/src.zip"

[flag]
type = "dynamic"

[docker]
port = 80

[docker.recommended_resources]
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

- `schema_version` / `image_tag` / `image_ref`
- `[flag] env_var`、`value = ""`（空串不再表示 dynamic）
- `port = "80/tcp"` 字符串（v1 一律整数、隐式 TCP）
- 容器运行时字段：`container_id` / `host_port` / `network` / `privileged` / `cap_add` 等

### 规则摘要

| 项 | 规则 |
|----|------|
| `version` | SemVer，禁止 build metadata（`+`）；同一 safe_name+version = 同一 package 内容 |
| `safe_name` | 可选；缺省由 name 派生（ASCII 小写/空白转 `-`/折叠）；无法派生时显式提供 |
| 幂等导入 | 同 `safe_name`+`version`+相同 `package_digest` → 返回已有 Revision |
| 版本冲突 | 同 version 不同 package → `CHALLENGE_VERSION_CONFLICT` |
| flag | explicit `dynamic` / `static`；dynamic 禁止 value；static 必须 value |
| dynamic flag | 平台运行时生成，注入固定 `FLAG` env；entrypoint 写 `/flag` 后同 shell `unset` |
| image | 平台生成：`<registry.image_prefix>/challenges/<safe_name>:<version>` |
| build context | 仅 `src/` |
| 附件 | optional；必须在 `attachment/` 下；随 Revision 不可变；sha256 记录 |

## 导入 API

`POST /api/admin/challenges/import`  
multipart 字段：`package_zip`

流程：safe extract → validate → identity/revision(building) → fcmc build(/push) → pin digest → ready。

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
| 镜像 namespace | `challenges/<safe_name>:<version>` | `gameboxes/<safe_name>:<version>` |
| 共享 | name / optional safe_name / version / revision / fcmc image pipeline / Registry / RepoDigest | 同左 |

## 示例

见 `crates/fcmc/tests/fixtures/challenge/` 与 fcmc scaffold（`fcmc gen -n xxx`）。
