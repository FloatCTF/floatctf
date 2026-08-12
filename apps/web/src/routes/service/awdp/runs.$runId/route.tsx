import { UnderlineNav } from "@primer/react";
import { Outlet, createFileRoute } from "@tanstack/react-router";

import { RunWriteupEditor } from "@/components/awdp/RunWriteupEditor";
import { ServiceRouteGuard } from "@/routes/service/route";
import { RouterNavItem } from "../../events/jeopardy.$id/route";

export const Route = createFileRoute("/service/awdp/runs/$runId")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/**
 * AWDP Practice Run 页面布局（与 challenge 详情页同款）：
 *  - 顶部 UnderlineNav：Training | WriteUp
 *  - 左侧（flex-5）做题区（Outlet：Training 工作台 / WriteUp 编辑器）
 *  - 右侧（flex-7，border-l）Writeup 编辑器常驻
 */
function RouteComponent() {
	const { runId } = Route.useParams();
	return (
		<div className="flex h-full w-full flex-col">
			<UnderlineNav aria-label="Repository">
				<RouterNavItem to="/service/awdp/runs/$runId" params={{ runId }}>
					Training
				</RouterNavItem>
				<RouterNavItem
					to="/service/awdp/runs/$runId/writeup"
					params={{ runId }}
				>
					WriteUp
				</RouterNavItem>
			</UnderlineNav>

			<div className="flex h-full w-full min-h-0">
				{/* 左侧：做题（Training 工作台 / WriteUp 大编辑器）；上/下留出呼吸感（pt/pb-4），
				   避免内容紧贴导航与底边，左右 px-4 保持适中 */}
				<div className="flex flex-col px-4 pt-4 pb-4 flex-5 min-h-0">
					<Outlet />
				</div>

				{/* 右侧：写 WP（与 challenge 详情页一致，常驻） */}
				<div className="flex-7 h-full flex flex-col min-h-0 border-l">
					<RunWriteupEditor runId={runId} />
				</div>
			</div>
		</div>
	);
}
