/**
 * AWD 网络配置表单的纯函数：解析、校验与比较。
 *
 * 校验规则与后端保持一致，前端不重复计算后端返回的派生数据（容量等）：
 * - Ipv4Cidr：点分十进制 + /0–/32，解析结果按掩码归一化；
 * - NetworkPool：地址池长度 ≤ 赛事子网长度 ≤ 队伍子网长度；
 * - 平台两个地址池、赛事手动分配的两个网段均不允许重叠；
 * - WireGuardPortRange：1 ≤ 起始端口 ≤ 结束端口 ≤ 65535。
 */

import type { PlatformNetworkSettings } from "@/api/awd";

const MAX_PREFIX = 32;
const MAX_PORT = 65535;

/** 解析后的 IPv4 网段。 */
export type Ipv4Cidr = {
	/** 归一化后的网络地址（无符号 32 位整数）。 */
	network: number;
	/** 前缀长度（0–32）。 */
	prefix: number;
	/**
	 * 输入地址是否带有主机位（例如 10.10.0.5/16）。
	 * 平台存储使用 PostgreSQL `cidr` 列，只接受网络地址，故这类输入必须拦在前端。
	 */
	hasHostBits: boolean;
};

function prefixMask(prefix: number): number {
	if (prefix <= 0) return 0;
	return (0xffffffff << (32 - prefix)) >>> 0;
}

/** 解析 IPv4 CIDR；格式非法或前缀超界返回 null。 */
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

/** 网络地址校验提示（PostgreSQL `cidr` 列不接受带主机位的地址）。 */
const HOST_BITS_MESSAGE = "应为网络地址（主机位为 0），例如 10.10.0.0/16。";

/** 两个网段是否存在地址重叠（任一包含另一个即视为重叠）。 */
export function cidrOverlaps(a: Ipv4Cidr, b: Ipv4Cidr): boolean {
	const mask = prefixMask(Math.min(a.prefix, b.prefix));
	return (a.network & mask) >>> 0 === (b.network & mask) >>> 0;
}

/** 解析 host:port 形式的接入地址；非法返回 null。 */
export function parseHostPort(
	value: string,
): { host: string; port: number } | null {
	const text = value.trim();
	const separator = text.lastIndexOf(":");
	if (separator <= 0) return null;
	const host = text.slice(0, separator);
	const port = parseBoundedInteger(text.slice(separator + 1), 1, MAX_PORT);
	if (port === null) return null;
	if (!/^[A-Za-z0-9._\-[\]:]+$/.test(host)) return null;
	return { host, port };
}

/** 解析十进制整数文本；空串、非数字或超界返回 null。 */
function parseBoundedInteger(
	value: string,
	min: number,
	max: number,
): number | null {
	const text = value.trim();
	if (!/^\d+$/.test(text)) return null;
	const parsed = Number(text);
	if (parsed < min || parsed > max) return null;
	return parsed;
}

/**
 * AWD 运行时允许的最大赛事子网长度：预检 `validate_event_network` 要求
 * `gamebox_cidr` 为 /16 或更大（`gamebox_cidr must be /16 or smaller`），
 * 因此赛事子网长度填入值不得大于 16，否则部署后预检必然失败。
 */
export const AWD_MAX_EVENT_PREFIX = 16;

/** 校验子网长度字段，返回错误文案；通过时返回 undefined。 */
function prefixError(
	value: string,
	required: string,
	lowerBound?: number,
	lowerMessage?: string,
): string | undefined {
	if (!value.trim()) return required;
	const parsed = parseBoundedInteger(value, 0, MAX_PREFIX);
	if (parsed === null) return `应为 0–${MAX_PREFIX} 的整数。`;
	if (lowerBound !== undefined && parsed < lowerBound) return lowerMessage;
	return undefined;
}

// ── 平台级网络设置 ──

export type PlatformNetworkForm = {
	gamebox_pool: string;
	gamebox_event_prefix: string;
	gamebox_team_prefix: string;
	wireguard_pool: string;
	wireguard_event_prefix: string;
	wireguard_team_prefix: string;
	wireguard_public_endpoint: string;
	wireguard_port_min: string;
	wireguard_port_max: string;
};

export type PlatformNetworkErrors = Partial<
	Record<keyof PlatformNetworkForm, string>
