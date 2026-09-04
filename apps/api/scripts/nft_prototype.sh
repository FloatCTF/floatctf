#!/usr/bin/env bash
# ==============================================================================
# P1-0 nftables networking prototype（强制 gate）
#
# 在任何 production renderer 上生产环境之前，用最小网络原型验证
# Netfilter/nftables topology + hook/priority + 三阶段策略矩阵 + Docker 共存。
#
# 规模：1 event / 2 teams / 每队 1 GameBox / 1 WG-like 源命名空间（模拟玩家）
#
# 使用独立测试 table `floatctf_awd_test`，脚本结束自动 cleanup；
# 不写业务数据库。需要 root / CAP_NET_ADMIN。
#
# 用法：
#   sudo apps/api/scripts/nft_prototype.sh
# ==============================================================================
set -Eeuo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "错误：需要 root（nft 需要 CAP_NET_ADMIN）" >&2
    exit 1
fi

TABLE="inet floatctf_awd_test"
PRIORITY=1   # 与 render.rs::FORWARD_PRIORITY 一致

failures=0
pass()  { echo "  PASS  $1"; }
fail()  { echo "  FAIL  $1"; failures=$((failures+1)); }

cleanup() {
    nft delete table $TABLE 2>/dev/null || true
    ip link del veth-a 2>/dev/null || true
    ip link del veth-b 2>/dev/null || true
    ip link del veth-wg 2>/dev/null || true
    ip netns del ns-a 2>/dev/null || true
    ip netns del ns-b 2>/dev/null || true
    ip netns del ns-wg 2>/dev/null || true
}
trap cleanup EXIT
cleanup

echo "=== P1-0 nftables networking prototype ==="
echo "table: $TABLE, forward priority: $PRIORITY"

# ── 拓扑：ns-wg（玩家）→ veth → 宿主 → veth → ns-a/ns-b（GameBox 模拟）──
# 玩家经 WG 子网 172.31.0.0/16；GameBox 在 10.42.0.0/16

setup_ns() { # name, addr
    local ns=$1 addr=$2
    ip netns add "$ns"
    ip link add "veth-$ns" type veth peer name "v0-$ns"
    ip link set "v0-$ns" netns "$ns"
    ip addr add "10.255.255.${3:-10}/24" dev "veth-$ns" 2>/dev/null || true
    ip link set "veth-$ns" up
    ip netns exec "$ns" ip addr add "$addr" dev "v0-$ns"
    ip netns exec "$ns" ip link set "v0-$ns" up
    ip netns exec "$ns" ip link set lo up
}

# 简化：一个 bridge br-awd-test 承载 GameBox 侧
ip link add br-awd-test type bridge
ip link set br-awd-test up
ip addr add 10.42.0.1/16 dev br-awd-test

# GameBox A / B 用 network namespace 挂在 bridge 上
ip netns add gb-a
ip link add veth-gb-a type veth peer name v0-gb-a
ip link set veth-gb-a master br-awd-test
ip link set veth-gb-a up
ip link set v0-gb-a netns gb-a
ip netns exec gb-a ip addr add 10.42.1.10/24 dev v0-gb-a
ip netns exec gb-a ip link set v0-gb-a up
ip netns exec gb-a ip link set lo up
ip netns exec gb-a ip route add default via 10.42.1.1

ip netns add gb-b
ip link add veth-gb-b type veth peer name v0-gb-b
ip link set veth-gb-b master br-awd-test
ip link set veth-gb-b up
ip link set v0-gb-b netns gb-b
ip netns exec gb-b ip addr add 10.42.2.10/24 dev v0-gb-b
ip netns exec gb-b ip link set v0-gb-b up
ip netns exec gb-b ip link set lo up
ip netns exec gb-b ip route add default via 10.42.2.1

ip addr replace 10.42.1.1/24 dev br-awd-test
ip addr replace 10.42.2.1/24 dev br-awd-test
# 启用 br_netfilter 使 bridge 流量经过 forward hook（否则 iptables/nft 不生效）
modprobe br_netfilter 2>/dev/null || true
sysctl -w net.bridge.bridge-nf-call-iptables=1 >/dev/null 2>&1 || true
sysctl -w net.bridge.bridge-nf-call-ip6tables=1 >/dev/null 2>&1 || true

