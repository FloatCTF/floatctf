/**
 * AWDP 工作台的域内组件 —— 阶段步进条 / 我的靶机 / 实例卡 / Break·Fix 动作区 /
 * 轮次与评测 / 积分榜矩阵 / 趋势。
 *
 * 组件只做展示与各自的数据读取；`overview` 的取数与实例生命周期动作由页面持有，
 * 以保证「选中的靶机」在两列之间一致。
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	Activity,
	CheckCircle2,
	Download,
	FileUp,
	FlaskConical,
	Play,
	RefreshCw,
	RotateCcw,
	Square,
	Terminal,
	TriangleAlert,
	XCircle,
} from "lucide-react";

import type {
	AwdpEvaluationDto,
	AwdpGameBox,
	AwdpInstance,
	AwdpOverview,
	AwdpRoundDto,
	AwdpScoreRow,
	AwdpScoreboardDetail,
	AwdpTrendItem,
	BreakSubmitResponse,
	ManualCheckDto,
	PatchSubmitResponse,
} from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { TrendChart } from "~/components/app/charts";
import { DataTable } from "~/components/app/data-table";
import { Field } from "~/components/app/form";
import { CopyText, MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, ErrorBlock, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { formatDateTime, formatDuration, formatInt, formatScore } from "~/lib/format";
import { useNow } from "~/lib/hooks";

import {
	AWDP_PHASE_ORDER,
	AWDP_PREPARING_FIX_NOTE,
	awdpBreakGate,
	awdpInstanceControlGate,
	awdpEvaluationLabel,
	awdpEvaluationTone,
	awdpPatchGate,
	awdpPhaseIndex,
	awdpPhaseLabel,
	awdpPhaseTone,
	awdpRuntimeLabel,
	awdpRuntimeTone,
	awdpSourceGate,
	awdpTestCheckGate,
} from "./domain";

/** 阶段步进条：pending → Break → 准备 Fix（过渡）→ Fix → 已结束。 */
export function AwdpPhaseSteps({
	phase,
	currentRound,
	totalRounds,
	nextActionAt,
}: {
	phase: AwdpOverview["phase"];
	totalRounds: number;
	currentRound: number;
	nextActionAt: string | null;
}): ReactNode {
	const currentIndex = awdpPhaseIndex(phase);
	const now = useNow(1000);
	const remaining = (() => {
		if (!nextActionAt) return null;
		const target = Date.parse(nextActionAt);
		if (!Number.isFinite(target)) return null;
		return Math.max(0, Math.floor((target - now.getTime()) / 1000));
	})();

	return (
		<div className="space-y-3">
			<ol className="flex flex-wrap items-stretch gap-2">
				{AWDP_PHASE_ORDER.map((step, index) => {
					const done = currentIndex > index;
					const active = currentIndex === index;
					return (
						<li key={step} className="flex items-center gap-2">
							<TonePill
								tone={active ? awdpPhaseTone(step) : done ? "success" : "muted"}
								className={active ? "ring-1 ring-current" : undefined}
							>
								{awdpPhaseLabel(step)}
							</TonePill>
							{index < AWDP_PHASE_ORDER.length - 1 ? (
								<span className="text-muted-foreground">→</span>
							) : null}
						</li>
					);
				})}
			</ol>
			<div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
				{phase === "fix" ? (
					<span className="tnum">
						回合 {formatInt(currentRound)} / {formatInt(totalRounds)}
					</span>
				) : null}
				{remaining !== null && (phase === "break" || phase === "fix") ? (
					<span className="tnum">
						{phase === "break" ? "Break 剩余" : "下一轮判定倒计时"}{" "}
						{formatDuration(remaining)}
					</span>
				) : null}
				{nextActionAt ? (
					<span>下一动作时间：{formatDateTime(nextActionAt, { seconds: true })}</span>
				) : null}
			</div>
			{phase === "preparing_fix" ? (
				<div
					className="flex items-start gap-2 rounded-md border border-[var(--warning)]/40 bg-[var(--warning)]/10 px-3 py-2 text-xs"
					role="status"
				>
					<TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-[var(--warning)]" />
					<span>{AWDP_PREPARING_FIX_NOTE}</span>
				</div>
			) : null}
		</div>
	);
}

