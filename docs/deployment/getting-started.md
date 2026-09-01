# FloatCTF 生产部署（Phase 10 起步）

> 在尚不支持 `scripts/deploy.sh` 自动化前的极简部署入口；完整细节见
> `docs/deployment/portability.md` 与 `chore/phase10-deployment-design.md`。

## 一次性主机初始化（root）

```bash
sudo scripts/init.sh          # 检查+初始化：docker/nftables/WG/ip/sysctl/br_netfilter/floatctf 用户/布局
sudo scripts/init.sh --check  # 只读预检（不写任何状态）
```

幂等：重复运行安全；`/home/floatctf/.initialized` 标记已存在时跳过主体写入。

## 构建可移植发布包（普通用户）

```bash
scripts/build-release.sh              # 自动：musl 可用则 musl，否则容器 glibc-2.34 基线
scripts/build-release.sh --container  # 强制容器基线
```

产物：`release/floatctf-<version>/`（`bin/floatctf` + `web/` + 镜像 + checksums.txt）。
不推送镜像（本地 daemon）。

## 部署（普通用户 + root/sudo，或直接 root）

```bash
scripts/deploy.sh <release-dir>                 # 默认 release/floatctf-*
scripts/deploy.sh --dry-run                     # 只预检+装配，不写系统
API_PORT=9290 scripts/deploy.sh <release-dir>   # 用环境变量覆盖端口（冲突避让）
```

流程：`precheck → 配置(.env+toml+nginx) → 装配产物 → infra(--wait) → 迁移 → systemd → 启动 API`。
首次部署生成密钥；重部署保留密钥。失败即退出（非零）。

## 服务管理

```bash
sudo systemctl start floatctf.target   # 拉起 infra + api 全部服务
sudo systemctl status floatctf-api     # API
journalctl -u floatctf-api -f          # API 日志
docker compose -f /home/floatctf/compose.yml ps   # infra 健康
```

## 验收要点（部署后）

- 前端：`curl http://127.0.0.1:8080/` → 200 且含 `<title>FloatCTF</title>`。
- API：`curl http://127.0.0.1:9090/api/announcements` → 401（已监听，需认证）。
- AWD 网络健康（管理员）：`/api/admin/awd/network/health` → nftables/wireguard/docker 全 Healthy。
- SSE（选手）：带 player token 请求 `/api/events/{id}/awd/stream` → `: connected` 首帧。