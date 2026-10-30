import { ActionList, Text, useConfirm } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import { OIDC_SCOPE_TEXT, OidcStatusLabel, formatTime } from "@/components/oidc/shared";
import type { OidcAuthorizationDto } from "@floatctf/sdk";

export const Route = createFileRoute("/service/authorizations")({
	component: RouteComponent,
});

const SUBJECT = "Authorized Apps";

function RouteComponent() {
	const confirmDialog = useConfirm();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();

	const revokeMutation = useMutation({
		mutationFn: (row: OidcAuthorizationDto) => serviceApi.oidc.revokeAuthorization(row.client_id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [SUBJECT] });
			banner.showBanner("success", "已撤销授权");
		},
		onError: (error) => banner.showErrorBanner(error),
	});

	const columns = [
		{ accessorKey: "client_name", header: "应用", field: "client_name", rowHeader: true },
		{
			accessorKey: "homepage_url",
			header: "主页",
			field: "homepage_url",
			renderCell: (row: OidcAuthorizationDto) => row.homepage_url ?? "—",
		},
		{
			accessorKey: "scopes",
			header: "已授权信息",
			field: "scopes",
			renderCell: (row: OidcAuthorizationDto) => (
				<div className="flex flex-col text-sm">
					{row.scopes.map((s) => (
						<span key={s}>{OIDC_SCOPE_TEXT[s] ?? s}</span>
					))}
				</div>
			),
		},
		{
			accessorKey: "client_status",
			header: "应用状态",
			field: "client_status",
			renderCell: (row: OidcAuthorizationDto) => <OidcStatusLabel status={row.client_status} />,
		},
		{
			accessorKey: "granted_at",
			header: "授权时间",
			field: "granted_at",
			renderCell: (row: OidcAuthorizationDto) => formatTime(row.granted_at),
		},
		{
			accessorKey: "last_used_at",
			header: "最近使用",
			field: "last_used_at",
			renderCell: (row: OidcAuthorizationDto) => formatTime(row.last_used_at),
		},
	];

	const columnActions = (row: OidcAuthorizationDto) => (
		<ActionList>
			<ActionList.Item
				variant="danger"
				onSelect={async () => {
					const ok = await confirmDialog({
						title: `撤销对 ${row.client_name} 的授权？`,
						content:
							"撤销后该应用持有的访问令牌立即失效，无法再读取你的资料；下次使用 FloatCTF 登录时需要重新授权。注意：你在该应用内已经登录的会话由应用自己管理，不会被自动退出。",
						confirmButtonType: "danger",
					});
					if (ok) revokeMutation.mutate(row);
				}}
			>
				撤销授权
			</ActionList.Item>
		</ActionList>
	);

	return (
		<>
			<Text as="p" className="m-2 text-sm">
				以下第三方应用可以通过 FloatCTF 账号（OpenID Connect）识别你的身份。FloatCTF 不会向它们提供密码、管理员权限、战队或比赛信息。
			</Text>
			<GenericTable
				className="m-2"
				subject={SUBJECT}
				columns={columns}
				queryFn={() => serviceApi.oidc.listAuthorizations()}
				getRowId={(row) => row.client_id}
				disableAdd
				disablePagination
				columnActions={columnActions}
				externalBanner={banner}
			/>
		</>
	);
}
