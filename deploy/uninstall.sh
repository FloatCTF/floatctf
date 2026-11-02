#!/usr/bin/env bash
#
# FloatCTF uninstall (Phase 10.9) — 独立卸载脚本，可脱离源码签出运行.
#
# 本脚本安装到 $FCTF_ROOT/uninstall.sh（由 scripts/install.sh 每次成功部署自动安装），
# 必须能在用户删除 Git 签出后独立工作：绝不依赖仓库相对路径 / scripts/install.sh /
# 源码 / mise / cargo / pnpm / git / chore / docs。仅依赖宿主既有工具：
#   systemctl, systemd, docker, docker compose, nft, iptables, ip, wg,
#   usermod/userdel/groupdel, rm/install/find/cp/trap。
#
# 两个模式：
#   sudo $FCTF_ROOT/uninstall.sh            SAFE UNINSTALL —— 移除可运行应用
#                                               （systemd、生产 Compose 容器/赛事资源、API image、
#                                               bootstrap web 资产），但保留可恢复状态：
#                                               data/{postgres,rustfs,caddy,caddy-config}, config/, .env,
#                                               runtime/, logs/, .initialized, 本卸载脚本，
#                                               **以及 frontends/（已安装前端 + registry.json）
#                                               与 frontend.sh（前端管理器）** ——
#                                               这样重新部署能恢复同一套前端集合。
#                                               语义：deploy → safe uninstall → deploy 应恢复相同的
#                                               应用数据与密钥（用户/赛事/数据仍在）。
#                                               安全守卫：数据库会被保留，但 AWD/AWDP 运行时会被销毁。
#                                               若存在**进行中**的 AWD 赛事或 AWDP run（数据库仍在运行
#                                               且可查询），默认拒绝卸载（避免「runtime 没了、库里
#                                               赛事还写着 running」的不一致状态）。确认要拆时用
#                                               --force 跳过守卫。
#   sudo $FCTF_ROOT/uninstall.sh --force    SAFE UNINSTALL + 跳过活跃运行时守卫
#                                               （**数据库仍保留**，进行中的赛事 runtime 会被销毁；
#                                                只应在已确认可以放弃该场次时使用）
#   sudo $FCTF_ROOT/uninstall.sh --purge    PERMANENT 删除全部 FloatCTF 自有数据
#                                               （PG/RustFS 数据、config、secrets、runtime、
#                                               日志、API image/build context、bootstrap web、
#                                               **frontends/ 已安装前端与注册表、frontend.sh**、
#                                               compose、systemd 单元、
#                                               动态赛事资源、sysctl/modules 文件、helper 用户、
#                                               安装根目录、本脚本自身）。需输入确认文本
#                                               "PURGE FLOATCTF"（除非 --yes）。
#
# 共享宿主依赖永不卸载：Docker / docker compose / nftables 包 / wireguard-tools /
# iproute2 / systemd。绝不触碰无关 Docker 对象 / WG 接口 / nftables 状态 / 路由 /
# libvirt / Incus / 其他应用。
#
set -Eeuo pipefail

# 安装根：安装时 install.sh 用 sed 把 __FLOATCTF_HOME__ 固化为实际根；
# 运行时仍可用环境变量 FLOATCTF_HOME 或 FCTF_ROOT 覆盖。
FCTF_ROOT="${FLOATCTF_HOME:-${FCTF_ROOT:-__FLOATCTF_HOME__}}"
FCTF_USER="floatctf"
FCTF_HELPER_USER="floatctf-helper"
HELPER_INSTALL_PATH="/usr/local/libexec/floatctf-helper"

info() { printf '%s[INFO]%s %s\n'  "$(tput setaf 4 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
ok()   { printf '%s[ OK ]%s %s\n'  "$(tput setaf 2 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
warn() { printf '%s[WARN]%s %s\n'  "$(tput setaf 3 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*"; }
die()  { printf '%s[FAIL]%s %s\n'  "$(tput setaf 1 2>/dev/null || true)" "$(tput sgr0 2>/dev/null || true)" "$*" >&2; exit 1; }

# ── 根权限 ────────────────────────────────────────────────────────────────────
require_root() {
    [ "$(id -u)" -eq 0 ] || die "需要 root。请改用: sudo $FCTF_ROOT/uninstall.sh"
}

# ── 工具可用性（宿主既有；缺失则报错，不尝试安装）──────────────────────────────
MODE="safe"
PURGE_YES=0
SAFE_FORCE=0
SELF_TMP=""

usage() {
    cat <<EOF
用法：
  sudo $FCTF_ROOT/uninstall.sh             安全卸载（保留 PG/RustFS 数据、config、secrets）
  sudo $FCTF_ROOT/uninstall.sh --force     安全卸载并跳过「活跃 AWD/AWDP 运行时」守卫
                                           （数据库仍保留；进行中的赛事 runtime 会被销毁）
  sudo $FCTF_ROOT/uninstall.sh --purge     永久删除全部 FloatCTF 自有数据（需确认 PURGE FLOATCTF）
  sudo $FCTF_ROOT/uninstall.sh --purge --yes  跳过确认（仅限非交互 purge）
  sudo $FCTF_ROOT/uninstall.sh --help

说明：
  安全卸载会保留数据库，但会销毁 AWD/AWDP 运行时（容器、赛事网络、WireGuard 接口、
  nftables 表）。若数据库仍可查询且存在进行中的 AWD 赛事 / AWDP run，本脚本默认**拒绝**
  卸载并列出受影响的 id：请先结束或归档这些赛事，或确认放弃后加 --force。
  --purge 会连数据库一起删除，因此不做该守卫，但会打印销毁内容警告。
EOF
}

parse_args() {
    # 用 while 循环而非递归：递归版在参数耗尽时结尾返回非零，
    # 叠加 set -e 会导致脚本静默 exit 1、零输出（--purge 实测复发）。
    while [ "$#" -ge 1 ]; do
        case "$1" in
            --purge) MODE="purge";;
            --yes)   PURGE_YES=1;;
            --force) SAFE_FORCE=1;;
            -h|--help) usage; exit 0 ;;
            *) die "未知参数: $1（--help 查看用法）";;
        esac
        shift
    done
    return 0
}

