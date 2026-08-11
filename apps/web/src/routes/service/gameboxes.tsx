import { Button, Heading, Label, Spinner } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { type GameBoxCatalogDto, awdpRunApi } from "@/api/awdpRuns";
import { useMsgBanner } from "@/components";
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

	const catalogQuery = useQuery({
		queryKey: ["gamebox-catalog"],
		queryFn: () => awdpRunApi.gameboxCatalog(),
	});
	const catalog = catalogQuery.data?.data ?? [];

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

	if (catalogQuery.isLoading) {
		return (
			<div className="p-4">
				<Spinner size="large" />
			</div>
		);
	}
	if (catalogQuery.isError) {
		return (
			<div className="p-4 flex flex-col gap-2">
				<banner.BannerComponent />
				<InlineMessage variant="critical">
					{(catalogQuery.error as Error)?.message ??
						"Failed to load GameBox catalog."}
				</InlineMessage>
			</div>
		);
	}

	return (
		<div className="m-2 flex flex-col gap-3">
			<banner.BannerComponent />
			<div className="flex items-center gap-2">
				<Heading as="h2">Gameboxes</Heading>
				<p className="text-sm opacity-70">
					AWDP 可训练 GameBox（source.zip 产物就绪，Break → Fix → Turns）。
				</p>
			</div>
			<div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
				{catalog.map((gb) => (
					<CatalogCard
						key={gb.id}
						gb={gb}
						starting={
							startMutation.isPending && startMutation.variables === gb.id
						}
						onStart={() => startMutation.mutate(gb.id)}
					/>
				))}
			</div>
			{catalog.length === 0 && (
				<p className="text-sm opacity-70">暂无 GameBox。</p>
			)}
		</div>
	);
}

function CatalogCard({
	gb,
	starting,
	onStart,
}: {
	gb: GameBoxCatalogDto;
	starting: boolean;
	onStart: () => void;
}) {
	const active = gb.active_training;
	return (
		<section className="p-3 rounded border flex flex-col gap-2">
			<div className="flex items-center gap-2 mb-1">
				<h4 className="font-bold flex-1 break-all">{gb.name}</h4>
				<Label variant="accent">AWDP</Label>
				{active && <Label variant="success">Training</Label>}
			</div>
			<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm">
				<dt className="font-bold">Description</dt>
				<dd className="font-medium whitespace-pre-wrap break-all">
					{gb.description || "-"}
				</dd>
				<dt className="font-bold">Category</dt>
				<dd className="font-medium">{gb.category || "-"}</dd>
				<dt className="font-bold">Version</dt>
				<dd className="font-medium">{gb.version ?? "-"}</dd>
			</dl>
			<div className="mt-auto pt-2">
				{active ? (
					<AppLink
						to="/service/awdp/runs/$runId"
						params={{ runId: active.run_id }}
						style={{ textDecoration: "none" }}
					>
						<Button variant="primary" block>
							Continue Training
						</Button>
					</AppLink>
				) : (
					<Button variant="primary" block disabled={starting} onClick={onStart}>
						{starting ? "Starting…" : "Start Training"}
					</Button>
				)}
			</div>
		</section>
	);
}
