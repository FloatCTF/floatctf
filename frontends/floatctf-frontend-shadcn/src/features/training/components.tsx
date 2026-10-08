/**
 * AWDP Training Ground 的域内组件：
 * 阶段步进条 / 实例卡 / Break·Fix·ALL Check 动作区 / 轮次与评测 / 积分明细 / Writeup。
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
	Save,
	ShieldCheck,
	Square,
	Terminal,
	TriangleAlert,
	XCircle,
} from "lucide-react";

import type {
	AllCheckDto,
	AwdpInstance,
	AwdpRunDto,
	AwdpRunEvaluationDto,
	AwdpRunScoresDto,
	AwdpRunWriteupDto,
	AwdpRoundDto,
	BreakSubmitResponse,
	ManualCheckDto,
	PatchSubmitResponse,
} from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable } from "~/components/app/data-table";
import { Field } from "~/components/app/form";
import { MarkdownEditor } from "~/components/app/markdown";
import { CopyText, MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, ErrorBlock, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { formatDateTime, formatDuration, formatInt, formatScore } from "~/lib/format";
import { useNow } from "~/lib/hooks";

import {
	RUN_ALL_CHECK_CONSEQUENCES,
	RUN_PHASE_ORDER,
	runInstanceControlGate,
	RUN_PREPARING_FIX_NOTE,
	allCheckRunGate,
	breakRunGate,
	isRunInstanceRunning,
	patchRunGate,
	runEvaluationLabel,
	runEvaluationTone,
	runPhaseIndex,
	runPhaseLabel,
	runPhaseTone,
	runRuntimeLabel,
	runRuntimeTone,
	sourceRunGate,
	testCheckRunGate,
} from "./domain";

/** 阶段步进条（pending → Break → 准备 Fix → Fix → 已结束）。 */
export function RunPhaseSteps({
	phase,
	currentRound,
	totalRounds,
	nextActionAt,
}: {
	phase: AwdpRunDto["phase"];
	currentRound: number;
	totalRounds: number;
	nextActionAt: string | null;
}): ReactNode {
	const currentIndex = runPhaseIndex(phase);
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
				{RUN_PHASE_ORDER.map((step, index) => {
					const done = currentIndex > index;
					const active = currentIndex === index;
					return (
						<li key={step} className="flex items-center gap-2">
							<TonePill
								tone={active ? runPhaseTone(step) : done ? "success" : "muted"}
								className={active ? "ring-1 ring-current" : undefined}
							>
								{runPhaseLabel(step)}
							</TonePill>
							{index < RUN_PHASE_ORDER.length - 1 ? (
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
					<span>{RUN_PREPARING_FIX_NOTE}</span>
				</div>
			) : null}
		</div>
	);
}

/** 实例卡：端点、重建代数、重置次数 + 启动 / 停止 / 重置 / 刷新（取数由页面持有）。 */
export function TrainingInstancePanel({
	gameboxName,
	broken,
	instance,
	isPending,
	isError,
	error,
	judgeEndpoint,
	busy,
	phase,
	onStart,
	onStop,
	onReset,
	onRefresh,
}: {
	gameboxName: string;
	broken: boolean;
	instance: AwdpInstance | null;
	isPending: boolean;
	isError: boolean;
	error: unknown;
	judgeEndpoint: AwdpRunDto["judge_endpoint"];
	busy: boolean;
	phase: AwdpRunDto["phase"];
	onStart: () => void;
	onStop: () => void;
	onReset: () => void;
	onRefresh: () => void;
}): ReactNode {
	const running = isRunInstanceRunning(instance);
	const control = runInstanceControlGate(phase);

	return (
		<SectionCard
			title="我的靶机实例"
			description={gameboxName}
			actions={
				<div className="flex items-center gap-1">
					<TonePill tone={broken ? "danger" : "muted"}>
						{broken ? "已被攻破" : "未被攻破"}
					</TonePill>
					<Button variant="ghost" size="icon-xs" onClick={onRefresh} aria-label="刷新实例">
						<RefreshCw />
					</Button>
				</div>
			}
		>
			{isPending ? (
				<TableSkeleton rows={2} columns={2} />
			) : isError ? (
				<ErrorBlock error={error} title="加载实例失败" onRetry={onRefresh} />
			) : (
				<div className="space-y-3">
					<div className="flex flex-wrap items-center gap-2">
						<TonePill tone={runRuntimeTone(instance?.runtime_state)}>
							{runRuntimeLabel(instance?.runtime_state)}
						</TonePill>
						{instance ? (
							<>
								<span className="tnum text-xs text-muted-foreground">
									重建代数 {formatInt(instance.runtime_generation)}
								</span>
								<span className="tnum text-xs text-muted-foreground">
									手动 Reset {formatInt(instance.reset_count)} 次
								</span>
								<MonoText className="text-muted-foreground">
									{instance.instance_id}
								</MonoText>
							</>
						) : (
							<span className="text-xs text-muted-foreground">
								尚未启动：实例不存在，端点也不会暴露。
							</span>
						)}
					</div>

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
						<p className="text-xs text-muted-foreground">实例未运行时不暴露服务端点。</p>
					)}

					{!control.allowed ? <TonePill tone="warning">{control.reason}</TonePill> : null}
					<div className="flex flex-wrap items-center gap-2">
						<Button
							variant="outline"
							size="sm"
							disabled={busy || running || !control.allowed}
							title={
								!control.allowed
									? control.reason
									: running
										? "实例已在运行"
										: "启动（或复用）实例"
							}
							onClick={onStart}
						>
							<Play /> 启动实例
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={busy || !running || !control.allowed}
							title={
								!control.allowed
									? control.reason
									: running
										? "停止实例（保留端点）"
										: "实例未运行"
							}
							onClick={onStop}
						>
							<Square /> 停止实例
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={busy || !instance || !control.allowed}
							title={
								!control.allowed
									? control.reason
									: instance
										? "重置为原始状态（破坏性）"
										: "实例不存在"
							}
							onClick={onReset}
						>
							<RotateCcw /> 重置实例
						</Button>
					</div>

					{judgeEndpoint ? (
						<div className="space-y-1 rounded-md border bg-muted/30 px-3 py-2 text-xs">
							<p className="flex items-center gap-1.5 font-medium">
								<ShieldCheck className="size-3.5" /> 练习 Flag Server（data plane）
							</p>
							<p className="text-muted-foreground">
								作用域 {judgeEndpoint.scope}：仅 GameBox 内部网络可达，用作题目内的 SSRF / 攻击目标。
							</p>
							<div className="flex flex-wrap items-center gap-2">
								<CopyText value={judgeEndpoint.base_url} label="Flag Server 地址" />
								<CopyText value={judgeEndpoint.flag_url} label="Flag URL" />
							</div>
						</div>
					) : null}
				</div>
			)}
		</SectionCard>
	);
}

/**
 * 动作区：Break flag / Fix 补丁 / Test Check / ALL Check / 源码下载。
 * 所有结果（accepted/scored/already_broken、patch applied/failed、自检三项、ALL Check 终态）
 * 都直接展示后端返回值。
 */
export function TrainingActionPanel({
	runId,
	gameboxId,
	phase,
	instance,
	onChanged,
}: {
	runId: string;
	gameboxId: string;
	phase: AwdpRunDto["phase"];
	instance: AwdpInstance | null;
	onChanged: () => void;
}): ReactNode {
	const client = useClient();
	const confirm = useConfirm();
	const [flag, setFlag] = useState("");
	const [breakResult, setBreakResult] = useState<BreakSubmitResponse | null>(null);
	const [patchResult, setPatchResult] = useState<PatchSubmitResponse | null>(null);
	const [checkResult, setCheckResult] = useState<ManualCheckDto | null>(null);
	const [allCheckResult, setAllCheckResult] = useState<AllCheckDto | null>(null);
	const fileRef = useRef<HTMLInputElement | null>(null);

	const breakGate = breakRunGate(phase);
	const patchGate = patchRunGate(phase, instance);
	const testGate = testCheckRunGate(instance);
	const allGate = allCheckRunGate(phase, instance);
	const sourceGate = sourceRunGate(phase);

	const breakMutation = useMutation({
		mutationFn: (value: string) =>
			call<BreakSubmitResponse>(
				client.awdp.runs.submitBreak(runId, gameboxId, value),
				"提交 Break flag",
			),
		onSuccess: (result) => {
			setBreakResult(result);
			setFlag("");
			if (result.scored) toast.success("Break 得分已入账");
			else if (result.already_broken) toast.info("此前已攻破", "本次提交不计分。");
			else if (result.accepted) toast.info("flag 已接受，但未计分");
			else toast.error("flag 未被接受");
			onChanged();
		},
		onError: (error) => toast.apiError("提交 Break flag 失败", error),
	});

	const patchMutation = useMutation({
		mutationFn: (file: File) =>
			call<PatchSubmitResponse>(
				client.awdp.runs.uploadPatch(runId, gameboxId, file),
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
			call<ManualCheckDto>(client.awdp.runs.testCheck(runId, gameboxId), "Test Check"),
		onSuccess: (result) => {
			setCheckResult(result);
			toast.success("自检完成", "不计分，仅用于提交前确认。");
			onChanged();
		},
		onError: (error) => toast.apiError("Test Check 失败", error),
	});

	const allCheckMutation = useMutation({
		mutationFn: () =>
			call<AllCheckDto>(client.awdp.runs.allCheck(runId, gameboxId), "ALL Check"),
		onSuccess: (result) => {
			setAllCheckResult(result);
			if (result.status === "patched") {
				toast.success(
					"ALL Check 通过",
					result.swept
						? `剩余 ${result.swept_rounds} 个回合已全部计分，run 已结束。`
						: "判定通过。",
				);
			} else {
				toast.error("ALL Check 未通过", "本次不落账、不扣分，等本轮官方 check。");
			}
			onChanged();
		},
		onError: (error) => toast.apiError("ALL Check 失败", error),
	});

	const sourceMutation = useMutation({
		mutationFn: () =>
			call<string>(client.awdp.runs.sourceUrl(runId, gameboxId), "获取源码地址"),
		onSuccess: (url) => {
			if (!url) {
				toast.error("后端未返回下载地址");
				return;
			}
			// presigned URL 直接交给浏览器，不 fetch 到内存。
			window.open(url, "_blank", "noopener,noreferrer");
			toast.success("已打开源码下载");
		},
		onError: (error) => toast.apiError("获取源码地址失败", error),
	});

	return (
		<div className="space-y-5">
			<SectionCard
				title="Break：提交 flag"
				description="在 Break 阶段攻破靶机并提交 flag。"
				actions={<TonePill tone={runPhaseTone(phase)}>{runPhaseLabel(phase)}</TonePill>}
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
						htmlFor="training-break-flag"
						error={!breakGate.allowed ? breakGate.reason : undefined}
					>
						<div className="flex gap-2">
							<Input
								id="training-break-flag"
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
				title="Fix：补丁 / 自检 / 全量校验"
				description="上传 patch.sh 修复漏洞；Test Check 自检；ALL Check 一键官方判定。"
			>
				<div className="space-y-4">
					<Field
						label="patch.sh"
						htmlFor="training-patch"
						error={!patchGate.allowed ? patchGate.reason : undefined}
						hint="multipart 字段名为 patch_file；不要手动设置 Content-Type。"
					>
						<div className="flex flex-wrap items-center gap-2">
							<Input
								id="training-patch"
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
							disabled={!allGate.allowed || allCheckMutation.isPending}
							title={allGate.allowed ? "一键官方判定：通过即结束本 run" : allGate.reason}
							onClick={async () => {
								// ALL Check 通过会直接结束本 run（剩余回合全部计分）→ 破坏性，需确认。
								const ok = await confirm({
									title: "执行 ALL Check（一键官方判定）？",
									description: "立即用官方流程判定当前补丁；一旦通过，本 run 会直接结束。",
									consequences: RUN_ALL_CHECK_CONSEQUENCES,
									tone: "danger",
									confirmText: "执行 ALL Check",
								});
								if (ok) allCheckMutation.mutate();
							}}
						>
							<CheckCircle2 /> ALL Check
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
					</div>
					{!testGate.allowed ? (
						<p className="text-xs text-muted-foreground">{testGate.reason}</p>
					) : null}
					{!allGate.allowed ? (
						<p className="text-xs text-muted-foreground">{allGate.reason}</p>
					) : null}
					{!sourceGate.allowed ? (
						<p className="text-xs text-muted-foreground">{sourceGate.reason}</p>
					) : null}

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
									<span>Healthcheck：{checkResult.healthcheck_ok ? "通过" : "未通过"}</span>
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
												? "漏洞仍可利用（不计分）"
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

					{allCheckResult ? (
						<div className="space-y-2 rounded-md border px-3 py-2 text-xs">
							<div className="flex flex-wrap items-center gap-2">
								<span className="text-muted-foreground">ALL Check 终态：</span>
								<TonePill
									tone={allCheckResult.status === "patched" ? "success" : "danger"}
								>
									{runEvaluationLabel(allCheckResult.status)}
								</TonePill>
								<TonePill tone={allCheckResult.swept ? "success" : "muted"}>
									{allCheckResult.swept
										? `剩余回合已全部计分（${allCheckResult.swept_rounds}）`
										: "未扫尾计分"}
								</TonePill>
								<span className="tnum text-muted-foreground">
									目标回合 #{allCheckResult.target_round}
								</span>
							</div>
							{allCheckResult.healthcheck_detail ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{allCheckResult.healthcheck_detail}
								</pre>
							) : null}
							{allCheckResult.judge_detail ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{allCheckResult.judge_detail}
								</pre>
							) : null}
							{allCheckResult.exploit_detail ? (
								<pre className="max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
									{allCheckResult.exploit_detail}
								</pre>
							) : null}
						</div>
					) : null}
				</div>
			</SectionCard>
		</div>
	);
}

