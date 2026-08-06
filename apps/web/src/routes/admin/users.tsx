import { TextInput } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { Users } from "@/entity";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/users")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{
			accessorKey: "username",
			header: "Username",
			field: "username",
			sortBy: true,
		},
		{
			accessorKey: "nickname",
			header: "Nickname",
			field: "nickname",
			sortBy: true,
		},
		{ accessorKey: "email", header: "Email", field: "email", sortBy: true },
	];

	const mutationUser = useReactive<Partial<Users>>({
		username: "",
		email: "",
		password: "",
		nickname: "",
	});

	const mutationColumns = [
		{
			header: "Username",
			field: "username",
			render: (
				<TextInput
					value={mutationUser.username}
					onChange={(e) => {
						mutationUser.username = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Email",
			field: "email",
			render: (
				<TextInput
					value={mutationUser.email}
					onChange={(e) => {
						mutationUser.email = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Nickname",
			field: "nickname",
			render: (
				<TextInput
					value={mutationUser.nickname}
					onChange={(e) => {
						mutationUser.nickname = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Password",
			field: "password",
			render: (
				<TextInput
					value={mutationUser.password}
					onChange={(e) => {
						mutationUser.password = e.target.value;
					}}
				/>
			),
		},
	];

	const filterKeys = ["id", "username", "nickname", "email"];

	return (
		<GenericTable
			subject="Users"
			columns={columns}
			queryFn={adminApi.users.fetch}
			createFn={adminApi.users.create}
			removeFn={adminApi.users.remove}
			patchFn={adminApi.users.patch}
			filterKeys={filterKeys}
			mutationColumns={mutationColumns}
			mutationData={mutationUser}
		/>
	);
}
