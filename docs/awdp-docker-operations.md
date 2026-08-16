# AWDP Docker 运维操作全览

> 面向 FloatCTF AWD/AWDP 引擎的 Docker 相关操作调研：一场 AWDP 比赛从**赛前准备 → 赛中运行 → 赛后清理 → 平台运维**四个阶段涉及的每一次 Docker 操作（网络 / 容器 / 镜像 / Flag / 裁判 / 监控 / 清理），逐条列出**操作名、目的、实现要点（Docker API / 命令 / 选项）与常见坑**，并给出**「操作 vs FloatCTF 现状」对照表**（fcmc runtime 与 `modules/event/awd/*` 服务层已实现 ✅ / 部分实现 ⚠️ / 未实现 ❌）。
>
> 调研方法：Docker 官方文档（docs.docker.com）、WireGuard 官方文档、开源 AWD/AWDP 平台源码/文档，均给出一手来源链接。对照仓库以 `crates/fcmc/src/runtime/*.rs` 与 `apps/api/src/modules/event/awd/{service,infrastructure,system}/*` 为准。

---

## 1. 画像速览

FloatCTF 的 AWD 引擎采用一种接近「春秋云境」的**基础设施容器 + 独立裁判 worker** 架构：

- 每个赛事有一个 **internal=true** 的 Docker bridge 网络（`docker_network_name`），GameBox 容器都挂在上面，彼此内网可达；平台级还有一个「数据面」概念（`fctf-awdp-practice`），JudgeServer 与 GameBox 走同一数据网络（`target_ip:container_port` 直连）。
- 基础设施容器：`flagserver`、`judgeserver` 等，均以 `extra_hosts`（`flagserver:<ip>:judgeserver:<ip>`）注入每个 GameBox 的 `/etc/hosts`。
- **两套赛制、两套网络编排**：AWD（`modules/event/awd/`）走 WireGuard + 每队子网 + nftables；AWDP（`modules/event/awdp/`）不走 WG，沿用「随机 high-port 宿主端口暴露」+ create-only 提源码 + copy/exec/restart 打补丁。
- **Flag 架构是「flag server 按 source IP 解析」**，而不是往每个容器里每轮注入 Flag：`flag_service` 按 GameBox+round 生成确定性 Flag 并入库（`awd_flag_issues`），GameBox 需要时从 flag server 的 `/flag` 端点按其真实 TCP peer IP 取回（见 `awdp-judgeserver::handle_flag` → `internal/awdp/flag/resolve`）。
- **裁判 worker**（`awd-judgeserver` / `awdp-judgeserver`）是独立进程：拉取 job → 对 `target_ip:port` 做 HTTP/TCP 健康检查 → 执行 `check.py`/`exploit.py`（宿主侧 `python3`，**不是 docker exec 进容器**）→ 回调平台提交结果。

因此很多「教科书式 AWD Docker 操作」（镜像钉扎、每队子网 + WireGuard、nftables、Reset=stop+remove+recreate、归档清理）FloatCTF 已实现，而「往运行中容器每轮注入 Flag / docker exec 判题 / 容器内 patch」则被其 flag-server + 独立裁判架构替代。

### 已实现 vs 未实现（一览）

