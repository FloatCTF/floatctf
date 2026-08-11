import { Button, FormControl, Heading, Label, Select, TextInput } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";
import dayjs from "dayjs";
import { type FormEvent, useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { awdpPlayerApi } from "@/api/awdp";
import {
	EVENT_STATUS_LABEL,
	SubmitWriteup,
	computeEventStatus,
	useMsgInlineBanner,
} from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function formatDate(iso?: string | null) {
	return dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss");
}

const PHASE_META: Record<string, { text: string; variant: "attention" | "accent" | "success" | "done" }> = {
	pending: { text: "Pending", variant: "attention" },
	break: { text: "Break", variant: "accent" },
	fix: { text: "Fix", variant: "success" },
	ended: { text: "Ended", variant: "done" },
};

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgInlineBanner();
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);

	const { data, isLoading, isError, error } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
	const eventData = data?.data;
	const ev = eventData?.event;

	const status = computeEventStatus(ev?.start_time ?? "", ev?.end_time ?? "");
	const showStatusText = EVENT_STATUS_LABEL[status];
	const joined = eventData?.joined ?? false;
	const myTeam = eventData?.team_result;

	const overviewQuery = useQuery({
		queryKey: ["awdp-overview", id],
		queryFn: () => awdpPlayerApi.overview(id),
		enabled: !!eventData,
	});
	const overview = overviewQuery.data?.data;

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["eventInfo", id] });
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", id] });
	};

	const joinEventMutation = useMutation({
		mutationFn: serviceApi.events.join,
		onMutate: () => banner.hideBanner(),
		onSuccess: invalidate,
		onError: (e) => banner.showErrorBanner(e),
	});

	const createEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.createTeam,
		onMutate: () => banner.hideBanner(),
		onSuccess: invalidate,
		onError: (e) => banner.showErrorBanner(e),
	});

	const joinEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.joinTeam,
		onMutate: () => banner.hideBanner(),
		onSuccess: invalidate,
		onError: (e) => banner.showErrorBanner(e),
	});

	const quitEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.quitTeam,
		onMutate: () => banner.hideBanner(),
		onSuccess: invalidate,
		onError: (e) => banner.showErrorBanner(e),
	});

	// Break flag 提交（每 GameBox 一次性）。
	const [breakGamebox, setBreakGamebox] = useState("");
	const [flag, setFlag] = useState("");
	const submitBreakMutation = useMutation({
		mutationFn: ({ egId, value }: { egId: string; value: string }) =>
			awdpPlayerApi.submitBreak(id, egId, value),
		onMutate: () => banner.hideBanner(),
		onSuccess: (res) => {
			const d = res.data;
			if (!d) return;
			if (d.accepted) {
				banner.showBanner(
					d.scored
						? "success"
						: "warning",
					d.scored ? "Flag accepted, +score" : "Flag accepted (already broken)",
				);
				setFlag("");
			} else {
				banner.showBanner("critical", "Flag rejected");
			}
			queryClient.invalidateQueries({ queryKey: ["awdp-overview", id] });
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	// Team 表单状态
	const [teamId, setTeamId] = useState("");
	const [teamName, setTeamName] = useState("");

	const isLeaving =
		quitEventTeamMutation.isPending || joinEventTeamMutation.isPending;

	const phase = overview?.phase ?? "pending";
	const phaseMeta = PHASE_META[phase] ?? PHASE_META.pending;
	// 倒计时目标：pending → 开赛时间；break → break 结束；fix → 下一回合 cutoff。
	const countdownTarget = (() => {
		if (!overview) return null;
		if (phase === "break") return overview.break_ends_at;
		if (phase === "fix") return overview.next_action_at;
		if (phase === "pending") return ev?.start_time ?? null;
		return null;
	})();
	const remaining = countdownTarget
		? Math.max(0, Math.floor((new Date(countdownTarget).getTime() - now) / 1000))
		: 0;
	const countdownText =
		countdownTarget && remaining > 0
			? `${Math.floor(remaining / 3600)}h ${Math.floor((remaining % 3600) / 60)}m ${remaining % 60}s`
			: "-";

	if (isLoading) {
		return <div className="p-4">Loading…</div>;
	}
	if (isError) {
		return (
			<div className="p-4">
				<InlineMessage variant="critical">
					{(error as Error)?.message ?? "Failed to load event."}
				</InlineMessage>
			</div>
		);
	}
	if (!eventData || !ev) {
		return (
			<div className="p-4">
				<InlineMessage variant="warning">Event not found.</InlineMessage>
			</div>
		);
	}
	if (status !== "upcoming" && !joined) {
		return (
			<div className="p-4">
				<InlineMessage variant="warning">
					You are not joined this event.
				</InlineMessage>
			</div>
		);
	}

	return (
		<div className="flex p-3 w-full gap-3 justify-between">
			<MDEditor.Markdown
				source={ev.rules}
				className="border rounded p-4 flex-[3]"
			/>

			<div className="flex flex-col gap-3 flex-1 min-w-[320px]">
				<div className="flex flex-col gap-3">
					{/* 队伍区（参考 AWD/Jeopardy 参赛形态） */}
					<section className="p-3 rounded border flex gap-5">
						{status !== "upcoming" && joined && (
							<SubmitWriteup eventId={id} teamId={myTeam?.team.id} />
						)}
						{joined && (
							<div className="flex flex-col gap-3">
								<div className="flex items-center gap-2 mb-2">
									<Heading as="h2">{myTeam?.team.name}</Heading>
									{myTeam?.team.banned && (
										<Label variant="danger">Banned</Label>
									)}
								</div>
								<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2">
									<dt className="font-bold">ID</dt>
									<dd className="font-medium break-all">{myTeam?.team.id}</dd>
									{myTeam?.members.map((member) => (
										<>
											<dt key={member.member.user_id} className="font-bold">
												{member.member.role}
											</dt>
											<dd
												key={member.member.user_id}
												className="font-medium break-all"
											>
												{member.member_name} @{" "}
												{dayjs
													.utc(member.member.joined_at)
													.local()
													.format("YYYY-MM-DD HH:mm:ss")}
											</dd>
										</>
									))}
								</dl>
								{status === "upcoming" && (
									<Button
										className="w-28"
										variant="danger"
										onClick={() =>
											quitEventTeamMutation.mutate({
												event_id: id,
												team_id: myTeam?.team.id ?? "",
											})
										}
										disabled={isLeaving}
										aria-label="Leave event"
									>
										{isLeaving ? "Leaving…" : "Leave"}
									</Button>
								)}
							</div>
						)}
						{/* 未开始未加入 */}
						{status === "upcoming" && !joined && (
							<>
								<form
									className="flex w-full flex-col gap-2"
									onSubmit={(e: FormEvent) => {
										e.preventDefault();
										joinEventTeamMutation.mutate({
											event_id: id,
											team_id: teamId,
										});
									}}
								>
									<FormControl required>
										<FormControl.Label>Team ID</FormControl.Label>
										<TextInput
											value={teamId}
											onChange={(e) => setTeamId(e.target.value)}
											aria-label="Team ID"
										/>
									</FormControl>
									<Button variant="primary" type="submit">
										Join
									</Button>
								</form>
								<form
									className="flex w-full flex-col gap-2"
									onSubmit={(e: FormEvent) => {
										e.preventDefault();
										createEventTeamMutation.mutate({
											event_id: id,
											name: teamName,
										});
									}}
								>
									<FormControl required>
										<FormControl.Label>Team Name</FormControl.Label>
										<TextInput
											value={teamName}
											onChange={(e) => setTeamName(e.target.value)}
											aria-label="Team Name"
										/>
									</FormControl>
									<Button variant="primary" type="submit">
										Create
									</Button>
								</form>
								{ev.allow_join && (
									<Button
										onClick={() => joinEventMutation.mutate(ev.id)}
										disabled={joinEventMutation.isPending}
									>
										{joinEventMutation.isPending ? "Joining…" : "Join Event"}
									</Button>
								)}
							</>
						)}
					</section>

					{/* AWDP 专属：阶段 / 倒计时 / 分数 */}
					<section className="p-3 rounded border">
						<div className="flex items-center gap-2 mb-2">
							<Label variant={phaseMeta.variant}>Phase: {phaseMeta.text}</Label>
							<span className="text-sm font-medium ml-auto">
								我的得分：<strong>{overview?.my_score ?? 0}</strong>
							</span>
						</div>
						<dl className="grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2">
							<dt className="font-bold">Countdown</dt>
							<dd className="font-medium tabular-nums">{countdownText}</dd>
							<dt className="font-bold">Break Score</dt>
							<dd className="font-medium">+{overview?.break_score ?? 0} / GameBox</dd>
							<dt className="font-bold">Fix Score</dt>
							<dd className="font-medium">
								+{overview?.fix_round_score ?? 0} / Turn
							</dd>
							<dt className="font-bold">Rounds</dt>
							<dd className="font-medium">
								{overview?.current_round ?? 0} / {overview?.total_rounds ?? "-"}
							</dd>
							<dt className="font-bold">Break 至</dt>
							<dd className="font-medium">{formatDate(overview?.break_ends_at)}</dd>
							<dt className="font-bold">Fix 至</dt>
							<dd className="font-medium">{formatDate(overview?.fix_ends_at)}</dd>
						</dl>
					</section>

					{/* AWDP 专属：Break flag 提交（每 GameBox 一次性） */}
					{joined && phase === "break" && (
						<section className="p-3 rounded border flex flex-col gap-2">
							<FormControl>
								<FormControl.Label>Submit Break Flag</FormControl.Label>
								<Select
									value={breakGamebox}
									onChange={(e) => setBreakGamebox(e.target.value)}
								>
									<Select.Option value="">Select GameBox…</Select.Option>
									{(overview?.gameboxes ?? []).map((gb) => (
										<Select.Option key={gb.id} value={gb.id}>
											{gb.name}
											{gb.broken ? " (broken)" : ""}
										</Select.Option>
									))}
								</Select>
							</FormControl>
							<FormControl>
								<TextInput
									value={flag}
									onChange={(e) => setFlag(e.target.value)}
									placeholder="flag{...}"
									block
								/>
							</FormControl>
							<Button
								variant="primary"
								disabled={
									!flag || !breakGamebox || submitBreakMutation.isPending
								}
								onClick={() =>
									submitBreakMutation.mutate({ egId: breakGamebox, value: flag })
								}
							>
								Submit
							</Button>
						</section>
					)}

					<banner.BannerComponent />
				</div>

				{/* 事件信息卡 */}
				<section className="p-3 rounded border">
					<div className="flex items-center gap-2 mb-2">
						<Heading as="h2">{ev.title}</Heading>
						{joined ? (
							<Label variant="success">Joined</Label>
						) : (
							<Label variant="attention">Unjoined</Label>
						)}
					</div>
					<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2">
						<dt className="font-bold">ID</dt>
						<dd className="font-medium break-all">{ev.id}</dd>
						<dt className="font-bold">Type</dt>
						<dd className="font-medium">{ev.family} / {ev.participant_mode}</dd>
						<dt className="font-bold">Start</dt>
						<dd className="font-medium">{formatDate(ev.start_time)}</dd>
						<dt className="font-bold">End</dt>
						<dd className="font-medium">{formatDate(ev.end_time)}</dd>
						<dt className="font-bold">Status</dt>
						<dd className="font-medium">{showStatusText}</dd>
						<dt className="font-bold">Description</dt>
						<dd className="font-medium whitespace-pre-wrap">
							{ev.description || "-"}
						</dd>
					</dl>
				</section>
			</div>
		</div>
	);
}
