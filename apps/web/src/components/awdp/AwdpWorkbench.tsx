/**
 * AWDP 共享工作台（主计划 §65-68）。
 *
 * Practice Run 与 Competition Event 共用的 Break/Fix/Ended 三态渲染：
 *  - 纯展示 + 交互编排：数据由调用方组装为统一 view-model；
 *  - 操作一律经回调上抛（onSubmitBreak / onUploadPatch / onTestCheck /
 *    onDownloadSource / onTrainAgain / onStartInstance / onStopInstance /
 *    onResetInstance），调用方负责真实 API 调用与 query 失效；
 *  - 交互提示用内部 useMsgBanner（Primer Banner，非原生弹窗）；
 *  - 绝不展示 exploit 内容/路径、source object key、credentials。
 *
 * 从既有赛事页抽取的块：
 *  - PHASE_META / EVAL_STATUS_LABEL（index.tsx / rounds.tsx）
 *  - countdown 逻辑（index.tsx）
 *  - GameBoxCard 整卡（gameboxes.tsx）：endpoints / 实例操作 / flag 提交 /
 *    patch 上传 / test-check / source 下载
 *  - Official History 表（rounds × evaluations 拼装，rounds.tsx 样式）
 */
import { Button, Label, TextInput } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import dayjs from "dayjs";
import { Fragment, useEffect, useRef, useState } from "react";

import type {
	AwdpPhase,
	BreakSubmitResponse,
	ManualCheckDto,
	PatchSubmitResponse,
} from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { AwdpPhaseOverview } from "@/components/awdp/AwdpPhaseOverview";

// ────────────────────────────────────────────────────────────────────────────
// 常量与工具
// ────────────────────────────────────────────────────────────────────────────

export const EVAL_STATUS_LABEL: Record<string, string> = {
	pending: "pending",
	running: "running",
	no_patch: "no patch (+0)",
	service_down: "service down (+0)",
	functional_broken: "broken (+0)",
	vulnerable: "vulnerable (+0)",
	patched: "patched (+score)",
	platform_error: "platform error",
};

function fmtTime(iso?: string | null) {
	return iso ? dayjs.utc(iso).local().format("MM-DD HH:mm:ss") : "-";
}

function useNow() {
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);
	return now;
}

/**
 * rounds × official evaluations 拼装 Official History（§67）。
 * 已 completed 且无评估行 → no_patch (+0)；有评估行按 status 映射，
 * 仅 patched 获得 +fixRoundScore；pending/running 未结算 delta=null。
 */
export function buildAwdpHistory(
	rounds: {
		sequence: number;
		starts_at: string;
		cutoff_at: string;
		status: string;
	}[],
	evals: {
		round_sequence: number | null;
		kind: string;
		status: string;
		finished_at: string | null;
	}[],
	fixRoundScore: number,
): AwdpHistoryRow[] {
	const official = new Map<
		number,
		{ status: string; finished_at: string | null }
	>();
	for (const e of evals) {
		if (
			e.kind === "official" &&
			e.round_sequence != null &&
			!official.has(e.round_sequence)
		) {
			official.set(e.round_sequence, {
				status: e.status,
				finished_at: e.finished_at,
			});
		}
	}
	return rounds.map((r) => {
		const ev = official.get(r.sequence);
		const status = ev
			? ev.status
			: r.status === "completed"
				? "no_patch"
				: r.status === "evaluating"
					? "running"
					: "pending";
		const settled = ev
			? !["pending", "running"].includes(status)
			: r.status === "completed";
		return {
			id: `r${r.sequence}`,
			sequence: r.sequence,
			starts_at: r.starts_at,
			cutoff_at: r.cutoff_at,
			status,
			label: EVAL_STATUS_LABEL[status] ?? status,
			delta: settled ? (status === "patched" ? fixRoundScore : 0) : null,
			finished_at: ev?.finished_at ?? null,
		};
	});
}

// ────────────────────────────────────────────────────────────────────────────
// View-model
// ────────────────────────────────────────────────────────────────────────────

