import { Button, FormControl, Heading, Label, TextInput } from "@primer/react";
import { Banner, InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";
import dayjs from "dayjs";
import { type FormEvent, useMemo, useState } from "react";
import { match } from "ts-pattern";

import { serviceApi } from "@/api";
import { eventInfoQueryOptions } from "@/api/queries";
import { SubmitWriteup, useMsgInlineBanner } from "@/components";
import { ParticipantMode } from "@/entity";
import type { EventInfo } from "..";

export const Route = createFileRoute("/service/events/jeopardy/$id/")({
	component: RouteComponent,
});

function parseMs(iso?: string): number {
	if (!iso) return Number.NaN;
	return dayjs.utc(iso).valueOf(); // 始终按 UTC 解析
}

function getEventStatus(start?: string | null, end?: string | null) {
	if (!start) return "unknown";
	const now = Date.now();
	const s = new Date(start).getTime();
	if (Number.isNaN(s)) return "unknown";
	if (s > now) return "upcoming";
	if (end == null || end === "") return "ongoing";
	const e = new Date(end).getTime();
	if (Number.isNaN(e)) return "unknown";
	if (e < now) return "ended";
	return "ongoing";
}

function formatDate(iso?: string) {
	return dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss");
}

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();

	const banner = useMsgInlineBanner();

	const { data, isLoading, isError, error } = useQuery(
		eventInfoQueryOptions(id),
	);

	const eventData: EventInfo | undefined = data?.data;
	const ev = eventData?.event;

	const status = useMemo(
		() => getEventStatus(ev?.start_time, ev?.end_time),
		[ev?.start_time, ev?.end_time],
	);

	const showStatusText =
		status === "upcoming"
			? "Upcoming"
			: status === "ended"
				? "Ended"
				: status === "ongoing"
					? "Ongoing"
					: "TBD";

	// 统一失效：join/leave/建队/退队后，本页与各 tab 页（challenges/instances/scoreboard/trend/announcement）都要重新获取。
	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["eventInfo", id] });
		queryClient.invalidateQueries({ queryKey: ["eventChallenges", id] });
		queryClient.invalidateQueries({ queryKey: ["event_instances", id] });
		queryClient.invalidateQueries({ queryKey: ["event_scoreboard", id] });
		queryClient.invalidateQueries({ queryKey: ["event_trend", id] });
		queryClient.invalidateQueries({ queryKey: ["announcements", id] });
	};

	// 统一命名 join/leave mutation；把隐藏消息放到 onMutate（清空）和 onError（展示）
	const joinEventMutation = useMutation({
		mutationFn: serviceApi.events.join,
		onMutate: () => {
			banner.hideBanner();
		},
		onSuccess: invalidate,

		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	const leaveEventMutation = useMutation({
		mutationFn: serviceApi.events.leave,
		onMutate: () => {
			banner.hideBanner();
		},
		onSuccess: invalidate,

		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	// Team 表单状态（占位实现）
	const [teamId, setTeamId] = useState("");
	const [teamName, setTeamName] = useState("");
	const createEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.createTeam,
		onSuccess: invalidate,
		// 报名成功

		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	const quitEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.quitTeam,
		onSuccess: invalidate,

		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	const joinEventTeamMutation = useMutation({
		mutationFn: serviceApi.events.joinTeam,
		onSuccess: invalidate,

		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	const handleJoinSingle = () => {
		if (!ev) return;
		joinEventMutation.mutate(ev.id);
	};
	const handleLeaveSingle = () => {
		if (!ev) return;
		leaveEventMutation.mutate(ev.id);
	};

	const isJoining = joinEventMutation.isPending;
	const isLeaving =
		leaveEventMutation.isPending || quitEventTeamMutation.isPending;

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
	if (status !== "upcoming" && !eventData.joined) {
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
				className="border rounded p-4  flex-28"
			/>

			<div className="flex flex-col gap-3 flex-13">
				{/* 右侧：操作 */}
				<div className="flex flex-col gap-3 min-w-[320px]">
					{match(ev.participant_mode)
						.with(ParticipantMode.Individual, () => (
							<section className="p-3 rounded border flex  items-center min-h-[72px]">
								{status === "upcoming" && (
									<Button
										className="w-28"
										variant={eventData.joined ? "danger" : "primary"}
										onClick={
											eventData.joined ? handleLeaveSingle : handleJoinSingle
										}
										disabled={eventData.joined ? isLeaving : isJoining}
										aria-label={eventData.joined ? "Leave event" : "Join event"}
									>
										{eventData.joined
											? isLeaving
												? "Leaving…"
												: "Leave"
											: isJoining
												? "Joining…"
												: "Join"}
									</Button>
								)}
								{status !== "upcoming" && eventData.joined && (
									<SubmitWriteup eventId={id} />
								)}
							</section>
						))
						.with(ParticipantMode.Team, () => (
							<section className="p-3 rounded border flex gap-5">
								{status !== "upcoming" && eventData.joined && (
									<SubmitWriteup
										eventId={id}
										teamId={eventData.team_result?.team.id}
									/>
								)}
								{eventData.joined && (
									<div className="flex flex-col gap-3">
										<div className="flex items-center gap-2 mb-2">
											<Heading as="h2">
												{eventData.team_result?.team.name}
											</Heading>
											{eventData.team_result?.team.banned && (
												<Label variant="danger">Banned</Label>
											)}
										</div>
										<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2">
											<dt className="font-bold">ID</dt>
											<dd className="font-medium break-all">
												{eventData.team_result?.team.id}
											</dd>

											{eventData.team_result?.members.map((member) => (
												<>
													<dt key={member.member.user_id} className="font-bold">
														{member.member.role}
													</dt>
													<dd
														key={member.member.user_id}
														className="font-medium  break-all"
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
										{/* 已加入未开始 */}
										{status === "upcoming" && (
											<Button
												className="w-28"
												variant="danger"
												onClick={() =>
													quitEventTeamMutation.mutate({
														event_id: id,
														team_id: eventData.team_result?.team.id ?? "",
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
								{status === "upcoming" && !eventData.joined && (
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
									</>
								)}
							</section>
						))
						.otherwise(() => (
							<span>Unsupported event type</span>
						))}

					<banner.BannerComponent />
				</div>
				<section className="p-3 rounded border">
					<div className="flex items-center gap-2 mb-2">
						<Heading as="h2">{ev.title}</Heading>
						{eventData.joined ? (
							<Label variant="success">Joined</Label>
						) : (
							<Label variant="attention">Unjoined</Label>
						)}
					</div>

					{/* 用 <dl> 语义化描述列表，替代 div+tr/td 混用 */}
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