/** 左列：我的靶机（选择决定中间动作区与实例卡的对象）。 */
export function AwdpGameboxList({
	gameboxes,
	selectedId,
	onSelect,
}: {
	gameboxes: AwdpGameBox[];
	selectedId: string | null;
	onSelect: (id: string) => void;
}): ReactNode {
	if (gameboxes.length === 0) {
		return (
			<EmptyBlock
				title="本赛事没有可用 GameBox"
				description="赛事尚未部署 GameBox，或所有 GameBox 都被隐藏。"
			/>
		);
	}
	return (
		<ul className="space-y-2">
			{gameboxes.map((gamebox) => {
				const active = gamebox.id === selectedId;
				return (
					<li key={gamebox.id}>
						<button
							type="button"
							onClick={() => onSelect(gamebox.id)}
							aria-pressed={active}
							className={`w-full rounded-md border px-3 py-2 text-left transition-colors ${
								active ? "border-primary bg-primary/5" : "hover:bg-muted/40"
							}`}
						>
							<div className="flex items-center justify-between gap-2">
								<span className="truncate text-sm font-medium">{gamebox.name}</span>
								<span className="flex items-center gap-1">
									{gamebox.broken ? (
										<TonePill tone="danger">已被攻破</TonePill>
									) : (
										<TonePill tone="muted">未被攻破</TonePill>
									)}
									<TonePill tone={awdpRuntimeTone(gamebox.instance?.runtime_state)}>
										{awdpRuntimeLabel(gamebox.instance?.runtime_state)}
									</TonePill>
								</span>
							</div>
							<p className="mt-1 flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
								<span>{gamebox.category || "未分类"}</span>
								{gamebox.exposed.length > 0 ? (
									<>
										<span>·</span>
										<span>暴露</span>
										{gamebox.exposed.map(([proto, port]) => (
											<MonoText key={`${proto}:${port}`} className="text-muted-foreground">
												{proto}:{port}
											</MonoText>
										))}
									</>
								) : null}
								{gamebox.enabled ? null : <TonePill tone="muted">已禁用</TonePill>}
							</p>
						</button>
					</li>
				);
			})}
		</ul>
	);
}

