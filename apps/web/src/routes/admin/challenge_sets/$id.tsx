import { useCallback, useRef, useState } from "react";

import { adminApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import { DatetimeToShow } from "@/util";
import type { Challenges } from "@/entity";
import { AdminRouteGuard } from "@/routes/admin/route";
import { useSelectedRowIds } from "@/util";
import { Dialog } from "@primer/react/experimental";
import { Button } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/admin/challenge_sets/$id")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const queryClient = useQueryClient();
    const subject = `Challenge Set #${id}`;
    const banner = useMsgBanner();
    const [challengeSetSelectedRowIds, setChallengeSetSelectedRowIds] =
        useSelectedRowIds();

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
            accessorKey: "hidden",
            header: "Hidden",
            field: "hidden",
            renderCell: (row: Challenges) => {
                return <span>{row.hidden ? "✓" : <></>}</span>;
            },
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

    const filterKeys = ["name", "category", "hidden"];

    const custom_actions = (
        <div className="flex gap-1">
            <AddChallengeButton
                set_id={id}
                refresh_query_key={subject}
                banner={banner}
            />
        </div>
    );

    return (
        <div className="flex gap-2 m-2 items-start">
            <GenericTable
                subject={subject}
                columns={columns}
                filterKeys={filterKeys}
                getRowId={(row) => row.id}
                queryFn={adminApi.challenges.getChallengeSet(id)}
                removeFn={adminApi.challenges.removeChallengeFromSet(id)}
                selectedRowIds={challengeSetSelectedRowIds}
                onSelectedRowIdsChange={setChallengeSetSelectedRowIds}
                customActions={custom_actions}
                disableAdd={true}
                externalBanner={banner}
            />
        </div>
    );
}

function AddChallengeButton({
    set_id,
    refresh_query_key,
    banner,
}: {
    set_id: string;
    refresh_query_key?: string;
    banner: ReturnType<typeof useMsgBanner>;
}) {
    const queryClient = useQueryClient();
    const [isOpen, setIsOpen] = useState(false);
    const buttonRef = useRef<HTMLButtonElement>(null);
    const onDialogClose = useCallback(() => setIsOpen(false), []);
    const [userSelectedRowIds, setUserSelectedRowIds] = useSelectedRowIds();

    const addChallengeToSetMutation = useMutation({
        mutationFn: adminApi.challenges.addChallengeToSet,
        onSuccess: () => {
            if (refresh_query_key) {
                queryClient.invalidateQueries({
                    queryKey: [refresh_query_key],
                });
            }
            banner.showBanner("success", "Add Challenges to Set Success");
            setIsOpen(false);
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    const user_op_actions = (
        <Button
            variant="primary"
            onClick={() => {
                addChallengeToSetMutation.mutate({
                    set_id: set_id,
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
            accessorKey: "hidden",
            header: "Hidden",
            field: "hidden",
            renderCell: (row: Challenges) => {
                return <span>{row.hidden ? "✓" : <></>}</span>;
            },
        },
    ];

    const filterKeys = ["name", "id", "category"];

    return (
        <>
            {isOpen && (
                <Dialog title="Add Challenges to Set" onClose={onDialogClose}>
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
                Add Challenges
            </Button>
        </>
    );
}
