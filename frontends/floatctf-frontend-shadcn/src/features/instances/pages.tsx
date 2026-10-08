/**
 * 我的实例 —— 选手自己启动的题目 / AWDP 练习环境，支持单选销毁与批量销毁。
 * 实例会自动过期回收（`destroy_at`），因此这里展示过期时间而不只是创建时间。
 */

import { useState } from "react";
import { Link } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Boxes, Trash2 } from "lucide-react";

import { callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText, PageBody, PageHeader, Toolbar } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import type { Instances as InstanceDto } from "@floatctf/sdk";
import { formatDateTime, formatRelative } from "~/lib/format";
import { useDocumentTitle, useNow } from "~/lib/hooks";

function instanceTone(status: string | undefined): "success" | "warning" | "danger" | "muted" {
	switch ((status ?? "").toLowerCase()) {
		case "running":
			return "success";
		case "pending":
		case "starting":
			return "warning";
		case "failed":
		case "error":
			return "danger";
		default:
			return "muted";
	}
}

export function InstancesPage() {
	useDocumentTitle("我的实例 · FloatCTF");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const now = useNow(30_000);
	const [selected, setSelected] = useState<string[]>([]);

	const query = useQuery({
		queryKey: qk.instances.list({ limit: 200 }),
		queryFn: () => callList<InstanceDto>(client.service.instances.fetch({ limit: 200 })),
		refetchInterval: 30_000,
	});

	const destroy = useMutation({
		mutationFn: (id: string) => callVoid(client.service.instances.destroy(id), "销毁实例"),
		onSuccess: () => {
			toast.success("实例已销毁");
			void queryClient.invalidateQueries({ queryKey: qk.instances.all });
		},
		onError: (error) => toast.apiError("销毁失败", error),
	});

	const bulkDestroy = useMutation({
		mutationFn: (ids: string[]) => callVoid(client.service.instances.bulkDelete(ids), "批量销毁"),
		onSuccess: (_data, ids) => {
			toast.success(`已销毁 ${ids.length} 个实例`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.instances.all });
		},
		onError: (error) => toast.apiError("批量销毁失败", error),
	});

	const columns: DataTableColumn<InstanceDto>[] = [
		{
			id: "name",
			header: "题目 / 靶机",
			sortValue: (row) => row.challenge_title ?? row.gamebox_title ?? row.identifier,
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate text-sm font-medium">
						{row.challenge_title ?? row.gamebox_title ?? "（未知实例）"}
					</p>
					<p className="truncate text-xs text-muted-foreground">
						{row.event_title ?? (row.run_id ? "AWDP 练习" : "题库练习")}
					</p>
				</div>
			),
		},
		{
			id: "type",
			header: "类型",
			hideBelow: "sm",
			cell: (row) => (
				<TonePill tone="neutral">{row.challenge_id ? "题目实例" : "靶机实例"}</TonePill>
			),
		},
		{
			id: "status",
			header: "状态",
			sortValue: (row) => row.status,
			cell: (row) => (
				<TonePill tone={instanceTone(row.status)}>{row.status || "未知"}</TonePill>
			),
		},
		{
			id: "identifier",
			header: "容器",
			hideBelow: "lg",
			cell: (row) => <MonoText>{row.identifier || "—"}</MonoText>,
		},
		{
			id: "created",
			header: "创建",
			hideBelow: "md",
			sortValue: (row) => row.created_at,
			cell: (row) => (
				<span className="text-xs text-muted-foreground">{formatRelative(row.created_at, now)}</span>
			),
		},
		{
			id: "expires",
			header: "自动回收",
			hideBelow: "md",
			sortValue: (row) => row.destroy_at ?? "",
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{row.destroy_at ? formatDateTime(row.destroy_at) : "—"}
				</span>
			),
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="我的实例"
				description="你启动的题目与靶机环境。实例到期会被平台自动回收。"
				actions={
					<Toolbar>
						{selected.length > 0 ? (
							<Button
								variant="destructive"
								onClick={async () => {
									const ok = await confirm({
										title: `销毁选中的 ${selected.length} 个实例？`,
										description: "实例对应的容器会被停止并删除。",
										consequences: ["容器内未保存的修改会丢失", "需要重新启动实例才能继续操作"],
										tone: "danger",
										confirmText: "批量销毁",
									});
									if (ok) bulkDestroy.mutate(selected);
								}}
								disabled={bulkDestroy.isPending}
							>
								<Trash2 /> 销毁选中（{selected.length}）
							</Button>
						) : null}
						<Button variant="outline" asChild>
							<Link to="/challenges">
								<Boxes /> 去题库启动
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={5} columns={5} />}
				errorTitle="加载实例失败"
				isEmpty={(result) => result.items.length === 0}
				empty={
					<EmptyBlock
						title="还没有运行中的实例"
						description="在题库或赛事里启动题目后，实例会出现在这里。"
						icon={<Boxes className="size-5" />}
						action={
							<Button size="sm" asChild>
								<Link to="/challenges">浏览题库</Link>
							</Button>
						}
					/>
				}
			>
				{(result) => (
					<DataTable
						data={result.items}
						getRowId={(row) => row.id}
						columns={columns}
						selectable
						selectedIds={selected}
						onSelectionChange={setSelected}
						rowActions={(row) => (
							<Button
								variant="ghost"
								size="sm"
								disabled={destroy.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: "销毁该实例？",
										description: row.challenge_title ?? row.gamebox_title ?? row.identifier,
										consequences: ["容器会被删除，环境内未保存的修改会丢失"],
										tone: "danger",
										confirmText: "销毁",
									});
									if (ok) destroy.mutate(row.id);
								}}
							>
								<Trash2 /> 销毁
							</Button>
						)}
						mobileCard={(row) => (
							<div className="space-y-1">
								<div className="flex items-center justify-between gap-2">
									<span className="truncate text-sm font-medium">
										{row.challenge_title ?? row.gamebox_title ?? "（未知实例）"}
									</span>
									<TonePill tone={instanceTone(row.status)}>{row.status || "未知"}</TonePill>
								</div>
								<MonoText>{row.identifier}</MonoText>
								<p className="text-xs text-muted-foreground">
									自动回收：{row.destroy_at ? formatDateTime(row.destroy_at) : "—"}
								</p>
							</div>
						)}
					/>
				)}
			</QueryState>
		</PageBody>
	);
}
