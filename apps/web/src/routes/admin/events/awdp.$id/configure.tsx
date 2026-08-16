import {
	Box,
	Button,
	FormControl,
	Label,
	Spinner,
	TextInput,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import {
	type ChangeEvent,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";

import { type AwdpEventConfigDto, awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import {
	AwdpTimeline,
	computeTimelineState,
	formatDuration,
} from "@/components/awdp/AwdpPhaseOverview";
import { AdminRouteGuard } from "../../route";
import { EventContext } from "./route";

export const Route = createFileRoute("/admin/events/awdp/$id/configure")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

/** Break 保底时长（与后端 BREAK_MIN_SECS 一致；总时长零头归 Break，但不低于此值）。 */
const BREAK_MIN_SECS = 60;

type FormState = {
	/** Fix 阶段时长（秒）——可配置；实时取整为回合时长的整数倍（整数回合）。 */
	fixDurationSecs: string;
	/** Break 阶段时长（秒）——只读，由“比赛总时长 - Fix 时长”推导（吸收零头）。 */
	breakDurationSecs: string;
	fixRoundIntervalSecs: string;
	breakScore: string;
	fixRoundScore: string;
};

const DEFAULT_FORM: FormState = {
	fixDurationSecs: "3600",
	breakDurationSecs: "3600",
	fixRoundIntervalSecs: "600",
	// 默认 Break Score = Fix 满分 × 0.6（150 × 6 轮 × 0.6 = 540）。
	breakScore: "540",
	fixRoundScore: "150",
};

const PHASE_LABEL: Record<
	string,
	{ text: string; variant: "success" | "accent" | "attention" | "done" }
> = {
	pending: { text: "pending", variant: "attention" },
	break: { text: "break", variant: "accent" },
	fix: { text: "fix", variant: "success" },
	ended: { text: "ended", variant: "done" },
};

function fmt(iso: string | null | undefined) {
	if (!iso) return "-";
	return new Date(iso).toLocaleString();
}

function RouteComponent() {
	const { id } = Route.useParams();
	const event = useContext(EventContext);
	const qc = useQueryClient();
	const banner = useMsgBanner({});
	const [form, setForm] = useState<FormState>(DEFAULT_FORM);
	const [dirty, setDirty] = useState(false);
	const loadedVersion = useRef<string | null>(null);
	// 管理员手动改过 Break Score 后，不再被 Fix 参数联动覆盖。
	const breakScoreTouched = useRef(false);

	const configQuery = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
	});
	const config: AwdpEventConfigDto | null = configQuery.data?.data ?? null;

	useEffect(() => {
		if (!config) {
			if (!dirty && loadedVersion.current !== null) {
				setForm(DEFAULT_FORM);
				loadedVersion.current = null;
				breakScoreTouched.current = false;
			}
			return;
		}
		if (loadedVersion.current === config.updated_at) return;
		if (dirty && loadedVersion.current !== null) return;
		const total =
			event?.start_time && event?.end_time
				? Math.floor(
						(new Date(event.end_time).getTime() -
							new Date(event.start_time).getTime()) /
							1000,
					)
				: null;
		const breakDurationSecs =
			total !== null
				? Math.max(0, total - config.fix_duration_secs)
				: config.break_duration_secs;
		setForm({
			breakDurationSecs: String(breakDurationSecs),
			fixDurationSecs: String(config.fix_duration_secs),
			fixRoundIntervalSecs: String(config.fix_round_interval_secs),
			breakScore: String(config.break_score),
			fixRoundScore: String(config.fix_round_score),
		});
		loadedVersion.current = config.updated_at;
		// 表单被远端配置（重新）加载后，恢复 Break Score 自动联动：
		// 否则切换事件/刷新后残留的手动编辑标记会让动态计算永久失效。
		breakScoreTouched.current = false;
	}, [config, dirty]);

	const save = useMutation({
		mutationFn: () =>
			awdpAdminApi.updateConfig(id, {
				expected_updated_at: loadedVersion.current ?? undefined,
				// V3：Fix 可配置（实时取整为整数回合并夹取），Break 由总时长 - Fix 推导。
				fix_duration_secs: effectiveFixSecs,
				fix_round_interval_secs: turnSecs,
				break_score: Number(form.breakScore),
				fix_round_score: Number(form.fixRoundScore),
			}),
		onSuccess: () => {
			setDirty(false);
			// 保存成功后继续编辑 Fix 参数时仍按 ×0.6 规则联动（除非再次手动改 Break Score）。
			breakScoreTouched.current = false;
			banner.showBanner("success", "AWDP configuration saved");
			qc.invalidateQueries({ queryKey: ["awdp-config", id] });
			qc.invalidateQueries({ queryKey: ["event", id] });
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			void configQuery.refetch();
		},
	});

	if (configQuery.isLoading) return <Spinner size="large" />;
	if (configQuery.isError) {
		return <div>Failed to load AWDP configuration.</div>;
	}

	const editable = !config || config.phase === "pending";
	const remoteChanged = Boolean(
		config &&
			loadedVersion.current !== null &&
			loadedVersion.current !== config.updated_at,
	);
	const formReady = !config || loadedVersion.current === config.updated_at;
	// 比赛总时长 = event.end_time - event.start_time。
	const totalSecs = (() => {
		if (!event?.start_time || !event?.end_time) return null;
		const diff =
			new Date(event.end_time).getTime() - new Date(event.start_time).getTime();
		return Math.floor(diff / 1000);
	})();
	const fixSecs = Number(form.fixDurationSecs) || 0;
	const turnSecs = Number(form.fixRoundIntervalSecs) || 0;

	// V3 核心：Fix 取整为整数回合（回合数 × 回合时长），再夹取到
	// [1 回合, 总时长 − Break 保底]；Break = 总时长 − Fix，吸收全部零头（可非整数）。
	const rawRounds = turnSecs > 0 ? Math.floor(fixSecs / turnSecs) : 0;
	let effectiveFixSecs = rawRounds * turnSecs;
	const maxFixSecs =
		totalSecs !== null ? Math.max(0, totalSecs - BREAK_MIN_SECS) : null;
	if (maxFixSecs !== null) {
		effectiveFixSecs = Math.min(effectiveFixSecs, maxFixSecs);
	}
	if (turnSecs > 0) {
		effectiveFixSecs = Math.max(effectiveFixSecs, turnSecs);
	}
	const breakSecs =
		totalSecs !== null
			? Math.max(0, totalSecs - effectiveFixSecs)
			: Number(form.breakDurationSecs) || 0;
	const totalRounds = turnSecs > 0 ? effectiveFixSecs / turnSecs : 0;
	// 有效状态：Fix 至少 1 回合、Break 不低于保底、Fix 为整数回合。
	const fixTooSmall = turnSecs > 0 && effectiveFixSecs < turnSecs;
	const breakBelowMin = totalSecs !== null && breakSecs < BREAK_MIN_SECS;
	const shortEvent =
		totalSecs !== null &&
		turnSecs > 0 &&
		totalSecs < turnSecs + BREAK_MIN_SECS;
	const invalid =
		effectiveFixSecs <= 0 ||
		turnSecs <= 0 ||
		fixTooSmall ||
		breakBelowMin ||
		shortEvent;
	// 提示：整数回退 / 夹取原因。
	const fixHint = (() => {
		if (effectiveFixSecs !== fixSecs && turnSecs > 0) {
			if (shortEvent) {
				return "总时长过短（不足 1 回合 + Break 保底），请缩小回合时长";
			}
			if (fixSecs > effectiveFixSecs) {
				return maxFixSecs !== null && fixSecs > maxFixSecs
					? `Fix 过大：夹取为 ${effectiveFixSecs}s（Break 保底 ${BREAK_MIN_SECS}s）`
					: `输入 ${fixSecs}s，取整为 ${effectiveFixSecs}s（${totalRounds} 回合 × ${turnSecs}s）`;
			}
		}
		return null;
	})();

	// Timeline 预览：根据当前表单输入实时计算（Break 由总时长 - Fix 推导）。
	const startIso = event?.start_time ?? config?.started_at ?? null;
	const startMs = startIso ? new Date(startIso).getTime() : null;
	const breakEndIso =
		startMs !== null
			? new Date(startMs + breakSecs * 1000).toISOString()
			: null;
	const fixEndIso =
		startMs !== null && totalSecs !== null
			? new Date(startMs + totalSecs * 1000).toISOString()
			: startMs !== null
				? new Date(startMs + (breakSecs + effectiveFixSecs) * 1000).toISOString()
				: null;
	const timelineState = computeTimelineState({
		phase: "ended",
		breakDurationSecs: breakSecs,
		fixDurationSecs: effectiveFixSecs,
		totalRounds,
		now: 0,
	});
	// Break Score 推导：全部防守成功总分（Fix 分 × 回合数）× 0.6。
	const deriveBreakScore = (f: FormState) => {
		const fixScore = Number(f.fixRoundScore) || 0;
		const turn = Number(f.fixRoundIntervalSecs) || 0;
		const fixDuration = Number(f.fixDurationSecs) || 0;
		const rounds = turn > 0 ? Math.floor(fixDuration / turn) : 0;
		return String(Math.round((fixScore * rounds * 3) / 5));
	};
	const phaseMeta = config ? PHASE_LABEL[config.phase] : null;

	const set =
		(key: keyof FormState) => (event: ChangeEvent<HTMLInputElement>) => {
			setDirty(true);
			if (key === "breakDurationSecs") {
				// Break 时长由“比赛总时长 - Fix 时长”推导，不允许直接修改。
				return;
			}
			if (key === "breakScore") {
				breakScoreTouched.current = true;
				setForm((current) => ({ ...current, breakScore: event.target.value }));
				return;
			}
			setForm((current) => {
				const next = { ...current, [key]: event.target.value };
				// 改 Fix/回合时，Break Score 按 ×0.6 规则自动重算（未手动覆盖时）。
				if (
					!breakScoreTouched.current &&
					(key === "fixDurationSecs" || key === "fixRoundIntervalSecs")
				) {
					next.breakScore = deriveBreakScore(next);
				}
				return next;
			});
		};
	const submit = () => {
		const breakScore = Number(form.breakScore);
		const fixRoundScore = Number(form.fixRoundScore);
		const values = [effectiveFixSecs, turnSecs, breakScore, fixRoundScore];
		if (values.some((value) => !Number.isSafeInteger(value) || value < 0)) {
			banner.showBanner(
				"critical",
				"All numeric fields must be non-negative integers.",
			);
			return;
		}
		if (invalid) {
			banner.showBanner(
				"critical",
				shortEvent
					? "赛事总时长过短：无法同时容纳 1 个完整回合与 Break 保底，请缩小回合时长。"
					: "Fix 至少需要 1 个完整回合，且 Break 不得低于保底时长。",
			);
			return;
		}
		save.mutate();
	};

	return (
		<div className="mt-3" style={{ maxWidth: 920 }}>
			<banner.BannerComponent className="mb-3" />
			<Box
				sx={{
					p: 4,
					border: "1px solid",
					borderColor: "border.default",
					borderRadius: 2,
				}}
			>
				<div className="d-flex flex-items-center flex-justify-between">
					<div>
						<h3 className="m-0">AWDP Configure</h3>
						<p className="color-fg-muted mb-0 mt-1">
							Break → Fix 双阶段：Fix 可配（取整为整数回合），Break = 总时长 − Fix
							（零头归 Break）。赛事开始前可改；进入 Break 后参数冻结。
						</p>
					</div>
					{phaseMeta && (
						<Label variant={phaseMeta.variant}>{phaseMeta.text}</Label>
					)}
				</div>

				{remoteChanged && (
					<Box sx={{ mt: 3, p: 3, bg: "attention.subtle", borderRadius: 2 }}>
						The server configuration changed while this form had unsaved edits.
						<Button
							className="ml-2"
							onClick={() => {
								loadedVersion.current = null;
								setDirty(false);
							}}
						>
							Reload server values
						</Button>
					</Box>
				)}

				{!editable && config && (
					<Box
						sx={{
							mt: 3,
							p: 3,
							bg: "attention.subtle",
							borderRadius: 2,
						}}
					>
						当前阶段为 <strong>{config.phase}</strong>，AWDP 参数已锁定。
					</Box>
				)}

				<Section title="Break & Fix">
					<NumberField
						label="Fix Duration"
						caption={
							`Fix 阶段时长（秒）。取整为整数回合：当前 ${formatDuration(effectiveFixSecs)}（${totalRounds} 回合 × ${formatDuration(turnSecs)}）。` +
							(fixHint ? ` ${fixHint}` : "")
						}
						value={form.fixDurationSecs}
						onChange={set("fixDurationSecs")}
						min={1}
						disabled={!editable}
						invalid={invalid}
					/>
					<BreakReadonly
						label="Break Duration"
						value={`${formatDuration(breakSecs)}（${breakSecs}s）`}
						hint={
							totalSecs !== null
								? `= 总时长 ${formatDuration(totalSecs)} − Fix · 零头全部归 Break（可非整数）`
								: "由“比赛总时长 − Fix 时长”推导，不可修改"
						}
						warn={breakBelowMin}
					/>
					<NumberField
						label="Turn Interval"
						caption={`每回合时长（秒），默认 10min。回合数 = Fix ÷ Turn（Fix 将取整为 Turn 的整数倍）。`}
						value={form.fixRoundIntervalSecs}
						onChange={set("fixRoundIntervalSecs")}
						min={1}
						disabled={!editable}
						invalid={shortEvent}
					/>
				</Section>

				<Section title="Scoring">
					<NumberField
						label="Break Score"
						caption={`Break 阶段一次性得分（每 GameBox）；默认 = Fix 满分 × 0.6（当前 ${deriveBreakScore(form)}）。改 Fix/回合时自动重算，手动改过则不再跟随。`}
						value={form.breakScore}
						onChange={set("breakScore")}
						min={0}
						disabled={!editable}
					/>
					<NumberField
						label="Fix Score / Turn"
						caption="Fix 阶段每回合 PATCHED 得分。"
						value={form.fixRoundScore}
						onChange={set("fixRoundScore")}
						min={0}
						disabled={!editable}
					/>
				</Section>

				<Section title="Timeline">
					{totalSecs !== null && totalSecs > 0 ? (
						<>
							<AwdpTimeline
								state={timelineState}
								phase="ended"
								totalRounds={totalRounds}
								showMarker={false}
							/>
							<div className="flex justify-between text-[11px] text-[var(--fgColor-muted)] tabular-nums mt-0.5">
								<span>Start {fmt(startIso)}</span>
								<span>Break 至 {fmt(breakEndIso)}</span>
								<span>End {fmt(fixEndIso)}</span>
							</div>
						</>
					) : (
						<p className="text-sm opacity-70">
							设置赛事开始/结束时间后展示时间轴预览。
						</p>
					)}
					<dl className="grid grid-cols-[8rem_1fr] gap-y-1 text-sm mt-2">
						<dt className="font-bold">Event Start</dt>
						<dd className="font-medium">{fmt(startIso)}</dd>
						<dt className="font-bold">Event End</dt>
						<dd className="font-medium">{fmt(fixEndIso)}</dd>
						<dt className="font-bold">Break 至</dt>
						<dd className="font-medium">{fmt(breakEndIso)}</dd>
					</dl>
					<p className="text-xs text-[var(--fgColor-muted)] mt-1">
						Total {formatDuration(totalSecs)} · Break {formatDuration(breakSecs)} ·
						Fix {formatDuration(effectiveFixSecs)}（{totalRounds} 回合）· Turn{" "}
						{formatDuration(turnSecs)}
					</p>
				</Section>

				<Box sx={{ mt: 4 }}>
					<Button
						variant="primary"
						disabled={
							!editable || !formReady || remoteChanged || save.isPending || invalid
						}
						onClick={submit}
					>
						{save.isPending ? "Saving…" : "Save AWDP Configuration"}
					</Button>
				</Box>
			</Box>
		</div>
	);
}

function Section({
	title,
	children,
}: { title: string; children: React.ReactNode }) {
	return (
		<section className="mt-4">
			<h4 className="mb-2">{title}</h4>
			<div
				style={{
					display: "grid",
					gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
					gap: 16,
				}}
			>
				{children}
			</div>
		</section>
	);
}

type NumberFieldProps = {
	label: string;
	caption: string;
	value: string;
	onChange: (event: ChangeEvent<HTMLInputElement>) => void;
	min: number;
	disabled: boolean;
	/** 输入非法（如赛事过短）时红色描边。 */
	invalid?: boolean;
};

function NumberField({
	label,
	caption,
	value,
	onChange,
	min,
	disabled,
	invalid,
}: NumberFieldProps) {
	return (
		<FormControl disabled={disabled}>
			<FormControl.Label>{label}</FormControl.Label>
			<FormControl.Caption>{caption}</FormControl.Caption>
			<TextInput
				type="number"
				value={value}
				onChange={onChange}
				min={min}
				step={1}
				required
				block
				className={invalid ? "border-red-600" : undefined}
			/>
		</FormControl>
	);
}

/** Break 只读展示行（非输入框形态，明确不可编辑；由总时长 − Fix 推导）。 */
function BreakReadonly({
	label,
	value,
	hint,
	warn,
}: {
	label: string;
	value: string;
	hint: string;
	warn: boolean;
}) {
	return (
		<div>
			<div className="text-sm font-semibold mb-1">{label}</div>
			<div
				className={`rounded border px-3 py-2 bg-[var(--bgColor-muted)] ${
					warn ? "border-red-600" : "border-[var(--borderColor-default)]"
				}`}
			>
				<span className="text-sm font-medium tabular-nums">{value}</span>
				<p className="text-xs text-[var(--fgColor-muted)] mt-0.5">
					{warn ? `⚠ ${hint}（当前低于保底 ${BREAK_MIN_SECS}s）` : hint}
				</p>
			</div>
		</div>
	);
}
