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
import { AwdpEventWorkbench } from "./-workbench";

export const Route = createFileRoute("/service/events/awdp/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function formatDate(iso?: string | null) {
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

	// Team 表单状态
	const [teamId, setTeamId] = useState("");
	const [teamName, setTeamName] = useState("");

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

					{/* AWDP 工作台（§65：共用组件，VM 由赛事 overview 适配） */}
					<AwdpEventWorkbench eventId={id} />

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
						<dd className="font-medium">
							{ev.family} / {ev.participant_mode}
						</dd>
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
