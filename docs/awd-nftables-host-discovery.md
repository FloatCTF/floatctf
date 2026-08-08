# FloatCTF AWD Host Netfilter / nftables Discovery（P1-1）

> 本文档记录 FloatCTF AWD 防火墙在宿主上的 Netfilter 事实与决策。
> **priority 是平台技术常量，不是赛事配置**——由本发现确定，实现为
> `infrastructure/firewall/render.rs` 的 `FORWARD_PRIORITY`。

## 1. 当前环境实测（2026-08-08，开发机）

| 检查项 | 命令 | 结果 |
|---|---|---|
| nft 版本 | `nft --version` | `nftables v1.1.6 (Commodore Bullmoose #7)` |
| kernel | `uname -r` | `7.1.5-arch1-2` |
| Docker firewall backend | `docker info` | `Firewall Backend: iptables` |
| iptables 前端 | `iptables --version`（无 sudo 时不可用） | 待 root 环境确认 |
| firewalld | `firewall-cmd --state`（无 sudo 时不可用） | 待 root 环境确认 |
| 已有 nftables base chains | `sudo nft list ruleset` | 待 root 环境确认 |
| CAP_NET_ADMIN | `nft list tables` | 无 sudo 下失败 → capability 判定为 Unsupported |

## 2. root 环境必跑检查清单

```bash
# 1. iptables 前端模式（legacy vs nf_tables）
iptables --version
#    若输出含 "nf_tables" → iptables-nft（内核里是 nftables 实现）
#    若输出 "legacy" → iptables-legacy（独立 Netfilter 路径）

# 2. Docker firewall backend 证据
docker info | grep -i firewall
cat /etc/docker/daemon.json 2>/dev/null   # 是否 iptables=false 等

# 3. firewalld
firewall-cmd --state

# 4. 宿主现有 nftables base chains（hooks/priorities）
sudo nft list ruleset | grep -E "hook (forward|input|output)" 

# 5. 我们的 capability 判定应返回 Supported
#    （apps/api 启动日志 + Phase 2 Precheck Host env 快照）
```

## 3. Priority 决策

**决策：`FLOATCTF_FORWARD_PRIORITY = 1`**（`type filter hook forward priority 1`）。

依据（Netfilter hook 语义）：

- forward hook 上多个 base chain 按 priority 数值升序执行；`NF_ACCEPT` 后
  netfilter core 会继续尝试下一个已注册 hook function。
- iptables `filter` FORWARD 与 firewalld 的 forward chain 常规注册在 **priority 0**。
- FloatCTF 需要**在 Docker/firewalld 放行之后仍能施加 restrictive DROP**，
  因此选择 `1`（紧随 0 之后执行）：
  - Docker/firewalld ACCEPT 的包仍会到达我们的 chain → 我们可以 DROP；
  - 我们 DROP 的包不会回滚到 Docker（DROP 是终止裁决）。

**必须由 P1-0 prototype（root 环境）验证**：

```text
□ 宿主 SSH 不受影响（INPUT 不动，只挂 forward）
□ Docker 正常业务网络不受影响
□ Hardening/Attack/Pause 矩阵实测通过（apps/api/scripts/nft_prototype.sh）
□ 与 firewalld（若运行）共存无冲突
□ 若发现 firewalld/Docker 使用了非 0 priority 的 forward chain，
  按实际冲突调整本常量并更新此文档
```

## 4. Ownership 边界（铁律 §4 落地）

FloatCTF 只允许：

```text
create / replace / delete table inet floatctf_awd
```

以及该 table 下的 chains / sets / maps / rules。

禁止：`nft flush ruleset`、修改 Docker-owned tables、firewalld tables、
libvirt tables、管理员自定义 tables、直接修改 DOCKER chain / DOCKER-USER。

## 5. 与 Docker backend 的兼容结论

Docker backend = iptables（当前开发机）时：FloatCTF nftables 与 Docker iptables
经 Netfilter 共同作用于 forward 数据流，互不修改对方规则 —— 可工作。

Docker backend 未来切换 native nftables 时：双方各自维护自己的 tables，
FloatCTF 业务架构不变（Scenario I，Phase 5）。

## 6. capability 判定（P1-12 实现）

`infrastructure/firewall/env.rs::check_host_capability`：

```text
nft binary exists
+ nft list tables 成功（隐式验证 nf_tables + CAP_NET_ADMIN）
→ Supported

缺任一 → HostNetworkCapability::Unsupported
→ 不自动 fallback iptables（Noop 仅 unit test / dev mock，永远不允许 Verified）
```
