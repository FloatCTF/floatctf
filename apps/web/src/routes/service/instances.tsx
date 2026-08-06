import { Button } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import type { Instances } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/instances")({
    component: RouteComponent,
});

function RouteComponent() {
    useTitle("Instances | FloatCTF");

    const subject = "Instances";
    const banner = useMsgBanner();
    const queryClient = useQueryClient();

    const filterKeys = ["id", "status", "ref", "challenge_id", "gamebox_id"];

    const mutationInstance = useMutation({
        mutationFn: serviceApi.instances.destroy,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: [subject] });
            banner.showBanner("success", "Instance destroyed successfully");
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    const columns = [
        {
            accessorKey: "challenge_id",
            header: "Challenge",
            field: "challenge_id",
            rowHeader: true,
            renderCell: (row: Instances) => {
                return row.challenge_id ? (
                    <Link
                        to={"/service/challenges/$id"}
                        params={{ id: row.challenge_id }}
                    >
                        {row.challenge_id}
                    </Link>
                ) : (
                    <span>—</span>
                );
            },
        },
        {
            accessorKey: "status",
            header: "Status",
            field: "status",
        },
        {
            accessorKey: "ref",
            header: "Ref",
            field: "ref",
        },
        {
            accessorKey: "user_id",
            header: "User",
            field: "user_id",
        },
        {
            accessorKey: "destroy_at",
            header: "Destroy At",
            field: "destroy_at",
            renderCell: (row: Instances) => {
                return <span>{DatetimeToShow(row.destroy_at)}</span>;
            },
        },
        {
            accessorKey: "action",
            header: "Action",
            field: "action",
            renderCell: (row: Instances) => {
                return (
                    <Button
                        variant="invisible"
                        onClick={() => {
                            mutationInstance.mutate(row.id);
                        }}
                        style={{ color: "#DB0000" }}
                    >
                        Destroy
                    </Button>
                );
            },
        },
    ];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.instances.fetch}
            enableInternalActions={false}
            externalBanner={banner}
            disableAdd={true}
            disableSelect={true}
        />
    );
}
