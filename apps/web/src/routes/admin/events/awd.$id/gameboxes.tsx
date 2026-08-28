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

	const toggle = useMutation({
		mutationFn: (vars: { egId: string; enabled: boolean }) =>
			adminApi.awd.updateEventGamebox(id, vars.egId, { enabled: vars.enabled }),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const columns = [
		{
			accessorKey: "gamebox_name",
			header: "GameBox",
			field: "gamebox_name",
			sortBy: true,
		},
		{
			accessorKey: "gamebox_version",
			header: "Version",
			field: "gamebox_version",
			renderCell: (row: EventGameBoxDto) => (
				<span>{row.gamebox_version ?? "-"}</span>
			),
		},
		{
			accessorKey: "host_offset",
			header: "Host Offset",
			field: "host_offset",
		},
		{
			accessorKey: "attack_score",
			header: "Attack Score",
			field: "attack_score",
		},
		{
			accessorKey: "judge_down_penalty",
			header: "Down Penalty",
			field: "judge_down_penalty",
		},
		{
			accessorKey: "first_bonus",
			header: "First Blood",
			field: "first_bonus",
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
				removeFn={async (ids) => {
					for (const eventGameboxId of ids) {
						await adminApi.awd.removeEventGamebox(id, eventGameboxId);
					}
					return { code: 0, message: "ok", data: ids.length };
				}}
				columnActions={columnActions}
				customActions={customActions}
				disableAdd
				filterKeys={["gamebox_name", "gamebox_safe_name"]}
				externalBanner={banner}
			/>
		</div>
	);
}

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
			queryClient.invalidateQueries({ queryKey: [LIB_QUERY_KEY] });
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
		{ accessorKey: "name", header: "Name", field: "name", sortBy: true },
		{ accessorKey: "safe_name", header: "Safe Name", field: "safe_name" },
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