>;

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

/** 由接口返回值构造表单初值。 */
export function platformFormFromSettings(
	settings: PlatformNetworkSettings,
): PlatformNetworkForm {
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

/** 判断表单内容是否与给定初值一致（忽略首尾空白）。 */
export function isSamePlatformForm(
	a: PlatformNetworkForm,
	b: PlatformNetworkForm,
): boolean {
	return (Object.keys(a) as (keyof PlatformNetworkForm)[]).every(
		(key) => a[key].trim() === b[key].trim(),
	);
}

/** 平台级设置校验，返回字段错误；无错误时返回空对象。 */
export function validatePlatformNetworkForm(
	form: PlatformNetworkForm,
): PlatformNetworkErrors {
	const errors: PlatformNetworkErrors = {};

	const gamebox = parseIpv4Cidr(form.gamebox_pool);
	if (!form.gamebox_pool.trim()) {
		errors.gamebox_pool = "请填写 GameBox 地址池，例如 10.10.0.0/16。";
	} else if (!gamebox) {
		errors.gamebox_pool = "格式不正确，应为 CIDR（例如 10.10.0.0/16）。";
	} else if (gamebox.hasHostBits) {
		errors.gamebox_pool = HOST_BITS_MESSAGE;
	}
	const wireguard = parseIpv4Cidr(form.wireguard_pool);
	if (!form.wireguard_pool.trim()) {
		errors.wireguard_pool = "请填写 WireGuard 地址池，例如 10.20.0.0/16。";
	} else if (!wireguard) {
		errors.wireguard_pool = "格式不正确，应为 CIDR（例如 10.20.0.0/16）。";
	} else if (wireguard.hasHostBits) {
		errors.wireguard_pool = HOST_BITS_MESSAGE;
	}
	if (
		gamebox &&
		wireguard &&
		!gamebox.hasHostBits &&
		!wireguard.hasHostBits &&
		cidrOverlaps(gamebox, wireguard)
	) {
		const overlapMessage = "两个地址池不允许重叠，请改用相互独立的网段。";
		errors.gamebox_pool = overlapMessage;
		errors.wireguard_pool = overlapMessage;
	}

	const gameboxEventPrefix = prefixError(
		form.gamebox_event_prefix,
		"请填写 GameBox 赛事子网长度。",
		gamebox?.prefix,
		`不得小于地址池长度 /${gamebox?.prefix}。`,
	);
	if (gameboxEventPrefix) {
		errors.gamebox_event_prefix = gameboxEventPrefix;
	} else {
		const parsedEventPrefix = parseBoundedInteger(
			form.gamebox_event_prefix,
			0,
			MAX_PREFIX,
		);
		if (
			parsedEventPrefix !== null &&
			parsedEventPrefix > AWD_MAX_EVENT_PREFIX
		) {
			errors.gamebox_event_prefix = `不得超过 ${AWD_MAX_EVENT_PREFIX}：AWD 运行时要求赛事网段为 /${AWD_MAX_EVENT_PREFIX} 或更大。`;
		}
	}
	const gameboxTeamPrefix = prefixError(
		form.gamebox_team_prefix,
		"请填写 GameBox 队伍子网长度。",
		parseBoundedInteger(form.gamebox_event_prefix, 0, MAX_PREFIX) ?? undefined,
		"不得小于赛事子网长度。",
	);
	if (gameboxTeamPrefix) errors.gamebox_team_prefix = gameboxTeamPrefix;

	const wireguardEventPrefix = prefixError(
		form.wireguard_event_prefix,
		"请填写 WireGuard 赛事子网长度。",
		wireguard?.prefix,
		`不得小于地址池长度 /${wireguard?.prefix}。`,
	);
	if (wireguardEventPrefix)
		errors.wireguard_event_prefix = wireguardEventPrefix;
	const wireguardTeamPrefix = prefixError(
		form.wireguard_team_prefix,
		"请填写 WireGuard 队伍子网长度。",
		parseBoundedInteger(form.wireguard_event_prefix, 0, MAX_PREFIX) ??
			undefined,
		"不得小于赛事子网长度。",
	);
	if (wireguardTeamPrefix) {
		errors.wireguard_team_prefix = wireguardTeamPrefix;
	}

	const portMin = parseBoundedInteger(form.wireguard_port_min, 1, MAX_PORT);
	if (!form.wireguard_port_min.trim()) {
		errors.wireguard_port_min = "请填写起始端口。";
	} else if (portMin === null) {
		errors.wireguard_port_min = `应为 1–${MAX_PORT} 的整数。`;
	}
	const portMax = parseBoundedInteger(form.wireguard_port_max, 1, MAX_PORT);
	if (!form.wireguard_port_max.trim()) {
		errors.wireguard_port_max = "请填写结束端口。";
	} else if (portMax === null) {
		errors.wireguard_port_max = `应为 1–${MAX_PORT} 的整数。`;
	} else if (portMin !== null && portMax < portMin) {
		errors.wireguard_port_max = "不得小于起始端口。";
	}

	const endpoint = form.wireguard_public_endpoint.trim();
	if (endpoint && !parseHostPort(endpoint)) {
		errors.wireguard_public_endpoint =
			"格式不正确，应为 host:port，例如 vpn.example.com:51820。";
	}

	return errors;
}

// ── 赛事级手动分配 ──

export type ManualAllocationForm = {
	gamebox_cidr: string;
	wireguard_cidr: string;
	wireguard_listen_port: string;
};

export type ManualAllocationErrors = Partial<
	Record<keyof ManualAllocationForm, string>
>;

export const EMPTY_MANUAL_ALLOCATION_FORM: ManualAllocationForm = {
	gamebox_cidr: "",
	wireguard_cidr: "",
	wireguard_listen_port: "",
};

/**
 * 赛事手动分配校验：覆盖格式（含网络地址要求）与两个网段的重叠约束。
 * 是否与现有分配冲突、网段长度是否满足平台队伍子网长度、端口是否落在平台端口池内，
 * 均由后端判定并逐条返回错误。
 */
export function validateManualAllocationForm(
	form: ManualAllocationForm,
): ManualAllocationErrors {
	const errors: ManualAllocationErrors = {};

	const gamebox = parseIpv4Cidr(form.gamebox_cidr);
	if (!form.gamebox_cidr.trim()) {
		errors.gamebox_cidr = "请填写 GameBox 网段。";
	} else if (!gamebox) {
		errors.gamebox_cidr = "格式不正确，应为 CIDR（例如 10.97.0.0/16）。";
	} else if (gamebox.hasHostBits) {
		errors.gamebox_cidr = HOST_BITS_MESSAGE;
	}
	// AWD 运行时要求赛事网段 /16 或更大（预检 gamebox_cidr must be /16 or smaller）。
	if (
		!errors.gamebox_cidr &&
		gamebox &&
		!gamebox.hasHostBits &&
		gamebox.prefix > AWD_MAX_EVENT_PREFIX
	) {
		errors.gamebox_cidr = `不得小于 /${AWD_MAX_EVENT_PREFIX}：AWD 运行时要求赛事网段为 /${AWD_MAX_EVENT_PREFIX} 或更大。`;
	}
	const wireguard = parseIpv4Cidr(form.wireguard_cidr);
	if (!form.wireguard_cidr.trim()) {
		errors.wireguard_cidr = "请填写 WireGuard 网段。";
	} else if (!wireguard) {
		errors.wireguard_cidr = "格式不正确，应为 CIDR（例如 10.113.0.0/16）。";
	} else if (wireguard.hasHostBits) {
		errors.wireguard_cidr = HOST_BITS_MESSAGE;
	}
	if (
		gamebox &&
		wireguard &&
		!gamebox.hasHostBits &&
		!wireguard.hasHostBits &&
		cidrOverlaps(gamebox, wireguard)
	) {
		const overlapMessage = "两个网段不允许重叠，请改用相互独立的网段。";
		errors.gamebox_cidr = overlapMessage;
		errors.wireguard_cidr = overlapMessage;
	}

	const portText = form.wireguard_listen_port.trim();
	if (portText && parseBoundedInteger(portText, 1, MAX_PORT) === null) {
		errors.wireguard_listen_port = `应为 1–${MAX_PORT} 的整数；留空表示由平台自动分配。`;
	}

	return errors;
}

/** 表单是否存在校验错误。 */
export function hasErrors(errors: Record<string, string | undefined>): boolean {
	return Object.values(errors).some((message) => Boolean(message));
}
