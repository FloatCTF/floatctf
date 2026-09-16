//! 手工/运维向容器操作用例。

/// Full agent manual. 这份文档面向 AI 助手/自动化工具：包含全部子命令、
/// 全部选项、Content Contract、包目录布局、镜像命名、代理、运行时检查与常见错误。
pub const AGENT_MANUAL: &str = r#"================================================================================
fcmc — FloatCTF 容器构建与配置工具（AI 助手完整手册）
================================================================================

fcmc 是 FloatCTF 平台的 Challenge / GameBox 容器镜像构建与配置校验 CLI。
本手册面向 AI 助手 / 自动化工具使用，涵盖：全部子命令、全部选项、meta.toml
契约（Challenge 与 GameBox）、包目录布局、镜像命名规则、构建代理、运行时检查
与常见错误排查。所有示例均可在真实环境执行。

【Content Contract 的 source of truth】
  https://github.com/FloatCTF/floatctf-content  （scripts/content.py）
  fcmc 不定义第二套公共 metadata contract：发生冲突时以 floatctf-content 为准。

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

  重要概念（三者不同）:
    content id   包目录名（floatctf-content 中 challenges/<id>）。Event 引用、
                 catalog 的 id 都用它。
    name         meta.toml 中的显示名（可以是中文），只用于 UI。
    safe_name    Docker repository 名；meta.toml 可省略，缺省从 **content id**
                 派生 —— 绝不从 name 派生。

--------------------------------------------------------------------------------
2. check — 校验包配置（可选运行时验证）
--------------------------------------------------------------------------------
  用法: fcmc check [-p <目录>] [-f challenge|gamebox] [--runtime]
  别名: -f c / -f g

  选项:
    -p, --path <目录>     要检查的包目录（缺省 "."）。目录内必须含 meta.toml；
                          目录名即 content id。
    -f, --format <类型>   challenge (c) | gamebox (g)。缺省按以下优先级推断：
                            1. 路径位于 gameboxes/<id> → GameBox
                            2. 路径位于 challenges/<id> → Challenge
                            3. standalone 包且 meta.toml 含 [gamebox] 段 → GameBox
                            4. 否则 → Challenge
                          官方 canonical GameBox **可以没有** [gamebox] 段，
                          这类 standalone 包必须显式 -f gamebox。
    --runtime             额外连接 Docker 做运行时验证（见第 7 节）。

  行为:
    - 两层检查，报告里分开显示：
        A. Content Contract：官方公共字段（name/version/author/category/
           difficulty/tags/description）、safe_name 解析、[flag]/[docker]。
           违反 → "metadata contract invalid"。
        B. FCMC operational：附件文件是否存在、src/Dockerfile 是否存在
           （container vs static）、judge/awdp 脚本是否存在、[gamebox] 是否齐备。
           违反 → 该操作无法执行（不是 metadata 不合法）。
    - 输出"配置检查报告"（OK/WARN/ERR 分级 + 最终结果 通过/失败）。
    - 校验失败时退出码为 1；WARN 不阻断。
    - --runtime 且静态检查通过时，追加"[运行时检查]"段：拉取/确保镜像、启动
      临时容器、打印访问信息，等待用户按 Enter（或 Ctrl+C）后停止并删除容器。

  示例:
    fcmc check                    # 检查当前目录
    fcmc check -p ./test_g        # 检查指定目录
    fcmc check -f gamebox         # standalone canonical GameBox（无 [gamebox] 段）
    fcmc check --runtime          # 检查 + 运行时验证