# ── 自删除安全（§18）：purge 会把安装根（含本脚本）删掉，Bash 不能继续读
#    已删除的自身文件。策略：把本脚本复制到 root 独有的 /tmp/floatctf-uninstall.<pid>.sh，
#    然后 exec 该临时副本续跑（临时副本自身设置 EXIT trap 删除自己，不留特权脚本在 /tmp）。
#    用环境变量 FCTF_UNINSTALL_CONT 标识内部续跑模式，避免无限递归。
INTERNAL_MODE_VAR="FCTF_UNINSTALL_CONT"
SELF_TMP=""

run_purge_via_temp() {
    if [ -n "${!INTERNAL_MODE_VAR:-}" ]; then
        # 已在临时副本内部续跑：让本副本负责清理自身（$0 == /tmp 副本），随后照常执行主体。
        SELF_TMP="$0"
        trap 'rm -f "$SELF_TMP"' EXIT INT TERM
        return 0
    fi
    # 原始进程：复制到 /tmp 后 exec（原进程被替换，其 trap 失效；临时副本接续清理）。
    local src="$0"
    [ -f "$src" ] || src="$FCTF_ROOT/uninstall.sh"
    SELF_TMP="/tmp/floatctf-uninstall.$$.sh"
    install -m 0700 "$src" "$SELF_TMP"
    local yesflag=()
    [ "$PURGE_YES" = "1" ] && yesflag+=(--yes)
    exec env "$INTERNAL_MODE_VAR=1" bash "$SELF_TMP" --purge "${yesflag[@]}"
}

# ============================================================================
# 动态 AWD / AWDP 资源清理（所有权严格限定：只按命名/Label 前缀匹配）
# ============================================================================
# FloatCTF 自有的命名契约（apps/api 源码与数据库确认）：
#   - 赛事 WireGuard 接口     : fawg_<8hex>
#   - 赛事 Docker 桥          : fctfawd<8hex>（docker 网络名 fctf-awd-<8hex>）
#   - 赛事 FlagServer 容器    : fctf-flagserver-<8hex>
#   - 赛事 JudgeServer 容器   : fctf-judgeserver-<8hex>
#   - 赛事 Docker 网络        : fctf-awd-<8hex>
#   - GameBox 容器            : 携带 awd.event_id / awd.resource_kind 标签
#   - 平台 control 网络       : fctf-platform-control
#   - AWDP Docker 网络        : fctf-awdp-practice / fctf-awdp-<12hex>
#     （旧版 fctf-awdp-control 仅作卸载兼容清理）
#   - AWDP JudgeServer 容器   : fctf-awdp-practice-judge / fctf-awdp-judge-<12hex>
#   - nftables 表             : inet floatctf_awd（全局）+ floatctf_awdp_*（AWDP）
#   - Docker 反欺骗放行规则   : 严格按 fawg_ 接口 / fctfawd 桥 / 赛事 CIDR 限定

AWK_NAME_OK='^[A-Za-z0-9_.-]+$'

require_tools() {
    local c
    for c in systemctl docker nft ip wg; do
        if ! command -v "$c" >/dev/null 2>&1; then
            warn "缺少命令: $c（跳过依赖它的清理步骤）"
        fi
    done
    command -v iptables >/dev/null 2>&1 || warn "缺少 iptables（跳过 Docker 反欺骗规则清理）"
}

# 名字是否为 FloatCTF 自有可安全删除的接证对象（拒绝通配/元字符）。
valid_fctf_name() { [[ "$1" =~ ${AWK_NAME_OK} ]] && [[ "$1" == fawg_* || "$1" == fctfawd* || "$1" == fctf-platform-control || "$1" == fctf-awd-* || "$1" == fctf-flagserver-* || "$1" == fctf-judgeserver-* || "$1" == fctf-awdp-* ]]; }

