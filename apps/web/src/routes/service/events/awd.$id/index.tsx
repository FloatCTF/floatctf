import { Button, FormControl, Label, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";
import { useState } from "react";

import { serviceApi } from "@/api";
import {
	EventStatusBadge,
	computeEventStatus,
	useMsgBanner,
} from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const qc = useQueryClient();
	const [flag, setFlag] = useState("");
	const [teamId, setTeamId] = useState("");
	const [teamName, setTeamName] = useState("");

	const eventQ = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
	const eventData = eventQ.data?.data;
	const event = eventData?.event;
	const status = computeEventStatus(
		event?.start_time ?? "",
		event?.end_time ?? "",
	);
	const joined = eventData?.joined ?? false;
	const myTeam = eventData?.team_result;

	const invalidate = () => {
		qc.invalidateQueries({ queryKey: ["eventInfo", id] });
		qc.invalidateQueries({ queryKey: ["awd-gameboxes", id] });
	};

	const submit = useMutation({
		mutationFn: () => serviceApi.awd.submitFlag(id, flag),
		onSuccess: (res) => {
			if (res.code === 0) {
				banner.showBanner("success", "Flag accepted");
				setFlag("");
			} else {
				banner.showBanner("critical", res.message || "Submit failed");
			}
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	const joinEvent = useMutation({
		mutationFn: () => serviceApi.events.join(id),
		onSuccess: () => {
			banner.showBanner("success", "Joined event");
			invalidate();
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	const createTeam = useMutation({
		mutationFn: () =>
			serviceApi.events.createTeam({ event_id: id, name: teamName }),
		onSuccess: () => {
			banner.showBanner("success", "Team created");
			setTeamName("");
			invalidate();
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	const joinTeam = useMutation({
		mutationFn: () =>
			serviceApi.events.joinTeam({ event_id: id, team_id: teamId }),
		onSuccess: () => {
			banner.showBanner("success", "Joined team");
			setTeamId("");
			invalidate();
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	const quitTeam = useMutation({
		mutationFn: () =>
			serviceApi.events.quitTeam({
				event_id: id,
				team_id: myTeam?.team.id ?? "",
			}),
		onSuccess: () => {
			banner.showBanner("success", "Left team");
			invalidate();
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	return (
		<div className="flex flex-col gap-4 max-w-2xl">
			<banner.BannerComponent />
			<div className="flex items-center gap-3">
				<h4 className="font-bold">Overview</h4>
				{event && (
					<EventStatusBadge
						startTime={event.start_time}
						endTime={event.end_time}
					/>
				)}
			</div>

			{/* 队伍区：未加入 → 加入/创建；已加入 → 队伍信息 */}
			{!joined ? (
				<section className="p-4 rounded border flex flex-col gap-4">
					<p className="text-sm opacity-80">
						比赛为团队制：先加入队伍才能部署游戏盒、访问 WireGuard 并提交 Flag。
					</p>
					{event?.allow_join && (
						<Button
							variant="primary"
							disabled={joinEvent.isPending}
							onClick={() => joinEvent.mutate()}
						>
							{joinEvent.isPending ? "Joining…" : "Join Event"}
						</Button>
					)}
					<form
						className="flex w-full flex-col gap-2"
						onSubmit={(e) => {
							e.preventDefault();
							if (teamName.trim()) createTeam.mutate();
						}}
					>
						<FormControl required>
							<FormControl.Label>Create Team</FormControl.Label>
							<TextInput
								value={teamName}
								onChange={(e) => setTeamName(e.target.value)}
								placeholder="Team name"
								aria-label="Team name"
								block
							/>
						</FormControl>
						<Button
							variant="primary"
							type="submit"
							disabled={createTeam.isPending}
						>
							{createTeam.isPending ? "Creating…" : "Create Team"}
						</Button>
					</form>
					<form
						className="flex w-full flex-col gap-2"
						onSubmit={(e) => {
							e.preventDefault();
							if (teamId.trim()) joinTeam.mutate();
						}}
					>
						<FormControl>
							<FormControl.Label>Join Team by ID</FormControl.Label>
							<TextInput
								value={teamId}
								onChange={(e) => setTeamId(e.target.value)}
								placeholder="Team ID"
								aria-label="Team ID"
								block
							/>
						</FormControl>
						<Button type="submit" disabled={joinTeam.isPending}>
							{joinTeam.isPending ? "Joining…" : "Join Team"}
						</Button>
					</form>
				</section>
			) : (
				<section className="p-4 rounded border flex flex-col gap-3">
					<div className="flex items-center gap-2">
						<h4 className="font-bold">{myTeam?.team.name ?? "My Team"}</h4>
						{myTeam?.team.banned && <Label variant="danger">Banned</Label>}
					</div>
					<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2 text-sm">
						<dt className="font-bold">Team ID</dt>
						<dd className="break-all">{myTeam?.team.id}</dd>
						{myTeam?.members.map((m) => (
							<>
								<dt key={m.member.user_id} className="font-bold">
									{m.member.role}
								</dt>
								<dd key={m.member.user_id} className="break-all">
									{m.member_name} @{" "}
									{dayjs
										.utc(m.member.joined_at)
										.local()
										.format("YYYY-MM-DD HH:mm:ss")}
								</dd>
							</>
						))}
					</dl>
					{status === "upcoming" && (
						<Button
							variant="danger"
							className="w-28"
							disabled={quitTeam.isPending}
							onClick={() => quitTeam.mutate()}
						>
							{quitTeam.isPending ? "Leaving…" : "Leave"}
						</Button>
					)}
				</section>
			)}

			{/* Flag 提交：仅已加入且进行中 */}
			{joined && status === "ongoing" && (
				<div className="flex flex-col gap-2">
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
						disabled={!flag || submit.isPending}
						onClick={() => submit.mutate()}
					>
						Submit
					</Button>
				</div>
			)}

			{joined && status === "upcoming" && (
				<p className="text-sm opacity-70">
					比赛未开始。可前往 GameBoxes / WireGuard / SSH 页查看部署信息。
				</p>
			)}
			{joined && status === "ended" && (
				<p className="text-sm opacity-70">
					比赛已结束，可查看 Scoreboard 最终排名。
				</p>
			)}
		</div>
	);
}
