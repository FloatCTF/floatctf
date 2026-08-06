#!/bin/bash
set -e
source ./.env
mkdir -p "$INSTALLER_DIR"
RUN_DIR="/${INSTALLER_DIR#./}"
RUN_DIR="${RUN_DIR//\/\///}"
#==============================INSTALL_CONFIG================================
API_ELF_URL="https://github.com/FloatCTF/floatctf-api/releases/latest/download/floatctf-linux-amd64-musl"
SQL_DIST_URL="https://github.com/FloatCTF/floatctf/releases/latest/download/sql.tar.gz"
HTML_DIST_URL="https://github.com/FloatCTF/floatctf-web/releases/latest/download/html.tar.gz"
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
log_info "Making directories...."
mkdir -p "$INSTALLER_DIR/"{bin,html,data,nginx/conf,tmp}
mkdir -p "$INSTALLER_DIR/logs/"{api,nginx,rustfs}
mkdir -p "$INSTALLER_DIR/api/challenges"



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



# important
JWT_SECRET_KEY=$(openssl rand -hex 32)

## conf files
mkdir -p "$INSTALLER_DIR"
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
RUSTFS_VOLUMES="${RUN_DIR}/data/rustfs0"
RUSTFS_ADDRESS=${RUSTFS_ADDRESS}
RUSTFS_OBS_LOG_DIRECTORY="${RUN_DIR}/logs/rustfs"


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


LOG_DIR="${RUN_DIR}/logs/api"
CHALLENGES_DIR="${RUN_DIR}/api/challenges"
UPLOAD_DIR="${RUN_DIR}/api/uploads"
WEAPONS_DIR="${RUN_DIR}/api/weapons"
IMAGES_DIR="${RUN_DIR}/api/images"

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
worker_priority 0;

events {
    worker_connections 1024;
    multi_accept on;
    use epoll;
}

http {
    include       /etc/nginx/mime.types;
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
    access_log ${RUN_DIR}/logs/nginx/access.log main;
    error_log ${RUN_DIR}/logs/nginx/error.log warn;

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

        ssl_certificate ${RUN_DIR}/keys/fullchain.pem;
        ssl_certificate_key ${RUN_DIR}/keys/privkey.pem;

        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;

        # 🌐 前端静态页面
        location / {
            root ${RUN_DIR}/html;
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
