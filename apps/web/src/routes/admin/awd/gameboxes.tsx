import { PencilIcon, PlusIcon } from "@primer/octicons-react";
import { Button, Dialog, Spinner, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import type { GameBoxConfigPayload, GameBoxLibraryDto } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

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
		queryKey: ["admin-awd-gamebox-library"],
		queryFn: () => adminApi.awd.listGameboxes(),
	});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: ["admin-awd-gamebox-library"] });
	};

	const hide = useMutation({
		mutationFn: (gameboxId: string) => adminApi.awd.hideGamebox(gameboxId),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const items = q.data?.data ?? [];
	const editing = editId ? items.find((g) => g.id === editId) : null;

	return (
		<div>
			<banner.BannerComponent />
			<div className="mb-2 flex items-center justify-between">
				<h3 className="m-0">AWD GameBox Library（GameBox = AWD 题目长期身份）</h3>
				<Button onClick={() => setCreateOpen(true)}>
					New GameBox
				</Button>
			</div>
			{q.isLoading ? (
				<Spinner />
			) : (
				<table className="w-full text-sm">
					<thead>
						<tr className="border-b text-left">
							<th className="px-2 py-1">Name</th>
							<th className="px-2 py-1">safe_name</th>
							<th className="px-2 py-1">Latest Revision</th>
							<th className="px-2 py-1">Image</th>
							<th className="px-2 py-1">Hidden</th>
							<th className="px-2 py-1"></th>
						</tr>
					</thead>
					<tbody>
						{items.map((g: GameBoxLibraryDto) => (
							<tr key={g.id} className="border-b">
								<td className="px-2 py-1">{g.name}</td>
								<td className="px-2 py-1">{g.safe_name}</td>
								<td className="px-2 py-1">
									{g.latest_revision
										? `rev ${g.latest_revision.revision_number} · ${g.latest_revision.username}@${g.latest_revision.image_ref}`
										: "(none)"}
								</td>
								<td className="px-2 py-1">
									{g.latest_revision?.image_digest
										? g.latest_revision.image_ref.slice(0, 40)
										: "digest 未 pin"}
								</td>
								<td className="px-2 py-1">{g.hidden ? "yes" : "no"}</td>
								<td className="px-2 py-1">
									<Button
										size="small"
										onClick={() => setEditId(g.id)}
									>
										Edit (Rev N+1)
									</Button>
									<Button
										size="small"
										variant="danger"
										className="ml-1"
										onClick={() => {
											if (window.confirm(`Hide GameBox ${g.name}?`)) {
												hide.mutate(g.id);
											}
										}}
									>
										Hide
									</Button>
								</td>
							</tr>
						))}
					</tbody>
				</table>
			)}
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
