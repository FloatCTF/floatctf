import { ActionList, Text, useConfirm } from "@primer/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import {
	OIDC_STATUS_TEXT,
	OidcStatusLabel,
	formatTime,
} from "@/components/oidc/shared";
import type { OidcAdminClientDto } from "@floatctf/sdk";
import { OidcClientStatus, OidcClientType } from "@floatctf/sdk/entity";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/oidc_clients")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const SUBJECT = "OIDC Clients";

function RouteComponent() {
	const confirmDialog = useConfirm();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();

	const reviewMutation = useMutation({
		mutationFn: async ({
			row,
			action,
		}: {
			row: OidcAdminClientDto;
			action: "approve" | "reject" | "suspend" | "reinstate" | "pkce-on" | "pkce-off";
		}) => {
			switch (action) {
				case "pkce-on":
					return adminApi.oidc_clients.setRequirePkce(row.client_id, true);
				case "pkce-off":
					return adminApi.oidc_clients.setRequirePkce(row.client_id, false);
				default:
					return adminApi.oidc_clients[action](row.client_id);
			}
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [SUBJECT] });
			banner.showBanner("success", "已更新");
		},
		onError: (error) => banner.showErrorBanner(error),
	});

	const columns = [
		{ accessorKey: "name", header: "应用", field: "name", rowHeader: true },
		{
			accessorKey: "client_id",
			header: "Client ID",
			field: "client_id",
			renderCell: (row: OidcAdminClientDto) => (
				<span className="font-mono">{row.client_id}</span>
			),
		},
		{
			accessorKey: "owner_username",
			header: "所有者",
			field: "owner_username",
		},
		{
			accessorKey: "client_type",
			header: "类型",
			field: "client_type",
			renderCell: (row: OidcAdminClientDto) =>
				row.client_type === OidcClientType.Public ? "公开（PKCE）" : "机密",
		},
		{
			accessorKey: "status",
			header: "状态",
			field: "status",
			renderCell: (row: OidcAdminClientDto) => (
				<OidcStatusLabel status={row.status} />
			),
		},
		{
			accessorKey: "redirect_uris",
			header: "回调地址",
			field: "redirect_uris",
			renderCell: (row: OidcAdminClientDto) => (
				<div className="flex flex-col font-mono text-xs">
					{row.redirect_uris.map((uri) => (
						<span key={uri}>{uri}</span>
					))}
				</div>
			),
		},
		{
			accessorKey: "require_pkce",
			header: "PKCE",
			field: "require_pkce",
			renderCell: (row: OidcAdminClientDto) =>
				row.require_pkce ? "强制" : "已豁免（需 nonce）",
		},
		{
			accessorKey: "active_authorizations",
			header: "授权用户",
			field: "active_authorizations",
		},
		{
			accessorKey: "created_at",
			header: "创建时间",
			field: "created_at",
			renderCell: (row: OidcAdminClientDto) => formatTime(row.created_at),
		},
	];

	const act = async (
		row: OidcAdminClientDto,
		action: Parameters<typeof reviewMutation.mutate>[0]["action"],
		confirm?: { title: string; content: string },
	) => {
		if (confirm) {
			const ok = await confirmDialog({
				...confirm,
				confirmButtonType: "danger",
			});
			if (!ok) return;
		}
		reviewMutation.mutate({ row, action });
	};

	const columnActions = (row: OidcAdminClientDto) => (
		<ActionList>
			{(row.status === OidcClientStatus.Pending ||
				row.status === OidcClientStatus.Rejected) && (
				<ActionList.Item onSelect={() => act(row, "approve")}>
					批准
				</ActionList.Item>
			)}
			{row.status === OidcClientStatus.Pending && (
				<ActionList.Item
					variant="danger"
					onSelect={() =>
						act(row, "reject", {
							title: `拒绝 ${row.name}？`,
							content: "应用将保持不可用，所有者无法自行启用。",
						})
					}
				>
					拒绝
				</ActionList.Item>
			)}
			{[
				OidcClientStatus.Pending,
				OidcClientStatus.Active,
				OidcClientStatus.Disabled,
			].includes(row.status) && (
				<ActionList.Item
					variant="danger"
					onSelect={() =>
						act(row, "suspend", {
							title: `封禁 ${row.name}？`,
							content: `封禁后该应用无法发起登录，已签发的 access token 立即失效（当前 ${row.active_authorizations} 位用户已授权）。所有者不能自行解除封禁。`,
						})
					}
				>
					封禁
				</ActionList.Item>
			)}
			{row.status === OidcClientStatus.Suspended && (
				<ActionList.Item onSelect={() => act(row, "reinstate")}>
					解除封禁
				</ActionList.Item>
			)}
			{row.client_type === OidcClientType.Confidential &&
				(row.require_pkce ? (
					<ActionList.Item
						variant="danger"
						onSelect={() =>
							act(row, "pkce-off", {
								title: `为 ${row.name} 豁免 PKCE？`,
								content:
									"仅用于无法发送 PKCE 的机密客户端（如 OIDF 一致性测试）。豁免后授权请求必须携带 nonce。公开客户端不能豁免。",
							})
						}
					>
						豁免 PKCE
					</ActionList.Item>
				) : (
					<ActionList.Item onSelect={() => act(row, "pkce-on")}>
						恢复强制 PKCE
					</ActionList.Item>
				))}
		</ActionList>
	);

	return (
		<>
			<Text as="p" className="m-2 text-sm">
				第三方应用通过 FloatCTF 账号登录（OpenID Connect）。新登记的应用需要在此批准后才能使用。
				{" "}状态：{Object.values(OIDC_STATUS_TEXT).join(" / ")}。
			</Text>
			<GenericTable
				className="m-2"
				subject={SUBJECT}
				columns={columns}
				queryFn={() => adminApi.oidc_clients.fetch()}
				getRowId={(row) => row.client_id}
				disableAdd
				disablePagination
				columnActions={columnActions}
				externalBanner={banner}
			/>
		</>
	);
}
