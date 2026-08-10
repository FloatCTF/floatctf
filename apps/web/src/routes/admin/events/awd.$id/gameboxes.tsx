import { CheckIcon } from "@primer/octicons-react";
import { ActionList, Button, Dialog } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useCallback, useRef, useState } from "react";

import { adminApi } from "@/api";
import type { EventGameBoxDto, GameBoxLibraryDto } from "@/api/awd";
import type { QueryParams } from "@/api/axios";
import { GenericTable, useMsgBanner } from "@/components";
import { QUERY_KEY as LIB_QUERY_KEY } from "@/routes/admin/awd/gameboxes";
import { useSelectedRowIds } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const subject = `awd_event_gameboxes: ${id}`;
	const banner = useMsgBanner({});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: [subject] });
	};

	// Enable / Disable 切换（对应 Jeopardy 的 Open/Hide）
	const toggle = useMutation({
		mutationFn: (vars: { egId: string; enabled: boolean }) =>
			adminApi.awd.updateEventGamebox(id, vars.egId, { enabled: vars.enabled }),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const columns = [
		{
			accessorKey: "gamebox_name",
			header: "GameBox Name",
			field: "gamebox_name",
			sortBy: true,
		},
		{
			accessorKey: "gamebox_safe_name",
			header: "Safe Name",
			field: "gamebox_safe_name",
		},
		{
			accessorKey: "host_offset",
			header: "Host Offset",
			field: "host_offset",
		},
		{
			accessorKey: "break_points",
			header: "Break Points",
			field: "break_points",
		},
		{
			accessorKey: "enabled",
			header: "Enabled",
			field: "enabled",
			renderCell: (row: EventGameBoxDto) => (
				<span>{row.enabled ? <CheckIcon /> : <></>}</span>
			),
		},
	];

	const columnActions = (row: EventGameBoxDto) => (
		<ActionList>
			<ActionList.Item
				key={`${row.id}-toggle`}
				onSelect={() => toggle.mutate({ egId: row.id, enabled: !row.enabled })}
			>
				{row.enabled ? "Disable" : "Enable"}
			</ActionList.Item>
		</ActionList>
	);

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
					adminApi.awd.listEventGameboxes(id, params)
				}
				removeFn={(ids) =>
					adminApi.awd
						.removeEventGamebox(id, ids[0])
						.then((r) => ({ ...r, data: 0 }))
				}
				columnActions={columnActions}
				customActions={customActions}
				disableAdd
				filterKeys={["gamebox_name", "gamebox_safe_name"]}
				externalBanner={banner}
			/>
		</div>
	);
}

/**
 * Add GameBox to Event —— 照搬 Jeopardy AddChallengeButton：
 * Dialog 内嵌一个库的 GenericTable，勾选后批量 add（同 Add Event Challenges 交互）。
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
				await adminApi.awd.addEventGamebox(event_id, {
					gamebox_id: gameboxId,
				});
			}
		},
		onSuccess: () => {
			if (refresh_query_key) {
				queryClient.invalidateQueries({ queryKey: [refresh_query_key] });
			}
			setIsOpen(false);
			setUserSelectedRowIds(new Set());
			banner.showBanner("success", "Add Event GameBoxes Success");
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
			Add
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
			accessorKey: "image_ref",
			header: "Image",
			field: "image_ref",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>{row.image_ref ?? "-"}</span>
			),
		},
	];

	return (
		<>
			{isOpen && (
				<Dialog title="Add GameBoxes to Event" onClose={onDialogClose}>
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
				Add GameBoxes
			</Button>
		</>
	);
}