--------------------------------------------------------------------------------
3. build — 构建 Docker 镜像
--------------------------------------------------------------------------------
  用法: fcmc build [-p <目录>] [-f challenge|gamebox] [-t <tag>] [--proxy <[ip:]port>]
  别名: -f c / -f g

  选项:
    -p, --path <目录>      要构建的包目录（缺省 "."）；目录名即 content id。
    -f, --format <类型>    challenge (c) | gamebox (g)。缺省按 check 相同的优先级
                           自动识别。
    -t, --tag <镜像名>     显式镜像 tag（build override），例如 myreg/x:test。
                           缺省时使用 floatctf-content canonical ref:
                             challenge: floatctf/{safe_name}:challenge-v{version}
                             gamebox  : floatctf/{safe_name}:gamebox-v{version}
                           （floatctf 是官方 namespace；平台导入由 API 从平台
                           配置取 registry prefix 并显式传 tag。）
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
    - 是否为容器只看 src/Dockerfile 是否存在（**不看** [docker] 段）。
      缺少 src/Dockerfile → 明确报 "content is static: src/Dockerfile not found"。
    - 只把包的 src/ 目录作为构建上下文（meta.toml、attachment/、judge/ 都
      不会进入镜像）。
    - 构建日志以流式方式打印到 stdout（每一步 STEP / 拉取 / RUN 输出可见）。
    - 构建产物信息：image_id（本地镜像 ID）+ target_ref（tag）。
    - 构建超时 600 秒（默认），失败以非 0 退出并打印错误。

  示例:
    fcmc build                       # 自动识别类型并构建（canonical ref）
    fcmc build -f gamebox -t myreg/x:test   # 显式 override tag
    fcmc build --proxy 7890          # 代理构建（apt 等走代理）

--------------------------------------------------------------------------------
4. gen — 生成包模板
--------------------------------------------------------------------------------
  用法: fcmc gen -n <名称> [-o <输出目录>] [-f challenge|gamebox] [-t] [--safe-name <slug>]
  别名: -f c / -f g

  选项:
    -n, --name <名称>      包名称（必填）。会作为生成的子目录名 = content id。
    -o, --output <目录>    输出目录（缺省 "."）。实际生成到 <output>/<name>/。
    -f, --format <类型>    challenge (c) | gamebox (g)，缺省 challenge。
    -t, --template         仅对 format=gamebox 生效：生成 awd-base 基础模板
                           （ubuntu 24.04 + ssh + apache 的 AWD 基础镜像源码）。
    --safe-name <slug>     显式 safe_name。content id 无法派生出合法 slug 时必填
                           （例如纯中文目录名）；否则报错
                           "unable to derive safe_name from content id; provide --safe-name"，
                           不会生成一个立刻 invalid 的包。

  生成物:
    Challenge（缺省）:
      <name>/meta.toml           # 官方 Content Contract（见第 5 节）
      <name>/src/Dockerfile      # php:8.2-apache-bookworm 示例
      <name>/src/entrypoint.sh   # 动态 flag 写入 /flag 后 unset FLAG 再 exec
      <name>/src/flag            # 占位 flag（运行时动态覆盖）
      <name>/src/index.php       # 读取 /flag 的示例应用
      <name>/attachment/note.txt  # 附件示例（可替换为 src.zip 等真实附件）

    GameBox（-f gamebox）:
      <name>/meta.toml
      <name>/src/Dockerfile      # 自包含 php:8.2-apache + openssh-server
      <name>/src/entrypoint.sh   # GAMEBOX_USERNAME/USERPASS 契约（见第 6 节）
      <name>/src/index.php       # SSRF curl 示例页
      <name>/src/flag.php        # AWDP 契约：/flag.php 按 FLAG env 返回本实例 flag
      <name>/judge/check.py      # judge 脚本：HTTP 健康检查（多 IP 批量，不进镜像）
      <name>/awdp/exploit.py     # AWD-P 攻击脚本：SSRF 打 flag（多 IP 批量，不进镜像）

    脚本接口（judge/awdp 一致，对齐 examples/test-g）:
      python <脚本> ip1 ip2 ... ipN        # 支持多 IP 批量，并发执行
      stdout 输出 JSON 数组: [{"success": bool, "gamebox_ip": "<ip>", ...}]
      退出码: 0 = 全部成功；1 = 任一失败；2 = 用法错误（未传 IP）

  示例:
    fcmc gen -n easy-web
    fcmc gen -n easy-awd-web -f gamebox
    fcmc gen -n awd-base -f gamebox -t        # 基础模板
    fcmc gen -n 题目 --safe-name challenge-001 # content id 无法派生时的显式值

