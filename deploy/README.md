> **本目录是 `install.sh` 的模板副本**，不是第二个安装入口。
> `install.sh --mode compose` 会生成同样的栈；这里的命令用于阅读、对照，或你确实想完全手动时。
> 安装请用 [INSTALL.md](../INSTALL.md)（`--mode host` / `--mode compose`）。

# 部署：两种方式，二选一

| | A. `install.sh` | B. `docker compose` |
|---|---|---|
| 适合 | 生产首次安装、升级 | 已有编排体系 / 想自己控制每一步 |
| 覆盖 | 全量 | 全量（**前提：helper 已在位**） |
| 前置 | 无（脚本装齐） | helper + systemd unit + control network |

> **硬约束**：API 启动时会 `Ping` `helper-control.sock`、并连接 `helper-docker.sock`；
> 任一不可达都会 **panic 退出**。API 按架构**不允许**直接挂 `/var/run/docker.sock`。
> 所以「纯 compose」不存在——B 方式也必须先装 `floatctf-helper`。

---

## A. install.sh

```bash
curl -fsSL https://github.com/FloatCTF/floatctf/releases/latest/download/install.sh -o install.sh
sudo env SITE_ADDRESS=ctf.example.com bash install.sh
sudo systemctl start floatctf.target
```

脚本会：装 `floatctf-helper` + 三个 systemd unit → 下 6 个产物 → 渲染 TOML/Caddyfile/compose →
建 `fctf-platform-control` 网络 → 本地构建 `floatctf/api:<V>` → 起 Compose。
升级 = 重跑同一条命令。

---

## B. docker compose

前置：Linux + systemd + Docker（含 compose 插件）、`python3`（含 `tomllib`）、固定域名。

### 1. 装 helper（一次性）

```bash
# 从 release 取二进制与 unit
sudo install -m 0755 floatctf-helper /usr/local/libexec/floatctf-helper
sudo install -m 0644 deploy/helper/floatctf-helper.service /etc/systemd/system/

# 前置用户/组：floatctf 组、floatctf-helper 用户（加入 docker 组）
sudo groupadd -f floatctf
sudo useradd --system --no-create-home --gid floatctf --groups docker floatctf-helper || true
sudo systemctl daemon-reload && sudo systemctl enable --now floatctf-helper

# 自检：两个 socket 都在
ls -l /run/floatctf/helper-control.sock /run/floatctf/helper-docker.sock
```

### 2. 准备目录与文件

布局与 install.sh 完全一致（compose 里的 bind mount 依赖它）：

```bash
sudo mkdir -p /var/lib/floatctf/{config/caddy,config/oidc,data,logs,web,frontends,runtime,db}
cd /var/lib/floatctf

# 三个模板：compose / 配置 / 站点
sudo cp deploy/compose.prod.yml   ./compose.prod.yml
sudo cp deploy/.env.example       ./.env            # 然后改掉 CHANGE_ME
sudo cp infra/config/floatctf.prod.toml ./config/floatctf.toml
sudo cp infra/caddy/Caddyfile.prod      ./config/caddy/Caddyfile

# release 产物里取：web-dist / merged.sql / ops-tools（migrate.sh + migrations）
sudo tar -xzf web-dist.tar.gz -C web/
sudo cp merged.sql ./merged.sql
sudo tar -xzf ops-tools.tar.gz -C .                 # 得到 db/migrate.sh、db/migrations/
```

### 3. 建 control network（compose 里声明为 external）

```bash
sudo docker network create --driver bridge --internal \
  --subnet 10.42.8.0/24 --ip-range 10.42.8.128/25 \
  --label io.floatctf.managed=true fctf-platform-control
```

### 4. 起栈

```bash
cd /var/lib/floatctf
sudo docker compose -f compose.prod.yml up -d --wait
sudo docker compose -f compose.prod.yml ps        # 五项 Up/healthy
```

首次启动 Postgres 会用挂载的 `merged.sql` 初始化；**升级**时改用：

```bash
sudo FLOATCTF_CONFIG=/var/lib/floatctf/config/floatctf.toml \
  /var/lib/floatctf/db/migrate.sh apply
```

### 5. 验收

```bash
curl -fsS -o /dev/null -w '%{http_code}\n' https://$SITE_ADDRESS/           # 200
sudo docker compose -f compose.prod.yml exec api \
  curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9090/api/users/me  # 401
```

---

## 文件对照

| 文件 | 来源 | B 方式需要 |
|---|---|---|
| `deploy/compose.prod.yml` | 本目录（install.sh 按版本从这里拉取） | ✓ |
| `deploy/.env.example` | 本目录 | ✓ |
| `deploy/helper/floatctf-helper.service` | 本目录（同上，测试守护） | ✓ |
| `infra/config/floatctf.prod.toml` | 仓库 | ✓ |
| `infra/caddy/Caddyfile.prod` | 仓库 | ✓ |
| `web-dist.tar.gz` / `merged.sql` / `ops-tools.tar.gz` | GitHub Release | ✓ |
| API 镜像 | `docker.io/floatctf/floatctf-api:<V>`（release 自动发布） | ✓（或改成 `API_IMAGE=floatctf/api:<V>` 用本地构建） |

### API 镜像与 GID

发布镜像固定 `FLOATCTF_GID=65532`（镜像只能烘一个 gid）。宿主 `floatctf` 组 gid 不同时，
容器仍以数值身份正常运行，只是 Web 终端里 `id` 解析不到组名。

要完全对齐，二选一：

```bash
# ① 让宿主组用 65532
sudo groupmod -g 65532 floatctf

# ② 用宿主真实 GID 本地构建（脚本会校验 user=65532:65532）
bash scripts/build-api-image.sh \
  --binary ./floatctf --tag "$VERSION" \
  --gid "$(getent group floatctf | cut -d: -f3)"
# 之后在 .env 里设 API_IMAGE=floatctf/api:<V>
```
