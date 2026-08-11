import { CheckIcon } from "@primer/octicons-react";
import {
	ActionList,
	Button,
	ButtonGroup,
	Dialog,
	Stack,
	TextInput,
	ToggleSwitch,
	useConfirm,
} from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useCallback, useMemo, useRef, useState } from "react";

import { adminApi } from "@/api";
import type {
	GameBoxBuildResult,
	GameBoxCheckResult,
	GameBoxLibraryDto,
	GameBoxScanItem,
} from "@/api/awd";
import { type QueryParams, type UniResponse } from "@/api/axios";
import { GenericTable, useMsgBanner } from "@/components";
import { useSelectedRowIds } from "@/util";
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
	const [selectedRowIds, setSelectedRowIds] = useSelectedRowIds();
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: [QUERY_KEY] });
	};

	const hide = useMutation({
		mutationFn: (gameboxId: string) => adminApi.awd.hideGamebox(gameboxId),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	// 编辑表单：身份 + 可编辑运行参数（digest/镜像 pin/build 状态/judge/health 由导入决定，不在此列）
	const mutationData = useReactive<Partial<GameBoxLibraryDto>>({
		name: "",
		category: "other",
		description: "",
		hidden: false,
		username: "",
		cpu_millis: undefined,
		memory_bytes: undefined,
		pids_limit: undefined,
	});

	// 数字输入：空 → null（清空），非法输入保持不变
	const toNumOrNull = (v: string) => {
		if (v === "") return null;
		const n = Number(v);
		return Number.isNaN(n) ? undefined : n;
	};

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
		{
			header: "hidden",
			field: "hidden",
			render: (
				<Stack direction="horizontal" align="center">
					<ToggleSwitch
						aria-labelledby="default-toggle-label"
						checked={mutationData.hidden}
						onClick={() => {
							mutationData.hidden = !mutationData.hidden;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "username",
			field: "username",
			render: (
				<TextInput
					value={mutationData.username ?? ""}
					onChange={(e) => {
						mutationData.username = e.target.value;
					}}
					placeholder="容器内用户名（healthcheck/judge 执行用）；留空清空"
				/>
			),
		},
		{
			header: "recommended_cpu_millis",
			field: "cpu_millis",
			render: (
				<TextInput
					value={mutationData.cpu_millis ?? ""}
					onChange={(e) => {
						mutationData.cpu_millis = toNumOrNull(e.target.value);
					}}
					placeholder="CPU 限额（毫核），如 1000"
				/>
			),
		},
		{
			header: "recommended_memory_bytes",
			field: "memory_bytes",
			render: (
				<TextInput
					value={mutationData.memory_bytes ?? ""}
					onChange={(e) => {
						mutationData.memory_bytes = toNumOrNull(e.target.value);
					}}
					placeholder="内存限额（字节），如 536870912"
				/>
			),
		},
		{
			header: "recommended_pids_limit",
			field: "pids_limit",
			render: (
				<TextInput
					value={mutationData.pids_limit ?? ""}
					onChange={(e) => {
						mutationData.pids_limit = toNumOrNull(e.target.value);
					}}
					placeholder="进程数限额，如 100"
				/>
			),
		},
	];

	const columns = [
		{
			accessorKey: "name",
			header: "Name",
			field: "name",
			rowHeader: true,
			sortBy: true,
		},
		{
			accessorKey: "safe_name",
			header: "Safe Name",
			field: "safe_name",
		},
		{
			accessorKey: "version",
			header: "Version",
			field: "version",
			renderCell: (row: GameBoxLibraryDto) => <span>{row.version ?? "-"}</span>,
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
				<span>{row.hidden ? <CheckIcon size={16} /> : <></>}</span>
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

	// Edit = 身份 + 可编辑运行参数（只发 diff 中实际变化的字段；undefined 不发送）
	const patchFn = async (
		data: Partial<GameBoxLibraryDto>,
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await adminApi.awd.updateGamebox(data.id!, {
			name: data.name,
			category: data.category,
			description: data.description,
			hidden: data.hidden,
			username: data.username,
			recommended_cpu_millis: data.cpu_millis,
			recommended_memory_bytes: data.memory_bytes,
			recommended_pids_limit: data.pids_limit,
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
				removeFn={adminApi.awd.removeGamebox}
				mutationColumns={mutationColumns}
				mutationData={mutationData}
				columnActions={columnActions}
				customActions={
					<GameBoxActions gamebox_id_list={Array.from(selectedRowIds)} />
				}
				disableAdd={true}
				selectedRowIds={selectedRowIds}
				onSelectedRowIdsChange={setSelectedRowIds}
				filterKeys={["name", "safe_name", "category", "hidden"]}
				subtitle="GameBox = 身份 + 当前版本 package（导入严格递增 version，解压至 GAMEBOXES_DIR）"
			/>
		</div>
	);
}

// package zip 导入（与 admin/challenges.tsx 的 ImportButton 逻辑一致）
function GameBoxActions({ gamebox_id_list }: { gamebox_id_list?: string[] }) {
	return (
		<div className="flex gap-1">
			<ButtonGroup>
				<ImportButton />
				<CheckButton gamebox_id_list={gamebox_id_list} />
				<ScanButton />
			</ButtonGroup>
		</div>
	);
}

// 扫描 GAMEBOXES_DIR 登记未入库 package（结果弹窗展示）
export function ScanButton() {
	const [isOpen, setIsOpen] = useState(false);
	const [items, setItems] = useState<GameBoxScanItem[]>([]);
	const [loading, setLoading] = useState(false);
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});

	const handleScan = async () => {
		setLoading(true);
		try {
			const res = await adminApi.awd.scanGameboxes();
			setItems(res.data ?? []);
			setIsOpen(true);
			queryClient.invalidateQueries({ queryKey: [QUERY_KEY] });
		} catch (e) {
			banner.showBanner("critical", (e as Error).message || "扫描失败，请重试");
		} finally {
			setLoading(false);
		}
	};

	const columns = useMemo(
		() => [
			{
				accessorKey: "safe_name",
				header: "Safe Name",
				field: "safe_name",
				rowHeader: true,
			},
			{
				accessorKey: "name",
				header: "Name",
				field: "name",
			},
			{
				accessorKey: "version",
				header: "Version",
				field: "version",
			},
			{
				accessorKey: "status",
				header: "Status",
				field: "status",
				renderCell: (row: GameBoxScanItem) => (
					<span
						className={
							row.status === "error"
								? "text-red-500"
								: row.status === "added"
									? "text-green-600"
									: "text-gray-500"
						}
					>
						{row.status}
					</span>
				),
			},
			{
				accessorKey: "message",
				header: "Message",
				field: "message",
			},
		],
		[],
	);

	const table = useReactTable<GameBoxScanItem>({
		data: items,
		columns,
		getCoreRowModel: getCoreRowModel(),
		getRowId: (row) => row.safe_name,
	});

	return (
		<>
			{isOpen && (
				<Dialog title="Scan Results" onClose={() => setIsOpen(false)}>
					<Table.Container className="m-2">
						<DataTable
							aria-labelledby="repositories-default"
							// @ts-ignore
							columns={columns}
							// @ts-ignore
							getRowId={(row) => row.safe_name}
							// @ts-ignore
							data={table.getRowModel().rows.map((row) => row.original)}
						/>
					</Table.Container>
				</Dialog>
			)}
			<Button onClick={handleScan} disabled={loading}>
				{loading ? "Scanning..." : "Scan"}
			</Button>
		</>
	);
}

// 检查当前版本镜像本地可用 + package 目录已镜像（与 admin/challenges.tsx CheckButton 一致）
export function CheckButton({
	gamebox_id_list,
}: {
	gamebox_id_list?: string[];
}) {
	const idsToCheck: string[] | undefined =
		gamebox_id_list && gamebox_id_list.length > 0 ? gamebox_id_list : undefined;
	const [isOpen, setIsOpen] = useState(false);
	const buttonRef = useRef<HTMLButtonElement>(null);
	const onDialogClose = useCallback(() => setIsOpen(false), []);
	const banner = useMsgBanner({});

	// 数据获取
	const { data, isLoading } = useQuery({
		queryKey: ["GameBoxCheck", idsToCheck],
		queryFn: () => adminApi.awd.checkGameboxes(idsToCheck),
		enabled: isOpen,
		refetchOnWindowFocus: false,
		staleTime: 60_000, // 1 分钟内重复打开不会再请求
	});
	const queryClient = useQueryClient();
	const [building, setBuilding] = useState(false);

	const buildGameboxMutation = useMutation({
		mutationFn: (gamebox_id_list?: string[]) =>
			adminApi.awd.buildGameboxes(gamebox_id_list),
		onSuccess: (data) => {
			setBuilding(false);
			banner.showBanner(
				"success",
				data.data?.map((r) => r.message).join("\n") ?? "",
			);
			queryClient.invalidateQueries({ queryKey: ["GameBoxCheck"] });
		},
		onError: (e) => {
			setBuilding(false);
			banner.showBanner("critical", e.message);
		},
	});
	// 列定义只生成一次
	const columns = useMemo(
		() => [
			{
				accessorKey: "gamebox_name",
				header: "GameBox Name",
				field: "gamebox_name",
				rowHeader: true,
			},
			{
				accessorKey: "docker_image",
				header: "Docker Image",
				field: "docker_image",
				renderCell: (row: GameBoxCheckResult) => {
					return (
						<span>
							{row.docker_image ? (
								<CheckIcon />
							) : (
								<Button
									size="small"
									variant="primary"
									onClick={() => {
										setBuilding(true);
										buildGameboxMutation.mutate([row.id]);
									}}
									disabled={building}
								>
									Build
								</Button>
							)}
						</span>
					);
				},
			},
			{
				accessorKey: "package_dir",
				header: "Package Dir",
				field: "package_dir",
				renderCell: (row: GameBoxCheckResult) => {
					return <span>{row.package_dir ? <CheckIcon /> : <></>}</span>;
				},
			},
		],
		[buildGameboxMutation, building],
	);

	// 过滤出不可用的 gamebox
	const invalidData = useMemo(
		() => (data?.data ?? []).filter((r: GameBoxCheckResult) => !r.is_ok),
		[data],
	);

	// 表格实例
	const table = useReactTable({
		data: invalidData,
		columns,
		getCoreRowModel: getCoreRowModel(),
		getRowId: (row) => row.gamebox_name, // 👈 用 gamebox_name 保证唯一 key
	});

	if (isLoading) {
		return <div>Loading…</div>;
	}

	return (
		<>
			{isOpen && (
				<Dialog title="Unavailable GameBoxes" onClose={onDialogClose}>
					<Table.Container className="m-2">
						<DataTable
							aria-labelledby="repositories-default"
							// @ts-ignore
							columns={columns}
							getRowId={(row) => row.gamebox_name}
							data={table.getRowModel().rows.map((row) => row.original)}
						/>
					</Table.Container>
				</Dialog>
			)}
			<Button ref={buttonRef} onClick={() => setIsOpen(!isOpen)}>
				Check
			</Button>
		</>
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
		mutationFn: (vars: { file: File }) => adminApi.awd.importGamebox(vars.file),
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
