import {
    ActionList,
    Button,
    Dialog,
    FormControl,
    Stack,
    TextInput,
} from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useState } from "react";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { SuperAdmin } from "@floatctf/sdk/entity/super_admin";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/super_admins")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function ChangePasswordDialog({
    id,
    isOpen,
    onClose,
}: {
    id: string;
    isOpen: boolean;
    onClose: () => void;
}) {
    const queryClient = useQueryClient();
    const [newPassword, setNewPassword] = useState("");

    const patchMutation = useMutation({
        mutationFn: (data: { password: string }) =>
            adminApi.super_admin.patch(id, data),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Super Admins"] });
            onClose();
            setNewPassword("");
        },
    });

    if (!isOpen) return null;
    return (
        <Dialog title="Change Password" onClose={onClose}>
            <Stack direction="vertical" gap="spacious">
                <FormControl>
                    <FormControl.Label>New Password</FormControl.Label>
                    <TextInput
                        type="password"
                        value={newPassword}
                        onChange={(e) => setNewPassword(e.target.value)}
                    />
                </FormControl>
                <Button
                    variant="primary"
                    onClick={() => {
                        if (newPassword) {
                            patchMutation.mutate({ password: newPassword });
                        }
                    }}
                >
                    Confirm
                </Button>
            </Stack>
        </Dialog>
    );
}

function RouteComponent() {
    const [dialogState, setDialogState] = useState<{
        isOpen: boolean;
        id: string;
    }>({ isOpen: false, id: "" });

    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "username",
            header: "用户名",
            field: "username",
            label: "用户名",
            sortBy: true,
        },
        {
            accessorKey: "email",
            header: "邮箱",
            field: "email",
            label: "邮箱",
            sortBy: true,
        },
        {
            accessorKey: "password",
            header: "密码",
            field: "password",
            label: "密码",
            renderCell: () => "****",
        },
    ];

    const columnActions = (row: SuperAdmin) => (
        <ActionList>
            <ActionList.Item
                onClick={() => setDialogState({ isOpen: true, id: row.id })}
            >
                Change Password
            </ActionList.Item>
        </ActionList>
    );

    const mutationData = useReactive<Partial<SuperAdmin>>({
        username: "",
        password: "",
        email: "",
    });

    const mutationColumns = [
        {
            header: "username",
            field: "username",
            label: "用户名",
            render: (
                <TextInput
                    value={mutationData.username}
                    onChange={(e) => {
                        mutationData.username = e.target.value;
                    }}
                    placeholder="Username"
                />
            ),
        },
        {
            header: "密码",
            field: "password",
            label: "密码",
            render: (
                <TextInput
                    type="password"
                    value={mutationData.password}
                    onChange={(e) => {
                        mutationData.password = e.target.value;
                    }}
                    placeholder="Password"
                />
            ),
        },
        {
            header: "邮箱",
            field: "email",
            label: "邮箱",
            render: (
                <TextInput
                    value={mutationData.email}
                    onChange={(e) => {
                        mutationData.email = e.target.value;
                    }}
                    placeholder="Email"
                />
            ),
        },
    ];

    const filterKeys = ["id", "username", "email"];

    return (
        <>
            <GenericTable
                subject="Super Admins"
                columns={columns}
                queryFn={adminApi.super_admin.fetch}
                createFn={adminApi.super_admin.create}
                removeFn={adminApi.super_admin.remove}
                filterKeys={filterKeys}
                mutationColumns={mutationColumns}
                mutationData={mutationData}
                columnActions={columnActions}
                disableSelect={false}
            />

            <ChangePasswordDialog
                id={dialogState.id}
                isOpen={dialogState.isOpen}
                onClose={() => setDialogState({ isOpen: false, id: "" })}
            />
        </>
    );
}
