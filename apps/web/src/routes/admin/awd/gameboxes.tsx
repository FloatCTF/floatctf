import { ActionList, TextInput, useConfirm } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import type { GameBoxLibraryDto } from "@/api/awd";
import { type QueryParams, type UniResponse } from "@/api/axios";
import { GenericTable, useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

export const QUERY_KEY = "AWDGameBoxes";

function RouteComponent() {
	const confirmDialog = useConfirm();
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: [QUERY_KEY] });
	};

	const hide = useMutation({
		mutationFn: (gameboxId: string) => adminApi.awd.hideGamebox(gameboxId),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	// Identity-only edit form (package import is the create path; UI upload TODO).
	const mutationData = useReactive<Partial<GameBoxLibraryDto>>({
		name: "",
		category: "other",
		description: "",
		hidden: false,
	});

	const mutationColumns = [
		{
			header: "name",
			field: "name",
			render: (
				<TextInput
					value={mutationData.name ?? ""}
					onChange={(e) => {
						mutationData.name = e.target.value;
					}}
				/>
			),
		},
		{
			header: "category",
			field: "category",
			render: (
				<TextInput
					value={mutationData.category ?? ""}
					onChange={(e) => {
						mutationData.category = e.target.value;
					}}
				/>
			),
		},
		{
			header: "description",
			field: "description",
			render: (
				<TextInput
					value={mutationData.description ?? ""}
					onChange={(e) => {
						mutationData.description = e.target.value;
					}}
				/>
			),
		},
	];

	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{ accessorKey: "name", header: "Name", field: "name", sortBy: true },
		{
			accessorKey: "safe_name",
			header: "Safe Name",
			field: "safe_name",
		},
		{
			accessorKey: "version",
			header: "Version",
			field: "version",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>{row.version ?? "-"}</span>
			),
		},
		{
			accessorKey: "build_status",
			header: "Build",
			field: "build_status",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>{row.build_status ?? "-"}</span>
			),
		},
		{
			accessorKey: "image_ref",
			header: "Image",
			field: "image_ref",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>
					{row.image_ref ?? "-"}
					{row.image_repo_digest ? " 🔒" : ""}
				</span>
			),
		},
		{
			accessorKey: "hidden",
			header: "Hidden",
			field: "hidden",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>{row.hidden ? "yes" : "no"}</span>
			),
		},
	];

	const columnActions = (row: GameBoxLibraryDto) => (
		<ActionList>
			<ActionList.Item
				variant="danger"
				onSelect={async () => {
					const ok = await confirmDialog({
						title: `Hide GameBox ${row.name}？`,
						content: "被赛事引用时将被拒绝。",
						confirmButtonType: "danger",
					});
					if (ok) hide.mutate(row.id);
				}}
			>
				Hide
			</ActionList.Item>
		</ActionList>
	);

	const queryFn = async (
		params?: QueryParams,
	): Promise<UniResponse<GameBoxLibraryDto[]>> => {
		const res = await adminApi.awd.listGameboxes(params);
		return res;
	};

	// Create via package import is not wired in GenericTable yet.
	// TODO: replace Add dialog with package zip upload → adminApi.awd.importGamebox.
	const createFn = async (
		_data: Partial<GameBoxLibraryDto>,
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		throw new Error(
			"请使用 package zip 导入（POST /api/admin/awd/gameboxes/import）；UI 上传待补",
		);
	};

	// Edit = identity metadata only
	const patchFn = async (
		data: Partial<GameBoxLibraryDto>,
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await adminApi.awd.updateGamebox(data.id!, {
			name: mutationData.name,
			category: mutationData.category,
			description: mutationData.description,
			hidden: mutationData.hidden,
		});
		return res;
	};

	return (
		<div>
			<banner.BannerComponent />
			<GenericTable
				subject={QUERY_KEY}
				columns={columns}
				queryFn={queryFn}
				createFn={createFn}
				patchFn={patchFn}
				mutationColumns={mutationColumns}
				mutationData={mutationData}
				columnActions={columnActions}
				filterKeys={["name", "safe_name", "category", "hidden"]}
				subtitle="GameBox = 身份 + immutable Revision（package import 构建镜像）"
			/>
		</div>
	);
}
