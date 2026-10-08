/**
 * 「GameBox 挂载」标签：赛事与 GameBox 的挂载关系（EventGameBox）增删改。
 *
 * 已核实的后端语义：
 * - 只能挂载 `build_status == "ready"` 的 GameBox；`host_offset` 需在 2..254，缺省自动取未占用值；
 * - UNIQUE(event_id, gamebox_id)：重复挂载返回 409；
 * - 新增时会从 GameBox 的 recommended_* 预填资源，并记录配置代数变更（需重新预检）；
 * - 移除时若该 GameBox 已有实例，DB 层 RESTRICT 拒绝（后端返回冲突），必须先处理实例。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Boxes, Pencil, Plus, Trash2 } from "lucide-react";

import type { EventGameBoxDto } from "@floatctf/sdk";

import { call, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, InlineError, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { Switch } from "~/components/ui/switch";
import { formatBytes, formatDateTime } from "~/lib/format";

import { useAwdEventGameboxes, useAwdGameboxLibrary } from "./queries";

function intOrError(raw: string, min: number, max: number): string | null {
	const trimmed = raw.trim();
	if (trimmed === "") return null;
	if (!/^-?\d+$/.test(trimmed)) return "必须是整数";
	const value = Number(trimmed);
	if (value < min || value > max) return `需在 ${min} – ${max} 之间`;
	return null;
}

/* ── 新增挂载 ─────────────────────────────────────────────────────────────── */

function AddEventGameboxSheet({
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
	const libraryQuery = useAwdGameboxLibrary(true);

	const [gameboxId, setGameboxId] = useState("");
	const [hostOffset, setHostOffset] = useState("");
	const [hidden, setHidden] = useState(false);
	const [attackScore, setAttackScore] = useState("100");
	const [judgeDownPenalty, setJudgeDownPenalty] = useState("200");
	const [firstBonus, setFirstBonus] = useState("20");

	const errors = {
		host_offset: intOrError(hostOffset, 2, 254) ?? undefined,
		attack_score: intOrError(attackScore, -1_000_000_000, 1_000_000_000) ?? undefined,
		judge_down_penalty:
			intOrError(judgeDownPenalty, -1_000_000_000, 1_000_000_000) ?? undefined,
		first_bonus: intOrError(firstBonus, -1_000_000_000, 1_000_000_000) ?? undefined,
	};
	const hasError = Object.values(errors).some((value) => value !== undefined);

	const mutation = useMutation({
		mutationFn: (body: {
			gamebox_id: string;
			host_offset?: number;
			hidden?: boolean;
			attack_score?: number;
			judge_down_penalty?: number;
			first_bonus?: number;
		}) => call<EventGameBoxDto>(client.awd.admin.addEventGamebox(eventId, body), "挂载 GameBox"),
		onSuccess: (gamebox) => {
			toast.success(
				`已挂载「${gamebox.gamebox_name}」`,
				`主机编号 #${gamebox.host_offset}；配置代数已变更，需重新预检。`,
			);
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventGameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("挂载 GameBox 失败", error),
	});

	const library = libraryQuery.data?.items ?? [];
	const ready = library.filter(
		(item) => item.build_status === "ready" && !attachedIds.has(item.id),
	);

	const submit = (event: FormEvent) => {
		event.preventDefault();
		if (gameboxId === "" || hasError) return;
		const body: {
			gamebox_id: string;
			host_offset?: number;
			hidden?: boolean;
			attack_score?: number;
			judge_down_penalty?: number;
			first_bonus?: number;
		} = { gamebox_id: gameboxId, hidden };
		if (hostOffset.trim() !== "") body.host_offset = Number(hostOffset);
		if (attackScore.trim() !== "") body.attack_score = Number(attackScore);
		if (judgeDownPenalty.trim() !== "") body.judge_down_penalty = Number(judgeDownPenalty);
		if (firstBonus.trim() !== "") body.first_bonus = Number(firstBonus);
		mutation.mutate(body);
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="挂载 GameBox"
			description="仅可挂载已构建成功（ready）的 GameBox；资源与判题参数默认取 GameBox 的推荐值。"
			width="lg"
			footer={
				<FormFooter
					onCancel={onClose}
					formId="awd-add-gamebox-form"
					submitLabel="挂载"
					isPending={mutation.isPending}
					disabled={gameboxId === "" || hasError}
				/>
			}
		>
			<form id="awd-add-gamebox-form" onSubmit={submit} className="space-y-4">
				<Field label="GameBox" htmlFor="awd-add-gamebox" required hint="已挂载或未构建完成的不会出现在列表里">
					<Select value={gameboxId} onValueChange={setGameboxId} disabled={libraryQuery.isPending}>
						<SelectTrigger id="awd-add-gamebox" aria-label="选择 GameBox">
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
						没有可挂载的 GameBox：GameBox 库中没有状态为 ready 的靶机（或全部已挂载）。
					</p>
				) : null}
				<FormGrid columns={2}>
					<Field
						label="主机编号（host_offset）"
						htmlFor="awd-add-host-offset"
						hint="留空自动分配 2–254 内的空闲编号"
						error={errors.host_offset}
					>
						<Input
							id="awd-add-host-offset"
							type="number"
							inputMode="numeric"
							value={hostOffset}
							onChange={(event) => setHostOffset(event.target.value)}
							placeholder="自动"
						/>
					</Field>
					<Field label="对选手隐藏" htmlFor="awd-add-hidden" hint="隐藏后选手端列表不展示该靶机">
						<Switch id="awd-add-hidden" checked={hidden} onCheckedChange={setHidden} />
					</Field>
					<Field label="攻击得分" htmlFor="awd-add-attack" error={errors.attack_score}>
						<Input
							id="awd-add-attack"
							type="number"
							inputMode="numeric"
							value={attackScore}
							onChange={(event) => setAttackScore(event.target.value)}
						/>
					</Field>
					<Field label="宕机罚分" htmlFor="awd-add-penalty" error={errors.judge_down_penalty}>
						<Input
							id="awd-add-penalty"
							type="number"
							inputMode="numeric"
							value={judgeDownPenalty}
							onChange={(event) => setJudgeDownPenalty(event.target.value)}
						/>
					</Field>
					<Field label="一血加成" htmlFor="awd-add-first-bonus" error={errors.first_bonus}>
						<Input
							id="awd-add-first-bonus"
							type="number"
							inputMode="numeric"
							value={firstBonus}
							onChange={(event) => setFirstBonus(event.target.value)}
						/>
					</Field>
				</FormGrid>
			</form>
		</FormSheet>
	);
}