/** 实例生命周期卡（启动 / 停止 / 重置 / 刷新）—— 动作与门禁原因全部显式。 */
export function AwdpInstancePanel({
	gamebox,
	instance,
	isPending,
	isError,
	error,
	busy,
	onStart,
	onStop,
	onReset,
	onRefresh,
	phase,
}: {
	gamebox: AwdpGameBox | null;
	instance: AwdpInstance | null;
	isPending: boolean;
	isError: boolean;
	error: unknown;
	busy: boolean;
	onStart: () => void;
	onStop: () => void;
	onReset: () => void;
	onRefresh: () => void;
	phase: AwdpOverview["phase"];
}): ReactNode {
	if (!gamebox) {
		return <EmptyBlock title="请选择一个靶机" description="先从左列选择要操作的 GameBox。" />;
	}
	const running = instance?.runtime_state === "running";
	const control = awdpInstanceControlGate(phase);

	return (
		<SectionCard
			title="实例"
			description={`${gamebox.name} 的容器实例：端点、重建代数与手动 Reset 次数。`}
			actions={
				<Button variant="ghost" size="icon-xs" onClick={onRefresh} aria-label="刷新实例">
					<RefreshCw />
				</Button>
			}
		>
			{isPending ? (
				<TableSkeleton rows={2} columns={2} />
			) : isError ? (
				<ErrorBlock error={error} title="加载实例失败" onRetry={onRefresh} />
			) : (
				<div className="space-y-3">
					<div className="flex flex-wrap items-center gap-2">
						<TonePill tone={awdpRuntimeTone(instance?.runtime_state)}>
							{awdpRuntimeLabel(instance?.runtime_state)}
						</TonePill>
						{instance ? (
							<>
								<span className="tnum text-xs text-muted-foreground">
									重建代数 {formatInt(instance.runtime_generation)}
								</span>
								<span className="tnum text-xs text-muted-foreground">
									手动 Reset {formatInt(instance.reset_count)} 次
								</span>
							</>
						) : (
							<span className="text-xs text-muted-foreground">
								尚未启动：实例不存在，端点也不会暴露。
							</span>
						)}
						<span className="text-xs text-muted-foreground">阶段 {awdpPhaseLabel(phase)}</span>
					</div>
					{!control.allowed ? (
						<TonePill tone="warning">{control.reason}</TonePill>
					) : null}

					{instance && instance.endpoints.length > 0 ? (
						<ul className="space-y-1.5">
							{instance.endpoints.map((endpoint) => {
								const url = `${endpoint.protocol}://${endpoint.public_host}:${endpoint.public_port}`;
								return (
									<li
										key={`${endpoint.protocol}:${endpoint.public_port}`}
										className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2"
									>
										<Terminal className="size-3.5 text-muted-foreground" />
										<CopyText value={url} label="服务地址" />
										<span className="tnum text-xs text-muted-foreground">
											容器端口 {endpoint.container_port}
										</span>
									</li>
								);
							})}
						</ul>
					) : (
						<p className="text-xs text-muted-foreground">
							实例未运行时不暴露服务端点。
						</p>
					)}

					<div className="flex flex-wrap items-center gap-2">
						<Button
							variant="outline"
							size="sm"
							disabled={busy || running || !control.allowed}
							onClick={onStart}
							title={
								!control.allowed
									? control.reason
									: running
										? "实例已在运行"
										: "启动（或复用）实例"
							}
						>
							<Play /> 启动
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={busy || !running || !control.allowed}
							onClick={onStop}
							title={
								!control.allowed
									? control.reason
									: running
										? "停止实例（保留端点）"
										: "实例未运行"
							}
						>
							<Square /> 停止
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={busy || !instance || !control.allowed}
							onClick={onReset}
							title={
								!control.allowed
									? control.reason
									: instance
										? "重置为原始状态（破坏性）"
										: "实例不存在"
							}
						>
							<RotateCcw /> 重置实例
						</Button>
					</div>
				</div>
			)}
		</SectionCard>
	);
}

/**
 * 中列：Break / Fix 动作区。
 * - Break：提交 flag（仅 Break 阶段）；
 * - Fix：上传 patch.sh（仅 Fix 阶段 + 实例运行中）、Test Check 自检、源码下载（仅 Fix）。
 * 结果（accepted/scored/already_broken、patch applied/failed、自检三项）**如实展示后端返回值**。
 */
