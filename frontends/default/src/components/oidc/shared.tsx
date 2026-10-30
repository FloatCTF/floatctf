import { CopyIcon } from "@primer/octicons-react";
import { Dialog } from "@primer/react/experimental";
import { Flash, IconButton, Label, Text } from "@primer/react";
import { OidcClientStatus } from "@floatctf/sdk/entity";
import dayjs from "dayjs";

/** OIDC 应用状态 → 文案与 Label 配色（与后端 oidc_client_status 一一对应）。 */
export const OIDC_STATUS_TEXT: Record<OidcClientStatus, string> = {
	[OidcClientStatus.Pending]: "待审批",
	[OidcClientStatus.Active]: "已启用",
	[OidcClientStatus.Disabled]: "已暂停",
	[OidcClientStatus.Suspended]: "已封禁",
	[OidcClientStatus.Rejected]: "未通过",
};

const STATUS_VARIANT: Record<
	OidcClientStatus,
	"attention" | "success" | "secondary" | "danger" | "severe"
> = {
	[OidcClientStatus.Pending]: "attention",
	[OidcClientStatus.Active]: "success",
	[OidcClientStatus.Disabled]: "secondary",
	[OidcClientStatus.Suspended]: "danger",
	[OidcClientStatus.Rejected]: "severe",
};

export const OidcStatusLabel = ({ status }: { status: OidcClientStatus }) => (
	<Label variant={STATUS_VARIANT[status]}>{OIDC_STATUS_TEXT[status]}</Label>
);

/** scope → 用户可读含义。 */
export const OIDC_SCOPE_TEXT: Record<string, string> = {
	openid: "账号标识（必需）",
	profile: "用户名、昵称、头像",
	email: "邮箱（email_verified=false）",
};

export const formatTime = (value?: string | null) =>
	value ? dayjs(value).format("YYYY-MM-DD HH:mm") : "—";

/** 一键复制。 */
export const CopyButton = ({
	value,
	label,
	onCopied,
}: {
	value: string;
	label: string;
	onCopied?: () => void;
}) => (
	<IconButton
		icon={CopyIcon}
		aria-label={`复制${label}`}
		size="small"
		variant="invisible"
		onClick={async () => {
			await navigator.clipboard.writeText(value);
			onCopied?.();
		}}
	/>
);

/** 只展示一次的 client secret。关闭后无法再次查看（服务端只存哈希）。 */
export const OneTimeSecretDialog = ({
	clientId,
	secret,
	onClose,
}: {
	clientId: string;
	secret: string;
	onClose: () => void;
}) => (
	<Dialog
		title="请立即保存 Client Secret"
		onClose={onClose}
		footerButtons={[{ buttonType: "primary", content: "我已妥善保存", onClick: onClose }]}
	>
		<Flash variant="warning" className="mb-3">
			Client Secret 只显示这一次，FloatCTF 不保存明文、之后无法再次查看。请保存到服务端的密钥管理中，切勿放进前端代码或公开仓库。
		</Flash>
		<div className="flex flex-col gap-2">
			<Text className="text-sm">Client ID</Text>
			<div className="flex items-center gap-1 font-mono break-all">
				{clientId}
				<CopyButton value={clientId} label=" Client ID" />
			</div>
			<Text className="text-sm">Client Secret</Text>
			<div className="flex items-center gap-1 font-mono break-all" data-testid="oidc-client-secret">
				{secret}
				<CopyButton value={secret} label=" Client Secret" />
			</div>
		</div>
	</Dialog>
);
