import {
    RocketIcon,
    ScreenFullIcon,
    ScreenNormalIcon,
    TriangleRightIcon,
} from "@primer/octicons-react";
import { Label, LabelGroup, Spinner, Text, Timeline } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import type { Events } from "@/entity";
import { RemainingTimer } from "@/routes/service/events/jeopardy.$id/route";
import {
    ScoreBoard,
    type ScoreboardItem,
} from "@/routes/service/events/jeopardy.$id/scoreboard";
import {
    TrendChart,
    type TrendItem,
} from "@/routes/service/events/jeopardy.$id/trend";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/jeopardy/$id/data_present")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

export type DataEventChallenge = {
    name: string;
    category: string;
    points: number;
    solved_count: number;
    solved_percent: number;
};

export type DataEventChallengeSolve = {
    user_nickname: string;
    challenge_name: string;
    challenge_category: string;
    created_at: string; // NaiveDateTime → string
    bonus_points: number;
};

// 你需要在别处定义 Event, ScoreboardItem, TrendItem 的 TS 类型
export type DataPresent = {
    event: Events; // 对应 events::Model
    user_count: number;
    team_count: number;
    solved_recent_15: DataEventChallengeSolve[];
    event_challenges: DataEventChallenge[];
    scoreboard_top10: ScoreboardItem[];
    trend: TrendItem[];
};

