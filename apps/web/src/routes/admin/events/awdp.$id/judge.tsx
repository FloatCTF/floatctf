import {
	Box,
	Button,
	FormControl,
	Label,
	Spinner,
	TextInput,
	ToggleSwitch,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { type ChangeEvent, useEffect, useRef, useState } from "react";

import { type PracticeJudgeConfigDto, awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awdp/$id/judge")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

type FormState = {
	enabled: boolean;
	intervalSecs: string;
	flagPath: string;
	judgeServerUrl: string;
};

const STATUS_VARIANT: Record<
	string,
	"success" | "default" | "danger" | "attention"
> = {
	running: "success",
	stopped: "default",
	failed: "danger",
};

const STATUS_LABEL: Record<string, string> = {
	running: "running",
	stopped: "stopped",
	failed: "failed",
};

function fmt(iso: string | null | undefined) {
	if (!iso) return "-";
	return DatetimeToShow(iso);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const qc = useQueryClient();
	const banner = useMsgBanner({});
	const loadedVersionRef = useRef<string | null>(null);
	const [form, setForm] = useState<FormState>({
		enabled: false,
		intervalSecs: "60",
		flagPath: "/flag.php",
		judgeServerUrl: "",
	});
	const [dirty, setDirty] = useState(false);

	const configQuery = useQuery({
		queryKey: ["awdp-practice-judge", id],
		queryFn: () => awdpAdminApi.getPracticeJudge(id),
	});
	const config: PracticeJudgeConfigDto | null = configQuery.data?.data ?? null;

	const resultsQuery = useQuery({
		queryKey: ["awdp-practice-judge-results", id],
		queryFn: () => awdpAdminApi.listPracticeJudgeResults(id, { limit: 50 }),
		refetchInterval: 30_000,
	});

	// 配置加载 → 表单回填（仅首次 / 服务端变化时，不覆盖本地编辑）。
	useEffect(() => {
		if (!config) return;
		const version = `${config.updated_at}:${config.enabled}:${config.interval_secs}:${config.flag_path}:${config.judge_server_url}`;
		if (loadedVersionRef.current === version) return;
		if (dirty && loadedVersionRef.current !== null) return;
		setForm({
			enabled: config.enabled,
			intervalSecs: String(config.interval_secs),
			flagPath: config.flag_path,
			judgeServerUrl: config.judge_server_url,
		});
		setDirty(false);
		loadedVersionRef.current = version;
	}, [config, dirty]);

	const save = useMutation({
		mutationFn: () =>
			awdpAdminApi.updatePracticeJudge(id, {
				enabled: form.enabled,
				interval_secs: Number(form.intervalSecs),
				flag_path: form.flagPath,
				judge_server_url: form.judgeServerUrl,
			}),
		onSuccess: () => {
			setDirty(false);
			banner.showBanner("success", "Practice Judge configuration saved");
			qc.invalidateQueries({ queryKey: ["awdp-practice-judge", id] });
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			void configQuery.refetch();
		},
	});

	const deploy = useMutation({
		mutationFn: () => awdpAdminApi.deployPracticeJudge(id),
		onSuccess: () => {
			banner.showBanner("success", "Judge server deployed");
			qc.invalidateQueries({ queryKey: ["awdp-practice-judge", id] });
		},
		onError: (error) => banner.showErrorBanner(error),
	});

	const stop = useMutation({
		mutationFn: () => awdpAdminApi.stopPracticeJudge(id),
		onSuccess: () => {
			banner.showBanner("success", "Judge server stopped");
			qc.invalidateQueries({ queryKey: ["awdp-practice-judge", id] });
		},
		onError: (error) => banner.showErrorBanner(error),
	});

	const set =
		(key: keyof FormState) => (event: ChangeEvent<HTMLInputElement>) => {
			setDirty(true);
			setForm((current) => ({ ...current, [key]: event.target.value }));
		};

	const submit = () => {
		const interval = Number(form.intervalSecs);
		if (!Number.isSafeInteger(interval) || interval < 10 || interval > 86400) {
			banner.showBanner(
				"critical",
				"检查间隔必须是 10~86400 之间的整数（秒）。",
			);
			return;
		}
		if (!form.flagPath.startsWith("/")) {
			banner.showBanner(
				"critical",
				"Flag 端点路径必须以 / 开头（如 /flag.php）。",
			);
			return;
		}
		if (
			form.judgeServerUrl.trim() !== "" &&
			!/^https?:\/\//.test(form.judgeServerUrl.trim())
		) {
			banner.showBanner(
				"critical",
				"JudgeServer 地址必须是 http(s):// 开头或留空。",
			);
			return;
		}
		save.mutate();
	};

	if (configQuery.isLoading) return <Spinner size="large" />;
	if (configQuery.isError) {
		return <div>Failed to load practice judge configuration.</div>;
	}

	const status = config?.container_status ?? "stopped";
	const running = status === "running";
	const busy = deploy.isPending || stop.isPending || save.isPending;

	return (
		<div className="mt-3" style={{ maxWidth: 960 }}>
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
						<h3 className="m-0">AWDP Practice Judge</h3>
						<p className="color-fg-muted mb-0 mt-1">
							练习 GameBox 统一加入 data 子网{" "}
							<code>{config?.network_name ?? "-"}</code>；JudgeServer 是
							Pull + Lease worker：主动领取评估（manual Test Check / official
							Turn），执行 internal healthcheck → judge →（official）exploit。
						</p>
						<div className="mt-2 d-flex flex-items-center flex-wrap gap-2">
							<Label variant={config?.worker_health === "healthy" ? "success" : "attention"}>
								worker: {config?.worker_health ?? "-"}
							</Label>
							<Label variant="default">pending: {config?.pending_evaluations ?? 0}</Label>
							<Label variant="default">running: {config?.running_evaluations ?? 0}</Label>
							<Label variant="default">
								last heartbeat: {fmt(config?.last_heartbeat)}
							</Label>
							<span className="color-fg-muted text-small">
								data endpoint: <code>{config?.data_endpoint ?? "-"}</code>
								（仅 GameBox 内部网络可达）
							</span>
						</div>
					</div>
					<Label variant={STATUS_VARIANT[status] ?? "default"}>
						{STATUS_LABEL[status] ?? status}
					</Label>
				</div>

				<Section title="例行检查配置">
					<Box
						sx={{
							display: "flex",
							alignItems: "center",
							justifyContent: "space-between",
							border: "1px solid",
							borderColor: "border.default",
							borderRadius: 2,
							p: 3,
							mb: 3,
						}}
					>
						<div>
							<div className="font-bold">启用练习 Judge</div>
							<div className="text-sm color-fg-muted">
								开启后，sweep worker 每间隔对全部运行中的练习实例派发检查。
							</div>
						</div>
						<ToggleSwitch
							aria-labelledby="practice-judge-enabled-label"
							checked={form.enabled}
							onClick={() => {
								setDirty(true);
								setForm((c) => ({ ...c, enabled: !c.enabled }));
							}}
						/>
					</Box>
					<NumberField
						label="检查间隔（秒）"
						caption="例行检查间隔，默认 60。保存后由 sweep worker 按间隔执行。"
						value={form.intervalSecs}
						onChange={set("intervalSecs")}
					/>
					<TextField
						label="Flag 端点路径"
						caption="flag curl 验证的端点路径（GameBox 按 FLAG env 返回 flag），默认 /flag.php。"
						value={form.flagPath}
						onChange={set("flagPath")}
					/>
					<TextField
						label="JudgeServer 地址（可选）"
						caption="留空自动推导为练习子网内固定 IP（如 http://10.42.2.2:8082）。部署在其他位置时手动填写。"
						value={form.judgeServerUrl}
						onChange={set("judgeServerUrl")}
					/>
					<Box sx={{ mt: 3 }}>
						<Button
							variant="primary"
							disabled={!dirty || busy}
							onClick={submit}
						>
							{save.isPending ? "Saving…" : "Save Judge Configuration"}
						</Button>
					</Box>
				</Section>

				<Section title="JudgeServer">
					<dl className="grid grid-cols-[10rem_1fr] gap-y-1 text-sm">
						<dt className="font-bold">部署地址</dt>
						<dd className="font-medium">
							<code>{config?.resolved_judge_server_url ?? "-"}</code>
						</dd>
						<dt className="font-bold">容器</dt>
						<dd className="font-medium">{config?.container_id ?? "-"}</dd>
						<dt className="font-bold">最近检查</dt>
						<dd className="font-medium">{fmt(config?.last_sweep_at)}</dd>
					</dl>
					<Box sx={{ mt: 3, display: "flex", gap: 2 }}>
						<Button
							variant="primary"
							disabled={running || busy}
							onClick={() => deploy.mutate()}
						>
							{deploy.isPending ? "Deploying…" : "Deploy Judge Server"}
						</Button>
						<Button
							variant="danger"
							disabled={!running || busy}
							onClick={() => stop.mutate()}
						>
							{stop.isPending ? "Stopping…" : "Stop Judge Server"}
						</Button>
					</Box>
					{config?.enabled && !running && (
						<Box
							sx={{
								mt: 3,
								p: 3,
								bg: "attention.subtle",
								borderRadius: 2,
							}}
						>
							已启用练习 Judge 但 JudgeServer 未运行——例行检查不会派发。
							请先部署 JudgeServer。
						</Box>
					)}
				</Section>
			</Box>

			<Box
				sx={{
					mt: 3,
					p: 4,
					border: "1px solid",
					borderColor: "border.default",
					borderRadius: 2,
				}}
			>
				<h3 className="m-0">最近检查结果</h3>
				<p className="color-fg-muted mb-3 mt-1">
					最近 {resultsQuery.data?.data?.length ?? 0} 条 exploit / flag
					检查结果（30s 自动刷新）。
				</p>
				{resultsQuery.isLoading ? (
					<Spinner size="medium" />
				) : !resultsQuery.data?.data?.length ? (
					<p className="text-sm color-fg-muted mb-0">
						暂无检查结果——启用练习 Judge 并部署 JudgeServer
						后，运行中的练习实例会产生检查记录。
					</p>
				) : (
					<table
						className="width-full"
						style={{
							borderCollapse: "collapse",
							fontSize: 13,
						}}
					>
						<thead>
							<tr
								style={{ borderBottom: "1px solid var(--borderColor-default)" }}
							>
								<th className="text-left p-2">GameBox</th>
								<th className="text-left p-2">Owner</th>
								<th className="text-left p-2">Kind</th>
								<th className="text-left p-2">Status</th>
								<th className="text-left p-2">Detail</th>
								<th className="text-left p-2">Time</th>
							</tr>
						</thead>
						<tbody>
							{resultsQuery.data.data.map((row) => (
								<tr
									key={row.id}
									style={{
										borderBottom: "1px solid var(--borderColor-muted)",
									}}
								>
									<td className="p-2">{row.gamebox_name}</td>
									<td className="p-2">
										<span className="color-fg-muted">
											{(row.owner_user_id ?? row.owner_team_id ?? "-").slice(
												0,
												8,
											)}
										</span>
									</td>
									<td className="p-2">
										<Label
											variant={
												row.check_kind === "exploit" ? "accent" : "attention"
											}
										>
											{row.check_kind}
										</Label>
									</td>
									<td className="p-2">
										<Label
											variant={
												row.status === "success"
													? "success"
													: row.status === "failure"
														? "attention"
														: "danger"
											}
										>
											{row.status}
										</Label>
									</td>
									<td className="p-2" style={{ maxWidth: 420 }}>
										<span
											className="color-fg-muted"
											title={row.detail ?? ""}
											style={{
												display: "block",
												overflow: "hidden",
												textOverflow: "ellipsis",
												whiteSpace: "nowrap",
											}}
										>
											{row.detail ?? "-"}
										</span>
									</td>
									<td className="p-2">{fmt(row.created_at)}</td>
								</tr>
							))}
						</tbody>
					</table>
				)}
			</Box>
		</div>
	);
}

// 服务端版本跟踪（避免每次 refetch 覆盖本地编辑）在组件内 useRef 维护。

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

function NumberField({
	label,
	caption,
	value,
	onChange,
}: {
	label: string;
	caption: string;
	value: string;
	onChange: (event: ChangeEvent<HTMLInputElement>) => void;
}) {
	return (
		<FormControl>
			<FormControl.Label>{label}</FormControl.Label>
			<FormControl.Caption>{caption}</FormControl.Caption>
			<TextInput
				type="number"
				value={value}
				onChange={onChange}
				min={10}
				step={1}
				block
			/>
		</FormControl>
	);
}

function TextField({
	label,
	caption,
	value,
	onChange,
}: {
	label: string;
	caption: string;
	value: string;
	onChange: (event: ChangeEvent<HTMLInputElement>) => void;
}) {
	return (
		<FormControl>
			<FormControl.Label>{label}</FormControl.Label>
			<FormControl.Caption>{caption}</FormControl.Caption>
			<TextInput value={value} onChange={onChange} block />
		</FormControl>
	);
}
