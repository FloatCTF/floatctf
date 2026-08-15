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
import { AdminRouteGuard } from "../../route";
import { EventContext } from "./route";

export const Route = createFileRoute("/admin/events/awdp/$id/configure")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

type FormState = {
	breakDurationSecs: string;
	fixDurationSecs: string;
	fixRoundIntervalSecs: string;
	breakScore: string;
	fixRoundScore: string;
};

const DEFAULT_FORM: FormState = {
	breakDurationSecs: "3600",
	fixDurationSecs: "3600",
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
		const fixDurationSecs =
			total !== null
				? Math.max(0, total - config.break_duration_secs)
				: config.fix_duration_secs;
		setForm({
			breakDurationSecs: String(config.break_duration_secs),
			fixDurationSecs: String(fixDurationSecs),
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
				break_duration_secs: Number(form.breakDurationSecs),
				fix_round_interval_secs: Number(form.fixRoundIntervalSecs),
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
	// 比赛总时长 = event.end_time - event.start_time；Fix 时长 = 总时长 - Break 时长。
	const totalSecs = (() => {
		if (!event?.start_time || !event?.end_time) return null;
		const diff =
			new Date(event.end_time).getTime() - new Date(event.start_time).getTime();
		return Math.floor(diff / 1000);
	})();
	const breakSecs = Number(form.breakDurationSecs) || 0;
	const fixSecs =
		totalSecs !== null
			? Math.max(0, totalSecs - breakSecs)
			: Number(form.fixDurationSecs) || 0;
	const turnSecs = Number(form.fixRoundIntervalSecs) || 0;
	const totalRounds = Math.floor(fixSecs / (turnSecs || 1));
	// Timeline 预览：根据当前表单输入实时计算（Fix 时长由比赛总时长 - Break 推导）。
	const startIso = event?.start_time ?? config?.started_at ?? null;
	const startMs = startIso ? new Date(startIso).getTime() : null;
	const breakEndIso =
		startMs !== null
			? new Date(startMs + breakSecs * 1000).toISOString()
			: null;
	const fixStartIso = breakEndIso;
	const fixEndIso =
		startMs !== null
			? new Date(startMs + (breakSecs + fixSecs) * 1000).toISOString()
			: null;
	// Break Score 推导：全部防守成功总分（Fix 分 × 回合数）× 0.6。
	const deriveBreakScore = (f: FormState) => {
		const fixScore = Number(f.fixRoundScore) || 0;
		const brk = Number(f.breakDurationSecs) || 0;
		const fixDuration =
			totalSecs !== null
				? Math.max(0, totalSecs - brk)
				: Number(f.fixDurationSecs) || 0;
		const rounds = Math.floor(
			fixDuration / (Number(f.fixRoundIntervalSecs) || 1),
		);
		return String(Math.round((fixScore * rounds * 3) / 5));
	};
	const phaseMeta = config ? PHASE_LABEL[config.phase] : null;

	const set =
		(key: keyof FormState) => (event: ChangeEvent<HTMLInputElement>) => {
			setDirty(true);
			if (key === "fixDurationSecs") {
				// Fix 时长由“比赛总时长 - Break 时长”推导，不允许直接修改。
				return;
			}
			if (key === "breakDurationSecs") {
				setForm((current) => {
					const next = {
						...current,
						breakDurationSecs: event.target.value,
					};
					if (totalSecs !== null) {
						next.fixDurationSecs = String(
							Math.max(0, totalSecs - Number(next.breakDurationSecs)),
						);
					}
					if (!breakScoreTouched.current) {
						next.breakScore = deriveBreakScore(next);
					}
					return next;
				});
				return;
			}
			if (key === "breakScore") {
				breakScoreTouched.current = true;
				setForm((current) => ({ ...current, breakScore: event.target.value }));
				return;
			}
			setForm((current) => {
				const next = { ...current, [key]: event.target.value };
				// 改回合/单轮分时，Break Score 按 ×0.6 规则自动重算（未手动覆盖时）。
				if (
					!breakScoreTouched.current &&
					(key === "fixRoundIntervalSecs" || key === "fixRoundScore")
				) {
					next.breakScore = deriveBreakScore(next);
				}
				return next;
			});
		};
	const submit = () => {
		const breakDurationSecs = Number(form.breakDurationSecs) || 0;
		const fixDurationSecs = fixSecs;
		const fixRoundIntervalSecs = Number(form.fixRoundIntervalSecs) || 0;
		const breakScore = Number(form.breakScore);
		const fixRoundScore = Number(form.fixRoundScore);
		const values = [
			breakDurationSecs,
			fixDurationSecs,
			fixRoundIntervalSecs,
			breakScore,
			fixRoundScore,
		];
		if (values.some((value) => !Number.isSafeInteger(value) || value < 0)) {
			banner.showBanner(
				"critical",
				"All numeric fields must be non-negative integers.",
			);
			return;
		}
		if (breakDurationSecs <= 0 || fixDurationSecs <= 0) {
			banner.showBanner("critical", "Break/Fix duration must be positive.");
			return;
		}
		if (
			fixRoundIntervalSecs <= 0 ||
			fixDurationSecs % fixRoundIntervalSecs !== 0
		) {
			banner.showBanner(
				"critical",
				"Turn interval must be a positive divisor of Fix duration.",
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
							Break → Fix 双阶段（默认 1h + 1h / 每回合
							10min）。赛事开始前可改； 进入 Break 后参数冻结。总回合数 = Fix
							时长 ÷ 回合时长。
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
						label="Break Duration"
						caption="Break 阶段时长（秒），默认 3600。"
						value={form.breakDurationSecs}
						onChange={set("breakDurationSecs")}
						min={1}
						disabled={!editable}
					/>
					<NumberField
						label="Fix Duration"
						caption="Fix 阶段时长（秒），由比赛总时长 - Break 时长自动推导，不可直接修改。"
						value={form.fixDurationSecs}
						onChange={set("fixDurationSecs")}
						min={1}
						disabled
					/>
					<NumberField
						label="Turn Interval"
						caption="每回合时长（秒），默认 600，需整除 Fix 时长。"
						value={form.fixRoundIntervalSecs}
						onChange={set("fixRoundIntervalSecs")}
						min={1}
						disabled={!editable}
					/>
				</Section>

				<Section title="Scoring">
					<NumberField
						label="Break Score"
						caption={`Break 阶段一次性得分（每 GameBox）；默认 = Fix 满分 × 0.6（当前 ${deriveBreakScore(form)}）。改回合/单轮分时自动重算，手动改过则不再跟随。`}
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
					<dl className="grid grid-cols-[8rem_1fr] gap-y-1 text-sm">
						<dt className="font-bold">Event Start</dt>
						<dd className="font-medium">{fmt(startIso)}</dd>
						<dt className="font-bold">Break 至</dt>
						<dd className="font-medium">{fmt(breakEndIso)}</dd>
						<dt className="font-bold">Fix 开始</dt>
						<dd className="font-medium">{fmt(fixStartIso)}</dd>
						<dt className="font-bold">Fix 至</dt>
						<dd className="font-medium">{fmt(fixEndIso)}</dd>
						<dt className="font-bold">Total</dt>
						<dd className="font-medium">
							{totalSecs !== null ? `${totalSecs}s` : "-"}
						</dd>
						<dt className="font-bold">Break</dt>
						<dd className="font-medium">{breakSecs}s</dd>
						<dt className="font-bold">Fix</dt>
						<dd className="font-medium">{fixSecs}s（总时长 - Break）</dd>
						<dt className="font-bold">Turn</dt>
						<dd className="font-medium">{turnSecs}s</dd>
						<dt className="font-bold">Rounds</dt>
						<dd className="font-medium">{totalRounds}</dd>
					</dl>
				</Section>

				<Box sx={{ mt: 4 }}>
					<Button
						variant="primary"
						disabled={
							!editable || !formReady || remoteChanged || save.isPending
						}
						onClick={submit}
					>
						{save.isPending ? "Saving…" : "Save AWDP Configuration"}
					</Button>
					<span className="ml-3 text-sm color-fg-muted">
						总回合数：{totalRounds}
					</span>
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
};

function NumberField({
	label,
	caption,
	value,
	onChange,
	min,
	disabled,
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
			/>
		</FormControl>
	);
}
