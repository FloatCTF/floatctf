import { Button } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";

export const Route = createFileRoute("/admin/events/awdp/$id/ops")({
	component: RouteComponent,
});

const PHASE_LABEL: Record<string, string> = {
	pending: "Pending",
	break: "Break",
	fix: "Fix",
	ended: "Ended",
};

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();
	const { data } = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
	});
	const cfg = data?.data?.data;
	const phase = cfg?.phase ?? "pending";

	const invalidate = () => queryClient.invalidateQueries({ queryKey: ["awdp-config", id] });

	const start = useMutation({
		mutationFn: () => awdpAdminApi.start(id),
		onSuccess: () => {
			banner.showBanner("success", "赛事已开始（Break）");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const breakToFix = useMutation({
		mutationFn: () => awdpAdminApi.breakToFix(id),
		onSuccess: () => {
			banner.showBanner("success", "Break → Fix（实例已重置）");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const finish = useMutation({
		mutationFn: () => awdpAdminApi.finish(id),
		onSuccess: () => {
			banner.showBanner("info", "赛事已结束");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	return (
		<div className="p-3 max-w-lg">
			<div className="mb-3 text-sm">
				当前 Phase：<b>{PHASE_LABEL[phase] ?? phase}</b>
			</div>
			<div className="flex flex-col gap-2 items-start">
				<Button variant="primary" disabled={phase !== "pending" || start.isPending} onClick={() => start.mutate()}>
					Start Event（→ Break）
				</Button>
				<Button disabled={phase !== "break" || breakToFix.isPending} onClick={() => breakToFix.mutate()}>
					Break → Fix（重置全部实例）
				</Button>
				<Button variant="danger" disabled={phase !== "fix" || finish.isPending} onClick={() => finish.mutate()}>
					Finish（→ Ended）
				</Button>
			</div>
			<p className="text-xs text-gray-500 mt-3">
				Break 到期后 tick 会自动推进到 Fix；Fix 最后一轮 cutoff 自动结束。手动按钮用于提前推进。
			</p>
		</div>
	);
}
