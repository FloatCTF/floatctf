import dayjs from "dayjs";
import { TrashIcon } from "@primer/octicons-react";
import { ActionList } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi, type ImageInfo } from "@/api";
import { GenericTable } from "@/components";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/docker/images")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const queryClient = useQueryClient();

    const formatSize = (bytes: number): string => {
        if (bytes < 1024) return bytes + " B";
        if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(2) + " KB";
        if (bytes < 1024 * 1024 * 1024)
            return (bytes / (1024 * 1024)).toFixed(2) + " MB";
        return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
    };

    const columns = [
        { accessorKey: "id", header: "Image ID", field: "id", rowHeader: true },
        {
            accessorKey: "repo_tags",
            header: "Tags",
            field: "repo_tags",
            renderCell: (row: ImageInfo) => (
                <span>{row.repo_tags.join(", ") || "<none>"}</span>
            ),
        },
        {
            accessorKey: "size",
            header: "Size",
            field: "size",
            renderCell: (row: ImageInfo) => <span>{formatSize(row.size)}</span>,
        },
        {
            accessorKey: "created",
            header: "Created",
            field: "created",
            renderCell: (row: ImageInfo) => (
                <span>
                    {dayjs(row.created * 1000).format("YYYY-MM-DD HH:mm")}
                </span>
            ),
        },
    ];

    const filterKeys = ["name", "reference"];

    const deleteMutation = useMutation({
        mutationFn: (image_id: string) => adminApi.docker.deleteImage(image_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Images"] });
        },
    });

    const columnActions = (row: ImageInfo) => (
        <ActionList>
            <ActionList.Item
                variant="danger"
                onClick={() => {
                    if (
                        confirm(
                            `Delete image ${row.repo_tags.join(", ") || row.id}?`,
                        )
                    ) {
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
            subject="Images"
            columns={columns}
            filterKeys={filterKeys}
            queryFn={adminApi.docker.fetchImages}
            enableInternalActions={true}
            disableAdd={true}
            disableSelect={true}
            columnActions={columnActions}
            getRowId={(row) => row.id}
        />
    );
}
