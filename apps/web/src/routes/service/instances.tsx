import { Button, Label } from "@primer/react";
import type { UniResponse } from "@/api/axios";
import type { InstancesDto as Instances } from "@/api/service/instances";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";

import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

/** 列表页展示类型：实例基础字段 + 关联名称（后端计算字段）。 */
type InstanceRow = Instances & {
	challenge_title?: string | null;
	event_title?: string | null;
	user_name?: string | null;
};

export const Route = createFileRoute("/service/instances")({
	component: RouteComponent,
});

function RouteComponent() {
	useTitle("Instances | FloatCTF");

	const subject = "Instances";
	const banner = useMsgBanner();
	const queryClient = useQueryClient();

	const filterKeys = [
		"id",
		"status",
		"identifier",
		"challenge_id",
		"event_id",
		"gamebox_id",
		"run_id",
	];

	const mutationInstance = useMutation({
		mutationFn: serviceApi.instances.destroy,
		onSuccess: (_data, id: string) => {
			// 乐观移除：destroy 成功后立即从缓存剔除该行（不依赖 refetch，
			// refetch 慢/失败时行也会立刻消失），再 invalidate 兜底拉真实状态。
			queryClient.setQueriesData(
				{ queryKey: [subject] },
				(old: UniResponse<Instances[]> | undefined) => {
					if (!old || !Array.isArray(old.data)) return old;
					return {
						...old,
						data: old.data.filter((row) => row.id !== id),
					};
				},
			);
			queryClient.invalidateQueries({ queryKey: [subject] });
			banner.showBanner("success", "Instance destroyed successfully");
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	const columns = [
		{
			accessorKey: "challenge_title",
			header: "Content",
			field: "challenge_title",
			rowHeader: true,
			renderCell: (row: InstanceRow) => {
				if (row.challenge_id) {
					return (
						<AppLink
							to={"/service/challenges/$id"}
							params={{ id: row.challenge_id }}
						>
							{row.challenge_title ?? row.challenge_id}
						</AppLink>
					);
				}
				if (row.run_id) {
					return (
						<AppLink
							to={"/service/awdp/runs/$runId"}
							params={{ runId: row.run_id }}
						>
							{row.gamebox_title ?? row.gamebox_id ?? "—"}
						</AppLink>
					);
				}
				return <span>—</span>;
			},
		},
		{
			accessorKey: "instance_type",
			header: "Type",
			field: "instance_type",
			renderCell: (row: InstanceRow) => (
				<Label variant={row.run_id ? "success" : "accent"}>
					{row.run_id ? "Gamebox" : "Challenge"}
				</Label>
			),
		},
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
		},
		{
			accessorKey: "event_title",
			header: "Event",
			field: "event_title",
			renderCell: (row: InstanceRow) => {
				return <span>{row.event_title ?? row.event_id}</span>;
			},
		},
		{
			accessorKey: "identifier",
			header: "Identifier",
			field: "identifier",
		},
		{
			accessorKey: "user_name",
			header: "User",
			field: "user_name",
			renderCell: (row: InstanceRow) => {
				return <span>{row.user_name ?? row.user_id}</span>;
			},
		},
		{
			accessorKey: "destroy_at",
			header: "Destroy At",
			field: "destroy_at",
			renderCell: (row: Instances) => {
				return <span>{DatetimeToShow(row.destroy_at)}</span>;
			},
		},
		{
			accessorKey: "action",
			header: "Action",
			field: "action",
			renderCell: (row: Instances) => {
				if (row.challenge_id) {
					return (
						<Button
							variant="invisible"
							onClick={() => {
								mutationInstance.mutate(row.id);
							}}
							style={{ color: "#DB0000" }}
						>
							Destroy
						</Button>
					);
				}
				if (row.run_id) {
					return (
						<AppLink
							to={"/service/awdp/runs/$runId"}
							params={{ runId: row.run_id }}
						>
							Open
						</AppLink>
					);
				}
				return null;
			},
		},
	];

	return (
		<GenericTable
			subject={subject}
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.instances.fetch}
			removeFn={serviceApi.instances.bulkDelete}
			staleTime={0}
			enableInternalActions={false}
			externalBanner={banner}
			disableAdd={true}
		/>
	);
}
