/**
 * 「预检与轮换」标签：手动预检、预检历史与失败原因、内部令牌轮换。
 *
 * 已核实的后端语义：
 * - 预检要求赛事网络已分配，且状态处于运行前的配置态（含 prechecking）；
 * - `POST .../awd/precheck` 实际返回 `{ run_id, status, error_msg, failed_checks, checks }`
 *   对象（SDK 把它标注成 `string`，见交付报告 PUBLIC SDK GAP），这里按对象解析并做兜底；
 * - `AwdPrecheckRun.error_msg` 是 JSON 字符串，需 `JSON.parse` 后展示 errors / notes。
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, ShieldCheck, ShieldX } from "lucide-react";

import type { AwdPrecheckRun } from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { errorText } from "~/api/errors";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { KeyValueList, MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { formatDateTime } from "~/lib/format";

import { useAwdEventNetwork, useAwdStatus } from "./queries";
import {
	AWD_ACTION_CONFIRM,
	buildAwdGates,
	parsePrecheckReport,
	PRECHECK_STATUS_LABEL,
	precheckStatusTone,
	runAwdAction,
} from "./shared";

/** 后端实际返回的预检结果（比 SDK 声明的 `string` 更宽，因此这里显式声明并用兜底解析）。 */
interface AwdPrecheckResult {
	run_id?: string;
	status?: string;
	error_msg?: string | null;
	failed_checks?: string[];
}

