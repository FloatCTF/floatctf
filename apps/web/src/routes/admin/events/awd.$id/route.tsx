import {
	Box,
	Button,
	FormControl,
	Spinner,
	TextInput,
	UnderlineNav,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { createContext, useState } from "react";

import { adminApi } from "@/api";
import { useMsgBanner } from "@/components";
import {
	type Challenges,
	type EventChallenges,
	EventType,
	type Events,
} from "@/entity";
import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";

export const Route = createFileRoute("/admin/events/awd/$id")({
	component: RouteComponent,
});

export const EventContext = createContext<Events | null>(null);

export type EventChallengeResult = {
	id: string;
	event_challenge: EventChallenges;
	challenge: Challenges;
};
function RouteComponent() {
	const { id } = Route.useParams();
	const qc = useQueryClient();
	const banner = useMsgBanner({});
	const [roundDurationSecs, setRoundDurationSecs] = useState("600");
	const [plannedStartAt, setPlannedStartAt] = useState("");

	const {
		data: event_data,
		isLoading,
		isError,
	} = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});

	const statusQuery = useQuery({
		queryKey: ["admin-awd-status", id],
		queryFn: () => adminApi.awd.getStatus(id),
	});
	const awdStatus = statusQuery.data?.data ?? null;
	const awdReady = awdStatus !== null;

	const initAwd = useMutation({
		mutationFn: async () => {
			const body: Record<string, unknown> = {
				event_id: id,
				round_duration_secs: Number(roundDurationSecs) || 600,
			};
			if (plannedStartAt) {
				body.planned_start_at = new Date(plannedStartAt).toISOString();
			}
			return adminApi.awd.createEvent(body);
		},
		onSuccess: () => {
			banner.showBanner("success", "AWD 赛事已初始化");
			qc.invalidateQueries({ queryKey: ["admin-awd-status", id] });
			qc.invalidateQueries({ queryKey: ["event", id] });
		},
		onError: (e) => {
			banner.showBanner(
				"critical",
				e instanceof Error ? e.message : "初始化失败",
			);
		},
	});

	const event = event_data?.data;

	if (isLoading || statusQuery.isLoading) {
		return <Spinner size="large" />;
	}

	if (isError || !event) {
		return <div>Error loading event</div>;
	}

	if (!awdReady) {
		return (
			<div>
				<h3>
					{event.title} #{event.id}
				</h3>
				<Box
					sx={{
						mt: 3,
						p: 4,
						border: "1px solid",
						borderColor: "border.default",
						borderRadius: 2,
						maxWidth: 560,
					}}
				>
					<h4>初始化 AWD 赛事</h4>
					<p style={{ color: "var(--fgColor-muted, #57606a)" }}>
						该赛事尚未初始化为 AWD。初始化会生成赛事密钥与内部
						Token，之后才能配置网络 / GameBoxes / 队伍并部署。
					</p>
					<FormControl sx={{ mt: 3 }}>
						<FormControl.Label>单轮时长（秒）</FormControl.Label>
						<FormControl.Caption>
							每轮 Hardening / Attack / Grace 的时长，例如 600 秒。
						</FormControl.Caption>
						<TextInput
							value={roundDurationSecs}
							onChange={(e) => setRoundDurationSecs(e.target.value)}
							type="number"
						/>
					</FormControl>
					<FormControl sx={{ mt: 3 }}>
						<FormControl.Label>定时开赛（可选）</FormControl.Label>
						<FormControl.Caption>
							留空则手动在 Ops 页启动。
						</FormControl.Caption>
						<TextInput
							value={plannedStartAt}
							onChange={(e) => setPlannedStartAt(e.target.value)}
							type="datetime-local"
						/>
					</FormControl>
					<Box sx={{ mt: 4 }}>
						<Button
							variant="primary"
							onClick={() => initAwd.mutate()}
							disabled={initAwd.isPending}
						>
							{initAwd.isPending ? "初始化中…" : "初始化 AWD 赛事"}
						</Button>
					</Box>
				</Box>
			</div>
		);
	}

	return (
		<div>
			<h3>
				{event.title} #{event.id}
			</h3>
			<UnderlineNav aria-label="Repository">
				<RouterNavItem to="/admin/events/awd/$id/gameboxes" params={{ id }}>
					GameBoxes
				</RouterNavItem>
				<RouterNavItem to="/admin/events/awd/$id/network" params={{ id }}>
					Network
				</RouterNavItem>
				<RouterNavItem to="/admin/events/awd/$id/ops" params={{ id }}>
					Ops
				</RouterNavItem>
				{event?.type === EventType.AwdTeam && (
					<RouterNavItem to="/admin/events/awd/$id/teams" params={{ id }}>
						Teams
					</RouterNavItem>
				)}
				<RouterNavItem to="/admin/events/awd/$id/announcements" params={{ id }}>
					Announcements
				</RouterNavItem>
				<RouterNavItem to="/admin/events/awd/$id/writeups" params={{ id }}>
					WriteUps
				</RouterNavItem>
				<RouterNavItem to="/admin/events/awd/$id/logs" params={{ id }}>
					Logs
				</RouterNavItem>
			</UnderlineNav>
			<EventContext.Provider value={event}>
				<Outlet /> {/* 普通 TanStack Router 的 Outlet */}
			</EventContext.Provider>
		</div>
	);
}
