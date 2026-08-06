import { ThumbsupIcon, CommentIcon, EyeIcon } from "@primer/octicons-react";
import { Avatar, TextInput, Textarea } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import type { Discussions } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/discussions/my")({
    component: RouteComponent,
});

function RouteComponent() {
    useTitle("My Discussions | FloatCTF");
    const queryClient = useQueryClient();
    const banner = useMsgBanner();

    const { data: me } = useQuery({
        queryKey: ["profile"],
        queryFn: () => serviceApi.users.getMe(),
        select: (res) => res.data,
    });

    const subject = "MyDiscussions";

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

    const mutationData = useReactive<{ title: string; content: string }>({
        title: "",
        content: "",
    });

    const mutationColumns = [
        {
            header: "title",
            field: "title",
            render: (
                <TextInput
                    value={mutationData.title}
                    onChange={(e) => {
                        mutationData.title = e.target.value;
                    }}
                    placeholder="Discussion title"
                />
            ),
        },
    ];

    return (
        <GenericTable
            className="mt-2"
            subject={subject}
            columns={columns}
            queryFn={(params) =>
                serviceApi.discussions.fetch({
                    ...params,
                    filter: params?.filter
                        ? `${params.filter},author_id:${me?.id || ""}`
                        : `author_id:${me?.id || ""}`,
                })
            }
            filterKeys={filterKeys}
            mutationColumns={mutationColumns}
            mutationData={mutationData}
            createFn={serviceApi.discussions.create as any}
            patchFn={serviceApi.discussions.patch}
            removeFn={async (ids) => {
                for (const id of ids) {
                    await serviceApi.discussions.remove(id);
                }
                queryClient.invalidateQueries({ queryKey: [subject] });
                banner.showBanner(
                    "success",
                    `Deleted ${ids.length} discussion(s)`,
                );
                return { code: 0, message: "OK", data: ids.length } as any;
            }}
        />
    );
}
