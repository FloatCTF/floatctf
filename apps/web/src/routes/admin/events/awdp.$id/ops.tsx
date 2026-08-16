import { Button, ButtonGroup, Label, Spinner, useConfirm } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awdp/$id/ops")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const PHASE_META: Record<
	string,
	{ text: string; variant: "attention" | "accent" | "success" | "done" }
> = {
	pending: { text: "pending", variant: "attention" },
	break: { text: "break", variant: "accent" },
	fix: { text: "fix", variant: "success" },
	ended: { text: "ended", variant: "done" },
};

function RouteComponent() {
	const { id } = Route.useParams();
	const confirmDialog = useConfirm();
	const banner = useMsgBanner({});
	const qc = useQueryClient();

	const configQuery = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
	});
	const phase = configQuery.data?.data?.phase ?? "pending";
	const phaseMeta = PHASE_META[phase] ?? PHASE_META.pending;

	const invalidate = () => {
		qc.invalidateQueries({ queryKey: ["awdp-config", id] });
		qc.invalidateQueries({ queryKey: ["event", id] });
	};

	const start = useMutation({
		mutationFn: () => awdpAdminApi.start(id),
		onSuccess: () => {
			banner.showBanner("success", "Started → Break");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});
	const breakToFix = useMutation({
		mutationFn: () => awdpAdminApi.breakToFix(id),
		onSuccess: () => {
			banner.showBanner("success", "Break → Fix (all instances reset)");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});
	const finish = useMutation({
		mutationFn: () => awdpAdminApi.finish(id),
		onSuccess: () => {
			banner.showBanner("success", "Finished");
			invalidate();
		},
		onError: banner.showErrorBanner,
	});

	const pending = start.isPending || breakToFix.isPending || finish.isPending;

	return (
		<div className="flex flex-col gap-4 m-2">
			<banner.BannerComponent />
			<section>
				<h4 className="font-bold mb-2">Lifecycle</h4>
				<div className="flex items-center gap-2 mb-2">
					<Label variant={phaseMeta.variant}>Phase: {phaseMeta.text}</Label>
				</div>
				<ButtonGroup>
					<Button
						variant="primary"
						disabled={phase !== "pending" || pending}
						onClick={() => start.mutate()}
					>
						Start (→ Break)
					</Button>
					<Button
						disabled={phase !== "break" || pending}
						onClick={() => breakToFix.mutate()}
					>
						Break → Fix
					</Button>
					<Button
						variant="danger"
						disabled={phase !== "fix" || pending}
						onClick={async () => {
							const ok = await confirmDialog({
								title: "确认结束赛事？",
								content:
									"Fix → Ended：选手不能再提交 flag / patch，剩余评估仍会结算。",
								confirmButtonType: "danger",
							});
							if (ok) finish.mutate();
						}}
					>
						Finish (→ Ended)
					</Button>
				</ButtonGroup>
				{pending && (
					<span className="ml-2">
						<Spinner size="small" />
					</span>
				)}
			</section>

			<section>
				<h4 className="font-bold mb-2">说明</h4>
				<ul className="text-sm opacity-80 list-disc pl-5 flex flex-col gap-1">
					<li>
						Break 到期后 tick 会自动推进到 Fix；Fix 最后一轮 cutoff 自动结束。
					</li>
					<li>手动按钮用于提前推进阶段。</li>
					<li>
						Break → Fix 会把全部实例重置为 pristine（runtime_generation
						+1，公开端口不变）。
					</li>
				</ul>
			</section>
		</div>
	);
}
