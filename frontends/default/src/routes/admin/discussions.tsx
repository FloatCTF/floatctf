import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { Discussions } from "@floatctf/sdk/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/discussions")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

// admin 列表展示作者昵称（后端批量填充，缺失回落为 UUID）
type DiscussionRow = Discussions & {
    author_nickname?: string;
};

function RouteComponent() {
    useTitle("Discussions | FloatCTF");

    const subject = "Discussions";

    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "id",
            rowHeader: true,
        },
        {
            accessorKey: "title",
            header: "标题",
            field: "title",
            sortBy: true,
        },
        {
            accessorKey: "author_nickname",
            header: "作者",
            field: "author_id",
            renderCell: (row: DiscussionRow) => {
                return <span>{row.author_nickname ?? row.author_id}</span>;
            },
        },
        {
            accessorKey: "view_count",
            header: "浏览",
            field: "view_count",
            sortBy: true,
        },
        {
            accessorKey: "like_count",
            header: "点赞",
            field: "like_count",
            sortBy: true,
        },
        {
            accessorKey: "comment_count",
            header: "评论",
            field: "comment_count",
            sortBy: true,
        },
        {
            accessorKey: "created_at",
            header: "创建时间",
            field: "created_at",
            sortBy: true,
            renderCell: (row: Discussions) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
        {
            accessorKey: "updated_at",
            header: "更新时间",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Discussions) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];

    const filterKeys = ["id", "title", "author_id"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={adminApi.discussions.fetch}
            removeFn={adminApi.discussions.remove}
            filterKeys={filterKeys}
            disableAdd={true}
            disableSelect={false}
        />
    );
}
