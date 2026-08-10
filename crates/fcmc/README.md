# fcmc — FloatCTF 容器构建与配置工具

fcmc 是 [FloatCTF](https://github.com/FloatCTF/floatctf) 平台的 **Challenge / GameBox
容器镜像构建与配置校验 CLI + 库**。它负责：

- **模板生成**：生成 Challenge（Jeopardy 题）与 GameBox（AWD 攻防题）的包骨架；
- **配置校验**：严格解析并校验 `meta.toml` 清单（Challenge manifest v1 / GameBox manifest v1）；
- **镜像构建**：以包内 `src/` 为唯一构建上下文构建 Docker 镜像，支持构建代理；
- **运行时验证**：起一个临时容器，打印访问地址（Challenge）或 Docker IP + SSH 凭据（GameBox），按 Enter 后自动清理；
- **库接口**：`metadata` / `runtime` / `application` 三个层次，供平台 API（`apps/api`）复用同一套解析、构建与镜像逻辑。

> 交互式完整手册（面向 AI / 自动化工具，非常详细）：`fcmc help --agent`

---

## 目录

- [快速开始](#快速开始)
- [命令参考](#命令参考)
- [包目录布局](#包目录布局)
- [meta.toml 契约](#metatoml-契约)
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

# 2. 按需修改 meta.toml（flag 类型、端口、分类等）与 src/ 下的应用代码
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

`build` 与 `check` 都会**自动识别包类型**：meta.toml 含 `[gamebox]` 段按 GameBox
处理，否则按 Challenge 处理；`--format` 仅用于显式覆盖。

---

## 命令参考

| 命令 | 用途 | 主要选项 |
|------|------|----------|
| `check` | 校验包配置（+ 可选运行时验证） | `-p/--path`，`--runtime` |
| `build` | 构建 Docker 镜像 | `-p/--path`，`-f/--format`，`-t/--tag`，`--proxy [ip:]port` |
| `gen` | 生成包模板 | `-n/--name`（必填），`-o/--output`，`-f/--format`，`-t/--template` |
| `help` | 输出使用说明 | `--agent`（完整 AI 手册），或指定命令名 |

- `fcmc help --agent` — 376 行完整手册（命令、选项、meta.toml 契约、布局、镜像命名、代理、常见错误）；
- `fcmc help check` / `help build` / `help gen` — 单命令详解；
- 退出码：0 = 成功，非 0 = 失败（便于脚本判断）。

### check

```text
用法: fcmc check [-p <目录>] [--runtime]
```

- 静态检查（默认）：解析并校验 meta.toml —— safe_name、SemVer version、flag 类型、
  docker 端口（1..65535）、建议资源（>0）、附件路径、GameBox 的 healthchecks / judge
  脚本 / `src/Dockerfile` 存在性，输出分级（OK / WARN / ERR）报告；
- `--runtime`：在静态检查通过后连接 Docker 起临时容器做运行时验证（见[运行时检查](#运行时检查)）。

### build

```text
用法: fcmc build [-p <目录>] [-f challenge|gamebox] [-t <tag>] [--proxy <[ip:]port>]
```

- 镜像 tag 缺省自动推导：`floatctf/challenges/<safe_name>:<version>`（challenge）
  或 `floatctf/gameboxes/<safe_name>:<version>`（gamebox）；`floatctf` 仅为 CLI
  默认 registry 前缀，平台导入由 API 从平台配置取前缀并显式传 tag；
- 只把 `src/` 作为构建上下文，`meta.toml` / `attachment/` / `judge/` 永不进镜像；
- Docker 构建日志**流式打印**到 stdout（每步 STEP 可见）；
- `--proxy`：构建阶段需要外网（apt / curl / git clone）时使用（见[构建代理](#镜像命名与构建代理)）。

### gen

```text
用法: fcmc gen -n <名称> [-o <输出目录>] [-f challenge|gamebox] [-t]
```

生成物：

| 文件 | Challenge | GameBox |
|------|-----------|---------|
| `meta.toml` | manifest v1 | manifest v1 |
| `src/Dockerfile` | php:8.2-apache-bookworm | php:8.2-apache + openssh-server |
| `src/entrypoint.sh` | 动态 flag 写入 `/flag` 后 unset 再 exec | `GAMEBOX_USERNAME/USERPASS` 契约 + 启 sshd |
| `src/index.php` | 读 `/flag` 示例 | SSRF curl 示例 |
| `src/flag` | 占位（动态覆盖） | — |
| `attachment/` | 附件目录（留空） | — |
| `judge/check.py` | — | judge 脚本占位（不进镜像） |

---

## 包目录布局

### Challenge

```text
<package>/
├── meta.toml          # 包清单（必须）
├── src/               # 唯一构建上下文（必须含 Dockerfile）
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
├── src/               # 唯一构建上下文（必须含 Dockerfile）
│   ├── Dockerfile
│   ├── entrypoint.sh
│   └── index.php
└── judge/             # judge 脚本（可选），绝不进镜像
```

---

## meta.toml 契约

> 两个 manifest 都使用 **严格校验**（`deny_unknown_fields`）：未知字段 / 已废弃字段
> （如 `image_tag`、`env_var`、`schema_version`、字符串端口 `"80/tcp"`）直接报错，
> 绝不静默忽略。

### Challenge manifest v1

```toml
name = "easy-web"                # 必填，显示名
version = "1.0.0"                # 必填，SemVer（拒绝 build metadata，如 1.0.0+b1）
author = "your_email@example.com" # 必填
category = "web"                 # 必填
description = "Challenge description"  # 必填
# safe_name = "easy-web-01"      # 可选；缺省由 name 派生，派生失败必须显式给
# attachment = "attachment/src.zip"    # 可选；必须以 attachment/ 开头

[flag]                           # 必填
type = "dynamic"                 # dynamic | static
# type = "static"
# value = "flag{xxx}"            # static 必填；dynamic 禁止携带 value

[docker]                         # 可选；缺省表示无容器（纯附件题）
port = 80                        # 唯一暴露端口，1..65535（拒绝 "80/tcp"）

[docker.recommended_resources]   # 可选；缺省 500 / 268435456 / 100
cpu_millis = 500                 # 毫核
memory_bytes = 268435456         # 字节（256 MiB）
pids_limit = 100                 # 进程数上限
```

字段规则：

- **`[flag]`（必填）**
  - `type = "dynamic"`：平台在实例创建时生成 flag，注入 `FLAG` 环境变量，
    入口脚本（`entrypoint.sh`）写入 `/flag` 后 `unset FLAG` 再 `exec "$@"`
    —— 应用进程永远拿不到真实 flag 的环境变量；
  - `type = "static"`：`value` 必填（非空），flag 直接打进镜像，运行时不再注入；
- **`[docker]`（可选）**
  - `port`：唯一整数端口，是运行时端口绑定与 readiness TCP 探针端口；
    Dockerfile 的 `EXPOSE` 不作为可信来源；
  - 注意：纯附件题（misc/crypto 不发容器）可省略整个 `[docker]` 段；
- **`[docker.recommended_resources]`（可选）**：作者建议资源，每项必须 > 0，
  平台会校验是否超过平台上限。

### GameBox manifest v1

```toml
name = "hello-floatctf"
version = "1.0.0"
author = "your_email"
category = "web"
description = "hello floatctf"

[gamebox]                        # 必填
username = "floatctf"            # SSH 登录用户名（普通 Linux 用户名）

[[gamebox.healthchecks]]         # 可选，0..N 条 readiness 探针
type = "http"                    # http | tcp
port = 80
path = "/"
expected_status = 200

# [[gamebox.healthchecks]]
# type = "tcp"
# port = 22

[judge]                          # 可选（缺省 WARN）
script = "judge/check.py"        # 必须位于 judge/ 下且真实存在

[gamebox.recommended_resources]  # 可选
cpu_millis = 1000
memory_bytes = 536870912
pids_limit = 100
```

字段规则：

- `[gamebox].username`（必填）：SSH 登录用户，匹配 `^[a-z_][a-z0-9_-]{0,31}$`；
- `[[gamebox.healthchecks]]`：平台的应用层 readiness 探针（HTTP 或 TCP），
  不是 Docker HEALTHCHECK；
- `[judge].script`：judge 脚本路径，必须位于 `judge/` 下；judge 不进镜像，
  由平台单独分发执行；
- 运行时契约：平台以 `GAMEBOX_USERNAME` / `GAMEBOX_USERPASS` 环境变量注入
  登录凭据，入口脚本负责建用户、设密码、`unset` 凭据、启 sshd 再 `exec "$@"`。

### safe_name 派生规则（Challenge / GameBox 共用）

- 缺省由 `name` 派生：ASCII 转小写，空白/标点归一化为 `-`，连续分隔符合并、去首尾；
  `"Easy Web 01"` → `easy-web-01`；
- 派生失败（如纯中文 `测试题`）→ 必须显式提供 `safe_name`；
- 显式 `safe_name` 必须匹配 `^[a-z0-9][a-z0-9_-]*$`；
- `safe_name` 是包身份（不是版本），`safe_name + version` 是包发布身份。

---

## 镜像命名与构建代理

### 命名规则

```text
challenge: <registry_prefix>/challenges/<safe_name>:<version>
gamebox  : <registry_prefix>/gameboxes/<safe_name>:<version>
```

- 命名空间（`challenges/` vs `gameboxes/`）编码包类型，绝不把类型编进 tag；
- CLI 默认前缀 `floatctf`；平台 API 从平台配置（TOML）取前缀；
- tag 是"人类可读版本名"；平台运行时的不可变身份是 **RepoDigest**（registry
  上的 sha256 摘要），与本地 `image_id` 严格区分。

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

## 运行时检查

`fcmc check --runtime` 在静态检查通过后：

### Challenge

- 以 `FLAG=flag{runtime-check}`（动态 flag）或空 env（静态 flag）启动临时容器；
- 打印 `访问地址: http://127.0.0.1:<映射端口>`；
- 按 **Enter**（或 Ctrl+C）后停止并删除容器（`auto_remove`）。

### GameBox

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
  静态 flag 授权 —— 这些是平台 API（`apps/api`）的职责；
- 平台 API 导入包时调用 fcmc 的**库接口**（`ImageRuntime::build_image` /
  `ensure_image` 等），构建日志默认不写 stdout（`verbose=false`），避免污染服务端日志。

---

## 开发与测试

```bash
# 构建
cargo build -p fcmc

# 测试（当前 137 项：metadata 契约、模板生成、CLI 解析、运行时；Docker 相关用例可跳过）
cargo test -p fcmc

# 手动端到端（需要 Docker）
fcmc gen --name e2e-web
cd e2e-web
fcmc check && fcmc build --proxy 7890 && fcmc check --runtime
```

参考示例包（真实可用）：

- [`examples/test_c`](../../examples/test_c) — Challenge 包（动态 flag + docker）
- [`examples/test_g`](../../examples/test_g) — GameBox 包（SSH + healthchecks + judge）

---

交互式 AI 手册：`fcmc help --agent`。
