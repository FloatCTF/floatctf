import { client, serviceApi, userServiceApi } from "@/api";
import { useAppNavigate } from "@/navigation";
import { isCookieMode, mutateSession } from "@/session";
import { useAuthStore } from "@/stores/AuthStore";
import { useSessionStore } from "@/stores/SessionStore";
import { getLearnOSUrl } from "@/util";
import { Button, Dialog } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { AccountMenu } from "./AccountMenu";
import { formatApiError } from "./apiErrorMessage";

export interface GenericHeaderProps
	extends React.HTMLAttributes<HTMLDivElement> {
	title: string;
	homePath: string;
	logo?: string;
	showUserInfo?: boolean;
	userProfilePath?: string;
	useProfileQuery?: boolean;
	onLogout: () => void;
}

export const GenericHeader = ({
	title,
	homePath,
	logo,
	showUserInfo = false,
	useProfileQuery = false,
	userProfilePath = "/service/profile",
	onLogout,
	...props
}: GenericHeaderProps) => {
	const authStore = useAuthStore();
	const appNavigate = useAppNavigate();
	const cookieMode = useSessionStore((s) => s.mode === "cookie");

	const { data, isLoading } = useQuery({
		queryKey: ["profile"],
		queryFn: () => userServiceApi.getMe(),
		enabled: useProfileQuery,
	});

	const profile = data?.data;

	useEffect(() => {
		if (useProfileQuery && profile && authStore.nickname !== profile.nickname) {
			authStore.setNickname(profile.nickname);
		}
	}, [profile, authStore, useProfileQuery]);

	const handleGoHome = () => {
		appNavigate({ to: homePath });
	};

	const handleGoProfile = () => {
		appNavigate({ to: userProfilePath });
	};

	if (useProfileQuery && isLoading) {
		return <></>;
	}

	return (
		<header
			className="bgColor-muted border-bottom d-flex flex-items-center p-2 justify-between"
			{...props}
		>
			<div
				className="d-flex flex-items-center gap-2 hover:cursor-pointer"
				onClick={handleGoHome}
				onKeyUp={(e) => e.key === "Enter" && handleGoHome()}
				role="button"
				tabIndex={0}
			>
				{logo && (
					<img
						src={logo}
						alt="Logo"
						style={{ width: 28, height: 28, borderRadius: 4 }}
					/>
				)}
				<h3>{title}</h3>
			</div>

			{showUserInfo ? (
				<div className="flex justify-center items-center gap-2">
					<a
						className="btn"
						href={getLearnOSUrl()}
						target="_blank"
						rel="noopener noreferrer"
					>
						LearnOS
					</a>
					{cookieMode ? (
						// 统一会话：账号菜单（切换 / 添加 / 退出），状态以服务端为准。
						<AccountMenu />
					) : (
						<>
							<span
								className="hover:cursor-pointer hover:underline"
								onKeyUp={(e) => e.key === "Enter" && handleGoProfile()}
								onClick={handleGoProfile}
							>
								{authStore.username}
								{authStore.nickname && <span>:{authStore.nickname}</span>}
							</span>
							<Button variant="danger" onClick={onLogout}>
								Logout
							</Button>
						</>
					)}
				</div>
			) : (
				<Button variant="danger" onClick={onLogout}>
					Logout
				</Button>
			)}
		</header>
	);
};

/**
 * 退出 FloatCTF（当前账号）：**服务端确认撤销后**才更新界面；失败如实告知「未完成退出」，
 * 绝不出现「界面显示已退出、身份会话仍有效」。不通知第三方应用（LearnOS 已有会话保持不变）。
 */
async function signOutCurrentAccount(): Promise<void> {
	if (isCookieMode()) {
		await mutateSession((csrf) =>
			client.service.webSession.logout("account", null, csrf),
		);
		return;
	}
	// 旧版 JWT 模式：先撤销 OIDC 身份会话，成功后再丢弃本地令牌。
	try {
		await serviceApi.oidc.endSso();
	} catch (error) {
		// 旧版 API 没有该接口：没有服务端会话可撤销。
		if ((error as { httpStatus?: number }).httpStatus !== 404) throw error;
	}
	useAuthStore.getState().removeToken();
}

export const ServiceHeader = () => {
	const appNavigate = useAppNavigate();
	const [logoutError, setLogoutError] = useState<string | null>(null);
	const [pending, setPending] = useState(false);

	const logout = async () => {
		if (pending) return;
		setPending(true);
		try {
			await signOutCurrentAccount();
			setLogoutError(null);
			appNavigate({ to: "/" });
		} catch (error) {
			setLogoutError(formatApiError(error));
		} finally {
			setPending(false);
		}
	};

	return (
		<>
			<GenericHeader
				title="FloatCTF"
				homePath="/"
				logo="/float.png"
				showUserInfo
				useProfileQuery
				onLogout={() => void logout()}
			/>
			{logoutError && (
				<Dialog
					title="退出未完成"
					onClose={() => setLogoutError(null)}
					footerButtons={[
						{ content: "关闭", onClick: () => setLogoutError(null) },
						{
							content: "重试",
							buttonType: "primary",
							onClick: () => void logout(),
						},
					]}
				>
					<p>你仍处于登录状态：服务端没有确认撤销本次登录（{logoutError}）。请重试。</p>
				</Dialog>
			)}
		</>
	);
};
export const AdminHeader = () => {
	const authStore = useAuthStore();
	const appNavigate = useAppNavigate();

	return (
		<GenericHeader
			title="FloatCTF Admin"
			homePath="/admin"
			logo="/float.png"
			showUserInfo={false}
			useProfileQuery={false}
			onLogout={() => {
				authStore.removeAdminToken();
				appNavigate({ to: "/admin" });
			}}
		/>
	);
};