/* ── 编辑挂载 ─────────────────────────────────────────────────────────────── */

interface EventGameboxForm {
	enabled: boolean;
	hidden: boolean;
	cpu_millis: string;
	memory_bytes: string;
	pids_limit: string;
	judge_timeout_secs: string;
	judge_retry_interval_secs: string;
	attack_score: string;
	judge_down_penalty: string;
	first_bonus: string;
}

interface EventGameboxPatch {
	enabled?: boolean;
	hidden?: boolean;
	cpu_millis?: number;
	memory_bytes?: number;
	pids_limit?: number;
	judge_timeout_secs?: number;
	judge_retry_interval_secs?: number;
	attack_score?: number;
	judge_down_penalty?: number;
	first_bonus?: number;
}

function EditEventGameboxSheet({
	eventId,
	row,
	onClose,
}: {
	eventId: string;
	row: EventGameBoxDto;
	onClose: () => void;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState<EventGameboxForm>({
		enabled: row.enabled,
		hidden: row.hidden,
		cpu_millis: String(row.cpu_millis),
		memory_bytes: String(row.memory_bytes),
		pids_limit: String(row.pids_limit),
		judge_timeout_secs: row.judge_timeout_secs === null ? "" : String(row.judge_timeout_secs),
		judge_retry_interval_secs:
			row.judge_retry_interval_secs === null ? "" : String(row.judge_retry_interval_secs),
		attack_score: String(row.attack_score),
		judge_down_penalty: String(row.judge_down_penalty),
		first_bonus: String(row.first_bonus),
	});
	const [errors, setErrors] = useState<Record<string, string | undefined>>({});

	const mutation = useMutation({
		mutationFn: (patch: EventGameboxPatch) =>
			call<EventGameBoxDto>(
				client.awd.admin.updateEventGamebox(eventId, row.id, patch),
				"更新赛事 GameBox",
			),
		onSuccess: (updated) => {
			toast.success(`已更新「${updated.gamebox_name}」`);
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventGameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("更新赛事 GameBox 失败", error),
	});

	const submit = (event: FormEvent) => {
		event.preventDefault();
		const nextErrors: Record<string, string | undefined> = {
			cpu_millis: intOrError(form.cpu_millis, 0, 1_000_000) ?? undefined,
			memory_bytes: intOrError(form.memory_bytes, 0, 1_000_000_000_000) ?? undefined,
			pids_limit: intOrError(form.pids_limit, 0, 1_000_000) ?? undefined,
			attack_score: intOrError(form.attack_score, -1_000_000_000, 1_000_000_000) ?? undefined,
			judge_down_penalty:
				intOrError(form.judge_down_penalty, -1_000_000_000, 1_000_000_000) ?? undefined,
			first_bonus: intOrError(form.first_bonus, -1_000_000_000, 1_000_000_000) ?? undefined,
			judge_timeout_secs: intOrError(form.judge_timeout_secs, 1, 3_600) ?? undefined,
			judge_retry_interval_secs:
				intOrError(form.judge_retry_interval_secs, 1, 3_600) ?? undefined,
		};
		// 后端 updateEventGamebox 对这两个字段没有 double_option：显式 null 会被忽略（不是清空），
		// 因此这里禁止「把已有值清空」，避免出现静默无效的保存。
		if (form.judge_timeout_secs.trim() === "" && row.judge_timeout_secs !== null) {
			nextErrors.judge_timeout_secs = "后端不支持清空该覆盖值（null 会被忽略）：请填写数值";
		}
		if (form.judge_retry_interval_secs.trim() === "" && row.judge_retry_interval_secs !== null) {
			nextErrors.judge_retry_interval_secs =
				"后端不支持清空该覆盖值（null 会被忽略）：请填写数值";
		}
		setErrors(nextErrors);
		if (Object.values(nextErrors).some((value) => value !== undefined)) return;

		const patch: EventGameboxPatch = {};
		if (form.enabled !== row.enabled) patch.enabled = form.enabled;
		if (form.hidden !== row.hidden) patch.hidden = form.hidden;
		if (form.cpu_millis !== String(row.cpu_millis)) patch.cpu_millis = Number(form.cpu_millis);
		if (form.memory_bytes !== String(row.memory_bytes)) {
			patch.memory_bytes = Number(form.memory_bytes);
		}
		if (form.pids_limit !== String(row.pids_limit)) patch.pids_limit = Number(form.pids_limit);
		if (form.attack_score !== String(row.attack_score)) {
			patch.attack_score = Number(form.attack_score);
		}
		if (form.judge_down_penalty !== String(row.judge_down_penalty)) {
			patch.judge_down_penalty = Number(form.judge_down_penalty);
		}
		if (form.first_bonus !== String(row.first_bonus)) patch.first_bonus = Number(form.first_bonus);
		const originalTimeout = row.judge_timeout_secs === null ? "" : String(row.judge_timeout_secs);
		if (form.judge_timeout_secs !== originalTimeout && form.judge_timeout_secs.trim() !== "") {
			patch.judge_timeout_secs = Number(form.judge_timeout_secs);
		}
		const originalRetry =
			row.judge_retry_interval_secs === null ? "" : String(row.judge_retry_interval_secs);
		if (form.judge_retry_interval_secs !== originalRetry && form.judge_retry_interval_secs.trim() !== "") {
			patch.judge_retry_interval_secs = Number(form.judge_retry_interval_secs);
		}

		if (Object.keys(patch).length === 0) {
			toast.warning("没有需要保存的修改");
			return;
		}
		mutation.mutate(patch);
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title={`赛事 GameBox：${row.gamebox_name}`}
			description={`${row.gamebox_safe_name}${row.gamebox_version ? ` · v${row.gamebox_version}` : ""} · 主机编号 #${row.host_offset}`}
			width="lg"
			footer={
				<FormFooter
					onCancel={onClose}
					formId="awd-edit-gamebox-form"
					isPending={mutation.isPending}
				/>
			}
		>
			<form id="awd-edit-gamebox-form" onSubmit={submit} className="space-y-4">
				<FormGrid columns={2}>
					<Field label="启用" htmlFor="awd-eg-enabled" hint="停用后该 GameBox 不参与比赛">
						<Switch
							id="awd-eg-enabled"
							checked={form.enabled}
							onCheckedChange={(value) => setForm((prev) => ({ ...prev, enabled: value }))}
						/>
					</Field>
					<Field label="对选手隐藏" htmlFor="awd-eg-hidden">
						<Switch
							id="awd-eg-hidden"
							checked={form.hidden}
							onCheckedChange={(value) => setForm((prev) => ({ ...prev, hidden: value }))}
						/>
					</Field>
					<Field label="CPU（millis）" htmlFor="awd-eg-cpu" error={errors.cpu_millis}>
						<Input
							id="awd-eg-cpu"
							type="number"
							inputMode="numeric"
							value={form.cpu_millis}
							onChange={(event) => setForm((prev) => ({ ...prev, cpu_millis: event.target.value }))}
						/>
					</Field>
					<Field label="内存（字节）" htmlFor="awd-eg-mem" error={errors.memory_bytes}>
						<Input
							id="awd-eg-mem"
							type="number"
							inputMode="numeric"
							value={form.memory_bytes}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, memory_bytes: event.target.value }))
							}
						/>
					</Field>
					<Field label="进程数上限" htmlFor="awd-eg-pids" error={errors.pids_limit}>
						<Input
							id="awd-eg-pids"
							type="number"
							inputMode="numeric"
							value={form.pids_limit}
							onChange={(event) => setForm((prev) => ({ ...prev, pids_limit: event.target.value }))}
						/>
					</Field>
					<Field
						label="判题超时覆盖（秒）"
						htmlFor="awd-eg-judge-timeout"
						hint="留空 = 不修改（后端不支持清空已有覆盖值）"
						error={errors.judge_timeout_secs}
					>
						<Input
							id="awd-eg-judge-timeout"
							type="number"
							inputMode="numeric"
							value={form.judge_timeout_secs}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, judge_timeout_secs: event.target.value }))
							}
							placeholder="继承赛事默认"
						/>
					</Field>
					<Field
						label="判题重试间隔覆盖（秒）"
						htmlFor="awd-eg-judge-retry"
						hint="留空 = 不修改（后端不支持清空已有覆盖值）"
						error={errors.judge_retry_interval_secs}
					>
						<Input
							id="awd-eg-judge-retry"
							type="number"
							inputMode="numeric"
							value={form.judge_retry_interval_secs}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, judge_retry_interval_secs: event.target.value }))
							}
							placeholder="继承赛事默认"
						/>
					</Field>
					<Field label="攻击得分" htmlFor="awd-eg-attack" error={errors.attack_score}>
						<Input
							id="awd-eg-attack"
							type="number"
							inputMode="numeric"
							value={form.attack_score}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, attack_score: event.target.value }))
							}
						/>
					</Field>
					<Field label="宕机罚分" htmlFor="awd-eg-penalty" error={errors.judge_down_penalty}>
						<Input
							id="awd-eg-penalty"
							type="number"
							inputMode="numeric"
							value={form.judge_down_penalty}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, judge_down_penalty: event.target.value }))
							}
						/>
					</Field>
					<Field label="一血加成" htmlFor="awd-eg-first-bonus" error={errors.first_bonus}>
						<Input
							id="awd-eg-first-bonus"
							type="number"
							inputMode="numeric"
							value={form.first_bonus}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, first_bonus: event.target.value }))
							}
						/>
					</Field>
				</FormGrid>
				<p className="text-xs text-muted-foreground">
					修改计分 / 资源 / 判题参数会使配置代数 +1，需要通过一次新的预检才能开赛。
				</p>
			</form>
		</FormSheet>
	);
}