--------------------------------------------------------------------------------
5. meta.toml 契约 — 官方 FloatCTF Content Contract
--------------------------------------------------------------------------------
  目录布局:
    <package>/
      meta.toml          # 包清单（必须）
      src/               # 唯一构建上下文：Dockerfile + 应用代码
                         #   （存在 src/Dockerfile = container content）
      attachment/        # 可选附件（src.zip 等），绝不进入镜像
      judge/             # 仅 GameBox：judge 脚本，绝不进入镜像
      awdp/              # 仅 GameBox：AWD-P 攻击脚本，绝不进入镜像

  公共字段（Challenge 与 GameBox 完全一致）:
    必填:
      name         显示名（非空；可以是中文）
      version      **严格 x.y.z**：^\d+\.\d+\.\d+$。"1.0.0" 合法；
                   拒绝 "1.0" / "v1.0.0" / "1.0.0-rc.1" / "1.0.0+build"。
      author       作者（非空）
      category     分类（非空；不限制取值，现有 content 用
                   ai/crypto/misc/pwn/reverse/web）
      difficulty   unknown | beginner | easy | medium | hard | expert
      tags         字符串数组；允许 []；每项 strip 后必须非空
      description  描述（非空）
    可选:
      safe_name    显式 safe_name（见第 5.2 节）；缺省由 **content id（目录名）** 派生
      [flag]       flag 配置（省略 = 运行时不注入 FLAG）
      [docker]     容器元数据（见下）

  未知的顶层扩展字段会被**忽略**（与 floatctf-content 一致，不报错）。
  但 FCMC 自己拥有的严格段（[gamebox] / [judge] / [awdp] / [flag]）出现未知
  字段会报错。

  [flag]（可选）:
    type = "dynamic"   动态 flag：平台在实例创建时生成并注入 FLAG 环境变量，
                       入口脚本写入 /flag；严禁同时带 value。
    type = "static"    静态 flag：必须提供 value（非空字符串），value 打进入
                       镜像，运行时不再注入 FLAG 环境变量。
    没有 [flag] 时 metadata 依然合法；运行时检查不会注入任何 FLAG。
    示例:
      [flag]
      type = "dynamic"
      # type = "static"
      # value = "flag{xxx}"

  [docker]（可选，公共段；Challenge 与 GameBox 共用）:
    port = 80          可选整数端口（1..65535）。拒绝字符串 "80/tcp"、0、65536。
                       是运行时端口绑定与 readiness TCP 探针端口
                       （Dockerfile 的 EXPOSE 不作为可信来源）。
                       只写 [docker.recommended_resources] 而不写 port 也合法。
    示例:
      [docker]
      port = 80

      [docker.recommended_resources]
      cpu_millis = 500          # 毫核
      memory_bytes = 268435456  # 字节（256 MiB）
      pids_limit = 100          # 进程数上限

  [docker.recommended_resources]（可选，partial）:
    每个字段出现时必须 > 0；**不要求三个字段齐全**。
    normalize 时物化缺省值：
      Challenge: 500 / 268435456 / 100
      GameBox  : 1000 / 536870912 / 100

  禁止的写法:
    字符串端口（如 port = "80/tcp"）、port = 0、非正数资源、
    [gamebox.recommended_resources]（资源唯一来源是 [docker.recommended_resources]）、
    [gamebox] 内的未知字段（break_points / fix_points / down_points / first_bonus /
    services / resources…）。

