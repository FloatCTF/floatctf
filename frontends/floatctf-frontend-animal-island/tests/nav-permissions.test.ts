/**
 * 权限 → 菜单 的行为测试。
 *
 * 这是本前端唯一「没有 API 也能证明」的关键不变式：
 * **没有权限就看不到对应菜单**。它完全由 `visibleConsoleNav` 决定，
 * 因此可以脱离后端直接断言。
 *
 * 运行方式（本仓没有测试运行器，用仓库里已有的 tsc 编译后跑）：
 *
 * ```bash
 * pnpm exec tsc tests/nav-permissions.test.ts \
 *   --outDir /tmp/navtest --module commonjs --target es2022 \
 *   --moduleResolution node --esModuleInterop --skipLibCheck --strict
 * node /tmp/navtest/tests/nav-permissions.test.js
 * ```
 *
 * 断言失败时进程以非 0 退出，可直接当门禁用。
 */
import { CONSOLE_NAV, ISLAND_NAV, flatNav, visibleConsoleNav } from "../src/app/nav";
import { hasPermission } from "../src/app/permission-logic";

// ── 极简断言 ────────────────────────────────────────────────────────────────

let failures = 0;
let checks = 0;

function check(label: string, cond: boolean, detail?: string): void {
	checks += 1;
	if (cond) {
		console.log(`  ✓ ${label}`);
	} else {
		failures += 1;
		console.error(`  ✗ ${label}${detail ? `\n      ${detail}` : ""}`);
	}
}

function section(title: string): void {
	console.log(`\n${title}`);
}

// ── 后端权限目录（fixture）──────────────────────────────────────────────────
//
// 来源：`apps/api/src/core/permission.rs` 的 `define_permissions!`（29 条）。
// 这是**刻意的快照**：nav 里出现目录外的字符串 = 拼写错误 = 该菜单永远不显示，
// 而这类错误不会报任何错。快照过期时这条断言会失败，提醒两边对齐。

const BACKEND_PERMISSIONS: readonly string[] = [
	"users.read",
	"users.manage",
	"roles.read",
	"roles.manage",
	"challenges.read",
	"challenges.manage",
	"gameboxes.read",
	"gameboxes.manage",
	"weapons.read",
	"weapons.manage",
	"events.read",
	"events.manage",
	"events.participants.manage",
	"events.lifecycle.manage",
	"awd.read",
	"awd.manage",
	"awdp.read",
	"awdp.manage",
	"announcements.manage",
	"discussions.moderate",
	"settings.read",
	"settings.manage",
	"system.read",
	"logs.read",
	"scheduler.manage",
	"docker.read",
	"docker.manage",
	"database.execute",
	"terminal.open",
];

const ALL = new Set(BACKEND_PERMISSIONS);
const NONE = new Set<string>();

// ── 1. 结构自洽 ────────────────────────────────────────────────────────────

section("1. 导航结构自洽");

const consoleItems = CONSOLE_NAV.flatMap((g) => g.items);
const islandItems = ISLAND_NAV.flatMap((g) => g.items);

check(
	"每个控制台条目都声明了非空 permission",
	consoleItems.every((i) => typeof i.permission === "string" && i.permission.length > 0),
);
check(
	"控制台条目没有重复路径",
	new Set(consoleItems.map((i) => i.to)).size === consoleItems.length,
);
check(
	"岛屿条目没有重复路径",
	new Set(islandItems.map((i) => i.to)).size === islandItems.length,
);

// ── 2. 权限串都在后端目录内（拼写守卫）────────────────────────────────────

section("2. 权限串拼写（对照后端 29 条）");

const declared = [...new Set(consoleItems.map((i) => i.permission))].sort();
const unknown = declared.filter((p) => !ALL.has(p));

check(
	`声明的 ${declared.length} 条权限全部存在于后端目录`,
	unknown.length === 0,
	unknown.length > 0 ? `目录外的权限串：${unknown.join(", ")}` : undefined,
);

// ── 3. 核心不变式：没权限 → 看不到菜单 ────────────────────────────────────

section("3. 核心不变式：没有权限就看不到对应菜单");

const noneVisible = visibleConsoleNav(NONE);
check(
	"空权限集 → 控制台一个分组都不显示",
	noneVisible.length === 0,
	`实际显示了 ${noneVisible.length} 个分组`,
);

