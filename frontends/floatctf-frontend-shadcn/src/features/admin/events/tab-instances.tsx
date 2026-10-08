/**
 * 赛事控制台 · 实例标签。
 *
 * `client.admin.instances.listForEvent(eventId)` 返回**归一化**的 `AdminInstanceRow[]`
 * （challenge / gamebox 类型、对应 content_title）—— 该接口不支持 `page`/`limit`，
 * 一次返回全量，因此这里客户端分页。
 *
 * ⚠️ 管理端实例列表**不返回 flag**（后端脱敏），页面也不展示任何 flag 字段。
 */

import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Boxes } from "lucide-react";

import { callMaybe } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import type { DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock } from "~/components/app/states";
import { Button } from "~/components/ui/button";
import type { AdminInstanceRow } from "@floatctf/sdk";
import { formatDateTime } from "~/lib/format";

import { clientPageCount, clientPaged, MobileFacts, PagedTable } from "./shared";

function typeTone(type: AdminInstanceRow["instance_type"]): "info" | "success" {
	return type === "challenge" ? "info" : "success";
}

function ownerLabel(row: AdminInstanceRow): ReactNode {
	if (row.user_name) return <span>{row.user_name}</span>;
	if (row.team_name) return <span>{row.team_name}（战队）</span>;
	const fallback = row.user_id ?? row.team_id;
	return fallback ? <MonoText>{fallback}</MonoText> : <span className="text-xs">—</span>;
}

export function EventInstancesTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);

	const query = useQuery({
		queryKey: qk.admin.eventInstances(eventId),
		queryFn: async () => {
			const rows = (await callMaybe<AdminInstanceRow[]>(
				client.admin.instances.listForEvent(eventId),
				"赛事实例",
			)) ?? [];
			return { items: rows, meta: { total: rows.length } };
		},
	});

	const total = query.data?.meta?.total ?? query.data?.items.length ?? 0;
	const safePage = Math.min(page, clientPageCount(total, pageSize));
	const pagedQuery = clientPaged(query, safePage, pageSize);

	const columns: DataTableColumn<AdminInstanceRow>[] = [
		{
			id: "instance_type",
			header: "类型",
			cell: (row) => (
				<TonePill tone={typeTone(row.instance_type)}>
					{row.instance_type === "challenge" ? "Challenge" : "GameBox"}
				</TonePill>
			),
			sortValue: (row) => row.instance_type,
		},
		{
			id: "status",
			header: "状态",
			cell: (row) => <TonePill tone="neutral">{row.status}</TonePill>,
			sortValue: (row) => row.status,
		},
		{
			id: "identifier",
			header: "标识",
			cell: (row) => <MonoText title={row.identifier}>{row.identifier}</MonoText>,
			sortValue: (row) => row.identifier,
			hideBelow: "md",
		},
		{
			id: "content",
			header: "内容",
			cell: (row) => (
				<span>
					{row.content_title ?? row.challenge_id ?? row.gamebox_id ?? "—"}
				</span>
			),
			sortValue: (row) => row.content_title ?? row.challenge_id ?? row.gamebox_id ?? "",
		},
		{
			id: "owner",
			header: "归属",
			cell: (row) => ownerLabel(row),
			sortValue: (row) => row.user_name ?? row.team_name ?? "",
		},
		{
			id: "runtime_generation",
			header: "代数",
			align: "right",
			cell: (row) =>
				row.runtime_generation === null || row.runtime_generation === undefined ? (
					<span className="text-xs">—</span>
				) : (
					<MonoText>{row.runtime_generation}</MonoText>
				),
			sortValue: (row) => row.runtime_generation ?? 0,
			hideBelow: "lg",
		},
		{
			id: "created_at",
			header: "创建时间",
			cell: (row) => <MonoText>{formatDateTime(row.created_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.created_at,
			hideBelow: "lg",
		},
		{
			id: "destroy_at",
			header: "销毁时间",
			cell: (row) =>
				row.destroy_at ? (
					<MonoText>{formatDateTime(row.destroy_at, { seconds: true })}</MonoText>
				) : (
					<span className="text-xs">—</span>
				),
			sortValue: (row) => row.destroy_at ?? "",
			hideBelow: "lg",
		},
	];

	const mobileCard = (row: AdminInstanceRow): ReactNode => (
		<MobileFacts
			items={[
				{
					k: "类型",
					v: (
						<TonePill tone={typeTone(row.instance_type)}>
							{row.instance_type === "challenge" ? "Challenge" : "GameBox"}
						</TonePill>
					),
				},
				{ k: "状态", v: <TonePill tone="neutral">{row.status}</TonePill> },
				{ k: "标识", v: <MonoText>{row.identifier}</MonoText> },
				{ k: "内容", v: row.content_title ?? row.challenge_id ?? row.gamebox_id ?? "—" },
				{ k: "归属", v: ownerLabel(row) },
				{
					k: "销毁时间",
					v: row.destroy_at ? <MonoText>{formatDateTime(row.destroy_at)}</MonoText> : "—",
				},
			]}
		/>
	);

	return (
		<SectionCard
			title="赛事实例"
			description="归一化的 event_instances 视图（Challenge 与 GameBox 统一呈现）。管理端列表**不返回 flag**，因此这里不展示任何 flag。"
		>
			<PagedTable
				query={pagedQuery}
				columns={columns}
				getRowId={(row) => row.id}
				page={safePage}
				pageSize={pageSize}
				total={total}
				onPageChange={setPage}
				onPageSizeChange={(size) => {
					setPageSize(size);
					setPage(1);
				}}
				mobileCard={mobileCard}
				errorTitle="加载赛事实例失败"
				empty={
					<EmptyBlock
						title="本赛事暂无实例"
						description="实例由选手启动靶机 / GameBox 后产生，或由 AWD / AWDP 部署流程创建。"
						icon={<Boxes className="size-5" />}
						action={
							<Button variant="outline" size="sm" onClick={() => void query.refetch()}>
								重新加载
							</Button>
						}
					/>
				}
			/>
		</SectionCard>
	);
}
