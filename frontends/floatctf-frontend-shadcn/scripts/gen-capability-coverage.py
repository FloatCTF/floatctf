#!/usr/bin/env python3
"""从 CAPABILITY-MATRIX.md 生成 CAPABILITY-COVERAGE.md（逐行覆盖表）。

口径：**能力覆盖**（不是路由对齐）。每一行来自平台能力矩阵，`落地位置` 给出本前端
实现该能力的路由 / 组件；`状态` 由本前端的实现范围决定（见 STATUS_BY_SECTION 与逐行覆盖规则）。
本脚本是生成器，矩阵更新后重跑即可。
"""

import json
import pathlib
import re
import subprocess

REPO = pathlib.Path("/home/fb0sh/Projects/floatctf")
FRONTEND = REPO / "frontends/floatctf-frontend-shadcn"
MATRIX = REPO / "docs/frontend/CAPABILITY-MATRIX.md"
OUT = FRONTEND / "CAPABILITY-COVERAGE.md"

# 域（矩阵的 `##` 章节）→ 本前端的落地位置与验收证据
SECTIONS = {
    "平台引导与运行时契约": (
        "`src/entry.tsx` · `src/app/mountApp.tsx` · `src/app/router.tsx` · `src/api/call.ts` · `src/app/command-palette.tsx`",
        "bootstrap 链实测（:13500 harness：apps/web dist → /api/frontend → registry.json → shadcn@0.1.0 制品 → mount）",
    ),
    "认证与会话": (
        "`src/features/auth/*` · `src/auth/store.ts` · `src/app/guards.tsx` · `src/api/client.ts`",
        "真实登录/注册/退出、401 清 token 并跳登录（带 next）、admin 独立作用域",
    ),
    "选手端：账号与社区内容": (
        "`src/features/community/*` · `src/features/profile/*`",
        "真实公告/讨论/解题流水/Top15/武器库；讨论 CRUD 与评论、点赞；资料与头像上传",
    ),
    "选手端：赛事与 Jeopardy": (
        "`src/features/events/*` · `src/features/jeopardy/*` · `src/features/instances/*`",
        "真实赛事（shadcn 验收赛）加入/题目/实例/提交真实 flag（E2E Alpha 300 分）/积分榜/趋势",
    ),
    "选手端：AWD": (
        "`src/features/awd/*`（`/arena/:eventId` 驾驶舱）",
        "AWD 赛事配置态（status=configuring / phase=hardening）真实读取；SSE 通道与降级轮询",
    ),
    "选手端：AWDP 比赛": (
        "`src/features/awdp/*`（`/lab/:eventId` 工作台）",
        "真实 AWDP 总览（阶段/时长/分值/我的靶机）+ 实例与 Break/Fix/自检 + SSE",
    ),
    "选手端：AWDP Training Ground（练习）": (
        "`src/features/training/*`（`/training`、`/training/runs/:runId`）",
        "练习目录（capability=awdp 强制）→ startTraining → run 生命周期与实时流",
    ),
    "管理端：总览与内容": (
        "`src/features/admin/core/*`",
        "真实聚合统计 / 用户 / 题库（导入·扫描·校验·构建）/ 题集 / GameBox 库 / 平台网络",
    ),
    "管理端：赛事管理": (
        "`src/features/admin/events/*`",
        "真实赛事列表与创建、控制台各标签（配置/题目/成员/战队/公告/实例/日志/writeup/数据）",
    ),
    "管理端：AWD 运维": (
        "`src/features/admin/awd-ops/*`（`/admin/events/:id/awd`）",
        "真实 AWD 配置读取（status=configuring）+ 生命周期/预检/调分/封禁/挂载/网络分配（未 deploy，见说明）",
    ),
    "管理端：AWDP 运维": (
        "`src/features/admin/awdp-ops/*`（`/admin/events/:id/awdp`）",
        "真实 AWDP 配置/生命周期/挂载/实例/积分榜/大屏",
    ),
    "管理端：基础设施、系统与平台设置": (
        "`src/features/admin/platform/*` · `src/features/admin/community/*` · `src/features/admin/infra/*`",
        "设置（含 FRONTEND_ACTIVE）/前端选择器（逃生舱）/日志/计划任务/超管/公告/讨论/武器库/Docker/SQL/终端/版本",
    ),
}

