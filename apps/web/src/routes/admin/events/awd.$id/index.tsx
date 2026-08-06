import { CheckIcon, KebabHorizontalIcon } from "@primer/octicons-react";
import {
    ActionList,
    ActionMenu,
    Button,
    ButtonGroup,
    Dialog,
    IconButton,
} from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useCallback, useContext, useRef, useState } from "react";

import { adminApi } from "@/api";
import { ActionSelect, GenericTable, useMsgBanner } from "@/components";
import type { Challenges } from "@/entity";
import { CheckButton } from "@/routes/admin/challenges";
import { DatetimeToShow, useSelectedRowIds } from "@/util";
import { EventContext } from "./route";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

export type EventChallenge = {
    event_id: string;
    challenge_id: string;
    hidden: boolean;
    points: number;
};

export type EventChallengeResult = {
    id: string;
    event_challenge: EventChallenge;
    challenge: Challenges;
};

function RouteComponent() {
    const event = useContext(EventContext);
    const { id } = Route.useParams();
    const queryClient = useQueryClient();
    const subject = `event_challenges: ${id}`;
    const banner = useMsgBanner();
    const open_event_challenge = useMutation({
        mutationFn: adminApi.event_challenges.open,
        onSuccess: () => {
            queryClient.invalidateQueries({
                queryKey: [subject],
            });
        },
    });
    const hidden_event_challenge = useMutation({
        mutationFn: adminApi.event_challenges.hidden,
        onSuccess: () => {
            queryClient.invalidateQueries({
                queryKey: [subject],
            });
        },
    });

    const columns = [
        {
            accessorKey: "challenge.id",
            header: "Challenge ID",
            field: "challenge.id",
            rowHeader: true,
        },
        {
            accessorKey: "challenge.name",
            header: "Challenge Name",
            field: "challenge.name",
            sortBy: true,
        },
        {
            accessorKey: "challenge.category",
            header: "Challenge Category",
            field: "challenge.category",
            sortBy: true,
        },
        {
            accessorKey: "event_challenge.points",
            header: "Challenge Points",
            field: "event_challenge.points",
            sortBy: true,
        },
        {
            accessorKey: "event_challenge.hidden",
            header: "Hidden",
            field: "event_challenge.hidden",

            renderCell: (row: EventChallengeResult) => {
                return (
                    <span>
                        {row.event_challenge.hidden ? <CheckIcon /> : <></>}
                    </span>
                );
            },
            sortBy: true,
        },
    ];

    const columns_action = (row: EventChallengeResult) => {
        return (
            <ActionList>
                <ActionList.Item
                    key={`${row.id}-edit`}
                    onClick={() => {
                        if (row.event_challenge.hidden) {
                            open_event_challenge.mutate({
                                event_id: id,
                                challenge_id: row.challenge.id,
                            });
                        } else {
                            hidden_event_challenge.mutate({
                                event_id: id,
                                challenge_id: row.challenge.id,
                            });
                        }
                    }}
                >
                    {row.event_challenge.hidden ? "Open" : "Hide"}
                </ActionList.Item>
            </ActionList>
        );
    };

    const [eventChallengeSelectedRowIds, setEventChallengeSelectedRowIds] =
        useSelectedRowIds();
    const custom_actions = (
        <div className="flex gap-1">
            <OpenChallengesButton
                event_id={id}
                refresh_query_key={subject}
                banner={banner}
                challenge_id_list={Array.from(eventChallengeSelectedRowIds)}
            />
            <CreateChallengeSetButton
                name={event?.title ?? "Challenge Set"}
                description={event?.description ?? "Challenge Description"}
                banner={banner}
                challenge_id_list={Array.from(eventChallengeSelectedRowIds)}
            />
            <AddChallengeButton event_id={id} refresh_query_key={subject} />
        </div>
    );
    const filterKeys = ["name", "challenge_id", "hidden", "category"];
    return (
        <div className="flex gap-2 m-2 items-start">
            <GenericTable
                subject={subject}
                columns={columns}
                filterKeys={filterKeys}
                getRowId={(row) => row.challenge.id}
                queryFn={adminApi.event_challenges.fetch(id)}
                removeFn={adminApi.event_challenges.remove(id)}
                selectedRowIds={eventChallengeSelectedRowIds}
                onSelectedRowIdsChange={setEventChallengeSelectedRowIds}
                columnActions={columns_action}
                customActions={custom_actions}
                disableAdd={true}
                externalBanner={banner}
            />
        </div>
    );
}

function AddChallengeButton({
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
    const [userSelectedRowIds, setUserSelectedRowIds] = useSelectedRowIds();
    const banner = useMsgBanner();
    const addEventChallengesMutation = useMutation({
        mutationFn: adminApi.event_challenges.add,
        onSuccess: () => {
            if (refresh_query_key) {
                queryClient.invalidateQueries({
                    queryKey: [refresh_query_key],
                });
            }
            banner.showBanner("success", "Add Event Challenges Success");
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const user_op_actions = (
        <Button
            variant="primary"
            onClick={() => {
                addEventChallengesMutation.mutate({
                    event_id: event_id,
                    challenge_id_list: Array.from(userSelectedRowIds),
                });
            }}
        >
            Add
        </Button>
    );
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        { accessorKey: "name", header: "Name", field: "name", sortBy: true },
        {
            accessorKey: "category",
            header: "Category",
            field: "category",
            sortBy: true,
        },

        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            renderCell: (row: Challenges) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];
    const filterKeys = ["name", "id", "category"];
    return (
        <>
            {isOpen && (
                <Dialog title="Add Event Challenges" onClose={onDialogClose}>
                    <GenericTable
                        subject="Challenges"
                        columns={columns}
                        queryFn={adminApi.challenges.fetch}
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
                Add Event Challenges
            </Button>
        </>
    );
}

function CreateChallengeSetButton({
    name,
    description,
    challenge_id_list,
    banner,
}: {
    name: string;
    description?: string;
    challenge_id_list: string[];
    banner: ReturnType<typeof useMsgBanner>;
}) {
    const createChallengeSetMutation = useMutation({
        mutationFn: adminApi.events.createChallengeSet,
        onSuccess: () => {
            banner.showBanner(
                "success",
                `Create Challenge Set Success: ${name} #${challenge_id_list.length}`,
            );
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    return (
        <>
            <Button
                variant="primary"
                onClick={() => {
                    if (challenge_id_list.length === 0) {
                        banner.showBanner(
                            "critical",
                            "Please select at least one challenge",
                        );
                        return;
                    }
                    createChallengeSetMutation.mutate({
                        name: name,
                        description: description,
                        challenge_id_list: challenge_id_list,
                    });
                }}
            >
                As Challenge Set
            </Button>
        </>
    );
}

function OpenChallengesButton({
    event_id,
    refresh_query_key,
    banner,
    challenge_id_list,
}: {
    event_id: string;
    refresh_query_key?: string;
    banner: ReturnType<typeof useMsgBanner>;
    challenge_id_list: string[];
}) {
    const queryClient = useQueryClient();
    const openEventChallengesMutation = useMutation({
        mutationFn: adminApi.event_challenges.open,
        onSuccess: () => {
            if (refresh_query_key) {
                queryClient.invalidateQueries({
                    queryKey: [refresh_query_key],
                });
            }
            banner.showBanner(
                "success",
                `Open Event Challenges Success: ${challenge_id_list.length}`,
            );
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    return (
        <Button
            onClick={() => {
                openEventChallengesMutation.mutate({
                    event_id: event_id,
                    challenge_id_list: challenge_id_list,
                });
            }}
        >
            Open Challenges
        </Button>
    );
}
