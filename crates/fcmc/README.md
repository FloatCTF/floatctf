# fcmc — FloatCTF 容器构建与配置工具

fcmc 是 [FloatCTF](https://github.com/FloatCTF/floatctf) 平台的 **Challenge / GameBox
容器镜像构建与配置校验 CLI + 库**。它负责：

- **模板生成**：生成 Challenge（Jeopardy 题）与 GameBox（AWD 攻防题）的包骨架；
- **配置校验**：解析并校验 `meta.toml`（公共字段对齐官方 Content Contract）；
- **镜像构建**：以包内 `src/` 为唯一构建上下文构建 Docker 镜像，支持构建代理；
- **运行时验证**：起一个临时容器，打印访问地址（Challenge）或 Docker IP + SSH 凭据（GameBox），按 Enter 后自动清理；
- **库接口**：`metadata` / `runtime` / `application` 三个层次，供平台 API（`apps/api`）复用同一套解析、构建与镜像逻辑。

> ## Content Contract 的 source of truth
>
> ```text
> https://github.com/FloatCTF/floatctf-content   （scripts/content.py）
> ```
>
> **fcmc 不定义第二套公共 metadata contract。** `name` / `version` / `author` /
> `category` / `difficulty` / `tags` / `description` / `safe_name` / `[flag]` /
> `[docker]` 的规则全部来自该仓库；发生冲突时以 `floatctf-content` 为准
> （先改 Python，再同步 fcmc）。FCMC 只额外拥有 `[gamebox]` / `[judge]` /
> `[awdp]` 这些 AWD 运行时扩展。

## 安装在 crates.io 上发布

```bash
cargo install fcmc          # 从 crates.io 安装最新版
```

> 版本来自 crates.io：https://crates.io/crates/fcmc
> 交互式完整手册（面向 AI / 自动化工具，非常详细）：`fcmc help --agent`

---

## 目录

- [快速开始](#快速开始)
- [命令参考](#命令参考)
- [包目录布局](#包目录布局)
- [meta.toml 契约](#metatoml-契约)
- [Content Contract 对齐](#content-contract-对齐)
- [镜像命名与构建代理](#镜像命名与构建代理)
- [运行时检查](#运行时检查)
- [与平台 API 的边界](#与平台-api-的边界)
- [开发与测试](#开发与测试)

---

## 快速开始

### Challenge（Jeopardy 题）

```bash
# 1. 生成模板
fcmc gen --name easy-web

# 2. 按需修改 meta.toml（difficulty/tags/flag 类型、端口、分类等）与 src/ 下的应用代码
cd easy-web
vim meta.toml

# 3. 校验配置（不需要 Docker）
fcmc check

# 4. 构建镜像（需要 Docker；外网构建可加 --proxy 7890）
fcmc build --proxy 7890

# 5. 运行时验证：起临时容器，打印 http://127.0.0.1:<port>，按 Enter 退出
fcmc check --runtime
```

### GameBox（AWD 攻防题）

```bash
fcmc gen --name easy-awd-web --format gamebox
cd easy-awd-web
fcmc check
fcmc build --format gamebox
fcmc check --runtime   # 打印 Docker IP + SSH 用户/密码，可 SSH 进容器测试
```

### 基础模板（awd-base 源码）

```bash
fcmc gen --name awd-base --format gamebox --template
```

`check` 与 `build` 的包类型识别优先级：

1. 显式 `-f/--format`；
2. 路径位于 `gameboxes/<id>` → GameBox；
3. 路径位于 `challenges/<id>` → Challenge；
4. standalone 包且 `meta.toml` 含 `[gamebox]` 段 → GameBox；
5. 否则 → Challenge。

> 官方 canonical GameBox **可以没有** `[gamebox]` 段；这类 standalone 包需要
> 显式 `-f gamebox`（放进 `gameboxes/<id>` 也能自动识别）。

---

## 命令参考

| 命令 | 用途 | 主要选项 |
|------|------|----------|
| `check` | 校验包配置（+ 可选运行时验证） | `-p/--path`，`-f/--format`，`--runtime` |
| `build` | 构建 Docker 镜像 | `-p/--path`，`-f/--format`，`-t/--tag`，`--proxy [ip:]port` |
| `gen` | 生成包模板 | `-n/--name`（必填），`-o/--output`，`-f/--format`，`-t/--template`，`--safe-name` |
| `help` | 输出使用说明 | `--agent`（完整 AI 手册），或指定命令名 |

- `fcmc help --agent` — 完整手册（命令、选项、Content Contract、布局、镜像命名、代理、常见错误）；
- `fcmc help check` / `help build` / `help gen` — 单命令详解；
- 退出码：0 = 成功，非 0 = 失败（便于脚本判断）。

### check

```text
用法: fcmc check [-p <目录>] [-f challenge|gamebox] [--runtime]
```

检查分两层，报告里分开显示：

- **A. FloatCTF Content Contract**：官方公共字段（`name`/`version`/`author`/
  `category`/`difficulty`/`tags`/`description`）、`safe_name` 解析、`[flag]`/
  `[docker]`。违反 → `metadata contract invalid: …`（ERR）。
- **B. FCMC operational**：附件文件是否存在、`src/Dockerfile` 是否存在
  （container vs static）、`judge`/`awdp` 脚本是否存在、`[gamebox]` 是否齐备。
  违反 → 该操作无法执行（ERR/WARN），**不是** metadata 不合法。

`--runtime`：静态检查通过后连接 Docker 起临时容器做运行时验证（见[运行时检查](#运行时检查)）。

### build

```text
用法: fcmc build [-p <目录>] [-f challenge|gamebox] [-t <tag>] [--proxy <[ip:]port>]
```

- 镜像 tag 缺省使用 **floatctf-content canonical ref**：
  `floatctf/{safe_name}:challenge-v{version}`（challenge）或
  `floatctf/{safe_name}:gamebox-v{version}`（gamebox）；
- `-t/--tag` 是 **build override**（如本地调试用的 `myreg/x:test`），与官方命名明确分离；
- 是否为容器只看 `src/Dockerfile`：缺少时明确报
  `content is static: src/Dockerfile not found`；
- 只把 `src/` 作为构建上下文，`meta.toml` / `attachment/` / `judge/` / `awdp/` 永不进镜像；
- `--proxy`：构建阶段需要外网（apt / curl / git clone）时使用（见[镜像命名与构建代理](#镜像命名与构建代理)）。

### gen

```text
用法: fcmc gen -n <名称> [-o <输出目录>] [-f challenge|gamebox] [-t] [--safe-name <slug>]
```

- `<名称>` 同时是生成的目录名，也就是 **content id**；
- `--safe-name` 显式指定 safe_name；content id 无法派生出合法 slug 时必填，
  否则报 `unable to derive safe_name from content id; provide --safe-name`，
  **不会**生成一个立刻 invalid 的包。

生成物：

| 文件 | Challenge | GameBox |
|------|-----------|---------|
| `meta.toml` | Content Contract | Content Contract + `[gamebox]`/`[judge]`/`[awdp]` |
| `src/Dockerfile` | php:8.2-apache-bookworm | php:8.2-apache + openssh-server |
| `src/entrypoint.sh` | 动态 flag 写入 `/flag` 后 unset 再 exec | `GAMEBOX_USERNAME/USERPASS` 契约 + 启 sshd |
| `src/index.php` | 读 `/flag` 示例 | SSRF curl 示例 |
| `src/flag.php` | — | AWDP 契约：`/flag.php` 按 `FLAG` env 返回 flag |
| `src/flag` | 占位（动态覆盖） | — |
| `attachment/` | 附件目录 + 示例文件 | — |
| `judge/check.py` | — | judge 脚本：HTTP 健康检查（多 IP 批量，不进镜像） |
| `awdp/exploit.py` | — | AWD-P 攻击脚本（多 IP 批量，不进镜像） |

---

## 包目录布局

### Challenge

```text
<package>/
├── meta.toml          # 包清单（必须）
├── src/               # 唯一构建上下文（存在 src/Dockerfile = container content）
│   ├── Dockerfile
│   ├── entrypoint.sh
│   ├── flag           # 动态 flag 占位
│   └── index.php
└── attachment/        # 可选附件（src.zip 等），绝不进镜像
```

### GameBox

```text
<package>/
├── meta.toml
├── src/               # 唯一构建上下文
│   ├── Dockerfile
│   ├── entrypoint.sh
│   ├── index.php
│   └── flag.php       # AWDP 契约：/flag.php 按 FLAG env 返回 flag
├── judge/             # judge 脚本（可选），绝不进镜像
└── awdp/              # AWD-P 攻击脚本（可选），绝不进镜像
```

> 在 floatctf-content 仓库中，包放在 `challenges/<content-id>/` 或
> `gameboxes/<content-id>/`，**目录名就是 content id**。

---

## meta.toml 契约

> **公共字段没有 `deny_unknown_fields`**：与 floatctf-content 一样，无关的顶层
> 扩展字段被忽略，不会因为“多写了一个字段”就判 metadata 不合法。FCMC 自己拥有
> 的严格段是 `[gamebox]` / `[judge]` / `[awdp]` / `[flag]`（出现未知字段报错）。

### 公共字段（Challenge / GameBox 完全一致）

```toml
name = "easy-web"                 # 必填，显示名（可以是中文）
version = "1.0.0"                 # 必填，严格 x.y.z（^\d+\.\d+\.\d+$）
author = "you@example.com"        # 必填
category = "web"                  # 必填，不限制取值
difficulty = "easy"               # 必填：unknown|beginner|easy|medium|hard|expert
tags = ["web"]                    # 必填：字符串数组，允许 []，每项 strip 后非空
description = "Challenge description"  # 必填，非空
# safe_name = "easy-web"          # 可选；缺省由 **content id（目录名）** 派生
# attachment = "attachment/src.zip"    # 可选；Challenge 专属，必须以 attachment/ 开头

[flag]                            # 可选（缺失 = 运行时不注入 FLAG）
type = "dynamic"                  # dynamic | static
# type = "static"
# value = "flag{xxx}"            # static 必填；dynamic 禁止携带 value

[docker]                          # 可选，公共段
port = 80                         # 可选，1..65535（拒绝 "80/tcp" 与 0）

[docker.recommended_resources]    # 可选，partial：出现即须 > 0，不要求三项齐全
cpu_millis = 500                  # 毫核
memory_bytes = 268435456          # 字节（256 MiB）
pids_limit = 100                  # 进程数上限
```

版本规则：只接受 `x.y.z`（`^\d+\.\d+\.\d+$`）。`1.0.0` / `0.0.1` / `12.34.56` /
`01.0.0` 合法；**拒绝** `1.0`、`v1.0.0`、`1.0.0-rc.1`、`1.0.0+build`。

### `[flag]`（可选）

- `type = "dynamic"`：平台在实例创建时生成 flag，注入 `FLAG` 环境变量，入口脚本
  （`entrypoint.sh`）写入 `/flag` 后 `unset FLAG` 再 `exec "$@"` —— 应用进程永远
  拿不到真实 flag 的环境变量；
- `type = "static"`：`value` 必填（非空），flag 直接打进镜像，运行时不再注入；
- 没有 `[flag]`：metadata 合法，运行时检查不注入任何 flag env。

### `[docker]` 与资源建议

- `port` 可选，是运行时端口绑定与 readiness TCP 探针端口；Dockerfile 的 `EXPOSE`
  不作为可信来源；
- 只写 `[docker.recommended_resources]` 而不写 `port` 也合法；
- 资源建议是 **partial**：每个出现的字段必须 > 0，未出现的字段在 normalize 时
  物化默认值 —— Challenge `500 / 268435456 / 100`，GameBox `1000 / 536870912 / 100`；
- 是否为容器**只看 `src/Dockerfile`**，不看 `[docker]` 是否存在。static 内容即使
  写了 `[docker]` 也不会被当作容器。

### GameBox 运行时扩展（可选，不是 Content Contract 必需字段）

```toml
[gamebox]                         # 可选；缺失时 metadata 依然合法
username = "floatctf"             # SSH 登录用户名（FCMC 运行时契约）

[[gamebox.healthchecks]]          # 0..N 条 readiness 探针
type = "http"                     # http | tcp
port = 80
path = "/"
expected_status = 200

[judge]                           # 可选（缺省 WARN）
script = "judge/check.py"         # 必须位于 judge/ 下且真实存在

[awdp]                            # 可选（出现则内部字段全部必填）
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/html"
```

- 缺少 `[gamebox]`：静态 `check` 通过（只给 WARN）；
  `check --runtime` / AWD 部署报明确的 operational 错误
  `GameBox runtime metadata [gamebox] is required for runtime check`；
- 资源建议**只**来自 `[docker.recommended_resources]`；
  `[gamebox.recommended_resources]` 已删除（出现即报错）；
- 平台运行时的不可变身份是 **RepoDigest**（registry 上的 sha256 摘要），
  与本地 `image_id` 严格区分。

### safe_name 派生规则

`safe_name` 是 Docker repository 名（`^[a-z0-9]+(?:[._-][a-z0-9]+)*$`），
而 `id`（目录名）可以含空格、大写、撇号甚至中文。

- 缺省由 **content id（目录名）** 派生，**绝不从 `name` 派生**；
- 派生算法与 `content.py::derive_safe_name` 逐条一致：
  小写 → Unicode NFKD → 删除 combining marks → 删除 `'` 与 `’` →
  非 `a-z0-9._-` 转 `-` → 连续 `[._-]{2,}` 合并为 `-` → strip `._-` → 校验 pattern。

```text
comment                     → comment
Android_reverse             → android_reverse
FloatCTF-qidong             → floatctf-qidong
Cirno's perfect math class  → cirnos-perfect-math-class
Cirno’s book                → cirnos-book
foo   bar                   → foo-bar
foo__bar                    → foo-bar
--Foo..Bar--                → foo-bar
foo.bar                     → foo.bar
题目                        → 派生失败，必须显式 safe_name
```

- 显式 `safe_name` 先 trim 再校验：`" custom-name "` → `custom-name`；
- `safe_name` 字段一旦出现就必须合法：`safe_name = ""` / `"   "` 属于非法
  （**不会**回退到派生）；
- 同类型下 `safe_name` 不允许冲突 —— 这是**仓库级**校验，由 floatctf-content
  负责；fcmc 只校验单个包。

### 官方 canonical image ref

```text
Challenge: floatctf/{safe_name}:challenge-v{version}
GameBox:   floatctf/{safe_name}:gamebox-v{version}
```

类型编码在 **tag** 里，不再使用 `challenges/` 与 `gameboxes/` repository path。
Challenge 与 GameBox 允许共用同一个 `safe_name`（tag 不同）：

```text
floatctf/comment:challenge-v1.0.0
floatctf/comment:gamebox-v1.0.0
floatctf/cirnos-perfect-math-class:challenge-v1.0.0
```

---

## Content Contract 对齐

- 权威实现：[`floatctf-content/scripts/content.py`](https://github.com/FloatCTF/floatctf-content/blob/main/scripts/content.py)
  与其测试 `scripts/tests/test_content.py`；
- 对齐测试：`crates/fcmc/tests/content_contract_parity.rs`，直接使用
  floatctf-content 官方 fixture 的逐字节副本
  （`crates/fcmc/tests/fixtures/content_contract/`，见该目录 README 的来源说明）；
- 覆盖：canonical Challenge（container / static）、canonical GameBox（**无**
  `[gamebox]` 段）、`safe_name` / `version` / image ref 逐例 parity、
  `[docker.recommended_resources]` 迁移、AWD 扩展保留；
- fcmc **不**实现 catalog.json 生成、Event 反向关联或仓库级 safe_name 冲突检测
  —— 这些仍属于 floatctf-content。

---

## 镜像命名与构建代理

### 命名规则

```text
challenge: {registry_prefix}/{safe_name}:challenge-v{version}
gamebox  : {registry_prefix}/{safe_name}:gamebox-v{version}
```

- 官方 `registry_prefix`（namespace）是 `floatctf`；CLI 未提供 `-t` 时使用它；
- 平台 API 从平台配置（TOML）取 prefix，并显式传 tag；
- `-t/--tag` 是显式的 build override，用于本地调试；官方 ref 与 override 明确分离。

### 构建代理

```bash
fcmc build --proxy 7890           # → host.docker.internal:7890
fcmc build --proxy 10.0.0.1:7890  # → 原样使用
```

设置代理后给 `docker build` 注入：

- `--add-host=host.docker.internal:host-gateway`
- `HTTP_PROXY=http://<proxy>` / `HTTPS_PROXY=http://<proxy>`
- `ALL_PROXY=socks5://<proxy>`

适用于构建阶段需要外网的场景（apt-get、curl、git clone 等）；不传则不注入。

---

## Docker 连接策略

`fcmc` 作为独立 CLI / SDK 使用时优先检查 `/run/floatctf/helper-docker.sock`：

- helper socket 存在：通过 `floatctf-helper` 的 Docker policy proxy 访问 Docker；
- helper socket 不存在：fallback 到 Bollard 的本机 Docker 默认连接；
- helper socket 已存在但不可访问或后端异常：直接报错，不静默 fallback，避免绕过已经建立的权限边界。

FloatCTF 平台 API 不使用这个自动 fallback；生产与标准开发配置都会显式连接 helper socket。
因此 fcmc 仍可脱离 FloatCTF 单独使用，同时平台运行时保持严格的 helper 权限模型。

---

## 运行时检查

`fcmc check --runtime` 在静态检查通过后：

### Challenge

- 镜像：`floatctf/{safe_name}:challenge-v{version}`；
- `[flag] type = "dynamic"` → 以 `FLAG=flag{runtime-check}` 启动；static / 无 `[flag]`
  → 不注入任何 flag env；
- 端口绑定来自 `[docker].port`；没有 `port` 时不绑定任何端口（只打印 WARN，不会
  因为缺 `[docker]` 判 metadata 非法）；
- 打印 `访问地址: http://127.0.0.1:<映射端口>`；
- 按 **Enter**（或 Ctrl+C）后停止并删除容器（`auto_remove`）。

### GameBox

- 镜像：`floatctf/{safe_name}:gamebox-v{version}`；
- 需要 `[gamebox].username`（缺失 → 明确的 operational 错误）；
- 以 `GAMEBOX_USERNAME=<username>` + 随机密码（`Fc` + 12 位 hex）启动临时容器；
- 打印：

  ```text
  Docker IP: 172.17.0.3
  SSH 用户: floatctf
  SSH 密码: Fc2d1521088136
  SSH 连接: ssh floatctf@172.17.0.3
  端口映射: 127.0.0.1:32777 -> 容器内 22/tcp
  ```

- 可直接用打印的凭据 SSH 进容器测试（默认 bridge 网络，宿主机可直达容器 IP）；
  按 **Enter** 后停止并删除容器。

---

## 与平台 API 的边界

- **fcmc 负责**：模板生成、manifest 解析校验、镜像构建/标签/推送/拉取/检查、
  容器生命周期、本机运行时验证；
- **fcmc 绝不负责**：读写平台数据库、比分/事件/实例等业务状态、竞争 flag 生成、
  静态 flag 授权、`catalog.json` 生成、Event 反向关联 —— 这些是平台 API
  （`apps/api`）与 floatctf-content 的职责；
- 平台 API 导入包时调用 fcmc 的**库接口**（`ImageRuntime::build_image` /
  `ensure_image` 等），构建日志默认不写 stdout（`verbose=false`），避免污染服务端日志。

---

## 开发与测试

```bash
# 构建
cargo build -p fcmc

# 测试（metadata 契约、模板生成、CLI 解析、Content Contract parity、运行时；Docker 相关用例可跳过）
cargo test -p fcmc

# 与 Python 权威实现交叉验证 safe_name / version（开发用）
cargo run -p fcmc --example parity_dump       # 打印 Rust 侧结果
python3 - <<'PY'                              # 打印 Python 侧结果后逐行比对
...
PY

# 手动端到端（需要 Docker）
fcmc gen --name e2e-web
cd e2e-web
fcmc check && fcmc build --proxy 7890 && fcmc check --runtime
```

参考示例包（真实可用）：

- [`examples/test-c`](../../examples/test-c) — Challenge 包（动态 flag + docker）
- [`examples/test-g`](../../examples/test-g) — GameBox 包（SSH + healthchecks + judge + awdp）

---

交互式 AI 手册：`fcmc help --agent`。
