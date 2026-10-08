/**
 * GameBox 库管理（`/admin/gameboxes`）—— 导入 / 扫描 / 校验 / 构建 / 隐藏 / 删除 / 更新。
 *
 * 真实接口（`client.awd.admin.*`，与 `client.service.awd` 无关的管理端作用域）：
 * `listGameboxes` / `importGamebox(package_zip)` / `updateGamebox(id, body)` /
 * `hideGamebox(id)` / `removeGamebox(id_list)` / `scanGameboxes()` /
 * `checkGameboxes(gamebox_id_list?)` / `buildGameboxes(gamebox_id_list?)`。
 *
 * JSON 字段陷阱（已按后端源码核实）：
 * - **读**：`healthchecks_json` / `judge_args_json` 是 `unknown | null`；
 * - **写**：`PATCH` 要求 JSON **字符串**（后端会 `serde_json::from_str`），显式 `null` = 清空；
 *   没改动的字段**不发**，避免把 `unknown` 重新序列化后写回。
 */

import { useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
	EyeOff,
	FileUp,
	Hammer,
	Pencil,
	RefreshCw,
	ScanSearch,
	ShieldCheck,
	Trash2,
} from "lucide-react";

import { call, callList, uploadFile } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { DropdownMenuItem, DropdownMenuSeparator } from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { Progress } from "~/components/ui/progress";
import { Switch } from "~/components/ui/switch";
import { Textarea } from "~/components/ui/textarea";
import { formatBytes } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type {
	GameBoxBuildResult,
	GameBoxCheckResult,
	GameBoxLibraryDto,
	GameBoxScanItem,
	ImportGameBoxResponse,
	QueryParams,
} from "@floatctf/sdk";

import { BuildResultTable, ScanResultTable } from "./ops";
import {
	BuildStatusPill,
	ResultsCard,
	RowMenu,
	SearchInput,
	formatJsonText,
	parseIntInput,
	parseJsonInput,
	useAdminListState,
} from "./shared";

const SEARCH_KEYS = ["name", "safe_name", "category"];

interface GameboxFormState {
	name: string;
	category: string;
	description: string;
	hidden: boolean;
	username: string;
	cpuMillis: string;
	memoryBytes: string;
	pidsLimit: string;
	healthchecksJson: string;
	judgeScriptName: string;
	judgeScriptContent: string;
	judgeArgsJson: string;
	judgeTimeoutSecs: string;
	judgeRetryIntervalSecs: string;
}

type GameboxFormErrors = Partial<Record<keyof GameboxFormState, string>>;

interface GameboxOriginal {
	healthchecksJson: string;
	judgeArgsJson: string;
}

const EMPTY_FORM: GameboxFormState = {
	name: "",
	category: "",
	description: "",
	hidden: false,
	username: "",
	cpuMillis: "",
	memoryBytes: "",
	pidsLimit: "",
	healthchecksJson: "",
	judgeScriptName: "",
	judgeScriptContent: "",
	judgeArgsJson: "",
	judgeTimeoutSecs: "",
	judgeRetryIntervalSecs: "",
};

/** JSON 文本字段的统一取值：空 → null（清空）；与原值相同 → undefined（不动）；否则必须是合法 JSON。 */
function resolveJsonPatch(
	text: string,
	original: string,
	field: keyof GameboxFormErrors,
	errors: GameboxFormErrors,
): string | null | undefined {
	const trimmed = text.trim();
	if (trimmed === "") return null;
	if (trimmed === original.trim()) return undefined;
	const parsed = parseJsonInput(trimmed);
	if (!parsed.ok) {
		errors[field] = `不是合法 JSON：${parsed.error}`;
		return undefined;
	}
	return JSON.stringify(parsed.value);
}

function validate(form: GameboxFormState, original: GameboxOriginal): GameboxFormErrors {
	const errors: GameboxFormErrors = {};
	if (!form.name.trim()) errors.name = "名称不能为空";
	if (!form.category.trim()) errors.category = "分类不能为空";

	for (const [key, label] of [
		["cpuMillis", "推荐 CPU"],
		["memoryBytes", "推荐内存"],
		["pidsLimit", "推荐 PID 上限"],
	] as const) {
		const parsed = parseIntInput(form[key]);
		if (parsed.kind === "invalid") errors[key] = `${label}应为整数；留空表示不修改。`;
		else if (parsed.kind === "ok" && parsed.value <= 0) errors[key] = `${label}必须大于 0。`;
	}

	for (const [key, label] of [
		["judgeTimeoutSecs", "评测超时"],
		["judgeRetryIntervalSecs", "评测重试间隔"],
	] as const) {
		const parsed = parseIntInput(form[key]);
		if (parsed.kind === "invalid") errors[key] = `${label}应为整数（秒）；留空表示清空并使用赛事默认值。`;
		else if (parsed.kind === "ok" && parsed.value < 0) errors[key] = `${label}不能为负数。`;
	}

	resolveJsonPatch(form.healthchecksJson, original.healthchecksJson, "healthchecksJson", errors);
	resolveJsonPatch(form.judgeArgsJson, original.judgeArgsJson, "judgeArgsJson", errors);
	return errors;
}

