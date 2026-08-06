import { RocketIcon } from "@primer/octicons-react";
import { ProgressBar, Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import {
    Link,
    Outlet,
    createFileRoute,
    useMatchRoute,
} from "@tanstack/react-router";
import { useTitle } from "ahooks";
import { useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { DatetimeToShow } from "@/util";
import dayjs from "dayjs";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/jeopardy/$id")({
    component: RouteComponent,
    loader: ServiceRouteGuard,
});

function RouteComponent() {
    const { id } = Route.useParams();

    const { data, isLoading } = useQuery({
        queryKey: ["eventInfo", id],
        queryFn: () => serviceApi.events.get(id),
    });
    const eventInfo = data?.data;

    useTitle(`${eventInfo?.event.title ?? "Event"} | FloatCTF`);

    if (isLoading) {
        return <Spinner size="large" />;
    }

    if (isLoading) {
        return <Spinner size="large" />;
    }

    return (
        <div>
            <div className="flex gap-1 items-center">
                <RocketIcon size={20} />
                <h3 className=" font-bold">{eventInfo?.event.title}</h3>
            </div>
            <RemainingTimer
                start_at={DatetimeToShow(eventInfo?.event.start_time)}
                end_at={DatetimeToShow(eventInfo?.event.end_time)}
            />
            <UnderlineNav aria-label="Repository">
                <RouterNavItem
                    to="/service/events/jeopardy/$id"
                    params={{ id }}
                >
                    Overview
                </RouterNavItem>
                <RouterNavItem
                    to="/service/events/jeopardy/$id/challenges"
                    params={{ id }}
                >
                    Challenges
                </RouterNavItem>

                <RouterNavItem
                    to="/service/events/jeopardy/$id/instances"
                    params={{ id }}
                >
                    Instances
                </RouterNavItem>
                <RouterNavItem
                    to="/service/events/jeopardy/$id/scoreboard"
                    params={{ id }}
                >
                    Scoreboard
                </RouterNavItem>
                <RouterNavItem
                    to="/service/events/jeopardy/$id/trend"
                    params={{ id }}
                >
                    Trend
                </RouterNavItem>
                <RouterNavItem
                    to="/service/events/jeopardy/$id/announcement"
                    params={{ id }}
                >
                    Announcement
                </RouterNavItem>
            </UnderlineNav>
            <Outlet />
        </div>
    );
}

export type RouterNavItemProps = {
    to: string;
    // biome-ignore lint/suspicious/noExplicitAny: <explanation>
    params?: Record<string, any>;
    children: React.ReactNode;
};

export function RouterNavItem({ to, params, children }: RouterNavItemProps) {
    const matchRoute = useMatchRoute();
    const isActive = matchRoute({ to, params, fuzzy: false });

    return (
        <Link style={{ textDecoration: "none" }} to={to} params={params}>
            <UnderlineNav.Item aria-current={isActive ? "page" : undefined}>
                {children}
            </UnderlineNav.Item>
        </Link>
    );
}
type RemainingTimerProps = {
    start_at: string;
    end_at: string;
};

const formatTime = (ms: number) => {
    let diffSeconds = Math.floor(ms / 1000);
    const days = Math.floor(diffSeconds / (24 * 3600));
    diffSeconds %= 24 * 3600;
    const hours = Math.floor(diffSeconds / 3600);
    diffSeconds %= 3600;
    const minutes = Math.floor(diffSeconds / 60);
    const seconds = diffSeconds % 60;

    const parts: string[] = [];
    if (days) parts.push(`${days}d`);
    if (hours) parts.push(`${hours}h`);
    if (minutes) parts.push(`${minutes}m`);
    if (seconds || parts.length === 0) parts.push(`${seconds}s`);

    return parts.join(" ");
};
export const RemainingTimer = ({ start_at, end_at }: RemainingTimerProps) => {
    const [display, setDisplay] = useState({
        text: "",
        percentage: 0,
    });

    // 时间格式化函数

    useEffect(() => {
        const update = () => {
            const startTime = dayjs(start_at).toDate().getTime();
            const endTime = dayjs(end_at).toDate().getTime();
            const now = new Date().getTime();

            let text = "";
            let percentage = 0;
            const total = endTime - startTime;

            if (now < startTime) {
                // 未开始 → 进度 100%
                text = `Starts in: ${formatTime(startTime - now)}`;
                percentage = 100;
            } else if (now >= startTime && now < endTime) {
                // 进行中 → 剩余时间占总时长百分比
                const remaining = endTime - now;
                text = `Ends in: ${formatTime(remaining)}`;
                percentage = (remaining / total) * 100;
            } else {
                // 已结束 → 进度 0%
                text = "Ended";
                percentage = 0;
            }

            setDisplay({ text, percentage });
        };

        update(); // 初始化
        const timer = setInterval(update, 1000);
        return () => clearInterval(timer);
    }, [start_at, end_at]);

    return (
        <>
            <span
                style={{
                    color: "var(--fgColor-muted)",
                    font: "var(--text-body-shorthand-medium)",
                }}
                className="text-center"
            >
                {display.text}
            </span>
            <ProgressBar
                animated
                progress={display.percentage}
                aria-label="remaining"
            />
        </>
    );
};
