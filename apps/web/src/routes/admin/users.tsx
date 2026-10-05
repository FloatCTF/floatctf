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
			header: "用户名",
			field: "username",
			sortBy: true,
		},
		{
			accessorKey: "nickname",
			header: "昵称",
			field: "nickname",
			sortBy: true,
		},
		{ accessorKey: "email", header: "邮箱", field: "email", sortBy: true },
	];

	const mutationUser = useReactive<Partial<Users>>({
		username: "",
		email: "",
		password: "",
		nickname: "",
	});

	const mutationColumns = [
		{
			header: "用户名",
			field: "username",
			label: "用户名",
			required: true,
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
			header: "邮箱",
			field: "email",
			label: "邮箱",
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
			header: "昵称",
			field: "nickname",
			label: "昵称",
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
			header: "密码",
			field: "password",
			label: "密码（至少 8 位）",
			required: true,
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
