import { CheckIcon } from "@primer/octicons-react";
import { Button, Dialog } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useCallback, useRef, useState } from "react";

import { adminApi } from "@/api";
import type { AwdpAdminEventGameBoxDto } from "@/api/awdp";
import { awdpAdminApi } from "@/api/awdp";
import type { QueryParams } from "@/api/axios";
import { GenericTable, useMsgBanner } from "@/components";
import { QUERY_KEY as LIB_QUERY_KEY } from "@/routes/admin/awd/gameboxes";
import { useSelectedRowIds } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awdp/$id/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const subject = `awdp_event_gameboxes: ${id}`;
	const banner = useMsgBanner({});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: [subject] });
	};

	const columns = [
		{
			accessorKey: "name",
			header: "GameBox Name",
			field: "name",
			sortBy: true,
		},
		{
			accessorKey: "safe_name",
			header: "Safe Name",
			field: "safe_name",
		},
		{
			accessorKey: "category",
			header: "Category",
			field: "category",
		},
		{
			accessorKey: "awdp_source_code_dir",
			header: "Source Dir",
			field: "awdp_source_code_dir",
			renderCell: (row: AwdpAdminEventGameBoxDto) => (
				<span className="font-mono text-xs">{row.awdp_source_code_dir ?? "-"}</span>
			),
		},
		{
			accessorKey: "enabled",
			header: "Enabled",
			field: "enabled",
			renderCell: (row: AwdpAdminEventGameBoxDto) => (
				<span>{row.enabled ? <CheckIcon /> : <></>}</span>
			),
		},
	];

	const customActions = (
		<div className="flex gap-1">
			<AddGameBoxButton event_id={id} refresh_query_key={subject} />
		</div>
	);

	return (
		<div className="m-2 flex items-start gap-2">
			<GenericTable
				subject={subject}
				columns={columns}
				queryFn={(params?: QueryParams) =>
					awdpAdminApi.listEventGameboxes(id, params)
				}
				removeFn={(ids) =>
					awdpAdminApi
						.detachGamebox(id, ids[0])
						.then((r) => ({ ...r, data: 0 }))
				}
				customActions={customActions}
				disableAdd
				filterKeys={["name", "safe_name", "category"]}
				externalBanner={banner}
			/>
		</div>
	);
}

/**
 * Add GameBox to AWDP Event —— 照搬 AWD AddGameBoxButton：
 * Dialog 内嵌库 GenericTable，勾选后批量 attach（仅完整 [awdp] capability 可挂载）。
 */
function AddGameBoxButton({
	event_id,
	refresh_query_key,
}: {
	event_id: string;
	refresh_query_key?: string;
}) {
	const queryClient = useQueryClient();
	const [isOpen, setIsOpen] = useState(false);
	const buttonRef = useRef<HTMLButtonElement>(null);
	const onDialogClose = useCallback(() => setIsOpen(false), []);
	const [userSelectedRowIds, setUserSelectedRowIds] = useSelectedRowIds();
	const banner = useMsgBanner();

	const addMutation = useMutation({
		mutationFn: async (ids: string[]) => {
			for (const gameboxId of ids) {
				await awdpAdminApi.attachGamebox(event_id, gameboxId);
			}
		},
		onSuccess: () => {
			if (refresh_query_key) {
				queryClient.invalidateQueries({ queryKey: [refresh_query_key] });
			}
			setIsOpen(false);
			setUserSelectedRowIds(new Set());
			banner.showBanner("success", "Attach GameBoxes Success");
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	const user_op_actions = (
		<Button
			variant="primary"
			disabled={userSelectedRowIds.size === 0}
			onClick={() => {
				addMutation.mutate(Array.from(userSelectedRowIds));
			}}
		>
			Attach
		</Button>
	);

	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{
			accessorKey: "name",
			header: "Name",
			field: "name",
			sortBy: true,
		},
		{
			accessorKey: "safe_name",
			header: "Safe Name",
			field: "safe_name",
		},
		{
			accessorKey: "category",
			header: "Category",
			field: "category",
		},
	];

	return (
		<>
			{isOpen && (
				<Dialog title="Attach GameBoxes to AWDP Event" onClose={onDialogClose}>
					<GenericTable
						subject={LIB_QUERY_KEY}
						columns={columns}
						queryFn={(params?: QueryParams) =>
							adminApi.awd.listGameboxes(params)
						}
						disableAdd
						filterKeys={["name", "safe_name", "category", "hidden"]}
						enableInternalActions={false}
						selectedRowIds={userSelectedRowIds}
						onSelectedRowIdsChange={setUserSelectedRowIds}
						customActions={user_op_actions}
						externalBanner={banner}
					/>
				</Dialog>
			)}
			<Button
				variant="primary"
				ref={buttonRef}
				onClick={() => setIsOpen(!isOpen)}
			>
				Attach GameBoxes
			</Button>
		</>
	);
}