export function AdminGameboxesPage(): ReactNode {
	useDocumentTitle("GameBox 库 · FloatCTF");
	const client = useClient();
	const queryClient = useAppQueryClient();
	const confirm = useConfirm();
	const list = useAdminListState(SEARCH_KEYS);

	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [editing, setEditing] = useState<GameBoxLibraryDto | null>(null);
	const [form, setForm] = useState<GameboxFormState>(EMPTY_FORM);
	const [original, setOriginal] = useState<GameboxOriginal>({
		healthchecksJson: "",
		judgeArgsJson: "",
	});
	const [errors, setErrors] = useState<GameboxFormErrors>({});

	const fileInputRef = useRef<HTMLInputElement>(null);
	const [file, setFile] = useState<File | null>(null);
	const [progress, setProgress] = useState(0);
	const [imported, setImported] = useState<GameBoxLibraryDto | null>(null);

	const [scanItems, setScanItems] = useState<GameBoxScanItem[] | null>(null);
	const [checkItems, setCheckItems] = useState<GameBoxCheckResult[] | null>(null);
	const [buildItems, setBuildItems] = useState<GameBoxBuildResult[] | null>(null);

	const params: QueryParams = {
		page: list.page,
		limit: list.pageSize,
		...(list.filter ? { filter: list.filter } : {}),
	};

	const query = useQuery({
		queryKey: qk.awd.library(params),
		queryFn: () => callList<GameBoxLibraryDto>(client.awd.admin.listGameboxes(params)),
	});

	function invalidate(): void {
		void queryClient.invalidateQueries({ queryKey: qk.awd.library() });
	}

	const save = useMutation({
		mutationFn: (input: { id: string; body: GameboxFormState; original: GameboxOriginal }) => {
			const { body } = input;
			const cpu = parseIntInput(body.cpuMillis);
			const memory = parseIntInput(body.memoryBytes);
			const pids = parseIntInput(body.pidsLimit);
			const timeout = parseIntInput(body.judgeTimeoutSecs);
			const retry = parseIntInput(body.judgeRetryIntervalSecs);
			// 空串在推荐资源字段上等于「不修改」（后端 `Option<i64>`，null 不更新）。
			return call<GameBoxLibraryDto>(
				client.awd.admin.updateGamebox(input.id, {
					name: body.name.trim(),
					category: body.category.trim(),
					description: body.description,
					hidden: body.hidden,
					// username / judge_script_* 的空串由后端归一化为「清空」。
					username: body.username,
					recommended_cpu_millis: cpu.kind === "ok" ? cpu.value : undefined,
					recommended_memory_bytes: memory.kind === "ok" ? memory.value : undefined,
					recommended_pids_limit: pids.kind === "ok" ? pids.value : undefined,
					healthchecks_json: resolveJsonPatch(
						body.healthchecksJson,
						input.original.healthchecksJson,
						"healthchecksJson",
						{},
					),
					judge_script_name: body.judgeScriptName,
					judge_script_content: body.judgeScriptContent,
					judge_args_json: resolveJsonPatch(
						body.judgeArgsJson,
						input.original.judgeArgsJson,
						"judgeArgsJson",
						{},
					),
					// 空 = 清空（继承赛事默认值）。
					judge_timeout_secs: timeout.kind === "ok" ? timeout.value : null,
					judge_retry_interval_secs: retry.kind === "ok" ? retry.value : null,
				}),
				"更新 GameBox",
			);
		},
		onSuccess: () => {
			toast.success("GameBox 已更新");
			invalidate();
			setEditing(null);
			setErrors({});
		},
		onError: (error) => toast.apiError("更新 GameBox 失败", error),
	});

	const hide = useMutation({
		mutationFn: (id: string) => call<null>(client.awd.admin.hideGamebox(id), "隐藏 GameBox"),
		onSuccess: () => {
			toast.success("GameBox 已隐藏", "选手端与赛事选择器不再列出它。");
			invalidate();
		},
		onError: (error) => toast.apiError("隐藏 GameBox 失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) => call<number>(client.awd.admin.removeGamebox(ids), "删除 GameBox"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 个 GameBox`, "磁盘上的 package 目录已由后端一并清理。");
			invalidate();
			setSelectedIds([]);
		},
		onError: (error) => toast.apiError("删除 GameBox 失败", error),
	});

	const importMutation = useMutation({
		mutationFn: (selected: File) =>
			uploadFile<ImportGameBoxResponse>({
				client,
				scope: "admin",
				url: "/awd/gameboxes/import",
				field: "package_zip",
				file: selected,
				onProgress: setProgress,
			}),
		onSuccess: (data) => {
			setImported(data.gamebox);
			setFile(null);
			setProgress(0);
			toast.success("GameBox 包导入完成", `靶机：${data.gamebox.name}`);
			invalidate();
		},
		onError: (error) => {
			setProgress(0);
			toast.apiError("导入 GameBox 包失败", error);
		},
	});

	const scan = useMutation({
		mutationFn: () =>
			call<GameBoxScanItem[]>(client.awd.admin.scanGameboxes(), "扫描 GameBox 目录"),
		onSuccess: (items) => {
			setScanItems(items);
			const failed = items.filter((item) => item.status === "error").length;
			if (failed > 0) toast.warning(`扫描完成：${items.length} 条，其中 ${failed} 条失败`);
			else toast.success(`扫描完成：${items.length} 条结果`);
			invalidate();
		},
		onError: (error) => toast.apiError("扫描 GameBox 目录失败", error),
	});

	const check = useMutation({
		mutationFn: (ids?: string[]) =>
			call<GameBoxCheckResult[]>(client.awd.admin.checkGameboxes(ids), "校验 GameBox"),
		onSuccess: (items) => {
			setCheckItems(items);
			const failed = items.filter((item) => !item.is_ok).length;
			if (failed > 0) toast.warning(`校验完成：${items.length} 个，${failed} 个不可用`);
			else toast.success(`校验完成：${items.length} 个全部可用`);
			invalidate();
		},
		onError: (error) => toast.apiError("校验 GameBox 失败", error),
	});

	const build = useMutation({
		mutationFn: (ids?: string[]) =>
			call<GameBoxBuildResult[]>(client.awd.admin.buildGameboxes(ids), "构建 GameBox"),
		onSuccess: (items) => {
			setBuildItems(items);
			const failed = items.filter((item) => !item.is_ok);
			if (items.length === 0) {
				toast.warning(
					"没有可构建的对象",
					"构建只对 build_status=ready 的 GameBox 生效，请先确认当前版本镜像已 pin。",
				);
			} else if (failed.length > 0) {
				toast.error(`${failed.length} 个 GameBox 构建失败`, failed[0]?.message);
			} else {
				toast.success(`${items.length} 个 GameBox 镜像已就绪`);
			}
			invalidate();
		},
		onError: (error) => toast.apiError("构建 GameBox 失败", error),
	});

	function openEdit(row: GameBoxLibraryDto): void {
		const nextOriginal: GameboxOriginal = {
			healthchecksJson: formatJsonText(row.healthchecks_json),
			judgeArgsJson: formatJsonText(row.judge_args_json),
		};
		setOriginal(nextOriginal);
		setForm({
			name: row.name,
			category: row.category,
			description: row.description,
			hidden: row.hidden,
			username: row.username ?? "",
			cpuMillis: row.cpu_millis === null ? "" : String(row.cpu_millis),
			memoryBytes: row.memory_bytes === null ? "" : String(row.memory_bytes),
			pidsLimit: row.pids_limit === null ? "" : String(row.pids_limit),
			healthchecksJson: nextOriginal.healthchecksJson,
			judgeScriptName: row.judge_script_name ?? "",
			judgeScriptContent: row.judge_script_content ?? "",
			judgeArgsJson: nextOriginal.judgeArgsJson,
			judgeTimeoutSecs:
				row.judge_timeout_secs === null || row.judge_timeout_secs === undefined
					? ""
					: String(row.judge_timeout_secs),
			judgeRetryIntervalSecs:
				row.judge_retry_interval_secs === null || row.judge_retry_interval_secs === undefined
					? ""
					: String(row.judge_retry_interval_secs),
		});
		setErrors({});
		setEditing(row);
	}

	function submit(): void {
		if (!editing) return;
		const nextErrors = validate(form, original);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) {
			toast.warning("请先修正表单中的错误");
			return;
		}
		save.mutate({ id: editing.id, body: form, original });
	}

	async function confirmHide(row: GameBoxLibraryDto): Promise<void> {
		const ok = await confirm({
			title: `隐藏 GameBox「${row.name}」？`,
			description: "隐藏是「归档」语义（hidden = true），不会删除镜像与磁盘目录。",
			consequences: [
				"选手端与赛事的 GameBox 选择器不再列出它",
				"已被赛事引用的现有实例不受影响；被引用时删除请求会被拒绝",
				"要恢复可见请用「编辑」把 hidden 关掉",
			],
			tone: "danger",
			confirmText: "隐藏",
			confirmPhrase: row.name,
		});
		if (ok) hide.mutate(row.id);
	}

	async function confirmRemoveOne(row: GameBoxLibraryDto): Promise<void> {
		const ok = await confirm({
			title: `删除 GameBox「${row.name}」？`,
			description: "该操作不可恢复。",
			consequences: [
				"数据库记录与磁盘 package 目录（GAMEBOXES_DIR/{safe_name}）一并删除",
				"已被任何赛事或 AWDP Run 引用时后端会拒绝删除，请先解除挂载",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: row.name,
		});
		if (ok) remove.mutate([row.id]);
	}

	async function confirmRemoveSelected(): Promise<void> {
		if (selectedIds.length === 0) {
			toast.warning("请先选择要删除的 GameBox");
			return;
		}
		const ok = await confirm({
			title: `删除选中的 ${selectedIds.length} 个 GameBox？`,
			description: "该操作不可恢复。",
			consequences: [
				`${selectedIds.length} 个 GameBox 的记录与磁盘 package 目录会被删除`,
				"被赛事 / AWDP Run 引用的条目后端会拒绝，需先解除挂载",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: "delete",
		});
		if (ok) remove.mutate(selectedIds);
	}

	async function confirmBuild(ids: string[]): Promise<void> {
		const ok = await confirm({
			title: ids.length === 1 ? "构建该 GameBox 的镜像？" : `构建选中的 ${ids.length} 个 GameBox？`,
			description: "构建 = 确保当前版本 pin 的镜像在本地 Docker 中存在（必要时按 digest 拉取）。",
			consequences: [
				"会占用宿主 Docker / 网络资源，镜像较大时可能耗时数分钟",
				"只对 build_status=ready 的对象生效，其它会被跳过",
				"每个对象的成功 / 失败消息逐条展示",
			],
			tone: "default",
			confirmText: "开始构建",
		});
		if (ok) build.mutate(ids.length > 0 ? ids : undefined);
	}

	async function confirmScan(): Promise<void> {
		const ok = await confirm({
			title: "扫描 GameBox 目录并登记未入库的 package？",
			description: "扫描后端配置的 GAMEBOXES_DIR，把磁盘上存在但数据库里没有的靶机包登记进来。",
			consequences: [
				"会向 GameBox 库写入新条目（已存在的包只会 skipped，不覆盖版本）",
				"结果逐条列出 added / skipped / error 及原因",
			],
			tone: "default",
			confirmText: "开始扫描",
		});
		if (ok) scan.mutate();
	}

	async function confirmImport(): Promise<void> {
		if (!file) return;
		const ok = await confirm({
			title: `导入 GameBox 包「${file.name}」？`,
			description: "导入 = 解压 package + 同步构建镜像并 pin digest（单版本模型，版本号严格递增）。",
			consequences: [
				"会占用宿主 Docker / 网络资源，构建可能耗时数分钟",
				"同名靶机会按递增版本 upsert 身份行，版本冲突时导入失败",
				"失败会写入 build_status=failed 与错误原因，可用「校验」查看",
			],
			tone: "default",
			confirmText: "开始导入",
		});
		if (ok) importMutation.mutate(file);
	}

	const columns: DataTableColumn<GameBoxLibraryDto>[] = [
		{
			id: "name",
			header: "GameBox",
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate text-sm font-medium">{row.name}</p>
					<div className="flex flex-wrap items-center gap-1.5">
						<MonoText className="text-muted-foreground">{row.safe_name}</MonoText>
						<span className="text-xs text-muted-foreground">· {row.category}</span>
					</div>
				</div>
			),
			sortValue: (row) => row.name,
		},
		{
			id: "version",
			header: "版本 / 构建",
			cell: (row) => (
				<div className="flex flex-wrap items-center gap-1">
					<MonoText>{row.version ?? "—"}</MonoText>
					<BuildStatusPill status={row.build_status} />
				</div>
			),
			sortValue: (row) => row.version ?? "",
		},
		{
			id: "image",
			header: "镜像",
			cell: (row) =>
				row.image_ref ? (
					<div className="min-w-0">
						<MonoText title={row.image_ref}>{row.image_ref}</MonoText>
						{row.image_repo_digest ? (
							<span className="ml-1 text-xs text-muted-foreground" title="已 pin 到 repo digest">
								🔒
							</span>
						) : null}
					</div>
				) : (
					<span className="text-xs text-muted-foreground">无镜像</span>
				),
			hideBelow: "lg",
		},
		{
			id: "runtime",
			header: "资源",
			cell: (row) => (
				<div className="tnum font-mono text-xs text-muted-foreground">
					{row.cpu_millis !== null ? `${row.cpu_millis}m` : "—"} /{" "}
					{row.memory_bytes !== null ? formatBytes(row.memory_bytes) : "—"} /{" "}
					{row.pids_limit ?? "—"} pids
				</div>
			),
			hideBelow: "xl",
		},
		{
			id: "judge",
			header: "评测脚本",
			cell: (row) => (
				<div className="space-y-0.5 text-xs">
					{row.judge_script_name ? (
						<MonoText>{row.judge_script_name}</MonoText>
					) : (
						<span className="text-muted-foreground">未配置脚本</span>
					)}
					<div className="tnum font-mono text-muted-foreground">
						超时 {row.judge_timeout_secs ?? "默认"}s · 重试 {row.judge_retry_interval_secs ?? "默认"}s
					</div>
				</div>
			),
			hideBelow: "xl",
		},
		{
			id: "healthchecks",
			header: "健康检查",
			cell: (row) =>
				row.healthchecks_json === null || row.healthchecks_json === undefined ? (
					<TonePill tone="muted">未配置</TonePill>
				) : (
					<TonePill tone="success" title={formatJsonText(row.healthchecks_json)}>
						已配置
					</TonePill>
				),
			hideBelow: "sm",
		},
		{
			id: "hidden",
			header: "可见性",
			cell: (row) => (
				<TonePill tone={row.hidden ? "muted" : "success"}>
					{row.hidden ? "已隐藏" : "公开"}
				</TonePill>
			),
			sortValue: (row) => (row.hidden ? 1 : 0),
		},
	];

	const canBuild = (row: GameBoxLibraryDto): boolean => row.build_status === "ready";

	return (
		<PageBody>
			<PageHeader
				title="GameBox 库"
				description="AWD / AWDP 靶机模板库：身份 + 当前版本 package。新条目只能通过导入或扫描进入。"
				actions={
					<Toolbar>
						<Button variant="outline" onClick={() => query.refetch()} disabled={query.isFetching}>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button onClick={() => fileInputRef.current?.click()} disabled={importMutation.isPending}>
							<FileUp /> 导入 GameBox 包
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="GameBox 列表"
				description="搜索在名称 / safe_name / 分类上做服务端 OR 匹配。"
				actions={
					<Toolbar>
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索名称 / safe_name / 分类"
						/>
						<Button
							variant="outline"
							size="sm"
							onClick={() => void confirmScan()}
							disabled={scan.isPending}
						>
							<ScanSearch /> {scan.isPending ? "扫描中…" : "扫描目录"}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => check.mutate(selectedIds.length > 0 ? selectedIds : undefined)}
							disabled={check.isPending}
						>
							<ShieldCheck />{" "}
							{check.isPending ? "校验中…" : selectedIds.length > 0 ? "校验选中" : "校验全部"}
						</Button>
						{selectedIds.length > 0 ? (
							<>
								<Button
									variant="outline"
									size="sm"
									onClick={() => void confirmBuild(selectedIds)}
									disabled={build.isPending}
								>
									<Hammer /> 构建选中（{selectedIds.length}）
								</Button>
								<Button
									variant="destructive"
									size="sm"
									onClick={() => void confirmRemoveSelected()}
									disabled={remove.isPending}
								>
									<Trash2 /> 删除选中（{selectedIds.length}）
								</Button>
							</>
						) : null}
					</Toolbar>
				}
				contentClassName="p-0"
				footer={
					// 数据到位前不渲染分页条，避免出现「共 0 条」这种与事实不符的文案。
					query.data ? (
						<PaginationBar
							page={list.page}
							pageSize={list.pageSize}
							total={query.data.meta?.total ?? 0}
							onPageChange={list.setPage}
							onPageSizeChange={list.setPageSize}
						/>
					) : null
				}
			>
				<input
					ref={fileInputRef}
					type="file"
					accept=".zip,application/zip"
					className="hidden"
					onChange={(event) => {
						const selected = event.target.files?.[0] ?? null;
						event.target.value = "";
						if (!selected) return;
						if (!selected.name.toLowerCase().endsWith(".zip")) {
							toast.error("只支持 ZIP 靶机包", "package zip 应包含 meta.toml 与运行所需文件。");
							return;
						}
						setFile(selected);
						setImported(null);
					}}
				/>

				{file ? (
					<div className="space-y-2 border-b bg-muted/30 px-4 py-3">
						<div className="flex flex-wrap items-center justify-between gap-2">
							<div className="min-w-0">
								<p className="truncate text-sm font-medium">{file.name}</p>
								<p className="tnum text-xs text-muted-foreground">
									{formatBytes(file.size)} · 导入为同步构建，可能耗时数分钟
								</p>
							</div>
							<div className="flex items-center gap-2">
								<Button variant="ghost" size="sm" onClick={() => setFile(null)}>
									取消
								</Button>
								<Button size="sm" onClick={() => void confirmImport()} disabled={importMutation.isPending}>
									<FileUp /> {importMutation.isPending ? "导入中…" : "开始导入"}
								</Button>
							</div>
						</div>
						{importMutation.isPending || progress > 0 ? (
							<div className="space-y-1">
								<Progress value={progress} aria-label="上传进度" />
								<p className="tnum text-xs text-muted-foreground">上传进度 {progress}%</p>
							</div>
						) : null}
					</div>
				) : null}

				{imported ? (
					<div className="flex flex-wrap items-center gap-3 border-b bg-[var(--success)]/10 px-4 py-2 text-sm">
						<span className="font-medium">导入成功：</span>
						<span>{imported.name}</span>
						<MonoText>{imported.safe_name}</MonoText>
						<MonoText>v{imported.version ?? "—"}</MonoText>
						<BuildStatusPill status={imported.build_status} />
					</div>
				) : null}

				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					errorTitle="加载 GameBox 库失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						list.filter ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的 GameBox"
								description={`没有名称 / safe_name / 分类匹配「${list.search}」的靶机。`}
							/>
						) : (
							<EmptyBlock
								title="GameBox 库是空的"
								description="导入靶机包，或扫描后端 GAMEBOXES_DIR 里已有的 package。"
								action={
									<Button size="sm" onClick={() => fileInputRef.current?.click()}>
										<FileUp /> 导入 GameBox 包
									</Button>
								}
							/>
						)
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							selectable
							selectedIds={selectedIds}
							onSelectionChange={setSelectedIds}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<div className="flex items-start justify-between gap-2">
										<span className="text-sm font-medium">{row.name}</span>
										<BuildStatusPill status={row.build_status} />
									</div>
									<MonoText className="text-muted-foreground">
										{row.safe_name} · {row.category} · v{row.version ?? "—"}
									</MonoText>
									{row.image_ref ? <MonoText>{row.image_ref}</MonoText> : null}
									<p className="tnum font-mono text-xs text-muted-foreground">
										{row.cpu_millis !== null ? `${row.cpu_millis}m` : "—"} /{" "}
										{row.memory_bytes !== null ? formatBytes(row.memory_bytes) : "—"} /{" "}
										{row.pids_limit ?? "—"} pids
									</p>
									<div className="flex flex-wrap gap-2 pt-1">
										<Button variant="outline" size="sm" onClick={() => openEdit(row)}>
											<Pencil /> 编辑
										</Button>
										<Button
											variant="outline"
											size="sm"
											onClick={() => check.mutate([row.id])}
											disabled={check.isPending}
										>
											<ShieldCheck /> 校验
										</Button>
										{canBuild(row) ? (
											<Button
												variant="outline"
												size="sm"
												onClick={() => void confirmBuild([row.id])}
												disabled={build.isPending}
											>
												<Hammer /> 构建
											</Button>
										) : null}
										{!row.hidden ? (
											<Button variant="outline" size="sm" onClick={() => void confirmHide(row)}>
												<EyeOff /> 隐藏
											</Button>
										) : null}
										<Button
											variant="destructive"
											size="sm"
											onClick={() => void confirmRemoveOne(row)}
										>
											<Trash2 /> 删除
										</Button>
									</div>
								</div>
							)}
							rowActions={(row) => (
								<RowMenu label={`GameBox ${row.name} 的操作`}>
									<DropdownMenuItem onSelect={() => openEdit(row)}>
										<Pencil /> 编辑
									</DropdownMenuItem>
									<DropdownMenuItem onSelect={() => check.mutate([row.id])}>
										<ShieldCheck /> 校验
									</DropdownMenuItem>
									{canBuild(row) ? (
										<DropdownMenuItem onSelect={() => void confirmBuild([row.id])}>
											<Hammer /> 构建镜像
										</DropdownMenuItem>
									) : null}
									{!row.hidden ? (
										<DropdownMenuItem onSelect={() => void confirmHide(row)}>
											<EyeOff /> 隐藏
										</DropdownMenuItem>
									) : null}
									<DropdownMenuSeparator />
									<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemoveOne(row)}>
										<Trash2 /> 删除
									</DropdownMenuItem>
								</RowMenu>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>

			{scanItems ? (
				<ResultsCard
					title="GameBox 目录扫描结果"
					description={`后端返回 ${scanItems.length} 条（added / skipped / error 原样展示）。`}
					onDismiss={() => setScanItems(null)}
				>
					<ScanResultTable items={scanItems} />
				</ResultsCard>
			) : null}

			{checkItems ? (
				<ResultsCard
					title="校验结果"
					description={`${checkItems.length} 个，${checkItems.filter((item) => !item.is_ok).length} 个不可用。`}
					onDismiss={() => setCheckItems(null)}
				>
					<GameboxCheckResultTable
						items={checkItems}
						onBuild={(id) => void confirmBuild([id])}
						building={build.isPending}
					/>
				</ResultsCard>
			) : null}

			{buildItems ? (
				<ResultsCard
					title="构建结果"
					description="逐个结果与后端消息（失败原因原样展示）。"
					onDismiss={() => setBuildItems(null)}
				>
					<BuildResultTable items={buildItems} nameOf={(row) => row.gamebox_name} label="GameBox" />
				</ResultsCard>
			) : null}

			<FormSheet
				open={editing !== null}
				onOpenChange={(open) => {
					if (!open) setEditing(null);
				}}
				title={`编辑 GameBox${editing ? `：${editing.name}` : ""}`}
				description="身份与运行参数可改；版本、镜像 digest 与 build_status 由导入决定。"
				width="xl"
				footer={
					<FormFooter
						onCancel={() => setEditing(null)}
						submitLabel="保存修改"
						isPending={save.isPending}
						formId="admin-gamebox-form"
						hint={
							editing
								? `safe_name=${editing.safe_name} · v${editing.version ?? "—"}`
								: undefined
						}
					/>
				}
			>
				<form
					id="admin-gamebox-form"
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<FormGrid columns={2}>
						<Field label="名称" htmlFor="gamebox-name" required error={errors.name}>
							<Input
								id="gamebox-name"
								value={form.name}
								onChange={(event) => setForm({ ...form, name: event.target.value })}
							/>
						</Field>
						<Field label="分类" htmlFor="gamebox-category" required error={errors.category}>
							<Input
								id="gamebox-category"
								value={form.category}
								onChange={(event) => setForm({ ...form, category: event.target.value })}
							/>
						</Field>
					</FormGrid>
					<Field label="描述" htmlFor="gamebox-description">
						<Textarea
							id="gamebox-description"
							rows={3}
							value={form.description}
							onChange={(event) => setForm({ ...form, description: event.target.value })}
						/>
					</Field>
					<div className="flex items-center justify-between rounded-lg border px-3 py-2">
						<div>
							<p className="text-sm font-medium">隐藏（归档）</p>
							<p className="text-xs text-muted-foreground">
								隐藏后选手端与赛事选择器不再列出它；已在使用的实例不受影响。
							</p>
						</div>
						<Switch
							checked={form.hidden}
							onCheckedChange={(checked) => setForm({ ...form, hidden: checked })}
							aria-label="隐藏 GameBox"
						/>
					</div>

					<FormGrid columns={2}>
						<Field
							label="容器内用户名"
							htmlFor="gamebox-username"
							hint="healthcheck / judge 在容器内执行时使用的用户；留空表示清空。"
						>
							<Input
								id="gamebox-username"
								value={form.username}
								onChange={(event) => setForm({ ...form, username: event.target.value })}
							/>
						</Field>
						<Field
							label="推荐 CPU（毫核）"
							htmlFor="gamebox-cpu"
							error={errors.cpuMillis}
							hint="留空表示不修改；必须大于 0。"
						>
							<Input
								id="gamebox-cpu"
								inputMode="numeric"
								value={form.cpuMillis}
								onChange={(event) => setForm({ ...form, cpuMillis: event.target.value })}
							/>
						</Field>
						<Field
							label="推荐内存（字节）"
							htmlFor="gamebox-memory"
							error={errors.memoryBytes}
							hint="留空表示不修改；必须大于 0。"
						>
							<Input
								id="gamebox-memory"
								inputMode="numeric"
								value={form.memoryBytes}
								onChange={(event) => setForm({ ...form, memoryBytes: event.target.value })}
							/>
						</Field>
						<Field
							label="推荐 PID 上限"
							htmlFor="gamebox-pids"
							error={errors.pidsLimit}
							hint="留空表示不修改；必须大于 0。"
						>
							<Input
								id="gamebox-pids"
								inputMode="numeric"
								value={form.pidsLimit}
								onChange={(event) => setForm({ ...form, pidsLimit: event.target.value })}
							/>
						</Field>
						<Field
							label="评测超时（秒）"
							htmlFor="gamebox-timeout"
							error={errors.judgeTimeoutSecs}
							hint="留空 = 清空，使用赛事默认值；最小 0。"
						>
							<Input
								id="gamebox-timeout"
								inputMode="numeric"
								value={form.judgeTimeoutSecs}
								onChange={(event) => setForm({ ...form, judgeTimeoutSecs: event.target.value })}
							/>
						</Field>
						<Field
							label="评测重试间隔（秒）"
							htmlFor="gamebox-retry"
							error={errors.judgeRetryIntervalSecs}
							hint="留空 = 清空，使用赛事默认值；最小 0。"
						>
							<Input
								id="gamebox-retry"
								inputMode="numeric"
								value={form.judgeRetryIntervalSecs}
								onChange={(event) => setForm({ ...form, judgeRetryIntervalSecs: event.target.value })}
							/>
						</Field>
						<Field
							label="评测脚本名"
							htmlFor="gamebox-judge-name"
							hint="留空表示清空（回退到包内默认脚本）。"
						>
							<Input
								id="gamebox-judge-name"
								value={form.judgeScriptName}
								onChange={(event) => setForm({ ...form, judgeScriptName: event.target.value })}
							/>
						</Field>
					</FormGrid>

					<Field
						label="评测脚本内容"
						htmlFor="gamebox-judge-content"
						hint="留空表示清空，回退到包内默认脚本。"
					>
						<Textarea
							id="gamebox-judge-content"
							rows={6}
							className="font-mono text-xs"
							value={form.judgeScriptContent}
							onChange={(event) => setForm({ ...form, judgeScriptContent: event.target.value })}
						/>
					</Field>

					<Field
						label="healthchecks_json"
						htmlFor="gamebox-healthchecks"
						error={errors.healthchecksJson}
						hint="JSON 文本；留空 = 清空该配置，内容未修改则不会提交。后端会按 JSON 解析，非法 JSON 会 400。"
					>
						<Textarea
							id="gamebox-healthchecks"
							rows={6}
							className="font-mono text-xs"
							placeholder='例如 [{"name":"port-80","type":"tcp","port":80}]'
							value={form.healthchecksJson}
							onChange={(event) => setForm({ ...form, healthchecksJson: event.target.value })}
						/>
					</Field>

					<Field
						label="judge_args_json"
						htmlFor="gamebox-judge-args"
						error={errors.judgeArgsJson}
						hint="JSON 文本；留空 = 清空该配置，内容未修改则不会提交。"
					>
						<Textarea
							id="gamebox-judge-args"
							rows={6}
							className="font-mono text-xs"
							value={form.judgeArgsJson}
							onChange={(event) => setForm({ ...form, judgeArgsJson: event.target.value })}
						/>
					</Field>
				</form>
			</FormSheet>
		</PageBody>
	);
}

