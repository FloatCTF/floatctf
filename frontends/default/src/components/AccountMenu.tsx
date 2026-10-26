import type { GlobalLogoutReport, WebAccount } from "@floatctf/sdk";
import {
	AlertIcon,
	CheckCircleIcon,
	PersonAddIcon,
	PersonIcon,
	SignOutIcon,
	SyncIcon,
	XCircleIcon,
} from "@primer/octicons-react";
import { ActionList, ActionMenu, Avatar, Dialog, Text, useConfirm } from "@primer/react";
import { useState } from "react";

import { client } from "@/api";
import { useAppNavigate } from "@/navigation";
import { mutateSession } from "@/session";
import { useSessionStore } from "@/stores/SessionStore";

import { formatApiError } from "./apiErrorMessage";

/**
 * 头像：只用平台上传的同源图片（`/public/...`）；没有头像或图片加载失败（404、存储未公开等）时显示首字母，
 * 不出现浏览器的破图标记。
 */
export function AccountAvatar({ account, size = 20 }: { account: Pick<WebAccount, "avatar" | "nickname" | "username">; size?: number }) {
	const [failedSrc, setFailedSrc] = useState<string | null>(null);
	const src = account.avatar?.startsWith("/public/") ? account.avatar : null;
	if (src && failedSrc !== src) {
		// 固定尺寸、不收缩：按钮的 leadingVisual 是 flex 子项，裸 <img> 会被压成一条线。
		return (
			<span className="d-inline-flex flex-shrink-0" style={{ width: size, height: size }}>
				<Avatar src={src} size={size} alt="" onError={() => setFailedSrc(src)} />
			</span>
		);
	}
	const initial = (account.nickname || account.username).trim().charAt(0).toUpperCase() || "?";
	return (
		<span
			aria-hidden="true"
			data-avatar-fallback=""
			className="d-inline-flex flex-items-center flex-justify-center circle color-bg-subtle color-fg-default text-bold"
			style={{ width: size, height: size, fontSize: Math.round(size * 0.45) }}
		>
			{initial}
		</span>
	);
}

/**
 * 主站账号菜单（统一 Cookie 会话部署）：
 * - 本浏览器中真实有效的账号（服务端判定）之间切换；
 * - 添加账号（保留当前已登录的账号）；
 * - 退出当前账号 / 退出本浏览器全部账号 —— 只撤销 FloatCTF 身份会话，**不**通知第三方应用。
 *
 * 所有操作都等服务端确认后才更新界面；失败弹出「操作未完成」。
 */
export function AccountMenu() {
	const status = useSessionStore((s) => s.status);
	const navigate = useAppNavigate();
	const confirm = useConfirm();
	const [error, setError] = useState<{ title: string; message: string } | null>(null);
	const [pending, setPending] = useState(false);
	const setLogoutReport = useSessionStore((s) => s.setLogoutReport);

	if (!status?.user) return null;
	const current = status.user;
	const others = status.accounts.filter((a) => !a.active);

	const run = async (title: string, action: () => Promise<void>) => {
		if (pending) return;
		setPending(true);
		try {
			await action();
			setError(null);
		} catch (e) {
			setError({ title, message: formatApiError(e) });
		} finally {
			setPending(false);
		}
	};

	const switchTo = (account: WebAccount) =>
		run("切换账号未完成", async () => {
			await mutateSession((csrf) => client.service.webSession.select(account.id, csrf));
			navigate({ to: "/service/top" });
		});

	const signOutCurrent = () =>
		run("退出未完成", async () => {
			await mutateSession((csrf) => client.service.webSession.logout("account", null, csrf));
			navigate({ to: "/" });
		});

	const signOutEverywhere = async () => {
		const ok = await confirm({
			title: "退出 FloatCTF 及所有关联应用？",
			content:
				"将退出本浏览器中的当前账号，并通知用该次登录进入过的、支持标准退出的应用（如 LearnOS）结束会话。不支持即时通知的应用会如实列出，需要你在应用内手动退出。其他设备上的登录不受影响。",
			confirmButtonContent: "退出所有关联应用",
			confirmButtonType: "danger",
		});
		if (!ok) return;
		await run("退出未完成", async () => {
			const next = await mutateSession((csrf) => client.service.webSession.logout("global", null, csrf));
			// 退出后 /service 守卫会把页面带回登录页并卸载本菜单：结果交给根布局的 GlobalLogoutNotice 展示。
			setLogoutReport(next.logout_report ?? { deliveries: [], without_backchannel: [] });
		});
	};

	const signOutBrowser = async () => {
		const ok = await confirm({
			title: "退出本浏览器中的全部账号？",
			content: `将退出本浏览器中已登录的 ${status.accounts.length} 个 FloatCTF 账号。已经登录的第三方应用（如 LearnOS）不受影响。`,
			confirmButtonContent: "全部退出",
			confirmButtonType: "danger",
		});
		if (!ok) return;
		await run("退出未完成", async () => {
			await mutateSession((csrf) => client.service.webSession.logout("browser", null, csrf));
			navigate({ to: "/" });
		});
	};

	return (
		<>
			<ActionMenu>
				<ActionMenu.Button
					aria-label={`账号菜单：${current.nickname}（@${current.username}）`}
					leadingVisual={() => <AccountAvatar account={current} />}
				>
					{current.nickname}
				</ActionMenu.Button>
				<ActionMenu.Overlay width="medium" align="end">
					<ActionList>
						<ActionList.Group>
							<ActionList.GroupHeading>当前账号</ActionList.GroupHeading>
							<ActionList.Item onSelect={() => navigate({ to: "/service/profile" })}>
								<ActionList.LeadingVisual>
									<AccountAvatar account={current} />
								</ActionList.LeadingVisual>
								{current.nickname}
								<ActionList.Description variant="block">@{current.username} · 个人资料</ActionList.Description>
							</ActionList.Item>
						</ActionList.Group>
						{others.length > 0 && (
							<ActionList.Group>
								<ActionList.GroupHeading>切换到本浏览器中的其他账号</ActionList.GroupHeading>
								{others.map((account) => (
									<ActionList.Item key={account.id} onSelect={() => void switchTo(account)} disabled={pending}>
										<ActionList.LeadingVisual>
											<AccountAvatar account={account} />
										</ActionList.LeadingVisual>
										{account.nickname}
										<ActionList.Description variant="block">@{account.username}</ActionList.Description>
									</ActionList.Item>
								))}
							</ActionList.Group>
						)}
						<ActionList.Divider />
						{status.accounts.length < status.max_accounts && (
							<ActionList.Item onSelect={() => navigate({ to: "/", search: { add: true } })}>
								<ActionList.LeadingVisual>
									<PersonAddIcon />
								</ActionList.LeadingVisual>
								添加账号
							</ActionList.Item>
						)}
						<ActionList.Item onSelect={() => void signOutCurrent()} disabled={pending}>
							<ActionList.LeadingVisual>
								<SignOutIcon />
							</ActionList.LeadingVisual>
							退出当前账号
							<ActionList.Description variant="block">只退出 FloatCTF，不影响已登录的第三方应用</ActionList.Description>
						</ActionList.Item>
						{status.global_logout && (
							<ActionList.Item variant="danger" onSelect={() => void signOutEverywhere()} disabled={pending}>
								<ActionList.LeadingVisual>
									<SignOutIcon />
								</ActionList.LeadingVisual>
								退出所有关联应用…
								<ActionList.Description variant="block">同时通知已接入标准退出的应用</ActionList.Description>
							</ActionList.Item>
						)}
						{status.accounts.length > 1 && (
							<ActionList.Item variant="danger" onSelect={() => void signOutBrowser()} disabled={pending}>
								<ActionList.LeadingVisual>
									<PersonIcon />
								</ActionList.LeadingVisual>
								退出本浏览器中的全部账号
							</ActionList.Item>
						)}
					</ActionList>
				</ActionMenu.Overlay>
			</ActionMenu>
			{error && (
				<Dialog
					title={error.title}
					onClose={() => setError(null)}
					footerButtons={[{ content: "关闭", onClick: () => setError(null) }]}
				>
					<p>服务端没有确认这次操作（{error.message}），当前登录状态没有改变。请稍后重试。</p>
				</Dialog>
			)}
		</>
	);
}

