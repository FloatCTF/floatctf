import type { UniResponse } from "@/api/axios";
import { Button, Label, Spinner, useConfirm } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";
import { awdPlayerApi } from "@/api/awd";
import type { AwdGameBox } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/** Determine if reset is allowed based on AWD state. */
function resetAllowed(awdPhase: string | undefined, awdStatus: string | undefined, banned: boolean, finalSettlement: boolean): { allowed: boolean; reason: string } {
	if (banned) return { allowed: false, reason: "Team is banned." };
	if (!awdStatus || !awdPhase) return { allowed: false, reason: "AWD not configured." };
	if (awdStatus === "finished" || awdStatus === "archived") return { allowed: false, reason: "Competition finished." };
	if (finalSettlement) return { allowed: false, reason: "Final settlement — competition is closed." };
	if (awdStatus === "paused") return { allowed: false, reason: "Competition paused." };
	if (awdStatus === "network_error") return { allowed: false, reason: "Infrastructure unavailable." };
	if (awdPhase === "pause") return { allowed: false, reason: "Competition paused." };
	if (awdPhase === "hardening" || awdPhase === "attack") return { allowed: true, reason: "" };
	return { allowed: false, reason: "Reset not available." };
}

function statusVariant(s: string): "success" | "danger" | "attention" | "default" {
	switch (s) {
		case "running":
		case "ready":
			return "success";
		case "resetting":
			return "attention";
		case "missing":
		case "start_failed":
		case "reset_failed":
		case "stopped":
			return "danger";
		default:
			return "default";
	}
}

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner();
	const confirmDialog = useConfirm();
	const queryClient = useQueryClient();

	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdGameBox[]>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awd-gameboxes", id],
		queryFn: () => serviceApi.awd.gameboxes(id),
	});

	const statusQuery = useQuery({
		queryKey: ["awd-player-status", id],
		queryFn: () => awdPlayerApi.status(id),
		retry: false,
	});

	const awdStatus = statusQuery.data?.data ?? null;
	const resetState = resetAllowed(awdStatus?.phase, awdStatus?.status, awdStatus?.banned ?? false, awdStatus?.final_settlement ?? false);

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
			renderCell: (row: AwdGameBox) => (
				<Label variant={statusVariant(row.status)}>{row.status}</Label>
			),
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
					onClick={async () => {
						const ok = await confirmDialog({
							title: "Reset GameBox?",
							content:
								"Reset is destructive: the container will be destroyed and rebuilt from the original image. All your modifications will be lost. This may incur a score penalty if you exceed your free reset quota.",
							confirmButtonType: "danger",
						});
						if (ok) resetMutation.mutate(row.id);
					}}
					disabled={!resetState.allowed || resetMutation.isPending}
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

	if (isLoading || statusQuery.isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return (
			<div className="p-4 flex flex-col gap-2">
				<banner.BannerComponent />
				<p className="text-sm opacity-80">
					Unable to load GameBoxes. Join a team on the Overview page first.
					If already joined and empty, the event may not be deployed yet.
				</p>
			</div>
		);
	}

	return (
		<div className="m-2">
			<banner.BannerComponent />
			{!resetState.allowed && (
				<p className="text-sm text-[var(--fgColor-muted)] mb-2">
					Reset unavailable: {resetState.reason}
				</p>
			)}
			<p className="text-sm opacity-80 mb-2">
				GameBoxes are accessed via SSH (see SSH page for credentials). Reset destroys
				and rebuilds the container from the original image — all modifications are lost.
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
					No GameBoxes: event may not be deployed yet, or your team was created
					after deployment (requires admin to re-deploy).
				</p>
			)}
		</div>
	);
}