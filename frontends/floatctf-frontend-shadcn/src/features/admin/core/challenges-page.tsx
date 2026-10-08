/**
 * 挑战管理（`/admin/challenges`）—— CRUD + 导入 / 校验 / 构建 / 扫描。
 *
 * 真实接口：`client.admin.challenges.*`（`fetch` / `create` / `patch` / `remove` /
 * `importChallenge(package_zip)` / `checkChallenges(challenge_id_list?)` /
 * `buildChallenges(challenge_id_list?)` / `scanChallenges()`）。
 *
 * 语义要点（读后端 `catalog/admin.rs` 与 `build/mod.rs` 核实）：
 * - 手工 create 只接受 name / category / description / hidden（版本内容只能经包导入进入）；
 * - `static_flag_value` 是**明文 flag**：列表里必须用 `<SecretValue>` 渲染；
 * - `container_port` 为 null = 没有 docker 运行时（static / attachment-only）；
 * - `build` 不是重新构建，而是「确保当前版本 pin 的镜像在本地存在」（必要时按 digest 拉取），
 *   只对 `build_status=ready` 的题目生效。
 */

import { useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
	FileUp,
	Hammer,
	Pencil,
	Plus,
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
	SecretValue,
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
import { formatBytes, formatDateTime } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type {
	BuildChallengeResult,
	ChallengeCheckResult,
	ChallengeScanItem,
	ChallengesListItem,
	ImportChallengeResponse,
	QueryParams,
} from "@floatctf/sdk";

import { BuildResultTable, ScanResultTable } from "./ops";
import {
	BuildStatusPill,
	ResultsCard,
	RowMenu,
	SearchInput,
	parseIntInput,
	useAdminListState,
} from "./shared";

const SEARCH_KEYS = ["name", "safe_name", "category"];

interface ChallengeFormState {
	name: string;
	category: string;
	description: string;
	hidden: boolean;
	staticFlagValue: string;
	containerPort: string;
	cpuMillis: string;
	memoryBytes: string;
	pidsLimit: string;
}

type ChallengeFormErrors = Partial<Record<keyof ChallengeFormState, string>>;

const EMPTY_FORM: ChallengeFormState = {
	name: "",
	category: "",
	description: "",
	hidden: true,
	staticFlagValue: "",
	containerPort: "",
	cpuMillis: "",
	memoryBytes: "",
	pidsLimit: "",
};

const PORT_RANGE = "容器端口应为 1–65535 的整数；留空表示无 docker 运行时。";

function validate(form: ChallengeFormState, mode: "create" | "edit"): ChallengeFormErrors {
	const errors: ChallengeFormErrors = {};
	if (!form.name.trim()) errors.name = "题目名称不能为空";
	if (!form.category.trim()) errors.category = "分类不能为空";
	if (mode === "edit") {
		const port = parseIntInput(form.containerPort);
		if (port.kind === "invalid") errors.containerPort = PORT_RANGE;
		else if (port.kind === "ok" && (port.value < 1 || port.value > 65535)) {
			errors.containerPort = PORT_RANGE;
		}
		for (const [key, label] of [
			["cpuMillis", "推荐 CPU"],
			["memoryBytes", "推荐内存"],
			["pidsLimit", "推荐 PID 上限"],
		] as const) {
			const parsed = parseIntInput(form[key]);
			if (parsed.kind === "invalid") errors[key] = `${label}应为整数；留空表示不修改。`;
			else if (parsed.kind === "ok" && parsed.value <= 0) errors[key] = `${label}必须大于 0。`;
		}
	}
	return errors;
}

export function AdminChallengesPage(): ReactNode {
	useDocumentTitle("题库管理 · FloatCTF");
	const client = useClient();
	const queryClient = useAppQueryClient();
	const confirm = useConfirm();
	const list = useAdminListState(SEARCH_KEYS);

	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [editing, setEditing] = useState<{ mode: "create" | "edit"; row?: ChallengesListItem } | null>(
		null,
	);
	const [form, setForm] = useState<ChallengeFormState>(EMPTY_FORM);
	const [errors, setErrors] = useState<ChallengeFormErrors>({});

	const fileInputRef = useRef<HTMLInputElement>(null);
	const [file, setFile] = useState<File | null>(null);
	const [progress, setProgress] = useState(0);
	const [imported, setImported] = useState<ChallengesListItem | null>(null);

	const [scanItems, setScanItems] = useState<ChallengeScanItem[] | null>(null);
	const [checkItems, setCheckItems] = useState<ChallengeCheckResult[] | null>(null);
	const [buildItems, setBuildItems] = useState<BuildChallengeResult[] | null>(null);

	const params: QueryParams = {
		page: list.page,
		limit: list.pageSize,
		...(list.filter ? { filter: list.filter } : {}),
	};

	const query = useQuery({
		queryKey: qk.admin.challenges(params),
		queryFn: () => callList<ChallengesListItem>(client.admin.challenges.fetch(params)),
	});

	function invalidate(): void {
		void queryClient.invalidateQueries({ queryKey: qk.admin.challenges() });
	}

	const save = useMutation({
		mutationFn: (input: { mode: "create" | "edit"; id?: string; body: ChallengeFormState }) => {
			const { body } = input;
			if (input.mode === "create") {
				// 手工创建只有身份字段；版本 / 镜像 / flag 由包导入写入。
				return call<ChallengesListItem>(
					client.admin.challenges.create({
						name: body.name.trim(),
						category: body.category.trim(),
						description: body.description,
						hidden: body.hidden,
					}),
					"创建题目",
				);
			}
			const port = parseIntInput(body.containerPort);
			const cpu = parseIntInput(body.cpuMillis);
			const memory = parseIntInput(body.memoryBytes);
			const pids = parseIntInput(body.pidsLimit);
			// patch(Partial<ChallengesListItem>) 的 URL 用 `.id` 拼接，**必须**带 id。
			return call<ChallengesListItem>(
				client.admin.challenges.patch({
					id: input.id ?? "",
					name: body.name.trim(),
					category: body.category.trim(),
					description: body.description,
					hidden: body.hidden,
					// 空串 = 清空明文 flag（后端 `filter(|s| !s.trim().is_empty())`）。
					static_flag_value: body.staticFlagValue,
					// 显式 null = 清空容器端口（变为无 docker 运行时）。
					container_port: port.kind === "ok" ? port.value : null,
					...(cpu.kind === "ok" ? { recommended_cpu_millis: cpu.value } : {}),
					...(memory.kind === "ok" ? { recommended_memory_bytes: memory.value } : {}),
					...(pids.kind === "ok" ? { recommended_pids_limit: pids.value } : {}),
				}),
				"更新题目",
			);
		},
		onSuccess: (_data, input) => {
			toast.success(input.mode === "create" ? "题目已创建" : "题目已更新");
			invalidate();
			setEditing(null);
			setErrors({});
		},
		onError: (error) => toast.apiError("保存题目失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) => call<number>(client.admin.challenges.remove(ids), "删除题目"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 道题目`, "题目包目录与镜像不会被自动清理，如需回收请手动处理。");
			invalidate();
			setSelectedIds([]);
		},
		onError: (error) => toast.apiError("删除题目失败", error),
	});

	const importMutation = useMutation({
		mutationFn: (selected: File) =>
			uploadFile<ImportChallengeResponse>({
				client,
				scope: "admin",
				url: "/challenges/import",
				field: "package_zip",
				file: selected,
				onProgress: setProgress,
			}),
		onSuccess: (data) => {
			setImported(data.challenge);
			setFile(null);
			setProgress(0);
			toast.success("题目包导入完成", `题目：${data.challenge.name}`);
			invalidate();
		},
		onError: (error) => {
			setProgress(0);
			toast.apiError("导入题目包失败", error);
		},
	});

	const scan = useMutation({
		mutationFn: () => call<ChallengeScanItem[]>(client.admin.challenges.scanChallenges(), "扫描题目目录"),
		onSuccess: (items) => {
			setScanItems(items);
			const failed = items.filter((item) => item.status === "error").length;
			if (failed > 0) toast.warning(`扫描完成：${items.length} 条，其中 ${failed} 条失败`);
			else toast.success(`扫描完成：${items.length} 条结果`);
			invalidate();
		},
		onError: (error) => toast.apiError("扫描题目目录失败", error),
	});

	const check = useMutation({
		mutationFn: (ids?: string[]) =>
			call<ChallengeCheckResult[]>(client.admin.challenges.checkChallenges(ids), "校验题目"),
		onSuccess: (items) => {
			setCheckItems(items);
			const failed = items.filter((item) => !item.is_ok).length;
			if (failed > 0) toast.warning(`校验完成：${items.length} 题，${failed} 题不可用`);
			else toast.success(`校验完成：${items.length} 题全部可用`);
			invalidate();
		},
		onError: (error) => toast.apiError("校验题目失败", error),
	});

	const build = useMutation({
		mutationFn: (ids?: string[]) =>
			call<BuildChallengeResult[]>(client.admin.challenges.buildChallenges(ids), "构建题目"),
		onSuccess: (items) => {
			setBuildItems(items);
			const failed = items.filter((item) => !item.is_ok);
			if (items.length === 0) {
				toast.warning(
					"没有可构建的镜像",
					"构建只对 build_status=ready（当前版本镜像已 pin）的题目生效；static / 仅附件题目没有镜像。",
				);
			} else if (failed.length > 0) {
				toast.error(`${failed.length} 题构建失败`, failed[0]?.message);
			} else {
				toast.success(`${items.length} 题镜像已就绪`);
			}
			invalidate();
		},
		onError: (error) => toast.apiError("构建题目失败", error),
	});

	function openCreate(): void {
		setForm(EMPTY_FORM);
		setErrors({});
		setEditing({ mode: "create" });
	}

	function openEdit(row: ChallengesListItem): void {
		setForm({
			name: row.name,
			category: row.category,
			description: row.description,
			hidden: row.hidden,
			staticFlagValue: row.static_flag_value ?? "",
			containerPort: row.container_port === null || row.container_port === undefined
				? ""
				: String(row.container_port),
			cpuMillis: row.recommended_cpu_millis ? String(row.recommended_cpu_millis) : "",
			memoryBytes: row.recommended_memory_bytes ? String(row.recommended_memory_bytes) : "",
			pidsLimit: row.recommended_pids_limit ? String(row.recommended_pids_limit) : "",
		});
		setErrors({});
		setEditing({ mode: "edit", row });
	}

	function submit(): void {
		if (!editing) return;
		const nextErrors = validate(form, editing.mode);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) {
			toast.warning("请先修正表单中的错误");
			return;
		}
		save.mutate({ mode: editing.mode, id: editing.row?.id, body: form });
	}

	async function confirmRemoveOne(row: ChallengesListItem): Promise<void> {
		const ok = await confirm({
			title: `删除题目「${row.name}」？`,
			description: "该操作不可恢复。",
			consequences: [
				"题目行被删除，该题在所有赛事中的挂题、解题记录与题解将级联失效",
				"已启动的实例与镜像不会自动回收，需要人工清理",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: row.name,
		});
		if (ok) remove.mutate([row.id]);
	}

	async function confirmRemoveSelected(): Promise<void> {
		if (selectedIds.length === 0) {
			toast.warning("请先选择要删除的题目");
			return;
		}
		const ok = await confirm({
			title: `删除选中的 ${selectedIds.length} 道题目？`,
			description: "该操作不可恢复。",
			consequences: [
				`${selectedIds.length} 道题目及其解题记录会被级联删除`,
				"已构建的镜像与磁盘上的题目包目录不会被回收",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: "delete",
		});
		if (ok) remove.mutate(selectedIds);
	}

	async function confirmBuild(ids: string[]): Promise<void> {
		const ok = await confirm({
			title: ids.length === 1 ? "构建该题目的镜像？" : `构建选中的 ${ids.length} 道题目？`,
			description:
				"构建 = 确保当前版本 pin 的镜像在本地 Docker 中存在（缺失时按 digest 拉取），不是重新编译。",
			consequences: [
				"会占用宿主 Docker / 网络资源，镜像较大时可能耗时数分钟",
				"只对 build_status=ready 的题目生效；static / 仅附件题目没有镜像，会被跳过",
				"失败原因（如 registry 不可达）会逐条列出",
			],
			tone: "default",
			confirmText: "开始构建",
		});
		if (ok) build.mutate(ids.length > 0 ? ids : undefined);
	}

	async function confirmScan(): Promise<void> {
		const ok = await confirm({
			title: "扫描题目目录并登记未入库的题目包？",
			description: "扫描后端配置的 CHALLENGES_DIR，把磁盘上存在但数据库里没有的题目包登记进来。",
			consequences: [
				"会向题库写入新题目行（已存在的包只会被 skipped，不会覆盖版本）",
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
			title: `导入题目包「${file.name}」？`,
			description: "导入 = 解压 package + 按 manifest 同步构建镜像并 pin digest。",
			consequences: [
				"会占用宿主 Docker / 网络资源，构建可能耗时数分钟",
				"同名题目按严格递增版本号 upsert 身份行，可能出现 safe_name 冲突而失败",
				"失败会写入 build_status=failed 与 build_error，可用「校验」查看",
			],
			tone: "default",
			confirmText: "开始导入",
		});
		if (ok) importMutation.mutate(file);
	}

	const columns: DataTableColumn<ChallengesListItem>[] = [
		{
			id: "name",
			header: "题目",
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
					<span className="text-xs text-muted-foreground">无镜像（static / 仅附件）</span>
				),
			hideBelow: "lg",
		},
		{
			id: "runtime",
			header: "运行时 / 资源",
			cell: (row) => (
				<div className="space-y-0.5">
					<div className="text-xs">
						{row.container_port === null || row.container_port === undefined ? (
							<span className="text-muted-foreground">无容器端口</span>
						) : (
							<MonoText>port {row.container_port}</MonoText>
						)}
					</div>
					<div className="tnum font-mono text-xs text-muted-foreground">
						{row.recommended_cpu_millis ? `${row.recommended_cpu_millis}m` : "—"} /{" "}
						{row.recommended_memory_bytes ? formatBytes(row.recommended_memory_bytes) : "—"} /{" "}
						{row.recommended_pids_limit ?? "—"} pids
					</div>
					{row.attachment ? (
						<div className="max-w-48 truncate text-xs text-muted-foreground" title={row.attachment.path}>
							附件 {row.attachment.name}
							{row.attachment.size ? ` · ${formatBytes(row.attachment.size)}` : ""}
						</div>
					) : null}
				</div>
			),
			hideBelow: "xl",
		},
		{
			id: "flag",
			header: "静态 Flag",
			cell: (row) =>
				row.static_flag_value ? (
					<SecretValue value={row.static_flag_value} label="静态 flag" />
				) : (
					<span className="text-xs text-muted-foreground">
						{row.flag_type ? row.flag_type : "—"}
					</span>
				),
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
			hideBelow: "sm",
		},
		{
			id: "updated_at",
			header: "最近更新",
			cell: (row) => (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.updated_at)}</span>
			),
			sortValue: (row) => Date.parse(row.updated_at),
			hideBelow: "lg",
		},
	];

	const canBuild = (row: ChallengesListItem): boolean =>
		row.build_status === "ready" && row.container_port !== null && row.container_port !== undefined;

	return (
		<PageBody>
			<PageHeader
				title="题库管理"
				description="题目的身份字段可手工维护；版本内容、镜像与 flag 通过导入题目包进入。"
				actions={
					<Toolbar>
						<Button variant="outline" onClick={() => query.refetch()} disabled={query.isFetching}>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button onClick={openCreate}>
							<Plus /> 新建题目
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="题目列表"
				description="搜索在题目名称 / safe_name / 分类上做服务端 OR 匹配。"
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
							<ShieldCheck /> {check.isPending ? "校验中…" : selectedIds.length > 0 ? "校验选中" : "校验全部"}
						</Button>
						<Button
							variant="outline"
							size="sm"
							onClick={() => fileInputRef.current?.click()}
							disabled={importMutation.isPending}
						>
							<FileUp /> 选择题目包
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
							toast.error("只支持 ZIP 题目包", "package zip 应为 meta.toml + src/** + attachment/**。");
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
						{imported.build_error ? (
							<span className="text-xs text-destructive">{imported.build_error}</span>
						) : null}
					</div>
				) : null}

				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					errorTitle="加载题库失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						list.filter ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的题目"
								description={`没有名称 / safe_name / 分类匹配「${list.search}」的题目。`}
							/>
						) : (
							<EmptyBlock
								title="题库还是空的"
								description="导入题目包（建议）或手工创建题目身份。"
								action={
									<Button size="sm" onClick={() => fileInputRef.current?.click()}>
										<FileUp /> 选择题目包
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
								<RowMenu label={`题目 ${row.name} 的操作`}>
									<DropdownMenuItem onSelect={() => openEdit(row)}>
										<Pencil /> 编辑
									</DropdownMenuItem>
									<DropdownMenuItem onSelect={() => check.mutate([row.id])}>
										<ShieldCheck /> 校验该题
									</DropdownMenuItem>
									{canBuild(row) ? (
										<DropdownMenuItem onSelect={() => void confirmBuild([row.id])}>
											<Hammer /> 构建镜像
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
					title="题目目录扫描结果"
					description={`后端返回 ${scanItems.length} 条（added / skipped / error 原样展示）。`}
					onDismiss={() => setScanItems(null)}
				>
					<ScanResultTable items={scanItems} />
				</ResultsCard>
			) : null}

			{checkItems ? (
				<ResultsCard
					title="校验结果"
					description={`${checkItems.length} 题，${checkItems.filter((item) => !item.is_ok).length} 题不可用。`}
					onDismiss={() => setCheckItems(null)}
				>
					<CheckResultTable items={checkItems} onBuild={(id) => void confirmBuild([id])} building={build.isPending} />
				</ResultsCard>
			) : null}

			{buildItems ? (
				<ResultsCard
					title="构建结果"
					description="逐题结果与后端消息（失败原因原样展示）。"
					onDismiss={() => setBuildItems(null)}
				>
					<BuildResultTable items={buildItems} nameOf={(row) => row.challenge_name} label="题目" />
				</ResultsCard>
			) : null}

			<FormSheet
				open={editing !== null}
				onOpenChange={(open) => {
					if (!open) setEditing(null);
				}}
				title={editing?.mode === "edit" ? "编辑题目" : "新建题目"}
				description={
					editing?.mode === "edit"
						? "版本、镜像与附件由导入决定，这里只能维护身份字段、静态 flag 与运行时参数。"
						: "手工创建只写入身份字段；要获得可运行的镜像请使用「选择题目包」导入。"
				}
				width="lg"
				footer={
					<FormFooter
						onCancel={() => setEditing(null)}
						submitLabel={editing?.mode === "edit" ? "保存修改" : "创建题目"}
						isPending={save.isPending}
						formId="admin-challenge-form"
					/>
				}
			>
				<form
					id="admin-challenge-form"
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<FormGrid columns={2}>
						<Field label="题目名称" htmlFor="challenge-name" required error={errors.name}>
							<Input
								id="challenge-name"
								value={form.name}
								onChange={(event) => setForm({ ...form, name: event.target.value })}
							/>
						</Field>
						<Field
							label="分类"
							htmlFor="challenge-category"
							required
							error={errors.category}
							hint="如 web / pwn / misc。"
						>
							<Input
								id="challenge-category"
								value={form.category}
								onChange={(event) => setForm({ ...form, category: event.target.value })}
							/>
						</Field>
					</FormGrid>
					<Field label="描述" htmlFor="challenge-description" hint="Markdown 文本。">
						<Textarea
							id="challenge-description"
							rows={4}
							value={form.description}
							onChange={(event) => setForm({ ...form, description: event.target.value })}
						/>
					</Field>
					<div className="flex items-center justify-between rounded-lg border px-3 py-2">
						<div>
							<p className="text-sm font-medium">隐藏题目</p>
							<p className="text-xs text-muted-foreground">隐藏后选手端不可见，管理员始终可见。</p>
						</div>
						<Switch
							checked={form.hidden}
							onCheckedChange={(checked) => setForm({ ...form, hidden: checked })}
							aria-label="隐藏题目"
						/>
					</div>

					{editing?.mode === "edit" ? (
						<>
							<Field
								label="静态 Flag（明文）"
								htmlFor="challenge-static-flag"
								error={errors.staticFlagValue}
								hint="仅 flag_type=static 时生效；留空表示清空。此处填写的内容会以明文提交到后端。"
							>
								<Input
									id="challenge-static-flag"
									autoComplete="off"
									className="font-mono text-xs"
									value={form.staticFlagValue}
									onChange={(event) => setForm({ ...form, staticFlagValue: event.target.value })}
								/>
							</Field>
							<FormGrid columns={2}>
								<Field
									label="容器端口"
									htmlFor="challenge-port"
									error={errors.containerPort}
									hint={PORT_RANGE}
								>
									<Input
										id="challenge-port"
										inputMode="numeric"
										value={form.containerPort}
										onChange={(event) => setForm({ ...form, containerPort: event.target.value })}
									/>
								</Field>
								<Field
									label="推荐 CPU（毫核）"
									htmlFor="challenge-cpu"
									error={errors.cpuMillis}
									hint="留空表示不修改。"
								>
									<Input
										id="challenge-cpu"
										inputMode="numeric"
										value={form.cpuMillis}
										onChange={(event) => setForm({ ...form, cpuMillis: event.target.value })}
									/>
								</Field>
								<Field
									label="推荐内存（字节）"
									htmlFor="challenge-memory"
									error={errors.memoryBytes}
									hint="留空表示不修改。"
								>
									<Input
										id="challenge-memory"
										inputMode="numeric"
										value={form.memoryBytes}
										onChange={(event) => setForm({ ...form, memoryBytes: event.target.value })}
									/>
								</Field>
								<Field
									label="推荐 PID 上限"
									htmlFor="challenge-pids"
									error={errors.pidsLimit}
									hint="留空表示不修改。"
								>
									<Input
										id="challenge-pids"
										inputMode="numeric"
										value={form.pidsLimit}
										onChange={(event) => setForm({ ...form, pidsLimit: event.target.value })}
									/>
								</Field>
							</FormGrid>
						</>
					) : null}
				</form>
			</FormSheet>
		</PageBody>
	);
}

