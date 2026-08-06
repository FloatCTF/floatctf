import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { EventAnnouncements } from "@/entity";

import { DatetimeToShow } from "@/util";
import { TextInput, Textarea } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/announcements")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const subject = `announcement: ${id}`;
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        { accessorKey: "title", header: "Title", field: "title" },
        { accessorKey: "content", header: "Content", field: "content" },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            renderCell: (row: EventAnnouncements) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
    ];
    const mutationEventAnnouncement = useReactive<Partial<EventAnnouncements>>({
        title: "",
        content: "",
    });
    const mutationColumns = [
        {
            header: "title",
            field: "title",
            render: (
                <TextInput
                    value={mutationEventAnnouncement.title}
                    onChange={(e) => {
                        mutationEventAnnouncement.title = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "content",
            field: "content",
            render: (
                <Textarea
                    value={mutationEventAnnouncement.content}
                    onChange={(e) => {
                        mutationEventAnnouncement.content = e.target.value;
                    }}
                />
            ),
        },
    ];

    const filterKeys = ["id", "title", "content"];

    return (
        <GenericTable
            className="m-2"
            subject={subject}
            queryFn={adminApi.event_announcements.fetch(id)}
            createFn={adminApi.event_announcements.create(id)}
            patchFn={adminApi.event_announcements.patch(id)}
            removeFn={adminApi.event_announcements.remove(id)}
            mutationData={mutationEventAnnouncement}
            columns={columns}
            mutationColumns={mutationColumns}
            filterKeys={filterKeys}
        />
    );
}
