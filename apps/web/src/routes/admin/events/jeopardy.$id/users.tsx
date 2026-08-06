import { CheckIcon } from "@primer/octicons-react";
import { ActionList, Button, Dialog } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { ActionSelect, GenericTable, useMsgBanner } from "@/components";
import type { EventUsers, Users } from "@/entity";
import { DatetimeToShow, useSelectedRowIds } from "@/util";
import { useCallback, useRef, useState } from "react";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/jeopardy/$id/users")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

export type EventUserResult = {
    id: string;
    user: Users;
    event_user: EventUsers;
};

function RouteComponent() {
    const { id } = Route.useParams();
    const subject = `event_users: ${id}`;

    const queryClient = useQueryClient();
    const bannedEventUser = useMutation({
        mutationFn: adminApi.event_users.banned,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: [subject] });
        },
    });

    const unbannedEventUser = useMutation({
        mutationFn: adminApi.event_users.unbanned,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: [subject] });
        },
    });

    const columns = [
        {
            accessorKey: "user.id",
            header: "ID",
            field: "user.id",
            rowHeader: true,
        },
        {
            accessorKey: "user.username",
            header: "Username",
            field: "user.username",
        },
        {
            accessorKey: "user.nickname",
            header: "Nickname",
            field: "user.nickname",
        },
        {
            accessorKey: "event_user.points",
            header: "Points",
            field: "event_user.points",
            sortBy: true,
        },
        {
            accessorKey: "event_user.banned",
            header: "Banned",
            field: "event_user.banned",
            renderCell: (row: EventUserResult) => {
                return (
                    <span>{row.event_user.banned ? <CheckIcon /> : <></>}</span>
                );
            },
            sortBy: true,
        },
        {
            accessorKey: "event_user.joined_at",
            header: "Joined At",
            field: "event_user.joined_at",
            renderCell: (row: EventUserResult) => {
                return <span>{DatetimeToShow(row.event_user.joined_at)}</span>;
            },
            sortBy: true,
        },
    ];

    const columns_actions = (row: EventUserResult) => {
        return (
            <ActionList>
                {row.event_user.banned ? (
                    <ActionList.Item
                        variant="default"
                        onSelect={() => {
                            unbannedEventUser.mutate({
                                event_id: id,
                                user_id: row.user.id,
                            });
                        }}
                    >
                        Unbanned
                    </ActionList.Item>
                ) : (
                    <ActionList.Item
                        variant="danger"
                        onSelect={() => {
                            bannedEventUser.mutate({
                                event_id: id,
                                user_id: row.user.id,
                            });
                        }}
                    >
                        Banned
                    </ActionList.Item>
                )}
            </ActionList>
        );
    };

    const custom_actions = (
        <div className="flex gap-1">
            <AddUserButton event_id={id} refresh_query_key={subject} />
        </div>
    );

    const filterKeys = ["user_id", "username", "nickname", "points", "banned"];

    return (
        <div className="flex gap-2 m-2 items-start">
            <GenericTable
                subject={subject}
                queryFn={adminApi.event_users.fetch(id)}
                removeFn={adminApi.event_users.delete(id)}
                columns={columns}
                disableAdd={true}
                // disablePagination={true}

                columnActions={columns_actions}
                getRowId={(row: EventUserResult) => row.user.id}
                customActions={custom_actions}
                filterKeys={filterKeys}
            />
        </div>
    );
}

function AddUserButton({
    event_id,
    refresh_query_key,
}: {
    event_id: string;
    refresh_query_key?: string;
}) {
    const queryClient = useQueryClient();
    const [isOpen, setIsOpen] = useState(false);
    const buttonRef = useRef<HTMLButtonElement>(null);
    const onDialogClose = useCallback(() => setIsOpen(false), []);
    // for users
    const user_columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "username",
            header: "Username",
            field: "username",
            sortBy: true,
        },
        {
            accessorKey: "nickname",
            header: "Nickname",
            field: "nickname",
            sortBy: true,
        },
    ];

    const [userSelectedRowIds, setUserSelectedRowIds] = useSelectedRowIds();
    const banner = useMsgBanner();
    const addEventUserMutation = useMutation({
        mutationFn: adminApi.event_users.add,
        onSuccess: () => {
            banner.showBanner("success", "Add event user success");
            queryClient.invalidateQueries({ queryKey: [refresh_query_key] });
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const user_op_actions = (
        <Button
            variant="primary"
            onClick={() => {
                addEventUserMutation.mutate({
                    event_id: event_id,
                    user_id_list: Array.from(userSelectedRowIds),
                });
            }}
        >
            Add
        </Button>
    );
    const filterKeys = ["id", "username", "nickname"];
    return (
        <>
            {isOpen && (
                <Dialog title="Add Event Users" onClose={onDialogClose}>
                    <GenericTable
                        subject="Users"
                        columns={user_columns}
                        queryFn={adminApi.users.fetch}
                        filterKeys={filterKeys}
                        disableAdd={true}
                        enableInternalActions={false}
                        selectedRowIds={userSelectedRowIds}
                        onSelectedRowIdsChange={setUserSelectedRowIds}
                        customActions={user_op_actions}
                        externalBanner={banner}
                    />
                </Dialog>
            )}
            <Button
                variant="primary"
                ref={buttonRef}
                onClick={() => setIsOpen(!isOpen)}
            >
                Add Event Users
            </Button>
        </>
    );
}