export function AwdpActionPanel({
	eventId,
	gamebox,
	instance,
	phase,
	onChanged,
}: {
	eventId: string;
	gamebox: AwdpGameBox | null;
	instance: AwdpInstance | null;
	phase: AwdpOverview["phase"];
	onChanged: () => void;
}): ReactNode {
	const client = useClient();
	const [flag, setFlag] = useState("");
	const [breakResult, setBreakResult] = useState<BreakSubmitResponse | null>(null);
	const [patchResult, setPatchResult] = useState<PatchSubmitResponse | null>(null);
	const [checkResult, setCheckResult] = useState<ManualCheckDto | null>(null);
	const fileRef = useRef<HTMLInputElement | null>(null);

	const gameboxId = gamebox?.id ?? "";
	const breakGate = awdpBreakGate(phase);
	const patchGate = awdpPatchGate(phase, instance);
	const testGate = awdpTestCheckGate(instance);
	const sourceGate = awdpSourceGate(phase);

	const breakMutation = useMutation({
		mutationFn: (value: string) =>
			call<BreakSubmitResponse>(
				client.awdp.player.submitBreak(eventId, gameboxId, value),
				"提交 Break flag",
			),
		onSuccess: (result) => {
			setBreakResult(result);
			setFlag("");
			if (result.scored) toast.success("Break 得分已入账");
			else if (result.already_broken) toast.info("该靶机此前已被攻破", "本次提交不计分。");
			else if (result.accepted) toast.info("flag 已接受，但未计分");
			else toast.error("flag 未被接受");
			onChanged();
		},
		onError: (error) => toast.apiError("提交 Break flag 失败", error),
	});

	const patchMutation = useMutation({
		mutationFn: (file: File) =>
			call<PatchSubmitResponse>(
				client.awdp.player.uploadPatch(eventId, gameboxId, file),
				"上传补丁",
			),
		onSuccess: (result) => {
			setPatchResult(result);
			if (result.status === "applied") toast.success("补丁已应用");
			else toast.error("补丁应用失败", result.error_message ?? undefined);
			if (fileRef.current) fileRef.current.value = "";
			onChanged();
		},
		onError: (error) => {
			setPatchResult(null);
			toast.apiError("上传补丁失败", error);
		},
	});

	const checkMutation = useMutation({
		mutationFn: () =>
			call<ManualCheckDto>(client.awdp.player.testCheck(eventId, gameboxId), "Test Check"),
		onSuccess: (result) => {
			setCheckResult(result);
			toast.success("自检完成", "结果不计分，仅用于提交前确认。");
			onChanged();
		},
		onError: (error) => toast.apiError("Test Check 失败", error),
	});

	const sourceMutation = useMutation({
		mutationFn: () =>
			call<string>(client.awdp.player.sourceUrl(eventId, gameboxId), "获取源码地址"),
		onSuccess: (url) => {
			if (!url) {
				toast.error("后端未返回下载地址");
				return;
			}
			// presigned URL 直接交给浏览器下载，不 fetch 到内存。
			window.open(url, "_blank", "noopener,noreferrer");
			toast.success("已打开源码下载");
		},
		onError: (error) => toast.apiError("获取源码地址失败", error),
	});

	if (!gamebox) {
		return <EmptyBlock title="请选择一个靶机" description="先从左列选择要操作的 GameBox。" />;
	}

	return (
		<div className="space-y-5">
			<SectionCard
				title="Break：提交 flag"
				description="从靶机中取到 flag 后提交，按赛事配置计 Break 分。"
				actions={<TonePill tone={awdpPhaseTone(phase)}>{awdpPhaseLabel(phase)}</TonePill>}
			>
				<form
					className="space-y-3"
					onSubmit={(event) => {
						event.preventDefault();
						const value = flag.trim();
						if (!value || !breakGate.allowed || breakMutation.isPending) return;
						breakMutation.mutate(value);
					}}
				>
					<Field
						label="flag"
						htmlFor="awdp-break-flag"
						error={!breakGate.allowed ? breakGate.reason : undefined}
						hint="flag 只提交给后端校验，不写入 URL。"
					>
						<div className="flex gap-2">
							<Input
								id="awdp-break-flag"
								value={flag}
								autoComplete="off"
								spellCheck={false}
								placeholder="flag{...}"
								className="font-mono"
								disabled={!breakGate.allowed || breakMutation.isPending}
								onChange={(event) => setFlag(event.target.value)}
							/>
							<Button
								type="submit"
								disabled={
									!breakGate.allowed ||
									breakMutation.isPending ||
									flag.trim().length === 0
								}
							>
								<FlaskConical /> 提交
							</Button>
						</div>
					</Field>
				</form>
				{breakResult ? (
					<div className="mt-3 flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-xs">
						<span className="text-muted-foreground">后端返回：</span>
						<TonePill tone={breakResult.accepted ? "success" : "danger"}>
							{breakResult.accepted ? "已接受" : "未接受"}
						</TonePill>
						<TonePill tone={breakResult.scored ? "success" : "muted"}>
							{breakResult.scored ? "已计分" : "未计分"}
						</TonePill>
						<TonePill tone={breakResult.already_broken ? "warning" : "muted"}>
							{breakResult.already_broken ? "此前已攻破" : "首次攻破"}
						</TonePill>
					</div>
				) : null}
			</SectionCard>

			<SectionCard
				title="Fix：补丁与自检"
				description="上传 patch.sh 修复漏洞；Test Check 是官方评测前的自检（不计分）。"
			>
				<div className="space-y-4">
					<div className="space-y-2">
						<Field
							label="patch.sh"
							htmlFor="awdp-patch"
							error={!patchGate.allowed ? patchGate.reason : undefined}
							hint="以 multipart 字段 patch_file 上传；不要设置 Content-Type，SDK 会自动带 boundary。"
						>
							<div className="flex flex-wrap items-center gap-2">
								<Input
									id="awdp-patch"
									ref={fileRef}
									type="file"
									className="max-w-sm"
									disabled={!patchGate.allowed || patchMutation.isPending}
									onChange={(event) => {
										const file = event.target.files?.[0];
										if (file) patchMutation.mutate(file);
									}}
								/>
								<Button
									variant="outline"
									size="sm"
									disabled={!patchGate.allowed || patchMutation.isPending}
									onClick={() => fileRef.current?.click()}
								>
									<FileUp /> 选择并上传
								</Button>
							</div>
						</Field>
						{patchResult ? (
							<div className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-xs">
								<span className="text-muted-foreground">补丁结果：</span>
								<TonePill tone={patchResult.status === "applied" ? "success" : "danger"}>
									{patchResult.status === "applied" ? "已应用" : "应用失败"}
								</TonePill>
								{patchResult.error_message ? (
									<span className="text-destructive">{patchResult.error_message}</span>
								) : null}
							</div>
						) : null}
					</div>

					<div className="flex flex-wrap items-center gap-2">
						<Button
							variant="outline"
							size="sm"
							disabled={!testGate.allowed || checkMutation.isPending}
							title={testGate.allowed ? "运行 healthcheck + judge + exploit 诊断" : testGate.reason}
							onClick={() => checkMutation.mutate()}
						>
							<Activity /> Test Check
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={!sourceGate.allowed || sourceMutation.isPending}
							title={sourceGate.allowed ? "下载源码压缩包" : sourceGate.reason}
							onClick={() => sourceMutation.mutate()}
						>
							<Download /> 下载源码
						</Button>
						{!testGate.allowed ? (
							<span className="text-xs text-muted-foreground">{testGate.reason}</span>
						) : null}
						{!sourceGate.allowed ? (
							<span className="text-xs text-muted-foreground">{sourceGate.reason}</span>
						) : null}
					</div>

					{checkResult ? (
						<div className="space-y-2 rounded-md border px-3 py-2 text-xs">
							<p className="font-medium">
								自检结果（评估 <MonoText>{checkResult.evaluation_id}</MonoText>）
							</p>
							<ul className="space-y-1">
								<li className="flex items-center gap-2">
									{checkResult.healthcheck_ok ? (
										<CheckCircle2 className="size-3.5 text-[var(--success)]" />
									) : (
										<XCircle className="size-3.5 text-destructive" />
									)}
									<span>
										Healthcheck：{checkResult.healthcheck_ok ? "通过" : "未通过"}
									</span>
								</li>
								<li className="flex items-center gap-2">
									{checkResult.judge_ok ? (
										<CheckCircle2 className="size-3.5 text-[var(--success)]" />
									) : (
										<XCircle className="size-3.5 text-destructive" />
									)}
									<span>Judge：{checkResult.judge_ok ? "通过" : "未通过"}</span>
								</li>
								<li className="flex items-center gap-2">
									{checkResult.exploit_ok === null ? (
										<TonePill tone="muted">未执行</TonePill>
									) : checkResult.exploit_ok ? (
										<XCircle className="size-3.5 text-destructive" />
									) : (
										<CheckCircle2 className="size-3.5 text-[var(--success)]" />
									)}
									<span>
										Exploit：
										{checkResult.exploit_ok === null
											? "未执行到该步骤"
											: checkResult.exploit_ok
												? "漏洞仍可利用（不计分，需继续修复）"
												: "已不可利用"}
									</span>
								</li>
							</ul>
							{checkResult.healthcheck_detail?.length ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{checkResult.healthcheck_detail.join("\n")}
								</pre>
							) : null}
							{checkResult.judge_detail ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{checkResult.judge_detail}
								</pre>
							) : null}
							{checkResult.exploit_detail ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{checkResult.exploit_detail}
								</pre>
							) : null}
						</div>
					) : null}
				</div>
			</SectionCard>
		</div>
	);
}

