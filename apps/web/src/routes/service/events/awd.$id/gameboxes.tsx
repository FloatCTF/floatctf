import { Button, Spinner } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const qc = useQueryClient();
	const q = useQuery({
		queryKey: ["awd-gameboxes", id],
		queryFn: () => serviceApi.awd.gameboxes(id),
	});

	const reset = useMutation({
		mutationFn: (instanceId: string) =>
			serviceApi.awd.resetGamebox(id, instanceId),
		onSuccess: () => {
			banner.showBanner("success", "Reset requested");
			qc.invalidateQueries({ queryKey: ["awd-gameboxes", id] });
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	if (q.isLoading) return <Spinner />;
	if (q.isError) {
		return (
			<div>
				<banner.BannerComponent />
				<p className="text-sm opacity-80">
					未获取到游戏盒列表：请先到 Overview 页加入队伍；若已加入且长时间为空，
					说明赛事尚未部署或你的队伍创建于部署之后（需管理员重新 Deploy）。
				</p>
			</div>
		);
	}
	const boxes = q.data?.data ?? [];

	return (
		<div>
			<banner.BannerComponent />
			<p className="text-sm opacity-80 mb-2">
				游戏盒通过 SSH 访问（见 SSH 页凭据）；Reset 后实例重建但 IP/凭据不变。
			</p>
			<table className="w-full text-sm">
				<thead>
					<tr>
						<th align="left">IP</th>
						<th align="left">Status</th>
						<th align="left">Health</th>
						<th align="left">Container</th>
						<th />
					</tr>
				</thead>
				<tbody>
					{boxes.map((b) => (
						<tr key={b.id}>
							<td>
								<code>{b.gamebox_ip}</code>
							</td>
							<td>{b.status}</td>
							<td>{b.health_status}</td>
							<td>
								<code>{b.container_name}</code>
							</td>
							<td>
								<Button
									size="small"
									disabled={reset.isPending}
									onClick={() => reset.mutate(b.id)}
								>
									Reset
								</Button>
							</td>
						</tr>
					))}
					{boxes.length === 0 && (
						<tr>
							<td colSpan={5}>
								暂无游戏盒：赛事尚未部署，或你的队伍创建于部署之后（需管理员重新
								Deploy）。
							</td>
						</tr>
					)}
				</tbody>
			</table>
		</div>
	);
}
