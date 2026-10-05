import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { Announcements } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/announcements")({
    component: RouteComponent,
});

function RouteComponent() {
    useTitle("Announcements | FloatCTF");

    const subject = "Announcements";

    const columns = [
        {
            accessorKey: "publisher",
            header: "Publisher",
            field: "publisher",
        },
        {
            accessorKey: "title",
            header: "标题",
            field: "title",
            sortBy: true,
        },
        {
            accessorKey: "content",
            header: "内容",
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
            accessorKey: "created_at",
            header: "创建时间",
            field: "created_at",
            sortBy: true,
            renderCell: (row: Announcements) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
        {
            accessorKey: "updated_at",
            header: "更新时间",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Announcements) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];

    const filterKeys = ["id", "title", "content"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.announcements.fetch}
            enableInternalActions={false}
            disableAdd={true}
            disableSelect={true}
        />
    );
}
