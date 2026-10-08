/**
 * 赛事相关的共享组件：状态徽章、参赛/退赛、战队、writeup 提交。
 *
 * 参赛语义（与后端守卫及 Default 行为一致）：
 * - 仅在赛事**未开始**时可以加入/退出（个人赛走 `events.join/leave`，战队赛走 `createTeam/joinTeam/quitTeam`）；
 * - 非「未开始」且未加入 → 不展示赛事工作区内容（后端也会拒绝受保护接口）。
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileUp, LogIn, LogOut, ShieldAlert, Upload, Users } from "lucide-react";

import { call, callMaybe, callVoid, uploadFile } from "~/api/call";
import { useClient } from "~/api/client";
import { isNotFound } from "~/api/errors";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import {
	KeyValueList,
	MonoText,
	ReadonlyBlock,
	SectionCard,
} from "~/components/app/page";
import { EmptyBlock, InlineError, LoadingBlock } from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import { Separator } from "~/components/ui/separator";
import type { EventInfo } from "@floatctf/sdk";
import type { Events } from "@floatctf/sdk/entity";
import { formatRange, formatRelative } from "~/lib/format";
import {
	EVENT_FAMILY_LABEL,
	EVENT_PURPOSE_LABEL,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
	PARTICIPANT_MODE_LABEL,
	computeEventStatus,
} from "~/lib/event-status";
import { useNow } from "~/lib/hooks";

export function EventStatusPill({
	event,
	className,
}: {
	event: Events;
	className?: string;
}) {
	const now = useNow(30_000);
	const status = computeEventStatus(event.start_time, event.end_time ?? null, now.getTime());
	return (
		<TonePill tone={EVENT_STATUS_TONE[status]} className={className}>
			{EVENT_STATUS_LABEL[status]}
		</TonePill>
	);
}

export function EventFactsCard({ event }: { event: Events }) {
	const now = useNow(30_000);
	return (
		<SectionCard title="赛事信息">
			<KeyValueList
				items={[
					{ key: "赛制家族", value: EVENT_FAMILY_LABEL[event.family] ?? event.family },
					{ key: "参与模式", value: PARTICIPANT_MODE_LABEL[event.participant_mode] ?? event.participant_mode },
					{ key: "赛事类型", value: EVENT_PURPOSE_LABEL[event.purpose] ?? event.purpose },
					{ key: "时间窗", value: formatRange(event.start_time, event.end_time ?? null) },
					{ key: "允许加入", value: event.allow_join ? "允许" : "不允许自助加入" },
					{
						key: "Flag 前缀",
						value: event.flag_prefix ? <MonoText>{event.flag_prefix}</MonoText> : "—",
					},
					{
						key: "隐藏状态",
						value: event.hidden ? (
							<TonePill tone="muted">已隐藏</TonePill>
						) : (
							<TonePill tone="success">公开</TonePill>
						),
					},
					{
						key: "更新于",
						value: <span className="text-xs">{formatRelative(event.updated_at, now)}</span>,
					},
				]}
				columns={2}
			/>
		</SectionCard>
	);
}

/** 个人赛参赛 / 退赛（仅未开始时可用）。 */
export function JoinLeaveControl({ info }: { info: EventInfo }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const now = useNow(30_000);
	const status = computeEventStatus(info.event.start_time, info.event.end_time ?? null, now.getTime());
	const canJoin = status === "upcoming";

	const invalidate = () => {
		void queryClient.invalidateQueries({ queryKey: qk.events.detail(info.event.id) });
		void queryClient.invalidateQueries({ queryKey: qk.events.all });
	};

	const join = useMutation({
		mutationFn: () => callVoid(client.service.events.join(info.event.id), "加入赛事"),
		onSuccess: () => {
			toast.success("已加入赛事");
			invalidate();
		},
		onError: (error) => toast.apiError("加入失败", error),
	});

	const leave = useMutation({
		mutationFn: () => callVoid(client.service.events.leave(info.event.id), "退出赛事"),
		onSuccess: () => {
			toast.success("已退出赛事");
			invalidate();
		},
		onError: (error) => toast.apiError("退出失败", error),
	});

	return (
		<div className="space-y-2">
			{info.joined ? (
				<Button
					variant="destructive"
					disabled={!canJoin || leave.isPending}
					title={canJoin ? undefined : "赛事已开始，无法退出"}
					onClick={async () => {
						const ok = await confirm({
							title: "退出该赛事？",
							description: info.event.title,
							consequences: ["退出后需要重新加入才能继续参赛", "已获得的解题记录仍保留"],
							tone: "danger",
							confirmText: "退出赛事",
						});
						if (ok) leave.mutate();
					}}
				>
					<LogOut /> 退出赛事
				</Button>
			) : (
				<Button
					disabled={!canJoin || join.isPending}
					title={canJoin ? undefined : "赛事已开始，无法加入"}
					onClick={() => join.mutate()}
				>
					<LogIn /> 加入赛事
				</Button>
			)}
			{!canJoin ? (
				<p className="text-xs text-muted-foreground">
					参赛名单在赛事开始后即锁定（后端同样会拒绝此时的加入/退出请求）。
				</p>
			) : null}
			{join.isError ? <InlineError error={join.error} /> : null}
			{leave.isError ? <InlineError error={leave.error} /> : null}
		</div>
	);
}