const allVisible = visibleConsoleNav(ALL);
check(
	"全部权限 → 所有分组都显示",
	allVisible.length === CONSOLE_NAV.length,
	`期望 ${CONSOLE_NAV.length}，实际 ${allVisible.length}`,
);
check(
	"全部权限 → 所有条目都显示",
	allVisible.flatMap((g) => g.items).length === consoleItems.length,
);

// 逐条：只给某一条权限时，可见集合必须**恰好**等于「permission 属于该集合的条目」。
//
// 注意不能假设「一条权限 = 一个菜单」：`system.read` 同时门控 `/console`（总览）
// 与 `/console/version`（版本），这与后端路由表一致（`dashboard/summary` 与
// `system/version` 都要 `perm:system.read`）。这里断言的是真正的语义。
let perPermissionOk = true;
const perPermissionDetail: string[] = [];
for (const item of consoleItems) {
	const one = new Set([item.permission]);
	const actual = visibleConsoleNav(one)
		.flatMap((g) => g.items.map((i) => i.to))
		.sort();
	const expected = consoleItems
		.filter((i) => i.permission === item.permission)
		.map((i) => i.to)
		.sort();

	if (actual.join("|") !== expected.join("|")) {
		perPermissionOk = false;
		perPermissionDetail.push(
			`${item.permission} 期望 [${expected.join(", ")}]，实际 [${actual.join(", ")}]`,
		);
	}
}
check(
	"逐条：只给一条权限 → 可见集合恰好等于该权限门控的条目",
	perPermissionOk,
	perPermissionDetail.join("; ") || undefined,
);

// 明确记录"一条权限门控多个菜单"的情况，避免将来有人以为是 bug。
const multi = Object.entries(
	consoleItems.reduce<Record<string, number>>((acc, i) => {
		acc[i.permission] = (acc[i.permission] ?? 0) + 1;
		return acc;
	}, {}),
).filter(([, n]) => n > 1);
check(
	"共享同一权限的菜单已被显式确认（当前：system.read → 总览 + 版本）",
	multi.length === 1 && multi[0][0] === "system.read" && multi[0][1] === 2,
	`实际共享情况：${multi.map(([p, n]) => `${p}×${n}`).join(", ") || "无"}`,
);

// 反向：给全部权限**减去**某一条 → 该菜单必须消失。
let removalOk = true;
const removalDetail: string[] = [];
for (const item of consoleItems) {
	const without = new Set(BACKEND_PERMISSIONS.filter((p) => p !== item.permission));
	const paths = visibleConsoleNav(without).flatMap((g) => g.items.map((i) => i.to));
	if (paths.includes(item.to)) {
		removalOk = false;
		removalDetail.push(`移除 ${item.permission} 后 ${item.to} 仍然可见`);
	}
}
check(
	"逐条：拿掉一条权限 → 对应菜单立即消失",
	removalOk,
	removalDetail.join("; ") || undefined,
);

// 空分组不应残留（否则侧栏会出现一个没有条目的标题）。
const emptyGroups = allVisible.filter((g) => g.items.length === 0);
check("不会残留空分组", emptyGroups.length === 0);

// ── 4. 岛屿（选手端）不受控制台权限影响 ───────────────────────────────────

section("4. 岛屿导航与权限解耦");

check(
	"岛屿条目全部 permission: null（所有人可见）",
	islandItems.every((i) => i.permission === null),
);
check(
	"flatNav 在空权限下仍返回全部岛屿条目",
	flatNav(NONE).filter((i) => i.group.startsWith("岛屿")).length === islandItems.length,
);
check(
	"flatNav 在空权限下不返回任何控制台条目",
	flatNav(NONE).every((i) => !i.group.startsWith("控制台")),
);

// ── 5. 未持有的权限不产生任何入口 ─────────────────────────────────────────

section("5. 只读权限不应带出写入口（当前粒度基线）");

// 记录当前「读权限单独存在时能看到什么」，作为后续做按钮级门控的基线。
//
// `awd.read` / `awdp.read` 在这里**不**带出菜单：AWD/AWDP 的入口是赛事详情页上的
// 按钮（它们的一切都以赛事为作用域），不是顶层导航项 —— 这是刻意的设计。
const readOnlySet = new Set([
	"users.read",
	"challenges.read",
	"gameboxes.read",
	"weapons.read",
	"events.read",
	"awd.read",
	"awdp.read",
	"settings.read",
	"docker.read",
]);
const readOnlyPaths = visibleConsoleNav(readOnlySet)
	.flatMap((g) => g.items.map((i) => i.to))
	.sort();
