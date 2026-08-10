//! Detailed usage manual for fcmc — printed by `fcmc help --agent` (AI-facing)
//! and `fcmc help <command>`.

/// Full agent manual. 这份文档面向 AI 助手/自动化工具：包含全部子命令、
/// 全部选项、meta.toml 契约、包目录布局、镜像命名、代理、运行时检查与常见错误。
pub const AGENT_MANUAL: &str = r#"================================================================================
fcmc — FloatCTF 容器构建与配置工具（AI 助手完整手册）
================================================================================

fcmc 是 FloatCTF 平台的 Challenge / GameBox 容器镜像构建与配置校验 CLI。
本手册面向 AI 助手 / 自动化工具使用，涵盖：全部子命令、全部选项、meta.toml
契约（Challenge 与 GameBox）、包目录布局、镜像命名规则、构建代理、运行时检查
与常见错误排查。所有示例均可在真实环境执行。

--------------------------------------------------------------------------------
0. 快速上手
--------------------------------------------------------------------------------
  # 1) 生成一个 Challenge 包模板
  fcmc gen --name easy-web

  # 2) 进入包目录，按需修改 meta.toml 与 src/ 内容
  cd easy-web

  # 3) 校验配置（无需 Docker）
  fcmc check

  # 4) 构建镜像（需要 Docker；可加 --proxy 走代理）
  fcmc build --proxy 7890

  # 5) 运行时验证（起一个临时容器，打印访问地址/SSH 凭据，按 Enter 退出）
  fcmc check --runtime

  # 6) 生成 GameBox（AWD 攻防）包模板并构建
  fcmc gen --name easy-awd-web --format gamebox
  cd easy-awd-web
  fcmc check
  fcmc build --format gamebox

--------------------------------------------------------------------------------
1. 用法与全局信息
--------------------------------------------------------------------------------
  二进制: target/debug/fcmc（开发环境: cargo run -p fcmc -- <COMMAND>）
  版本  : fcmc --version
  帮助  : fcmc --help            （clap 原生简版帮助）
         fcmc help <command>     （单命令详解）
         fcmc help --agent       （本完整手册）

  子命令: check | build | gen | help
  退出码: 0 = 成功；非 0 = 失败（check 失败、build 失败、运行时检查失败都会
         以非 0 退出，便于脚本/CI 判断）。

--------------------------------------------------------------------------------
2. check — 校验包配置（可选运行时验证）
--------------------------------------------------------------------------------
  用法: fcmc check [-p <目录>] [--runtime]
  别名: 无

  选项:
    -p, --path <目录>   要检查的包目录（缺省 "."）。目录内必须含 meta.toml。
    --runtime           额外连接 Docker 做运行时验证（见第 7 节）。

  行为:
    - 自动识别包类型：读 meta.toml，包含 "[gamebox]" 段 → 按 GameBox 检查，
      否则按 Challenge 检查。无需显式指定类型。
    - 静态检查（无 Docker）：解析 meta.toml + 语义校验（safe_name、version、
      flag、docker port/资源、附件路径、judge 脚本、src/Dockerfile 存在性），
      输出"配置检查报告"（OK/WARN/ERR 分级 + 最终结果 通过/失败）。
    - 校验失败时退出码为 1；WARN 不阻断。
    - --runtime 且静态检查通过时，追加"[运行时检查]"段：拉取/确保镜像、启动
      临时容器、打印访问信息，等待用户按 Enter（或 Ctrl+C）后停止并删除容器。

  示例:
    fcmc check                    # 检查当前目录
    fcmc check -p ./test_g        # 检查指定目录（GameBox 自动识别）
    fcmc check --runtime          # 检查 + 运行时验证