systemctl_stop_units() {
    info "── 停止并停用 FloatCTF systemd 单元 ──"
    # 宿主 systemd 未必在运行（容器/精简宿主）——不存在时按已停止处理。
    if ! command -v systemctl >/dev/null 2>&1; then
        warn "宿主无 systemctl，跳过 systemd 单元操作"
        return
    fi
    systemctl stop floatctf.target 2>/dev/null || true
    systemctl disable floatctf.target floatctf-infra.service floatctf-helper.service 2>/dev/null || true
    systemctl stop floatctf-helper.service floatctf-infra.service 2>/dev/null || true
    # 旧版 native API unit 只作迁移兼容清理。
    systemctl disable --now floatctf-api.service 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
    systemctl reset-failed floatctf.target floatctf-infra.service floatctf-helper.service floatctf-api.service 2>/dev/null || true
    ok "systemd 单元已停止/停用"
}

# 先移除生产 API 容器，从源头停止接受应用流量 + 停止 recover_all 重建动态资源。
stop_api_first() {
    if command -v docker >/dev/null 2>&1; then
        docker rm -f floatctf-api >/dev/null 2>&1 || true
    fi
    # 旧版 native API unit / binary 只作迁移兼容清理。
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop floatctf-api.service 2>/dev/null || true
    fi
    if [ -x "$FCTF_ROOT/bin/floatctf" ]; then
        pkill -f "^$FCTF_ROOT/bin/floatctf" 2>/dev/null || true
    fi
}

cleanup_gameboxes() {
    info "── 清理 GameBox 容器（依据 awd.* 标签，所有权限定）──"
    # 只删携带 FloatCTF awd 标签的容器；无标签者绝不触碰。
    local ids
    ids=$(docker ps -aq --filter 'label=awd.resource_kind' 2>/dev/null | tr '\n' ' ')
    [ -n "${ids// /}" ] || { ok "无 GameBox 容器"; return; }
    # 双重校验：每个 id 必须持有 awd.resource_kind 标签才删（防御竞态）。
    for id in $ids; do
        [ -n "$id" ] || continue
        [ -n "$(docker inspect -f '{{ index .Config.Labels "awd.resource_kind" }}' "$id" 2>/dev/null)" ] || { warn "容器 $id 无 awd.resource_kind 标签，跳过"; continue; }
        docker rm -f "$id" >/dev/null 2>&1 || warn "删除容器 $id 失败（忽略）"
        ok "已删除 GameBox 容器 $id"
    done
}

cleanup_awd_named_containers() {
    info "── 清理赛事 FlagServer/JudgeServer 与 AWDP JudgeServer 容器（精确名字前缀）──"
    local pat c x
    for c in fctf-flagserver- fctf-judgeserver- fctf-awdp-practice-judge fctf-awdp-judge-; do
        pat="${c}*"
        while read -r x; do
            [ -n "$x" ] || continue
            if [[ "$x" =~ ${AWK_NAME_OK} ]] && [[ "$x" == "$c"* ]]; then
                docker rm -f "$x" >/dev/null 2>&1 \
                    && ok "已删除容器 $x" || { [ -z "$(docker ps -aq --filter name="^${x}$" 2>/dev/null)" ] && ok "容器 $x 已不存在" || warn "删除容器 $x 失败（忽略）"; }
            else
                warn "容器名不匹配 FloatCTF 契约，跳过: $x"
            fi
        done < <(docker ps -a --filter "name=^$pat" --format '{{.Names}}' 2>/dev/null)
    done
    ok "赛事/AWDP 命名容器清理完成"
}

cleanup_docker_networks() {
    info "── 清理赛事 Docker 网络（名字前缀限定）──"
    local pat c x
    for c in fctf-platform-control fctf-awd- fctf-awdp-practice fctf-awdp-control fctf-awdp-; do
        pat="${c}*"
        while read -r x; do
            [ -n "$x" ] || continue
            if [[ "$x" =~ ${AWK_NAME_OK} ]] && [[ "$x" == "$c"* ]]; then
                # 网络可能仍有容器相连：先断开本平台容器再删。
                docker network rm "$x" >/dev/null 2>&1 \
                    && ok "已删除网络 $x" || warn "删除网络 $x 失败（可能仍有连接，忽略）"
            else
                warn "网络名不匹配 FloatCTF 契约，跳过: $x"
            fi
        done < <(docker network ls --format '{{.Name}}' 2>/dev/null | grep -E "^${c}[A-Za-z0-9_.-]*$")
    done
    ok "赛事 Docker 网络清理完成"
}

