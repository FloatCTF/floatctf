import { TriangleDownIcon } from "@primer/octicons-react";
import { Button, Label, SelectPanel, Spinner, TextInput } from "@primer/react";
import { Banner, DataTable, Dialog, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useReactive } from "ahooks";
import { useCallback, useEffect, useMemo, useState } from "react";
import { RemainingTimer } from "../../challenges/$id";

import type { UniResponse } from "@/api/axios";
import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import type { Challenges, Instances } from "@/entity";
import type { AxiosError } from "axios";

export const Route = createFileRoute("/service/challenge_sets/$id/")({
    component: RouteComponent,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const subject = `Challenge Set #${id}`;
    const challengeDialog = useReactive({
        open: false,
        title: "",
        challenge_id: "",
    });

    const { data, isLoading, isError, error } = useQuery<
        UniResponse<Challenges[]>,
        AxiosError<{ message: string }>
    >({
        queryKey: ["challengeSet", id],
        queryFn: () => serviceApi.challenges.getChallengeSet(id),
    });

    const categories = [
        { text: "ALL" },
        { text: "Web" },
        { text: "Misc" },
        { text: "Pwn" },
        { text: "Crypto" },
        { text: "Reverse" },
        { text: "AI" },
    ];
    const [selected, setSelected] = useState(categories[0]);
    const [filter, setFilter] = useState("");
    const [open, setOpen] = useState(false);

    const filteredData = useMemo(() => {
        if (!data?.data) return [];
        if (selected.text === "ALL") return data.data;
        return data.data.filter(
            (row: Challenges) =>
                row.category.toLowerCase() === selected.text.toLowerCase(),
        );
    }, [data?.data, selected]);

    const filteredItems = categories.filter(
        (item) =>
            item.text === selected?.text ||
            item.text.toLowerCase().includes(filter.toLowerCase()),
    );

    const handleClose = useCallback(() => {
        challengeDialog.open = false;
    }, [challengeDialog]);

    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "id",
        },
        {
            accessorKey: "name",
            header: "Name",
            field: "name",
            renderCell: (row: Challenges) => {
                return (
                    <button
                        type="button"
                        onClick={() => {
                            challengeDialog.open = true;
                            challengeDialog.title = row.name;
                            challengeDialog.challenge_id = row.id;
                        }}
                        className="bg-transparent border-none p-0 m-0 text-blue-600 hover:underline cursor-pointer"
                    >
                        {row.name}
                    </button>
                );
            },
        },
        {
            accessorKey: "category",
            field: "category",
            header: () => {
                return (
                    <SelectPanel
                        renderAnchor={({ children, ...anchorProps }) => (
                            <Button
                                {...anchorProps}
                                trailingAction={TriangleDownIcon}
                                aria-haspopup="dialog"
                            >
                                {selected?.text ?? "Category"}
                            </Button>
                        )}
                        placeholder="Pick category"
                        open={open}
                        onOpenChange={setOpen}
                        items={filteredItems}
                        selected={selected}
                        // @ts-ignore
                        onSelectedChange={setSelected}
                        onFilterChange={setFilter}
                    />
                );
            },
        },
    ];

    const table = useReactTable({
        data: filteredData,
        columns: columns,
        getCoreRowModel: getCoreRowModel(),
    });

    if (isLoading) {
        return <Spinner size="large" />;
    }
    if (isError) {
        const msg =
            error.response?.data?.message || error.message || "Unknown error";
        return <div>{msg}</div>;
    }

    return (
        <Table.Container className="m-2">
            <Table.Title id="repositories-headerAction">{subject}</Table.Title>
            <Table.Divider />
            <DataTable
                aria-labelledby="challenge-set"
                // @ts-ignore
                columns={columns}
                data={table.getRowModel().rows.map((row) => row.original)}
            />
            <ChallengeDialog
                open={challengeDialog.open}
                title={challengeDialog.title}
                onClose={handleClose}
                id={challengeDialog.challenge_id}
            />
        </Table.Container>
    );
}

