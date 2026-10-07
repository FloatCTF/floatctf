import {
	Button,
	FormControl,
	Heading,
	Label,
	Select,
	Spinner,
	Text,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	API_CONTRACT_VERSION,
	DEFAULT_REGISTRY_URL,
	FRONTEND_RUNTIME_VERSION,
	type FloatCTFRegistry,
	parseRegistry,
} from "@floatctf/frontend-runtime";
import type { SettingsDto } from "@floatctf/sdk";
import { useCallback, useEffect, useMemo, useState } from "react";

import { adminApi } from "@/api";
import { useMsgBanner } from "@/components";

/** 平台设置键：当前生效的前端 ID（与后端 `FRONTEND_ACTIVE` 一致）。 */
const FRONTEND_ACTIVE_KEY = "FRONTEND_ACTIVE";

const REGISTRY_QUERY_KEY = ["frontend-registry"];
const ACTIVE_QUERY_KEY = ["frontend-active-setting"];

/**
 * 已安装前端选择器（管理端 → 设置）。
 *
 * 职责**仅限"选择一个已经装好的前端"**：
 * - 前端从**本地注册表**读取（`/__floatctf/frontends/registry.json`，同源静态文件）
 * - 选择结果写回既有动态设置 `FRONTEND_ACTIVE`（走既有认证设置 API）
 * - 切换后提示并一键刷新页面（router 由各前端自己拥有，运行中替换没有意义）
 *
 * 刻意**不提供**：安装、从 Git 克隆、构建、上传制品。
 * 这些属于运维/CLI（`sudo /var/lib/floatctf/frontend.sh install <repo|artifact>`）——
 * 浏览器端永远不接受"让后端去克隆并执行构建"的请求（见信任模型与安全审查）。
 *
 * Frontend ≠ Theme：一个前端是**完整的可替换浏览器应用**（自己的路由/布局/UX），
 * 不是配色方案；配色由某个前端自己决定是否实现（例如 light/dark）。
 */
