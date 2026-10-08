/**
 * 平台网络设置的**前端校验**（纯函数，与后端 `domain::network` 的约束对齐）。
 *
 * 依据（读 `apps/api/src/modules/event/awd/` 核实）：
 * - 地址池 / 赛事网段都用 PostgreSQL `cidr` 列存储 → 只接受**网络地址**（主机位必须为 0）；
 * - `NetworkPool`：池长 ≤ 赛事子网长 ≤ 队伍子网长；
 * - AWD 预检要求赛事 `gamebox_cidr` 为 /16 或更小 → 赛事子网长度**不得大于 16**；
 * - 端口范围 1..=65535 且起始 ≤ 结束；
 * - `wireguard_public_endpoint` 接受 `IP` / `IP:port` / `含点域名` / `含点域名:port`，**端口可省略**
 *   （后端 `is_valid_public_endpoint` 就是可选的，比 Default 前端更宽松）。
 *
 * 这里只是「尽早发现明显错误」；最终判定永远由后端做（错误文案来自后端）。
 */

import type { PlatformNetworkSettings } from "@floatctf/sdk";

const MAX_PREFIX = 32;
const MAX_PORT = 65535;

/** AWD 预检允许的最大赛事子网长度（必须 ≥ /16）。 */
export const AWD_MAX_EVENT_PREFIX = 16;

export interface Ipv4Cidr {
	network: number;
	prefix: number;
	/** 输入带主机位（如 10.10.0.5/16）—— 数据库 `cidr` 列会拒绝。 */
	hasHostBits: boolean;
}

function prefixMask(prefix: number): number {
	if (prefix <= 0) return 0;
	return (0xffffffff << (32 - prefix)) >>> 0;
}

export function parseIpv4Cidr(value: string): Ipv4Cidr | null {
	const parts = value.trim().split("/");
	if (parts.length !== 2) return null;
	const [address, prefixText] = parts;
	if (!/^\d{1,3}$/.test(prefixText)) return null;
	const prefix = Number(prefixText);
	if (prefix > MAX_PREFIX) return null;
	const octets = address.split(".");
	if (octets.length !== 4) return null;
	let raw = 0;
	for (const octet of octets) {
		if (!/^\d{1,3}$/.test(octet)) return null;
		const value = Number(octet);
		if (value > 255) return null;
		raw = ((raw << 8) | value) >>> 0;
	}
	const network = (raw & prefixMask(prefix)) >>> 0;
	return { network, prefix, hasHostBits: network !== raw };
}

function cidrOverlaps(a: Ipv4Cidr, b: Ipv4Cidr): boolean {
	const mask = prefixMask(Math.min(a.prefix, b.prefix));
	return ((a.network & mask) >>> 0) === ((b.network & mask) >>> 0);
}

function parseBoundedInt(value: string, min: number, max: number): number | null {
	const text = value.trim();
	if (!/^\d+$/.test(text)) return null;
	const parsed = Number(text);
	if (parsed < min || parsed > max) return null;
	return parsed;
}

/** `IP` / `IP:port` / `含点域名` / `含点域名:port`；端口可省略。 */
export function isValidPublicEndpoint(value: string): boolean {
	const text = value.trim();
	if (text === "") return false;

	const isIpv4 = (host: string): boolean =>
		/^\d{1,3}(\.\d{1,3}){3}$/.test(host) && host.split(".").every((part) => Number(part) <= 255);

	// 只支持 IPv4 / 含点域名（AWD 网络本身是 IPv4-only，后端虽允许 IPv6 字面量但平台不使用）。
	const separator = text.lastIndexOf(":");
	const host = separator > 0 ? text.slice(0, separator) : text;
	const portText = separator > 0 ? text.slice(separator + 1) : null;

	if (!isIpv4(host)) {
		const labels = host.split(".");
		const labelOk =
			host.length > 0 &&
			host.length <= 253 &&
			labels.length >= 2 &&
			labels.every(
				(label) =>
					label.length > 0 &&
					label.length <= 63 &&
					/^[A-Za-z0-9-]+$/.test(label),
			);
		if (!labelOk) return false;
	}
	if (portText === null) return true;
	const port = parseBoundedInt(portText, 1, MAX_PORT);
	return port !== null;
}

// ── 表单模型 ────────────────────────────────────────────────────────────────

export interface PlatformNetworkForm {
	gamebox_pool: string;
	gamebox_event_prefix: string;
	gamebox_team_prefix: string;
	wireguard_pool: string;
	wireguard_event_prefix: string;
	wireguard_team_prefix: string;
	wireguard_public_endpoint: string;
	wireguard_port_min: string;
	wireguard_port_max: string;
}

export type PlatformNetworkErrors = Partial<Record<keyof PlatformNetworkForm, string>>;

export const EMPTY_PLATFORM_NETWORK_FORM: PlatformNetworkForm = {
	gamebox_pool: "",
	gamebox_event_prefix: "",
	gamebox_team_prefix: "",
	wireguard_pool: "",
	wireguard_event_prefix: "",
	wireguard_team_prefix: "",
	wireguard_public_endpoint: "",
	wireguard_port_min: "",
	wireguard_port_max: "",
};