cleanup_wireguard() {
    info "── 清理赛事 WireGuard 接口（fawg_ 前缀限定）──"
    command -v ip >/dev/null 2>&1 || { warn "无 ip 命令，跳过 WG 接口清理"; return; }
    local iface
    for iface in $(ip -o link sh 2>/dev/null | awk -F': ' '{print $2}' | tr -d ' '); do
        [ -n "$iface" ] || continue
        [[ "$iface" =~ ^fawg_[0-9a-f]{8}$ ]] || continue
        ip link del "$iface" >/dev/null 2>&1 && ok "已删除 WG 接口 $iface" || warn "删除 WG 接口 $iface 失败（忽略）"
    done
    ok "赛事 WireGuard 接口清理完成（无关接口 wg0 等未触碰）"
}

# 删除 iptables 里 FloatCTF 自身的 Docker 反欺骗放行规则（严格按 fawg_ 接口限定）。
cleanup_iptables_docker_forward() {
    command -v iptables >/dev/null 2>&1 || { warn "无 iptables，跳过 Docker 反欺骗规则清理"; return; }
    info "── 清理 Docker 反欺骗放行规则（仅限 iifname=fawg_* 的 FloatCTF 规则）──"
    local line spec
    # raw PREROUTING 与 filter DOCKER-USER 中 -i fawg_*/fctfawd* 且 -j ACCEPT 的行
    for table in raw filter; do
        for chain in PREROUTING DOCKER-USER; do
            while read -r line; do
                [ -n "$line" ] || continue
                # 转成 -D 删除
                spec=${line#-A }
                [[ "$spec" == *" -i fawg_"* || "$spec" == *" -i fctfawd"* ]] || continue
                [[ "$spec" == *" -j ACCEPT" ]] || continue
                iptables -t "$table" -D $spec >/dev/null 2>&1 \
                    && ok "已删除规则 [$table $spec]" || warn "删除规则 [$table $spec] 失败（忽略）"
            done < <(iptables -t "$table" -S "$chain" 2>/dev/null | grep '^-A ' || true)
        done
    done
    ok "Docker 反欺骗放行规则清理完成"
}

cleanup_nftables() {
    info "── 清理 FloatCTF 自有 nftables 表（仅 floatctf_awd / floatctf_awdp_*）──"
    command -v nft >/dev/null 2>&1 || { warn "无 nft，跳过 nftables 清理"; return; }
    local fam table
    # nft list tables 输出形如 `table inet floatctf_awd`（每行一个）。
    while read -r fam table; do
        [ -n "$table" ] || continue
        case "$table" in
            floatctf_awd)
                nft delete table "$fam" "$table" >/dev/null 2>&1 && ok "已删除表 $fam $table" || warn "删除表 $fam $table 失败（忽略）"
                ;;
            floatctf_awdp_*)
                nft delete table "$fam" "$table" >/dev/null 2>&1 && ok "已删除表 $fam $table" || warn "删除表 $fam $table 失败（忽略）"
                ;;
            *) warn "非 FloatCTF 表，跳过: $fam $table" ;;
        esac
    done < <(nft list tables 2>/dev/null | sed -n 's/^table \([a-z]*\) \(.*\)$/\1 \2/p')
    ok "nftables 清理完成（未 flush ruleset，未触碰无关表）"
}

# ============================================================================
# 生产 Compose 容器（API / postgres / redis / rustfs / caddy）
# ============================================================================
stop_infra_containers() {
    info "── 停止/移除生产 Compose 容器（保护 bind-mount 数据）──"
    if [ -f "$FCTF_ROOT/compose.prod.yml" ] && [ -d "$FCTF_ROOT" ]; then
        # 用系统 docker compose 插件；无 -> 尝试 docker-compose。
        ( cd "$FCTF_ROOT" \
            && { docker compose -f compose.prod.yml down 2>/dev/null \
                 || docker compose -f compose.prod.yml stop 2>/dev/null \
                 || docker stop floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy 2>/dev/null || true; } ) \
            && ok "生产容器已停止/移除（持久数据保留）"
    else
        warn "未找到 $FCTF_ROOT/compose.prod.yml，跳过 compose down；尝试按名字精确停止"
        docker stop floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy 2>/dev/null || true
    fi
    # 兜底：强制移除（精确名字，防误删无关容器）
    local c
    for c in floatctf-api floatctf-postgres floatctf-redis floatctf-rustfs floatctf-caddy; do
        if [ -n "$(docker ps -aq --filter name="^${c}$" 2>/dev/null)" ]; then
            docker rm -f "$c" >/dev/null 2>&1 && ok "已移除容器 $c" || warn "移除容器 $c 失败（忽略）"
        fi
    done
}

# ============================================================================
# 活跃 AWD/AWDP 运行时守卫
# ============================================================================
#
# 背景：safe_uninstall **刻意保留数据库**，却会删除全局 nft 表 floatctf_awd、
# 所有 fawg_* 接口、所有 fctf-awd-*/fctf-awdp-* 网络与 FlagServer/JudgeServer
# 容器。若此时赛事正在进行，结果就是「runtime 没了、库里赛事还是 running」的
# 不一致状态：恢复后既看不到成绩也无法续赛。因此默认拒绝，只允许显式 --force。
#
# 判据（只读，绝不改库）：
#   AWD  进行中 = awd_events.status NOT IN
#                 ('draft','configuring','finished','archived','deploy_failed','verification_failed')
#   AWDP 进行中 = awdp_runs.finished_at IS NULL
# 表/列名对照 apps/api/src/entity/awd_events.rs（table_name="awd_events"，字段
# status: AwdEventStatus）与 apps/api/src/entity/awdp_runs.rs
# （table_name="awdp_runs"，字段 finished_at: Option<DateTimeWithTimeZone>）。

