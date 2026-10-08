/**
 * 赛事控制台 · 日志 / Writeup 标签。
 *
 * - 日志：`event_logs.fetch(eventId)({page,limit})`（柯里化）
 * - Writeup：`event_writeups.fetch(eventId)({page,limit})`（柯里化）+ `events.getReport(eventId)`
 *   取 S3 对象键，再用 `download.download(key)` 触发浏览器下载
 *   （`download` 返回 void，失败抛普通 `Error`，这里用 toast 显示真实原因）。
 */

import { useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Download, FileText, RefreshCw } from "lucide-react";

import { callList, callMaybe } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill, type PillTone } from "~/components/app/badges";
import type { DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import type { EventLogs, EventWriteup } from "@floatctf/sdk/entity";
import { formatDateTime } from "~/lib/format";

import { MobileFacts, PagedTable } from "./shared";

function levelTone(level: string): PillTone {
	switch (level.trim().toLowerCase()) {
		case "info":
			return "info";
		case "warn":
		case "warning":
			return "warning";
		case "error":
		case "critical":
			return "danger";
		case "debug":
			return "muted";
		default:
			return "neutral";
	}
}

/* ── 日志 ───────────────────────────────────────────────────────────────── */

export function EventLogsTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);

	const query = useQuery({
		queryKey: qk.admin.eventLogs(eventId, { page, limit: pageSize }),
		queryFn: () =>
			callList<EventLogs>(client.admin.event_logs.fetch(eventId)({ page, limit: pageSize })),
	});

	const columns: DataTableColumn<EventLogs>[] = [
		{
			id: "level",
			header: "级别",
			cell: (row) => <TonePill tone={levelTone(row.level)}>{row.level}</TonePill>,
			sortValue: (row) => row.level,
		},
		{
			id: "action",
			header: "操作",
			cell: (row) => <MonoText>{row.action}</MonoText>,
			sortValue: (row) => row.action,
		},
		{
			id: "details",
			header: "详情",
			cell: (row) => (
				<span className="line-clamp-2 max-w-md text-xs break-all">{row.details}</span>
			),
		},
		{
			id: "user_id",
			header: "用户 / 战队",
			cell: (row) =>
				row.user_id || row.team_id ? (
					<span className="flex flex-col gap-0.5">
						{row.user_id ? <MonoText>user {row.user_id}</MonoText> : null}
						{row.team_id ? <MonoText>team {row.team_id}</MonoText> : null}
					</span>
				) : (
					<span className="text-xs">—</span>
				),
			hideBelow: "lg",
		},
		{
			id: "ip_address",
			header: "IP",
			cell: (row) => (row.ip_address ? <MonoText>{row.ip_address}</MonoText> : <span className="text-xs">—</span>),
			sortValue: (row) => row.ip_address ?? "",
			hideBelow: "xl",
		},
		{
			id: "created_at",
			header: "时间",
			cell: (row) => <MonoText>{formatDateTime(row.created_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.created_at,
		},
	];

	const mobileCard = (row: EventLogs): ReactNode => (
		<MobileFacts
			items={[
				{ k: "级别", v: <TonePill tone={levelTone(row.level)}>{row.level}</TonePill> },
				{ k: "操作", v: <MonoText>{row.action}</MonoText> },
				{ k: "详情", v: <span className="text-xs break-all">{row.details}</span> },
				{ k: "时间", v: <MonoText>{formatDateTime(row.created_at)}</MonoText> },
			]}
		/>
	);

	return (
		<SectionCard
			title="赛事操作日志"
			description="平台在该赛事内记录的操作流水（管理端与选手端行为）。"
			actions={
				<Toolbar>
					<Button
						variant="outline"
						size="sm"
						disabled={query.isFetching}
						onClick={() => void query.refetch()}
					>
						<RefreshCw className={query.isFetching ? "animate-spin" : undefined} /> 刷新
					</Button>
				</Toolbar>
			}
		>
			<PagedTable
				query={query}
				columns={columns}
				getRowId={(row) => row.id}
				page={page}
				pageSize={pageSize}
				onPageChange={setPage}
				onPageSizeChange={(size) => {
					setPageSize(size);
					setPage(1);
				}}
				mobileCard={mobileCard}
				errorTitle="加载赛事日志失败"
				empty={<EmptyBlock title="暂无日志" description="该赛事还没有产生操作记录。" />}
			/>
		</SectionCard>
	);
}

/* ── Writeup ────────────────────────────────────────────────────────────── */

export function EventWriteupsTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);

	const query = useQuery({
		queryKey: qk.admin.eventWriteups(eventId, { page, limit: pageSize }),
		queryFn: () =>
			callList<EventWriteup>(
				client.admin.event_writeups.fetch(eventId)({ page, limit: pageSize }),
			),
	});

	const download = useMutation({
		mutationFn: (key: string) => client.admin.download.download(key),
		onError: (error) => toast.apiError("下载失败", error),
	});

	const exportAll = useMutation({
		mutationFn: async () => {
			// 1) 取 S3 对象键（后端 HTML/zip 报告）
			const key = await callMaybe<string>(client.admin.events.getReport(eventId), "生成 Writeup 报告");
			if (!key) throw new Error("后端未返回报告对象键（可能该赛事还没有 Writeup）");
			// 2) 预签名 URL + 触发浏览器下载
			await client.admin.download.download(key);
		},
		onSuccess: () => toast.success("已开始下载 Writeup 报告"),
		onError: (error) => toast.apiError("导出 Writeup 失败", error),
	});

	const columns: DataTableColumn<EventWriteup>[] = [
		{
			id: "user_id",
			header: "用户 ID",
			cell: (row) => <MonoText>{row.user_id}</MonoText>,
			sortValue: (row) => row.user_id,
		},
		{
			id: "team_id",
			header: "战队 ID",
			cell: (row) => (row.team_id ? <MonoText>{row.team_id}</MonoText> : <span className="text-xs">—</span>),
			sortValue: (row) => row.team_id ?? "",
			hideBelow: "md",
		},
		{
			id: "file",
			header: "文件",
			cell: (row) => {
				const name = row.file_url.split("/").pop() || row.file_url;
				return (
					<Button
						variant="link"
						size="xs"
						disabled={download.isPending}
						onClick={() => download.mutate(row.file_url)}
						aria-label={`下载 ${name}`}
					>
						<Download /> {name}
					</Button>
				);
			},
		},
		{
			id: "created_at",
			header: "提交时间",
			cell: (row) => <MonoText>{formatDateTime(row.created_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.created_at,
		},
	];

	const mobileCard = (row: EventWriteup): ReactNode => (
		<MobileFacts
			items={[
				{ k: "用户", v: <MonoText>{row.user_id}</MonoText> },
				{ k: "战队", v: row.team_id ? <MonoText>{row.team_id}</MonoText> : "—" },
				{ k: "文件", v: <MonoText>{row.file_url.split("/").pop() || row.file_url}</MonoText> },
				{ k: "提交时间", v: <MonoText>{formatDateTime(row.created_at)}</MonoText> },
			]}
		/>
	);

	return (
		<SectionCard
			title="赛事 Writeup"
			description="选手 / 战队提交的 Writeup 文件；「导出」会请求后端生成整场赛事的报告压缩包。"
			actions={
				<Toolbar>
					<Button
						size="sm"
						disabled={exportAll.isPending}
						onClick={() => exportAll.mutate()}
					>
						<Download /> 导出全部
					</Button>
				</Toolbar>
			}
		>
			<PagedTable
				query={query}
				columns={columns}
				getRowId={(row) => `${row.event_id}-${row.user_id}-${row.team_id ?? ""}`}
				page={page}
				pageSize={pageSize}
				onPageChange={setPage}
				onPageSizeChange={(size) => {
					setPageSize(size);
					setPage(1);
				}}
				mobileCard={mobileCard}
				errorTitle="加载 Writeup 失败"
				empty={
					<EmptyBlock
						title="暂无 Writeup"
						description="比赛结束后选手 / 战队提交的 Writeup 会出现在这里。"
						icon={<FileText className="size-5" />}
					/>
				}
			/>
		</SectionCard>
	);
}