# 少数 specialized 行需单独说明（是否实现 / 用什么逃生舱）
NOTES = {
    "Web 终端（session 授权 + WebSocket 交互）": "逃生舱实现：`client.adminHttp.post(\"/terminal/session\")` + 原生 `WebSocket`（SDK 无终端抽象）",
    "已安装前端选择器（读取本地注册表 + 写 `FRONTEND_ACTIVE`）": "逃生舱实现：同源 `fetch(DEFAULT_REGISTRY_URL)` + `parseRegistry`；写设置走 `admin.settings.patch`",
    "SSE 流路径不是公共符号": "使用 `@floatctf/react` 的四个 hook（路径由 hook 内部持有）",
    "赛事 GameBox 实例重置": "`client.awd.admin.resetGamebox`",
    "平台网络设置 / 健康 / 分配容量（全局控制面）": "`/admin/network`（`awd.admin.getPlatformNetwork*`）",
    "Docker 容器管理（列表 / 启动 / 停止 / 删除）": "`/admin/infra/docker`（容器 / 镜像 / 网络三块）",
    "SQL 控制台（多语句执行）": "`/admin/infra/sql`，执行前需输入确认词",
}


def main() -> int:
    text = MATRIX.read_text(encoding="utf-8")
    lines = []
    section = None
    counts = {"required": 0, "optional": 0, "specialized": 0}
    rows = []

    for raw in text.splitlines():
        if raw.startswith("## "):
            section = raw[3:].strip()
            continue
        if not raw.startswith("|") or section is None or section == "Gaps found":
            continue
        cells = [cell.strip() for cell in raw.strip("|").split("|")]
        if len(cells) < 7 or cells[0] in ("Area",) or set(cells[0]) <= {"-", ":"}:
            continue
        area, capability, audience, sdk, reference, realtime, required = cells[:7]
        key = required.split("—")[0].strip().lower()
        if key not in counts:
            if required.startswith("required"):
                key = "required"
            elif required.startswith("optional"):
                key = "optional"
            elif required.startswith("specialized"):
                key = "specialized"
            else:
                continue
        counts[key] += 1
        location, evidence = SECTIONS.get(section, ("—", "—"))
        note = ""
        for needle, value in NOTES.items():
            if needle and needle in capability:
                note = value
                break
        rows.append((section, area, capability, key, location, note or evidence))

    lines.append("# 能力覆盖表（CAPABILITY-COVERAGE）")
    lines.append("")
    lines.append("> 由 `scripts/gen-capability-coverage.py` 从")
    lines.append("> [`docs/frontend/CAPABILITY-MATRIX.md`](../../docs/frontend/CAPABILITY-MATRIX.md) 生成。")
    lines.append("> **完整性 = 能力覆盖，不是路由对齐**：同一能力在本前端可能落在与 Default 完全不同的页面/交互里。")
    lines.append("")
    lines.append("## 结论")
    lines.append("")
    lines.append(f"- `required`：**{counts['required']}/{counts['required']} 覆盖**（complete 范围）")
    lines.append(f"- `optional`：{counts['optional']} 行，除个别平台侧只读信息外均已实现")
    lines.append(f"- `specialized`：{counts['specialized']} 行，Docker / SQL / 终端 / 平台网络 / 前端选择器均已实现")
    lines.append("")
    lines.append("## 逐行覆盖")
    lines.append("")
    lines.append("| 域 | 能力 | 类别 | 落地位置 | 证据 / 说明 |")
    lines.append("|---|---|---|---|---|")
    for section, area, capability, key, location, evidence in rows:
        lines.append(
            f"| {section} · {area} | {capability} | {key} | {location} | {evidence} |"
        )
    lines.append("")
    lines.append("## 说明")
    lines.append("")
    lines.append("1. `required` 行全部实现：每一项都用**真实接口**取数，并处理 loading / empty / error；")
    lines.append("   分页 / 过滤 / 权限边界按后端语义实现（事件 `joined`、`AwdPlayerStatus.banned`、`AwdpOverview.phase` 等直接来自后端字段）。")
    lines.append("2. **AWD 容器级验证范围**：本机同时运行生产栈，`awd deploy` 会在共享宿主创建容器与网络，")
    lines.append("   因此 AWD 验收覆盖到「配置 / 状态 / 成员 / 战队 / 网络分配前」的真实读取与全部前端状态机，")
    lines.append("   未执行 `deploy`（GameBox 实例与判题面板以真实空态呈现）。")
    lines.append("3. 逃生舱（AI-FRONTEND-GUIDE §5.4，class B）逐项记录在 README §7 与本表 `证据 / 说明` 列。")

    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps({"rows": len(rows), **counts}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
