import { TextInput, Truncate } from "@primer/react";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { ChallengeSets } from "@/entity";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/admin/challenge_sets/")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const subject = "Challenge Sets";
    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "id",
            rowHeader: true,
            renderCell: (row: ChallengeSets) => {
                return (
                    <Link
                        to="/admin/challenge_sets/$id"
                        params={{ id: row.id }}
                    >
                        {row.id}
                    </Link>
                );
            },
        },
        {
            accessorKey: "name",
            header: "Name",
            field: "name",
            renderCell: (row: ChallengeSets) => {
                return (
                    <Link
                        to="/admin/challenge_sets/$id"
                        params={{ id: row.id }}
                    >
                        {row.name}
                    </Link>
                );
            },
            sortBy: true,
        },
        {
            accessorKey: "description",
            header: "Description",
            field: "description",
            sortBy: true,
            renderCell: (row: ChallengeSets) => {
                return <Truncate title={row.description ?? ""} />;
            },
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            sortBy: true,
            renderCell: (row: ChallengeSets) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
    ];
    const mutationChallengeSet = useReactive<Partial<ChallengeSets>>({
        name: "",
        description: "",
    });
    const mutationColumns = [
        {
            header: "name",
            field: "name",
            render: (
                <TextInput
                    value={mutationChallengeSet.name}
                    onChange={(e) => {
                        mutationChallengeSet.name = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "description",
            field: "description",
            render: (
                <TextInput
                    value={mutationChallengeSet.description}
                    onChange={(e) => {
                        mutationChallengeSet.description = e.target.value;
                    }}
                />
            ),
        },
    ];
    const filterKeys = ["id", "name", "description"];
    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={adminApi.challenges.getChallengeSets}
            createFn={adminApi.challenges.createChallengeSet}
            removeFn={adminApi.challenges.deleteChallengeSet}
            patchFn={adminApi.challenges.patchChallengeSet}
            mutationColumns={mutationColumns}
            mutationData={mutationChallengeSet}
        />
    );
}
