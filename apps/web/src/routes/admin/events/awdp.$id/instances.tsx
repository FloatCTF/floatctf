import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { awdpAdminApi } from "@/api/awdp";
import { useAwdpEventStream } from "@/hooks/useAwdpEventStream";

export const Route = createFileRoute("/admin/events/awdp/$id/instances")({
	component: RouteComponent,
});

function RouteComponent() {
	const { id } = Route.useParams();
	useAwdpEventStream({ eventId: id });
	const { data, isLoading } = useQuery({
		queryKey: ["awdp-admin-instances", id],
		queryFn: () => awdpAdminApi.listInstances(id),
		refetchInterval: 5000,
	});

	if (isLoading) {
		return (
			<div className="p-4">
				<Spinner />
			</div>
		);
	}
	const rows = data?.data?.data ?? [];

	return (
		<div className="p-3">
			<table className="w-full text-sm">
				<thead>
					<tr className="text-left text-gray-500">
						<th className="py-1">Instance</th>
						<th className="py-1">GameBox</th>
						<th className="py-1">Owner</th>
						<th className="py-1">State</th>
						<th className="py-1">Gen</th>
						<th className="py-1">Endpoints</th>
					</tr>
				</thead>
				<tbody>
					{rows.map((r) => (
						<tr key={r.instance_id} className="border-t">
							<td className="py-1 font-mono text-xs">
								{r.instance_id.slice(0, 8)}…
								<span className="text-gray-400"> ({r.container_name})</span>
							</td>
							<td className="py-1">{r.gamebox_name}</td>
							<td className="py-1 font-mono text-xs">
								{r.owner_user_id ? `user:${r.owner_user_id.slice(0, 8)}` : `team:${r.owner_team_id?.slice(0, 8)}`}
							</td>
							<td className="py-1">{r.runtime_state}</td>
							<td className="py-1">{r.runtime_generation}</td>
							<td className="py-1 font-mono text-xs">
								{r.endpoints.map((e) => `${e.protocol}://${e.public_host}:${e.public_port}`).join(" ") || "-"}
							</td>
						</tr>
					))}
					{rows.length === 0 && (
						<tr>
							<td colSpan={6} className="py-2 text-gray-400">
								暂无实例。
							</td>
						</tr>
					)}
				</tbody>
			</table>
		</div>
	);
}
