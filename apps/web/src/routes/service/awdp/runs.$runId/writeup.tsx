import { createFileRoute } from "@tanstack/react-router";

import { RunWriteupEditor } from "@/components/awdp/RunWriteupEditor";

export const Route = createFileRoute("/service/awdp/runs/$runId/writeup")({
	component: RouteComponent,
});

/** WriteUp 标签页：我的 Writeup 大编辑器（数据同一份 run writeup）。 */
function RouteComponent() {
	const { runId } = Route.useParams();
	return <RunWriteupEditor runId={runId} />;
}