/**
 * 右列：Fix 回合时间线 + 我的官方/手动评测。
 *
 * `enabled` 由页面的 overview 派生（`started_at !== null || phase !== "pending"`）：
 * 后端这两个端点在「赛事从未开始」时返回 400 `AWDP 事件尚未开始` —— 那是业务态，
 * 因此未开始时**不发起请求**，直接显示空态。
 */
export function AwdpRoundsEvaluationsPanel({
	eventId,
	enabled,
}: {
	eventId: string;
	enabled: boolean;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const roundsQuery = useQuery({
		queryKey: qk.awdp.rounds(eventId),
		queryFn: () => call<AwdpRoundDto[]>(client.awdp.player.rounds(eventId), "Fix 回合"),
		enabled,
	});
	const evaluationsQuery = useQuery({
		queryKey: qk.awdp.evaluations(eventId),
		queryFn: () =>
			call<AwdpEvaluationDto[]>(client.awdp.player.evaluations(eventId), "我的评估"),
		enabled,
	});

	// SSE 失效契约缺口兜底：`useAwdpEventStream` 失效的是 `["awdp-evals", eventId]`，
	// 而 `qk.awdp.evaluations` 是 `["awdp-evaluations", eventId]`（前缀不匹配），
	// 评测面板本身不会被事件流刷新。回合时间线**会**被刷新，因此以「回合刷新完成」为信号
	// 顺带失效一次评测（单向、不会成环），让评测结果也保持实时。
	// 已作为集成缺口写进交付报告；qk 由集成方维护，本 feature 不改共享文件。
	const roundsUpdatedAt = roundsQuery.dataUpdatedAt;
	useEffect(() => {
		if (roundsUpdatedAt === 0) return;
		void queryClient.invalidateQueries({ queryKey: qk.awdp.evaluations(eventId) });
	}, [roundsUpdatedAt, queryClient, eventId]);

	if (!enabled) {
		return (
			<EmptyBlock
				title="比赛尚未开始"
				description="AWDP 开始后，这里会出现 Fix 回合时间线与你的评测记录。"
			/>
		);
	}

	return (
		<div className="space-y-5">
			<SectionCard title="Fix 回合" description="每个回合结束时后端对在跑实例做官方判定。">
				{roundsQuery.isPending ? (
					<TableSkeleton rows={3} columns={4} />
				) : roundsQuery.isError ? (
					<ErrorBlock
						error={roundsQuery.error}
						title="加载回合失败"
						onRetry={() => void roundsQuery.refetch()}
					/>
				) : (roundsQuery.data ?? []).length === 0 ? (
					<EmptyBlock
						title="暂无回合"
						description="Fix 阶段开始后，后端会物化回合时间线。"
					/>
				) : (
					<DataTable
						data={roundsQuery.data ?? []}
						getRowId={(row) => row.id}
						columns={[
							{
								id: "sequence",
								header: "回合",
								sortValue: (row) => row.sequence,
								cell: (row) => <MonoText>#{row.sequence}</MonoText>,
							},
							{
								id: "cutoff",
								header: "截止",
								sortValue: (row) => row.cutoff_at,
								cell: (row) => (
									<span className="tnum text-xs">
										{formatDateTime(row.cutoff_at, { seconds: true })}
									</span>
								),
							},
							{
								id: "status",
								header: "状态",
								sortValue: (row) => row.status,
								cell: (row) => <MonoText>{row.status}</MonoText>,
							},
						]}
					/>
				)}
			</SectionCard>

			<SectionCard
				title="我的评测"
				description="manual = Test Check 自检；official = 每回合官方判定。"
			>
				{evaluationsQuery.isPending ? (
					<TableSkeleton rows={4} columns={4} />
				) : evaluationsQuery.isError ? (
					<ErrorBlock
						error={evaluationsQuery.error}
						title="加载评测失败"
						onRetry={() => void evaluationsQuery.refetch()}
					/>
				) : (evaluationsQuery.data ?? []).length === 0 ? (
					<EmptyBlock
						title="暂无评测记录"
						description="运行一次 Test Check 或等本轮官方判定后会出现在这里。"
					/>
				) : (
					<DataTable
						data={evaluationsQuery.data ?? []}
						getRowId={(row) => row.id}
						columns={[
							{
								id: "round",
								header: "回合",
								sortValue: (row) => row.round_sequence ?? 0,
								cell: (row) => (
									<MonoText>
										{row.round_sequence === null ? "—" : `#${row.round_sequence}`}
									</MonoText>
								),
							},
							{
								id: "kind",
								header: "类型",
								sortValue: (row) => row.kind,
								cell: (row) => (
									<TonePill tone={row.kind === "official" ? "info" : "muted"}>
										{row.kind === "official" ? "官方" : "自检"}
									</TonePill>
								),
							},
							{
								id: "status",
								header: "结果",
								sortValue: (row) => row.status,
								cell: (row) => (
									<TonePill tone={awdpEvaluationTone(row.status)}>
										{awdpEvaluationLabel(row.status)}
									</TonePill>
								),
							},
							{
								id: "finished",
								header: "完成时间",
								hideBelow: "md",
								sortValue: (row) => row.finished_at ?? "",
								cell: (row) => (
									<span className="tnum text-xs text-muted-foreground">
										{row.finished_at
											? formatDateTime(row.finished_at, { seconds: true })
											: "—"}
									</span>
								),
							},
						]}
					/>
				)}
			</SectionCard>
		</div>
	);
}

/** 底部 Tab：积分榜矩阵（Break/Fix 逐题逐回合）。 */
export function AwdpScoreboardPanel({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awdp.scoreboard(eventId),
		queryFn: () =>
			call<AwdpScoreboardDetail>(client.awdp.player.scoreboard(eventId), "AWDP 积分榜"),
	});

	if (query.isPending) return <TableSkeleton rows={6} columns={6} />;
	if (query.isError) {
		return (
			<ErrorBlock
				error={query.error}
				title="加载积分榜失败"
				onRetry={() => void query.refetch()}
			/>
		);
	}

	const data = query.data;
	if (!data || data.rows.length === 0) {
		return (
			<EmptyBlock
				title="暂无积分榜数据"
				description="比赛产生 Break / Fix 得分后这里会出现明细矩阵。"
			/>
		);
	}

	return (
		<div className="space-y-3">
			<p className="text-xs text-muted-foreground">
				对象：{data.participant_mode === "team" ? "队伍" : "个人"} · 题数{" "}
				{data.gameboxes.length} · 回合 {data.rounds.length}
			</p>
			{data.rows.map((row) => (
				<div
					key={row.subject_id}
					className={`space-y-2 rounded-md border p-3 ${row.is_me ? "border-primary bg-primary/5" : ""}`}
				>
					<div className="flex flex-wrap items-center gap-2">
						<MonoText className="font-semibold">#{row.rank}</MonoText>
						<span className="text-sm font-medium">{row.subject_name}</span>
						{row.is_me ? <TonePill tone="info">我</TonePill> : null}
						<span className="tnum ml-auto text-xs text-muted-foreground">
							Break {formatScore(row.break_score)} · Fix {formatScore(row.fix_score)} · 合计{" "}
							<span className="font-semibold text-foreground">
								{formatScore(row.total_score)}
							</span>
						</span>
					</div>
					<div className="overflow-x-auto">
						<table className="w-full text-xs">
							<thead>
								<tr className="text-muted-foreground">
									<th className="px-2 py-1 text-left font-normal">题目</th>
									<th className="px-2 py-1 text-left font-normal">Break</th>
									{data.rounds.map((round) => (
										<th key={round.sequence} className="px-2 py-1 text-left font-normal">
											R{round.sequence}
										</th>
									))}
									<th className="px-2 py-1 text-right font-normal">Fix 分</th>
								</tr>
							</thead>
							<tbody>
								{data.gameboxes.map((gamebox, index) => (
									<tr key={gamebox.id} className="border-t">
										<td className="px-2 py-1">{gamebox.name}</td>
										<td className="px-2 py-1">
											{row.break_status[index] ? (
												<TonePill tone="success">已攻破</TonePill>
											) : (
												<TonePill tone="muted">未攻破</TonePill>
											)}
										</td>
										{data.rounds.map((round, roundIndex) => {
											const status = row.fix_round_status[index]?.[roundIndex] ?? null;
											return (
												<td key={round.sequence} className="px-2 py-1">
													{status === null ? (
														<span className="text-muted-foreground">—</span>
													) : (
														<TonePill tone={awdpEvaluationTone(status)}>
															{awdpEvaluationLabel(status)}
														</TonePill>
													)}
												</td>
											);
										})}
										<td className="tnum px-2 py-1 text-right">
											{formatScore(row.fix_gamebox_score[index] ?? 0)}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
				</div>
			))}
		</div>
	);
}

/** 底部 Tab：官方积分榜（`awdp.player.scores` 聚合行）。 */
export function AwdpMyScoresPanel({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awdp.scores(eventId),
		queryFn: () => call<AwdpScoreRow[]>(client.awdp.player.scores(eventId), "AWDP 积分"),
	});

	if (query.isPending) return <TableSkeleton rows={5} columns={5} />;
	if (query.isError) {
		return (
			<ErrorBlock error={query.error} title="加载积分失败" onRetry={() => void query.refetch()} />
		);
	}
	const rows = query.data ?? [];
	if (rows.length === 0) {
		return <EmptyBlock title="暂无积分" description="比赛产生得分后这里会出现排名。" />;
	}
	return (
		<DataTable
			data={rows}
			getRowId={(row) => row.subject_id}
			columns={[
				{
					id: "rank",
					header: "排名",
					align: "right",
					sortValue: (row) => row.rank,
					cell: (row) => <MonoText>{row.rank}</MonoText>,
				},
				{
					id: "subject",
					header: "对象",
					sortValue: (row) => row.subject_name,
					cell: (row) => <span>{row.subject_name}</span>,
				},
				{
					id: "break",
					header: "Break",
					align: "right",
					sortValue: (row) => row.break_score,
					cell: (row) => <MonoText>{formatScore(row.break_score)}</MonoText>,
				},
				{
					id: "fix",
					header: "Fix",
					align: "right",
					sortValue: (row) => row.fix_score,
					cell: (row) => <MonoText>{formatScore(row.fix_score)}</MonoText>,
				},
				{
					id: "total",
					header: "合计",
					align: "right",
					sortValue: (row) => row.total_score,
					cell: (row) => (
						<MonoText className="font-semibold">{formatScore(row.total_score)}</MonoText>
					),
				},
			]}
		/>
	);
}

/**
 * 底部 Tab：官方积分趋势。
 *
 * 不叠加 `refetchInterval`：本页由 `useAwdpEventStream` 驱动失效（`awdp-trend` 在 hook 的
 * 失效列表里），SSE 断开时 hook 自身会降级为 15s 轮询并失效同一批 key —— 这正是约定 §4
 * 要求的「不要叠加轮询」；平台的 30s 新鲜度语义由此满足。
 */
export function AwdpTrendPanel({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awdp.trend(eventId),
		queryFn: () => call<AwdpTrendItem[]>(client.awdp.player.trend(eventId), "AWDP 趋势"),
	});

	if (query.isPending) return <TableSkeleton rows={5} columns={2} />;
	if (query.isError) {
		return (
			<ErrorBlock error={query.error} title="加载趋势失败" onRetry={() => void query.refetch()} />
		);
	}
	const series = (query.data ?? []).map((item) => ({
		name: item.name,
		points: item.points
			.map((point) => ({ x: Date.parse(point.time), y: point.score }))
			.filter((point) => Number.isFinite(point.x)),
	}));
	if (series.length === 0 || series.every((item) => item.points.length === 0)) {
		return <EmptyBlock title="暂无趋势数据" description="产生得分后这里会绘制累计得分曲线。" />;
	}
	return (
		<TrendChart
			series={series}
			formatY={(value) => formatScore(value)}
			formatX={(x) => new Date(x).toLocaleTimeString("zh-CN", { hour12: false })}
		/>
	);
}
