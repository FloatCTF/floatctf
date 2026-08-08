import { ActionList } from "@primer/react";
import { Button, Dialog, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import type { GameBoxConfigPayload, GameBoxLibraryDto } from "@/api/awd";
import { GenericTable, useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const QUERY_KEY = "AWDGameBoxes";

const EMPTY_CONFIG = {
	source_toml: "",
	image_ref: "",
	image_digest: null,
	username: "ctf",
	cpu_millis: 1000,
	memory_bytes: 512 * 1024 * 1024,
	pids_limit: 100,
	healthcheck: null,
	judge_script_name: null,
	judge_script_content: null,
	judge_args: null,
	judge_timeout_secs: null,
	judge_retry_interval_secs: null,
};

function RouteComponent() {
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});
	const [createOpen, setCreateOpen] = useState(false);
	const [editId, setEditId] = useState<string | null>(null);

	const q = useQuery({
		queryKey: [QUERY_KEY],
		queryFn: () => adminApi.awd.listGameboxes(),
	});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: [QUERY_KEY] });
	};

	const hide = useMutation({
		mutationFn: (gameboxId: string) => adminApi.awd.hideGamebox(gameboxId),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const items = q.data?.data ?? [];
	const editing = editId ? items.find((g) => g.id === editId) : null;

	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{ accessorKey: "name", header: "Name", field: "name", sortBy: true },
		{
			accessorKey: "safe_name",
			header: "Safe Name",
			field: "safe_name",
		},
		{
			accessorKey: "latest_revision",
			header: "Latest Revision",
			field: "latest_revision",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>
					{row.latest_revision
						? `rev ${row.latest_revision.revision_number}`
						: "(none)"}
				</span>
			),
		},
		{
			accessorKey: "image",
			header: "Image",
			field: "image",
			renderCell: (row: GameBoxLibraryDto) => (
				<span>
					{row.latest_revision?.image_ref ?? "-"}
					{row.latest_revision?.image_digest ? " 🔒" : ""}
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

	const customActions = (
		<Button variant="primary" onClick={() => setCreateOpen(true)}>
			New GameBox
		</Button>
	);

	const columnActions = (row: GameBoxLibraryDto) => (
		<ActionList>
			<ActionList.Item onSelect={() => setEditId(row.id)}>
				Edit (Rev N+1)
			</ActionList.Item>
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

	return (
		<div>
			<banner.BannerComponent />
			<GenericTable
				subject={QUERY_KEY}
				columns={columns}
				queryFn={adminApi.awd.listGameboxes}
				customActions={customActions}
				columnActions={columnActions}
				disableAdd
				disablePagination
				subtitle="GameBox = AWD 题目长期身份；编辑 = 创建不可变 Revision N+1"
			/>
			{createOpen && (
				<GameBoxFormDialog
					title="New GameBox"
					onClose={() => setCreateOpen(false)}
					onDone={() => {
						setCreateOpen(false);
						onDone();
					}}
				/>
			)}
			{editing && (
				<GameBoxFormDialog
					title={`Edit ${editing.name}（创建 Revision N+1）`}
					gameboxId={editing.id}
					initialName={editing.name}
					onClose={() => setEditId(null)}
					onDone={() => {
						setEditId(null);
						onDone();
					}}
				/>
			)}
		</div>
	);
}

function GameBoxFormDialog({
	title,
	gameboxId,
	initialName,
	onClose,
	onDone,
}: {
	title: string;
	gameboxId?: string;
	initialName?: string;
	onClose: () => void;
	onDone: () => void;
}) {
	const banner = useMsgBanner({});
	const [name, setName] = useState(initialName ?? "");
	const [config, setConfig] = useState<GameBoxConfigPayload>({ ...EMPTY_CONFIG });

	const create = useMutation({
		mutationFn: (body: {
			name: string;
			config: GameBoxConfigPayload;
		}) => adminApi.awd.createGamebox(body),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});
	const edit = useMutation({
		mutationFn: (body: { config: GameBoxConfigPayload }) =>
			adminApi.awd.editGameboxRevision(gameboxId!, body),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const submit = () => {
		if (gameboxId) {
			edit.mutate({ config });
		} else {
			create.mutate({ name, config });
		}
	};

	const setC = (k: string, v: unknown) =>
		setConfig((c) => ({ ...c, [k]: v }));

	return (
		<Dialog title={title} onClose={onClose}>
			<banner.BannerComponent />
			<div className="p-3">
				{!gameboxId && (
					<>
						<label className="mb-1 block text-sm">Name（展示名）</label>
						<TextInput
							className="mb-2 w-full"
							value={name}
							onChange={(e) => setName(e.target.value)}
						/>
					</>
				)}
				<label className="mb-1 block text-sm">Image</label>
				<TextInput
					className="mb-2 w-full"
					value={config.image_ref as string}
					onChange={(e) => setC("image_ref", e.target.value)}
					placeholder="registry.example.com/easy-web:v1"
				/>
				<label className="mb-1 block text-sm">
					Image Digest（生产前必须 pin，如 sha256:...）
				</label>
				<TextInput
					className="mb-2 w-full"
					value={(config.image_digest as string) ?? ""}
					onChange={(e) => setC("image_digest", e.target.value || null)}
					placeholder="sha256:（可选）"
				/>
				<label className="mb-1 block text-sm">SSH username</label>
				<TextInput
					className="mb-2 w-full"
					value={config.username as string}
					onChange={(e) => setC("username", e.target.value)}
				/>
				<label className="mb-1 block text-sm">
					Resources: cpu_millis / memory_bytes / pids_limit
				</label>
				<div className="mb-2 grid grid-cols-3 gap-1">
					<TextInput
						value={String(config.cpu_millis)}
						onChange={(e) => setC("cpu_millis", Number(e.target.value))}
					/>
					<TextInput
						value={String(config.memory_bytes)}
						onChange={(e) => setC("memory_bytes", Number(e.target.value))}
					/>
					<TextInput
						value={String(config.pids_limit)}
						onChange={(e) => setC("pids_limit", Number(e.target.value))}
					/>
				</div>
				<label className="mb-1 block text-sm">Judge script（判题属于 Revision，§9）</label>
				<TextInput
					className="mb-1 w-full"
					value={(config.judge_script_name as string) ?? ""}
					onChange={(e) => setC("judge_script_name", e.target.value || null)}
					placeholder="script name"
				/>
				<textarea
					className="mb-2 w-full border p-1 text-xs"
					rows={3}
					value={(config.judge_script_content as string) ?? ""}
					onChange={(e) => setC("judge_script_content", e.target.value || null)}
					placeholder="judge script content"
				/>
				<label className="mb-1 block text-sm">
					Judge timeout / retry（默认值；赛事可覆盖）
				</label>
				<div className="mb-3 grid grid-cols-2 gap-1">
					<TextInput
						value={config.judge_timeout_secs ? String(config.judge_timeout_secs) : ""}
						onChange={(e) =>
							setC("judge_timeout_secs", e.target.value ? Number(e.target.value) : null)
						}
						placeholder="timeout_secs"
					/>
					<TextInput
						value={
							config.judge_retry_interval_secs
								? String(config.judge_retry_interval_secs)
								: ""
						}
						onChange={(e) =>
							setC(
								"judge_retry_interval_secs",
								e.target.value ? Number(e.target.value) : null,
							)
						}
						placeholder="retry_interval_secs"
					/>
				</div>
				<Button
					block
					disabled={
						(gameboxId ? false : !name.trim()) ||
						!config.image_ref ||
						create.isPending ||
						edit.isPending
					}
					onClick={submit}
				>
					{gameboxId ? "Create Revision N+1" : "Create GameBox (Revision 1)"}
				</Button>
			</div>
		</Dialog>
	);
}