type ChallengeDialogProps = {
    open: boolean;
    title: string;
    onClose: () => void;
    id: string;
};

function ChallengeDialog({ open, title, onClose, id }: ChallengeDialogProps) {
    const { data: challenge } = useQuery({
        queryKey: ["challenge", id],
        queryFn: () => serviceApi.challenges.get(id),
        select: (data) => data.data,
    });

    const challengeStatus = useReactive({
        isRunning: false,
        instance: {} as Instances,
        flag: "",
    });

    const { data: instance_data, refetch: refetch_instance } = useQuery({
        queryKey: ["instance", id],
        queryFn: () => serviceApi.challenges.getInstance(id),
    });

    // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
    useEffect(() => {
        if (open) {
            if (instance_data?.data) {
                challengeStatus.isRunning = true;
                challengeStatus.instance = instance_data.data;
            } else {
                challengeStatus.isRunning = false;
                challengeStatus.instance = {} as Instances;
            }
        }
    }, [instance_data, open]);
    const banner = useMsgBanner();

    const mutationInstance = useMutation({
        mutationFn: serviceApi.instances.launch,
        onSuccess: (data) => {
            challengeStatus.isRunning = true;
            challengeStatus.instance = data.data!;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    const destroyInstance = useMutation({
        mutationFn: serviceApi.instances.destroy,
        onSuccess: (_data) => {
            challengeStatus.isRunning = false;
            challengeStatus.instance = {} as Instances;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });
    const submitFlag = useMutation({
        mutationFn: serviceApi.submit.submit,
        onSuccess: (_data) => {
            banner.showBanner("success", "Flag is correct!");
            challengeStatus.isRunning = false;
            challengeStatus.instance = {} as Instances;
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    if (!open) return null;

    return (
        <Dialog
            title={challenge?.name ?? title}
            onClose={onClose}
            className="max-w-lg"
        >
            <div className="flex flex-col gap-2 text-sm text-gray-800">
                <p>{challenge?.description}</p>

                {challenge?.attachment && (
                    <a
                        href={`/challenges/${challenge.safe_name}/${challenge.attachment}`}
                        download
                        target="_blank"
                        rel="noopener noreferrer"
                        className="inline-block mt-1"
                    >
                        <Label variant="accent">{challenge.attachment}</Label>
                    </a>
                )}

                <div className="mt-2">
                    <banner.BannerComponent />
                    {challengeStatus.instance && (
                        <div
                            // biome-ignore lint/security/noDangerouslySetInnerHtml: <explanation>
                            dangerouslySetInnerHTML={{
                                __html: challengeStatus.instance.content || "",
                            }}
                        />
                    )}
                    {challengeStatus.isRunning ? (
                        <div className="w-full flex flex-col gap-2 mb-4">
                            <RemainingTimer
                                destroy_at={challengeStatus.instance.destroy_at}
                                onExpire={() => {
                                    refetch_instance({ cancelRefetch: true });
                                    destroyInstance.mutate(
                                        challengeStatus.instance.id,
                                    );
                                }}
                            />

                            <div className="flex gap-2">
                                <TextInput
                                    className="flex-1"
                                    value={challengeStatus.flag}
                                    onChange={(e) => {
                                        challengeStatus.flag = e.target.value;
                                    }}
                                    placeholder="flag{}"
                                />
                                <Button
                                    variant="danger"
                                    onClick={() => {
                                        destroyInstance.mutate(
                                            challengeStatus.instance.id,
                                        );
                                    }}
                                >
                                    Destroy
                                </Button>

                                <Button
                                    variant="primary"
                                    onClick={() => {
                                        submitFlag.mutate({
                                            instance_id:
                                                challengeStatus.instance.id,
                                            flag: challengeStatus.flag,
                                        });
                                    }}
                                >
                                    Submit
                                </Button>
                            </div>
                        </div>
                    ) : (
                        <Button
                            variant="primary"
                            onClick={() => mutationInstance.mutate(id)}
                        >
                            Launch
                        </Button>
                    )}
                </div>
            </div>
        </Dialog>
    );
}