| 维度 | FloatCTF 现状 | 说明 |
|------|--------------|------|
| 镜像构建/推送/拉取/钉扎 | ✅ | `fcmc::ImageRuntime`（build/tag/push/pull/ensure/inspect/remove + RepoDigest pin） |
| 每赛事 Docker bridge 网络（internal + subnet） | ✅ | `fcmc::create_network` + `network_runtime` 编排 |
| 每队子网分配 | ⚠️ 部分 | `team_network_allocator` + `awd_team_networks`；Docker 层仍是单事件网络，队内隔离靠 nftables/conntrack |
| WireGuard VPN 接入 + peer 管理 | ✅ | `HostNetworkRuntime` + `system::wireguard`（wg/ip 命令） |
| 防火墙 / 隔离 | ✅ | `NftablesFirewallRuntime`（native nftables，Fail-Closed） |
| GameBox/基础设施容器创建（固定 IP/env/label/资源限制/cap_drop） | ✅ | `fcmc::create_gamebox` / `create_infrastructure_container` |
| 健康检查 / readiness | ⚠️ 部分 | Docker-level HC 置 None，改为**平台级 HTTP/TCP 探针**（judgeserver/precheck） |
| Reset / 快照恢复 | ✅ / ⚠️ | `reset_gamebox`（stop+remove+recreate）；**无快照（docker commit/export）恢复** |
| 每轮 Flag 下发与轮换 | ✅（等价替代） | flag server 按 source IP 解析，而非容器内注入 |
| resource limits / cap_drop / 防逃逸 | ✅ | `ResourceLimits` + gamebox 默认 cap_drop |
| 判分（checker / judge / exploit） | ✅ | 独立 judgeserver worker，宿主进程执行脚本 |
| 日志采集 | ✅ | `container_logs`（bollard logs） |
| 异常容器处理（卡死/失联/恢复） | ✅ | `recovery_service::recover_all` + `precheck_service` detect |
| 赛后清理（停容器/删网络/归档） | ✅ | `archive_service`（stop→remove→revoke peer→删 WG→删网络→置 Archived） |
| Docker 事件流（`docker events`） | ⚠️/❌ | 未用 bollard events；改为轮询 + recovery reconcile |
| 并发 / idempotency / 重试 | ✅ | scheduler task_key 幂等、deploy 幂等检查、judgeserver 重试 |
| image 垃圾回收 / docker system prune | ❌ | 未发现平台侧镜像裁剪 |

（对照表详见 §5。）

---

## 2. 赛前准备

### 2.1 镜像构建 / 校验 / 推送 / 拉取（image lifecycle）

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 构建镜像 | 从 GameBox src/ 产出一个不可变运行时镜像 | `docker build`（`fcmc::build_image`）：context `src/` tar 流式传给 daemon，`-t <image_prefix>/gameboxes/<safe_name>:<version>`，`--rm`，可注入 `--add-host`+HTTP(S)/ALL_PROXY build args（`build_proxy`）；`BuildImageOptionsBuilder` | 镜像标签可变→不可作 runtime 身份；必须在赛后 inspect 拿权威 image id，不能用日志 grep |
| 打 tag | 复核 / 分发给多节点 | `docker tag`（`tag_image`：`repo`/`tag` 拆分） | tag 会被覆盖，不能作为唯一身份 |
| 推送镜像 | 推到私有 registry，供重拉 | `docker push`（`push_image`）→ 推后 inspect 取 `RepoDigest`（`pick_repo_digest`），返回 `repo@sha256:…` | push 后必须拿 RepoDigest 钉扎；auth 缺失/过期 → `RegistryAuthFailed` |
| 拉取镜像 | 节点/恢复时拉回 | `docker pull`（`pull_image`：`create_image`, fromImage+tag, credentials） | 用 **digest** 拉可保证不可变一致性；只按 tag 拉可能拿到被覆盖的新内容 |
| 钉扎（pin） | 运行时用不可变镜像身份 | 优先 `image_repo_digest`（push 模式）→ 其次 `image_id`（LocalOnly）→ 禁可变 tag；`ensure_image` = inspect 本地存在 → 否则 pull | Ready Revision **绝不 rebuild**；本地镜像丢失时按 repo digest pull（见 challenge-package.md / gamebox-package.md） |
| 删除镜像 | 资源回收 | `docker image rm`（`remove_image`，force；404 容错） | 需要先停/删引用该镜像的容器 |

**来源**：
- Docker build: <https://docs.docker.com/engine/reference/commandline/build/> · multistage: <https://docs.docker.com/build/building/multistage/>
- push / pull: <https://docs.docker.com/engine/reference/commandline/push/> · <https://docs.docker.com/engine/reference/commandline/pull/>
- image inspect/tag/rm: <https://docs.docker.com/engine/reference/commandline/image_inspect/> · <https://docs.docker.com/engine/reference/commandline/tag/> · <https://docs.docker.com/engine/reference/commandline/image_rm/>
- digest vs tag 不可变性: <https://docs.docker.com/engine/reference/commandline/image_ls/>