const STATE_TEXT = {
	delivered: { icon: CheckCircleIcon, text: "已确认退出", className: "color-fg-success" },
	pending: { icon: SyncIcon, text: "未送达，将自动重试", className: "color-fg-attention" },
	failed: { icon: XCircleIcon, text: "未能确认", className: "color-fg-danger" },
} as const;

/**
 * 「退出所有关联应用」的结果（挂在根布局，跨路由保留）：逐个应用如实展示，不把未送达说成成功。
 */
export function GlobalLogoutNotice() {
	const report = useSessionStore((s) => s.logoutReport);
	const setLogoutReport = useSessionStore((s) => s.setLogoutReport);
	const navigate = useAppNavigate();
	if (!report) return null;
	return (
		<GlobalLogoutResult
			report={report}
			onClose={() => {
				setLogoutReport(null);
				navigate({ to: "/" });
			}}
		/>
	);
}

function GlobalLogoutResult({ report, onClose }: { report: GlobalLogoutReport; onClose: () => void }) {
	const synced =
		report.deliveries.every((d) => d.status === "delivered") && report.without_backchannel.length === 0;
	return (
		<Dialog
			title={synced ? "已退出 FloatCTF 及关联应用" : "已退出 FloatCTF，但应用未全部同步"}
			onClose={onClose}
			footerButtons={[{ content: "完成", buttonType: "primary", onClick: onClose }]}
		>
			<div className="d-flex flex-column gap-2">
				{report.deliveries.length === 0 && report.without_backchannel.length === 0 && (
					<Text as="p" className="m-0">该次登录没有进入过任何第三方应用，FloatCTF 登录已撤销。</Text>
				)}
				{report.deliveries.map((d) => {
					const state = STATE_TEXT[d.status];
					const Icon = state.icon;
					return (
						<div key={`${d.client_id}-${d.client_name}`} className="d-flex flex-items-center gap-2">
							<span className={state.className}>
								<Icon />
							</span>
							<span className="text-bold">{d.client_name}</span>
							<span className={`ml-auto ${state.className}`} title={d.detail ?? undefined}>
								{state.text}
							</span>
						</div>
					);
				})}
				{report.without_backchannel.length > 0 && (
					<div className="d-flex gap-2 color-fg-attention">
						<AlertIcon />
						<Text as="p" className="m-0">
							以下应用不支持即时退出通知，你在其中的登录可能仍然有效，请在应用内手动退出：
							{report.without_backchannel.join("、")}
						</Text>
					</div>
				)}
			</div>
		</Dialog>
	);
}