/** 轮次与评测。 */
export function RunRoundsEvaluationsPanel({ runId }: { runId: string }): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const roundsQuery = useQuery({
		queryKey: qk.runs.rounds(runId),
		queryFn: () => call<AwdpRoundDto[]>(client.awdp.runs.rounds(runId), "Fix 回合"),
	});
	const evaluationsQuery = useQuery({
		queryKey: qk.runs.evaluations(runId),
		queryFn: () =>
			call<AwdpRunEvaluationDto[]>(client.awdp.runs.evaluations(runId), "我的评测"),
	});

	// SSE 失效契约缺口兜底：`useAwdpRunStream` 失效的是 `["awdp-run-evals", runId]`，
	// 而 `qk.runs.evaluations` 是 `["awdp-run-evaluations", runId]`（前缀不匹配）。
	// 回合会被事件流刷新 → 以回合刷新完成为信号顺带失效评测（单向，不成环）。
	const roundsUpdatedAt = roundsQuery.dataUpdatedAt;
	useEffect(() => {
		if (roundsUpdatedAt === 0) return;
		void queryClient.invalidateQueries({ queryKey: qk.runs.evaluations(runId) });
	}, [roundsUpdatedAt, queryClient, runId]);

	return (
		<div className="space-y-5">
			<SectionCard title="Fix 回合" description="每个回合 cutoff 时后端对在跑实例做官方判定。">
				{roundsQuery.isPending ? (
					<TableSkeleton rows={3} columns={3} />
				) : roundsQuery.isError ? (
					<ErrorBlock
						error={roundsQuery.error}
						title="加载回合失败"
						onRetry={() => void roundsQuery.refetch()}
					/>
				) : (roundsQuery.data ?? []).length === 0 ? (
					<EmptyBlock
						title="暂无回合"
						description="进入 Fix 阶段后，后端会物化回合时间线。"
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
				description="manual = Test Check；official = 回合官方判定。"
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
									<TonePill tone={runEvaluationTone(row.status)}>
										{runEvaluationLabel(row.status)}
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

/** 积分明细（append-only ledger）。 */
export function RunScoresPanel({ runId }: { runId: string }): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.runs.scores(runId),
		queryFn: () => call<AwdpRunScoresDto>(client.awdp.runs.scores(runId), "积分明细"),
	});

	if (query.isPending) return <TableSkeleton rows={4} columns={5} />;
	if (query.isError) {
		return (
			<ErrorBlock error={query.error} title="加载积分失败" onRetry={() => void query.refetch()} />
		);
	}
	const data = query.data;
	if (!data || data.history.length === 0) {
		return (
			<EmptyBlock
				title="暂无积分记录"
				description="Break 攻破或 Fix 回合判定成功后会写入计分账本。"
			/>
		);
	}
	return (
		<div className="space-y-3">
			<div className="flex items-center gap-2">
				<span className="text-xs text-muted-foreground">当前总分</span>
				<MonoText className="text-base font-semibold">{formatScore(data.total)}</MonoText>
			</div>
			<DataTable
				data={data.history}
				getRowId={(row) => row.id}
				columns={[
					{
						id: "type",
						header: "类型",
						sortValue: (row) => row.score_type,
						cell: (row) => (
							<TonePill tone={row.score_type === "break" ? "info" : "success"}>
								{row.score_type === "break" ? "Break" : "Fix"}
							</TonePill>
						),
					},
					{
						id: "delta",
						header: "分值",
						align: "right",
						sortValue: (row) => row.delta,
						cell: (row) => (
							<MonoText className={row.delta < 0 ? "text-destructive" : undefined}>
								{row.delta > 0 ? `+${formatScore(row.delta)}` : formatScore(row.delta)}
							</MonoText>
						),
					},
					{
						id: "round",
						header: "回合",
						hideBelow: "sm",
						sortValue: (row) => row.fix_round_id ?? "",
						cell: (row) => (
							<MonoText className="text-muted-foreground">
								{row.fix_round_id ? "回合判定" : "—"}
							</MonoText>
						),
					},
					{ id: "gamebox", header: "靶机", hideBelow: "md", cell: (row) => <MonoText>{row.gamebox_id}</MonoText> },
					{
						id: "created",
						header: "时间",
						sortValue: (row) => row.created_at,
						cell: (row) => (
							<span className="tnum text-xs text-muted-foreground">
								{formatDateTime(row.created_at, { seconds: true })}
							</span>
						),
					},
				]}
			/>
		</div>
	);
}

