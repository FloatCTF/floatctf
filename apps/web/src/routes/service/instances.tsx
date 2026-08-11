import { Button } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import type { ChallengeInstances as Instances } from "@/entity";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

/** 列表页展示类型：实例基础字段 + 关联名称（后端计算字段）。 */
type InstanceRow = Instances & {
	challenge_title?: string;
	event_title?: string;
	user_name?: string;
};

export const Route = createFileRoute("/service/instances")({
	component: RouteComponent,
});

function RouteComponent() {
	useTitle("Instances | FloatCTF");

	const subject = "Instances";
	const banner = useMsgBanner();
	const queryClient = useQueryClient();

	const filterKeys = ["id", "status", "identifier", "challenge_id", "event_id"];

	const mutationInstance = useMutation({
		mutationFn: serviceApi.instances.destroy,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [subject] });
			banner.showBanner("success", "Instance destroyed successfully");
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	const columns = [
		{
			accessorKey: "challenge_title",
			header: "Challenge",
			field: "challenge_title",
			rowHeader: true,
			renderCell: (row: InstanceRow) => {
				return row.challenge_id ? (
					<AppLink
						to={"/service/challenges/$id"}
						params={{ id: row.challenge_id }}
					>
						{row.challenge_title ?? row.challenge_id}
					</AppLink>
				) : (
					<span>—</span>
				);
			},
		},
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
		},
		{
			accessorKey: "event_title",
			header: "Event",
			field: "event_title",
			renderCell: (row: InstanceRow) => {
				return <span>{row.event_title ?? row.event_id}</span>;
			},
		},
		{
			accessorKey: "identifier",
			header: "Identifier",
			field: "identifier",
		},
		{
			accessorKey: "user_name",
			header: "User",
			field: "user_name",
			renderCell: (row: InstanceRow) => {
				return <span>{row.user_name ?? row.user_id}</span>;
			},
		},
		{
			accessorKey: "destroy_at",
			header: "Destroy At",
			field: "destroy_at",
			renderCell: (row: Instances) => {
				return <span>{DatetimeToShow(row.destroy_at)}</span>;
			},
		},
		{
			accessorKey: "action",
			header: "Action",
			field: "action",
			renderCell: (row: Instances) => {
				return (
					<Button
						variant="invisible"
						onClick={() => {
							mutationInstance.mutate(row.id);
						}}
						style={{ color: "#DB0000" }}
					>
						Destroy
					</Button>
				);
			},
		},
	];

	return (
		<GenericTable
			subject={subject}
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.instances.fetch}
			enableInternalActions={false}
			externalBanner={banner}
			disableAdd={true}
			disableSelect={true}
		/>
	);
}
