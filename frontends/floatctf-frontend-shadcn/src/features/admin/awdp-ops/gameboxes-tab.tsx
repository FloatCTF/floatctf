/**
 * 「GameBox 挂载」标签：AWDP 赛事与 GameBox 的挂载关系（attach / detach / list）。
 *
 * 已核实的后端语义：
 * - attach 后端要求 GameBox `build_status == "ready"` **且**具备完整 [awdp] capability
 *   （存在 `awdp_source_artifact_key`），并受 UNIQUE(event_id, gamebox_id) 约束；
 * - 比赛进行中（Break / Fix）新挂载且未隐藏时，会为全部参与者自动启动该 GameBox 实例（幂等）；
 * - detach 是直接删除挂载行：`awdp_instances` / `awdp_breaks` / `awdp_score_events` 都以
 *   `event_gamebox_id` 外键 **ON DELETE CASCADE**，因此该题的实例、Break 记录与计分账本
 *   会一并消失。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Boxes, Link2, Plus, Trash2 } from "lucide-react";

import type { AwdpAdminEventGameBoxDto } from "@floatctf/sdk";

import { call, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, InlineError, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { Switch } from "~/components/ui/switch";
import { formatBytes } from "~/lib/format";

import { useAwdpEventGameboxes, useAwdpGameboxLibrary } from "./queries";

function AttachGameboxSheet({
	eventId,
	attachedIds,
	onClose,
}: {
	eventId: string;
	attachedIds: Set<string>;
	onClose: () => void;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const libraryQuery = useAwdpGameboxLibrary(true);
	const [gameboxId, setGameboxId] = useState("");
	const [hidden, setHidden] = useState(false);

	const mutation = useMutation({
		mutationFn: (input: { gameboxId: string; hidden: boolean }) =>
			call<AwdpAdminEventGameBoxDto>(
				client.awdp.admin.attachGamebox(eventId, input.gameboxId, input.hidden),
				"挂载 GameBox",
			),
		onSuccess: (row) => {
			toast.success(`已挂载「${row.name}」`);
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminGameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminInstances(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("挂载 GameBox 失败", error),
	});

	const ready = (libraryQuery.data?.items ?? []).filter(
		(item) => item.build_status === "ready" && !attachedIds.has(item.id),
	);

	const submit = async (event: FormEvent) => {
		event.preventDefault();
		if (gameboxId === "") return;
		const selected = ready.find((item) => item.id === gameboxId);
		const ok = await confirm({
			title: `挂载「${selected?.name ?? gameboxId}」？`,
			description: "挂载后该 GameBox 出现在本赛事（选手端与实例视图）。",
			consequences: [
				"复制 GameBox 推荐资源（CPU / 内存 / PIDs）到赛事挂载行",
				"若比赛正在进行（Break / Fix）且未隐藏，会为全部参与者自动启动该 GameBox 实例",
				"标记为隐藏则不为参与者启动，也不会出现在选手端",
				"后端要求 GameBox 已构建完成且具备完整 [awdp] capability，否则会拒绝并给出原因",
			],
			confirmText: "挂载",
		});
		if (ok) mutation.mutate({ gameboxId, hidden });
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="挂载 GameBox"
			description="仅列出已构建成功（ready）且尚未挂载的 GameBox；[awdp] capability 由后端校验。"
			footer={
				<FormFooter
					onCancel={onClose}
					formId="awdp-attach-form"
					submitLabel="挂载"
					isPending={mutation.isPending}
					disabled={gameboxId === ""}
				/>
			}
		>
			<form id="awdp-attach-form" onSubmit={(event) => void submit(event)} className="space-y-4">
				<Field label="GameBox" htmlFor="awdp-attach-gamebox" required>
					<Select value={gameboxId} onValueChange={setGameboxId} disabled={libraryQuery.isPending}>
						<SelectTrigger id="awdp-attach-gamebox" aria-label="选择 GameBox">
							<SelectValue
								placeholder={libraryQuery.isPending ? "加载 GameBox 库…" : "选择 GameBox…"}
							/>
						</SelectTrigger>
						<SelectContent>
							{ready.map((item) => (
								<SelectItem key={item.id} value={item.id}>
									{item.name}（{item.safe_name}）
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</Field>
				{libraryQuery.isError ? (
					<InlineError error={libraryQuery.error} />
				) : !libraryQuery.isPending && ready.length === 0 ? (
					<p className="text-xs text-muted-foreground">
						没有可挂载的 GameBox：库中没有 ready 状态的靶机（或全部已挂载）。
					</p>
				) : null}
				<Field
					label="对选手隐藏"
					htmlFor="awdp-attach-hidden"
					hint="隐藏的 GameBox 不会为参与者自动启动实例"
				>
					<Switch id="awdp-attach-hidden" checked={hidden} onCheckedChange={setHidden} />
				</Field>
			</form>
		</FormSheet>
	);
}

export function AwdpGameboxesTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const gameboxesQuery = useAwdpEventGameboxes(eventId);
	const [attaching, setAttaching] = useState(false);

	const detach = useMutation({
		mutationFn: (row: AwdpAdminEventGameBoxDto) =>
			callVoid(client.awdp.admin.detachGamebox(eventId, row.id), "解除挂载"),
		onSuccess: (_result, row) => {
			toast.success(`已解除「${row.name}」的挂载`);
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminGameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminInstances(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminScores(eventId) });
		},
		onError: (error) => toast.apiError("解除挂载失败", error),
	});

	const columns: DataTableColumn<AwdpAdminEventGameBoxDto>[] = [
		{
			id: "name",
			header: "GameBox",
			sortValue: (row) => row.name,
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate font-medium">{row.name}</p>
					<p className="text-xs text-muted-foreground">
						{row.safe_name} · {row.category}
					</p>
				</div>
			),
		},
		{
			id: "awdp_capable",
			header: "[awdp] 能力",
			cell: (row) => (
				<BooleanPill
					value={row.awdp_capable}
					trueText="完整"
					falseText="缺失"
					falseTone="danger"
				/>
			),
		},
		{
			id: "build_status",
			header: "构建状态",
			cell: (row) => row.build_status ?? "—",
			hideBelow: "md",
		},
		{
			id: "enabled",
			header: "启用",
			cell: (row) => <BooleanPill value={row.enabled} trueText="启用" falseText="停用" />,
			hideBelow: "md",
		},
		{
			id: "hidden",
			header: "选手可见",
			cell: (row) => (
				<BooleanPill value={!row.hidden} trueText="可见" falseText="已隐藏" falseTone="warning" />
			),
		},
		{
			id: "resources",
			header: "资源（CPU / 内存 / PIDs）",
			hideBelow: "lg",
			cell: (row) => (
				<MonoText>
					{row.cpu_millis}m / {formatBytes(row.memory_bytes)} / {row.pids_limit}
				</MonoText>
			),
		},
		{
			id: "awdp_source_code_dir",
			header: "源码目录",
			hideBelow: "xl",
			cell: (row) => <MonoText>{row.awdp_source_code_dir ?? "—"}</MonoText>,
		},
	];

	return (
		<SectionCard
			title="赛事 GameBox 挂载"
			description="AWDP 需要完整 [awdp] capability 的 GameBox；比赛进行中新挂载会为参与者自动启动实例。"
			actions={
				<Button size="sm" onClick={() => setAttaching(true)}>
					<Plus /> 挂载 GameBox
				</Button>
			}
		>
			<QueryState
				query={gameboxesQuery}
				skeleton={<TableSkeleton rows={4} columns={5} />}
				isEmpty={(rows) => rows.length === 0}
				empty={
					<EmptyBlock
						title="尚未挂载任何 GameBox"
						description="AWDP 赛事的题目就是 GameBox：挂载后参与者才会看到并启动实例。"
						icon={<Boxes className="size-5" />}
						action={
							<Button size="sm" onClick={() => setAttaching(true)}>
								<Plus /> 挂载 GameBox
							</Button>
						}
					/>
				}
			>
				{(rows) => (
					<DataTable
						data={rows}
						getRowId={(row) => row.id}
						columns={columns}
						mobileCard={(row) => (
							<div className="space-y-1">
								<p className="text-sm font-medium">{row.name}</p>
								<p className="text-xs text-muted-foreground">
									{row.safe_name} · {row.category} · {row.build_status ?? "—"}
								</p>
								<div className="flex flex-wrap gap-1 pt-1">
									<BooleanPill
										value={row.awdp_capable}
										trueText="[awdp] 完整"
										falseText="[awdp] 缺失"
										falseTone="danger"
									/>
									<BooleanPill value={!row.hidden} trueText="可见" falseText="已隐藏" falseTone="warning" />
								</div>
							</div>
						)}
						rowActions={(row) => (
							<Button
								variant="ghost"
								size="icon-sm"
								disabled={detach.isPending}
								aria-label={`解除 ${row.name} 的挂载`}
								title="解除挂载"
								onClick={async () => {
									const ok = await confirm({
										title: `解除「${row.name}」的挂载？（会级联删除数据）`,
										description:
											"挂载行被删除时，以 event_gamebox_id 为外键的数据会按 ON DELETE CASCADE 一并删除。",
										consequences: [
											"该 GameBox 在本赛事的实例记录被级联删除（容器不会自动停止）",
											"该题的 Break 记录与计分账本（score_events）被级联删除，积分榜历史随之变化",
											"选手端不再看到该题目；重新挂载不会恢复已删除的记录",
										],
										tone: "danger",
										confirmText: "解除挂载",
									});
									if (ok) detach.mutate(row);
								}}
							>
								<Trash2 className="text-destructive" />
							</Button>
						)}
					/>
				)}
			</QueryState>

			{attaching ? (
				<AttachGameboxSheet
					eventId={eventId}
					attachedIds={new Set((gameboxesQuery.data ?? []).map((row) => row.gamebox_id))}
					onClose={() => setAttaching(false)}
				/>
			) : null}

			<p className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
				<Link2 className="size-3" />
				挂载只决定赛事可见的题目；实例由参与者启动（比赛进行中挂载会自动启动）。
			</p>
		</SectionCard>
	);
}
