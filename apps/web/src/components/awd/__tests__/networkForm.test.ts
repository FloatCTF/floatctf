import { describe, expect, it } from "vitest";

import {
	EMPTY_MANUAL_ALLOCATION_FORM,
	type PlatformNetworkForm,
	cidrOverlaps,
	isSamePlatformForm,
	parseHostPort,
	parseIpv4Cidr,
	validateManualAllocationForm,
	validatePlatformNetworkForm,
} from "@/components/awd/networkForm";

const VALID_FORM: PlatformNetworkForm = {
	gamebox_pool: "10.10.0.0/16",
	gamebox_event_prefix: "20",
	gamebox_team_prefix: "24",
	wireguard_pool: "10.20.0.0/16",
	wireguard_event_prefix: "20",
	wireguard_team_prefix: "24",
	wireguard_public_endpoint: "vpn.example.com:51820",
	wireguard_port_min: "51820",
	wireguard_port_max: "51830",
};

describe("parseIpv4Cidr", () => {
	it("accepts valid CIDR and normalizes the network address", () => {
		expect(parseIpv4Cidr("10.10.20.5/24")).toEqual({
			network: (10 << 24) | (10 << 16) | (20 << 8),
			prefix: 24,
			hasHostBits: true,
		});
	});

	it("flags host bits so they can be rejected before hitting the DB", () => {
		expect(parseIpv4Cidr("10.10.0.0/16")?.hasHostBits).toBe(false);
		expect(parseIpv4Cidr("10.10.0.5/16")?.hasHostBits).toBe(true);
		expect(parseIpv4Cidr("10.10.20.0/24")?.hasHostBits).toBe(false);
	});

	it("rejects malformed values", () => {
		expect(parseIpv4Cidr("10.10.0.0")).toBeNull();
		expect(parseIpv4Cidr("10.10.0.0/16/8")).toBeNull();
		expect(parseIpv4Cidr("10.10.0/16")).toBeNull();
		expect(parseIpv4Cidr("10.10.0.256/16")).toBeNull();
		expect(parseIpv4Cidr("10.10.0.0/33")).toBeNull();
		expect(parseIpv4Cidr("10.10.0.0/aa")).toBeNull();
		expect(parseIpv4Cidr("")).toBeNull();
	});

	it("accepts /0 and /32", () => {
		expect(parseIpv4Cidr("0.0.0.0/0")?.prefix).toBe(0);
		expect(parseIpv4Cidr("10.10.10.10/32")?.prefix).toBe(32);
	});
});

describe("cidrOverlaps", () => {
	const cidr = (value: string) => {
		const parsed = parseIpv4Cidr(value);
		if (!parsed) throw new Error(`unparsable fixture: ${value}`);
		return parsed;
	};

	it("detects containment in both directions", () => {
		expect(cidrOverlaps(cidr("10.10.0.0/16"), cidr("10.10.20.0/24"))).toBe(
			true,
		);
		expect(cidrOverlaps(cidr("10.10.20.0/24"), cidr("10.10.0.0/16"))).toBe(
			true,
		);
	});

	it("treats adjacent subnets as disjoint", () => {
		expect(cidrOverlaps(cidr("10.10.0.0/16"), cidr("10.20.0.0/16"))).toBe(
			false,
		);
		expect(cidrOverlaps(cidr("10.10.0.0/17"), cidr("10.10.128.0/17"))).toBe(
			false,
		);
	});

	it("treats every network as contained in 0.0.0.0/0", () => {
		expect(cidrOverlaps(cidr("0.0.0.0/0"), cidr("192.168.1.0/24"))).toBe(true);
	});
});

describe("parseHostPort", () => {
	it("parses host:port", () => {
		expect(parseHostPort("vpn.example.com:51820")).toEqual({
			host: "vpn.example.com",
			port: 51820,
		});
	});

	it("rejects missing host or invalid port", () => {
		expect(parseHostPort("vpn.example.com")).toBeNull();
		expect(parseHostPort(":51820")).toBeNull();
		expect(parseHostPort("vpn.example.com:0")).toBeNull();
		expect(parseHostPort("vpn.example.com:70000")).toBeNull();
		expect(parseHostPort("vpn example.com:51820")).toBeNull();
	});
});