/** GameBox 校验结果表（含 package 目录检查）。 */
function GameboxCheckResultTable({
	items,
	onBuild,
	building,
}: {
	items: GameBoxCheckResult[];
	onBuild: (id: string) => void;
	building: boolean;
}): ReactNode {
	const columns: DataTableColumn<GameBoxCheckResult>[] = [
		{
			id: "gamebox_name",
			header: "GameBox",
			cell: (row) => <span className="text-sm font-medium">{row.gamebox_name}</span>,
			sortValue: (row) => row.gamebox_name,
		},
		{
			id: "docker_image",
			header: "镜像",
			cell: (row) =>
				row.docker_image ? (
					<TonePill tone="success">就绪</TonePill>
				) : (
					<Button variant="outline" size="xs" onClick={() => onBuild(row.id)} disabled={building}>
						<Hammer /> 构建
					</Button>
				),
			sortValue: (row) => (row.docker_image ? 1 : 0),
		},
		{
			id: "package_dir",
			header: "package 目录",
			cell: (row) => (
				<TonePill tone={row.package_dir ? "success" : "danger"}>
					{row.package_dir ? "已镜像" : "缺失"}
				</TonePill>
			),
			sortValue: (row) => (row.package_dir ? 1 : 0),
			hideBelow: "sm",
		},
		{
			id: "is_ok",
			header: "结论",
			cell: (row) => (
				<TonePill tone={row.is_ok ? "success" : "danger"}>{row.is_ok ? "可用" : "不可用"}</TonePill>
			),
			sortValue: (row) => (row.is_ok ? 1 : 0),
		},
	];

	return (
		<DataTable
			data={items}
			getRowId={(row) => row.id}
			columns={columns}
			mobileCard={(row) => (
				<div className="space-y-1.5">
					<div className="flex items-center justify-between gap-2">
						<span className="text-sm font-medium">{row.gamebox_name}</span>
						<TonePill tone={row.is_ok ? "success" : "danger"}>
							{row.is_ok ? "可用" : "不可用"}
						</TonePill>
					</div>
					<p className="text-xs text-muted-foreground">
						镜像 {row.docker_image ? "就绪" : "缺失"} · package 目录{" "}
						{row.package_dir ? "已镜像" : "缺失"}
					</p>
				</div>
			)}
			empty={<span>后端未返回任何校验结果</span>}
		/>
	);
}