/** Writeup 读写（一 run 一份）。 */
export function RunWriteupPanel({ runId }: { runId: string }): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	// `null` = 用户尚未编辑；非 null 时以本地草稿为准，避免 refetch 覆盖输入。
	const [draft, setDraft] = useState<string | null>(null);

	const query = useQuery({
		queryKey: qk.runs.writeup(runId),
		queryFn: () =>
			call<AwdpRunWriteupDto>(client.awdp.runs.getWriteup(runId), "我的 Writeup"),
	});

	const saved = query.data?.content ?? "";
	const value = draft ?? saved;
	const dirty = draft !== null && draft !== saved;

	const mutation = useMutation({
		mutationFn: (content: string) =>
			call<AwdpRunWriteupDto>(
				client.awdp.runs.saveWriteup(runId, content),
				"保存 Writeup",
			),
		onSuccess: (result) => {
			setDraft(null);
			queryClient.setQueryData(qk.runs.writeup(runId), result);
			void queryClient.invalidateQueries({ queryKey: qk.runs.writeup(runId) });
			toast.success("Writeup 已保存");
		},
		onError: (error) => toast.apiError("保存 Writeup 失败", error),
	});

	return (
		<SectionCard
			title="我的 Writeup"
			description="记录破题与修复思路（Markdown，支持插图）。一个 run 一份。"
			actions={
				<div className="flex items-center gap-2">
					{query.data?.updated_at ? (
						<span className="tnum text-xs text-muted-foreground">
							更新于 {formatDateTime(query.data.updated_at, { seconds: true })}
						</span>
					) : null}
					<Button
						size="sm"
						disabled={!dirty || mutation.isPending || query.isPending}
						onClick={() => mutation.mutate(value)}
					>
						<Save /> 保存
					</Button>
				</div>
			}
		>
			{query.isPending ? (
				<TableSkeleton rows={5} columns={2} />
			) : query.isError ? (
				<ErrorBlock
					error={query.error}
					title="加载 Writeup 失败"
					onRetry={() => void query.refetch()}
				/>
			) : (
				<div className="space-y-2">
					<MarkdownEditor
						value={value}
						onChange={setDraft}
						minHeight={320}
						placeholder="记录你如何发现并利用漏洞、如何修复，以及验证结果…"
					/>
					{dirty ? (
						<p className="text-xs text-muted-foreground">有未保存的修改。</p>
					) : null}
				</div>
			)}
		</SectionCard>
	);
}
