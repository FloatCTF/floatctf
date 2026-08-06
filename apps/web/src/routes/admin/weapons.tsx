import { adminApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import type { Weapons } from "@/entity/weapons";
import { DatetimeToShow } from "@/util";
import { CheckIcon } from "@primer/octicons-react";
import { ActionList, Stack, TextInput, ToggleSwitch } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useRef, useState } from "react";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/weapons")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const subject = "Weapons";
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        { accessorKey: "name", header: "Name", field: "name", sortBy: true },
        {
            accessorKey: "category",
            header: "Category",
            field: "category",
            sortBy: true,
        },
        {
            accessorKey: "has_file",
            header: "Has File",
            field: "has_file",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return <span>{row.has_file ? <CheckIcon /> : <></>}</span>;
            },
        },
        {
            accessorKey: "file_url",
            header: "File URL",
            field: "file_url",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return (
                    <a
                        href={`/public/${row.file_url}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        download
                    >
                        {row.file_url}
                    </a>
                );
            },
        },
        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];
    const mutationWeapon = useReactive<Partial<Weapons>>({
        name: "",
        category: "",
        description: "",
        has_file: false,
        file_url: "",
        download_count: 0,
    });

    const mutationColumns = [
        {
            header: "Name",
            field: "name",
            render: (
                <TextInput
                    value={mutationWeapon.name}
                    onChange={(e) => {
                        mutationWeapon.name = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "Category",
            field: "category",
            render: (
                <TextInput
                    value={mutationWeapon.category}
                    onChange={(e) => {
                        mutationWeapon.category = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "Description",
            field: "description",
            render: (
                <TextInput
                    value={mutationWeapon.description}
                    onChange={(e) => {
                        mutationWeapon.description = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "Has File",
            field: "has_file",
            render: (
                <Stack direction="horizontal" align="center">
                    <ToggleSwitch
                        aria-labelledby="default-toggle-label"
                        checked={mutationWeapon.has_file}
                        onClick={() => {
                            mutationWeapon.has_file = !mutationWeapon.has_file;
                        }}
                    />
                </Stack>
            ),
        },
        {
            header: "File URL",
            field: "file_url",
            render: (
                <TextInput
                    value={mutationWeapon.file_url}
                    onChange={(e) => {
                        mutationWeapon.file_url = e.target.value;
                    }}
                />
            ),
        },
    ];

    const banner = useMsgBanner({});
    const HandleUpload = ({ row }: { row: Weapons }) => {
        const inputRef = useRef<HTMLInputElement>(null);
        const queryClient = useQueryClient();

        const uploadMutation = useMutation({
            mutationFn: (weapon: File) =>
                adminApi.weapons.upload(row.id, weapon),
            onSuccess: () => {
                queryClient.invalidateQueries({ queryKey: [subject] });
                banner.showBanner("success", "Uploaded successfully");
            },
            onError: (error) => {
                banner.showErrorBanner(error);
            },
        });

        const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
            const file = e.target.files?.[0];
            if (!file) return;
            e.target.value = ""; // 允许连续选择同一个文件
            uploadMutation.mutate(file);
        };

        const handleClick = () => {
            inputRef.current?.click();
        };

        return (
            <>
                <input
                    type="file"
                    ref={inputRef}
                    style={{ display: "none" }}
                    onChange={handleFileSelect}
                />

                <ActionList.Item onClick={handleClick}>Upload</ActionList.Item>
            </>
        );
    };

    const columns_actions = (row: Weapons) => {
        return (
            <ActionList>
                <HandleUpload row={row} />
            </ActionList>
        );
    };
    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={adminApi.weapons.fetch}
            createFn={adminApi.weapons.create}
            removeFn={adminApi.weapons.remove}
            patchFn={adminApi.weapons.patch}
            mutationColumns={mutationColumns}
            mutationData={mutationWeapon}
            columnActions={columns_actions}
            externalBanner={banner}
            disablePagination={true}
        />
    );
}