### 2.2 网络规划（Docker bridge 网络 + 子网分配）

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 创建赛事网络 | 隔离 + 内网互通 | `docker network create -d bridge --internal --subnet <cidr> --ip-range …` + `com.docker.network.bridge.name=<name>`（`fcmc::create_network`：driver=bridge, internal, IPAM subnet config, options bridge.name） | `internal=true` 时容器**无法出网**（不能访问 internet 的 flag 校验接口），需确认基础设施是否走 host 网络或单独桥；同 subnet 与宿主默认 bridge 冲突 |
| 每队子网分配 | 每队一段独立网段 | 平台维护 `awd_network_settings`（全局池）+ `awd_network_allocations`/`awd_team_networks`（`team_network_allocator`）；Docker 层每事件一个网桥，队内区分靠固定 IP 与 nftables | 网段耗尽 / 与 WG 网段、宿主路由冲突；**规划要在完整 config 前（allocations FK→events）** |
| 固定 IP | 给每队每 GameBox 分配确定地址 | `--ip <ip>`（`fixed_ip` → NetworkingConfig endpoint IPAM ipv4） | IP 必须落在该网络 subnet 内且未被占；重复会 409 |
| DNS / 别名 | 容器间经名字互访 | `--network-alias`（`network_aliases`）与 `--add-host`（`extra_hosts`） | Docker 内嵌 DNS 只解析**同一自定义网络**上容器；`--add-host` 才是写 `/etc/hosts` 的可靠方式 |

**来源**：
- 网络概览: <https://docs.docker.com/engine/network/>
- bridge driver: <https://docs.docker.com/engine/network/drivers/bridge/>
- network create（subnet/ip-range/internal/bridge name）: <https://docs.docker.com/reference/cli/docker/network/create/>

### 2.3 基础设施容器预创建

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 预创建 flag server / judge server / VPN 入口 | 赛事开局即有统一访问点 | `docker run -d --network <evt> --ip <fixed_ip> -e EVENT_ID=… -e INTERNAL_TOKEN=<加密解密后> -e LISTEN_ADDR=… <image>`（`ensure_infra_container`） | token 需先 `AwdCrypto` 解密回明文，绝不能落日志（铁律§5）；`ensure_image` 拉镜像；DB 跟踪 + inspect 实时核验容器存活（P1-16），DB 有记录但容器没了要重建 |
| 生成对等者的路由指向 | 让 gamebox 通过 `extra_hosts` 找到 flagserver/judgeserver | `extra_hosts: ["flagserver:<ip>", "judgeserver:<ip>"]` | 基础设施容器 IP 变化时 `/etc/hosts` 不会自动更新，需 recreate |

（来源同 2.1/2.2；基础设施 token 加密解密见 `deploy_service.rs` + `crypto.rs`。）

---

## 3. 赛中运行

### 3.1 GameBox / 靶机容器创建

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 创建每队每靶机容器 | 为每队拉起可攻可守的实例 | `docker run` spec 来自 `fcmc::GameBoxSpec`：name、可见 pin 镜像、固定 IP、`GAMEBOX_USERNAME`/`GAMEBOX_USERPASS` env、labels（`awd.event_id/team_id/gamebox_instance_id/event_gamebox_id/runtime_generation/resource_kind`）、CpuQuota=millis*100/period 100000、Memory、PidsLimit、cap_drop（NET_ADMIN/NET_RAW/SYS_ADMIN）、healthcheck | env 里的 SSH 密码等敏感值；`auto_remove=false` 以便 reset 可控；labels 是跨容器/跨进程（recovery/清理）按 event 过滤的关键维度 |
| 创建基础设施容器 | flagserver/judgeserver 等 | 见 2.3 | — |
| 容器只读/权限最小化 | 防逃逸 | `--cap-drop=… --privileged=false`（`DockerContainerRuntime`），必要时 `--security-opt no-new-privileges` / seccomp | jailed 容器需要多少 cap 由 GameBox 决定；只给必要 cap |

