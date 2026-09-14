# GameBox Package Format (v1)

可移植 GameBox 包格式与平台导入管线说明。

## 目录结构

```
<gamebox>/
├── meta.toml          # 元数据 + runtime contract
├── src/               # 唯一 Docker build context
│   ├── Dockerfile
│   └── ...
└── judge/             # 可信判题脚本（永不进入镜像）
    └── check.py
```

## meta.toml

```toml
name = "TTT1"
version = "1.0.0"
author = "your_email"
category = "web"
description = "hello floatctf"
# optional
# safe_name = "ttt1"

[gamebox]
username = "floatctf"

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
expected_status = 200

[[gamebox.healthchecks]]
type = "tcp"
port = 22

[judge]
script = "judge/check.py"

[gamebox.recommended_resources]
cpu_millis = 1000
memory_bytes = 536870912
pids_limit = 100
```

### 禁止字段

- `schema_version` / `image_tag` / `image_ref`
- 计分：`break_points` / `fix_points` / `down_points` / `first_bonus` / `loss_points`（仅赛事 EventGameBox）
- `services` / 网络拓扑 / privileged / secrets

### 规则摘要

| 项 | 规则 |
|----|------|
| `version` | SemVer，禁止 build metadata（`+`） |
| `safe_name` | 可选；缺省由 name 派生；无法派生时必须显式给出 |
| image | 平台生成：`<registry.image_prefix>/gameboxes/<safe_name>:<version>`（与 Challenge 共用 `fcmc::build_artifact_image_ref(ArtifactKind)`） |
| build context | 仅 `src/` |
| judge | 导入时读入 Revision，自包含存储 |

## 导入 API

`POST /api/admin/awd/gameboxes/import`  
multipart 字段：`package_zip`

流程：safe extract → validate → create identity/revision(building) → fcmc build(/push) → pin digest → ready。

同 `safe_name`+`version`+相同 `package_digest`：幂等返回已有 Revision。  
同 version 不同 package：`VERSION_CONFLICT`。

## Runtime pin

`AwdEventGameBox` 钉住 `gamebox_revision_id`。  
Deploy / Reset / Recovery 使用：

1. `image_repo_digest`（`repo@sha256:…`，push 模式）
2. 否则 `image_id`（LocalOnly `sha256:…`）

禁止用可变 tag 作为 Ready 运行时身份；Reset 不 rebuild。

## Registry 配置

`apps/api/config/*.toml`：

```toml
[registry]
image_prefix = "floatctf"
push = false              # true = 必须 push 并解析 RepoDigest
build_timeout_secs = 600
# username / password / server_address
```

`push = false` 为显式 LocalOnly 开发模式，不是静默降级。

## 示例包

见 `crates/fcmc/tests/fixtures/gamebox/hello-floatctf/`。