/** 战队赛：建队 / 用 Team ID 加入 / 退队 / 查看成员。 */
export function TeamPanel({ info }: { info: EventInfo }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const now = useNow(30_000);
	const status = computeEventStatus(info.event.start_time, info.event.end_time ?? null, now.getTime());
	const canChange = status === "upcoming";
	const [teamName, setTeamName] = useState("");
	const [teamId, setTeamId] = useState("");

	const invalidate = () => {
		void queryClient.invalidateQueries({ queryKey: qk.events.detail(info.event.id) });
		void queryClient.invalidateQueries({ queryKey: qk.events.all });
	};

	const createTeam = useMutation({
		mutationFn: () =>
			call(client.service.events.createTeam({ event_id: info.event.id, name: teamName.trim() }), "创建战队"),
		onSuccess: () => {
			toast.success("战队已创建", "你已成为队长。");
			setTeamName("");
			invalidate();
		},
		onError: (error) => toast.apiError("创建战队失败", error),
	});

	const joinTeam = useMutation({
		mutationFn: (id: string) =>
			callVoid(client.service.events.joinTeam({ event_id: info.event.id, team_id: id }), "加入战队"),
		onSuccess: () => {
			toast.success("已加入战队");
			setTeamId("");
			invalidate();
		},
		onError: (error) => toast.apiError("加入战队失败", error),
	});

	const quitTeam = useMutation({
		mutationFn: (id: string) =>
			callVoid(client.service.events.quitTeam({ event_id: info.event.id, team_id: id }), "退出战队"),
		onSuccess: () => {
			toast.success("已退出战队");
			invalidate();
		},
		onError: (error) => toast.apiError("退出战队失败", error),
	});

	const team = info.team_result;

	if (!info.joined) {
		if (!canChange) {
			return (
				<EmptyBlock
					title="未参赛"
					description="赛事已开始，无法再加入。"
					icon={<ShieldAlert className="size-5" />}
				/>
			);
		}
		return (
			<div className="space-y-4">
				<div className="space-y-2">
					<Label htmlFor="join-team-id" className="text-xs">
						已有战队？用 Team ID 加入
					</Label>
					<div className="flex gap-2">
						<Input
							id="join-team-id"
							value={teamId}
							onChange={(event) => setTeamId(event.target.value)}
							placeholder="战队 UUID"
							className="font-mono text-xs"
						/>
						<Button
							variant="outline"
							disabled={teamId.trim() === "" || joinTeam.isPending}
							onClick={() => joinTeam.mutate(teamId.trim())}
						>
							<Users /> 加入
						</Button>
					</div>
				</div>
				<Separator />
				<div className="space-y-2">
					<Label htmlFor="create-team-name" className="text-xs">
						或创建新战队（创建者默认为队长）
					</Label>
					<div className="flex gap-2">
						<Input
							id="create-team-name"
							value={teamName}
							onChange={(event) => setTeamName(event.target.value)}
							placeholder="战队名称"
						/>
						<Button
							disabled={teamName.trim() === "" || createTeam.isPending}
							onClick={() => createTeam.mutate()}
						>
							创建
						</Button>
					</div>
				</div>
				{createTeam.isError ? <InlineError error={createTeam.error} /> : null}
				{joinTeam.isError ? <InlineError error={joinTeam.error} /> : null}
			</div>
		);
	}

	if (!team) {
		return (
			<EmptyBlock
				title="未加入战队"
				description="你已参赛但还没有战队，加入或创建一个战队后即可协作。"
			/>
		);
	}

	return (
		<div className="space-y-4">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<div className="flex flex-wrap items-center gap-2">
					<span className="text-sm font-semibold">{team.team.name}</span>
					{team.team.banned ? (
						<TonePill tone="danger" icon={<ShieldAlert />}>
							战队已被封禁
						</TonePill>
					) : null}
					<TonePill tone="info">{team.team.points} 分</TonePill>
				</div>
				{canChange ? (
					<Button
						variant="destructive"
						size="sm"
						disabled={quitTeam.isPending}
						onClick={async () => {
							const ok = await confirm({
								title: "退出战队？",
								description: team.team.name,
								consequences: ["退队后需要重新加入或创建战队", "战队共享的积分不受影响"],
								tone: "danger",
								confirmText: "退出战队",
							});
							if (ok) quitTeam.mutate(team.team.id);
						}}
					>
						<LogOut /> 退出战队
					</Button>
				) : null}
			</div>

			<KeyValueList
				columns={1}
				items={[
					{ key: "Team ID", value: <MonoText>{team.team.id}</MonoText> },
					...(team.team.description
						? [{ key: "战队简介", value: <ReadonlyBlock>{team.team.description}</ReadonlyBlock> }]
						: []),
					{
						key: "成员",
						value: (
							<ul className="space-y-1">
								{team.members.map((member) => (
									<li key={member.member.user_id} className="flex items-center gap-2 text-sm">
										<TonePill tone={member.member.role === "captain" ? "info" : "muted"}>
											{member.member.role === "captain" ? "队长" : "队员"}
										</TonePill>
										<span className="truncate">{member.member_name}</span>
										<span className="text-xs text-muted-foreground">
											{formatRelative(member.member.joined_at, now)}加入
										</span>
									</li>
								))}
							</ul>
						),
					},
				]}
			/>
			{quitTeam.isError ? <InlineError error={quitTeam.error} /> : null}
		</div>
	);
}

