import { PlusIcon, TrashIcon } from "@primer/octicons-react";
import { ActionList, ActionMenu } from "@primer/react";
import {
    Button,
    ButtonGroup,
    Dialog,
    FormControl,
    Stack,
    TextInput,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi, type NetworkInfo } from "@/api";
import { GenericTable } from "@/components";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/docker/networks")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const queryClient = useQueryClient();
    const [isOpen, setIsOpen] = useState(false);
    const [newNetwork, setNewNetwork] = useState({
        name: "",
        subnet: "",
        gateway: "",
        driver: "bridge",
    });

    const columns = [
        {
            accessorKey: "id",
            header: "Network ID",
            field: "id",
            rowHeader: true,
        },
        { accessorKey: "name", header: "Name", field: "name" },
        { accessorKey: "driver", header: "Driver", field: "driver" },
        { accessorKey: "scope", header: "Scope", field: "scope" },
        { accessorKey: "subnet", header: "Subnet", field: "subnet" },
        { accessorKey: "gateway", header: "Gateway", field: "gateway" },
    ];

    const filterKeys = ["name"];

    const createMutation = useMutation({
        mutationFn: (network: {
            name: string;
            subnet: string;
            gateway: string;
            driver?: string;
        }) => adminApi.docker.createNetwork(network),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Networks"] });
            setIsOpen(false);
            setNewNetwork({
                name: "",
                subnet: "",
                gateway: "",
                driver: "bridge",
            });
        },
    });

    const deleteMutation = useMutation({
        mutationFn: (network_id: string) =>
            adminApi.docker.deleteNetwork(network_id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["Networks"] });
        },
    });

    const columnActions = (row: NetworkInfo) => (
        <ActionList>
            <ActionList.Item
                variant="danger"
                onClick={() => {
                    if (confirm(`Delete network ${row.name}?`)) {
                        deleteMutation.mutate(row.id);
                    }
                }}
            >
                Delete
            </ActionList.Item>
        </ActionList>
    );

    const customActions = (
        <ButtonGroup>
            <Button variant="primary" onClick={() => setIsOpen(true)}>
                <PlusIcon />
                Create Network
            </Button>
        </ButtonGroup>
    );

    return (
        <>
            <GenericTable
                className="m-2"
                subject="Networks"
                columns={columns}
                filterKeys={filterKeys}
                queryFn={adminApi.docker.fetchNetworks}
                enableInternalActions={true}
                disableAdd={true}
                disableSelect={true}
                columnActions={columnActions}
                customActions={customActions}
                getRowId={(row) => row.id}
            />

            {isOpen && (
                <Dialog
                    title="Create Network"
                    onClose={() => setIsOpen(false)}
                    position="right"
                >
                    <Stack direction="vertical" gap="spacious">
                        <FormControl>
                            <FormControl.Label>Name</FormControl.Label>
                            <TextInput
                                value={newNetwork.name}
                                onChange={(e) =>
                                    setNewNetwork({
                                        ...newNetwork,
                                        name: e.target.value,
                                    })
                                }
                            />
                        </FormControl>
                        <FormControl>
                            <FormControl.Label>Subnet</FormControl.Label>
                            <TextInput
                                value={newNetwork.subnet}
                                onChange={(e) =>
                                    setNewNetwork({
                                        ...newNetwork,
                                        subnet: e.target.value,
                                    })
                                }
                                placeholder="e.g. 172.20.0.0/16"
                            />
                        </FormControl>
                        <FormControl>
                            <FormControl.Label>Gateway</FormControl.Label>
                            <TextInput
                                value={newNetwork.gateway}
                                onChange={(e) =>
                                    setNewNetwork({
                                        ...newNetwork,
                                        gateway: e.target.value,
                                    })
                                }
                                placeholder="e.g. 172.20.0.1"
                            />
                        </FormControl>
                        <FormControl>
                            <FormControl.Label>Driver</FormControl.Label>
                            <TextInput
                                value={newNetwork.driver}
                                onChange={(e) =>
                                    setNewNetwork({
                                        ...newNetwork,
                                        driver: e.target.value,
                                    })
                                }
                            />
                        </FormControl>
                        <Button
                            variant="primary"
                            onClick={() => createMutation.mutate(newNetwork)}
                            disabled={
                                !newNetwork.name ||
                                !newNetwork.subnet ||
                                !newNetwork.gateway
                            }
                        >
                            Create
                        </Button>
                    </Stack>
                </Dialog>
            )}
        </>
    );
}
