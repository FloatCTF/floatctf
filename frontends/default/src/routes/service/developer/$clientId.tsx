import {
	Button,
	Flash,
	FormControl,
	Spinner,
	Text,
	useConfirm,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import {
	ClientTextInputs,
	LogoutUrisInputs,
	type OidcClientFormState,
	RedirectUrisInput,
	ScopesInput,
	emptyOidcClientForm,
	toOidcClientInput,
} from "@/components/oidc/ClientForm";
import {
	CopyButton,
	OidcStatusLabel,
	OneTimeSecretDialog,
	formatTime,
} from "@/components/oidc/shared";
import { AppLink, useAppNavigate } from "@/navigation";
import {
	OidcClientStatus,
	OidcClientType,
} from "@floatctf/sdk/entity";

import { OIDC_CLIENTS_KEY } from "./index";

export const Route = createFileRoute("/service/developer/$clientId")({
	component: RouteComponent,
});

const ConfigRow = ({
	label,
	value,
	onCopied,
}: {
	label: string;
	value: string;
	onCopied: () => void;
}) => (
	<div className="flex flex-col sm:flex-row sm:items-center gap-1 py-1 border-b">
		<Text className="sm:w-48 text-sm font-semibold shrink-0">{label}</Text>
		<span className="font-mono text-sm break-all flex-1">{value}</span>
		<CopyButton value={value} label={label} onCopied={onCopied} />
	</div>
);

function RouteComponent() {
	const { clientId } = Route.useParams();
	const queryClient = useQueryClient();
	const confirmDialog = useConfirm();
	const banner = useMsgBanner();
	const appNavigate = useAppNavigate();
	const [secret, setSecret] = useState<string | null>(null);
	const form = useReactive<OidcClientFormState>(emptyOidcClientForm());

	const client = useQuery({
		queryKey: ["oidc-client", clientId],
		queryFn: () => serviceApi.oidc.getClient(clientId),
		select: (res) => res.data,
	});
	const provider = useQuery({
		queryKey: ["oidc-provider"],
		queryFn: () => serviceApi.oidc.provider(),
		staleTime: 5 * 60_000,
		select: (res) => res.data,
	});
	useTitle(`${client.data?.name ?? "OIDC App"} | FloatCTF`);

	useEffect(() => {
		if (!client.data) return;
		form.name = client.data.name;
		form.homepage_url = client.data.homepage_url ?? "";
		form.description = client.data.description ?? "";
		form.redirect_uris_text = client.data.redirect_uris.join("\n");
		form.scopes = client.data.allowed_scopes;
		form.post_logout_redirect_uris_text = (client.data.post_logout_redirect_uris ?? []).join("\n");
		form.backchannel_logout_uri = client.data.backchannel_logout_uri ?? "";
	}, [client.data, form]);

	const refresh = () => {
		queryClient.invalidateQueries({ queryKey: ["oidc-client", clientId] });
		queryClient.invalidateQueries({ queryKey: [OIDC_CLIENTS_KEY] });
	};
	const onError = (error: unknown) => banner.showErrorBanner(error);

	const saveMutation = useMutation({
		mutationFn: () => serviceApi.oidc.updateClient(clientId, toOidcClientInput(form)),
		onSuccess: () => {
			refresh();
			banner.showBanner("success", "已保存");
		},
		onError,
	});
	const rotateMutation = useMutation({
		mutationFn: (revokePrevious: boolean) =>
			serviceApi.oidc.rotateSecret(clientId, revokePrevious),
		onSuccess: (res) => {
			refresh();
			if (res.data?.client_secret) setSecret(res.data.client_secret);
		},
		onError,
	});
	const statusMutation = useMutation({
		mutationFn: (enable: boolean) =>
			enable ? serviceApi.oidc.enableClient(clientId) : serviceApi.oidc.disableClient(clientId),
		onSuccess: () => {
			refresh();
			banner.showBanner("success", "已更新应用状态");
		},
		onError,
	});
	const deleteMutation = useMutation({
		mutationFn: () => serviceApi.oidc.deleteClient(clientId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [OIDC_CLIENTS_KEY] });
			appNavigate({ to: "/service/developer" });
		},
		onError,
	});

	if (client.isLoading) return <Spinner />;
	if (client.isError || !client.data) {
		return (
			<Flash variant="danger" className="m-2">
				应用不存在或无权访问。<AppLink to="/service/developer">返回开发者中心</AppLink>
			</Flash>
		);
	}
	const c = client.data;
	const copied = () => banner.showBanner("success", "已复制");

	return (
		<div className="flex flex-col gap-4 p-2 max-w-4xl">
			<banner.BannerComponent />
			<div className="flex flex-wrap items-center gap-2">
				<AppLink to="/service/developer">开发者中心</AppLink>
				<span>/</span>
				<h2 className="text-xl font-semibold">{c.name}</h2>
				<OidcStatusLabel status={c.status} />
			</div>
			{c.status === OidcClientStatus.Pending && (
				<Flash>应用正在等待管理员审批，审批通过前无法用于登录。</Flash>
			)}
			{c.status_reason && <Flash variant="warning">管理员备注：{c.status_reason}</Flash>}

			<section>
				<h3 className="text-base mb-1 font-semibold">接入配置</h3>
				<Text as="p" className="text-sm mb-2">
					任何标准 OpenID Connect 客户端只需以下信息即可接入（Discovery 自动获取其余端点）。
				</Text>
				{provider.data && (
					<>
						<ConfigRow label="Issuer" value={provider.data.issuer} onCopied={copied} />
						<ConfigRow label="Discovery" value={provider.data.discovery_url} onCopied={copied} />
					</>
				)}
				<ConfigRow label="Client ID" value={c.client_id} onCopied={copied} />
				<ConfigRow label="Token 端点认证" value={c.token_endpoint_auth_method} onCopied={copied} />
				<ConfigRow label="Scopes" value={c.allowed_scopes.join(" ")} onCopied={copied} />
				<ConfigRow
					label="PKCE"
					value={c.require_pkce ? "必须（code_challenge_method=S256）" : "管理员已豁免：授权请求必须带 nonce"}
					onCopied={copied}
				/>
				{c.redirect_uris.map((uri) => (
					<ConfigRow key={uri} label="回调地址" value={uri} onCopied={copied} />
				))}
				<Text as="p" className="text-sm mt-2">
					以 ID Token 的 <code>iss</code> + <code>sub</code> 作为用户唯一标识；<code>email_verified</code> 恒为 false，请勿据此认定邮箱已验证。
				</Text>
			</section>

			{c.client_type === OidcClientType.Confidential && (
				<section>
					<h3 className="text-base mb-1 font-semibold">Client Secret</h3>
					<Text as="p" className="text-sm">
						最近轮换：{formatTime(c.secret_rotated_at ?? c.created_at)}
						{c.previous_secret_expires_at &&
							`；旧 secret 将于 ${formatTime(c.previous_secret_expires_at)} 失效`}
					</Text>
					<div className="flex flex-wrap gap-2 mt-2">
						<Button
							onClick={async () => {
								const ok = await confirmDialog({
									title: "轮换 Client Secret？",
									content:
										"将生成新的 secret（只显示一次）。旧 secret 在宽限期内仍然可用，便于你无停机更新配置。",
								});
								if (ok) rotateMutation.mutate(false);
							}}
						>
							轮换 secret
						</Button>
						<Button
							variant="danger"
							onClick={async () => {
								const ok = await confirmDialog({
									title: "轮换并立即作废旧 secret？",
									content:
										"适用于 secret 疑似泄露。旧 secret 立即失效，未更新配置的服务会立刻无法换取令牌。",
									confirmButtonType: "danger",
								});
								if (ok) rotateMutation.mutate(true);
							}}
						>
							轮换并立即作废旧 secret
						</Button>
					</div>
				</section>
			)}

			<section className="flex flex-col gap-2">
				<h3 className="text-base font-semibold">编辑应用</h3>
				<FormControl required>
					<FormControl.Label>应用名称</FormControl.Label>
					<ClientTextInputs.Name form={form} />
				</FormControl>
				<FormControl required>
					<FormControl.Label>回调地址（每行一个，精确匹配，需 https）</FormControl.Label>
					<RedirectUrisInput form={form} />
				</FormControl>
				<FormControl>
					<FormControl.Label>申请的用户信息</FormControl.Label>
					<ScopesInput form={form} />
				</FormControl>
				<FormControl>
					<FormControl.Label>应用主页</FormControl.Label>
					<ClientTextInputs.Homepage form={form} />
				</FormControl>
				<FormControl>
					<FormControl.Label>退出后回跳地址（每行一个，精确匹配）</FormControl.Label>
					<LogoutUrisInputs.PostLogout form={form} />
					<FormControl.Caption>
						用于 OpenID Connect RP-Initiated Logout（end_session_endpoint 的 post_logout_redirect_uri）。
					</FormControl.Caption>
				</FormControl>
				<FormControl>
					<FormControl.Label>后端退出通知地址（Back-Channel Logout）</FormControl.Label>
					<LogoutUrisInputs.Backchannel form={form} />
					<FormControl.Caption>
						{provider.data?.backchannel_logout_supported
							? "仅当用户明确选择「退出所有关联应用」时，FloatCTF 才会向此 https 地址发送签名的 Logout Token；内网地址需运维在 TOML 中放行。"
							: "当前部署未开启标准退出，此地址暂不会收到通知。"}
					</FormControl.Caption>
				</FormControl>
				<FormControl>
					<FormControl.Label>简介</FormControl.Label>
					<ClientTextInputs.Description form={form} />
				</FormControl>
				<div>
					<Button variant="primary" onClick={() => saveMutation.mutate()}>
						保存
					</Button>
				</div>
			</section>

			<section className="flex flex-col gap-2">
				<h3 className="text-base font-semibold">危险操作</h3>
				<Text as="p" className="text-sm">
					当前 {c.active_authorizations} 位用户已授权该应用。暂停后无法发起登录，已签发 access token 立即失效；已登录用户的应用内会话由应用自身管理。
				</Text>
				<div className="flex flex-wrap gap-2">
					{c.status === OidcClientStatus.Active && (
						<Button onClick={() => statusMutation.mutate(false)}>暂停应用</Button>
					)}
					{c.status === OidcClientStatus.Disabled && (
						<Button onClick={() => statusMutation.mutate(true)}>恢复应用</Button>
					)}
					<Button
						variant="danger"
						onClick={async () => {
							const ok = await confirmDialog({
								title: `删除应用 ${c.name}？`,
								content: "Client ID 立即失效，授权与令牌一并删除，操作不可撤销。",
								confirmButtonType: "danger",
							});
							if (ok) deleteMutation.mutate();
						}}
					>
						删除应用
					</Button>
				</div>
			</section>

			{secret && (
				<OneTimeSecretDialog clientId={c.client_id} secret={secret} onClose={() => setSecret(null)} />
			)}
		</div>
	);
}