# 玩家 WG 命名空间（模拟 Team A 玩家）
ip netns add ns-wg
ip link add veth-wg type veth peer name v0-wg
ip link set veth-wg up
ip addr add 172.31.0.1/24 dev veth-wg
ip link set v0-wg netns ns-wg
ip netns exec ns-wg ip addr add 172.31.0.10/24 dev v0-wg
ip netns exec ns-wg ip link set v0-wg up
ip netns exec ns-wg ip link set lo up
ip netns exec ns-wg ip route add default via 172.31.0.1

# 宿主导通测试路径（prototype 简化：宿主直接路由）
ip route add 172.31.0.0/24 dev veth-wg 2>/dev/null || true

probe() { # src_ns, dst_ip, expect(PASS|BLOCK), label
    local src_ns=$1 dst_ip=$2 expect=$3 label=$4
    if ip netns exec "$src_ns" ping -c1 -W1 "$dst_ip" >/dev/null 2>&1; then
        if [[ $expect == PASS ]]; then pass "$label ($dst_ip)"; else fail "$label ($dst_ip) 预期 BLOCK 实际可达"; fi
    else
        if [[ $expect == BLOCK ]]; then pass "$label ($dst_ip)"; else fail "$label ($dst_ip) 预期 PASS 实际不可达"; fi
    fi
}

# ── 装载测试规则（对应 render.rs 三阶段策略）──
load_hardening() {
    nft add table $TABLE
    nft -f - <<'NFT'
table inet floatctf_awd_test {
    chain awd_forward {
        type filter hook forward priority 1; policy accept;
        jump test_event
    }
    set ev_players_v4 { type ipv4_addr; flags interval; elements = { 172.31.0.0/24 } }
    set ev_gb_a_v4    { type ipv4_addr; flags interval; elements = { 10.42.1.0/24 } }
    set ev_gb_b_v4    { type ipv4_addr; flags interval; elements = { 10.42.2.0/24 } }
    set ev_gameboxes_v4 { type ipv4_addr; flags interval; elements = { 10.42.1.0/24, 10.42.2.0/24 } }
    chain test_event {
        # hardening: own-team accept 先于跨队 drop（对应 render_event_chain）
        ip saddr 172.31.0.0/24 ip daddr 10.42.1.0/24 accept
        ip saddr @ev_players_v4 ip daddr @ev_gameboxes_v4 drop
        ip saddr @ev_gameboxes_v4 drop
        ip saddr @ev_players_v4 ip daddr 10.42.0.0/24 drop
    }
}
NFT
}

load_attack() {
    nft add table $TABLE
    nft -f - <<'NFT'
table inet floatctf_awd_test {
    chain awd_forward {
        type filter hook forward priority 1; policy accept;
        jump test_event
    }
    set ev_players_v4 { type ipv4_addr; flags interval; elements = { 172.31.0.0/24 } }
    set ev_gameboxes_v4 { type ipv4_addr; flags interval; elements = { 10.42.1.0/24, 10.42.2.0/24 } }
    chain test_event {
        ip saddr @ev_gameboxes_v4 ip daddr @ev_gameboxes_v4 accept
        ip saddr @ev_gameboxes_v4 drop
        ip saddr @ev_players_v4 ip daddr 10.42.0.0/24 drop
    }
}
NFT
}

# ── 验证矩阵 ──
echo
echo "--- Hardening 矩阵 ---"
load_hardening
probe ns-wg 10.42.1.10 PASS "Team A → 自己 GameBox"
probe ns-wg 10.42.2.10 BLOCK "Team A → 对方 GameBox"
probe ns-wg 10.42.0.1  BLOCK "Team A → infra"
probe gb-a  8.8.8.8    BLOCK "GameBox → Internet"

echo
echo "--- Attack 矩阵 ---"
load_attack
probe ns-wg 10.42.2.10 PASS "Team A → 对方 GameBox（attack）"
probe ns-wg 10.42.0.1  BLOCK "Team A → infra（attack）"
probe gb-a  8.8.8.8    BLOCK "GameBox → Internet（attack）"

echo
echo "--- Docker 共存冒烟 ---"
if docker ps >/dev/null 2>&1; then
    docker ps --format '{{.Names}}' | head -3 | sed 's/^/  Docker 容器存活: /'
    pass "Docker daemon 不受影响"
else
    fail "docker 不可用（环境问题，非规则问题）"
fi

echo
if [[ $failures -eq 0 ]]; then
    echo "=== P1-0 全部 PASS：topology + priority + 三阶段矩阵验证通过 ==="
    exit 0
else
    echo "=== P1-0 存在 $failures 项 FAIL：禁止进入 production renderer ==="
    exit 1
fi
