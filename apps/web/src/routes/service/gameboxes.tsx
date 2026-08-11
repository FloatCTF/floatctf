import { Button, Label } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { type GameBoxCatalogDto, awdpRunApi } from "@/api/awdpRuns";
import { GenericTable, useMsgBanner } from "@/components";
import { AppLink } from "@/navigation";
import { ServiceRouteGuard } from "./route";

export const Route = createFileRoute("/service/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	useTitle("Gameboxes | FloatCTF");
	const banner = useMsgBanner({});
	const navigate = useNavigate();

	const startMutation = useMutation({
		mutationFn: (gameboxId: string) => awdpRunApi.startTraining(gameboxId),
		onSuccess: (res) => {
			const runId = res.data?.run_id;
			if (runId) {
				navigate({ to: "/service/awdp/runs/$runId", params: { runId } });
			}
		},
		onError: banner.showErrorBanner,
	});

	const columns = [
		{
			accessorKey: "name",
			header: "Name",
			field: "name",
			rowHeader: true,
			renderCell: (row: GameBoxCatalogDto) => {
				if (!row.active_training) {
					return <span>{row.name}</span>;
				}
				return (
					<AppLink
						to={"/service/awdp/runs/$runId"}
						params={{ runId: row.active_training.run_id }}
					>
						{row.name}
					</AppLink>
				);
			},
		},
		{
			accessorKey: "category",
			header: "Category",
			field: "category",
			renderCell: (row: GameBoxCatalogDto) => {
				return <span>{row.category || "—"}</span>;
			},
		},
		{
			accessorKey: "version",
			header: "Version",
			field: "version",
			renderCell: (row: GameBoxCatalogDto) => {
				return <span>{row.version ?? "—"}</span>;
			},
		},
		{
			accessorKey: "awdp_capable",
			header: "AWDP",
			field: "awdp_capable",
			renderCell: (row: GameBoxCatalogDto) => {
				return row.awdp_capable ? (
					<Label variant="accent">AWDP</Label>
				) : (
					<span>—</span>
				);
			},
		},
		{
			accessorKey: "training",
			header: "Training",
			field: "training",
			renderCell: (row: GameBoxCatalogDto) => {
				const active = row.active_training;
				if (!active) {
					return <span>—</span>;
				}
				return (
					<div className="flex items-center gap-2">
						<AppLink
							to={"/service/awdp/runs/$runId"}
							params={{ runId: active.run_id }}
						>
							Continue Training
						</AppLink>
						<Label variant="success">{active.phase}</Label>
					</div>
				);
			},
		},
		{
			accessorKey: "actions",
			header: "Actions",
			id: "actions",
			renderCell: (row: GameBoxCatalogDto) => {
				if (row.active_training) {
					return null;
				}
				const starting =
					startMutation.isPending && startMutation.variables === row.id;
				return (
					<Button
						variant="primary"
						size="small"
						disabled={starting}
						onClick={() => startMutation.mutate(row.id)}
					>
						{starting ? "Starting…" : "Start Training"}
					</Button>
				);
			},
		},
	];
	const filterKeys = ["name", "category", "description"];

	return (
		<>
			<banner.BannerComponent />
			<GenericTable
				subject="GameBoxCatalog"
				subtitle="AWDP 训练场：选择一个 GameBox 开始训练（Break → Fix → Turns）。"
				columns={columns}
				filterKeys={filterKeys}
				queryFn={awdpRunApi.gameboxCatalog}
				enableInternalActions={false}
				disableAdd={true}
				disableSelect={true}
				externalBanner={banner}
			/>
		</>
	);
}
