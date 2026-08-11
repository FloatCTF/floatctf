import { Button, FormControl, Heading, Label, TextInput } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";
import dayjs from "dayjs";
import { type FormEvent, useState } from "react";

import { serviceApi } from "@/api";
import {
	EVENT_STATUS_LABEL,
	SubmitWriteup,
	computeEventStatus,
	useMsgInlineBanner,
} from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function formatDate(iso?: string) {
	return dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss");
}

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgInlineBanner();

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

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["eventInfo", id] });
		queryClient.invalidateQueries({ queryKey: ["awd-gameboxes", id] });
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

	const submitFlagMutation = useMutation({
		mutationFn: ({ event_id, flag }: { event_id: string; flag: string }) =>
			serviceApi.awd.submitFlag(event_id, flag),
		onMutate: () => banner.hideBanner(),
		onSuccess: (res) => {
			if (res.code === 0) {
				banner.showBanner("success", "Flag accepted");
				setFlag("");
			} else {
				banner.showBanner("critical", res.message || "Submit failed");
			}
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	// Team 表单状态
	const [teamId, setTeamId] = useState("");
	const [teamName, setTeamName] = useState("");
	const [flag, setFlag] = useState("");

	const isLeaving =
		quitEventTeamMutation.isPending || joinEventTeamMutation.isPending;

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
				{/* 右侧：操作 */}
				<div className="flex flex-col gap-3">
					{/* 队伍区（参考 JeopardyTeam） */}
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
								{/* 已加入未开始 */}
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

					{/* AWD 专属：Flag 提交（仅已加入且进行中） */}
					{joined && status === "ongoing" && (
						<section className="p-3 rounded border flex flex-col gap-2">
							<FormControl>
								<FormControl.Label>Submit Flag</FormControl.Label>
								<TextInput
									value={flag}
									onChange={(e) => setFlag(e.target.value)}
									placeholder="flag{...}"
									block
								/>
							</FormControl>
							<Button
								variant="primary"
								disabled={!flag || submitFlagMutation.isPending}
								onClick={() =>
									submitFlagMutation.mutate({ event_id: id, flag })
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