export function AwdPrecheckTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const statusQuery = useAwdStatus(eventId);
	const networkQuery = useAwdEventNetwork(eventId);
	const [selectedRunId, setSelectedRunId] = useState<string | null>(null);

	const runsQuery = useQuery({
		queryKey: qk.awd.prechecks(eventId),
		queryFn: () => call<AwdPrecheckRun[]>(client.awd.admin.prechecks(eventId), "预检历史"),
		enabled: eventId !== "",
	});

	const status = statusQuery.data ?? null;
	const gates = buildAwdGates({
		status: status?.status ?? null,
		networkAllocated: networkQuery.data !== null && networkQuery.data !== undefined,
		finalSettlement: status?.final_settlement ?? false,
	});

	const invalidate = () => {
		void queryClient.invalidateQueries({ queryKey: qk.awd.prechecks(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.eventNetwork(eventId) });
	};

	const precheck = useMutation({
		mutationFn: () => call<AwdPrecheckResult>(client.awd.admin.precheck(eventId), "预检"),
		onSuccess: (result) => {
			const status = result?.status ?? "";
			const failed = result?.failed_checks ?? [];
			if (status === "passed") {
				toast.success("预检通过", "赛事状态已变为「预检通过」，可以开赛。");
			} else {
				const detail =
					failed.length > 0
						? `未通过项：${failed.join("、")}`
						: result?.error_msg
							? "后端已记录失败原因，见下方最新预检报告。"
							: "后端未返回失败明细，见下方预检历史。";
				toast.error("预检未通过", detail);
			}
			invalidate();
		},
		onError: (error) => toast.apiError("预检失败", error),
	});

	const rotate = useMutation({
		mutationFn: () => runAwdAction(client, eventId, "rotateTokens"),
		onSuccess: () => {
			toast.success("内部令牌已轮换", "FlagServer / JudgeServer 容器已按新令牌重建。");
			invalidate();
		},
		onError: (error) => toast.apiError("轮换内部令牌失败", error),
	});

	const runs = runsQuery.data ?? [];
	const selected =
		runs.find((run) => run.id === selectedRunId) ?? (runs.length > 0 ? runs[0] : null);
	const report = parsePrecheckReport(selected?.error_msg);

	const columns: DataTableColumn<AwdPrecheckRun>[] = [
		{
			id: "status",
			header: "结果",
			sortValue: (row) => row.status,
			cell: (row) => (
				<TonePill tone={precheckStatusTone(row.status)}>
					{PRECHECK_STATUS_LABEL[row.status] ?? row.status}
				</TonePill>
			),
		},
		{
			id: "trigger",
			header: "触发方式",
			cell: (row) => row.trigger ?? "—",
			hideBelow: "sm",
		},
		{
			id: "revision",
			header: "网络修订",
			align: "right",
			cell: (row) => (row.revision === null || row.revision === undefined ? "—" : row.revision),
			hideBelow: "md",
		},
		{
			id: "started_at",
			header: "开始",
			cell: (row) => formatDateTime(row.started_at, { seconds: true }),
			sortValue: (row) => row.started_at ?? "",
		},
		{
			id: "completed_at",
			header: "完成",
			cell: (row) => formatDateTime(row.completed_at, { seconds: true }),
			hideBelow: "md",
		},
		{
			id: "errors",
			header: "失败项",
			align: "right",
			cell: (row) => {
				const parsed = parsePrecheckReport(row.error_msg);
				if (parsed.errors.length === 0) return <span className="text-muted-foreground">—</span>;
				return <TonePill tone="danger">{parsed.errors.length}</TonePill>;
			},
		},
	];

	return (
		<div className="space-y-5">
			<SectionCard
				title="预检"
				description="检查配置 / 容器 / WireGuard / 网络 / flag / 判题链路；通过后状态变为「预检通过」。"
				actions={
					<Button
						size="sm"
						disabled={!gates.precheck.enabled || precheck.isPending}
						title={gates.precheck.reason ?? "运行一次预检"}
						onClick={async () => {
							const ok = await confirm(AWD_ACTION_CONFIRM.precheck);
							if (ok) precheck.mutate();
						}}
					>
						<ShieldCheck /> 运行预检
					</Button>
				}
			>
				<div className="space-y-3">
					<div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
						<TonePill tone={gates.precheck.enabled ? "success" : "warning"}>
							{gates.precheck.enabled ? "可以预检" : "当前不可预检"}
						</TonePill>
						{gates.precheck.reason ? <span>{gates.precheck.reason}</span> : null}
					</div>
					<KeyValueList
						columns={2}
						items={[
							{
								key: "赛事网络",
								value: networkQuery.isPending ? (
									<span className="text-muted-foreground">读取中…</span>
								) : networkQuery.isError ? (
									<span className="text-destructive">读取失败</span>
								) : networkQuery.data == null ? (
									<span className="text-destructive">未分配（预检前置条件）</span>
								) : (
									<TonePill tone="success">已分配</TonePill>
								),
								hint: networkQuery.isError ? errorText(networkQuery.error) : undefined,
							},
							{
								key: "预检通过时间",
								value: status?.verified_at ? (
									formatDateTime(status.verified_at, { seconds: true })
								) : (
									<span className="text-muted-foreground">未通过</span>
								),
							},
						]}
					/>
				</div>
			</SectionCard>

			<SectionCard
				title="预检历史"
				description="按开始时间倒序；点击一行查看该次预检的失败原因与提示。"
				actions={
					<Button
						variant="ghost"
						size="sm"
						disabled={runsQuery.isFetching}
						onClick={() => void runsQuery.refetch()}
					>
						<RefreshCw /> 刷新
					</Button>
				}
			>
				<QueryState
					query={runsQuery}
					skeleton={<TableSkeleton rows={4} columns={5} />}
					isEmpty={(data) => data.length === 0}
					empty={
						<EmptyBlock
							title="还没有预检记录"
							description="运行一次预检后，这里会显示每次预检的结果与失败原因。"
						/>
					}
				>
					{(data) => (
						<div className="space-y-4">
							<DataTable
								data={data}
								getRowId={(row) => row.id}
								columns={columns}
								onRowClick={(row) => setSelectedRunId(row.id)}
								mobileCard={(row) => (
									<div className="space-y-1">
										<TonePill tone={precheckStatusTone(row.status)}>
											{PRECHECK_STATUS_LABEL[row.status] ?? row.status}
										</TonePill>
										<p className="text-xs text-muted-foreground">
											{formatDateTime(row.started_at, { seconds: true })} · {row.trigger ?? "—"}
										</p>
									</div>
								)}
							/>
							{selected ? (
								<div className="space-y-3 rounded-md border p-3">
									<div className="flex flex-wrap items-center gap-2">
										<TonePill tone={precheckStatusTone(selected.status)}>
											{PRECHECK_STATUS_LABEL[selected.status] ?? selected.status}
										</TonePill>
										<span className="text-xs text-muted-foreground">
											{formatDateTime(selected.completed_at ?? selected.started_at, {
												seconds: true,
											})}
											{selected.revision !== null && selected.revision !== undefined
												? ` · 网络修订 ${selected.revision}`
												: ""}
										</span>
										<MonoText>{selected.id}</MonoText>
									</div>

									{report.errors.length > 0 ? (
										<ul className="space-y-1">
											{report.errors.map((entry, index) => (
												<li
													key={`${entry.component}-${index}`}
													className="flex items-start gap-2 text-sm"
												>
													<ShieldX className="mt-0.5 size-4 shrink-0 text-destructive" />
													<span>
														<span className="font-mono text-xs">{entry.component}</span>
														<span className="text-muted-foreground"> · </span>
														{entry.error ?? "（后端未提供原因）"}
													</span>
												</li>
											))}
										</ul>
									) : (
										<p className="text-sm text-muted-foreground">该次预检没有失败项。</p>
									)}

									{report.notes.length > 0 ? (
										<details className="text-sm">
											<summary className="cursor-pointer text-muted-foreground">
												提示信息（{report.notes.length}）
											</summary>
											<ul className="mt-2 space-y-1">
												{report.notes.map((entry, index) => (
													<li key={`${entry.component}-${index}`}>
														<span className="font-mono text-xs">{entry.component}</span>
														<span className="text-muted-foreground"> · </span>
														{entry.note}
													</li>
												))}
											</ul>
										</details>
									) : null}
								</div>
							) : null}
						</div>
					)}
				</QueryState>
			</SectionCard>

			<SectionCard
				title="内部令牌轮换"
				description="key_version +1 并替换 FlagServer / JudgeServer 的内部调用令牌（会原地重建这两个容器）。"
				actions={
					<Button
						variant="destructive"
						size="sm"
						disabled={!gates.rotateTokens.enabled || rotate.isPending}
						title={gates.rotateTokens.reason ?? "轮换内部令牌"}
						onClick={async () => {
							const ok = await confirm(AWD_ACTION_CONFIRM.rotateTokens);
							if (ok) rotate.mutate();
						}}
					>
						轮换内部令牌
					</Button>
				}
			>
				<div className="space-y-2 text-sm text-muted-foreground">
					<p>轮换只影响平台内部调用令牌，不改变题目 flag 本身（赛事密钥未变）。</p>
					<p>重建期间判题与 flag 结算短暂不可用；失败可重试，数据库令牌更新是原子的。</p>
					{gates.rotateTokens.reason ? (
						<p className="text-xs text-destructive">{gates.rotateTokens.reason}</p>
					) : null}
				</div>
			</SectionCard>
		</div>
	);
}
