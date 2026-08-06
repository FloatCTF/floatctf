import { Link, createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { Challenges } from "@/entity";
import { ServiceRouteGuard } from "@/routes/service/route";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/challenges/")({
    component: RouteComponent,
    loader: ServiceRouteGuard,
});

function RouteComponent() {
    useTitle("Challenges | FloatCTF");
    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "id",
            rowHeader: true,
            renderCell: (row: Challenges) => {
                return (
                    <Link
                        to={"/service/challenges/$id"}
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
            rowHeader: true,
            renderCell: (row: Challenges) => {
                return (
                    <Link
                        to={"/service/challenges/$id"}
                        params={{ id: row.id }}
                    >
                        {row.name}
                    </Link>
                );
            },
        },

        {
            accessorKey: "category",
            header: "Category",
            field: "category",
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
    const filterKeys = ["name", "category", "description"];

    return (
        <GenericTable
            subject="Challenges"
            subtitle="If you want submit yours, pls visit https://github.com/FloatCTF/challenge-template"
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.challenges.fetch}
            enableInternalActions={false}
            disableAdd={true}
            disableSelect={true}
        />
    );
}