const EXPECTED_READ_ONLY = [
	"/console/challenges",
	"/console/docker",
	"/console/events",
	"/console/gameboxes",
	"/console/network",
	"/console/settings",
	"/console/users",
	"/console/weapons",
];
check(
	"只读权限恰好带出 8 个菜单（AWD/AWDP 入口在赛事详情页，不在顶层）",
	readOnlyPaths.join("|") === EXPECTED_READ_ONLY.join("|"),
	`实际：${readOnlyPaths.join(", ")}`,
);
check(
	"只读权限看不到需要 manage 的菜单（日志/计划任务/终端/SQL/设置管理/前端管理）",
	!readOnlyPaths.some((p) =>
		[
			"/console/logs",
			"/console/tasks",
			"/console/terminal",
			"/console/database",
			"/console/frontends",
		].includes(p),
	),
);

// ── 6. 动作级门控（`hasPermission` 的语义）────────────────────────────────

section("6. 动作级门控：hasPermission 语义");

// `hasPermission` 是纯函数，因此可以直接测；`<Can>` 只是它的渲染封装。
check(
	"null 会话 → 一律无权限",
	hasPermission(null, "weapons.manage") === false,
);
check(
	"grants_all=true 且 permissions 为空 → **仍然有权限**（这是最容易写错的一处）",
	hasPermission({ grants_all: true, permissions: [] }, "weapons.manage") === true,
);
check(
	"grants_all=true → 连后端不认识的权限也算有（未来新增权限自动覆盖）",
	hasPermission({ grants_all: true, permissions: [] }, "some.future.permission") === true,
);
check(
	"普通账号：持有 → true",
	hasPermission({ grants_all: false, permissions: ["weapons.read"] }, "weapons.read") === true,
);
check(
	"普通账号：未持有 → false",
	hasPermission({ grants_all: false, permissions: ["weapons.read"] }, "weapons.manage") === false,
);
check(
	"permissions 为 null（老后端 / 字段缺失）→ 只读不崩",
	hasPermission({ grants_all: false, permissions: null }, "weapons.read") === false,
);
check(
	"只读权限**不**隐含写权限（read 不含 manage）",
	["weapons", "events", "challenges", "gameboxes", "users"].every(
		(res) =>
			hasPermission({ grants_all: false, permissions: [`${res}.read`] }, `${res}.manage`) ===
			false,
	),
);

// ── 7. 页面上的写按钮确实被门控了（源码守卫）────────────────────────────────

section("7. 源码守卫：写操作按钮必须包在 <Can> 里");

// 注意：这条守卫读的是**源码**，而本文件会被编译到 `.test-out/` 下运行，
// 因此不能用 `__dirname`（那会指向产物目录）。改从 cwd 找，
// 也就是说**必须在项目根目录运行**（见文件头的命令）。
const fs = require("node:fs") as typeof import("node:fs");
const pathMod = require("node:path") as typeof import("node:path");
const consoleDir = pathMod.join(process.cwd(), "src", "features", "console");

// 这些页面有写操作，必须至少出现一次 <Can（或 useCan 的禁用判断）。
const MUST_BE_GATED: ReadonlyArray<readonly [string, string]> = [
	["Events.tsx", "events.manage"],
	["EventDetail.tsx", "events.participants.manage"],
	["Challenges.tsx", "challenges.manage"],
	["Gameboxes.tsx", "gameboxes.manage"],
	["Weapons.tsx", "weapons.manage"],
	["Network.tsx", "awd.manage"],
	["Announcements.tsx", "announcements.manage"],
	["Discussions.tsx", "discussions.moderate"],
	["Users.tsx", "users.manage"],
	["UserDetail.tsx", "roles.manage"],
	["Roles.tsx", "roles.manage"],
	["Settings.tsx", "settings.manage"],
	["Frontends.tsx", "settings.manage"],
	["Tasks.tsx", "scheduler.manage"],
	["Docker.tsx", "docker.manage"],
	["AwdEvent.tsx", "events.lifecycle.manage"],
	["AwdpEvent.tsx", "events.lifecycle.manage"],
];