describe("isSamePlatformForm", () => {
	it("ignores surrounding whitespace", () => {
		expect(
			isSamePlatformForm(VALID_FORM, {
				...VALID_FORM,
				gamebox_pool: " 10.10.0.0/16 ",
			}),
		).toBe(true);
	});

	it("detects an edited field", () => {
		expect(
			isSamePlatformForm(VALID_FORM, {
				...VALID_FORM,
				gamebox_team_prefix: "25",
			}),
		).toBe(false);
	});
});

describe("validatePlatformNetworkForm", () => {
	it("accepts a valid form", () => {
		expect(validatePlatformNetworkForm(VALID_FORM)).toEqual({});
	});

	it("accepts an empty public endpoint", () => {
		expect(
			validatePlatformNetworkForm({
				...VALID_FORM,
				wireguard_public_endpoint: "",
			}),
		).toEqual({});
	});

	it("reports malformed pools", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			gamebox_pool: "10.10.0.0",
			wireguard_pool: "10.20.0.0/40",
		});
		expect(errors.gamebox_pool).toBeTruthy();
		expect(errors.wireguard_pool).toBeTruthy();
	});

	it("rejects pools that are not network addresses", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			gamebox_pool: "10.10.0.5/16",
		});
		expect(errors.gamebox_pool).toContain("网络地址");
	});

	it("reports overlapping pools on both fields", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			wireguard_pool: "10.10.128.0/17",
		});
		expect(errors.gamebox_pool).toBeTruthy();
		expect(errors.wireguard_pool).toBeTruthy();
	});

	it("rejects an event prefix shorter than the pool prefix", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			gamebox_event_prefix: "12",
		});
		expect(errors.gamebox_event_prefix).toContain("/16");
	});

	it("rejects a team prefix shorter than the event prefix", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			gamebox_team_prefix: "18",
		});
		expect(errors.gamebox_team_prefix).toBeTruthy();
	});

	it("rejects an inverted port range", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			wireguard_port_min: "52000",
			wireguard_port_max: "51000",
		});
		expect(errors.wireguard_port_max).toBeTruthy();
	});

	it("rejects a malformed public endpoint", () => {
		const errors = validatePlatformNetworkForm({
			...VALID_FORM,
			wireguard_public_endpoint: "vpn.example.com",
		});
		expect(errors.wireguard_public_endpoint).toBeTruthy();
	});

	it("reports every empty field", () => {
		const errors = validatePlatformNetworkForm({
			gamebox_pool: "",
			gamebox_event_prefix: "",
			gamebox_team_prefix: "",
			wireguard_pool: "",
			wireguard_event_prefix: "",
			wireguard_team_prefix: "",
			wireguard_public_endpoint: "",
			wireguard_port_min: "",
			wireguard_port_max: "",
		});
		expect(Object.keys(errors)).toHaveLength(8);
	});
});

describe("validateManualAllocationForm", () => {
	it("accepts valid CIDRs with an optional port", () => {
		expect(
			validateManualAllocationForm({
				gamebox_cidr: "10.10.20.0/24",
				wireguard_cidr: "10.20.20.0/24",
				wireguard_listen_port: "",
			}),
		).toEqual({});
		expect(
			validateManualAllocationForm({
				gamebox_cidr: "10.10.20.0/24",
				wireguard_cidr: "10.20.20.0/24",
				wireguard_listen_port: "51820",
			}),
		).toEqual({});
	});

	it("reports overlapping CIDRs on both fields", () => {
		const errors = validateManualAllocationForm({
			...EMPTY_MANUAL_ALLOCATION_FORM,
			gamebox_cidr: "10.10.0.0/16",
			wireguard_cidr: "10.10.20.0/24",
		});
		expect(errors.gamebox_cidr).toBeTruthy();
		expect(errors.wireguard_cidr).toBeTruthy();
	});

	it("rejects CIDRs that are not network addresses", () => {
		const errors = validateManualAllocationForm({
			gamebox_cidr: "10.10.20.5/24",
			wireguard_cidr: "10.20.20.0/24",
			wireguard_listen_port: "",
		});
		expect(errors.gamebox_cidr).toContain("网络地址");
		expect(errors.wireguard_cidr).toBeUndefined();
	});

	it("reports an invalid port", () => {
		const errors = validateManualAllocationForm({
			gamebox_cidr: "10.10.20.0/24",
			wireguard_cidr: "10.20.20.0/24",
			wireguard_listen_port: "70000",
		});
		expect(errors.wireguard_listen_port).toBeTruthy();
	});
});