**来源**：
- docker run: <https://docs.docker.com/engine/containers/run/>
- 资源约束（CPU quota/period、memory、pids）: <https://docs.docker.com/engine/containers/resource_constraints/>
- 权限/capability: <https://docs.docker.com/engine/containers/run/#runtime-privilege-and-linux-capabilities> · seccomp: <https://docs.docker.com/engine/security/seccomp/>

### 3.2 Reset / 快照恢复

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| Reset 靶机 | 恢复一张干净镜像 | `stop`（immediate timeout=0）→ `remove`（force, v=true）→ **recreate**（置 `runtime_generation+1`）（`fcmc::reset_gamebox` → `GameBoxResetSpec`） | 逻辑 Instance ≠ 容器（reset 换容器/世代，逻辑行保留）；reset 有次数上限与分值惩罚（`reset_service`：ban/ownership/protection/count） |
| 快照恢复（stop+rollback to snapshot） | 快速恢复到某轮可用态 | Docker 侧可用 `docker commit`（把运行态固化为镜像）/`docker export`（rootfs）实现，但 FloatCTF **未采用**，`reset_gamebox` 只做 stop+remove+recreate 的**最终一致性恢复** | 无快照意味着不能从"打坏的中途态"短秒回滚，只能整机重建；规模大时重建慢 |
| 优雅停 vs 立即停 | 控制 stop 行为 | `stop` 带 timeout：`DEFAULT_STOP_TIMEOUT`(10s，Jeopardy) vs `IMMEDIATE_STOP_TIMEOUT`(0s，AWD reset/清理) | stop 后要查 `State.ExitCode`/running 确认；force remove 时会顺带删匿名卷（`v=true`） |

**来源**：
- docker stop: <https://docs.docker.com/engine/reference/commandline/stop/>
- docker rm: <https://docs.docker.com/engine/reference/commandline/rm/>
- docker commit（快照）: <https://docs.docker.com/engine/reference/commandline/commit/>
- container state machine: <https://docs.docker.com/engine/reference/commandline/inspect/>

### 3.3 每轮 Flag 下发与轮换

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 生成当轮 Flag | 每 GameBox 每轮一个确定性 Flag | `generate_flag`（确定：同一 GameBox+同一 round = 同一 flag）+ `hash_flag` 存入 `awd_flag_issues`（幂等 find_or_create） | 需要时玩家其实不直接拿明文——攻击方提交的是从目标里"拿到"的 flag，而目标内那枚 flag 由 flag server 发出 |
| 把 Flag 送进目标容器 | 让攻击者能"取到" | 通用做法：ENV 注入 / 挂卷 / `docker cp` / 每轮 `docker exec` 写 /flag、或静态 `--env FLAG=…`（Jeopardy dynamic flag 用 env 注入见 handoff §15）；**FloatCTF 采用 flag-server 方案**：GameBox 向 flagserver `/flag` 按 source IP 取（`awdp-judgeserver::handle_flag`→`internal/awdp/flag/resolve`） | 每轮往运行中容器**动态换** flag 是 AWD 最大的难点：env 不更新、挂卷要 bind、exec 需要容器在跑且可写；flag-server 模型避开了这些，把"如何推进"变成"按 source IP 判定轮次" |
| 轮换 Flag | 每轮失效旧 flag | 轮推进（`round_service`）→ 新 round 生成新 flag issue；旧 round flag 依 `phase`/时间失效 | 依赖 `flag hash` 幂等；`submission_service` 校验提交是否当前 round 有效 |

**来源**：
- docker cp: <https://docs.docker.com/reference/cli/docker/cp/>
- volumes / bind mounts: <https://docs.docker.com/engine/storage/volumes/>
- docker exec: <https://docs.docker.com/engine/reference/commandline/exec/>

