import { CheckIcon } from "@primer/octicons-react";
import {
	Button,
	ButtonGroup,
	Dialog,
	Stack,
	TextInput,
	ToggleSwitch,
} from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useCallback, useMemo, useRef, useState } from "react";

import { adminApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import { AdminRouteGuard } from "@/routes/admin/route";
import type { ChallengesListItem } from "@/types/challengeDto";
import { DatetimeToShow, useSelectedRowIds } from "@/util";

export const Route = createFileRoute("/admin/challenges")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const banner = useMsgBanner({});
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{ accessorKey: "name", header: "Name", field: "name", sortBy: true },
		{
			accessorKey: "category",
			header: "Category",
			field: "category",
			sortBy: true,
		},
		{
			accessorKey: "version",
			header: "Version",
			field: "version",
			renderCell: (row: ChallengesListItem) => {
				return (
					<span>
						{row.version ?? "—"}{" "}
						{row.build_status === "ready" ? (
							<CheckIcon />
						) : row.build_status ? (
							<span className="text-red-500">{row.build_status}</span>
						) : null}
					</span>
				);
			},
		},
		{
			accessorKey: "image_ref",
			header: "Image",
			field: "image_ref",
			renderCell: (row: ChallengesListItem) => {
				return (
					<span>
						{row.image_ref ?? "—"}
						{row.image_repo_digest ? " 🔒" : ""}
					</span>
				);
			},
		},
		{
			accessorKey: "attachment",
			header: "Attachment",
			field: "attachment",
			renderCell: (row: ChallengesListItem) => {
				return (
					<span>
						{row.attachment ? (
							<span title={row.attachment.path}>{row.attachment.name}</span>
						) : (
							<></>
						)}
					</span>
				);
			},
		},
		{
			accessorKey: "hidden",
			header: "Hidden",
			field: "hidden",

			renderCell: (row: ChallengesListItem) => {
				return <span>{row.hidden ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
		},
		{
			accessorKey: "created_at",
			header: "Created At",
			field: "created_at",
			renderCell: (row: ChallengesListItem) => {
				return <span>{DatetimeToShow(row.created_at)}</span>;
			},
		},
		{
			accessorKey: "updated_at",
			header: "Updated At",
			field: "updated_at",
			renderCell: (row: ChallengesListItem) => {
				return <span>{DatetimeToShow(row.updated_at)}</span>;
			},
		},
	];

	const mutationChallenge = useReactive<Partial<ChallengesListItem>>({
		name: "",
		category: "",
		description: "",
		hidden: true,
		static_flag_value: "",
		container_port: undefined,
		recommended_cpu_millis: undefined,
		recommended_memory_bytes: undefined,
		recommended_pids_limit: undefined,
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
					value={mutationChallenge.name}
					onChange={(e) => {
						mutationChallenge.name = e.target.value;
					}}
				/>
			),
		},
		{
			header: "category",
			field: "category",
			render: (
				<TextInput
					value={mutationChallenge.category}
					onChange={(e) => {
						mutationChallenge.category = e.target.value;
					}}
				/>
			),
		},
		{
			header: "description",
			field: "description",
			render: (
				<TextInput
					value={mutationChallenge.description}
					onChange={(e) => {
						mutationChallenge.description = e.target.value;
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
						checked={mutationChallenge.hidden}
						onClick={() => {
							mutationChallenge.hidden = !mutationChallenge.hidden;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "static_flag_value",
			field: "static_flag_value",
			render: (
				<TextInput
					value={mutationChallenge.static_flag_value ?? ""}
					onChange={(e) => {
						mutationChallenge.static_flag_value = e.target.value;
					}}
					placeholder="仅 flag_type=static 时生效；留空表示清空"
				/>
			),
		},
		{
			header: "container_port",
			field: "container_port",
			render: (
				<TextInput
					value={mutationChallenge.container_port ?? ""}
					onChange={(e) => {
						mutationChallenge.container_port = toNumOrNull(
							e.target.value,
						);
					}}
					placeholder="容器端口 1-65535；留空表示无 docker 运行时"
				/>
			),
		},
		{
			header: "recommended_cpu_millis",
			field: "recommended_cpu_millis",
			render: (
				<TextInput
					value={mutationChallenge.recommended_cpu_millis ?? ""}
					onChange={(e) => {
						mutationChallenge.recommended_cpu_millis =
							toNumOrNull(e.target.value) ?? undefined;
					}}
					placeholder="CPU 限额（毫核），如 500"
				/>
			),
		},
		{
			header: "recommended_memory_bytes",
			field: "recommended_memory_bytes",
			render: (
				<TextInput
					value={mutationChallenge.recommended_memory_bytes ?? ""}
					onChange={(e) => {
						mutationChallenge.recommended_memory_bytes =
							toNumOrNull(e.target.value) ?? undefined;
					}}
					placeholder="内存限额（字节），如 268435456"
				/>
			),
		},
		{
			header: "recommended_pids_limit",
			field: "recommended_pids_limit",
			render: (
				<TextInput
					value={mutationChallenge.recommended_pids_limit ?? ""}
					onChange={(e) => {
						mutationChallenge.recommended_pids_limit =
							toNumOrNull(e.target.value) ?? undefined;
					}}
					placeholder="进程数限额，如 100"
				/>
			),
		},
	];
	const [selectedRowIds, setSelectedRowIds] = useSelectedRowIds();

	const custom_actions = (
		<div className="flex gap-1">
			<ButtonGroup>
				<ImportButton />
				<CheckButton challenge_id_list={Array.from(selectedRowIds)} />
				<ScanButton />
			</ButtonGroup>
		</div>
	);
	const filterKeys = [
		"id",
		"name",
		"safe_name",
		"category",
		"hidden",
		"description",
	];

	return (
		<GenericTable
			subject="Challenges"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={adminApi.challenges.fetch}
			createFn={adminApi.challenges.create}
			removeFn={adminApi.challenges.remove}
			patchFn={adminApi.challenges.patch}
			mutationColumns={mutationColumns}
			mutationData={mutationChallenge}
			customActions={custom_actions}
			disableAdd={true}
			selectedRowIds={selectedRowIds}
			onSelectedRowIdsChange={setSelectedRowIds}
		/>
	);
}

function ImportButton() {
	const banner = useMsgBanner({});
	const inputRef = useRef<HTMLInputElement>(null);
	const [file, setFile] = useState<File | null>(null);
	const [message, setMessage] = useState<null | {
		type: "success" | "error";
		text: string;
	}>(null);
	const queryClient = useQueryClient();

	const importMutation = useMutation({
		mutationFn: (vars: { file: File }) =>
			adminApi.challenges.importChallenge(vars.file),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["Challenges"] });
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
			{/* 左边：提示 / 文件名 / 复选框 / 上传按钮 */} {/* 全局提示 */}
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
			{/* 右边：导入按钮 */}
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

export type ChallengeCheckResult = {
	id: string;
	challenge_name: string;
	is_ok: boolean;
	docker_image: boolean;
	attachment: boolean;
};
export type BuildChallengeResult = {
	challenge_name: string;
	is_ok: boolean;
	message: string;
};
export type ChallengeScanItem = {
	safe_name: string;
	name: string | null;
	version: string | null;
	status: "added" | "skipped" | "error";
	message: string;
};
export type ImportChallengeResponse = {
	challenge: ChallengesListItem;
};

// 扫描 CHALLENGES_DIR 登记未入库 package（结果弹窗展示）
export function ScanButton() {
	const [isOpen, setIsOpen] = useState(false);
	const [items, setItems] = useState<ChallengeScanItem[]>([]);
	const [loading, setLoading] = useState(false);
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});

	const handleScan = async () => {
		setLoading(true);
		try {
			const res = await adminApi.challenges.scanChallenges();
			setItems(res.data ?? []);
			setIsOpen(true);
			queryClient.invalidateQueries({ queryKey: ["Challenges"] });
		} catch (e) {
			banner.showBanner(
				"critical",
				(e as Error).message || "扫描失败，请重试",
			);
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
				renderCell: (row: ChallengeScanItem) => (
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

	const table = useReactTable<ChallengeScanItem>({
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
							data={table
								.getRowModel()
								.rows.map((row) => row.original)}
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

export function CheckButton({
	challenge_id_list,
}: {
	challenge_id_list?: string[];
}) {
	const idsToCheck: string[] | undefined =
		challenge_id_list && challenge_id_list.length > 0
			? challenge_id_list
			: undefined;
	const [isOpen, setIsOpen] = useState(false);
	const buttonRef = useRef<HTMLButtonElement>(null);
	const onDialogClose = useCallback(() => setIsOpen(false), []);
	const banner = useMsgBanner({});

	// 数据获取
	const { data, isLoading } = useQuery({
		queryKey: ["ChallengeCheck", idsToCheck],
		queryFn: () => adminApi.challenges.checkChallenges(idsToCheck),
		enabled: isOpen,
		refetchOnWindowFocus: false,
		staleTime: 60_000, // 1 分钟内重复打开不会再请求
	});
	const queryClient = useQueryClient();
	const [building, setBuilding] = useState(false);

	const buildChallengeMutation = useMutation({
		mutationFn: (challenge_id_list?: string[]) =>
			adminApi.challenges.buildChallenges(challenge_id_list),
		onSuccess: (data) => {
			setBuilding(false);
			banner.showBanner(
				"success",
				data.data?.map((r) => r.message).join("\n") ?? "",
			);
			queryClient.invalidateQueries({ queryKey: ["ChallengeCheck"] });
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
				accessorKey: "challenge_name",
				header: "Challenge Name",
				field: "challenge_name",
				rowHeader: true,
			},
			{
				accessorKey: "docker_image",
				header: "Docker Image",
				field: "docker_image",
				renderCell: (row: ChallengeCheckResult) => {
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
										buildChallengeMutation.mutate([row.id]);
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
				accessorKey: "attachment",
				header: "Attachment",
				field: "attachment",
				renderCell: (row: ChallengeCheckResult) => {
					return <span>{row.attachment ? <CheckIcon /> : <></>}</span>;
				},
			},
		],
		[buildChallengeMutation, building],
	);

	// 过滤出不可用的挑战
	const invalidData = useMemo(
		() => (data?.data ?? []).filter((r: ChallengeCheckResult) => !r.is_ok),
		[data],
	);

	// 表格实例
	const table = useReactTable({
		data: invalidData,
		columns,
		getCoreRowModel: getCoreRowModel(),
		getRowId: (row) => row.challenge_name, // 👈 用 challenge_name 保证唯一 key
	});

	if (isLoading) {
		return <div>Loading…</div>;
	}

	return (
		<>
			{isOpen && (
				<Dialog title="Unavailable Challenges" onClose={onDialogClose}>
					<Table.Container className="m-2">
						<DataTable
							aria-labelledby="repositories-default"
							// @ts-ignore
							columns={columns}
							getRowId={(row) => row.challenge_name}
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