# 只读 SQL：以容器内 POSTGRES_USER/POSTGRES_DB 连接；SQL 通过位置参数传入，
# 避免在本脚本里嵌套引号。任何失败（容器不在、库未初始化、schema 缺失）
# 都以非零返回，由调用方决定如何降级。
pg_readonly_query() { # sql
    ( cd "$FCTF_ROOT" \
        && docker compose -f compose.prod.yml exec -T postgres \
            sh -c 'psql -X -q -A -t -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "$1"' _ "$1" 2>/dev/null )
}

# 探测活跃运行时。返回码：
#   0 = 存在进行中的 AWD/AWDP 运行时（详情已打印）
#   1 = 确无进行中的运行时
#   2 = 无法判定（已 warn；数据库不可查询时按"无活跃"处理，不阻断卸载）
probe_active_runtime() {
    if [ ! -f "$FCTF_ROOT/compose.prod.yml" ] || [ ! -f "$FCTF_ROOT/.env" ]; then
        warn "无法判定活跃赛事状态：缺少 $FCTF_ROOT/compose.prod.yml 或 $FCTF_ROOT/.env"
        return 2
    fi
    if ! command -v docker >/dev/null 2>&1; then
        warn "无法判定活跃赛事状态：无 docker 命令"
        return 2
    fi
    local cid
    cid="$( cd "$FCTF_ROOT" && docker compose -f compose.prod.yml ps -q postgres 2>/dev/null | head -1 )" || cid=""
    if [ -z "$cid" ]; then
        warn "PostgreSQL 未运行：无法判定活跃赛事状态（平台已停 -> 无运行中赛事）"
        return 2
    fi

    local awd_count awdp_count
    if ! awd_count="$(pg_readonly_query "SELECT count(*) FROM awd_events WHERE status NOT IN ('draft','configuring','finished','archived','deploy_failed','verification_failed')")" \
        || [ -z "$awd_count" ]; then
        warn "无法查询 AWD 赛事状态（数据库不可达或 schema 未初始化）"
        return 2
    fi
    if ! awdp_count="$(pg_readonly_query "SELECT count(*) FROM awdp_runs WHERE finished_at IS NULL")" \
        || [ -z "$awdp_count" ]; then
        warn "无法查询 AWDP run 状态（数据库不可达或 schema 未初始化）"
        return 2
    fi

    case "$awd_count" in ''|*[!0-9]*) warn "AWD 赛事计数异常: $awd_count"; return 2 ;; esac
    case "$awdp_count" in ''|*[!0-9]*) warn "AWDP run 计数异常: $awdp_count"; return 2 ;; esac
    if [ "$awd_count" -eq 0 ] && [ "$awdp_count" -eq 0 ]; then
        return 1
    fi

    echo ""
    echo "!!!! 检测到进行中的 AWD/AWDP 运行时 !!!!"
    if [ "$awd_count" -gt 0 ]; then
        echo "  AWD 赛事（进行中）: $awd_count 个"
        echo "    id: $(pg_readonly_query "SELECT string_agg(id::text, ', ') FROM awd_events WHERE status NOT IN ('draft','configuring','finished','archived','deploy_failed','verification_failed')" 2>/dev/null || true)"
    fi
    if [ "$awdp_count" -gt 0 ]; then
        echo "  AWDP run（未结束）: $awdp_count 个"
        echo "    id: $(pg_readonly_query "SELECT string_agg(id::text, ', ') FROM awdp_runs WHERE finished_at IS NULL" 2>/dev/null || true)"
    fi
    echo ""
    return 0
}

# 守卫：存在活跃运行时时返回非零（并解释后果）。无法判定时放行（只 warn）。
active_runtime_guard() {
    local rc=0
    probe_active_runtime || rc=$?
    case "$rc" in
        0)
            echo "原因：数据库会被保留，但 AWD/AWDP 运行时（FlagServer/JudgeServer/GameBox 容器、"
            echo "      赛事 Docker 网络、fawg_* WireGuard 接口、floatctf_awd* nft 表）会被销毁 ——"
            echo "      这会留下「赛事仍在 running / run 未结束，但 runtime 已消失」的不一致状态。"
            echo ""
            echo "处理：先结束或归档这些赛事（管理端结束赛事 / 结束 AWDP run），再重新运行卸载；"
            echo "      若确认放弃该场次，用 sudo $FCTF_ROOT/uninstall.sh --force"
            return 1
            ;;
        2)  return 0 ;;
        *)  return 0 ;;
    esac
}

