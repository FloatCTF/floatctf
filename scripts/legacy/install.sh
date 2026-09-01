#!/bin/bash
set -e
# require tar,curl,openssl,unzip
#==============================INSTALL_CONFIG================================
## downloading files
API_ELF_URL="https://github.com/FloatCTF/floatctf-api/releases/latest/download/floatctf-linux-amd64-musl"
SQL_DIST_URL="https://github.com/FloatCTF/floatctf/releases/latest/download/sql.tar.gz"
HTML_DIST_URL="https://github.com/FloatCTF/floatctf-web/releases/latest/download/html.tar.gz"
RUSTFS_ELF_URL="https://dl.rustfs.com/artifacts/rustfs/release/rustfs-linux-x86_64-musl-latest.zip"
NGINX_SRC_URL="https://nginx.org/download/nginx-1.26.2.tar.gz"
# base_config
INSTALLER_DIR="/app"

# API CONF
API_SERVER_IP="127.0.0.1"
API_SERVER_PORT=9090
API_USER="floatctf_api"
NODE_IP="127.0.0.1"

# nginx
NGINX_SERVER_HTTP_PORT=80
NGINX_SERVER_HTTPS_PORT=443
NGINX_USER="nginx"

# database
PG_HOST="localhost"
PG_PORT=5432
PG_USER=postgres
PG_PASSWORD=postgres
PG_DB=floatctf_db

# rustfs
RUSTFS_ACCESS_KEY=rustfsadmin
RUSTFS_SECRET_ACCESS_KEY=rustfsadmin
RUSTFS_ADDRESS="127.0.0.1:9000"
#==============================INSTALL_CONFIG================================
# ===== 颜色定义 =====
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# 辅助语句
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[WARN]${NC} $1"; }


INSTALLER_DIR=$(realpath "$INSTALLER_DIR")
#==============================MAIN============================================
if [ "$EUID" -ne 0 ]; then
    log_error "请使用 root 权限运行此脚本 (sudo ./install.sh)"
    exit 1
fi


# require tools
REQUIRED_TOOLS=("tar" "curl" "openssl" "unzip")
MISSING_TOOLS=()

for tool in "${REQUIRED_TOOLS[@]}"; do
    # 使用 command -v 检查工具是否在系统的 PATH 中
    if ! command -v "$tool" &> /dev/null; then
        MISSING_TOOLS+=("$tool")
    fi
done

