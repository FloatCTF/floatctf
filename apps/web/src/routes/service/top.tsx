import { Avatar } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/top")({
    component: RouteComponent,
});

export type TopUser = {
    no: number;
    nickname: string;
    avatar?: string;
    solved_count: number;
    solved_last_at: string;
};

function RouteComponent() {
    useTitle("Top | FloatCTF");
    const subject = "Top 15 Users";

    const columns = [
        {
            accessorKey: "no",
            header: "序号",
            field: "no",
            rowHeader: true,
        },
        {
            accessorKey: "nickname",
            header: "昵称",
            field: "nickname",
            rowHeader: true,
            renderCell: (row: TopUser) => (
                <div className="flex items-center gap-2">
                    {row.avatar ? (
                        <Avatar src={row.avatar} size={24} />
                    ) : (
                        <div
                            className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
                            style={{ width: 24, height: 24, fontSize: 10 }}
                        >
                            {row.nickname?.[0]?.toUpperCase() || "?"}
                        </div>
                    )}
                    <span>{row.nickname}</span>
                </div>
            ),
        },
        {
            accessorKey: "solved_count",
            header: "解出数",
            field: "solved_count",
        },
        {
            accessorKey: "solved_last_at",
            header: "最近解出时间",
            field: "solved_last_at",
            renderCell: (row: TopUser) => {
                return <span>{DatetimeToShow(row.solved_last_at)}</span>;
            },
        },
    ];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={() => serviceApi.solves.getTop15Users()}
            getRowId={(row: TopUser) => row.no.toString()}
            disableAdd={true}
            disablePagination={true}
            disableSelect={true}
            enableInternalActions={false}
        />
    );
}