remove_api_images() {
    info "── 移除 FloatCTF API runtime images ──"
    local id
    while read -r id; do
        [ -n "$id" ] || continue
        if [ "$(docker image inspect -f '{{ index .Config.Labels "io.floatctf.managed" }}' "$id" 2>/dev/null)" = "true" ]; then
            docker image rm -f "$id" >/dev/null 2>&1 || warn "删除 API image $id 失败（忽略）"
        else
            warn "floatctf/api image $id 无 managed label，跳过"
        fi
    done < <(docker image ls --filter 'reference=floatctf/floatctf-api:*' -q 2>/dev/null | sort -u)
    ok "API runtime image 清理完成"
}

remove_application_artifacts() {
    info "── 移除可运行应用产物（保留 data/config/.env/runtime/logs 与已安装前端）──"
    # 刻意**不**删除：$FCTF_ROOT/frontends（已安装前端 + registry.json）与
    # $FCTF_ROOT/frontend.sh。它们是运维/第三方投入的"已安装状态"，
    # 保留才能让"卸载 → 重新部署"恢复同一套前端集合（见 docs/frontend/ARCHITECTURE.md）。
    # image/ 是生产 API build context；bin/ 仅为旧版 native API 兼容清理。
    local p
    for p in "$FCTF_ROOT/image" "$FCTF_ROOT/bin" "$FCTF_ROOT/web" "$FCTF_ROOT/compose.dev.yml" "$FCTF_ROOT/compose.prod.yml" "$FCTF_ROOT/merged.sql" "$HELPER_INSTALL_PATH"; do
        if [ -e "$p" ] || [ -L "$p" ]; then
            rm -rf -- "$p" && ok "已移除 $p" || warn "移除 $p 失败（忽略）"
        fi
    done
}

# ============================================================================
# 主机初始化文件（purge 用）
# ============================================================================
SYSCTL_FILE="/etc/sysctl.d/99-floatctf.conf"
MODULES_FILE="/etc/modules-load.d/floatctf-br-netfilter.conf"

# ============================================================================
# SAFE UNINSTALL
# ============================================================================
safe_uninstall() {
    info "==== 安全卸载（保留可恢复状态）===="
    if [ ! -d "$FCTF_ROOT" ]; then
        info "$FCTF_ROOT 不存在 —— 已是未安装状态"
        ok "FloatCTF 已卸载（或从未安装）。"
        return
    fi

    # 0. 活跃运行时守卫 —— 必须排在**任何**拆除动作（含 stop_api_first）之前：
    #    数据库会被保留，但运行时会消失，进行中的赛事/run 会变成不一致状态。
    if [ "$SAFE_FORCE" = "1" ]; then
        warn "--force：跳过活跃运行时守卫（数据库保留；进行中的赛事 runtime 会被销毁）"
    elif ! active_runtime_guard; then
        die "检测到进行中的 AWD/AWDP 运行时，已拒绝安全卸载（未改动任何内容）；用 --force 强制卸载"
    fi

    # 1. 先停 API（停止接受流量 + 停止 recover_all 重建动态资源）
    stop_api_first
    # 2. 动态 AWD/AWDP 资源所有权清理
    require_tools
    cleanup_gameboxes
    cleanup_awd_named_containers
    cleanup_docker_networks
    cleanup_wireguard
    cleanup_iptables_docker_forward
    cleanup_nftables
    # 3. systemd（stop/disable/daemon-reload/reset-failed）
    systemctl_stop_units
    # 4. infra 容器（保数据）
    stop_infra_containers
    # 5. 移除可再生 API image 与应用产物
    remove_api_images
    remove_application_artifacts

    ok "FloatCTF 已卸载。"

    cat <<EOF

保留的数据（可恢复）:
  PostgreSQL 数据: $FCTF_ROOT/data/postgres
  RustFS 数据   : $FCTF_ROOT/data/rustfs
  Caddy 证书数据: $FCTF_ROOT/data/caddy
  配置/密钥      : $FCTF_ROOT/config 与 $FCTF_ROOT/.env
  运行时工作目录 : $FCTF_ROOT/runtime
  日志          : $FCTF_ROOT/logs
  已安装前端      : $FCTF_ROOT/frontends（含 registry.json，第三方前端与版本一并保留）
  前端管理器      : $FCTF_ROOT/frontend.sh

重新安装:
  运行 scripts/install.sh（会恢复相同数据与密钥，API 启动时自动重建 AWD 动态资源）。

完全删除:
  sudo $FCTF_ROOT/uninstall.sh --purge

本卸载脚本 $FCTF_ROOT/uninstall.sh 保留，作为生命周期/恢复工具（勿删）。
EOF
}

