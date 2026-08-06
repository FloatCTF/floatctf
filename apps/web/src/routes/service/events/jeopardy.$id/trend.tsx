import type { UniResponse } from "@/api/axios";
import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/events/jeopardy/$id/trend")({
    component: RouteComponent,
});
export type TrendPoint = {
    name: string;
    score: number; // total score
    time: string; // NaiveDateTime 可以用 ISO 字符串
};

export type TrendItem = {
    name: string;
    points: TrendPoint[];
};
function RouteComponent() {
    const { id } = Route.useParams();
    const { data, isLoading, isError, error } = useQuery<
        UniResponse<TrendItem[]>,
        AxiosError<{ message: string }>
    >({
        queryKey: ["event_trend", id],
        queryFn: () => serviceApi.events.getTrend(id),
        refetchInterval: 30000, // 30秒自动刷新
    });
    if (isLoading) {
        return <Spinner size="large" />;
    }
    if (isError) {
        return <div>{error.response?.data.message}</div>;
    }
    return (
        <div className="w-full h-full flex">
            {data?.data && (
                <TrendChart
                    className="flex justify-center items-center"
                    data={data.data}
                />
            )}
        </div>
    );
}

import { useMemo, useState } from "react";
import {
    Brush,
    CartesianGrid,
    LabelList,
    Legend,
    Line,
    LineChart,
    ResponsiveContainer,
    Tooltip,
    XAxis,
    YAxis,
} from "recharts";

type TrendChartProps = {
    data: TrendItem[];
    className?: string;
};

// 动态 chart 数据类型
type ChartPoint = {
    time: string;
    [key: string]: string | number | undefined;
};

export const TrendChart: React.FC<TrendChartProps> = ({ data, className }) => {
    const stringToHue = (str: string) => {
        let hash = 0;
        for (let i = 0; i < str.length; i++) {
            hash = str.charCodeAt(i) + ((hash << 5) - hash);
            hash = hash & hash; // 转成32位整数
        }
        return Math.abs(hash) % 360; // 返回 0~359
    };

    // 根据名字生成固定颜色
    const getColor = (name: string) => `hsl(${stringToHue(name)}, 65%, 50%)`;

    // 缓存 chartData
    const chartData = useMemo(() => {
        if (!data || data.length === 0) return [];

        // 1. 构造时间集合
        const allTimesSet = new Set<string>();
        const userPointsMap: Record<string, Map<string, number>> = {};

        for (const user of data) {
            const map = new Map<string, number>();
            for (const p of user.points) {
                allTimesSet.add(p.time);
                map.set(p.time, p.score);
            }
            userPointsMap[user.name] = map;
        }

        const allTimes = Array.from(allTimesSet).sort();
        const lastTime = allTimes[allTimes.length - 1];

        // 2. 构造 chartData
        const chartData: ChartPoint[] = allTimes.map((time) => {
            const point: ChartPoint = { time };
            for (const user of data) {
                point[user.name] = userPointsMap[user.name].get(time) ?? 0;

                if (time === lastTime) {
                    point[`${user.name}_right_label`] = user.name;
                }
            }
            return point;
        });

        return chartData;
    }, [data]);

    // 处理图例点击/双击逻辑
    const [hiddenLines, setHiddenLines] = useState<Set<string>>(new Set());
    const [singleLine, setSingleLine] = useState<string | null>(null);

    // biome-ignore lint/suspicious/noExplicitAny: <explanation>
    const handleLegendClick = (entry: any) => {
        const name = entry.value;

        if (singleLine === name) {
            setSingleLine(null);
            setHiddenLines(new Set());
        } else if (singleLine === null && hiddenLines.size === 0) {
            setSingleLine(name);
            setHiddenLines(
                new Set(data.map((u) => u.name).filter((n) => n !== name)),
            );
        } else {
            const newHidden = new Set(hiddenLines);
            if (newHidden.has(name)) newHidden.delete(name);
            else newHidden.add(name);

            if (newHidden.size === data.length) newHidden.clear();

            setHiddenLines(newHidden);
            setSingleLine(null);
        }
    };

    return (
        <div className={`${className} w-full h-full`}>
            <ResponsiveContainer width="95%" height={400}>
                <LineChart
                    data={chartData}
                    margin={{ top: 20, right: 80, left: 20, bottom: 20 }}
                >
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis
                        dataKey="time"
                        tickFormatter={(time) => DatetimeToShow(time)}
                    />
                    <YAxis />
                    <Tooltip
                        isAnimationActive={false}
                        labelFormatter={(time) => DatetimeToShow(time)}
                        formatter={(value) => [
                            `${Number(value).toFixed(2)}`,
                            "Score",
                        ]}
                        itemSorter={(item) => -(item.value ?? 0)}
                    />
                    <Legend onClick={handleLegendClick} />
                    {data.map((user, idx) => (
                        <Line
                            key={user.name}
                            type="monotone"
                            dataKey={user.name}
                            stroke={getColor(user.name)}
                            dot={false}
                            connectNulls
                            hide={hiddenLines.has(user.name)}
                        >
                            <LabelList
                                dataKey={`${user.name}_right_label`}
                                position="right"
                                formatter={(v) => v}
                                style={{ fill: getColor(user.name) }}
                            />
                        </Line>
                    ))}
                    <Brush
                        dataKey="time"
                        height={30}
                        stroke="#8884d8"
                        tickFormatter={(time) => DatetimeToShow(time)}
                    />
                </LineChart>
            </ResponsiveContainer>
        </div>
    );
};
