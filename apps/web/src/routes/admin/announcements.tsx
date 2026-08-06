import { TextInput, Textarea } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import { type Announcements } from "@/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/announcements")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    useTitle("Announcements | FloatCTF");
    const subject = "Announcements";
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "title",
            header: "Title",
            field: "title",
            sortBy: true,
        },
        {
            accessorKey: "content",
            header: "Content",
            field: "content",
            renderCell: (row: Announcements) => {
                return (
                    <div
                        style={{
                            whiteSpace: "pre-wrap",
                            wordBreak: "break-word",
                            textAlign: "left",
                        }}
                    >
                        {row.content || "—"}
                    </div>
                );
            },
        },
        {
            accessorKey: "publisher",
            header: "Publisher",
            field: "publisher",
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            sortBy: true,
            renderCell: (row: Announcements) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Announcements) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];
    const mutationAnnouncement = useReactive<Partial<Announcements>>({
        title: "",
        content: "",
    });
    const mutationColumns = [
        {
            header: "title",
            field: "title",
            render: (
                <TextInput
                    value={mutationAnnouncement.title}
                    onChange={(e) => {
                        mutationAnnouncement.title = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "content",
            field: "content",
            render: (
                <Textarea
                    value={mutationAnnouncement.content}
                    onChange={(e) => {
                        mutationAnnouncement.content = e.target.value;
                    }}
                />
            ),
        },
    ];
    const filterKeys = ["id", "title", "content"];
    return (
        <GenericTable
            subject={subject}
            columns={columns}
            mutationColumns={mutationColumns}
            mutationData={mutationAnnouncement}
            queryFn={adminApi.announcements.fetch}
            createFn={adminApi.announcements.create}
            removeFn={adminApi.announcements.remove}
            patchFn={adminApi.announcements.patch}
            filterKeys={filterKeys}
        />
    );
}