export function formFromSettings(settings: PlatformNetworkSettings): PlatformNetworkForm {
	return {
		gamebox_pool: settings.gamebox_pool,
		gamebox_event_prefix: String(settings.gamebox_event_prefix),
		gamebox_team_prefix: String(settings.gamebox_team_prefix),
		wireguard_pool: settings.wireguard_pool,
		wireguard_event_prefix: String(settings.wireguard_event_prefix),
		wireguard_team_prefix: String(settings.wireguard_team_prefix),
		wireguard_public_endpoint: settings.wireguard_public_endpoint ?? "",
		wireguard_port_min: String(settings.wireguard_port_min),
		wireguard_port_max: String(settings.wireguard_port_max),
	};
}

const HOST_BITS_MESSAGE = "应为网络地址（主机位为 0），例如 10.10.0.0/16。";

/** 前缀字段校验：必填、0–32 整数、不小于下限、赛事网段不得大于 /16。 */
function prefixError(value: string, required: string, lowerBound?: number): string | undefined {
	if (!value.trim()) return required;
	const parsed = parseBoundedInt(value, 0, MAX_PREFIX);
	if (parsed === null) return `应为 0–${MAX_PREFIX} 的整数。`;
	if (lowerBound !== undefined && parsed < lowerBound) return `不得小于地址池长度 /${lowerBound}。`;
	return undefined;
}

export function validatePlatformNetworkForm(form: PlatformNetworkForm): PlatformNetworkErrors {
	const errors: PlatformNetworkErrors = {};

	const gamebox = parseIpv4Cidr(form.gamebox_pool);
	if (!form.gamebox_pool.trim()) errors.gamebox_pool = "请填写 GameBox 地址池，例如 10.10.0.0/16。";
	else if (!gamebox) errors.gamebox_pool = "格式不正确，应为 CIDR（例如 10.10.0.0/16）。";
	else if (gamebox.hasHostBits) errors.gamebox_pool = HOST_BITS_MESSAGE;

	const wireguard = parseIpv4Cidr(form.wireguard_pool);
	if (!form.wireguard_pool.trim()) {
		errors.wireguard_pool = "请填写 WireGuard 地址池，例如 10.20.0.0/16。";
	} else if (!wireguard) errors.wireguard_pool = "格式不正确，应为 CIDR（例如 10.20.0.0/16）。";
	else if (wireguard.hasHostBits) errors.wireguard_pool = HOST_BITS_MESSAGE;

	if (gamebox && wireguard && !gamebox.hasHostBits && !wireguard.hasHostBits) {
		if (cidrOverlaps(gamebox, wireguard)) {
			const message = "两个地址池不允许重叠，请改用相互独立的网段。";
			errors.gamebox_pool = message;
			errors.wireguard_pool = message;
		}
	}

	const gameboxEventPrefix = prefixError(
		form.gamebox_event_prefix,
		"请填写 GameBox 赛事子网长度。",
		gamebox?.prefix,
	);
	if (gameboxEventPrefix) {
		errors.gamebox_event_prefix = gameboxEventPrefix;
	} else {
		const parsed = parseBoundedInt(form.gamebox_event_prefix, 0, MAX_PREFIX);
		if (parsed !== null && parsed > AWD_MAX_EVENT_PREFIX) {
			errors.gamebox_event_prefix = `不得超过 ${AWD_MAX_EVENT_PREFIX}：AWD 运行时要求赛事网段为 /${AWD_MAX_EVENT_PREFIX} 或更大。`;
		}
	}

	const eventPrefixValue = parseBoundedInt(form.gamebox_event_prefix, 0, MAX_PREFIX) ?? undefined;
	const gameboxTeamPrefix = prefixError(
		form.gamebox_team_prefix,
		"请填写 GameBox 队伍子网长度。",
		eventPrefixValue,
	);
	if (gameboxTeamPrefix) errors.gamebox_team_prefix = gameboxTeamPrefix;

	const wireguardEventPrefix = prefixError(
		form.wireguard_event_prefix,
		"请填写 WireGuard 赛事子网长度。",
		wireguard?.prefix,
	);
	if (wireguardEventPrefix) errors.wireguard_event_prefix = wireguardEventPrefix;

	const wgEventValue = parseBoundedInt(form.wireguard_event_prefix, 0, MAX_PREFIX) ?? undefined;
	const wireguardTeamPrefix = prefixError(
		form.wireguard_team_prefix,
		"请填写 WireGuard 队伍子网长度。",
		wgEventValue,
	);
	if (wireguardTeamPrefix) errors.wireguard_team_prefix = wireguardTeamPrefix;

	const portMin = parseBoundedInt(form.wireguard_port_min, 1, MAX_PORT);
	if (!form.wireguard_port_min.trim()) errors.wireguard_port_min = "请填写起始端口。";
	else if (portMin === null) errors.wireguard_port_min = `应为 1–${MAX_PORT} 的整数。`;

	const portMax = parseBoundedInt(form.wireguard_port_max, 1, MAX_PORT);
	if (!form.wireguard_port_max.trim()) errors.wireguard_port_max = "请填写结束端口。";
	else if (portMax === null) errors.wireguard_port_max = `应为 1–${MAX_PORT} 的整数。`;
	else if (portMin !== null && portMax < portMin) errors.wireguard_port_max = "不得小于起始端口。";

	const endpoint = form.wireguard_public_endpoint.trim();
	if (endpoint && !isValidPublicEndpoint(endpoint)) {
		errors.wireguard_public_endpoint =
			"格式不正确，应为「IP 或域名[:端口]」，例如 vpn.example.com:51820。";
	}

	return errors;
}