--------------------------------------------------------------------------------
3. build — 构建 Docker 镜像
--------------------------------------------------------------------------------
  用法: fcmc build [-p <目录>] [-f challenge|gamebox] [-t <tag>] [--proxy <[ip:]port>]
  别名: -f c / -f g

  选项:
    -p, --path <目录>      要构建的包目录（缺省 "."）。
    -f, --format <类型>    challenge (c) | gamebox (g)。缺省不传时按 meta.toml
                          内容自动识别（含 "[gamebox]" 段 → gamebox，否则 challenge）。
    -t, --tag <镜像名>     显式指定镜像 tag（如 myreg/challenges/x:1.0.0）。
                          缺省自动推导：
                            challenge: floatctf/challenges/<safe_name>:<version>
                            gamebox  : floatctf/gameboxes/<safe_name>:<version>
                          （floatctf 是 CLI 默认 registry 前缀；平台导入由
                           API 从平台配置取前缀并显式传 tag。）
    --proxy <[ip:]port>    构建代理。缺省 ip 时补 host.docker.internal，例如：
                            --proxy 7890           → host.docker.internal:7890
                            --proxy 10.0.0.1:7890  → 原样使用
                          效果：给 docker build 注入
                            --add-host=host.docker.internal:host-gateway
                            HTTP_PROXY=http://<proxy>  HTTPS_PROXY=http://<proxy>
                            ALL_PROXY=socks5://<proxy>
                          供构建阶段需要外网的指令（apt-get / curl / git clone 等）
                          使用；不传则不注入任何代理。

  行为:
    - 只把包的 src/ 目录作为构建上下文（meta.toml、attachment/、judge/ 都
      不会进入镜像）。
    - 构建日志以流式方式打印到 stdout（每一步 STEP / 拉取 / RUN 输出可见）。
    - 构建产物信息：image_id（本地镜像 ID）+ target_ref（tag）。
    - 构建超时 600 秒（默认），失败以非 0 退出并打印错误。

  示例:
    fcmc build                       # 自动识别类型并构建
    fcmc build -f gamebox -t myreg/gameboxes/g1:1.0.0
    fcmc build --proxy 7890          # 代理构建（apt 等走代理）

--------------------------------------------------------------------------------
4. gen — 生成包模板
--------------------------------------------------------------------------------
  用法: fcmc gen -n <名称> [-o <输出目录>] [-f challenge|gamebox] [-t]
  别名: -f c / -f g

  选项:
    -n, --name <名称>      包名称（必填）。会作为生成的子目录名。
    -o, --output <目录>    输出目录（缺省 "."）。实际生成到 <output>/<name>/。
    -f, --format <类型>    challenge (c) | gamebox (g)，缺省 challenge。
    -t, --template         仅对 format=gamebox 生效：生成 awd-base 基础模板
                          （ubuntu 24.04 + ssh + apache 的 AWD 基础镜像源码）。

  生成物:
    Challenge（缺省）:
      <name>/meta.toml           # manifest v1（见第 5 节契约）
      <name>/src/Dockerfile      # php:8.2-apache-bookworm 示例
      <name>/src/entrypoint.sh   # 动态 flag 写入 /flag 后 unset FLAG 再 exec
      <name>/src/flag            # 占位 flag（运行时动态覆盖）
      <name>/src/index.php       # 读取 /flag 的示例应用
      <name>/attachment/         # 附件目录（留空，可放 src.zip）

    GameBox（-f gamebox）:
      <name>/meta.toml
      <name>/src/Dockerfile      # 自包含 php:8.2-apache + openssh-server
      <name>/src/entrypoint.sh   # GAMEBOX_USERNAME/USERPASS 契约（见第 6 节）
      <name>/src/index.php       # SSRF curl 示例页
      <name>/judge/check.py      # judge 脚本占位（不进镜像）

  示例:
    fcmc gen -n easy-web
    fcmc gen -n easy-awd-web -f gamebox
    fcmc gen -n awd-base -f gamebox -t        # 基础模板