# ============================================================================
# PURGE
# ============================================================================
purge_confirm() {
    [ "$PURGE_YES" = "1" ] && return 0
    echo ""
    echo "!!!! 危险操作 !!!!"
    echo "你将永久删除全部 FloatCTF 自有数据，包括:"
    echo "  - PostgreSQL 数据        : $FCTF_ROOT/data/postgres"
    echo "  - RustFS 数据            : $FCTF_ROOT/data/rustfs"
    echo "  - Caddy 证书/账户状态    : $FCTF_ROOT/data/caddy"
    echo "  - 配置 / 密钥            : $FCTF_ROOT/config, $FCTF_ROOT/.env"
    echo "  - API image/build context / web / compose / runtime / 日志"
    echo "  - systemd 单元            floatctf-{helper,infra}.service, floatctf.target（含旧 api unit 兼容清理）"
    echo "  - 动态赛事资源            GameBox/FlagServer/JudgeServer 容器、赛事网络、"
    echo "                             WireGuard 接口、nftables 表、转发规则"
    echo "  - sysctl/modules 文件      /etc/sysctl.d/99-floatctf.conf,"
    echo "                              /etc/modules-load.d/floatctf-br-netfilter.conf"
    echo "  - floatctf / floatctf-helper 服务用户"
    echo "  - 安装根目录               $FCTF_ROOT（含本卸载脚本）"
    echo ""
    echo "此操作不可撤销。如要继续，请输入: PURGE FLOATCTF"
    read -r -p "> " ans || die "中断：未确认，已中止。"
    [ "$ans" = "PURGE FLOATCTF" ] || die "确认文本不匹配，已中止（未删除任何内容）。"
}

purge_remove_dynamic() {
    # 与 safe 阶段复用同一套所有权清理，确保浮动的赛事资源也被清除。
    require_tools
    cleanup_gameboxes
    cleanup_awd_named_containers
    cleanup_docker_networks
    cleanup_wireguard
    cleanup_iptables_docker_forward
    cleanup_nftables
}

purge_remove_systemd_units() {
    info "── 移除 FloatCTF systemd 单元 ──"
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop floatctf.target floatctf-api.service floatctf-helper.service floatctf-infra.service 2>/dev/null || true
        systemctl disable floatctf.target floatctf-api.service floatctf-helper.service floatctf-infra.service 2>/dev/null || true
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-helper.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        systemctl daemon-reload 2>/dev/null || true
        systemctl reset-failed floatctf.target floatctf-infra.service floatctf-helper.service floatctf-api.service 2>/dev/null || true
        ok "FloatCTF systemd 单元已移除"
    else
        rm -f /etc/systemd/system/floatctf-api.service \
              /etc/systemd/system/floatctf-helper.service \
              /etc/systemd/system/floatctf-infra.service \
              /etc/systemd/system/floatctf.target
        ok "无 systemctl，直接移除单元文件"
    fi
}

purge_remove_sysctl_modules() {
    info "── 移除 FloatCTF 自有 sysctl / modules-load 文件 ──"
    # 仅删除 FloatCTF 自有文件片段；绝不整体关闭 IPv4 转发或 br_netfilter
    # （可能已被其他宿主负载依赖）。删除持久化文件后 reload 内核参数是可选的，
    # 这里不自动关闭任何内核特性，只清理持久化声明。
    local removed=0
    if [ -f "$SYSCTL_FILE" ]; then
        rm -f "$SYSCTL_FILE" && { removed=1; ok "已移除 $SYSCTL_FILE"; }
    fi
    if [ -f "$MODULES_FILE" ]; then
        rm -f "$MODULES_FILE" && { removed=1; ok "已移除 $MODULES_FILE"; }
    fi
    # 不自动 sysctl -w 关闭转发/br_netfilter：其他负载可能依赖；文档已说明此取舍。
    # 用 if 而非 `[ ... ] && ok`：removed=1 时后者返回非零，叠加 set -e 会在
    # 删除安装根/宿主身份之前就退出（历史 purge 曾观察到目录与账号残留）。
    if [ "$removed" = "0" ]; then
        ok "无 FloatCTF sysctl/modules 文件（或已不存在）"
    fi
}

