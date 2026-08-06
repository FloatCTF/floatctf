import { Avatar } from "@primer/react";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { ChallengeWriteupResult } from "@/routes/service/challenges/$id/writeup";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/writeups/")({
    component: RouteComponent,
});

function RouteComponent() {
    useTitle("Writeups | FloatCTF");
    const subject = "Writeups";
    const filterKeys = ["id", "challenge_id"];

    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "writeup.id",
            rowHeader: true,
            sortBy: true,
            renderCell: (row: ChallengeWriteupResult) => (
                <Link
                    to="/service/writeups/$id"
                    params={{ id: row.writeup.id }}
                >
                    {row.writeup.id}
                </Link>
            ),
        },
        {
            accessorKey: "challenge",
            header: "Challenge",
            field: "challenge.name",
            sortBy: true,
            renderCell: (row: ChallengeWriteupResult) => (
                <Link
                    to="/service/challenges/$id"
                    params={{ id: row.challenge.id }}
                >
                    {row.challenge.name}
                </Link>
            ),
        },
        {
            accessorKey: "nickname",
            header: "Author",
            field: "nickname",
            sortBy: true,
            renderCell: (row: ChallengeWriteupResult) => (
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
            accessorKey: "email",
            header: "Email",
            field: "email",
            renderCell: (row: ChallengeWriteupResult) => (
                <a href={`mailto:${row.email}`}>{row.email}</a>
            ),
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "writeup.created_at",
            sortBy: true,
            renderCell: (row: ChallengeWriteupResult) => (
                <span>{DatetimeToShow(row.writeup.created_at)}</span>
            ),
        },
    ];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.challenges.getAllWriteups}
            enableInternalActions={false}
            disableAdd={true}
            disableSelect={true}
            getRowId={(row: ChallengeWriteupResult) => row.writeup.id}
        />
    );
}
