//! Template generation — writes template files to disk.
//!
//! This module contains the file generation logic and template strings.
//! It does NOT depend on Docker, bollard, or any runtime.

use anyhow::{Context, Result};

/// Generate a Challenge template directory.
pub fn generate_challenge_template(name: &str, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    let challenge_dir = Path::new(output_dir).join(name);
    fs::create_dir_all(&challenge_dir).context("Failed to create challenge directory")?;

    let src_dir = challenge_dir.join("src");
    fs::create_dir_all(&src_dir).context("Failed to create src directory")?;

    let attachment_dir = challenge_dir.join("attachment");
    fs::create_dir_all(&attachment_dir).context("Failed to create attachment directory")?;

    // meta.toml
    let meta_content = format!(
        r#"name = "{}"
author = "your_email@example.com" # modify
category = "Web" # modify
description = "Challenge description" # modify

attachment = "attachment/src.zip" # Optional

[flag]
value = ""       # is empty stand for dynamic flag # modify
env_var = "FLAG"


[docker]
image_tag = "floatctf/{}:challenge-web_v1.0" # modify
port = "80/tcp"
"#,
        name,
        name.to_lowercase()
    );
    fs::write(challenge_dir.join("meta.toml"), meta_content)
        .context("Failed to write meta.toml")?;

    // flag
    fs::write(src_dir.join("flag"), "flag{test_flag}").context("Failed to write flag")?;

    // flag.sh
    fs::write(
        src_dir.join("flag.sh"),
        r#"#!/bin/bash
# flag 动态替换脚本
sed -i "s/flag{test_flag}/$FLAG/" /flag

export FLAG=not_flag
FLAG=not_flag

rm -f /flag.sh
"#,
    )
    .context("Failed to write flag.sh")?;

    // entrypoint.sh
    fs::write(
        src_dir.join("entrypoint.sh"),
        r#"#!/bin/bash
if [ -f /flag.sh ]; then
    echo "--- 正在初始化 Flag ---"
    sed -i 's/\r//g' /flag.sh
    /flag.sh
fi

exec "$@"
"#,
    )
    .context("Failed to write entrypoint.sh")?;

    // index.php
    fs::write(
        src_dir.join("index.php"),
        r#"<?php
echo get_file_contents("/flag");
?>
"#,
    )
    .context("Failed to write index.php")?;

    // Dockerfile
    fs::write(
        src_dir.join("Dockerfile"),
        r#"FROM php:8.2-apache-bookworm
LABEL Author="your_name <your_email@example.com>"

COPY flag /flag
COPY flag.sh /flag.sh
COPY entrypoint.sh /entrypoint.sh
COPY index.php /var/www/html/index.php
RUN chmod +x /flag.sh
RUN chmod +x /entrypoint.sh

# 必须
EXPOSE 80
WORKDIR /var/www/html


ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "apache2-foreground" ]
"#,
    )
    .context("Failed to write Dockerfile")?;

    Ok(())
}

fn gamebox_meta_toml(name: &str) -> String {
    format!(
        r#"name = "{name}"
version = "1.0.0"
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
script = "judge/check.py"

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
"""GameBox functional check (judge).

Trusted by the platform; never baked into the Docker image.
Exit 0 = service OK, non-zero = down / broken.
"""
import sys


def main() -> int:
    # TODO: probe the target (env/args provided by judgeserver)
    print("ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
"#,
    )
    .context("Failed to write judge/check.py")?;
    Ok(())
}

/// Generate a GameBox template directory (new portable package format).
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

    // Dockerfile
    fs::write(
        src_dir.join("Dockerfile"),
        r#"FROM floatctf/awd-base:gamebox-web_v1.0.0

COPY index.php /var/www/html/index.php

RUN chown -R floatctf:floatctf /var/www/html
"#,
    )
    .context("Failed to write Dockerfile")?;

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

/// Generate a basic GameBox template directory (AWD base image sources).
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
USERNAME=${CTF_USER:-"floatctf"}
if [ "$USERNAME" != "floatctf" ]; then
    usermod -l "$USERNAME" floatctf || useradd -m -s /bin/bash "$USERNAME"
fi

# 2. 设置密码
if [ -n "$CTF_PASSWORD" ]; then
    echo "$USERNAME:$CTF_PASSWORD" | chpasswd
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