export type AwdpWorkbenchEndpoint = {
	protocol: "http" | "tcp";
	container_port: number;
	public_host: string;
	public_port: number;
};

export type AwdpWorkbenchGameBox = {
	/** 交互 key：competition = eg_id；practice = run 内 gamebox_id。 */
	id: string;
	gamebox_id: string;
	name: string;
	category: string;
	/** 未启动时的端点 fallback（competition overview 提供；practice 无）。 */
	exposed?: [string, number][];
	broken: boolean;
	enabled: boolean;
	/** Fix 阶段才非空。 */
	source_code_dir?: string | null;
	instance: {
		instance_id: string;
		runtime_state: string;
		runtime_generation: number;
		endpoints: AwdpWorkbenchEndpoint[];
	} | null;
};

export type AwdpHistoryRow = {
	id: string;
	sequence: number;
	starts_at: string;
	cutoff_at: string;
	status: string;
	label: string;
	/** null = 尚未结算（pending/running）。 */
	delta: number | null;
	finished_at: string | null;
};

export type AwdpScoreEventView = {
	id: string;
	score_type: "break" | "fix";
	delta: number;
	fix_round_id: string | null;
	created_at: string | null;
};

export type AwdpWorkbenchViewModel = {
	/** 空字符串则不渲染标题行（competition 由赛事路由头展示）。 */
	title?: string;
	/** 标题下方展示的描述（与挑战详情页同款 border-top 分隔）。 */
	description?: string;
	phase: AwdpPhase;
	/** Break 开始时间（新顶部面板 timeline 用）。 */
	startedAt?: string | null;
	/** Fix 开始时间。 */
	fixStartedAt?: string | null;
	/** Break 时长秒（timeline 宽度按真实比例）。 */
	breakDurationSecs: number;
	/** Fix 时长秒。 */
	fixDurationSecs: number;
	/** break → break_ends_at；fix → next_action_at（下一 cutoff）。 */
	phaseEndsAt: string | null;
	breakEndsAt?: string | null;
	fixEndsAt?: string | null;
	currentRound: number;
	totalRounds: number;
	nextCheckAt: string | null;
	score: number;
	breakScore: number;
	fixRoundScore: number;
	gameboxes: AwdpWorkbenchGameBox[];
	history: AwdpHistoryRow[];
	/** ended 阶段可选：计分明细（/scores）。 */
	scoreHistory?: AwdpScoreEventView[];
	/** 控制 Train Again 等 practice 专属交互。 */
	isPractice: boolean;
	/** 练习模式手动控制阶段（break↔fix）。 */
	canControlPhase?: boolean;
	/** 练习 data plane Flag Server endpoint（仅 GameBox 内部网络可达；Fix 阶段弱化展示）。 */
	judgeEndpoint?: {
		baseUrl: string;
		flagUrl: string;
	} | null;
};

export type AwdpWorkbenchProps = {
	viewModel: AwdpWorkbenchViewModel;
	onSubmitBreak: (
		egId: string,
		flag: string,
	) =>
		| Promise<BreakSubmitResponse | undefined>
		| BreakSubmitResponse
		| undefined;
	onUploadPatch: (
		egId: string,
		file: File,
	) =>
		| Promise<PatchSubmitResponse | undefined>
		| PatchSubmitResponse
		| undefined;
	onTestCheck: (
		egId: string,
	) => Promise<ManualCheckDto | undefined> | ManualCheckDto | undefined;
	/** 练习「End」：停止全部实例并恢复如初（仅在 practice 非 ended 时显示按钮）。 */
	onEnd?: () => void | Promise<void>;
	/** 练习模式手动控制阶段（break↔fix）。 */
	onSetPhase?: (target: "break" | "fix") => void | Promise<void>;
	/** Fix 阶段下载源码：返回 presigned URL（空则不打开）。 */
	onDownloadSource?: (
		egId: string,
	) => Promise<string | undefined> | string | undefined;
	/** 仅 practice ended：创建新 run（调用方负责跳转）。 */
	onTrainAgain?: () => Promise<void> | undefined;
	onStartInstance?: (egId: string) => void | Promise<void>;
	onStopInstance?: (egId: string) => void | Promise<void>;
	onResetInstance?: (egId: string) => void | Promise<void>;
	/** 赛事页可注入队伍区/写 up 等 context 专属区。 */
	children?: React.ReactNode;
};

