/**
 * 管理端 · 已安装前端选择器（`/admin/platform/frontends`，specialized 能力）。
 *
 * ## 逃生舱声明（PUBLIC SDK GAP · class B）
 *
 * 本地前端注册表**没有** SDK 抽象：它不是一个后端 API，而是宿主 `$FLOATCTF_HOME/frontends/`
 * 下由 Caddy 只读公开的同源静态文件 `/__floatctf/frontends/registry.json`
 * （见 `packages/frontend-runtime/src/version.ts::DEFAULT_REGISTRY_URL`）。
 * 因此这里用同源 `fetch(DEFAULT_REGISTRY_URL)` 取数，并用运行时提供的**权威校验器**
 * `parseRegistry` 做 schema 校验（严格白名单，fail-closed）。
 *
 * 语义边界（与平台一致）：
 * - 注册表描述「装了什么」，平台设置 `FRONTEND_ACTIVE` 描述「用哪个」；
 * - 注册表 404 / 非 JSON / schema 校验失败**必须**显示具体错误并提供重试，
 *   **绝不**当成「空注册表」（否则会出现「前端突然消失」而无法定位）；
 * - 本页只做「选择已安装的前端」，不安装 / 不构建 / 不克隆（那是宿主 CLI 的职责）。
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ExternalLink, Info, RefreshCw, RotateCw, ShieldAlert } from "lucide-react";
import { Link } from "react-router";

import {
	API_CONTRACT_VERSION,
	DEFAULT_REGISTRY_URL,
	FRONTEND_RUNTIME_VERSION,
	isMajorCompatible,
	parseRegistry,
	type FloatCTFRegistry,
	type FloatCTFRegistryFrontend,
	type FloatCTFRegistryVersion,
} from "@floatctf/frontend-runtime";
import type { SettingsDto } from "@floatctf/sdk";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	CopyText,
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	InlineError,
	LoadingBlock,
	RefreshingBadge,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { AbsoluteTime } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetHeader,
	SheetTitle,
} from "~/components/ui/sheet";
import { useDocumentTitle } from "~/lib/hooks";

import { textOf } from "./components";

const FRONTEND_ACTIVE_KEY = "FRONTEND_ACTIVE";

interface RegistryError {
	message: string;
	details: string[];
}

type RegistryState =
	| { status: "loading" }
	| { status: "error"; error: RegistryError }
	| { status: "ok"; registry: FloatCTFRegistry };

interface CompatInfo {
	compatible: boolean;
	reasons: string[];
}

/** 契约兼容性判定（与 `resolveFrontend` 同一套 major 规则）。 */
function checkCompatibility(version: FloatCTFRegistryVersion): CompatInfo {
	const reasons: string[] = [];
	if (!isMajorCompatible(version.compatibility.frontendRuntime, FRONTEND_RUNTIME_VERSION)) {
		reasons.push(
			`需要前端运行时契约 ${version.compatibility.frontendRuntime}，平台为 ${FRONTEND_RUNTIME_VERSION}`,
		);
	}
	if (!isMajorCompatible(version.compatibility.apiContract, API_CONTRACT_VERSION)) {
		reasons.push(
			`需要 API 契约 ${version.compatibility.apiContract}，平台为 ${API_CONTRACT_VERSION}`,
		);
	}
	return { compatible: reasons.length === 0, reasons };
}

function currentVersionOf(entry: FloatCTFRegistryFrontend): FloatCTFRegistryVersion | null {
	return entry.versions[entry.currentVersion] ?? null;
}

