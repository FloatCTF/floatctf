import { useMutation } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { type GameBoxCatalogDto, awdpRunApi } from "@/api/awdpRuns";
import { GenericTable, useMsgBanner } from "@/components";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";
import { CheckIcon } from "@primer/octicons-react";
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
				// 有 active run → 直接进入训练页；否则点击即开始训练。
				if (row.active_training) {
					return (
						<AppLink
							to={"/service/awdp/runs/$runId"}
							params={{ runId: row.active_training.run_id }}
						>
							{row.name}
						</AppLink>
					);
				}
				const starting =
					startMutation.isPending && startMutation.variables === row.id;
				return (
					<button
						type="button"
						disabled={starting}
						onClick={() => startMutation.mutate(row.id)}
						style={{
							color: "#0969da",
							textDecoration: "underline",
							cursor: starting ? "default" : "pointer",
							opacity: starting ? 0.6 : 1,
						}}
					>
						{row.name}
					</button>
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
			accessorKey: "solved",
			header: "Solved",
			field: "solved",
			renderCell: (row: GameBoxCatalogDto) => {
				return row.solved ? (
					<CheckIcon size={16} fill="var(--fgColor-success)" />
				) : null;
			},
		},
		{
			accessorKey: "author",
			header: "Author",
			field: "author",
			renderCell: (row: GameBoxCatalogDto) => {
				return <span>{row.author || "—"}</span>;
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
			accessorKey: "updated_at",
			header: "Updated At",
			field: "updated_at",
			renderCell: (row: GameBoxCatalogDto) => {
				return <span>{DatetimeToShow(row.updated_at)}</span>;
			},
		},
	];
	const filterKeys = ["name", "category", "description", "solved"];

	return (
		<>
			<banner.BannerComponent />
			<GenericTable
				subject="GameBoxCatalog"
				subtitle="AWDP 训练场：点击名称进入训练场，点「开始」启动训练（Break → Fix → Turns）。"
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
