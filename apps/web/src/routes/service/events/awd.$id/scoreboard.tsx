import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/scoreboard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-scores", id],
		queryFn: () => serviceApi.awd.scores(id),
	});

	if (q.isLoading) return <Spinner />;
	const rows = q.data?.data ?? [];

	return (
		<table className="w-full text-sm">
			<thead>
				<tr>
					<th align="left">#</th>
					<th align="left">Team</th>
					<th align="right">Attack</th>
					<th align="right">Defense</th>
					<th align="right">Total</th>
				</tr>
			</thead>
			<tbody>
				{rows.map((r) => (
					<tr key={r.team_id}>
						<td>{r.rank}</td>
						<td>{r.team_name}</td>
						<td align="right">{r.attack_score}</td>
						<td align="right">{r.defense_score}</td>
						<td align="right">
							<strong>{r.total_score}</strong>
						</td>
					</tr>
				))}
				{rows.length === 0 && (
					<tr>
						<td colSpan={5}>No scores yet.</td>
					</tr>
				)}
			</tbody>
		</table>
	);
}
