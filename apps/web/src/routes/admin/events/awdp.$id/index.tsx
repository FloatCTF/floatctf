import { Button, Heading, Label } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { awdpAdminApi } from "@/api/awdp";

export const Route = createFileRoute("/admin/events/awdp/$id/")({
	component: RouteComponent,
});

const PHASE_LABEL: Record<string, { text: string; variant: "attention" | "success" | "done" | "severe" }> = {
	pending: { text: "Pending", variant: "attention" },
	break: { text: "Break", variant: "attention" },
	fix: { text: "Fix", variant: "success" },
	ended: { text: "Ended", variant: "done" },
};

function RouteComponent() {
	const { id } = Route.useParams();
	const navigate = useNavigate();
	const { data } = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});
	const { data: cfgData } = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
		retry: false,
	});
	const ev = data?.data;
	const cfg = cfgData?.data?.data;
	const meta = cfg ? (PHASE_LABEL[cfg.phase] ?? PHASE_LABEL.pending) : null;

	return (
		<div className="p-3">
			<Heading as="h2" sx={{ mb: 2 }}>
				{ev?.title ?? "AWDP Event"}
			</Heading>
			{cfg ? (
				<div className="flex items-center gap-3 mb-3">
					<Label variant={meta!.variant}>{meta!.text}</Label>
					<span className="text-sm text-gray-500">
						{cfg.total_rounds} rounds · break +{cfg.break_score} · fix +{cfg.fix_round_score}
					</span>
				</div>
			) : null}
			<Button variant="primary" onClick={() => navigate({ to: "/admin/events/awdp/$id/configure", params: { id } })}>
				进入配置
			</Button>
		</div>
	);
}
