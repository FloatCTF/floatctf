# AWD Network Control Plane 重构 — 完成核对（§116 DoD）

日期：2026-08-09 ｜ 分支：awd ｜ 未 push

## DoD 34 条核对

| # | 验收项 | 状态 | 证据 |
|---|--------|------|------|
| 1 | 独立 Platform AWD Networking 页面 | ✅ | apps/web/src/routes/admin/awd/network.tsx |
| 2 | 平台页管理 Pools / WG Port / Endpoint / Host Health | ✅ | Address Pools + WG Settings + Host Status 四区 |
| 3 | 不存在 reusable Network Profile | ✅ | rg NetworkProfile/network_profile 0 命中 |
| 4 | Event 独立 Event Network 页面 | ✅ | awd.$id/network.tsx（UnderlineNav tab） |
| 5 | Event 默认 Automatic allocation | ✅ | PUT 空 body → allocate_automatic |
| 6 | Advanced 才允许手动 CIDR | ✅ | 页面 Advanced 折叠区 + manual 模式 |
| 7 | Draft 不强制占用 network | ✅ | create_awd_event 无网络字段；Configuring 才 allocate |
| 8 | Configuring reserve Event Network | ✅ | allocate 要求 Draft/Configuring |
| 9 | Deploying 前必须有完整 network allocation | ✅ | deploy_event require_by_event_id + lock |
| 10 | Deploy 后 addressing locked | ✅ | locked_at + AWD_NETWORK_LOCKED + 测试 #6 |
| 11 | CIDR 分配并发安全 | ✅ | pg_advisory_xact_lock + 20 并发测试唯一 |
| 12 | 自动/手动都防 overlap | ✅ | 全组合校验 + manual 测试（pool 外/重叠拒绝） |
| 13 | PostgreSQL 使用 CIDR/INET | ✅ | 原生类型 + with-ipnetwork + IMPLICIT CAST |
| 14 | WG port 并发安全且 UNIQUE | ✅ | DB UNIQUE + 并发测试端口唯一 |
| 15 | Team subnet 自动稳定持久 | ✅ | subnet_index（Migration E）+ TeamSubnetAllocator |
| 16 | Team rename 不改变 subnet | ✅ | 测试 #7（幂等 + 重命名不变） |
| 17 | next_gamebox_host 删除 | ✅ | GameBox 重构已删，rg 0 命中 |
| 18 | GameBox IP = subnet + host_offset | ✅ | 既有实现 + domain 测试 |
| 19 | Reset 不改变 IP | ✅ | awd_gamebox_domain reset 测试 |
| 20 | Recovery 不重新分配 CIDR | ✅ | recovery 以 awd_event_networks 为 desired |
| 21 | Ban 不改变 allocation | ✅ | ban_service 仅 suspend peers |
| 22 | Pause 不改变 allocation | ✅ | pause 只改 phase/policy |
| 23 | Finished 不释放 allocation | ✅ | 释放仅在 Archive cleanup 后 |
| 24 | Archive cleanup 成功后才 release | ✅ | release_allocations + 测试 #8 |
| 25 | Docker Network ID 属 Observed | ✅ | awd_runtime_resources（§14） |
| 26 | Firewall 只 native nftables | ✅ | Phase 1 已定 |
| 27 | 不依赖 Docker firewall backend | ✅ | 平台页仅观测 backend |
| 28 | Precheck 实测网络矩阵 | ⚠️ 环境门控 | §62 结构性检查已实现；真实包流矩阵 = Phase 5 E2E / CI-host-network |
| 29 | Network mutation 使 Verified 失效 | ✅ | touch_configuration（既有） |
| 30 | 实体由生成工具产生 | ✅ | db:gen + gen_entities.py post-process（脚本内自动化） |
| 31 | legacy network duplicate fields 删除 | ✅ | rg 0 残留（awd_events 8 字段已删） |
| 32 | 无 Network Profile compatibility layer | ✅ | 无任何兼容层 |
| 33 | 测试全部通过 | ✅ | workspace 223 + IPAM 8（DB-gated）+ 前端 tsc/vite build |
| 34 | 未 push | ✅ | 本地 commit 8 个 |

## 关键交付物

- Migration A-E（27 migrations 合并）：awd_network_settings / awd_event_networks /
  awd_network_allocations / subnet_index + CIDR/INET 类型化 + text→inet/cidr IMPLICIT CAST
- domain/network.rs：NetworkPool / WireGuardPortRange / InfraHostPolicy / TeamSubnetAllocator /
  Ipv4Cidr::to_ipnetwork + 惰性子网迭代（11 个纯函数测试）
- event_network_service：automatic / manual / reallocate / lock / release（事务 + advisory lock）
- team_network_allocator：稳定 subnet_index（修复了多新 team 同 index 的 bug）
- 平台 API（/admin/awd/network + health + allocations）与赛事 API（GET/PUT/reallocate）
- 前端：平台 Networking 页 + Event Network tab（Challenges 风格）
- 测试：awd_network_ipam.rs 8 个 DB-gated（§90-100）

## 遗留（环境门控）

- DoD #28 真实数据面矩阵：apps/api/scripts/nft_prototype.sh（需 root）+ CI-host-network
- 新平台页/赛事网络页的浏览器端到端验证（已启动 dev API 验证路由 401 正常）