--------------------------------------------------------------------------------
5.1 meta.toml 契约 — GameBox 的 AWD 运行时扩展
--------------------------------------------------------------------------------
  以下段 **不是** 官方 Content Contract 的必需字段，缺失时 metadata 依然合法；
  只有需要对应运行时能力时才会报明确的 operational 错误。

  [gamebox]（可选）:
    username = "floatctf"       登录用户名（非空；普通 Linux 用户名）
    [[gamebox.healthchecks]]    0..N 条 readiness 探针
      type = "http"  需要 port（1..65535）+ path（如 "/"）+ expected_status（100..599）
      type = "tcp"   需要 port（1..65535）
    缺少 [gamebox] 时：
      - check（静态）通过，只给 WARN；
      - check --runtime / AWD 部署报错：
        "GameBox runtime metadata [gamebox] is required for runtime check"。

  [judge]（可选，缺省 WARN）:
    script = "judge/check.py"   # 必须位于 judge/ 下，且文件真实存在；兼容键名 check_script

  [awdp]（可选 section，缺省 WARN；出现则内部字段全部必填）:
    source_code_dir = "/var/www/html"   # 必填；容器内源码绝对路径，平台据此打包源码 zip 提供给选手
    exploit_script = "awdp/exploit.py"   # 必填；必须位于 awdp/ 下，且文件真实存在

  完整示例:
    name = "hello-floatctf"
    version = "1.0.0"
    author = "your_email"
    category = "web"
    difficulty = "unknown"
    tags = ["web"]
    description = "hello floatctf"

    [docker]
    port = 80

    [docker.recommended_resources]
    cpu_millis = 1000
    memory_bytes = 536870912
    pids_limit = 100

    [gamebox]
    username = "floatctf"

    [[gamebox.healthchecks]]
    type = "http"
    port = 80
    path = "/"
    expected_status = 200

    [judge]
    check_script = "judge/check.py"

    [awdp]
    source_code_dir = "/var/www/html"
    exploit_script = "awdp/exploit.py"

--------------------------------------------------------------------------------
5.2 safe_name 派生规则（Challenge 与 GameBox 共用）
--------------------------------------------------------------------------------
    - safe_name 是 Docker repository 名，必须匹配
      ^[a-z0-9]+(?:[._-][a-z0-9]+)*$（允许 foo.bar，允许单个 _/-/. 作分隔符）。
    - meta.toml 可省略 safe_name；此时由 **content id（目录名）** 派生，
      **绝不从 name 派生**：
        comment                     → comment
        Android_reverse             → android_reverse
        FloatCTF-qidong             → floatctf-qidong
        Cirno's perfect math class  → cirnos-perfect-math-class
        Cirno's book（U+2019）      → cirnos-book
        foo   bar                   → foo-bar
        foo__bar                    → foo-bar
        --Foo..Bar--                → foo-bar
        foo.bar                     → foo.bar
        题目                        → 派生失败，必须显式 safe_name
      派生算法（与 content.py 完全一致）:
        lowercase → Unicode NFKD → 删除 combining marks → 删除 ' 与 ’ →
        非 a-z0-9._- 转 '-' → 连续 [._-]{2,} 合并为 '-' → strip . _ -
    - 显式 safe_name 会先 trim 再校验：" custom-name " → "custom-name"。
    - safe_name 字段一旦出现就必须合法：safe_name = "" 或 "   " 属于非法
      （不会回退到派生），错误 "invalid safe_name"。
    - 同类型下 safe_name 不允许冲突（由 floatctf-content 的仓库级校验负责；
      fcmc 只校验单包）。

--------------------------------------------------------------------------------
5.3 版本规则
--------------------------------------------------------------------------------
    严格 ^\d+\.\d+\.\d+$：
      "1.0.0"、"0.0.1"、"12.34.56"、"01.0.0" 合法；
      "1.0"、"v1.0.0"、"1.0.0-rc.1"、"1.0.0+build" 非法。

--------------------------------------------------------------------------------
6. 镜像命名与构建规则
--------------------------------------------------------------------------------
    官方唯一格式（与 floatctf-content/catalog.json 一致），类型编码在 tag 中，
    不再使用 challenges/ 与 gameboxes/ repository path:
      Challenge: floatctf/{safe_name}:challenge-v{version}
      GameBox  : floatctf/{safe_name}:gamebox-v{version}
    例如:
      floatctf/comment:challenge-v1.0.0
      floatctf/cirnos-perfect-math-class:challenge-v1.0.0
      floatctf/comment:gamebox-v1.0.0
    Challenge 与 GameBox 允许共用同一个 safe_name（tag 不同）。
    platform API 导入时从平台配置（TOML）取 registry prefix，并显式传 tag。
    tag 是"人类可读版本名"；平台运行时的不可变身份是 RepoDigest
    （registry 上的 sha256 摘要），与本地 image_id 严格区分。
    构建上下文只有 src/：meta.toml、attachment/、judge/、awdp/ 永不打进镜像；
    judge/awdp 脚本由平台单独分发执行。