/** 校验结果表（challenges 专属：多一列 static / attachment-only 语义）。 */
function CheckResultTable({
	items,
	onBuild,
	building,
}: {
	items: ChallengeCheckResult[];
	onBuild: (id: string) => void;
	building: boolean;
}): ReactNode {
	const columns: DataTableColumn<ChallengeCheckResult>[] = [
		{
			id: "challenge_name",
			header: "题目",
			cell: (row) => <span className="text-sm font-medium">{row.challenge_name}</span>,
			sortValue: (row) => row.challenge_name,
		},
		{
			id: "docker_image",
			header: "镜像",
			cell: (row) => {
				if (row.static_content) {
					return <TonePill tone="muted">static / 仅附件</TonePill>;
				}
				if (row.docker_image) return <TonePill tone="success">就绪</TonePill>;
				return (
					<Button variant="outline" size="xs" onClick={() => onBuild(row.id)} disabled={building}>
						<Hammer /> 构建
					</Button>
				);
			},
			sortValue: (row) => (row.docker_image ? 1 : 0),
		},
		{
			id: "attachment",
			header: "附件",
			cell: (row) => (
				<TonePill tone={row.attachment ? "success" : "muted"}>
					{row.attachment ? "存在" : "无"}
				</TonePill>
			),
			sortValue: (row) => (row.attachment ? 1 : 0),
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
						<span className="text-sm font-medium">{row.challenge_name}</span>
						<TonePill tone={row.is_ok ? "success" : "danger"}>
							{row.is_ok ? "可用" : "不可用"}
						</TonePill>
					</div>
					<p className="text-xs text-muted-foreground">
						镜像 {row.docker_image ? "就绪" : "缺失"} · 附件 {row.attachment ? "存在" : "无"}
						{row.static_content ? " · static / 仅附件" : ""}
					</p>
				</div>
			)}
			empty={<span>后端未返回任何校验结果</span>}
		/>
	);
}
