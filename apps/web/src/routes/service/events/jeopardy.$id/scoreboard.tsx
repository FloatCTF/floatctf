import type { UniResponse } from "@/api/axios";
import {
    CheckIcon,
    SparkleFillIcon,
    SparkleIcon,
    SparklesFillIcon,
} from "@primer/octicons-react";
import { Avatar, Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";

export const Route = createFileRoute("/service/events/jeopardy/$id/scoreboard")(
    {
        component: RouteComponent,
    },
);
export type ChallengeScoreboard = {
    name: string;
    solved: boolean;
    solved_no: number;
};

export type ScoreboardItem = {
    id: string;
    no: number;
    name: string;
    avatar?: string;
    score: number;
    solved_count: number;
    challenges: ChallengeScoreboard[];
};

function RouteComponent() {
    const { id } = Route.useParams();
    const { data, isLoading, isError, error } = useQuery<
        UniResponse<ScoreboardItem[]>,
        AxiosError<{ message: string }>
    >({
        queryKey: ["event_scoreboard", id],
        queryFn: () => serviceApi.events.getScoreboard(id),
        refetchInterval: 30000, // 30秒自动刷新
    });
    if (isLoading) {
        return <Spinner size="large" />;
    }

    if (isError) {
        return <div>{error.response?.data.message ?? error.message}</div>;
    }

    return <ScoreBoard data={data?.data ?? []} className="mt-2" />;
}

export function ScoreBoard({
    data,
    enableDynamicColumns = true,
    className,
}: {
    data: ScoreboardItem[];
    enableDynamicColumns?: boolean;
    className?: string;
}) {
    const baseColumns = [
        {
            accessorKey: "no",
            header: "Rank",
            field: "no",
        },
        {
            accessorKey: "name",
            header: "Name",
            field: "name",
            rowHeader: true,
            renderCell: (row: ScoreboardItem) => (
                <div className="flex items-center gap-2">
                    {row.avatar ? (
                        <Avatar src={row.avatar} size={24} />
                    ) : (
                        <div
                            className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium shrink-0"
                            style={{ width: 24, height: 24, fontSize: 10 }}
                        >
                            {row.name?.[0]?.toUpperCase() || "?"}
                        </div>
                    )}
                    <span>{row.name}</span>
                </div>
            ),
        },
        {
            accessorKey: "score",
            header: "Score",
            field: "score",
            renderCell: (row: ScoreboardItem) => (
                <span>{row.score.toFixed(2)}</span>
            ),
        },
        {
            accessorKey: "solved_count",
            header: "Solved",
            field: "solved_count",
        },
    ];
    let columns = baseColumns;
    if (enableDynamicColumns) {
        // 根据 challenges 动态生成列
        const challengeColumns =
            data && data.length > 0
                ? data[0].challenges.map((ch) => ({
                      accessorKey: `challenge_${ch.name}`,
                      header: ch.name,
                      field: `challenge_${ch.name}`,
                      renderCell: (row: ScoreboardItem) => {
                          const challenge = row.challenges.find(
                              (c) => c.name === ch.name,
                          );
                          if (!challenge) return null;
                          if (challenge.solved) {
                              if (challenge.solved_no === 1)
                                  return <SparklesFillIcon size={16} />;
                              if (challenge.solved_no === 2)
                                  return <SparkleFillIcon size={16} />;
                              if (challenge.solved_no === 3)
                                  return <SparkleIcon size={16} />;
                          }
                          return challenge.solved ? (
                              <CheckIcon size={16} />
                          ) : (
                              <></>
                          );
                      },
                  }))
                : [];
        // @ts-ignore
        columns = [...baseColumns, ...challengeColumns];
    } else {
        columns = baseColumns;
    }

    const table = useReactTable({
        data: data ?? [],
        columns,
        getCoreRowModel: getCoreRowModel(),
    });

    return (
        <Table.Container className={`${className} `}>
            <Table.Subtitle id="repositories-subtitle-titleSubtitle">
                <div className="flex gap-2 ">
                    <span>
                        First Blood: <SparklesFillIcon size={16} />
                    </span>
                    <span>
                        2nd solve: <SparkleFillIcon size={16} />
                    </span>
                    <span>
                        3rd solve: <SparkleIcon size={16} />
                    </span>
                    <span>
                        Solved: <CheckIcon size={16} />
                    </span>
                </div>
            </Table.Subtitle>
            <DataTable
                aria-labelledby="scoreboard"
                // @ts-ignore
                columns={columns}
                data={table.getRowModel().rows.map((row) => row.original)}
            />
        </Table.Container>
    );
}
