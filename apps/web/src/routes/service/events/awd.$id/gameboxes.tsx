import type { UniResponse } from "@/api/axios";
import { Button, Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";
import type { AwdGameBox } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner();
	const queryClient = useQueryClient();
	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdGameBox[]>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awd-gameboxes", id],
		queryFn: () => serviceApi.awd.gameboxes(id),
	});

	const resetMutation = useMutation({
		mutationFn: (instanceId: string) =>
			serviceApi.awd.resetGamebox(id, instanceId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["awd-gameboxes", id] });
			banner.showBanner("success", "Reset requested");
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	const columns = [
		{
			accessorKey: "gamebox_name",
			header: "GameBox",
			field: "gamebox_name",
			rowHeader: true,
			renderCell: (row: AwdGameBox) => (
				<span>{row.gamebox_name || row.container_name}</span>
			),
		},
		{
			accessorKey: "gamebox_ip",
			header: "IP",
			field: "gamebox_ip",
			renderCell: (row: AwdGameBox) => <code>{row.gamebox_ip}</code>,
		},
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
		},
		{
			accessorKey: "health_status",
			header: "Health",
			field: "health_status",
		},
		{
			accessorKey: "action",
			header: "Action",
			field: "action",
			renderCell: (row: AwdGameBox) => (
				<Button
					variant="invisible"
					onClick={() => resetMutation.mutate(row.id)}
					disabled={resetMutation.isPending}
				>
					Reset
				</Button>
			),
		},
	];

	const table = useReactTable({
		data: data?.data ?? [],
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return (
			<div className="p-4 flex flex-col gap-2">
				<banner.BannerComponent />
				<p className="text-sm opacity-80">
					未获取到游戏盒列表：请先到 Overview 页加入队伍；若已加入且长时间为空，
					说明赛事尚未部署或你的队伍创建于部署之后（需管理员重新 Deploy）。
				</p>
			</div>
		);
	}

	return (
		<div className="m-2">
			<banner.BannerComponent />
			<p className="text-sm opacity-80 mb-2">
				游戏盒通过 SSH 访问（见 SSH 页凭据）；Reset 后实例重建但 IP/凭据不变。
			</p>
			<Table.Container>
				<DataTable
					aria-labelledby="awd-gameboxes"
					// @ts-ignore
					columns={columns}
					data={table.getRowModel().rows.map((row) => row.original)}
				/>
			</Table.Container>
			{(data?.data?.length ?? 0) === 0 && (
				<p className="text-sm opacity-70 mt-2">
					暂无游戏盒：赛事尚未部署，或你的队伍创建于部署之后（需管理员重新
					Deploy）。
				</p>
			)}
		</div>
	);
}
