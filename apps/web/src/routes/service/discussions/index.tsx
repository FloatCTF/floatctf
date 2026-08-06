import { ThumbsupIcon, CommentIcon, EyeIcon } from "@primer/octicons-react";
import { Avatar } from "@primer/react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { Discussions } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/discussions/")({
    component: RouteComponent,
});

function RouteComponent() {
    useTitle("Discussions | FloatCTF");

    const subject = "Discussions";

    const columns = [
        {
            accessorKey: "author_nickname",
            header: "Author",
            field: "author_nickname",
            renderCell: (row: Discussions) => (
                <div className="flex items-center gap-2">
                    {row.author_avatar ? (
                        <Avatar src={row.author_avatar} size={24} />
                    ) : (
                        <div
                            className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium shrink-0"
                            style={{ width: 24, height: 24, fontSize: 10 }}
                        >
                            {row.author_nickname?.[0]?.toUpperCase() || "?"}
                        </div>
                    )}
                    <span>{row.author_nickname}</span>
                </div>
            ),
        },
        {
            accessorKey: "title",
            header: "Title",
            field: "title",
            sortBy: true,
            renderCell: (row: Discussions) => (
                <Link
                    to="/service/discussions/$id"
                    params={{ id: row.id }}
                    className="hover:underline font-medium"
                >
                    {row.title}
                </Link>
            ),
        },
        {
            accessorKey: "view_count",
            header: "Views",
            field: "view_count",
            sortBy: true,
            renderCell: (row: Discussions) => (
                <span className="flex items-center gap-1">
                    <EyeIcon size={14} />
                    {row.view_count}
                </span>
            ),
        },
        {
            accessorKey: "like_count",
            header: "Likes",
            field: "like_count",
            sortBy: true,
            renderCell: (row: Discussions) => (
                <span className="flex items-center gap-1">
                    <ThumbsupIcon size={14} />
                    {row.like_count}
                </span>
            ),
        },
        {
            accessorKey: "comment_count",
            header: "Comments",
            field: "comment_count",
            sortBy: true,
            renderCell: (row: Discussions) => (
                <span className="flex items-center gap-1">
                    <CommentIcon size={14} />
                    {row.comment_count}
                </span>
            ),
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            sortBy: true,
            renderCell: (row: Discussions) => (
                <span>{DatetimeToShow(row.created_at)}</span>
            ),
        },
    ];

    const filterKeys = ["id", "title"];

    return (
        <GenericTable
            className="mt-2"
            subject={subject}
            columns={columns}
            queryFn={serviceApi.discussions.fetch}
            filterKeys={filterKeys}
            disableSelect={true}
            disableAdd={true}
        />
    );
}
