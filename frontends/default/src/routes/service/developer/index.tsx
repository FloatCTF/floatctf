import { ActionList, Flash, Select, Text, useConfirm } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useState } from "react";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import {
	ClientTextInputs,
	type OidcClientFormState,
	RedirectUrisInput,
	ScopesInput,
	emptyOidcClientForm,
	toOidcClientInput,
} from "@/components/oidc/ClientForm";
import { OidcStatusLabel, OneTimeSecretDialog, formatTime } from "@/components/oidc/shared";
import { useAppNavigate } from "@/navigation";
import type { OidcClientDto, UniResponse } from "@floatctf/sdk";
import {
	OidcClientStatus,
	OidcClientType,
	OidcTokenAuthMethod,
} from "@floatctf/sdk/entity";

export const Route = createFileRoute("/service/developer/")({
	component: RouteComponent,
});

export const OIDC_CLIENTS_KEY = "My OIDC Apps";

/** 表格行 = 应用 DTO；新增对话框额外使用表单专用字段（不会出现在服务端数据里）。 */
type Row = OidcClientDto & {
	redirect_uris_text?: string;
	scopes?: string[];
	new_client_type?: OidcClientType;
	new_auth_method?: OidcTokenAuthMethod;
};

function RouteComponent() {
	const confirmDialog = useConfirm();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();
	const appNavigate = useAppNavigate();
	const [secret, setSecret] = useState<{ clientId: string; secret: string } | null>(null);

	const provider = useQuery({
		queryKey: ["oidc-provider"],
		queryFn: () => serviceApi.oidc.provider(),
		staleTime: 5 * 60_000,
		select: (res) => res.data,
		retry: false,
	});

	const defaults = (): Partial<Row> => ({
		...(emptyOidcClientForm() as Partial<Row>),
		new_client_type: OidcClientType.Confidential,
		new_auth_method: OidcTokenAuthMethod.ClientSecretBasic,
	});
	const form = useReactive<Partial<Row>>(defaults());

	const createFn = async (data: Partial<Row>): Promise<UniResponse<Row>> => {
		const res = await serviceApi.oidc.createClient({
			...toOidcClientInput(data as unknown as OidcClientFormState),
			client_type: data.new_client_type ?? OidcClientType.Confidential,
			token_endpoint_auth_method:
				data.new_client_type === OidcClientType.Public
					? OidcTokenAuthMethod.None
					: data.new_auth_method,
		});
		if (res.data?.client_secret) {
			setSecret({ clientId: res.data.client.client_id, secret: res.data.client_secret });
		}
		return { ...res, data: res.data?.client };
	};

	const statusMutation = useMutation({
		mutationFn: ({ row, enable }: { row: Row; enable: boolean }) =>
			enable
				? serviceApi.oidc.enableClient(row.client_id)
				: serviceApi.oidc.disableClient(row.client_id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [OIDC_CLIENTS_KEY] });
			banner.showBanner("success", "已更新应用状态");
		},
		onError: (error) => banner.showErrorBanner(error),
	});
	const deleteMutation = useMutation({
		mutationFn: (row: Row) => serviceApi.oidc.deleteClient(row.client_id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [OIDC_CLIENTS_KEY] });
			banner.showBanner("success", "应用已删除");
		},
		onError: (error) => banner.showErrorBanner(error),
	});

	const columns = [
		{ accessorKey: "name", header: "应用", field: "name", rowHeader: true },
		{
			accessorKey: "client_id",
			header: "Client ID",
			field: "client_id",
			renderCell: (row: Row) => <span className="font-mono">{row.client_id}</span>,
		},
		{
			accessorKey: "client_type",
			header: "类型",
			field: "client_type",
			renderCell: (row: Row) =>
				row.client_type === OidcClientType.Public ? "公开（PKCE）" : "机密",
		},
		{
			accessorKey: "status",
			header: "状态",
			field: "status",
			renderCell: (row: Row) => <OidcStatusLabel status={row.status} />,
		},
		{
			accessorKey: "allowed_scopes",
			header: "Scopes",
			field: "allowed_scopes",
			renderCell: (row: Row) => row.allowed_scopes.join(" "),
		},
		{
			accessorKey: "redirect_uris",
			header: "回调地址",
			field: "redirect_uris",
			renderCell: (row: Row) => (
				<div className="flex flex-col font-mono text-xs">
					{row.redirect_uris.map((uri) => (
						<span key={uri}>{uri}</span>
					))}
				</div>
			),
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
			renderCell: (row: Row) => formatTime(row.created_at),
		},
	];

	const mutationColumns = [
		{
			header: "name",
			field: "name",
			label: "应用名称",
			required: true,
			render: <ClientTextInputs.Name form={form as unknown as OidcClientFormState} />,
		},
		{
			header: "client_type",
			field: "new_client_type",
			label: "应用类型",
			render: (
				<Select
					value={form.new_client_type}
					onChange={(e) => {
						form.new_client_type = e.target.value as OidcClientType;
					}}
				>
					<Select.Option value={OidcClientType.Confidential}>
						机密客户端（有服务端，可安全保存 secret）
					</Select.Option>
					<Select.Option value={OidcClientType.Public}>
						公开客户端（SPA / 移动端，无 secret，强制 PKCE）
					</Select.Option>
				</Select>
			),
		},
		...(form.new_client_type === OidcClientType.Public
			? []
			: [
					{
						header: "token_endpoint_auth_method",
						field: "new_auth_method",
						label: "Token 端点认证方式",
						render: (
							<Select
								value={form.new_auth_method}
								onChange={(e) => {
									form.new_auth_method = e.target.value as OidcTokenAuthMethod;
								}}
							>
								<Select.Option value={OidcTokenAuthMethod.ClientSecretBasic}>
									client_secret_basic（推荐）
								</Select.Option>
								<Select.Option value={OidcTokenAuthMethod.ClientSecretPost}>
									client_secret_post
								</Select.Option>
							</Select>
						),
					},
				]),
		{
			header: "redirect_uris",
			field: "redirect_uris_text",
			label: "回调地址（每行一个，精确匹配，需 https）",
			required: true,
			render: <RedirectUrisInput form={form as unknown as OidcClientFormState} />,
		},
		{
			header: "scopes",
			field: "scopes",
			label: "申请的用户信息",
			render: <ScopesInput form={form as unknown as OidcClientFormState} />,
		},
		{
			header: "homepage_url",
			field: "homepage_url",
			label: "应用主页（授权页展示域名）",
			render: <ClientTextInputs.Homepage form={form as unknown as OidcClientFormState} />,
		},
		{
			header: "description",
			field: "description",
			label: "简介",
			render: <ClientTextInputs.Description form={form as unknown as OidcClientFormState} />,
		},
	];

	const columnActions = (row: Row) => (
		<ActionList>
			<ActionList.Item
				onSelect={() =>
					appNavigate({
						to: "/service/developer/$clientId",
						params: { clientId: row.client_id },
					})
				}
			>
				详情与接入配置
			</ActionList.Item>
			{row.status === OidcClientStatus.Active && (
				<ActionList.Item onSelect={() => statusMutation.mutate({ row, enable: false })}>
					暂停
				</ActionList.Item>
			)}
			{row.status === OidcClientStatus.Disabled && (
				<ActionList.Item onSelect={() => statusMutation.mutate({ row, enable: true })}>
					恢复
				</ActionList.Item>
			)}
			<ActionList.Item
				variant="danger"
				onSelect={async () => {
					const ok = await confirmDialog({
						title: `删除应用 ${row.name}？`,
						content: `删除后 Client ID 立即失效，${row.active_authorizations} 位用户的授权与已签发令牌一并删除。已登录该应用的用户会话由应用自身管理，不会被 FloatCTF 退出。操作不可撤销。`,
						confirmButtonType: "danger",
					});
					if (ok) deleteMutation.mutate(row);
				}}
			>
				删除
			</ActionList.Item>
		</ActionList>
	);

	if (provider.isError) {
		return (
			<Flash variant="warning" className="m-2">
				本平台未启用 FloatCTF 账号登录（OpenID Connect），暂不能登记第三方应用。
			</Flash>
		);
	}

	return (
		<>
			<Text as="p" className="m-2 text-sm">
				在这里登记你的应用，第三方网站即可使用标准 OpenID Connect 客户端以 FloatCTF 账号登录。
				{provider.data?.require_client_approval && " 新应用需要管理员审批后才能使用。"}
				{provider.data && (
					<>
						{" "}Issuer：<span className="font-mono">{provider.data.issuer}</span>
					</>
				)}
			</Text>
			<GenericTable
				className="m-2"
				subject={OIDC_CLIENTS_KEY}
				columns={columns}
				queryFn={() => serviceApi.oidc.listClients() as Promise<UniResponse<Row[]>>}
				createFn={createFn}
				getRowId={(row) => row.client_id}
				mutationColumns={mutationColumns}
				mutationData={form}
				defaultMutationData={defaults()}
				disablePagination
				columnActions={columnActions}
				externalBanner={banner}
			/>
			{secret && (
				<OneTimeSecretDialog
					clientId={secret.clientId}
					secret={secret.secret}
					onClose={() => setSecret(null)}
				/>
			)}
		</>
	);
}
