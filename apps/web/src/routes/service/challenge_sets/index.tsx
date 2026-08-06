import { Link, createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { ChallengeSets } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/challenge_sets/")({
    component: RouteComponent,
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
                        to="/service/challenge_sets/$id"
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
                        to="/service/challenge_sets/$id"
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
    const filterKeys = ["name", "description"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.challenges.getChallengeSets}
            disableAdd={true}
            disablePagination={true}
            disableSelect={true}
            enableInternalActions={false}
        />
    );
}
