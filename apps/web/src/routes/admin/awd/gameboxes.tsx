import { ActionList, TextInput, Textarea } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import { type QueryParams, type UniResponse } from "@/api/axios";
import type {
	GameBoxConfigPayload,
	GameBoxLibraryDto,
} from "@/api/awd";
import { GenericTable, useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

export const QUERY_KEY = "AWDGameBoxes";

/** 库行 = DTO + latest_revision 展开（内置 Edit 对话框按顶级字段回填，同 Challenges 页）。 */
export type FlattenedGameBox = GameBoxLibraryDto & {
	revision_number?: number | null;
	image_ref?: string;
	image_digest?: string | null;
	username?: string;
	cpu_millis?: number;
	memory_bytes?: number;
	pids_limit?: number;
	judge_script_name?: string | null;
	judge_script_content?: string | null;
	judge_timeout_secs?: number | null;
	judge_retry_interval_secs?: number | null;
	spec_digest?: string;
};

export const flattenGameBox = (g: GameBoxLibraryDto): FlattenedGameBox => ({
	...g,
	...(g.latest_revision ?? {}),
});

const CONFIG_KEYS = [
	"source_toml",
	"image_ref",
	"image_digest",
	"username",
	"cpu_millis",
	"memory_bytes",
	"pids_limit",
	"healthcheck",
	"judge_script_name",
	"judge_script_content",
	"judge_args",
	"judge_timeout_secs",
	"judge_retry_interval_secs",
] as const;

/** 从（展平的）表单/行数据里挑出 config 字段。 */
export const extractConfig = (
	d: Record<string, unknown>,
): GameBoxConfigPayload => {
	const cfg: Record<string, unknown> = {};
	for (const k of CONFIG_KEYS) {
		if (k in d) cfg[k] = d[k];
	}
	return cfg as GameBoxConfigPayload;
};

function RouteComponent() {
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

	// 内置 Add/Edit 对话框的表单数据（同 Challenges 页 mutationData 模式）
	const mutationData = useReactive<Partial<FlattenedGameBox>>({
		name: "",
		image_ref: "",
		image_digest: null,
		username: "ctf",
		cpu_millis: 1000,
		memory_bytes: 512 * 1024 * 1024,
		pids_limit: 100,
		judge_script_name: null,
		judge_script_content: null,
		judge_timeout_secs: null,
		judge_retry_interval_secs: null,
	});

	const mutationColumns = [
		{
			header: "name",
			field: "name",
			render: (
				<TextInput
					value={mutationData.name}
					onChange={(e) => {
						mutationData.name = e.target.value;
					}}
				/>
			),
		},
		{
			header: "image_ref",
			field: "image_ref",
			render: (
				<TextInput
					value={mutationData.image_ref}
					onChange={(e) => {
						mutationData.image_ref = e.target.value;
					}}
					placeholder="registry.example.com/easy-web:v1"
				/>
			),
		},
		{
			header: "image_digest",
			field: "image_digest",
			render: (
				<TextInput
					value={mutationData.image_digest ?? ""}
					onChange={(e) => {
						mutationData.image_digest = e.target.value || null;
					}}
					placeholder="sha256:…（生产前 pin）"
				/>
			),
		},
		{
			header: "username",
			field: "username",
			render: (
				<TextInput
					value={mutationData.username}
					onChange={(e) => {
						mutationData.username = e.target.value;
					}}
				/>
			),
		},
		{
			header: "cpu_millis",
			field: "cpu_millis",
			render: (
				<TextInput
					value={String(mutationData.cpu_millis ?? 1000)}
					onChange={(e) => {
						mutationData.cpu_millis = Number(e.target.value);
					}}
				/>
			),
		},
		{
			header: "memory_bytes",
			field: "memory_bytes",
			render: (
				<TextInput
					value={String(mutationData.memory_bytes ?? 0)}
					onChange={(e) => {
						mutationData.memory_bytes = Number(e.target.value);
					}}
				/>
			),
		},
		{
			header: "pids_limit",
			field: "pids_limit",
			render: (
				<TextInput
					value={String(mutationData.pids_limit ?? 100)}
					onChange={(e) => {
						mutationData.pids_limit = Number(e.target.value);
					}}
				/>
			),
		},
		{
			header: "judge_script_name",
			field: "judge_script_name",
			render: (
				<TextInput
					value={mutationData.judge_script_name ?? ""}
					onChange={(e) => {
						mutationData.judge_script_name = e.target.value || null;
					}}
					placeholder="可选"
				/>
			),
		},
		{
			header: "judge_script_content",
			field: "judge_script_content",
			render: (
				<Textarea
					value={mutationData.judge_script_content ?? ""}
					onChange={(e) => {
						mutationData.judge_script_content = e.target.value || null;
					}}
					placeholder="可选"
				/>
			),
		},
		{
			header: "judge_timeout_secs",
			field: "judge_timeout_secs",
			render: (
				<TextInput
					value={mutationData.judge_timeout_secs ?? ""}
					onChange={(e) => {
						mutationData.judge_timeout_secs = e.target.value
							? Number(e.target.value)
							: null;
					}}
					placeholder="可选"
				/>
			),
		},
		{
			header: "judge_retry_interval_secs",
			field: "judge_retry_interval_secs",
			render: (
				<TextInput
					value={mutationData.judge_retry_interval_secs ?? ""}
					onChange={(e) => {
						mutationData.judge_retry_interval_secs = e.target.value
							? Number(e.target.value)
							: null;
					}}
					placeholder="可选"
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
			accessorKey: "revision_number",
			header: "Latest Revision",
			field: "revision_number",
			renderCell: (row: FlattenedGameBox) => (
				<span>{row.revision_number ? `rev ${row.revision_number}` : "(none)"}</span>
			),
		},
		{
			accessorKey: "image_ref",
			header: "Image",
			field: "image_ref",
			renderCell: (row: FlattenedGameBox) => (
				<span>
					{row.image_ref ?? "-"}
					{row.image_digest ? " 🔒" : ""}
				</span>
			),
		},
		{
			accessorKey: "hidden",
			header: "Hidden",
			field: "hidden",
			renderCell: (row: FlattenedGameBox) => (
				<span>{row.hidden ? "yes" : "no"}</span>
			),
		},
	];

	const columnActions = (row: FlattenedGameBox) => (
		<ActionList>
			<ActionList.Item
				variant="danger"
				onSelect={() => {
					if (window.confirm(`Hide GameBox ${row.name}?（被赛事引用时将被拒绝）`)) {
						hide.mutate(row.id);
					}
				}}
			>
				Hide
			</ActionList.Item>
		</ActionList>
	);

	const queryFn = async (
		params?: QueryParams,
	): Promise<UniResponse<FlattenedGameBox[]>> => {
		const res = await adminApi.awd.listGameboxes(params);
		return { ...res, data: (res.data ?? []).map(flattenGameBox) };
	};
	const createFn = async (
		data: Partial<FlattenedGameBox>,
	): Promise<UniResponse<FlattenedGameBox>> => {
		const res = await adminApi.awd.createGamebox({
			name: data.name ?? "",
			config: extractConfig(data as Record<string, unknown>),
		});
		return { ...res, data: flattenGameBox(res.data ?? ({} as GameBoxLibraryDto)) };
	};
	// Edit row = 全量 config 提交；无改动时 digest 相同 → 后端去重不建新 Revision（§36）
	const patchFn = async (
		data: Partial<FlattenedGameBox>,
	): Promise<UniResponse<FlattenedGameBox>> => {
		const res = await adminApi.awd.editGameboxRevision(data.id!, {
			config: extractConfig(mutationData as unknown as Record<string, unknown>),
		});
		return { ...res, data: flattenGameBox(res.data ?? ({} as GameBoxLibraryDto)) };
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
				subtitle="GameBox = AWD 题目长期身份；Edit = 创建不可变 Revision N+1"
			/>
		</div>
	);
}