function RouteComponent() {
    const [isFull, setIsFull] = useState(false);
    const { id } = Route.useParams();
    const { data, isLoading, isError } = useQuery({
        queryKey: ["event_data_present", id],
        queryFn: async () => adminApi.events.getData(id),
        refetchInterval: 1000 * 30,
    });
    const dp: DataPresent | undefined = data?.data;

    if (isLoading) {
        return <Spinner size="large" />;
    }

    if (isError || !dp || !dp.event) {
        return <div>Error loading data</div>;
    }
    return (
        <div className="relative w-full h-full mb-2">
            <div
                className={`transition-all duration-500 ease-in-out
    ${
        isFull
            ? "fixed top-0 left-0 w-screen h-screen bg-white z-[9999] scale-100 opacity-100 overflow-auto"
            : ""
    }
    `}
            >
                <button
                    type="button"
                    onClick={() => setIsFull(!isFull)}
                    className=" absolute top-2 right-2 p-2 rounded  hover:bg-gray-300"
                >
                    {isFull ? (
                        <ScreenNormalIcon size={24} />
                    ) : (
                        <ScreenFullIcon size={24} />
                    )}
                </button>

                <div className="w-full h-full p-3">
                    <div id="head" className="flex gap-2 items-center">
                        <RocketIcon size={20} />
                        <h3>
                            {dp?.event.title} - {dp?.event?.type}
                        </h3>
                        <LabelGroup>
                            <Label variant="success" size="large">
                                Users {dp?.user_count}
                            </Label>
                            <Label variant="accent" size="large">
                                Teams {dp?.team_count}
                            </Label>
                            <Label variant="attention" size="large">
                                Challenges {dp?.event_challenges.length}
                            </Label>
                        </LabelGroup>
                    </div>
                    <RemainingTimer
                        start_at={DatetimeToShow(dp?.event.start_time)}
                        end_at={DatetimeToShow(dp?.event.end_time)}
                    />
                    <div className="flex">
                        <div id="top-box" className="flex flex-col flex-8">
                            <div className="flex">
                                {/* 左边 ScoreBoard，顶对齐 */}
                                <div className="flex items-start flex-4">
                                    <ScoreBoard
                                        className="mt-3"
                                        data={dp.scoreboard_top10}
                                        enableDynamicColumns={false}
                                    />
                                </div>

                                {/* 右边 TrendChart，居中对齐 */}
                                <div className="flex items-center justify-center flex-9">
                                    <TrendChart
                                        data={dp?.trend}
                                        className=" w-full h-full"
                                    />
                                </div>
                            </div>
                            <div id="bottom-box" className="flex-8 ">
                                <EventChallengesView
                                    data={dp?.event_challenges}
                                />
                            </div>
                        </div>

                        <div id="side-bar" className="flex-2  p-2 ">
                            <h3>Recent Solves</h3>
                            <Timeline>
                                {dp.solved_recent_15.map((solve) => (
                                    <Timeline.Item key={solve.created_at}>
                                        <Timeline.Badge>
                                            <TriangleRightIcon />
                                        </Timeline.Badge>
                                        <Timeline.Body>
                                            <Text>
                                                <Text sx={{ fontWeight: 600 }}>
                                                    {solve.user_nickname}
                                                </Text>
                                                {" solved "}
                                                <Text
                                                    sx={{
                                                        fontFamily: "mono",
                                                        fontWeight: 900,
                                                        textDecoration:
                                                            "underline",
                                                    }}
                                                >
                                                    {solve.challenge_category}/
                                                    {solve.challenge_name}
                                                </Text>
                                                <br />
                                                <Text
                                                    sx={{ color: "fg.muted" }}
                                                >
                                                    @{" "}
                                                    <span>
                                                        {DatetimeToShow(
                                                            solve.created_at,
                                                        )}
                                                    </span>
                                                </Text>
                                            </Text>
                                        </Timeline.Body>
                                    </Timeline.Item>
                                ))}
                            </Timeline>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}
function ChallengeTile({
    item,
    className = "",
}: {
    item: DataEventChallenge;
    className?: string;
}) {
    const pct = Math.max(0, Math.min(100, Number(item.solved_percent) || 0));
    return (
        <div
            className={`group rounded-xl border border-gray-200 bg-white p-4 shadow-sm transition hover:shadow-md hover:bg-gray-50 ${className}`}
        >
            {/* Header */}
            <div className="flex items-start justify-between gap-3">
                <h4
                    className="text-sm font-semibold leading-tight line-clamp-2"
                    title={item.name}
                >
                    {item.name}
                </h4>
                <span className="shrink-0 rounded-full border border-blue-200 bg-blue-50 px-2 py-0.5 text-[11px] font-medium text-blue-700">
                    {item.category}
                </span>
            </div>

            {/* Metrics */}
            <div className="mt-3 grid grid-cols-3 gap-3 text-xs">
                <div>
                    <div className="font-semibold">
                        {item.points.toFixed(2)}
                    </div>
                    <div className="text-gray-500">points</div>
                </div>
                <div>
                    <div className="font-semibold">{item.solved_count}</div>
                    <div className="text-gray-500">solves</div>
                </div>
                <div>
                    <div className="font-semibold">
                        {(pct * 100).toFixed(2)}%
                    </div>
                    <div className="text-gray-500">solved</div>
                </div>
            </div>

            {/* Progress bar */}
            <div className="mt-3 h-1.5 w-full rounded-full bg-gray-100">
                <div
                    className="h-full rounded-full bg-green-500 transition-[width] duration-300 group-hover:bg-green-600"
                    style={{ width: `${pct * 100}%` }}
                />
            </div>
        </div>
    );
}

// Responsive grid that auto-fills available width (Primer/GitHub-like)
export function ChallengeGrid({
    items,
    className,
}: {
    items: DataEventChallenge[];
    className?: string;
}) {
    return (
        <div
            className={`grid gap-3 ${className}`}
            style={{
                gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
            }}
        >
            {items.map((it) => (
                <ChallengeTile key={`${it.category}-${it.name}`} item={it} />
            ))}
        </div>
    );
}

function EventChallengesView({ data }: { data: DataEventChallenge[] }) {
    console.log(data);
    return (
        <div className="mx-auto p-6">
            <h3 className="mb-3 text-lg font-semibold">Challenges Detail</h3>
            <ChallengeGrid items={data} />
        </div>
    );
}