export function FrontendSelector() {
	const queryClient = useQueryClient();
	const banner = useMsgBanner({ duration: 6000 });
	const [selected, setSelected] = useState<string | null>(null);
	const [switched, setSwitched] = useState<string | null>(null);

	// ── 本地注册表（真实静态文件；解析用 bootstrap 的同一套权威校验器）──
	const registryQuery = useQuery({
		queryKey: REGISTRY_QUERY_KEY,
		staleTime: 0, // 新装前端后必须立刻可见：不做缓存
		queryFn: async (): Promise<FloatCTFRegistry> => {
			const response = await fetch(DEFAULT_REGISTRY_URL, {
				headers: { Accept: "application/json" },
				cache: "no-store",
				credentials: "same-origin",
			});
			if (!response.ok) {
				throw new Error(
					`读取本地前端注册表失败：HTTP ${response.status}（${DEFAULT_REGISTRY_URL}）`,
				);
			}
			const parsed = parseRegistry(await response.json());
			if (!parsed.ok) {
				throw new Error(`本地前端注册表不合法：${parsed.errors.join("；")}`);
			}
			return parsed.registry;
		},
	});

	// ── 当前 FRONTEND_ACTIVE（既有设置接口；受保护键，可编辑不可删除）──
	const activeQuery = useQuery({
		queryKey: ACTIVE_QUERY_KEY,
		queryFn: async (): Promise<SettingsDto | null> => {
			const response = await adminApi.settings.fetch();
			const rows = response.data ?? [];
			return rows.find((row) => row.key === FRONTEND_ACTIVE_KEY) ?? null;
		},
	});

	useEffect(() => {
		if (activeQuery.data && selected === null) {
			setSelected(activeQuery.data.value);
		}
	}, [activeQuery.data, selected]);

	const registry = registryQuery.data;
	const activeRow = activeQuery.data ?? null;

	const options = useMemo(() => {
		if (!registry) return [];
		return Object.keys(registry.frontends)
			.sort()
			.map((id) => {
				const entry = registry.frontends[id];
				const version = entry.versions[entry.currentVersion];
				const runtimeOk = version.compatibility.frontendRuntime === FRONTEND_RUNTIME_VERSION;
				const apiOk = version.compatibility.apiContract === API_CONTRACT_VERSION;
				return {
					id,
					protected: entry.protected,
					version: entry.currentVersion,
					name: version.name,
					installedAt: version.installedAt,
					entry: version.entry,
					compatible: runtimeOk && apiOk,
					incompatibility: !runtimeOk
						? `需要前端运行时契约 ${version.compatibility.frontendRuntime}，平台为 ${FRONTEND_RUNTIME_VERSION}`
						: !apiOk
							? `需要 API 契约 ${version.compatibility.apiContract}，平台为 ${API_CONTRACT_VERSION}`
							: null,
				};
			});
	}, [registry]);

	const patchMutation = useMutation({
		mutationFn: (value: string) => {
			if (!activeRow) throw new Error("设置 FRONTEND_ACTIVE 不存在，无法切换");
			return adminApi.settings.patch({ id: activeRow.id, value });
		},
		onSuccess: (_data, value) => {
			banner.showBanner(
				"success",
				`已切换到前端「${value}」。前端由浏览器在页面加载时解析，请刷新页面使其生效。`,
			);
			setSwitched(value);
			void queryClient.invalidateQueries({ queryKey: ACTIVE_QUERY_KEY });
			// 设置表（GenericTable 用 ["Settings", page, limit]）也同步刷新。
			void queryClient.invalidateQueries({ queryKey: ["Settings"] });
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	const isLoading = registryQuery.isLoading || activeQuery.isLoading;
	const currentValue = activeRow?.value ?? "";
	const dirty = selected !== null && selected !== currentValue;
	const selectedOption = options.find((option) => option.id === selected);
	const reload = useCallback(() => {
		window.location.reload();
	}, []);

	return (
		<div className="m-3 flex flex-col gap-3">
			<banner.BannerComponent />

			<div>
				<Heading as="h2" variant="small">
					前端（Frontend）
				</Heading>
				<Text as="p" className="text-sm color-fg-muted mt-1">
					前端是「完整的可替换浏览器应用」（自己的路由、布局、导航与交互），不是配色主题。
					这里只能选择一个已经安装好的前端；安装/升级/回滚由运维在宿主执行
					<Text className="font-mono"> sudo /var/lib/floatctf/frontend.sh install … </Text>
					，浏览器不会去克隆仓库或执行构建。
				</Text>
			</div>

			{activeQuery.isError ? (
				<Text as="p" className="text-sm color-danger-fg">
					读取设置 {FRONTEND_ACTIVE_KEY} 失败：
					{activeQuery.error instanceof Error ? activeQuery.error.message : String(activeQuery.error)}
				</Text>
			) : null}

			{registryQuery.isError ? (
				<Text as="p" className="text-sm color-danger-fg">
					{registryQuery.error instanceof Error
						? registryQuery.error.message
						: String(registryQuery.error)}
				</Text>
			) : null}

			{!registryQuery.isError && activeQuery.data === null && !activeQuery.isLoading ? (
				<Text as="p" className="text-sm color-attention-fg">
					设置表里没有 {FRONTEND_ACTIVE_KEY}（平台会在启动时自动补种默认值 default）。
				</Text>
			) : null}

			{isLoading ? <Spinner size="small" /> : null}

			{!isLoading && options.length > 0 ? (
				<>
					<FormControl>
						<FormControl.Label>当前生效的前端（{FRONTEND_ACTIVE_KEY}）</FormControl.Label>
						<Select
							value={selected ?? ""}
							disabled={!activeRow || patchMutation.isPending}
							onChange={(event) => setSelected(event.target.value)}
						>
							{options.map((option) => (
								<Select.Option
									key={option.id}
									value={option.id}
									disabled={!option.compatible}
								>
									{option.id} · {option.name} · v{option.version}
									{option.protected ? "（平台内置）" : ""}
									{option.compatible ? "" : "（契约不兼容，已禁用）"}
								</Select.Option>
							))}
						</Select>
						<FormControl.Caption>
							只列出本地注册表里已安装的前端；未安装的 ID 不会被展示，也无法在此提交。
						</FormControl.Caption>
					</FormControl>

					{selectedOption ? (
						<div className="flex flex-wrap items-center gap-2 text-sm">
							<Label variant={selectedOption.protected ? "accent" : "secondary"}>
								{selectedOption.protected ? "平台内置" : "第三方安装"}
							</Label>
							<Label variant={selectedOption.compatible ? "success" : "attention"}>
								{selectedOption.compatible ? "契约兼容" : "契约不兼容"}
							</Label>
							<Text className="color-fg-muted">
								版本 {selectedOption.version} · 入口{" "}
								<span className="font-mono">{selectedOption.entry}</span> · 安装于{" "}
								{selectedOption.installedAt}
							</Text>
							{selectedOption.incompatibility ? (
								<Text className="color-attention-fg">{selectedOption.incompatibility}</Text>
							) : null}
						</div>
					) : null}

					<div className="flex items-center gap-2">
						<Button
							variant="primary"
							disabled={!dirty || patchMutation.isPending || !activeRow}
							onClick={() => {
								if (selected) patchMutation.mutate(selected);
							}}
						>
							{patchMutation.isPending ? "保存中…" : "保存并切换"}
						</Button>
						<Button onClick={reload} disabled={!switched}>
							刷新页面
						</Button>
						{switched && !dirty ? (
							<Text className="text-sm color-fg-muted">
								已保存为「{switched}」；刷新页面后由该前端接管界面。
							</Text>
						) : null}
					</div>

					<Text as="p" className="text-sm color-fg-muted">
						破窗恢复：若自定义前端加载失败，任意页面加上{" "}
						<Text className="font-mono">?frontend=default</Text>{" "}
						即可用内置前端打开（只影响当前这次加载，不改动本设置，也不需要登录）。
					</Text>
				</>
			) : null}
		</div>
	);
}