### 3.4 判分（Checker / Judge）

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 健康检查 | 判存活（Service Up?） | Internal HTTP/TCP probe（`healthcheck_all`），重试，timeout | 探针失败可能是平台故障（`platform_error`）而非玩家失败——不能误判 service_down |
| 执行 check.py | 判功能正确（Functional OK?） | 宿主 `python3 check.py <target_ip>`，env 白名单（只留 PATH/HOME/LANG/JUDGE_*），stdout/stderr 截断 | 输出畸形/超时/spawn 失败 → `platform_error`（平台释放重试），绝不判玩家失败；env 泄敏（INTERNAL_TOKEN）必须被白名单挡住 |
| 执行 exploit.py（攻击每队） | 判 Vulnerable/Patched | 宿主 `python3 exploit.py <target_ip>`，official 注入 `FLOATCTF_PROOF_URL` 一次有效；practice manual 仅诊断不计分 | 不要把**攻击**也交给 GameBox 自身容器去 self-exploit（破坏隔离）；用独立 worker 从数据面打 `target_ip` |
| 结果闭环 / 幂等 | 判分一次性落库 | 结果 4xx=stale 放弃、5xx 重试（backoff）；平台侧 lease + `runtime_generation` 判 stale；`awd_score_events` 幂等键 | 并发 worker 领同一 job → lease/attempt 去重 |

**来源**（判分架构属 FloatCTF 自研，裁判脚本契约看 `awdp-judgeserver` 头注释；通用 check/exploit 实践见开源平台 §6）。
- docker exec（若要容器内判题的替代方案）: <https://docs.docker.com/engine/reference/commandline/exec/>

### 3.5 AWDP 防御补丁（patch）与源码下发

FloatCTF 的 **AWDP 模块**（`apps/api/src/modules/event/awdp/`）是单独的赛制，采用**不引 WireGuard、改用「随机 high-port 宿主端口暴露」**的模型（与 AWD 的 WG 模型不同），并额外引入两本质上是把文件写进运行中容器 / 从镜像取源码的 Docker 操作：

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 下发防御补丁（patch） | 选手提交 patch.sh 修复自身靶机 | `docker exec`（`mkdir -p` 建解压目录）→ `copy_into_container`（`docker cp`/put_archive 写 patch.sh+辅助文件到 writable layer）→ `docker exec /bin/sh patch.sh`（注入 `FLOATCTF_SOURCE_DIR` env、workdir=解压目录、超时+输出上限）→ **`restart_container`**（`restart` **保留 writable layer**，patch 生效；区别于 reset 的 stop+remove+recreate）（`patch_service.rs`） | 目标目录必须先存在（`put_archive` 要求）；脚本相对路径以解压目录为基准；重启保层是关键——用 reset 会让 patch 丢 |
| 提取源码包（source.zip / tar.gz） | 选手在 Fix 阶段拿到靶机源码 | **create-only**（不 start）临时容器 → `copy_from_container`（`docker cp` 从容器下载 `source_code_dir` 为 tar）→ 重打包 tar.gz（src/ + patch.sh）→ remove（`source_artifact.rs`） | 涉及 `MAX_COPY_BYTES`(1GiB) 上限；无论成败都 remove 临时容器，防泄漏 |

**来源**：
- docker cp（拷贝进/出容器）: <https://docs.docker.com/reference/cli/docker/cp/>
- docker exec: <https://docs.docker.com/engine/reference/commandline/exec/>
- restart（保留 writable layer）: <https://docs.docker.com/engine/reference/commandline/restart/>

### 3.6 攻击 / 防御流量与防火墙

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 每队网络隔离 | 攻击者只能访问目标队集合 | Docker `--internal` 网络 + 每队固定 IP + nftables 规则（`NftablesFirewallRuntime`：`nft -c` 校验 → `nft -f` 原子应用 → verify；Fail-Closed） | 只用 Docker 内部网络无法做到"每队可互打但平台隔离"；必须宿主 nftables；只管理自己的 `table inet floatctf_awd`，禁 `nft flush ruleset` |
| 防火墙规则 reconciliation | 期望态→实际态 | 渲染整表 → 校验 → 原子应用（完整 reconcile） | 失败必须 Fail Closed（调用方置 NetworkError）；校验失败不能只警告 |
| 连接清理 | 轮次间隙清掉旧攻击链接 | `conntrack -D` 按 CIDR（`conntrack::flush_for_cidr`；`clear_event_connections`/`clear_team_connections`），round 切换时 flush | conntrack 条目会让被打封的链接跨轮复活，必须逐轮刷新 |

