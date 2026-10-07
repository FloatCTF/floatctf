import { CheckIcon } from "@primer/octicons-react";
import {
	Button,
	ButtonGroup,
	Dialog,
	Label,
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
import type { ChallengesListItem } from "@floatctf/sdk";
import { useSelectedRowIds } from "@/util";

export const Route = createFileRoute("/admin/challenges")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const banner = useMsgBanner({});
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{ accessorKey: "name", header: "名称", field: "name", sortBy: true },
		{
			accessorKey: "safe_name",
			header: "安全名",
			field: "safe_name",
		},
		{
			accessorKey: "category",
			header: "分类",
			field: "category",
			sortBy: true,
		},
		{
			accessorKey: "version",
			header: "版本",
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
			header: "镜像",
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
			accessorKey: "hidden",
			header: "隐藏",
			field: "hidden",

			renderCell: (row: ChallengesListItem) => {
				return <span>{row.hidden ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
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
			header: "名称",
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
			header: "分类",
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
			header: "描述",
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
			header: "隐藏",
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
			header: "容器端口",
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
			header: "推荐 CPU（毫核）",
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
			header: "推荐内存（字节）",
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
			header: "推荐 PID 上限",
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
				导入
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
				header: "安全名",
				field: "safe_name",
				rowHeader: true,
			},
			{
				accessorKey: "name",
				header: "名称",
				field: "name",
			},
			{
				accessorKey: "version",
				header: "版本",
				field: "version",
			},
			{
				accessorKey: "status",
				header: "状态",
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
				header: "消息",
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
				{loading ? "Scanning..." : "扫描"}
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
			const results = data.data ?? [];
			const failed = results.filter((r) => !r.is_ok);
			const empty = results.length === 0;
			if (failed.length > 0) {
				// is_ok=false 必须报错，不能画成成功（否则点击 Build 像"没反应"）
				banner.showBanner(
					"critical",
					failed.map((r) => `${r.challenge_name}: ${r.message}`).join("\n"),
				);
			} else if (empty) {
				// /build 只对 build_status=ready 的题目生效；没有可构建项必须说明原因
				banner.showBanner(
					"warning",
					"没有可构建的镜像：请先导入题目包，并确认 build_status 为 ready（否则请查看 build_error）",
				);
			} else {
				banner.showBanner(
					"success",
					results.map((r) => r.message).join("\n") || "ok",
				);
			}
			queryClient.invalidateQueries({ queryKey: ["ChallengeCheck"] });
			queryClient.invalidateQueries({ queryKey: ["Challenges"] });
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
				header: "题目名称",
				field: "challenge_name",
				rowHeader: true,
			},
			{
				accessorKey: "docker_image",
				header: "Docker 镜像",
				field: "docker_image",
				renderCell: (row: ChallengeCheckResult) => {
					// static / attachment-only 题目没有镜像：不显示 Build（永远无法构建）
					if (row.static_content) {
						return (
							<Label size="small" variant="secondary">
								static
							</Label>
						);
					}
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
				header: "附件",
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
				检查
			</Button>
		</>
	);
}

import type {
	ChallengeCheckResult,
	BuildChallengeResult,
	ChallengeScanItem,
	ImportChallengeResponse,
} from "@floatctf/sdk";
export type {
	ChallengeCheckResult,
	BuildChallengeResult,
	ChallengeScanItem,
	ImportChallengeResponse,
};