if [ ${#MISSING_TOOLS[@]} -ne 0 ]; then
    log_error "错误：系统中缺少必要工具: ${MISSING_TOOLS[*]}"
    exit 1
fi
echo "✅ 权限及依赖检查通过，开始准备安装..."



# install
## making dirs
log_info "Making directories...."
mkdir -p "$INSTALLER_DIR/"{bin,html,backup,data,systemd,tmp}
mkdir -p "$INSTALLER_DIR/logs/"{api,nginx,rustfs}
mkdir -p "$INSTALLER_DIR/api/challenges"



## gen tls keys
log_info "Generating keys...."
if [ -d "$INSTALLER_DIR/keys" ] && [ -n "$(ls -A "$INSTALLER_DIR/keys" 2>/dev/null)" ]; then
    log_warn "keys 目录已存在且非空，跳过证书生成。"
else
    rm -rf "$INSTALLER_DIR/keys"
    mkdir -p "$INSTALLER_DIR/keys"

    openssl req -x509 -nodes -days 365 \
      -newkey rsa:2048 \
      -keyout "$INSTALLER_DIR/keys/privkey.pem" \
      -out "$INSTALLER_DIR/keys/fullchain.pem" \
      -subj "/C=CN/ST=Release/L=Run/O=FloatCTF/CN=localhost"
    log_success "[SUCCESS] 自签名证书生成成功。"
fi




log_info "Prepare for api"
if [ ! -f "$INSTALLER_DIR/bin/floatctf" ]; then
    curl -L "$API_ELF_URL" -o "$INSTALLER_DIR/tmp/floatctf"
    chmod +x "$INSTALLER_DIR/tmp/floatctf"
    cp "$INSTALLER_DIR/tmp/floatctf" "$INSTALLER_DIR/bin/floatctf"

    if id -u "$API_USER" >/dev/null 2>&1; then
        log_info "用户 '$API_USER' 已存在"
    else
        log_warn "用户 '$API_USER' 不存在，创建中..."
        # 创建系统用户，不允许登录
        useradd -r -s /usr/sbin/nologin -d $INSTALLER_DIR -m $API_USER

        log_success "用户 '$API_USER' 创建完成"
    fi
fi

log_info "Prepare for sql"
if [ ! -d "$INSTALLER_DIR/tmp/sql" ]; then
    curl -L "$SQL_DIST_URL" -o "$INSTALLER_DIR/tmp/sql.tar.gz"
    tar -xzf "$INSTALLER_DIR/tmp/sql.tar.gz" -C "$INSTALLER_DIR/tmp/"
    mv "$INSTALLER_DIR/tmp/src/sql" "$INSTALLER_DIR/tmp/sql"
fi

log_info "Prepare for html"
if [ -z "$(find "$INSTALLER_DIR/html" -maxdepth 0 -not -empty)" ]; then
    curl -L "$HTML_DIST_URL" -o "$INSTALLER_DIR/tmp/html.tar.gz"
    mkdir -p "$INSTALLER_DIR/tmp/html"
    tar -xzf "$INSTALLER_DIR/tmp/html.tar.gz" -C "$INSTALLER_DIR/html"
fi

log_info "Prepare for rustfs"
if [ ! -f "$INSTALLER_DIR/bin/rustfs" ]; then
    curl -L "$RUSTFS_ELF_URL" -o "$INSTALLER_DIR/tmp/rustfs.zip"
    unzip "$INSTALLER_DIR/tmp/rustfs.zip" -d "$INSTALLER_DIR/tmp/"
    chmod +x "$INSTALLER_DIR/tmp/rustfs"
    cp "$INSTALLER_DIR/tmp/rustfs" "$INSTALLER_DIR/bin/rustfs"
fi

log_info "Prepare for nginx"
if [ ! -f "$INSTALLER_DIR/nginx/sbin/nginx" ]; then
    if [ ! -f "$INSTALLER_DIR/tmp/nginx-1.26.2.tar.gz" ];then
        sudo apt-get update
        sudo apt-get install -y --no-install-recommends build-essential libpcre3 libpcre3-dev zlib1g-dev libssl-dev
        curl -L "$NGINX_SRC_URL" -o "$INSTALLER_DIR/tmp/nginx-1.26.2.tar.gz"
        mkdir -p "$INSTALLER_DIR/tmp/nginx-1.26.2"
        tar -zxvf "$INSTALLER_DIR/tmp/nginx-1.26.2.tar.gz" -C "$INSTALLER_DIR/tmp"
    fi
    PREFIX="$INSTALLER_DIR/nginx"
    (
            cd "$INSTALLER_DIR/tmp/nginx-1.26.2" || exit 1
            ./configure --prefix=${PREFIX} \
              --with-http_ssl_module \
              --with-http_v2_module \
              --with-http_stub_status_module \
              --with-http_realip_module \
              --with-http_gzip_static_module \
              --with-http_sub_module \
              --with-http_addition_module \
              --with-http_dav_module \
              --with-threads \
              --with-stream \
              --with-stream_ssl_module \
              --with-file-aio \
              --with-http_stub_status_module \
              --with-http_slice_module
            make -j$(nproc)
            sudo make install

            if id -u "$NGINX_USER" >/dev/null 2>&1; then
                log_info "用户 '$NGINX_USER' 已存在"
            else
                log_warn "用户 '$NGINX_USER' 不存在，创建中..."
                # 创建系统用户，不允许登录
                useradd -r -s /sbin/nologin "$NGINX_USER"
                log_success "用户 '$NGINX_USER' 创建完成"
            fi
    )
fi

log_info "Prepare for postgresql"
PG_VERSION=17
if ! (command -v psql &> /dev/null && psql --version | grep -q "$PG_VERSION"); then
    log_info "PostgreSQL $PG_VERSION 未安装或版本不匹配，开始安装..."
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends postgresql-common wget gnupg lsb-release git
    sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends postgresql-$PG_VERSION
    sudo systemctl enable postgresql
    sudo systemctl start postgresql

    DB_EXISTS=$(sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$PG_DB'" || echo "0")

    if [ "$DB_EXISTS" == "1" ]; then
        log_warn "数据库 '$PG_DB' 已存在，跳过创建步骤。"
    else
        log_info "正在创建数据库 '$PG_DB' 并设置密码..."
        sudo -u postgres psql -c "ALTER USER postgres PASSWORD '$PG_PASSWORD';"
        sudo -u postgres psql -c "CREATE DATABASE $PG_DB;"
        log_success "数据库 $PG_DB 初始化完成。"
    fi
fi

log_info "Prepare for docker"
if ! (command -v docker &> /dev/null); then
    log_info "正在安装 Docker..."
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends docker.io
    sudo systemctl enable docker
    sudo systemctl start docker
    log_success "Docker 安装成功。"
fi

## db update?
until PGPASSWORD="$PG_PASSWORD" psql -h 127.0.0.1 -U "$PG_USER" -d "$PG_DB" -c '\q' 2>/dev/null; do
    log_info "Waiting for PostgreSQL..."
    sleep 0.5
done

log_success "PostgreSQL is ready 🚀"

# 检查是否已初始化（通过检查是否有 users 表或 migrations 表）
INITED=$(PGPASSWORD="$PG_PASSWORD" psql -h 127.0.0.1 -U "$PG_USER" -d "$PG_DB" -tAc "SELECT 1 FROM pg_tables WHERE tablename IN ('users', 'schema_version') LIMIT 1" 2>/dev/null || echo "")
if [ "$INITED" = "1" ]; then
    log_warn "数据库已初始化，跳过 SQL 执行。"
else
    # 按顺序执行 SQL（01*.sql -> 02*.sql）
    for file in $(ls "$INSTALLER_DIR/tmp/sql/init"/*.sql | sort); do
        log_info "===> Running $file"
        PGPASSWORD="$PG_PASSWORD" psql \
        -h localhost \
        -U "$PG_USER" \
        -d "$PG_DB" \
        -f "$file"
    done
    log_success "All SQL executed successfully"
fi

# important
JWT_SECRET_KEY=$(openssl rand -hex 32)

## conf files
log_info "Writing to $INSTALLER_DIR/.env"
cat <<EOF > "$INSTALLER_DIR/.env"
##################### SYSTEM ################################
SYSTEM_VERSION="0.6.0"
SYSTEM_CHANGELOG_PATH="./CHANGELOG.md"

##################### DATABASE ##############################
POSTGRES_USER=${PG_USER}
POSTGRES_PASSWORD=${PG_PASSWORD}
POSTGRES_DB=${PG_DB}

###################### RUSTFS ################
RUSTFS_ACCESS_KEY=${RUSTFS_ACCESS_KEY}
RUSTFS_SECRET_KEY=${RUSTFS_SECRET_ACCESS_KEY}
RUSTFS_VOLUMES="${INSTALLER_DIR}/data/rustfs0"
RUSTFS_ADDRESS=${RUSTFS_ADDRESS}
RUSTFS_OBS_LOG_DIRECTORY="${INSTALLER_DIR}/logs/rustfs/"


##################### API ###################################
## server config
SERVER_LISTEN_IP="${API_SERVER_IP}"
SERVER_LISTEN_PORT=${API_SERVER_PORT}
DATABASE_URL="postgres://${PG_USER}:${PG_PASSWORD}@${PG_HOST}:${PG_PORT}/${PG_DB}"
RUST_LOG="actix_web=info,actix_server=info,floatctf=info"
SECRET=${JWT_SECRET_KEY}

## challenge and event
INSTANCE_MAX_PER_USER=2
INSTANCE_DESTROY_DELAY=60
EVENT_SCORE_DECAY=500
NODE_IP="${NODE_IP}"
HTTP_PREFIX="http://"


LOG_DIR="${INSTALLER_DIR}/logs/api"
CHALLENGES_DIR="${INSTALLER_DIR}/api/challenges"
UPLOAD_DIR="${INSTALLER_DIR}/api/uploads"
WEAPONS_DIR="${INSTALLER_DIR}/api/weapons"
IMAGES_DIR="${INSTALLER_DIR}/api/images"

# log and timestampz
TZ=Asia/Shanghai

# rustfs
RUSTFS_ENDPOINT_URL="http://${RUSTFS_ADDRESS}"
RUSTFS_ACCESS_KEY_ID="${RUSTFS_ACCESS_KEY}"
RUSTFS_SECRET_ACCESS_KEY="${RUSTFS_SECRET_ACCESS_KEY}"
RUSTFS_REGION="cn-east-1"

# Web Terminal
ENABLE_WEB_TERMINAL=0
EOF


log_info "Writing to $INSTALLER_DIR/nginx/conf/nginx.conf"
cat <<EOF > "$INSTALLER_DIR/nginx/conf/nginx.conf"
user ${NGINX_USER};
worker_processes auto;
worker_rlimit_nofile 100000;
daemon off;
worker_priority 0;

events {
    worker_connections 1024;
    multi_accept on;
    use epoll;
}

http {
    include       ${INSTALLER_DIR}/nginx/conf/mime.types;
    default_type  application/octet-stream;

    # Gzip 压缩
    gzip on;
    gzip_min_length 1024;
    gzip_buffers 16 8k;
    gzip_comp_level 6;
    gzip_types text/plain application/javascript application/x-javascript text/css application/xml text/javascript application/json;
    gzip_http_version 1.1;

    # 缓存
    proxy_cache_path /var/cache/nginx levels=1:2 keys_zone=cache_zone:10m max_size=100m inactive=60m use_temp_path=off;
    proxy_cache_key "\$scheme\$proxy_host\$request_uri";

    # 日志
    log_format main '\$remote_addr - \$remote_user [\$time_local] "\$request" '
                    '\$status \$body_bytes_sent "\$http_referer" '
                    '"\$http_user_agent" "\$http_x_forwarded_for"';
    access_log ${INSTALLER_DIR}/logs/nginx/access.log main;
    error_log ${INSTALLER_DIR}/logs/nginx/error.log warn;

    # 基础优化
    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    keepalive_requests 10000;
    server_tokens off;
    reset_timedout_connection on;

    server {
        listen ${NGINX_SERVER_HTTP_PORT};
        server_name _;
        return 301 https://\$host\$request_uri;
    }


    # 🔐 HTTPS 服务（正式）
    server {
        listen ${NGINX_SERVER_HTTPS_PORT} ssl;
        client_max_body_size 0;
        client_body_buffer_size 1m;

        server_name _;

        ssl_certificate ${INSTALLER_DIR}/keys/fullchain.pem;
        ssl_certificate_key ${INSTALLER_DIR}/keys/privkey.pem;

        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;

        # 🌐 前端静态页面
        location / {
            root ${INSTALLER_DIR}/html;
            try_files \$uri \$uri/ /index.html;
        }

        # 🚀 反向代理 API 服务
        location /api/ {
            proxy_pass http://${API_SERVER_IP}:${API_SERVER_PORT};
            proxy_http_version 1.1;

            # 基础转发配置
            proxy_set_header Host \$host;
            proxy_set_header Upgrade \$http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_cache_bypass \$http_upgrade;

            # 🔥 核心：传递真实 IP
            # 将直接连接 Nginx 的客户端 IP 放入 X-Real-IP
            proxy_set_header X-Real-IP \$remote_addr;
            # 将客户端 IP 追加到转发链列表中
            proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
            # 传递协议（http 或 https），对某些重定向逻辑很有用
            proxy_set_header X-Forwarded-Proto \$scheme;

            # 建议增加：防止后端读取 Header 超时
            proxy_connect_timeout 60s;
            proxy_read_timeout 60s;
        }

        location /public/ {
            rewrite ^/public/(.*)\$ /floatctf-public/\$1 break;
            proxy_pass http://${RUSTFS_ADDRESS};

            proxy_http_version 1.1;
            proxy_set_header Connection "";
        }

        location /private/ {
            rewrite ^/private/(.*)\$ /floatctf-private/\$1 break;
            proxy_pass http://${RUSTFS_ADDRESS};

            proxy_http_version 1.1;
            proxy_set_header Connection "";
        }

        # private generate_url

        # 🧩 提供附件资源（如 /files/crypto1/attachments/flag.txt）
        location ~ ^/challenges/([^/]+)/attachment/(.+)\$ {
            alias /app/api/challenges/\$1/attachment/\$2;
            add_header Content-Disposition "attachment; filename=\$2";
        }


    }


}
EOF


log_info "Writing to $INSTALLER_DIR/manage.sh"
cat <<EOF1 > "$INSTALLER_DIR/manage.sh"
#!/usr/bin/env bash
set -euo pipefail

# ------------------------------------------------------------
# manage.sh - 管理 floatctf 系列服务 (使用 systemd)
# 依赖: systemd
# ------------------------------------------------------------

# 颜色
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 必须用 root 执行
if [[ \$EUID -ne 0 ]]; then
    echo -e "\${RED}此脚本必须用 root 身份运行\${NC}"
    exit 1
fi

INSTALLER_DIR=${INSTALLER_DIR}

if [ ! -d "$INSTALLER_DIR" ]; then
  echo "❌ INSTALLER_DIR ($INSTALLER_DIR) not found"
  exit 1
fi
# 加载环境变量
if [ -f "${INSTALLER_DIR}/.env" ]; then
  set -a
  source "${INSTALLER_DIR}/.env"
  set +a
fi

WORK_DIR=\$(pwd)

TEMPLATE_DIR="\${WORK_DIR}/conf/systemd"
OUTPUT_DIR="${INSTALLER_DIR}/systemd"
SYSTEMD_DIR="/etc/systemd/system"
mkdir -p "\$RUSTFS_VOLUMES"

SERVICE_LIST=(floatctf-api floatctf-nginx floatctf-rustfs)


# 帮助信息
usage() {
    cat << EOF
用法: \$0 <命令> <服务>

命令:
  install      从模板生成单元并安装服务（enable）
  uninstall    停止、禁用并删除服务
  start        启动服务
  stop         停止服务
  restart      重启服务
  status       查看服务状态

服务:
  floatctf-api       API 后端服务
  floatctf-nginx     Nginx 服务
  floatctf-rustfs    RustFS 文件服务
  all                同时操作上述三个服务

示例:
  \$0 install all
  \$0 start floatctf-api
  \$0 status nginx
EOF
    exit 1
}

# 安装服务
install_service() {
    local svc="\$1"
    local src="\${OUTPUT_DIR}/\${svc}.service"
    local dest="\${SYSTEMD_DIR}/\${svc}.service"

    cp "\$src" "\$dest"
    chmod 644 "\$dest"
    systemctl daemon-reload
    systemctl enable "\$svc.service" || {
        echo -e "\${RED}启用服务 \$svc 失败\${NC}"
        exit 1
    }
    echo -e "\${GREEN}服务 \$svc 已安装并启用\${NC}"
}

# 卸载服务
uninstall_service() {
    local svc="\$1"
    local dest="\${SYSTEMD_DIR}/\${svc}.service"

    if systemctl is-active --quiet "\$svc.service"; then
        systemctl stop "\$svc.service" || true
    fi
    if systemctl is-enabled --quiet "\$svc.service" 2>/dev/null; then
        systemctl disable "\$svc.service" || true
    fi

    if [[ -f "\$dest" ]]; then
        rm -f "\$dest"
        systemctl daemon-reload
        echo -e "\${GREEN}服务 \$svc 已卸载\${NC}"
    else
        echo -e "\${YELLOW}服务 \$svc 未安装\${NC}"
    fi
}

# 操作服务
control_service() {
    local action="\$1"
    local svc="\$2"
    systemctl "\$action" "\$svc.service" || true
    echo -e "\${GREEN}\${svc}: \${action} 完成\${NC}"
}

# 查看状态
status_service() {
    local svc="\$1"
    systemctl status "\$svc.service" --no-pager 2>&1
}

# 处理单个服务
handle_single() {
    local svc="\$1"
    case "\$COMMAND" in
        install)
            install_service "\$svc"
            ;;
        uninstall)
            uninstall_service "\$svc"
            ;;
        start|stop|restart)
            control_service "\$COMMAND" "\$svc"
            ;;
        status)
            status_service "\$svc"
            ;;
        *)
            usage
            ;;
    esac
}

# 处理多个服务
handle_all() {
    for svc in "\${SERVICE_LIST[@]}"; do
        echo -e "\${YELLOW}--- 操作服务: \$svc ---\${NC}"
        handle_single "\$svc"
    done
}

# ---------- 主入口 ----------
if [[ \$# -lt 2 ]]; then
    usage
fi

COMMAND="\$1"
TARGET="\$2"

# 验证命令
case "\$COMMAND" in
    install|uninstall|start|stop|restart|status) ;;
    *) usage ;;
esac

# 验证目标服务
VALID_TARGETS=("all" "\${SERVICE_LIST[@]}")
if [[ ! " \${VALID_TARGETS[*]} " =~ " \${TARGET} " ]]; then
    echo -e "\${RED}无效的服务名: \$TARGET\${NC}"
    usage
fi

if [[ "\$TARGET" == "all" ]]; then
    handle_all
else
    handle_single "\$TARGET"
fi
EOF1
chmod +x "$INSTALLER_DIR/manage.sh"




## services
log_info "Writing to $INSTALLER_DIR/systemd/floatctf-api.service"
cat <<EOF > "$INSTALLER_DIR/systemd/floatctf-api.service"
[Unit]
Description=FloatCTF API Backend Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${API_USER}
Group=${API_USER}
ExecStart=${INSTALLER_DIR}/bin/floatctf
WorkingDirectory=${INSTALLER_DIR}
EnvironmentFile=-${INSTALLER_DIR}/.env
Restart=always
RestartSec=10
TimeoutStopSec=30
KillSignal=SIGTERM
KillMode=mixed
LimitNOFILE=65536
LimitNPROC=512

[Install]
WantedBy=multi-user.target
EOF


log_info "Writing to $INSTALLER_DIR/systemd/floatctf-rustfs.service"
cat <<EOF > "$INSTALLER_DIR/systemd/floatctf-rustfs.service"
[Unit]
Description=RustFS Object Storage Server
Documentation=https://rustfs.cn/docs/
After=network-online.target
Wants=network-online.target

[Service]
Type=notify
NotifyAccess=main
User=root
Group=root

WorkingDirectory=${INSTALLER_DIR}
EnvironmentFile=-${INSTALLER_DIR}/.env
ExecStart=${INSTALLER_DIR}/bin/rustfs $RUSTFS_VOLUMES

LimitNOFILE=1048576
LimitNPROC=32768
TasksMax=infinity

Restart=always
RestartSec=10s

OOMScoreAdjust=-1000
SendSIGKILL=no

TimeoutStartSec=30s
TimeoutStopSec=30s

NoNewPrivileges=true

ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectClock=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictRealtime=true

# service log configuration
StandardOutput=append:${INSTALLER_DIR}/logs/rustfs/rustfs.log
StandardError=append:${INSTALLER_DIR}/logs/rustfs/rustfs-err.log

[Install]
WantedBy=multi-user.target
EOF

log_info "Writing to $INSTALLER_DIR/systemd/floatctf-nginx.service"
cat <<EOF > "$INSTALLER_DIR/systemd/floatctf-nginx.service"
[Unit]
Description=The Nginx HTTP Server
After=network.target remote-fs.target nss-lookup.target

[Service]
Type=simple
User=root
Group=root

ExecStartPre=${INSTALLER_DIR}/nginx/sbin/nginx -t
ExecStart=${INSTALLER_DIR}/nginx/sbin/nginx
ExecReload=/bin/kill -s HUP $MAINPID
ExecStop=/bin/kill -s QUIT $MAINPID
KillSignal=SIGQUIT
KillMode=mixed
PrivateTmp=true
LimitNOFILE=1048576
LimitNPROC=32768
Restart=always
RestartSec=10s
TimeoutStopSec=30s

[Install]
WantedBy=multi-user.target
EOF


log_info "修正用户权限"
chown -R $API_USER:$API_USER $INSTALLER_DIR
chmod +x $INSTALLER_DIR/bin/*
usermod -aG docker $API_USER

log_success "All tasks done."