**来源**：
- Docker network internal: <https://docs.docker.com/engine/network/drivers/bridge/>
- nftables: <https://wiki.nftables.org/wiki-nftables/index.php/Ruleset> · 具体 nft 用法: <https://wiki.nftables.org/>
- conntrack: <https://conntrack-tools.netfilter.org/manual.html>

### 3.7 WireGuard VPN 接入与 peer 管理

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 创建赛事 WG 接口 | 给每队一个 L3 隧道入口 | `ip link add <iface> type wireguard` → `wg set <iface> private-key <key> listen-port <port>` → `ip addr add <addr>/<cidr> dev <iface>` → `ip link set <iface> up` + `sysctl net.ipv4.ip_forward=1`（`wireguard::create_interface`） | 接口名 ≤15 字符；create 失败可能因已存在（幂等吞错，configure 暴露真实问题） |
| 添加 peer（每队每玩家） | 允许某队 VPN 到达其 subnet | `wg set <iface> peer <pubkey> allowed-ips <team_subnet>`（`add_peer`） | `allowed-ips` 决定可路由网段，必须与队子网一致；pubkey 变更要做 peer 替换 |
| 轮换 peer / 吊销 | 踢人 / 换钥 | `wg set <iface> peer <pubkey> remove`（`revoke_peer`/`remove_peer`） | 只删 host 侧 peer，DB `awd_wireguard_peers` 状态也要同步（Active→Revoked，`archive_service`） |
| 事件清理时下线 WG | 赛后下线 | `ip link set <iface> down` + `ip link del <iface>`（`remove_wireguard`） | 接口已不存在时删除报错应容忍 |

**来源**：
- WireGuard 协议/概览: <https://www.wireguard.com/Protocol/Whitepaper.pdf> · 安装/quickstart: <https://www.wireguard.com/install/> / <https://www.wireguard.com/quickstart/>
- `wg` 命令参考: <https://www.wireguard.com/manpages/wg/en/> · `ip` 命令 WireGuard 用法: <https://www.man7.org/linux/man-pages/man8/wg.8.html>

### 3.8 监控 / 日志 / 异常容器

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 拉取容器日志 | 排障 / 取证 | `docker logs --tail N`（`container_logs`，bollard logs stdout+stderr，limit） | 日志可能含 flag/密钥，展示要截断脱敏 |
| 盘点赛事容器 | 发现缺失/多余 | `docker ps -a --filter label=awd.event_id=<id>`（`list_event_containers` → `ContainerFilter.label_equals`） | 必须 `all=true` 才能看到已停止容器 |
| 健康检测 & 失联处理 | 自动恢复 | `precheck_service`：`list_event_containers` + `inspect_container`（running/ip）；`recovery_service::recover_all`：按 event network 期望态重建缺失网络/容器 | DB 有记录但容器消失/未在跑 → 重建（Deploy 的 DB+实际核验双检查） |
| 资源用量监控（可选） | 防止跑飞 | `docker stats`（实时 CPU/内存流） | FloatCTF 用创建时的 ResourceLimits 固定上限，未做实时 stats 采集（见 §5 ❌） |
| Docker 事件流（可选） | 被动感知容器状态变更 | `docker events`（die/start/oom/destroy 等）实时推流 | FloatCTF 轮询 + recovery reconcile 替代 events（见 §5 ⚠️） |

**来源**：
- docker logs: <https://docs.docker.com/engine/reference/commandline/logs/>
- docker stats: <https://docs.docker.com/engine/reference/commandline/stats/>
- docker events: <https://docs.docker.com/engine/reference/commandline/events/>
- docker ps（filters）: <https://docs.docker.com/engine/reference/commandline/ps/>

