import { ActionList } from "@primer/react";
import { Label, useConfirm } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

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
    const confirmDialog = useConfirm();

    const onDone = () => {
        queryClient.invalidateQueries({ queryKey: [subject] });
        queryClient.invalidateQueries({ queryKey: ["admin-awd-scores", id] });
    };

    const banMutation = useMutation({
        mutationFn: ({ eventId, teamId }: {
            eventId: string;
            teamId: string;
        }) => adminApi.awd.banTeam(eventId, teamId, {}),
        onSuccess: () => {
            banner.showBanner("success", "Team banned");
            onDone();
        },
        onError: banner.showErrorBanner,
    });

    const unbanMutation = useMutation({
        mutationFn: ({ eventId, teamId }: { eventId: string; teamId: string }) =>
            adminApi.awd.unbanTeam(eventId, teamId),
        onSuccess: () => {
            banner.showBanner("success", "Team unbanned");
            onDone();
        },
        onError: banner.showErrorBanner,
    });

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
            header: "Score",
            field: "team.points",
            sortBy: true,
        },
        {
            accessorKey: "team.banned",
            header: "Ban",
            field: "team.banned",
            renderCell: (row: TeamResult) => (
                row.team.banned ? (
                    <Label variant="danger">Banned</Label>
                ) : (
                    <Label variant="default">Active</Label>
                )
            ),
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
                                <th className="px-2 py-1 text-left">Username</th>
                                <th className="px-2 py-1 text-left">Nickname</th>
                                <th className="px-2 py-1 text-left">Role</th>
                                <th className="px-2 py-1 text-left">Points</th>
                            </tr>
                        </thead>
                        <tbody>
                            {row.members.map((member) => (
                                <tr key={member.username} className="hover:bg-gray-50">
                                    <td className="px-2 py-1">{member.username}</td>
                                    <td className="px-2 py-1">{member.nickname}</td>
                                    <td className="px-2 py-1">{member.role}</td>
                                    <td className="px-2 py-1">{member.points}</td>
                                </tr>
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
                        onSelect={async () => {
                            const ok = await confirmDialog({
                                title: `Unban ${row.team.name}?`,
                                content: "Team will regain access to competition resources.",
                                confirmButtonType: "primary",
                            });
                            if (ok) {
                                unbanMutation.mutate({
                                    eventId: id,
                                    teamId: row.team.id,
                                });
                            }
                        }}
                    >
                        Unban
                    </ActionList.Item>
                ) : (
                    <ActionList.Item
                        variant="danger"
                        onSelect={async () => {
                            const ok = await confirmDialog({
                                title: `Ban ${row.team.name}?`,
                                content:
                                    "Team will lose all competition access (SSH, WireGuard, Flag submission, Reset). Ban is permanent until manually unbanned.",
                                confirmButtonType: "danger",
                            });
                            if (ok) {
                                banMutation.mutate({
                                    eventId: id,
                                    teamId: row.team.id,
                                });
                            }
                        }}
                    >
                        Ban
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