export function AdminFrontendsPage(): ReactNode {
	useDocumentTitle("前端选择器 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [state, setState] = useState<RegistryState>({ status: "loading" });
	const [selected, setSelected] = useState<string | null>(null);
	const [switched, setSwitched] = useState<string | null>(null);
	const [detailId, setDetailId] = useState<string | null>(null);

	// ── 本地注册表（同源静态文件 + 运行时权威校验器）—— 逃生舱，见文件头声明 ──
	const loadRegistry = useCallback(async () => {
		setState({ status: "loading" });
		try {
			const response = await fetch(DEFAULT_REGISTRY_URL, {
				headers: { Accept: "application/json" },
				cache: "no-store",
				credentials: "same-origin",
			});
			if (!response.ok) {
				setState({
					status: "error",
					error: {
						message: `读取本地前端注册表失败：HTTP ${response.status} ${response.statusText}`,
						details: [
							`地址：${DEFAULT_REGISTRY_URL}`,
							response.status === 404
								? "文件不存在：宿主可能未安装任何前端制品，或静态前缀未挂载。"
								: "同源静态文件不可读，请检查反向代理 / 挂载配置。",
						],
					},
				});
				return;
			}
			const text = await response.text();
			const parsed = parseRegistry(text);
			if (!parsed.ok) {
				setState({
					status: "error",
					error: {
						message: "本地前端注册表校验失败：registry.json 不符合注册表 schema",
						details: parsed.errors,
					},
				});
				return;
			}
			setState({ status: "ok", registry: parsed.registry });
		} catch (error) {
			setState({
				status: "error",
				error: {
					message: `无法读取本地前端注册表：${error instanceof Error ? error.message : String(error)}`,
					details: [
						`地址：${DEFAULT_REGISTRY_URL}`,
						"请确认当前页面与平台同源，且静态文件可访问。",
					],
				},
			});
		}
	}, []);

	useEffect(() => {
		void loadRegistry();
	}, [loadRegistry]);

	// ── 当前生效前端（平台设置 FRONTEND_ACTIVE，可编辑不可删除）──
	const activeQuery = useQuery({
		queryKey: qk.admin.settings({}),
		queryFn: () => callList<SettingsDto>(client.admin.settings.fetch()),
	});

	const activeRow = useMemo(
		() => (activeQuery.data?.items ?? []).find((row) => row.key === FRONTEND_ACTIVE_KEY) ?? null,
		[activeQuery.data],
	);
	const currentValue = activeRow?.value ?? "";

	useEffect(() => {
		if (activeRow && selected === null) setSelected(activeRow.value);
	}, [activeRow, selected]);

	const patchMutation = useMutation({
		mutationFn: (value: string) => {
			if (!activeRow) throw new Error("平台设置里没有 FRONTEND_ACTIVE，无法切换");
			return call(client.admin.settings.patch({ id: activeRow.id, value }), "切换前端");
		},
		onSuccess: (_data, value) => {
			toast.success(`已切换到前端「${value}」`, "刷新页面后由新前端接管界面。");
			setSwitched(value);
			void queryClient.invalidateQueries({ queryKey: qk.admin.settings() });
		},
		onError: (error) => toast.apiError("切换前端失败", error),
	});

	const registry = state.status === "ok" ? state.registry : null;

	const rows = useMemo(() => {
		if (!registry) return [] as FloatCTFRegistryFrontend[];
		return Object.values(registry.frontends).sort((a, b) => a.id.localeCompare(b.id));
	}, [registry]);

	const options = useMemo(() => {
		return rows.map((entry) => {
			const version = currentVersionOf(entry);
			const compat: CompatInfo = version
				? checkCompatibility(version)
				: { compatible: false, reasons: [`当前版本 ${entry.currentVersion} 不在 versions 中`] };
			return { entry, version, compat };
		});
	}, [rows]);

	const selectedOption = options.find((option) => option.entry.id === selected) ?? null;
	const dirty = selected !== null && selected !== currentValue;
	const detailEntry = rows.find((entry) => entry.id === detailId) ?? null;

	async function submitSwitch() {
		if (!selected || !selectedOption) return;
		if (!selectedOption.compat.compatible) {
			toast.warning("该前端与当前平台契约不兼容", selectedOption.compat.reasons.join("；"));
			return;
		}
		const ok = await confirm({
			title: `切换生效前端到「${selected}」？`,
			description: "只修改平台设置 FRONTEND_ACTIVE；已打开的页面需要刷新才会换前端。",
			consequences: [
				`所有用户下次加载页面都会使用「${selected}」`,
				"各前端的路由 / 布局 / 交互完全不同，用户会看到另一套界面",
				"本前端的管理端页面只能通过 ?frontend=default 或改回该设置来恢复",
			],
			confirmText: "保存并切换",
		});
		if (!ok) return;
		patchMutation.mutate(selected);
	}

	const columns: DataTableColumn<FloatCTFRegistryFrontend>[] = [
		{
			id: "id",
			header: "前端 ID",
			sortValue: (row) => row.id,
			cell: (row) => (
				<span className="inline-flex items-center gap-1.5">
					<MonoText className="font-medium">{row.id}</MonoText>
					{row.protected ? <TonePill tone="info">平台内置</TonePill> : null}
					{row.id === currentValue ? <TonePill tone="success">生效中</TonePill> : null}
				</span>
			),
		},
		{
			id: "name",
			header: "名称",
			cell: (row) => <span>{currentVersionOf(row)?.name ?? "—"}</span>,
		},
		{
			id: "version",
			header: "当前版本",
			cell: (row) => <MonoText>{row.currentVersion}</MonoText>,
		},
		{
			id: "compat",
			header: "契约兼容",
			cell: (row) => {
				const version = currentVersionOf(row);
				if (!version) return <TonePill tone="danger">版本缺失</TonePill>;
				const compat = checkCompatibility(version);
				return compat.compatible ? (
					<TonePill tone="success" icon={<CheckCircle2 />}>
						兼容
					</TonePill>
				) : (
					<TonePill tone="warning" title={compat.reasons.join("；")}>
						不兼容
					</TonePill>
				);
			},
		},
		{
			id: "runtime",
			header: "runtime / api",
			hideBelow: "md",
			cell: (row) => {
				const version = currentVersionOf(row);
				return (
					<MonoText className="text-muted-foreground">
						{version?.compatibility.frontendRuntime ?? "—"} /{" "}
						{version?.compatibility.apiContract ?? "—"}
					</MonoText>
				);
			},
		},
		{
			id: "entry",
			header: "入口",
			hideBelow: "lg",
			cell: (row) => (
				<MonoText className="text-muted-foreground">
					{currentVersionOf(row)?.entry ?? "—"}
				</MonoText>
			),
		},
		{
			id: "installedAt",
			header: "安装时间",
			align: "right",
			hideBelow: "sm",
			sortValue: (row) => Date.parse(currentVersionOf(row)?.installedAt ?? ""),
			cell: (row) => {
				const version = currentVersionOf(row);
				return version ? <AbsoluteTime value={version.installedAt} /> : <span>—</span>;
			},
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="前端"
				description="选择平台当前生效的可插拔前端。前端是完整的浏览器应用（自己的路由 / 布局 / 交互），不是配色主题。"
				badge={<TonePill tone="muted">specialized</TonePill>}
				actions={
					<Toolbar>
						<Button
							variant="outline"
							size="sm"
							onClick={() => void loadRegistry()}
							disabled={state.status === "loading"}
						>
							<RefreshCw />
							重读注册表
						</Button>
						<Button
							variant="outline"
							size="sm"
							asChild
						>
							<Link to="/admin/platform/settings">
								<ExternalLink />
								设置页面
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard
					label="已安装前端"
					value={registry ? rows.length : "…"}
					hint={registry ? `注册表更新于 ${registry.updatedAt}` : "等待注册表"}
					icon={<Info className="size-5" />}
				/>
				<StatCard
					label="平台运行时契约"
					value={FRONTEND_RUNTIME_VERSION}
					hint="FRONTEND_RUNTIME_VERSION（编译期常量）"
				/>
				<StatCard
					label="API 契约"
					value={API_CONTRACT_VERSION}
					hint="API_CONTRACT_VERSION（编译期常量）"
				/>
				<StatCard
					label="当前生效"
					value={currentValue || "—"}
					hint={
						activeRow
							? `FRONTEND_ACTIVE · 更新于 ${activeRow.updated_at}`
							: "平台设置里没有 FRONTEND_ACTIVE"
					}
					tone={activeRow ? "success" : "warning"}
				/>
			</div>

			<SectionCard
				title="注册表（本地静态文件）"
				description="只列出宿主已安装的前端；安装 / 升级 / 回滚由运维在宿主执行 frontend.sh。"
				actions={<RefreshingBadge active={state.status === "loading"} />}
			>
				{state.status === "loading" ? (
					<LoadingBlock label="读取注册表…" />
				) : state.status === "error" ? (
					<ErrorBlock
						error={new Error(state.error.message)}
						title="读取本地前端注册表失败"
						onRetry={() => void loadRegistry()}
					/>
				) : (
					<div className="space-y-4">
						<KeyValueList
							columns={3}
							items={[
								{
									key: "注册表地址",
									value: <CopyText value={DEFAULT_REGISTRY_URL} label="注册表地址" />,
								},
								{
									key: "schemaVersion",
									value: <MonoText>{String(state.registry.schemaVersion)}</MonoText>,
									hint: "FRONTEND_REGISTRY_SCHEMA_VERSION",
								},
								{
									key: "updatedAt",
									value: <AbsoluteTime value={state.registry.updatedAt} seconds />,
								},
							]}
						/>
						{rows.length === 0 ? (
							<EmptyBlock
								title="注册表里没有前端"
								description="宿主尚未安装任何前端制品；安装 default 后即可在此选择。"
							/>
						) : (
							<DataTable
								data={rows}
								getRowId={(row) => row.id}
								columns={columns}
								rowActions={(row) => (
									<Button variant="ghost" size="sm" onClick={() => setDetailId(row.id)}>
										版本明细
									</Button>
								)}
								mobileCard={(row) => {
									const version = currentVersionOf(row);
									const compat = version ? checkCompatibility(version) : null;
									return (
										<div className="space-y-2">
											<div className="flex items-center justify-between gap-2">
												<MonoText className="font-medium">{row.id}</MonoText>
												{compat ? (
													<TonePill tone={compat.compatible ? "success" : "warning"}>
														{compat.compatible ? "契约兼容" : "契约不兼容"}
													</TonePill>
												) : null}
											</div>
											<p className="text-sm">
												{version?.name ?? "—"} · v{row.currentVersion}
											</p>
											{compat && !compat.compatible ? (
												<p className="text-xs text-destructive">{compat.reasons.join("；")}</p>
											) : null}
											<Button variant="outline" size="sm" onClick={() => setDetailId(row.id)}>
												版本明细
											</Button>
										</div>
									);
								}}
							/>
						)}
					</div>
				)}
			</SectionCard>

			{state.status === "error" ? (
				<SectionCard
					title="注册表错误明细"
					description="fail-closed：读取 / 校验失败时不会退化成「空注册表」，错误逐条列出以便定位。"
				>
					<ul className="list-disc space-y-1 pl-5 text-sm">
						{state.error.details.map((detail) => (
							<li key={detail} className="text-muted-foreground break-all">
								{detail}
							</li>
						))}
					</ul>
				</SectionCard>
			) : null}

			<SectionCard
				title="切换生效前端"
				description="写入平台设置 FRONTEND_ACTIVE（受保护键，可改不可删）；刷新页面后生效。"
			>
				<div className="space-y-4">
					{activeQuery.isPending ? (
						<LoadingBlock label="读取 FRONTEND_ACTIVE…" />
					) : activeQuery.isError ? (
						<ErrorBlock
							error={activeQuery.error}
							title="读取平台设置失败"
							onRetry={() => void activeQuery.refetch()}
						/>
					) : !activeRow ? (
						<div className="flex items-start gap-2 rounded-md border border-[var(--warning)]/40 bg-[var(--warning)]/10 px-3 py-2 text-sm">
							<ShieldAlert className="mt-0.5 size-4 shrink-0" />
							<span>
								平台设置里没有 <span className="font-mono">{FRONTEND_ACTIVE_KEY}</span>
								（正常情况下后端启动时会补种默认值 default）。请先在
								<Link
									to="/admin/platform/settings"
									className="mx-1 underline underline-offset-4"
								>
									动态设置
								</Link>
								中创建该键，否则无法在此切换。
							</span>
						</div>
					) : (
						<>
							<div className="grid gap-4 lg:grid-cols-2">
								<div className="space-y-1.5">
									<p className="text-xs text-muted-foreground">
										当前生效的前端（{FRONTEND_ACTIVE_KEY}）
									</p>
									<Select
										value={selected ?? ""}
										onValueChange={setSelected}
										disabled={registry === null || patchMutation.isPending}
									>
										<SelectTrigger className="w-full" aria-label="选择生效前端">
											<SelectValue placeholder="选择已安装的前端" />
										</SelectTrigger>
										<SelectContent>
											{options.map((option) => (
												<SelectItem
													key={option.entry.id}
													value={option.entry.id}
													disabled={!option.compat.compatible}
												>
													{option.entry.id} · {option.version?.name ?? "?"} · v
													{option.entry.currentVersion}
													{option.entry.protected ? "（平台内置）" : ""}
													{option.compat.compatible ? "" : "（契约不兼容，已禁用）"}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
								<div className="space-y-1.5">
									<p className="text-xs text-muted-foreground">选中项详情</p>
									{selectedOption ? (
										<div className="space-y-1 text-sm">
											<div className="flex flex-wrap items-center gap-2">
												<BooleanPill
													value={selectedOption.compat.compatible}
													trueText="契约兼容"
													falseText="契约不兼容"
													trueTone="success"
													falseTone="warning"
												/>
												<BooleanPill
													value={selectedOption.entry.protected}
													trueText="平台内置"
													falseText="第三方安装"
												/>
											</div>
											<p className="text-xs text-muted-foreground">
												版本 {selectedOption.entry.currentVersion} · 入口{" "}
												<MonoText>{selectedOption.version?.entry ?? "—"}</MonoText>
											</p>
											{selectedOption.compat.reasons.length > 0 ? (
												<InlineError error={selectedOption.compat.reasons.join("；")} />
											) : null}
										</div>
									) : (
										<p className="text-sm text-muted-foreground">
											选择一个前端查看契约兼容性与入口。
										</p>
									)}
								</div>
							</div>

							<div className="flex flex-wrap items-center gap-2">
								<Button
									onClick={() => void submitSwitch()}
									disabled={!dirty || patchMutation.isPending || !activeRow}
								>
									{patchMutation.isPending ? "保存中…" : "保存并切换"}
								</Button>
								<Button
									variant="outline"
									onClick={() => window.location.reload()}
									disabled={switched === null}
								>
									<RotateCw />
									刷新页面
								</Button>
								{switched && !dirty ? (
									<p className="text-xs text-muted-foreground">
										已保存为「{switched}」；刷新页面后由该前端接管界面。
									</p>
								) : null}
								{activeQuery.isFetching ? (
									<RefreshingBadge active={activeQuery.isFetching && !activeQuery.isPending} />
								) : null}
							</div>

							<p className="text-xs text-muted-foreground">
								破窗恢复：任意页面加上 <span className="font-mono">?frontend=default</span>{" "}
								即可用内置前端打开（只影响本次加载，不改动本设置）。
							</p>
						</>
					)}
				</div>
			</SectionCard>

			<Sheet open={detailEntry !== null} onOpenChange={(open) => !open && setDetailId(null)}>
				<SheetContent className="flex w-full flex-col gap-0 overflow-y-auto sm:max-w-2xl">
					<SheetHeader>
						<SheetTitle>
							{detailEntry ? `${detailEntry.id} 的已安装版本` : "版本明细"}
						</SheetTitle>
						<SheetDescription>
							每个版本都必须显式声明与平台契约的兼容性；当前版本由注册表 currentVersion 指针决定。
						</SheetDescription>
					</SheetHeader>
					{detailEntry ? (
						<div className="space-y-3 px-4 pb-6">
							<KeyValueList
								columns={2}
								items={[
									{ key: "前端 ID", value: <MonoText>{detailEntry.id}</MonoText> },
									{
										key: "currentVersion",
										value: <MonoText>{detailEntry.currentVersion}</MonoText>,
									},
									{
										key: "protected",
										value: (
											<BooleanPill
												value={detailEntry.protected}
												trueText="平台内置（不可移除）"
												falseText="第三方安装"
											/>
										),
									},
									{
										key: "已安装版本数",
										value: <MonoText>{Object.keys(detailEntry.versions).length}</MonoText>,
									},
								]}
							/>
							<div className="space-y-2">
								{Object.entries(detailEntry.versions).map(([versionKey, version]) => {
									const compat = checkCompatibility(version);
									return (
										<div key={versionKey} className="space-y-1 rounded-md border px-3 py-2">
											<div className="flex flex-wrap items-center gap-2">
												<MonoText className="font-medium">v{versionKey}</MonoText>
												<span className="text-sm">{version.name}</span>
												{versionKey === detailEntry.currentVersion ? (
													<TonePill tone="success">当前版本</TonePill>
												) : null}
												<TonePill tone={compat.compatible ? "success" : "warning"}>
													{compat.compatible ? "契约兼容" : "契约不兼容"}
												</TonePill>
											</div>
											<p className="text-xs text-muted-foreground">
												runtime {version.compatibility.frontendRuntime} · api{" "}
												{version.compatibility.apiContract}
												{version.compatibility.sdk
													? ` · sdk ${version.compatibility.sdk}`
													: ""}
											</p>
											<p className="text-xs text-muted-foreground">
												入口 <MonoText>{version.entry}</MonoText> · 样式{" "}
												<MonoText>{version.styles.join(", ") || "（无）"}</MonoText>
											</p>
											<p className="text-xs text-muted-foreground">
												安装于 <AbsoluteTime value={version.installedAt} seconds />
												{version.author ? ` · 作者 ${textOf(version.author)}` : ""}
											</p>
											{version.description ? (
												<p className="text-xs text-muted-foreground">
													{version.description}
												</p>
											) : null}
											{compat.reasons.length > 0 ? (
												<InlineError error={compat.reasons.join("；")} />
											) : null}
										</div>
									);
								})}
							</div>
						</div>
					) : null}
				</SheetContent>
			</Sheet>
		</PageBody>
	);
}
