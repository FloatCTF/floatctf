import { CheckIcon } from "@primer/octicons-react";
import { ActionList, TreeView } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Fragment } from "react";

import { adminApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import type { EventTeamMemberRole, EventTeams } from "@/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/teams")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

export type TeamMemberResult = {
    username: string;
    nickname: string;
    role: EventTeamMemberRole;
    points: number;
};

export type TeamResult = {
    id: string;
    team: EventTeams;
    captain: string;
    members: TeamMemberResult[];
};

function RouteComponent() {
    const { id } = Route.useParams();
    const subject = `EventTeams-${id}`;
    const queryClient = useQueryClient();
    const banner = useMsgBanner({});
    const onDone = () => {
        queryClient.invalidateQueries({ queryKey: [subject] });
    };
    const bannedEventTeam = useMutation({
        mutationFn: ({ eventId, teamId, body }: {
            eventId: string;
            teamId: string;
            body: { reason?: string; durationSecs?: number };
        }) => adminApi.awd.banTeam(eventId, teamId, body),
        onSuccess: onDone,
        onError: banner.showErrorBanner,
    });

    const unbannedEventTeam = useMutation({
        mutationFn: ({ eventId, teamId }: { eventId: string; teamId: string }) =>
            adminApi.awd.unbanTeam(eventId, teamId),
        onSuccess: onDone,
        onError: banner.showErrorBanner,
    });

    // 封禁：询问时长（秒，留空/0 = 永久；>0 触发 P4-7 自动解封任务）
    const askBan = (teamId: string) => {
        const input = window.prompt(
            "AWD 跨层封禁（WG 挂起 + 防火墙 banned set + 连接清理）\n封禁时长（秒），留空为永久封禁；\n例如：3600 = 1 小时后自动解封",
        );
        if (input === null) return; // 取消
        const n = parseInt(input, 10);
        bannedEventTeam.mutate({
            eventId: id,
            teamId,
            body: {
                durationSecs: Number.isFinite(n) && n > 0 ? n : undefined,
            },
        });
    };

    const columns = [
        {
            accessorKey: "team.id",
            header: "Team ID",
            field: "team.id",
            rowHeader: true,
        },
        {
            accessorKey: "team.name",
            header: "Team Name",
            field: "team.name",
            sortBy: true,
        },
        {
            accessorKey: "team.points",
            header: "Points",
            field: "team.points",
            sortBy: true,
        },
        {
            accessorKey: "team.banned",
            header: "Banned",
            field: "team.banned",
            renderCell: (row: TeamResult) => {
                return <span>{row.team.banned ? <CheckIcon /> : <></>}</span>;
            },
            sortBy: true,
        },
        {
            accessorKey: "team.members",
            header: "Members",
            field: "team.members",
            renderCell: (row: TeamResult) => {
                return (
                    <table className="table-auto w-full border rounded">
                        <thead className="bg-gray-100">
                            <tr>
                                <th className="px-2 py-1 text-left">
                                    Username
                                </th>
                                <th className="px-2 py-1 text-left">
                                    Nickname
                                </th>
                                <th className="px-2 py-1 text-left">Role</th>
                                <th className="px-2 py-1 text-left">Points</th>
                            </tr>
                        </thead>
                        <tbody>
                            {row.members.map((row) => (
                                <Fragment key={row.username}>
                                    <tr className="cursor-pointer hover:bg-gray-50">
                                        <td className="px-2 py-1">
                                            {row.username}
                                        </td>
                                        <td className="px-2 py-1">
                                            {row.nickname}
                                        </td>
                                        <td className="px-2 py-1">
                                            {row.role}
                                        </td>
                                        <td className="px-2 py-1">
                                            {row.points}
                                        </td>
                                    </tr>
                                </Fragment>
                            ))}
                        </tbody>
                    </table>
                );
            },
        },
        {
            accessorKey: "team.created_at",
            header: "Created At",
            field: "team.created_at",
            renderCell: (row: TeamResult) => {
                return <span>{DatetimeToShow(row.team.created_at)}</span>;
            },
        },
    ];

    const columns_actions = (row: TeamResult) => {
        return (
            <ActionList>
                {row.team.banned ? (
                    <ActionList.Item
                        variant="default"
                        onSelect={() => {
                            unbannedEventTeam.mutate({
                                eventId: id,
                                teamId: row.team.id,
                            });
                        }}
                    >
                        Unbanned
                    </ActionList.Item>
                ) : (
                    <ActionList.Item
                        variant="danger"
                        onSelect={() => {
                            askBan(row.team.id);
                        }}
                    >
                        Ban (AWD)
                    </ActionList.Item>
                )}
            </ActionList>
        );
    };
    const filterKeys = ["id", "name", "points", "banned"];

    return (
        <div className="flex flex-col gap-2">
            <banner.BannerComponent />
            <GenericTable
                className="m-2"
                subject={subject}
                columns={columns}
                queryFn={adminApi.event_teams.getTeams(id)}
                removeFn={adminApi.event_teams.remove(id)}
                disablePagination={true}
                columnActions={columns_actions}
                getRowId={(row) => row.team.id}
                filterKeys={filterKeys}
            />
        </div>
    );
}
