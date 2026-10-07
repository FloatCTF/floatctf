/**
 * 路径安全工具：前端制品里的 `entry` / `styles` 一律是**相对包路径**。
 *
 * 拒绝：绝对路径、URL scheme、`..` 段、反斜杠、空段、控制字符、非 ASCII 之外的
 * 可疑字符（协议上只允许 [A-Za-z0-9._/-]，且不得以 `/` 开头）。
 *
 * 这些检查是安全边界（见 docs/frontend/ARCHITECTURE.md「信任模型 / 安全审查」）：
 * 一个被篡改的 `frontend.json` 不能把浏览器指向同源之外或注册表之外的路径。
 */

/** 前端 ID 允许形式：小写字母/数字开头，随后小写字母/数字/`.`/`_`/`-`。 */
export const FRONTEND_ID_PATTERN = /^[a-z0-9][a-z0-9._-]*$/;

/** 前端 ID 长度上限（同时用于文件系统目录名）。 */
export const FRONTEND_ID_MAX_LENGTH = 64;

/** 版本号（严格 semver，不含 build metadata 之外的宽松形式）。 */
const SEMVER_PATTERN =
	/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;

/** 相对包路径里单个路径段允许的字符。 */
const SAFE_SEGMENT_PATTERN = /^[A-Za-z0-9._@+-]+$/;

/** 允许出现在 URL 里的 scheme（用于检测 `https://…` 这类绝对地址）。 */
const URL_SCHEME_PATTERN = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

export function isSafeFrontendId(id: unknown): id is string {
	return (
		typeof id === "string" &&
		id.length > 0 &&
		id.length <= FRONTEND_ID_MAX_LENGTH &&
		FRONTEND_ID_PATTERN.test(id)
	);
}

export function isValidSemver(version: unknown): version is string {
	return typeof version === "string" && SEMVER_PATTERN.test(version);
}

export interface PathCheckResult {
	ok: boolean;
	reason?: string;
}

/**
 * 校验制品内的相对路径（`entry` / `styles` / 注册表里的资产路径）。
 *
 * 明确拒绝：
 * - 绝对路径（`/x`、`C:\x`）
 * - URL scheme（`https:`、`data:`、`file:`、`javascript:`）
 * - `..` 路径穿越与 `.` 段
 * - 反斜杠与空段（`a//b`）
 * - 前后空白、控制字符
 */
export function checkRelativeAssetPath(value: unknown): PathCheckResult {
	if (typeof value !== "string") {
		return { ok: false, reason: "path must be a string" };
	}
	if (value.length === 0) {
		return { ok: false, reason: "path must not be empty" };
	}
	if (value.length > 512) {
		return { ok: false, reason: "path is too long" };
	}
	if (value !== value.trim()) {
		return { ok: false, reason: "path must not have surrounding whitespace" };
	}
	// eslint-disable-next-line no-control-regex
	if (/[\u0000-\u001f\u007f]/.test(value)) {
		return { ok: false, reason: "path must not contain control characters" };
	}
	if (value.includes("\\")) {
		return { ok: false, reason: "path must not contain backslashes" };
	}
	if (URL_SCHEME_PATTERN.test(value)) {
		return { ok: false, reason: "path must not contain a URL scheme" };
	}
	if (value.startsWith("/") || value.startsWith("~")) {
		return { ok: false, reason: "path must be relative" };
	}
	const segments = value.split("/");
	for (const segment of segments) {
		if (segment === "" ) {
			return { ok: false, reason: "path must not contain empty segments" };
		}
		if (segment === "." || segment === "..") {
			return { ok: false, reason: "path must not contain '.' or '..' segments" };
		}
		if (!SAFE_SEGMENT_PATTERN.test(segment)) {
			return { ok: false, reason: `path segment contains unsupported characters: ${segment}` };
		}
	}
	return { ok: true };
}

/**
 * 在已校验的相对前缀上拼接已校验的相对路径，返回以 `/` 分隔的 URL 路径。
 *
 * 只做字符串拼接 + 归一化重复斜杠；调用方必须已经用
 * {@link checkRelativeAssetPath} / {@link isSafeFrontendId} 校验过每一段。
 */
export function joinUrlPath(...parts: string[]): string {
	const joined = parts
		.filter((part) => part.length > 0)
		.map((part, index) =>
			index === 0 ? part.replace(/\/+$/, "") : part.replace(/^\/+|\/+$/g, ""),
		)
		.join("/");
	return joined.startsWith("/") ? joined : `/${joined}`;
}

/**
 * 校验 `compatibility` 里的 mostly-integer major 约束串。
 *
 * v1 刻意只支持 **整数 major**（如 `"1"`），不支持 semver range 语法：
 * 语义确定、易测、不会因为 range 解析器差异产生"某台机器上能装"的分歧。
 */
export function parseMajorConstraint(value: unknown): number | null {
	if (typeof value !== "string") return null;
	const trimmed = value.trim();
	if (!/^(0|[1-9]\d*)$/.test(trimmed)) return null;
	const major = Number.parseInt(trimmed, 10);
	return Number.isSafeInteger(major) ? major : null;
}

/** major 是否与当前契约 major 兼容（v1：必须完全相等）。 */
export function isMajorCompatible(constraint: unknown, current: string): boolean {
	const major = parseMajorConstraint(constraint);
	if (major === null) return false;
	return major === Number.parseInt(current, 10);
}