--------------------------------------------------------------------------------
5. meta.toml 契约 — Challenge manifest v1
--------------------------------------------------------------------------------
  目录布局:
    <package>/
      meta.toml          # 包清单（必须）
      src/               # 唯一构建上下文：Dockerfile + 应用代码（必须含 Dockerfile）
      attachment/        # 可选附件（src.zip 等），绝不进入镜像

  字段（严格校验，未知字段直接报错）:
    必填:
      name        包显示名（非空）
      version     SemVer 版本，如 "1.0.0"；带 build metadata（如 1.0.0+b1）
                 会被拒绝
      author      作者（非空）
      category    分类，如 "web" / "pwn" / "crypto" / "misc"（非空）
      description 描述（非空）
      [flag]      flag 配置，见下
    可选:
      safe_name   显式安全名（见第 5.3 节）；缺省由 name 派生，派生失败必须显式给
      attachment  附件相对路径，必须以 "attachment/" 开头、指向普通文件、
                  拒绝 ../ 与绝对路径（不配 = 无附件）
      [docker]    容器配置，见下
      [docker.recommended_resources]  建议资源，见下

  [flag]（必填）:
    type = "dynamic"   动态 flag：平台在实例创建时生成并注入 FLAG 环境变量，
                       入口脚本写入 /flag；严禁同时带 value。
    type = "static"    静态 flag：必须提供 value（非空字符串），value 打进入
                       镜像，运行时不再注入 FLAG 环境变量。
    示例:
      [flag]
      type = "dynamic"
      # type = "static"
      # value = "flag{xxx}"

  [docker]（可选）:
    port = 80          仅一个整数端口（1..65535）。拒绝字符串 "80/tcp"、0、
                       65536。该端口是运行时唯一暴露端口与 readiness TCP 探针
                       端口（EXPOSE 不作为可信来源）。
    示例:
      [docker]
      port = 80

  [docker.recommended_resources]（可选，缺省 500/268435456/100）:
    cpu_millis = 500         # 毫核
    memory_bytes = 268435456 # 字节（256 MiB）
    pids_limit = 100         # 进程数上限
    每个字段必须 > 0。

  完整示例（动态 flag + docker）:
    name = "easy-web"
    version = "1.0.0"
    author = "your_email@example.com"
    category = "web"
    description = "Challenge description"
    # safe_name = "easy-web-01"     # 可选；缺省派生
    # attachment = "attachment/src.zip"  # 可选附件

    [flag]
    type = "dynamic"

    [docker]
    port = 80

    [docker.recommended_resources]
    cpu_millis = 500
    memory_bytes = 268435456
    pids_limit = 100

  禁止字段（显式报错，绝不静默忽略）:
    schema_version / image_tag / env_var / 字符串端口（如 port = "80/tcp"）/
    image_ref / image_digest / registry 凭据 / container_id / host_port /
    network / CIDR / privileged / network_mode / cap_add / host_mounts /
    运行时 flag 路径 / flag 环境变量名；以及任何未知字段。

--------------------------------------------------------------------------------
5.1 meta.toml 契约 — GameBox manifest v1
--------------------------------------------------------------------------------
  目录布局:
    <package>/
      meta.toml
      src/               # 唯一构建上下文（必须含 Dockerfile）
      judge/             # judge 脚本目录（可选；绝不进入镜像）

  字段（严格校验，未知字段直接报错）:
    必填:
      name / version / author / category / description  同 Challenge
      [gamebox].username   登录用户名（普通 Linux 用户名，如 "floatctf"）
    可选:
      safe_name
      [[gamebox.healthchecks]]   0..N 条 readiness 探针
      [gamebox.recommended_resources]
      [judge].script             judge 脚本相对路径（必须位于 judge/ 下）

  [[gamebox.healthchecks]]（可选）:
    type = "http" 需要 port（1..65535）+ path（如 "/"）+ expected_status（如 200）
    type = "tcp"  需要 port（1..65535）
    示例:
      [[gamebox.healthchecks]]
      type = "http"
      port = 80
      path = "/"
      expected_status = 200

      [[gamebox.healthchecks]]
      type = "tcp"
      port = 22

  [judge]（可选，缺省 WARN）:
    script = "judge/check.py"   # 必须位于 judge/ 下，且文件真实存在

  完整示例:
    name = "hello-floatctf"
    version = "1.0.0"
    author = "your_email"
    category = "web"
    description = "hello floatctf"

    [gamebox]
    username = "floatctf"

    [[gamebox.healthchecks]]
    type = "http"
    port = 80
    path = "/"
    expected_status = 200

    [judge]
    script = "judge/check.py"

    [gamebox.recommended_resources]
    cpu_millis = 1000
    memory_bytes = 536870912
    pids_limit = 100

  禁止字段: 同 Challenge（image_tag / schema_version / 未知字段等）。

