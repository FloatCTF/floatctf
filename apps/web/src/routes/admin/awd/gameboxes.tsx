import { ActionList, Button, TextInput, useConfirm } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useRef, useState } from "react";

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

	// Create via package import only（与 Challenges 页一致，无 Add 表单）
	const createFn = undefined;

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
				customActions={<ImportButton />}
				disableAdd={true}
				filterKeys={["name", "safe_name", "category", "hidden"]}
				subtitle="GameBox = 身份 + 当前版本 package（导入严格递增 version，解压至 GAMEBOXES_DIR）"
			/>
		</div>
	);
}

// package zip 导入（与 admin/challenges.tsx 的 ImportButton 逻辑一致）
function ImportButton() {
	const inputRef = useRef<HTMLInputElement>(null);
	const [file, setFile] = useState<File | null>(null);
	const [message, setMessage] = useState<null | {
		type: "success" | "error";
		text: string;
	}>(null);
	const queryClient = useQueryClient();

	const importMutation = useMutation({
		mutationFn: (vars: { file: File }) =>
			adminApi.awd.importGamebox(vars.file),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [QUERY_KEY] });
			setMessage({ type: "success", text: "上传成功 🎉" });
			setFile(null);

			// 3 秒后清理提示
			setTimeout(() => setMessage(null), 3000);
		},
		onError: (e) => {
			const msg =
				(e as AxiosError<{ message: string }>)?.response?.data?.message ||
				(e as Error).message ||
				"上传失败，请重试";
			setMessage({ type: "error", text: msg });
			setTimeout(() => setMessage(null), 6000);
		},
	});

	const handleClick = () => inputRef.current?.click();

	const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		const selected = e.target.files?.[0];
		if (!selected) return;
		if (!selected.name.toLowerCase().endsWith(".zip")) {
			setMessage({ type: "error", text: "只支持 ZIP 文件" });
			setTimeout(() => setMessage(null), 3000);
			return;
		}
		setFile(selected);
		e.target.value = "";
	};

	const handleUpload = () => {
		if (!file) return;
		importMutation.mutate({ file });
	};

	return (
		<div className="flex items-center gap-3">
			{/* 全局提示 */}
			{message && (
				<span
					className={`ml-2 text-sm ${
						message.type === "success" ? "text-green-600" : "text-red-500"
					}`}
				>
					{message.text}
				</span>
			)}
			{file && (
				<div className="flex items-center gap-3">
					<span className="text-sm text-gray-500">{file.name}</span>
					<Button
						onClick={handleUpload}
						disabled={importMutation.isPending}
						variant="primary"
					>
						{importMutation.isPending ? "Uploading..." : "Start Upload"}
					</Button>
				</div>
			)}
			{/* 导入按钮 */}
			<Button variant="primary" onClick={handleClick}>
				Import
			</Button>
			<input
				type="file"
				accept=".zip"
				ref={inputRef}
				className="hidden"
				onChange={handleChange}
			/>
		</div>
	);
}
