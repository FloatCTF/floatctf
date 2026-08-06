import { Button, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/wireguard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-wg", id],
		queryFn: () => serviceApi.awd.wireguardConfig(id),
		retry: false,
	});

	const conf = q.data?.data?.config ?? "";

	const download = () => {
		const blob = new Blob([conf], { type: "text/plain" });
		const url = URL.createObjectURL(blob);
		const a = document.createElement("a");
		a.href = url;
		a.download = `floatctf-awd-${id}.conf`;
		a.click();
		URL.revokeObjectURL(url);
	};

	if (q.isLoading) return <Spinner />;
	if (q.isError) {
		return (
			<p className="text-danger">
				Failed to load WireGuard config (join a team / wait for deploy).
			</p>
		);
	}

	return (
		<div className="flex flex-col gap-2">
			<Button onClick={download} disabled={!conf}>
				Download .conf
			</Button>
			<pre className="p-3 bg-canvas-subtle overflow-auto text-xs">{conf}</pre>
		</div>
	);
}
