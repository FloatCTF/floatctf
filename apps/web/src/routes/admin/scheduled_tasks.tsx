import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { ScheduledTasks } from "@/entity";
import { DatetimeToShow } from "@/util";
import dayjs from "dayjs";
import { CheckIcon, XIcon, PlayIcon } from "@primer/octicons-react";
import {
    ActionList,
    ConfirmationDialog,
    Label,
    Select,
    Stack,
    TextInput,
    ToggleSwitch,
} from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/scheduled_tasks")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

const statusToVariant = (status: string) => {
    switch (status) {
        case "pending":
            return "accent";
        case "running":
            return "attention";
        case "completed":
            return "success";
        case "failed":
            return "danger";
        default:
            return "default";
    }
};

function RouteComponent() {
    const subject = "ScheduledTasks";

    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "task_name",
            header: "Task Name",
            field: "task_name",
        },
        {
            accessorKey: "task_key",
            header: "Task Key",
            field: "task_key",
        },
        {
            accessorKey: "trigger_type",
            header: "Trigger Type",
            field: "trigger_type",
        },
        {
            accessorKey: "status",
            header: "Status",
            field: "status",
            renderCell: (row: ScheduledTasks) => (
                <Label variant={statusToVariant(row.status)}>
                    {row.status}
                </Label>
            ),
        },
        {
            accessorKey: "cron_expr",
            header: "Cron Expr",
            field: "cron_expr",
        },
        {
            accessorKey: "execute_at",
            header: "Execute At",
            field: "execute_at",
            renderCell: (row: ScheduledTasks) => (
                <span>{DatetimeToShow(row.execute_at)}</span>
            ),
        },
        {
            accessorKey: "enabled",
            header: "Enabled",
            field: "enabled",
            renderCell: (row: ScheduledTasks) =>
                row.enabled ? <CheckIcon /> : <XIcon />,
        },
        {
            accessorKey: "protected",
            header: "Protected",
            field: "protected",
            renderCell: (row: ScheduledTasks) =>
                row.protected ? <CheckIcon /> : <XIcon />,
        },
        {
            accessorKey: "last_run_at",
            header: "Last Run At",
            field: "last_run_at",
            renderCell: (row: ScheduledTasks) => (
                <span>{DatetimeToShow(row.last_run_at)}</span>
            ),
        },
        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            renderCell: (row: ScheduledTasks) => (
                <span>{DatetimeToShow(row.updated_at)}</span>
            ),
        },
    ];

    const mutationTask = useReactive<Partial<ScheduledTasks>>({
        task_name: "",
        task_key: "",
        trigger_type: "once",
        cron_expr: undefined,
        execute_at: undefined,
        expires_at: undefined,
        description: undefined,
        enabled: true,
        protected: false,
    });

    const mutationColumns = [
        {
            header: "task_name",
            field: "task_name",
            render: (
                <TextInput
                    value={mutationTask.task_name ?? ""}
                    onChange={(e) => {
                        mutationTask.task_name = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "task_key",
            field: "task_key",
            render: (
                <TextInput
                    value={mutationTask.task_key ?? ""}
                    onChange={(e) => {
                        mutationTask.task_key = e.target.value;
                    }}
                />
            ),
        },
        {
            header: "trigger_type",
            field: "trigger_type",
            render: (
                <Select
                    value={mutationTask.trigger_type ?? "once"}
                    onChange={(e) => {
                        mutationTask.trigger_type = e.target.value;
                    }}
                >
                    <Select.Option value="once">once</Select.Option>
                    <Select.Option value="cron">cron</Select.Option>
                    <Select.Option value="startup">startup</Select.Option>
                </Select>
            ),
        },
        {
            header: "cron_expr",
            field: "cron_expr",
            render: (
                <TextInput
                    value={mutationTask.cron_expr ?? ""}
                    onChange={(e) => {
                        mutationTask.cron_expr = e.target.value || undefined;
                    }}
                />
            ),
        },
        {
            header: "execute_at",
            field: "execute_at",
            render: (
                <input
                    type="datetime-local"
                    step="1"
                    value={
                        mutationTask.execute_at
                            ? dayjs
                                  .utc(mutationTask.execute_at)
                                  .local()
                                  .format("YYYY-MM-DDTHH:mm:ss")
                            : ""
                    }
                    onChange={(e) => {
                        const localTime = dayjs(e.target.value);
                        const utcTime = localTime
                            .utc()
                            .format("YYYY-MM-DDTHH:mm:ss[Z]");
                        mutationTask.execute_at = utcTime || undefined;
                    }}
                />
            ),
        },
        {
            header: "expires_at",
            field: "expires_at",
            render: (
                <input
                    type="datetime-local"
                    step="1"
                    value={
                        mutationTask.expires_at
                            ? dayjs
                                  .utc(mutationTask.expires_at)
                                  .local()
                                  .format("YYYY-MM-DDTHH:mm:ss")
                            : ""
                    }
                    onChange={(e) => {
                        const localTime = dayjs(e.target.value);
                        const utcTime = localTime
                            .utc()
                            .format("YYYY-MM-DDTHH:mm:ss[Z]");
                        mutationTask.expires_at = utcTime || undefined;
                    }}
                />
            ),
        },
        {
            header: "description",
            field: "description",
            render: (
                <TextInput
                    value={mutationTask.description ?? ""}
                    onChange={(e) => {
                        mutationTask.description = e.target.value || undefined;
                    }}
                />
            ),
        },
        {
            header: "enabled",
            field: "enabled",
            render: (
                <Stack direction="horizontal" align="center">
                    <ToggleSwitch
                        aria-labelledby="toggle-enabled"
                        checked={mutationTask.enabled ?? true}
                        onClick={() => {
                            mutationTask.enabled = !mutationTask.enabled;
                        }}
                    />
                </Stack>
            ),
        },
        {
            header: "protected",
            field: "protected",
            render: (
                <Stack direction="horizontal" align="center">
                    <ToggleSwitch
                        aria-labelledby="toggle-protected"
                        checked={mutationTask.protected ?? false}
                        onClick={() => {
                            mutationTask.protected = !mutationTask.protected;
                        }}
                    />
                </Stack>
            ),
        },
    ];

    const filterKeys = [
        "id",
        "task_name",
        "task_key",
        "trigger_type",
        "status",
        "enabled",
        "protected",
    ];

    const queryClient = useQueryClient();
    const [runConfirmOpen, setRunConfirmOpen] = useState(false);
    const [runTargetTask, setRunTargetTask] = useState<ScheduledTasks | null>(
        null,
    );

    const runMutation = useMutation({
        mutationFn: (task_id: string) => adminApi.scheduled_tasks.run(task_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["ScheduledTasks"] });
            setRunConfirmOpen(false);
            setRunTargetTask(null);
        },
    });

    const columnActions = (row: ScheduledTasks) => (
        <ActionList>
            <ActionList.Item
                onClick={() => {
                    setRunTargetTask(row);
                    setRunConfirmOpen(true);
                }}
            >
                <ActionList.LeadingVisual>
                    <PlayIcon />
                </ActionList.LeadingVisual>
                Run once
            </ActionList.Item>
        </ActionList>
    );

    return (
        <>
            <GenericTable
                subject={subject}
                columns={columns}
                queryFn={adminApi.scheduled_tasks.fetch}
                createFn={adminApi.scheduled_tasks.create}
                removeFn={adminApi.scheduled_tasks.remove}
                patchFn={adminApi.scheduled_tasks.patch}
                mutationColumns={mutationColumns}
                mutationData={mutationTask}
                filterKeys={filterKeys}
                columnActions={columnActions}
            />

            {runConfirmOpen && runTargetTask && (
                <ConfirmationDialog
                    onClose={(gesture) => {
                        if (gesture === "confirm") {
                            runMutation.mutate(runTargetTask.id);
                        } else {
                            setRunConfirmOpen(false);
                            setRunTargetTask(null);
                        }
                    }}
                    title="Run Task"
                    confirmButtonContent="Run"
                    cancelButtonContent="Cancel"
                >
                    Are you sure you want to run task "{runTargetTask.task_name}
                    " now?
                </ConfirmationDialog>
            )}
        </>
    );
}