/** 赛后 writeup（PDF）提交状态 + 上传。 */
export function EventWriteupPanel({ info }: { info: EventInfo }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [file, setFile] = useState<File | null>(null);

	const own = useQuery({
		queryKey: qk.events.ownWriteup(info.event.id),
		queryFn: async () => {
			try {
				return await callMaybe<string>(client.service.events.getOwnWp(info.event.id), "我的 writeup");
			} catch (error) {
				if (isNotFound(error)) return null;
				throw error;
			}
		},
		retry: false,
	});

	const upload = useMutation({
		mutationFn: (selected: File) =>
			uploadFile<null>({
				client,
				scope: "user",
				url: `/submit/writeup`,
				field: "writeup_pdf",
				file: selected,
				extra: {
					event_id: info.event.id,
					...(info.team_result ? { team_id: info.team_result.team.id } : {}),
				},
			}),
		onSuccess: () => {
			toast.success("writeup 已提交");
			setFile(null);
			void queryClient.invalidateQueries({ queryKey: qk.events.ownWriteup(info.event.id) });
		},
		onError: (error) => toast.apiError("提交失败", error),
	});

	if (own.isPending) return <LoadingBlock label="读取 writeup 状态…" />;

	const submitted = own.data && own.data.trim().length > 0;

	return (
		<div className="space-y-3">
			{submitted ? (
				<div className="rounded-md border bg-muted/30 p-3 text-sm">
					<p className="mb-1 font-medium">已提交 writeup</p>
					<MonoText className="text-muted-foreground">{own.data}</MonoText>
				</div>
			) : (
				<p className="text-sm text-muted-foreground">还没有提交 writeup（PDF）。</p>
			)}
			<div className="flex flex-wrap items-center gap-2">
				<Input
					type="file"
					accept="application/pdf"
					className="max-w-xs"
					onChange={(event) => setFile(event.target.files?.[0] ?? null)}
				/>
				<Button
					disabled={!file || upload.isPending}
					onClick={() => file && upload.mutate(file)}
				>
					{upload.isPending ? <Upload className="animate-pulse" /> : <FileUp />}
					{submitted ? "重新提交" : "提交 writeup"}
				</Button>
			</div>
			{upload.isError ? <InlineError error={upload.error} /> : null}
		</div>
	);
}

/** 未参赛且赛事已开始时的门禁提示（与后端权限一致）。 */
export function MemberGateNotice() {
	return (
		<div className="rounded-lg border border-[var(--warning)]/40 bg-[var(--warning)]/10 p-4">
			<p className="flex items-center gap-2 text-sm font-medium">
				<ShieldAlert className="size-4" /> 你尚未加入本赛事
			</p>
			<p className="mt-1 text-sm text-muted-foreground">
				赛事开始后参赛名单即锁定，题目、积分榜与实例都只对已参赛选手开放。请在赛事开始前加入。
			</p>
		</div>
	);
}
