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

公共字段与 Challenge 完全一致（source of truth = floatctf-content 的
`scripts/content.py`）。`[gamebox]` / `[judge]` / `[awdp]` 是 FCMC 提供的
AWD 运行时扩展，**不是**官方必需字段——官方 canonical GameBox 可以完全没有
`[gamebox]`，此时 metadata 依然合法，只有运行时操作才需要它。

```toml
name = "TTT1"
version = "1.0.0"
author = "your_email"
category = "web"
difficulty = "easy"
tags = ["web"]
description = "hello floatctf"
# optional：缺省由 **目录 ID**（content id）派生
# safe_name = "ttt1"

[docker]                          # 公共段；port 可省略
port = 80

[docker.recommended_resources]    # 资源唯一来源（partial：出现即须 > 0）
cpu_millis = 1000
memory_bytes = 536870912
pids_limit = 100

[gamebox]                         # 运行时扩展（可选）
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

[awdp]                            # 可选；出现则内部字段全部必填
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/html"
```

### 禁止字段

- 未知的**顶层**扩展字段会被忽略（与 floatctf-content 一致）；
  但 `[gamebox]` / `[judge]` / `[awdp]` 内的未知字段会被拒绝
- `[gamebox.recommended_resources]`（资源唯一来源是 `[docker.recommended_resources]`）
- 计分：`break_points` / `fix_points` / `down_points` / `first_bonus` / `loss_points`（仅赛事 EventGameBox）
- `services` / 网络拓扑 / privileged / secrets

### 规则摘要

| 项 | 规则 |
|----|------|
| 公共字段 | `name` / `version` / `author` / `category` / `difficulty` / `tags` / `description` 与 Challenge 完全一致 |
| `version` | 严格 `x.y.z`；拒绝 prerelease / build metadata |
| `safe_name` | 可选；缺省由 **content id（目录名）** 派生；无法派生时必须显式给出 |
| container 判定 | 只看 `src/Dockerfile` 是否存在（不看 `[docker]`） |
| image | 平台生成：`<registry.image_prefix>/<safe_name>:gamebox-v<version>`（与 Challenge 共用 `fcmc::content_image_ref(ArtifactKind)`） |
| build context | 仅 `src/` |
| judge / awdp | 导入时读入并自包含存储；绝不进入镜像 |

### AWDP 运行时契约（镜像侧，必须在 `src/` 里实现）

平台启动 GameBox 实例时注入以下环境变量，镜像的 `entrypoint.sh` / 应用必须按契约消费：

| 环境变量 | 用途 | 注入方 |
|----------|------|--------|
| `GAMEBOX_USERNAME` / `GAMEBOX_USERPASS` | 容器内登录凭据（entrypoint 建用户、设密码后 `unset` 再 `exec`） | `awd` / `awdp` runtime |
| `FLAG` | 本实例 flag；**Judge / Break 流程通过 HTTP 读取它验证实例可被攻破** | `awdp` runtime |

对应端点约定见 `awdp_practice_judge_settings.flag_path`（默认 `/flag.php`，注释为
"flag curl 验证的端点路径（如 /flag.php；GameBox 按 FLAG env 返回 flag）"）。
因此可被 AWDP 判定的 GameBox 至少需要：

```text
src/flag.php        # 输出 getenv('FLAG')（真实题目应把 flag 藏在漏洞之后）
src/entrypoint.sh   # GAMEBOX_USERNAME/GAMEBOX_USERPASS 契约 + 启 sshd 后 exec
src/Dockerfile      # COPY 上述文件，并把 php 文件 chmod 0644（Apache 以 www-data 运行）
```

`examples/test-g` 与 `fcmc gen --format gamebox` 生成的脚手架都已包含 `src/flag.php`。

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

见 `crates/fcmc/tests/fixtures/gameboxes/hello-floatctf/` 与 `crates/fcmc/tests/fixtures/content_contract/gameboxes/comment/`（floatctf-content 官方 fixture 副本，无 `[gamebox]` 段）。