---

## 4. 赛后清理

| 操作 | 目的 | 实现要点 | 常见坑 |
|------|------|----------|--------|
| 停 & 删全部靶机容器 | 释放资源 | 遍历 instances：`stop`（容错继续）→ `remove`（force）→ 已 404/409 容错（`archive_service`） | 删除顺序：先容器后网络；force remove 会删匿名卷 |
| 吊销 WG peers | 收回 VPN 接入 | 遍历 Active peers：`revoke_peer`（host）+ 落 DB `status=Revoked, revoked_at` | 一个失败不得中断其余；即便 host 操作失败也要先完成 DB 标记 |
| 删除 WireGuard 接口 | 收回宿主接口 | `remove_wireguard`（`ip link del`） | 接口从未创建时报错容忍 |
| 删除 Docker 网络 | 收网 | `remove_event_network`（按 `awd_runtime_resources` 记录的 docker_network_id；404 容错） | 需先确保该网络上无容器，否则删失败 |
| 归档（状态机） | 只清宿主资源、保留 DB | 上面所有操作后 `transition_event → Archived` | 只能运维 Finished 赛事；归档是状态机唯一入口（Phase 0） |

**来源**：
- docker stop/rm: <https://docs.docker.com/engine/reference/commandline/stop/> · <https://docs.docker.com/engine/reference/commandline/rm/>
- docker network rm: <https://docs.docker.com/engine/reference/commandline/network_rm/>

---

## 5. 操作 vs FloatCTF 现状对照表

> 依据：`crates/fcmc/src/runtime/{awd,docker,image,model,mod}.rs` + `apps/api/src/modules/event/awd/{service,infrastructure,system}/*`。