/* ── 标签主体 ─────────────────────────────────────────────────────────────── */

export function AwdGameboxesTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const eventGameboxes = useAwdEventGameboxes(eventId);
	const [adding, setAdding] = useState(false);
	const [editRow, setEditRow] = useState<EventGameBoxDto | null>(null);

	const remove = useMutation({
		mutationFn: (row: EventGameBoxDto) =>
			callVoid(client.awd.admin.removeEventGamebox(eventId, row.id), "移除挂载"),
		onSuccess: (_result, row) => {
			toast.success(`已移除「${row.gamebox_name}」的挂载`);
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventGameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		},
		onError: (error) => toast.apiError("移除挂载失败", error),
	});

	const columns: DataTableColumn<EventGameBoxDto>[] = [
		{
			id: "gamebox_name",
			header: "GameBox",
			sortValue: (row) => row.gamebox_name,
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate font-medium">{row.gamebox_name}</p>
					<p className="text-xs text-muted-foreground">
						{row.gamebox_safe_name}
						{row.gamebox_version ? ` · v${row.gamebox_version}` : ""}
					</p>
				</div>
			),
		},
		{
			id: "host_offset",
			header: "主机编号",
			align: "right",
			sortValue: (row) => row.host_offset,
			cell: (row) => <MonoText>#{row.host_offset}</MonoText>,
		},
		{
			id: "enabled",
			header: "启用",
			cell: (row) => <BooleanPill value={row.enabled} trueText="启用" falseText="停用" />,
		},
		{
			id: "hidden",
			header: "选手可见",
			hideBelow: "md",
			cell: (row) => (
				<BooleanPill
					value={!row.hidden}
					trueText="可见"
					falseText="已隐藏"
					falseTone="warning"
				/>
			),
		},
		{
			id: "scores",
			header: "计分（攻 / 罚 / 一血）",
			hideBelow: "lg",
			cell: (row) => (
				<MonoText>
					{row.attack_score} / {row.judge_down_penalty} / {row.first_bonus}
				</MonoText>
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
			id: "created_at",
			header: "挂载时间",
			hideBelow: "xl",
			cell: (row) => formatDateTime(row.created_at, { seconds: true }),
			sortValue: (row) => row.created_at,
		},
	];

	return (
		<SectionCard
			title="赛事 GameBox 挂载"
			description="赛事选择哪些 GameBox 参赛；资源与判题参数取自 GameBox 推荐值，可在此覆盖。"
			actions={
				<Button size="sm" onClick={() => setAdding(true)}>
					<Plus /> 挂载 GameBox
				</Button>
			}
		>
			<QueryState
				query={eventGameboxes}
				skeleton={<TableSkeleton rows={5} columns={5} />}
				isEmpty={(data) => data.items.length === 0}
				empty={
					<EmptyBlock
						title="尚未挂载任何 GameBox"
						description="从 GameBox 库选择已构建成功（ready）的靶机挂载到本赛事。"
						icon={<Boxes className="size-5" />}
						action={
							<Button size="sm" onClick={() => setAdding(true)}>
								<Plus /> 挂载 GameBox
							</Button>
						}
					/>
				}
			>
				{(data) => (
					<div className="space-y-3">
						<DataTable
							data={data.items}
							getRowId={(row) => row.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1">
									<p className="text-sm font-medium">{row.gamebox_name}</p>
									<p className="text-xs text-muted-foreground">
										#{row.host_offset} · 攻击 {row.attack_score} · 罚分 {row.judge_down_penalty}
									</p>
									<div className="flex flex-wrap gap-1 pt-1">
										<BooleanPill value={row.enabled} trueText="启用" falseText="停用" />
										<BooleanPill
											value={!row.hidden}
											trueText="可见"
											falseText="已隐藏"
											falseTone="warning"
										/>
									</div>
								</div>
							)}
							rowActions={(row) => (
								<>
									<Button
										variant="ghost"
										size="icon-sm"
										onClick={() => setEditRow(row)}
										aria-label={`编辑 ${row.gamebox_name}`}
										title="编辑"
									>
										<Pencil />
									</Button>
									<Button
										variant="ghost"
										size="icon-sm"
										disabled={remove.isPending}
										onClick={async () => {
											const ok = await confirm({
												title: `移除「${row.gamebox_name}」的挂载？`,
												description: "移除后该 GameBox 不再属于本赛事。",
												consequences: [
													"删除赛事与该 GameBox 的挂载记录（配置代数 +1，需重新预检）",
													"若该 GameBox 在本赛事已有实例，后端会拒绝移除（需先处理实例）",
													"选手端将不再看到该靶机",
												],
												tone: "danger",
												confirmText: "移除",
											});
											if (ok) remove.mutate(row);
										}}
										aria-label={`移除 ${row.gamebox_name}`}
										title="移除挂载"
									>
										<Trash2 className="text-destructive" />
									</Button>
								</>
							)}
						/>
						{data.items.length < data.total ? (
							<p className="text-xs text-muted-foreground">
								仅显示前 {data.items.length} 条（共 {data.total} 条）。
							</p>
						) : null}
					</div>
				)}
			</QueryState>

			{adding ? (
				<AddEventGameboxSheet
					eventId={eventId}
					attachedIds={new Set((eventGameboxes.data?.items ?? []).map((row) => row.gamebox_id))}
					onClose={() => setAdding(false)}
				/>
			) : null}
			{editRow ? (
				<EditEventGameboxSheet
					eventId={eventId}
					row={editRow}
					onClose={() => setEditRow(null)}
				/>
			) : null}
		</SectionCard>
	);
}
