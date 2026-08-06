import { PlayIcon, SquareIcon, TrashIcon } from "@primer/octicons-react";
import { ActionList, IconButton } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi, type FloatDockerContainer } from "@/api";
import { GenericTable } from "@/components";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/docker/")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const queryClient = useQueryClient();

    const columns = [
        {
            accessorKey: "id",
            header: "Container ID",
            field: "id",
            rowHeader: true,
        },
        { accessorKey: "name", header: "Name", field: "name" },
        { accessorKey: "image", header: "Image", field: "image" },
        { accessorKey: "status", header: "Status", field: "status" },
        { accessorKey: "ports", header: "Ports", field: "ports" },
        { accessorKey: "created", header: "Created", field: "created" },
    ];

    const filterKeys = ["id", "name", "status"];

    const stopMutation = useMutation({
        mutationFn: (container_id: string) =>
            adminApi.docker.stopContainer(container_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Containers"] });
        },
    });

    const startMutation = useMutation({
        mutationFn: (container_id: string) =>
            adminApi.docker.startContainer(container_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Containers"] });
        },
    });

    const deleteMutation = useMutation({
        mutationFn: (container_id: string) =>
            adminApi.docker.deleteContainer(container_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Containers"] });
        },
    });

    const columnActions = (row: FloatDockerContainer) => (
        <ActionList>
            {(row.status === "running" || row.status === "Running") && (
                <ActionList.Item onClick={() => stopMutation.mutate(row.id)}>
                    Stop
                </ActionList.Item>
            )}
            {row.status !== "running" && row.status !== "Running" && (
                <ActionList.Item onClick={() => startMutation.mutate(row.id)}>
                    Start
                </ActionList.Item>
            )}
            <ActionList.Item
                variant="danger"
                onClick={() => {
                    if (confirm(`Delete container ${row.name}?`)) {
                        deleteMutation.mutate(row.id);
                    }
                }}
            >
                Delete
            </ActionList.Item>
        </ActionList>
    );

    return (
        <GenericTable
            className="m-2"
            subject="Containers"
            columns={columns}
            filterKeys={filterKeys}
            queryFn={adminApi.docker.fetchContainers}
            enableInternalActions={true}
            disableAdd={true}
            columnActions={columnActions}
            getRowId={(row) => row.id}
        />
    );
}