| # | 操作（阶段） | FloatCTF 现状 | 未实现/缺口说明 |
|--|--------------|---------------|-----------------|
| 1 | 镜像构建 / push / pull / 钉扎（赛前） | ✅ `ImageRuntime`（build/tag/push/pull/ensure/inspect/remove + RepoDigest pin） | — |
| 2 | 镜像垃圾回收 / docker system prune（赛后/运维） | ❌ | 无平台侧镜像裁剪；建议在 archive/scheduler 加 `image rm`/prune 或以 built_at 清理孤儿镜像 |
| 3 | 每赛事 bridge 网络（internal/subnet/bridge name）（赛前） | ✅ `create_event_network` → `create_network` | — |
| 4 | 每队独立 Docker 子网（赛前） | ⚠️ | 有 `awd_team_networks`/allocator 与固定 IP，但 Docker 层是**单事件网络**；队内隔离靠 nftables/conntrack。若需更强隔离，需每队再建网或每队 `--ip-range` 子网 |
| 5 | WireGuard 接口 + peer（赛中/清理） | ✅ `HostNetworkRuntime` + `system::wireguard` | 无 Noop/Host 之外的真实 peer 轮换策略（rotate）
| 6 | nftables 防火墙（赛中） | ✅ `NftablesFirewallRuntime`（native, Fail-Closed, 只管理自有 table） | — |
| 7 | conntrack 逐轮/逐队清理（赛中） | ✅ `system::conntrack`（flush_for_cidr） | — |
| 8 | GameBox/基础设施容器创建（固定 IP/env/labels/资源限制/cap_drop）（赛中） | ✅ `create_gamebox` / `create_infrastructure_container` + `spec_to_create_body` | gamebox 的 Docker-level healthcheck 置 None（改平台探针）；若需容器内 readiness 需补 |
| 9 | Reset=stop+remove+recreate（赛中） | ✅ `reset_gamebox` + `reset_service`（次数限制/惩罚） | — |
| 10 | 快照恢复（docker commit/export 回滚）（赛中） | ❌ | reset 只能整机重建，无中途态快照回滚；建议若需秒级回滚引入 commit/快照卷 |
| 11 | 每轮 Flag 下发 / 动态注入容器（赛中） | ⚠️ 等价替代 | 采用 flag-server 按 source IP 解析（`flag_service` + `awdp-judgeserver::handle_flag`），**非容器内注入**；若未来做"每队自带 flag 需推进到目标"记得 env/exec/卷方案 |
| 12 | 判分 checker/judge/exploit（赛中） | ✅ 独立 judgeserver worker（宿主脚本打 `target_ip`） | 未用 docker exec 判题；若需容器内检查脚本需另走 exec |
| 13 | 判分幂等 / lease / 重试（运维） | ✅ lease-heartbeat + attempt + score_event 幂等键 | — |
| 13b | AWDP 防御补丁写入并保层重启（赛中） | ✅ `patch_service`：exec mkdir → copy_into_container → exec patch.sh → restart_container（保 writable layer） | — |
| 13c | AWDP 源码下发（create-only 容器取回）（赛中） | ✅ `source_artifact`：create(不 start)→copy_from_container→重打包→remove | — |
| 14 | 日志采集（赛中/运维） | ✅ `container_logs`（bollard logs） | 截断上限有，但无集中式日志归档到对象存储（reserved as gap） |
| 15 | 健康检测 & readiness（赛中） | ✅ 平台级 HTTP/TCP 探针（judgeserver/precheck）；⚠️ 容器级 healthcheck 未用 | 若需容器 `HEALTHCHECK` 状态需补 config |
| 16 | 异常容器处理（卡死/失联恢复）（运维） | ✅ `recovery_service::recover_all` + `precheck_service` detect + Deploy DB+实容器双检查 | — |
| 17 | Docker 事件流（`docker events` 被动感知）（运维） | ⚠️/❌ | 轮询 + recovery reconcile，未消费 `/events` 流；若需实时 OOM/start 通知可补 `bollard::system::events` |
| 18 | 资源用量实时监控（docker stats）（运维） | ❌ | 只在创建时固定上限；无运行期 stats 面板/告警 |
| 19 | 赛后清理（停容器/删网络/归档）（赛后） | ✅ `archive_service`（stop→remove→revoke peer→删 WG→删网络→Archived） | — |
| 20 | 并发 / idempotency / retry（运维） | ✅ scheduler task_key 幂等/active unique + deploy 幂等 + judgeserver 重试 | — |
| 21 | 平台访问 Docker 的抽象层（运维） | ✅ bollard `DockerContainerRuntime`/`ImageRuntime`/`AwdContainerRuntime` trait | 无 docker events/stats 抽象（见 17/18） |

**统计**：✅ 完全实现 ≈ 15，⚠️ 部分实现 ≈ 4，❌ 未实现 ≈ 4（镜像 GC、快照回滚、实时 stats、docker events——其中后两者可选增强）。

> **提示（网络架构差异）**：FloatCTF 的 **AWD**（WG 模型：WireGuard + 每队子网 + nftables，`modules/event/awd/`）与 **AWDP**（host 随机端口绑定模型，`modules/event/awdp/`）是两套独立的 Docker/网络编排。本表对两者均已覆盖；AWDP 不走 WG，而是沿用 Jeopardy 的端口暴露 + create-only 提源码 + copy/exec/restart 打补丁。

---

## 6. 参考来源汇总

- Docker 官方文档：网络 / run / 资源约束 / build / volumes / cp / exec / logs / stats / events / inspect / stop/rm / commit / image 相关，均在对应小节内联了链接。
- WireGuard 官方：<https://www.wireguard.com/> · <https://www.wireguard.com/manpages/wg/en/> · <https://www.man7.org/linux/man-pages/man8/wg.8.html>
- nftables：<https://wiki.nftables.org/> · conntrack：<https://conntrack-tools.netfilter.org/manual.html>
- 开源 AWD/AWDP 平台（详见 §6.1 增补）。

> 注：本次调研以「一手来源（Docker/WG 官方文档 + 开源平台源码）」为准；FloatCTF 自研架构（flag-server 按 source IP、独立 judgeserver worker、nftables reconcile、archive 状态机）的依据来自仓库源码注释与 `docs/handoff.md`、`docs/challenge-package.md`、`docs/gamebox-package.md`。