--------------------------------------------------------------------------------
5.2 safe_name 派生规则（Challenge 与 GameBox 共用）
--------------------------------------------------------------------------------
    - 可选字段。缺省时从 name 派生：ASCII 转小写，空白/标点归一化为连字符
      "-"，连续分隔符合并、去首尾。
        "Easy Web 01"  → "easy-web-01"
    - 派生失败（如纯中文名 "测试题"）→ 必须显式提供 safe_name，
      错误信息: safe_name is required (could not derive a valid slug from name)
    - 显式 safe_name 必须匹配 ^[a-z0-9][a-z0-9_-]*$（小写字母/数字开头，
      之后可含小写、数字、-、_）。
    - safe_name 是包身份（不是版本）；safe_name + version 是包发布身份。

--------------------------------------------------------------------------------
5.3 版本规则
--------------------------------------------------------------------------------
    - SemVer 校验（复用 crate 的 semver 库），拒绝 build metadata：
      "1.0.0" 合法；"1.0.0+build123" 报错。

--------------------------------------------------------------------------------
6. 镜像命名与构建规则
--------------------------------------------------------------------------------
    - 命名空间按包类型编码，绝不把类型编进 tag:
        Challenge: <prefix>/challenges/<safe_name>:<version>
        GameBox  : <prefix>/gameboxes/<safe_name>:<version>
    - CLI 默认 prefix = "floatctf"；平台 API 从平台配置（TOML）取 prefix，
      显式传 tag，绝不硬编码。
    - tag 是"人类可读版本名"；平台运行时的不可变身份是 RepoDigest
      （registry 上的 sha256 摘要），与本地 image_id 严格区分。
    - 构建上下文只有 src/：meta.toml、attachment/、judge/ 永不打进镜像；
      judge 脚本由平台单独分发执行。

--------------------------------------------------------------------------------
7. 运行时检查（check --runtime）详解
--------------------------------------------------------------------------------
  前提: 本机 Docker 可用；本地缺镜像时自动 ensure_image（构建产物或从
        registry 拉取）。

  Challenge:
    - 若 [flag] type = "dynamic"：以 FLAG=flag{runtime-check} 环境变量启动
      容器（入口脚本将其写入 /flag）；静态 flag 不打 env。
    - 等 2 秒后确认容器 running；打印:
        访问地址: http://127.0.0.1:<映射端口>  (容器内 <port>/tcp)
    - 容器以 auto_remove 创建；按 Enter 或 Ctrl+C 后停止并删除。

  GameBox:
    - 以 GAMEBOX_USERNAME=<meta.toml username> 与随机生成的
      GAMEBOX_USERPASS 环境变量启动（密码形如 Fc + 12 位 hex）。
    - 等 3 秒后确认容器 running；打印:
        Docker IP: <容器网络 IP>（如 172.17.0.3）
        SSH 用户 / SSH 密码
        SSH 连接: ssh <user>@<ip>
        端口映射: 127.0.0.1:<host_port> -> 容器内 22/tcp / 80/tcp
    - 用户可用打印的凭据 SSH 登录测试（默认 bridge 网络，从宿主机可直达
      容器 IP）；按 Enter 或 Ctrl+C 停止并删除。

