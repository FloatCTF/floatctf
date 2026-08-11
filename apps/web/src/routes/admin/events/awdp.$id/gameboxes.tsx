import { Box, Button, Select, Spinner } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { adminApi } from "@/api";

export const Route = createFileRoute("/admin/events/awdp/$id/gameboxes")({
	component: RouteComponent,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();
	const [selected, setSelected] = useState("");

	const { data, isLoading } = useQuery({
		queryKey: ["awdp-admin-gameboxes", id],
		queryFn: () => awdpAdminApi.listEventGameboxes(id),
	});
	// GameBox 库（仅完整 [awdp] capability 可选；后端校验，前端提示）。
	const { data: libraryData, isLoading: libLoading } = useQuery({
		queryKey: ["gamebox-library"],
		queryFn: async () => {
			const res = await adminApi.awd.listGameboxes();
			return res.data ?? [];
		},
	});

	const egs = data?.data?.data ?? [];
	const library = (libraryData ?? []) as Array<{
		id: string;
		name: string;
		safe_name: string;
		awdp_capable?: boolean;
	}>;

	const attach = useMutation({
		mutationFn: (gameboxId: string) => awdpAdminApi.attachGamebox(id, gameboxId),
		onSuccess: () => {
			banner.showBanner("success", "GameBox 已挂载");
			setSelected("");
			queryClient.invalidateQueries({ queryKey: ["awdp-admin-gameboxes", id] });
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const detach = useMutation({
		mutationFn: (egId: string) => awdpAdminApi.detachGamebox(id, egId),
		onSuccess: () => {
			banner.showBanner("info", "已移除");
			queryClient.invalidateQueries({ queryKey: ["awdp-admin-gameboxes", id] });
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	if (isLoading) {
		return (
			<div className="p-4">
				<Spinner />
			</div>
		);
	}

	return (
		<div className="p-3 max-w-3xl">
			<div className="flex gap-2 items-end mb-3">
				<Box sx={{ minWidth: 260 }}>
					<Select value={selected} onChange={(e) => setSelected(e.target.value)} disabled={libLoading}>
						<Select.Option value="">选择 GameBox（需完整 [awdp] capability）…</Select.Option>
						{library
							.filter((g) => !egs.some((eg) => eg.gamebox_id === g.id))
							.map((g) => (
								<Select.Option key={g.id} value={g.id}>
									{g.name}（{g.safe_name}）
								</Select.Option>
							))}
					</Select>
				</Box>
				<Button variant="primary" disabled={!selected || attach.isPending} onClick={() => attach.mutate(selected)}>
					Attach
				</Button>
			</div>

			<table className="w-full text-sm">
				<thead>
					<tr className="text-left text-gray-500">
						<th className="py-1">Name</th>
						<th className="py-1">Safe Name</th>
						<th className="py-1">Category</th>
						<th className="py-1">Source Dir</th>
						<th className="py-1">Enabled</th>
						<th className="py-1"></th>
					</tr>
				</thead>
				<tbody>
					{egs.map((eg) => (
						<tr key={eg.id} className="border-t">
							<td className="py-1 font-medium">{eg.name}</td>
							<td className="py-1">{eg.safe_name}</td>
							<td className="py-1">{eg.category}</td>
							<td className="py-1 font-mono text-xs">{eg.awdp_source_code_dir ?? "-"}</td>
							<td className="py-1">{eg.enabled ? "✓" : "✗"}</td>
							<td className="py-1">
								<Button size="small" variant="danger" onClick={() => detach.mutate(eg.id)}>
									Detach
								</Button>
							</td>
						</tr>
					))}
					{egs.length === 0 && (
						<tr>
							<td colSpan={6} className="py-2 text-gray-400">
								尚未挂载 GameBox。
							</td>
						</tr>
					)}
				</tbody>
			</table>
		</div>
	);
}