let gatedOk = true;
const gatedDetail: string[] = [];
for (const [file, perm] of MUST_BE_GATED) {
	const full = pathMod.join(consoleDir, file);
	if (!fs.existsSync(full)) {
		gatedOk = false;
		gatedDetail.push(`${file} 不存在`);
		continue;
	}
	const source = fs.readFileSync(full, "utf8");
	if (!source.includes(`permission="${perm}"`)) {
		gatedOk = false;
		gatedDetail.push(`${file} 未见 permission="${perm}" 的门控`);
	}
}
check(
	`${MUST_BE_GATED.length} 个含写操作的控制台页面都做了动作级门控`,
	gatedOk,
	gatedDetail.join("; ") || undefined,
);

// ── 8. 逐按钮门控守卫（比"文件里出现过 permission"强得多）────────────────────

section("8. 源码守卫：逐个写按钮检查，而不只是检查文件里出现过 <Can>");

// 为什么需要这条：上一版守卫只断言「文件里出现过 permission="X"」。
// 结果 **同一页面的其它写按钮**（表格行内、空状态里、状态列切换）全都漏过了 ——
// 实测有 5 个页面的行内「编辑/删除」+ 2 个空状态按钮 + 1 个状态列切换没有任何门控，
// 而旧守卫依然全绿。教训：守卫的**粒度**本身就是它的一部分。
//
// 这些是**已知且无害**的例外，每条都要有理由（不接受"暂时这样"）：
const GATE_EXEMPT: ReadonlyArray<readonly [string, string, string]> = [
	// [文件, 按钮文案, 理由]
	["Database.tsx", "执行", "整页由 nav 的 database.execute 门控 —— 进得来的人必然有权限"],
	["UserDetail.tsx", "去创建角色", "纯导航按钮（跳到 /console/roles），不写任何数据"],
	["AwdEvent.tsx", "确认封禁", "弹窗内的确认键；弹窗只能由已门控的「封禁」按钮打开"],
	["Roles.tsx", "保存", "弹窗内保存键，触发按钮已门控"],
	["Settings.tsx", "保存", "弹窗内保存键，触发按钮已门控"],
	["Weapons.tsx", "保存", "弹窗内保存键，触发按钮已门控"],
	["Challenges.tsx", "保存", "弹窗内保存键，触发按钮已门控"],
];

const WRITE_WORDS =
	/(新建|导入|创建|上传|编辑|删除|发布|保存|移除|执行|运行|停止|启动|构建|校验|扫描|轮换|封禁|解封|调分|挂载|卸载|重置|设为当前|隐藏|可见)/;

let perButtonOk = true;
const perButtonDetail: string[] = [];
let buttonCount = 0;

for (const file of fs.readdirSync(consoleDir).filter((n) => n.endsWith(".tsx"))) {
	const lines = fs.readFileSync(pathMod.join(consoleDir, file), "utf8").split("\n");
	// 用栈跟踪是否处于 <Can> 块内（先处理闭合，再处理开启，顺序不能反）
	const stack: string[] = [];
	lines.forEach((raw, idx) => {
		const ln = raw;
		for (let k = 0; k < (ln.match(/<\/Can>/g)?.length ?? 0); k++) stack.pop();
		const open = /<Can\s+permission="([^"]+)"/.exec(ln);
		if (open && !ln.includes("/>")) stack.push(open[1]);

		if (!/<Button\b/.test(ln)) return;
		// 按钮文案在紧随其后的若干行里（属性可能占多行）
		const block = lines.slice(idx, idx + 13).join("\n");
		const label = />\s*\n?\s*([\u4e00-\u9fa5]{2,6})\s*\n/.exec(block)?.[1];
		if (!label || !WRITE_WORDS.test(label)) return;
		buttonCount++;

		const exempt = GATE_EXEMPT.some(([f, l]) => f === file && l === label);
		if (stack.length === 0 && !exempt) {
			perButtonOk = false;
			perButtonDetail.push(`${file}:${idx + 1} 「${label}」未被 <Can> 包住`);
		}
	});
}

check(
	`逐个检查了 ${buttonCount} 个写操作按钮，未门控的必须命中豁免清单`,
	perButtonOk,
	perButtonDetail.join("; ") || undefined,
);
check(
	"豁免清单本身要小（>12 条说明门控在退化）",
	GATE_EXEMPT.length <= 12,
	`当前 ${GATE_EXEMPT.length} 条`,
);

// ── 汇总 ───────────────────────────────────────────────────────────────────

console.log(`\n${"─".repeat(60)}`);
if (failures === 0) {
	console.log(`全部通过：${checks} 项断言`);
	process.exit(0);
} else {
	console.error(`失败 ${failures} / ${checks} 项`);
	process.exit(1);
}