--------------------------------------------------------------------------------
7. 运行时检查（check --runtime）详解
--------------------------------------------------------------------------------
  前提: 本机 Docker 可用；本地缺镜像时自动 ensure_image（构建产物或从
        registry 拉取）。

  Challenge:
    - 镜像: floatctf/{safe_name}:challenge-v{version}
    - [flag] type = "dynamic"：以 FLAG=flag{runtime-check} 环境变量启动容器
      （入口脚本将其写入 /flag）；static / 无 [flag]：不注入任何 flag env。
    - 端口绑定来自 [docker].port；没有 port → 不绑定任何端口（只打印 WARN），
      不会因为没有 [docker] 判 metadata 非法。
    - 等 2 秒后确认容器 running；打印:
        访问地址: http://127.0.0.1:<映射端口>  (容器内 <port>/tcp)
    - 容器以 auto_remove 创建；按 Enter 或 Ctrl+C 后停止并删除。

  GameBox:
    - 镜像: floatctf/{safe_name}:gamebox-v{version}
    - 需要 [gamebox].username：缺失 → 明确的 operational 错误
      （不是 metadata 非法）。
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
    metadata contract invalid: ...
        Content Contract 校验失败。常见原因：缺 difficulty/tags、
        version 不是 x.y.z、tags 里有空字符串、description 为空、
        docker.port 越界、recommended_resources 非正数。

    unable to derive Docker safe_name from content id; set safe_name explicitly
        目录名无法派生出 ASCII safe_name（如纯中文目录名 "题目"）。在 meta.toml
        显式加 safe_name = "xxx"（或用 fcmc gen --safe-name）。

    invalid safe_name 'Foo Bar'
        显式 safe_name 不匹配 ^[a-z0-9]+(?:[._-][a-z0-9]+)*$。
        注意 safe_name = "" / "   " 也会报这个错（不会回退到派生）。

    invalid version '1.0' (expected x.y.z)
        version 必须严格 x.y.z；prerelease / build metadata 都不接受。

    GameBox runtime metadata [gamebox] is required for runtime check
        包 metadata 合法，但缺少 [gamebox].username，无法做 runtime check 或
        AWD 部署。补上 [gamebox] username 再试。

    content is static: src/Dockerfile not found
        该包是 static / attachment-only 内容，不能 build。补 src/Dockerfile，
        或不要对它跑 fcmc build。

    Invalid challenge meta.toml / Invalid gamebox meta.toml
        在 GameBox 目录里跑了 Challenge 解析（或反之）。用 -f/--format 显式指定，
        或把包放进 challenges/<id> / gameboxes/<id> 让其自动识别。

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
        [gamebox].username 不匹配 ^[a-z_][a-z0-9_-]{0,31}$。

--------------------------------------------------------------------------------
9. 与平台 API 的边界（重要）
--------------------------------------------------------------------------------
    - fcmc 只负责: 模板生成、meta.toml 解析校验、Docker 镜像构建/标签/推送/
      拉取/检查、容器生命周期、本机运行时验证。
    - fcmc 绝不负责: 读写平台数据库、比分/事件/实例管理等业务状态、竞争 flag
      生成、静态 flag 授权、catalog.json 生成、Event 反向关联。这些都是
      FloatCTF 平台 API（apps/api）与 floatctf-content 的职责。
    - 平台 API 导入包时调用的是 fcmc 的库接口（build_image / ensure_image 等），
      构建日志默认不走 stdout（verbose=false），避免污染服务端日志。
================================================================================
"#;

/// 打印完整 agent 手册（`fcmc help --agent`）。
pub fn print_agent_manual() {
    print!("{AGENT_MANUAL}");
}

/// 打印单命令详解页（`fcmc help <command>`）。
/// 对未知命令返回错误字符串，便于 CLI 非零退出。
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

/// 打印 AGENT_MANUAL 中 `start_marker` 与 `end_marker` 之间的段落
/// （含 start、不含 end），供分命令页面使用。
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