purge_remove_user() {
    info "── 移除 FloatCTF 宿主身份（helper 用户 + 历史账号 + 组）──"
    if id "$FCTF_HELPER_USER" >/dev/null 2>&1; then
        userdel "$FCTF_HELPER_USER" 2>/dev/null \
            && ok "已移除用户 $FCTF_HELPER_USER" \
            || warn "userdel $FCTF_HELPER_USER 失败（可能仍有进程占用）"
    else
        ok "用户 $FCTF_HELPER_USER 不存在"
    fi

    # v2 起安装器不再创建 floatctf 用户（容器用数值 uid，宿主不需要该身份）。
    # 若存在，只可能是历史安装或 dev setup 残留：确认是 system 账号 + nologin +
    # home 为安装根（或 v2 之前的默认根 /home/floatctf）才删除，避免误删同名账号。
    if id "$FCTF_USER" >/dev/null 2>&1; then
        local home shell
        home=$(getent passwd "$FCTF_USER" | cut -d: -f6)
        shell=$(getent passwd "$FCTF_USER" | cut -d: -f7)
        if { [ "$home" = "$FCTF_ROOT" ] || [ "$home" = "/home/floatctf" ]; } \
            && [ "$shell" = "/usr/sbin/nologin" ]; then
            userdel -r "$FCTF_USER" 2>/dev/null && ok "已移除历史用户 $FCTF_USER" \
                || { warn "userdel $FCTF_USER 失败（可能仍有进程占用 $FCTF_ROOT/runtime）"; \
                     # 回退：仅移除配置但保留记录，避免误删
                     warn "保留用户记录；请确认无 floatctf 进程后重试 userdel -r floatctf"; }
        else
            warn "账户 $FCTF_USER 不匹配预期（home=$home shell=$shell），跳过删除"
        fi
    else
        ok "用户 $FCTF_USER 不存在（v2 起不再创建）"
    fi

    if getent group "$FCTF_USER" >/dev/null 2>&1; then
        groupdel "$FCTF_USER" 2>/dev/null \
            && ok "已移除系统组 $FCTF_USER" \
            || warn "groupdel $FCTF_USER 失败（可能仍被账户使用）"
    fi
    # 绝不删除 docker 组或无关用户。
}

purge_run() {
    info "==== 永久删除（purge）===="
    purge_confirm

    # purge 连数据库一起删除，因此**不需要**守卫（不存在"runtime 没了、库里还在"的
    # 不一致状态）；但进行中的赛事意味着正在进行的比赛及其全部数据被销毁，
    # 必须显式、醒目地警告（不额外加交互确认，既有 purge_confirm 已足够）。
    if probe_active_runtime; then
        warn "════════════════════════════════════════════════════════════════"
        warn "注意：存在进行中的 AWD/AWDP 运行时 —— purge 将**永久销毁**以下内容："
        warn "  - 全部 PostgreSQL 数据（赛事/题目/用户/成绩，不可恢复）"
        warn "  - FlagServer / JudgeServer / GameBox 容器与 AWDP 判题容器"
        warn "  - 赛事 Docker 网络（fctf-awd-* / fctf-awdp-*）、fawg_* WireGuard 接口"
        warn "  - floatctf_awd* nftables 表与 Docker 反欺骗放行规则"
        warn "  - RustFS 数据、配置与密钥（$FCTF_ROOT/config、$FCTF_ROOT/.env）"
        warn "════════════════════════════════════════════════════════════════"
    fi

    # 先移除生产 API，阻止 recovery/scheduler 继续重建资源。
    stop_api_first
    # 动态赛事资源（所有权限定）
    purge_remove_dynamic
    # systemd 单元
    purge_remove_systemd_units
    # 生产容器（保数据到最后一刻：purge 会紧接着删除数据目录）
    stop_infra_containers
    remove_api_images
    # 主机初始化文件
    purge_remove_sysctl_modules
    # 删除安装根目录（含 PG/RustFS 数据、config、secrets、image、web、runtime、日志、
    # compose、.initialized、本脚本）。用临时文件夹承载 FCTF_ROOT 以彻底删除，随后遗留
    # 的空父目录不删（可能为系统原有 /home）。
    if [ -e "$FCTF_ROOT" ]; then
        rm -rf -- "$FCTF_ROOT" && ok "已删除安装根目录 $FCTF_ROOT" \
            || warn "删除 $FCTF_ROOT 部分失败（检查权限）"
    else
        ok "安装根目录 $FCTF_ROOT 已不存在"
    fi
    # 服务用户
    purge_remove_user

    ok "FloatCTF purge 完成。"
    echo ""
    echo "遗留检查："
    echo "  - 未曾触碰共享宿主依赖（Docker / compose / nftables 包 / WG 包 / iproute2 / systemd）。"
    echo "  - 已删除全部已安装前端与注册表（$FCTF_ROOT/frontends）以及前端管理器 frontend.sh。"
    echo "  - 未曾触碰无关 Docker 对象 / WG 接口 / nftables 规则 / 路由 / libvirt / Incus。"
    echo "  - 如需关闭 IPv4 转发 / br_netfilter，请手动评估（可能被其他负载依赖）。"
    echo ""
    echo "重新初始化并部署（全新安装）: "
    echo "  sudo ./scripts/install.sh"
}

# ── 主流程 ────────────────────────────────────────────────────────────────────
main() {
    parse_args "$@"
    # 无论哪种模式都必须 root；purge 在复制到 /tmp 前就校验，避免无谓复制。
    require_root

    if [ "$MODE" = "purge" ]; then
        # 自删除安全：在删除安装根（含自身）之前，把脚本复制到 /tmp 免责续跑。
        # 续跑模式里该函数只设置 EXIT trap 并返回；否则 exec 已替换当前进程（不返回）。
        run_purge_via_temp
        purge_run
    else
        safe_uninstall
    fi
}

# 本文件由 install.sh 写出到 $FCTF_ROOT/uninstall.sh 后就地执行。
main "$@"
