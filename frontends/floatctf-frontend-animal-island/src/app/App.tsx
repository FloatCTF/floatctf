/**
 * 路由表与守卫。
 *
 * 路由**完全归本前端所有**（平台不要求 Default 的路径或层级）。
 *
 * 守卫的边界（写清楚，避免误以为前端在保护什么）：
 * - 前端守卫只判断「有没有会话」以及「要不要给这个入口显示出来」；
 * - **权限与业务判定永远在服务端**。前端隐藏入口是体验，不是安全。
 *   直接输入 URL 也能到达页面 —— 页面随后会拿到 403，而 403 会被如实展示。
 */
import { Navigate, Route, Routes, useLocation } from "react-router";

import { Shell } from "~/app/shell";
import { useSession } from "~/auth/store";
import { LoginPage } from "~/features/auth/Login";
import { IslandHome } from "~/features/island/Home";
import { EventList } from "~/features/island/EventList";
import { EventDetail } from "~/features/island/EventDetail";
import { ChallengeList } from "~/features/island/ChallengeList";
import { ChallengeDetail } from "~/features/island/ChallengeDetail";
import { Scoreboard } from "~/features/island/Scoreboard";
import { DiscussionList } from "~/features/island/DiscussionList";
import { DiscussionDetail } from "~/features/island/DiscussionDetail";
import { WriteupList } from "~/features/island/WriteupList";
import { ProfilePage } from "~/features/island/Profile";
import { AwdEvent } from "~/features/island/AwdEvent";
import { AwdpEvent } from "~/features/island/AwdpEvent";
import { AwdpPractice } from "~/features/island/AwdpPractice";
import { AwdpRun } from "~/features/island/AwdpRun";
import { ConsoleHome } from "~/features/console/Home";
import { ConsoleEvents } from "~/features/console/Events";
import { ConsoleEventDetail } from "~/features/console/EventDetail";
import { ConsoleAwdEvent } from "~/features/console/AwdEvent";
import { ConsoleAwdpEvent } from "~/features/console/AwdpEvent";
import { ConsoleChallenges } from "~/features/console/Challenges";
import { ConsoleGameboxes } from "~/features/console/Gameboxes";
import { ConsoleWeapons } from "~/features/console/Weapons";
import { ConsoleNetwork } from "~/features/console/Network";
import { ConsoleAnnouncements } from "~/features/console/Announcements";
import { ConsoleDiscussions } from "~/features/console/Discussions";
import { ConsoleUsers } from "~/features/console/Users";
import { ConsoleUserDetail } from "~/features/console/UserDetail";
import { ConsoleRoles } from "~/features/console/Roles";
import { ConsoleLogs } from "~/features/console/Logs";
import { ConsoleSettings } from "~/features/console/Settings";
import { ConsoleFrontends } from "~/features/console/Frontends";
import { ConsoleTasks } from "~/features/console/Tasks";
import { ConsoleDocker } from "~/features/console/Docker";
import { ConsoleDatabase } from "~/features/console/Database";
import { ConsoleTerminal } from "~/features/console/Terminal";
import { ConsoleVersion } from "~/features/console/Version";
import { NotFound } from "~/features/NotFound";

/** 已登录才可进入；未登录跳登录页并记住来路。 */
function RequireSession({ children }: { children: React.ReactNode }) {
	const token = useSession((s) => s.token);
	const location = useLocation();
	if (!token) {
		const next = `${location.pathname}${location.search}`;
		return <Navigate to={`/login?next=${encodeURIComponent(next)}`} replace />;
	}
	return <>{children}</>;
}

export function App() {
	return (
		<Routes>
			<Route path="/login" element={<LoginPage />} />

			<Route
				path="/island"
				element={
					<RequireSession>
						<Shell />
					</RequireSession>
				}
			>
				<Route index element={<IslandHome />} />
				<Route path="events" element={<EventList />} />
				<Route path="events/:eventId" element={<EventDetail />} />
				{/* AWD 选手面板：按赛事进入（AWD 的一切都以赛事为作用域）。 */}
				<Route path="awd/:eventId" element={<AwdEvent />} />
				{/* AWDP 选手工作台（比赛）与练习。练习目录在路由上独立于具体赛事。 */}
				<Route path="awdp/practice" element={<AwdpPractice />} />
				<Route path="awdp/runs/:runId" element={<AwdpRun />} />
				<Route path="awdp/:eventId" element={<AwdpEvent />} />
				<Route path="challenges" element={<ChallengeList />} />
				<Route path="challenges/:challengeId" element={<ChallengeDetail />} />
				<Route path="scoreboard" element={<Scoreboard />} />
				<Route path="discussions" element={<DiscussionList />} />
				<Route path="discussions/:discussionId" element={<DiscussionDetail />} />
				<Route path="writeups" element={<WriteupList />} />
				<Route path="profile" element={<ProfilePage />} />
			</Route>

			<Route
				path="/console"
				element={
					<RequireSession>
						<Shell />
					</RequireSession>
				}
			>
				<Route index element={<ConsoleHome />} />
				<Route path="events" element={<ConsoleEvents />} />
				<Route path="events/:eventId" element={<ConsoleEventDetail />} />
				{/* AWD 管理控制台：同样按赛事进入。 */}
				<Route path="awd/:eventId" element={<ConsoleAwdEvent />} />
				<Route path="awdp/:eventId" element={<ConsoleAwdpEvent />} />
				<Route path="challenges" element={<ConsoleChallenges />} />
				<Route path="gameboxes" element={<ConsoleGameboxes />} />
				<Route path="weapons" element={<ConsoleWeapons />} />
				<Route path="network" element={<ConsoleNetwork />} />
				<Route path="announcements" element={<ConsoleAnnouncements />} />
				<Route path="discussions" element={<ConsoleDiscussions />} />
				<Route path="users" element={<ConsoleUsers />} />
				<Route path="users/:userId" element={<ConsoleUserDetail />} />
				<Route path="roles" element={<ConsoleRoles />} />
				<Route path="logs" element={<ConsoleLogs />} />
				<Route path="settings" element={<ConsoleSettings />} />
				<Route path="frontends" element={<ConsoleFrontends />} />
				<Route path="tasks" element={<ConsoleTasks />} />
				<Route path="docker" element={<ConsoleDocker />} />
				<Route path="database" element={<ConsoleDatabase />} />
				<Route path="terminal" element={<ConsoleTerminal />} />
				<Route path="version" element={<ConsoleVersion />} />
			</Route>

			<Route path="/" element={<Navigate to="/island" replace />} />
			<Route path="*" element={<NotFound />} />
		</Routes>
	);
}