--------------------------------------------------------------------------------
8. 常见错误与排查
--------------------------------------------------------------------------------
    unknown or legacy field in manifest: unknown field `xxx`
        meta.toml 含未知/已废弃字段（如 image_tag、env_var、schema_version、
        字符串端口）。删除该字段，或参照第 5 节契约修正。

    safe_name is required (could not derive a valid slug from name)
        name 无法派生出 ASCII safe_name（如纯中文）。在 meta.toml 显式加
        safe_name = "xxx"。

    Invalid challenge meta.toml
        在 GameBox 目录里跑了默认 Challenge 解析。build 未指定 --format 时
        会自动识别；若显式 -f challenge 而 meta.toml 含 [gamebox]，会报此错。

    image build timed out
        构建超过 600 秒。多因构建阶段需要外网（apt 等）而本机无代理：加
        --proxy 7890 重试；或检查 Docker 网络/镜像源。

    Failed to connect to Docker
        Docker daemon 不可用/未启动。启动 Docker 后再试（check --runtime
        与 build 都需要）。

    Container <name> is not running
        容器启动后未保持运行。查看构建日志/容器日志排查 entrypoint 或应用
        崩溃（如动态 flag 入口脚本 FLAG 缺失）。

    invalid GAMEBOX_USERNAME
        meta.toml [gamebox].username 不匹配 ^[a-z_][a-z0-9_-]{0,31}$。

--------------------------------------------------------------------------------
9. 与平台 API 的边界（重要）
--------------------------------------------------------------------------------
    - fcmc 只负责: 模板生成、meta.toml 解析校验、Docker 镜像构建/标签/推送/
      拉取/检查、容器生命周期、本机运行时验证。
    - fcmc 绝不负责: 读写平台数据库、比分/事件/实例管理等业务状态、竞争 flag
      生成、静态 flag 授权。这些都是 FloatCTF 平台 API（apps/api）的职责。
    - 平台 API 导入包时调用的是 fcmc 的库接口（build_image / ensure_image 等），
      构建日志默认不走 stdout（verbose=false），避免污染服务端日志。
================================================================================
"#;

/// Print the full agent manual (used by `fcmc help --agent`).
pub fn print_agent_manual() {
    print!("{AGENT_MANUAL}");
}

/// Print a single-command detailed page (`fcmc help <command>`).
/// Returns an error string for unknown commands so the CLI can exit non-zero.
pub fn print_command_manual(command: &str) -> Result<(), String> {
    match command {
        "check" => {
            print_agent_section("2. check — 校验包配置（可选运行时验证）", "3. build");
            Ok(())
        }
        "build" => {
            print_agent_section("3. build — 构建 Docker 镜像", "4. gen");
            Ok(())
        }
        "gen" => {
            print_agent_section("4. gen — 生成包模板", "5. meta.toml 契约");
            Ok(())
        }
        "help" => {
            println!("fcmc help --agent   完整 AI 手册");
            println!("fcmc help <command> 单命令详解（check | build | gen）");
            Ok(())
        }
        other => Err(format!(
            "unknown command '{other}' (expected: check | build | gen | help)"
        )),
    }
}

/// Print the section of AGENT_MANUAL between `start_marker` and `end_marker`
/// (start inclusive, end exclusive), used by the per-command pages.
fn print_agent_section(start_marker: &str, end_marker: &str) {
    let lines: Vec<&str> = AGENT_MANUAL.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains(start_marker))
        .unwrap_or(0);
    let end = lines
        .iter()
        .position(|l| l.contains(end_marker))
        .unwrap_or(lines.len());
    for line in &lines[start..end] {
        println!("{line}");
    }
}