// ────────────────────────────────────────────────────────────────────────────
// Component
// ────────────────────────────────────────────────────────────────────────────

export function AwdpWorkbench({
	viewModel,
	onSubmitBreak,
	onUploadPatch,
	onTestCheck,
	onDownloadSource,
	onSetPhase,
	onEnd,
	onTrainAgain,
	onStartInstance,
	onStopInstance,
	onResetInstance,
	children,
}: AwdpWorkbenchProps) {
	const banner = useMsgBanner({});
	const now = useNow();
	const { phase } = viewModel;
	const active = phase === "break" || phase === "fix";

	// 交互 state（按 gamebox id 索引）
	const [flagInputs, setFlagInputs] = useState<Record<string, string>>({});
	const [patchFiles, setPatchFiles] = useState<Record<string, File | null>>({});
	const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({});
	const [lastPatch, setLastPatch] = useState<
		Record<string, "applying" | "applied" | "failed">
	>({});
	const [checkResults, setCheckResults] = useState<
		Record<string, ManualCheckDto | undefined>
	>({});
	const [checking, setChecking] = useState<Record<string, boolean>>({});
	const [busy, setBusy] = useState<Record<string, boolean>>({});
	const setBusyKey = (key: string, value: boolean) =>
		setBusy((prev) => ({ ...prev, [key]: value }));
	// 阶段切换（SegmentedControl break↔fix）进行中：页面级阻塞，所有按钮禁用。
	const phaseBusy = !!(busy["phase:break"] || busy["phase:fix"]);

	// ── handlers ────────────────────────────────────────────────────────────

	const handleSubmitBreak = async (gb: AwdpWorkbenchGameBox) => {
		const flag = (flagInputs[gb.id] ?? "").trim();
		if (!flag) {
			return;
		}
		const key = `break:${gb.id}`;
		setBusyKey(key, true);
		try {
			const res = await onSubmitBreak(gb.id, flag);
			if (res?.accepted) {
				banner.showBanner(
					res.scored ? "success" : "warning",
					res.scored
						? "Flag accepted, +score"
						: "Flag accepted (already broken)",
				);
				setFlagInputs((prev) => ({ ...prev, [gb.id]: "" }));
			} else {
				banner.showBanner("critical", "Flag rejected");
			}
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(key, false);
		}
	};

	const handleUploadPatch = async (gb: AwdpWorkbenchGameBox) => {
		const file = patchFiles[gb.id];
		if (!file) {
			return;
		}
		const key = `patch:${gb.id}`;
		setBusyKey(key, true);
		setLastPatch((prev) => ({ ...prev, [gb.id]: "applying" }));
		try {
			const res = await onUploadPatch(gb.id, file);
			const status = res?.status === "applied" ? "applied" : "failed";
			setLastPatch((prev) => ({ ...prev, [gb.id]: status }));
			banner.showBanner(
				status === "applied" ? "success" : "critical",
				status === "applied" ? "Patch applied" : "Patch failed",
			);
			setPatchFiles((prev) => ({ ...prev, [gb.id]: null }));
			if (fileInputRefs.current[gb.id]) {
				fileInputRefs.current[gb.id]!.value = "";
			}
		} catch (e) {
			setLastPatch((prev) => ({ ...prev, [gb.id]: "failed" }));
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(key, false);
		}
	};

	const handleTestCheck = async (gb: AwdpWorkbenchGameBox) => {
		const key = `check:${gb.id}`;
		setBusyKey(key, true);
		setChecking((prev) => ({ ...prev, [gb.id]: true }));
		setCheckResults((prev) => ({ ...prev, [gb.id]: undefined }));
		try {
			const res = await onTestCheck(gb.id);
			if (res) {
				setCheckResults((prev) => ({ ...prev, [gb.id]: res }));
			}
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(key, false);
			setChecking((prev) => ({ ...prev, [gb.id]: false }));
		}
	};

	const handleDownloadSource = async (gb: AwdpWorkbenchGameBox) => {
		if (!onDownloadSource) {
			return;
		}
		const key = `source:${gb.id}`;
		setBusyKey(key, true);
		try {
			const url = await onDownloadSource(gb.id);
			if (url) {
				window.open(url, "_blank", "noopener,noreferrer");
			}
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(key, false);
		}
	};

	const handleInstanceOp = async (
		op: "start" | "stop" | "reset",
		gb: AwdpWorkbenchGameBox,
	) => {
		const fn =
			op === "start"
				? onStartInstance
				: op === "stop"
					? onStopInstance
					: onResetInstance;
		if (!fn) {
			return;
		}
		const key = `${op}:${gb.id}`;
		setBusyKey(key, true);
		try {
			await fn(gb.id);
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(key, false);
		}
	};

	const handleSetPhase = async (target: "break" | "fix") => {
		if (!onSetPhase) {
			return;
		}
		setBusyKey(`phase:${target}`, true);
		try {
			await onSetPhase(target);
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey(`phase:${target}`, false);
		}
	};

	/** 练习「End」：停止全部实例并恢复如初（调用方负责失效/切换 UI 状态）。 */
	const handleEnd = async () => {
		if (!onEnd) {
			return;
		}
		setBusyKey("end", true);
		try {
			await onEnd();
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey("end", false);
		}
	};



	const handleTrainAgain = async () => {
		if (!onTrainAgain) {
			return;
		}
		setBusyKey("train-again", true);
		try {
			await onTrainAgain();
		} catch (e) {
			banner.showErrorBanner(e);
		} finally {
			setBusyKey("train-again", false);
		}
	};

	// ── GameBox 卡片（抽取自 gameboxes.tsx GameBoxCard）──────────────────────

	const renderGameBox = (gb: AwdpWorkbenchGameBox) => {
		const inst = gb.instance;
		const running = inst?.runtime_state === "running";
		// pristine 重建（Reset）期间：本卡片所有按钮一并禁用，避免与容器重建竞态；
		// 阶段切换（页面级 phaseBusy）同样禁用本卡片全部按钮。
		const resetting = !!busy[`reset:${gb.id}`];
		const cardBlocked = resetting || phaseBusy;
		const check = checkResults[gb.id];
		const patchStatus = lastPatch[gb.id];

		return (
			<section key={gb.id} className="p-3 rounded border">
				<div className="flex items-center gap-2 mb-2">
					<h4 className="font-bold flex-1">{gb.name}</h4>
					<Label variant={gb.broken ? "danger" : "success"}>
						{gb.broken ? "Broken" : "Unbroken"}
					</Label>
					{!gb.enabled && <Label variant="secondary">Disabled</Label>}
					{inst && (
						<Label variant={running ? "success" : "secondary"}>
							{inst.runtime_state}
							{inst.runtime_generation > 1
								? ` (gen ${inst.runtime_generation})`
								: ""}
						</Label>
					)}
				</div>

				<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm mb-2">
					<dt className="font-bold">Category</dt>
					<dd className="font-medium">{gb.category}</dd>
					<dt className="font-bold">Endpoints</dt>
					<dd className="font-medium font-mono text-xs break-all">
						{inst?.endpoints && inst.endpoints.length > 0
							? inst.endpoints.map((e) => {
									const url = `${e.protocol}://${e.public_host}:${e.public_port}`;
									return (
										<span key={url}>
											{gb.category.toLowerCase() === "web" ? (
												<a
													href={url}
													target="_blank"
													rel="noreferrer"
													className="text-blue-600 underline"
												>
													{url}
												</a>
											) : (
												url
											)}
											{"  "}
										</span>
									);
								})
							: (gb.exposed ?? [])
									.map(([proto, port]) => `${proto}:${port} (未启动)`)
									.join("  ") || "-"}
					</dd>
					{gb.source_code_dir && (
						<>
							<dt className="font-bold">Source Path</dt>
							<dd className="font-medium font-mono text-xs">
								{gb.source_code_dir}
							</dd>
						</>
					)}
				</dl>

				{/* 实例操作（break|fix）：练习模式由页面级「开始 / End」统一管理生命周期，
				   不再显示单实例 Start/Stop（与 开始/End 重复）；Reset 保留。竞赛模式保持原样。 */}
				{active && (
					<div className="flex items-center gap-2 mb-2">
						{!viewModel.isPractice && (
							<>
								<Button
									variant="primary"
									disabled={
									cardBlocked || running || busy[`start:${gb.id}`]
								}
									onClick={() => handleInstanceOp("start", gb)}
								>
									Start
								</Button>
								<Button
									disabled={
									cardBlocked || !running || busy[`stop:${gb.id}`]
								}
									onClick={() => handleInstanceOp("stop", gb)}
								>
									Stop
								</Button>
							</>
						)}
						<Button
							variant="danger"
							disabled={cardBlocked || !inst || busy[`reset:${gb.id}`]}
							onClick={() => handleInstanceOp("reset", gb)}
						>
							{resetting ? "Resetting…" : "Reset"}
						</Button>
					</div>
				)}

				{/* Break：flag 提交（§66，每 GameBox 一次性） */}
				{phase === "break" && (
					<div className="flex flex-col gap-2 border-t pt-2">
						{viewModel.judgeEndpoint && (
							<div className="flex flex-col gap-1 rounded border border-dashed border-gray-300 p-2">
								<p className="font-mono text-xs">
									Flag Endpoint: {viewModel.judgeEndpoint.flagUrl}
								</p>
								<p className="text-xs text-gray-500">
									Available from your GameBox only
								</p>
							</div>
						)}
						<div className="flex items-center gap-2">
							<TextInput
								value={flagInputs[gb.id] ?? ""}
								onChange={(e) =>
									setFlagInputs((prev) => ({
										...prev,
										[gb.id]: e.target.value,
									}))
								}
								placeholder="flag{...}"
								block
							/>
							<Button
								variant="primary"
								disabled={
									cardBlocked || !flagInputs[gb.id] || busy[`break:${gb.id}`]
								}
								onClick={() => handleSubmitBreak(gb)}
							>
								Submit
							</Button>
						</div>
						{gb.broken && (
							<p className="text-sm">
								<span className="text-green-600 font-medium">Broken</span>
								<span className="ml-2 tabular-nums">
									+{viewModel.breakScore}
								</span>
							</p>
						)}
					</div>
				)}

				{/* Fix：源码 / Patch 上传 / Test Check（§67） */}
				{phase === "fix" && (
					<div className="flex flex-col gap-2 border-t pt-2">
						<div className="flex items-center gap-2 flex-wrap">
							<Button
								disabled={
									cardBlocked || busy[`source:${gb.id}`] || !onDownloadSource
								}
								onClick={() => handleDownloadSource(gb)}
							>
								{busy[`source:${gb.id}`] ? "…" : "Download Source"}
							</Button>
							<input
								ref={(el) => {
									fileInputRefs.current[gb.id] = el;
								}}
								type="file"
								accept=".sh,text/x-shellscript"
								className="hidden"
								onChange={(e) =>
									setPatchFiles((prev) => ({
										...prev,
										[gb.id]: e.target.files?.[0] ?? null,
									}))
								}
							/>
							<Button
								aria-label="选择 patch 脚本文件"
								onClick={() => fileInputRefs.current[gb.id]?.click()}
							>
								{patchFiles[gb.id] ? patchFiles[gb.id]!.name : "选择 patch 文件"}
							</Button>
							<Button
								variant="primary"
								disabled={
									cardBlocked || !patchFiles[gb.id] || busy[`patch:${gb.id}`]
								}
								onClick={() => handleUploadPatch(gb)}
							>
								{busy[`patch:${gb.id}`] ? "Applying…" : "Apply Patch"}
							</Button>
							<Button
								disabled={
									cardBlocked ||
									checking[gb.id] ||
									busy[`check:${gb.id}`] ||
									!running
								}
								onClick={() => handleTestCheck(gb)}
							>
								{checking[gb.id] ? "Checking…" : "Test Check"}
							</Button>
						</div>
						<div className="text-sm flex items-center gap-2">
							<span className="opacity-70">Last patch:</span>
							{patchStatus ? (
								<Label
									variant={
										patchStatus === "applied"
											? "success"
											: patchStatus === "failed"
												? "danger"
												: "attention"
									}
								>
									{patchStatus}
								</Label>
							) : (
								<span className="opacity-50">-</span>
							)}
						</div>
						{check && (
							<div className="text-sm">
								{check.healthcheck_ok == null ? (
									<span className="text-gray-500">Test Check 排队中…</span>
								) : (
									<>
										<span
											className={
												check.healthcheck_ok
													? "text-green-600"
													: "text-red-600"
											}
										>
											健康检查：{check.healthcheck_ok ? "OK" : "DOWN"}
										</span>
										<span className="mx-2">·</span>
										<span
											className={check.judge_ok ? "text-green-600" : "text-red-600"}
										>
											Judge：{check.judge_ok ? "PASS" : "FAIL"}
										</span>
										<span className="ml-2 text-xs opacity-60">（不计分）</span>
									</>
								)}
							</div>
						)}
					</div>
				)}
			</section>
		);
	};

	// ── Official History 表（rounds.tsx 样式）────────────────────────────────

	const historyColumns = [
		{
			accessorKey: "sequence",
			header: "Turn",
			field: "sequence",
			rowHeader: true,
			renderCell: (row: AwdpHistoryRow) => <span>#{row.sequence}</span>,
		},
		{
			accessorKey: "starts_at",
			header: "Starts",
			field: "starts_at",
			renderCell: (row: AwdpHistoryRow) => (
				<span>{fmtTime(row.starts_at)}</span>
			),
		},
		{
			accessorKey: "cutoff_at",
			header: "Cutoff",
			field: "cutoff_at",
			renderCell: (row: AwdpHistoryRow) => (
				<span>{fmtTime(row.cutoff_at)}</span>
			),
		},
		{
			accessorKey: "label",
			header: "Result",
			field: "label",
		},
		{
			accessorKey: "delta",
			header: "Score",
			field: "delta",
			renderCell: (row: AwdpHistoryRow) => (
				<span
					className={
						row.delta != null && row.delta > 0
							? "text-green-600 font-medium tabular-nums"
							: "tabular-nums"
					}
				>
					{row.delta != null
						? row.delta > 0
							? `+${row.delta}`
							: `${row.delta}`
						: "-"}
				</span>
			),
		},
	];
	const historyTable = useReactTable({
		data: viewModel.history,
		columns: historyColumns,
		getCoreRowModel: getCoreRowModel(),
	});

	return (
		<div className="h-full w-full flex flex-col gap-2 min-h-0">
			{/* 顶部：标题 + 描述（与挑战详情页同款：text-2xl + border-top 分隔，无按钮） */}
			{viewModel.title ? (
				<div id="awdp-meta" className="shrink-0">
					<p className="font-bold text-2xl">{viewModel.title}</p>
					{viewModel.description ? (
						<div className="border-top mt-2 pt-2">
							{viewModel.description}
						</div>
					) : null}
				</div>
			) : null}
			<banner.BannerComponent />

			{/* 顶部状态卡片：阶段控制（练习 SegmentedControl / 竞赛 badge）+ End | 事件式倒计时 + 得分 | Break→Fix 时间线 */}
			<div className="mb-2 shrink-0">
				<AwdpPhaseOverview
					phase={phase}
					startedAt={viewModel.startedAt ?? null}
					breakEndsAt={viewModel.breakEndsAt ?? null}
					fixStartedAt={viewModel.fixStartedAt ?? null}
					fixEndsAt={viewModel.fixEndsAt ?? null}
					breakDurationSecs={viewModel.breakDurationSecs}
					fixDurationSecs={viewModel.fixDurationSecs}
					currentRound={viewModel.currentRound}
					totalRounds={viewModel.totalRounds}
					nextCheckAt={viewModel.nextCheckAt}
					score={viewModel.score}
					now={now}
					canControlPhase={viewModel.canControlPhase}
					onSetPhase={onSetPhase ? handleSetPhase : undefined}
					phaseBusy={phaseBusy}
					onEnd={onEnd ? handleEnd : undefined}
					endBusy={!!busy.end || phaseBusy}
					endRunning={!!busy.end}
				/>
			</div>

			{/* GameBox 卡片列表（超出高度内部滚动，避免页面溢出） */}
			<div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 pr-1">
				{viewModel.gameboxes.map(renderGameBox)}
				{viewModel.gameboxes.length === 0 && (
					<p className="text-sm opacity-70">暂无 GameBox。</p>
				)}

				{/* Official History（fix|ended） */}
				{phase === "fix" || phase === "ended" ? (
					<section className="p-3 rounded border">
						<h4 className="font-bold mb-2">Official History</h4>
						<Table.Container>
							<DataTable
								aria-labelledby="awdp-official-history"
								// @ts-ignore
								columns={historyColumns}
								data={historyTable
									.getRowModel()
									.rows.map((row) => row.original)}
							/>
						</Table.Container>
						{viewModel.history.length === 0 && (
							<p className="text-sm opacity-70 mt-2">暂无评估记录。</p>
						)}
					</section>
				) : null}

				{/* Ended（§68） */}
				{phase === "ended" ? (
					<section className="p-3 rounded border flex flex-col gap-3">
						<div className="flex items-center gap-2">
							<h4 className="font-bold flex-1">Final Score</h4>
							<strong className="text-lg tabular-nums">
								{viewModel.score}
							</strong>
						</div>
						<div>
							<h5 className="font-bold text-sm mb-2">Break Results</h5>
							<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm">
								{viewModel.gameboxes.map((gb) => (
									<Fragment key={gb.id}>
										<dt className="font-bold">{gb.name}</dt>
										<dd className="font-medium">
											{gb.broken ? (
												<span className="text-green-600">
													Broken +{viewModel.breakScore}
												</span>
											) : (
												<span className="opacity-60">Unbroken</span>
											)}
										</dd>
									</Fragment>
								))}
								{viewModel.gameboxes.length === 0 && (
									<dd className="font-medium opacity-60">暂无 GameBox。</dd>
								)}
							</dl>
						</div>
						{viewModel.scoreHistory && viewModel.scoreHistory.length > 0 ? (
							<div>
								<h5 className="font-bold text-sm mb-2">Score Ledger</h5>
								<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm">
									{viewModel.scoreHistory.map((s) => (
										<Fragment key={s.id}>
											<dt className="font-bold">
												{s.score_type === "break" ? "Break" : "Fix"}
											</dt>
											<dd className="font-medium tabular-nums">
												{s.delta > 0 ? "+" : ""}
												{s.delta} · {fmtTime(s.created_at)}
											</dd>
										</Fragment>
									))}
								</dl>
							</div>
						) : null}
						{viewModel.isPractice && onTrainAgain && (
							<Button
								variant="primary"
								className="w-40"
								disabled={busy["train-again"]}
								onClick={handleTrainAgain}
							>
								{busy["train-again"] ? "Restarting…" : "Train Again"}
							</Button>
						)}
					</section>
				) : null}
			</div>

			{children}
		</div>
	);
}
