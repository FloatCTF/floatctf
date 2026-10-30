import type { WebAccount } from "@floatctf/sdk";
import {
	ActionList,
	Avatar,
	Button,
	FormControl,
	Heading,
	Link,
	Text,
	TextInput,
} from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useRef } from "react";
import { z } from "zod";

import { client, serviceApi } from "@/api";
import { useMsgInlineBanner } from "@/components";
import { AccountAvatar } from "@/components/AccountMenu";
import { AppLink } from "@/navigation";
import { isCookieMode, isSignedIn, mutateSession } from "@/session";
import { useAuthStore } from "@/stores/AuthStore";
import { useSessionStore } from "@/stores/SessionStore";
import { getLearnOSUrl } from "@/util";

const loginSearchSchema = z.object({
	/** 已登录时「添加账号」：保留当前账号，再登录一个。 */
	add: z.boolean().optional().catch(undefined),
});

export const Route = createFileRoute("/")({
	component: App,
	validateSearch: (search) => loginSearchSchema.parse(search),
	beforeLoad: async ({ search }) => {
		if (!search.add && (await isSignedIn())) {
			throw redirect({ to: "/service/top" });
		}
	},
});

/**
 * 统一会话：本浏览器里还有有效的 FloatCTF 账号、但主站当前未登录（例如刚退出其中一个账号）时，
 * 让用户**明确选择**用哪个账号继续，绝不按最近使用自动挑选。
 */
function BrowserAccounts({ accounts }: { accounts: WebAccount[] }) {
	const navigate = useNavigate();
	const banner = useMsgInlineBanner();
	const select = useMutation({
		mutationFn: (account: WebAccount) =>
			mutateSession((csrf) => client.service.webSession.select(account.id, csrf)),
		onSuccess: () => navigate({ to: "/service" }),
		onError: (error) => banner.showErrorBanner(error),
	});
	return (
		<div className="w-full max-w-sm">
			<Text as="p" className="color-fg-muted mb-2">
				本浏览器中已登录的 FloatCTF 账号，点选即可继续：
			</Text>
			<div className="border rounded-2">
				<ActionList aria-label="本浏览器中已登录的账号">
					{accounts.map((account) => (
						<ActionList.Item
							key={account.id}
							onSelect={() => select.mutate(account)}
							disabled={select.isPending}
						>
							<ActionList.LeadingVisual>
								<AccountAvatar account={account} />
							</ActionList.LeadingVisual>
							{account.nickname}
							<ActionList.Description variant="block">@{account.username}</ActionList.Description>
						</ActionList.Item>
					))}
				</ActionList>
			</div>
			<banner.BannerComponent />
			<Text as="p" className="color-fg-muted mt-3 mb-0">
				或使用其他账号登录：
			</Text>
		</div>
	);
}

function App() {
	useTitle("Login | FloatCTF");
	const banner = useMsgInlineBanner();
	const { add } = Route.useSearch();
	const session = useSessionStore((s) => s.status);
	const cookieMode = session?.mode === "cookie";
	const adding = Boolean(add && cookieMode && session?.authenticated);
	const pickable =
		cookieMode && !session?.authenticated && !add ? session.accounts : [];

	const form = useReactive({
		username: "",
		password: "",
		buttonMessage: "Sign in",
		buttonDisabled: false,
	});

	const usernameRef = useRef<HTMLInputElement>(null);
	const authStore = useAuthStore();
	const navigate = useNavigate();
	const mutation = useMutation({
		// 统一会话部署：服务端建立可撤销的身份会话（HttpOnly Cookie），浏览器不拿令牌；
		// 旧版部署：沿用平台 JWT。
		mutationFn: async (input: { username: string; password: string }) => {
			if (isCookieMode()) {
				await mutateSession((csrf) => client.service.webSession.login(input, csrf));
				return;
			}
			const data = await serviceApi.users.login(input);
			authStore.setToken(data.data!);
			authStore.setUsername(input.username);
		},
		onMutate: () => {
			banner.hideBanner();
			form.buttonMessage = "Signing in...";
			form.buttonDisabled = true;
		},
		onSuccess: () => {
			banner.showBanner("success", "Login success");
			form.buttonMessage = "Redirecting...";
			form.buttonDisabled = true;
			navigate({ to: "/service" });
		},
		onError: (error) => {
			banner.showErrorBanner(error, { 401: "用户名或密码错误" });
			form.buttonDisabled = false;
			form.buttonMessage = "Sign in";
			if (!isCookieMode()) authStore.removeToken();
		},
	});
	useEffect(() => {
		usernameRef.current?.focus();
	}, []);
	return (
		<div className="flex flex-col items-center justify-center pt-9 gap-5">
			<Avatar size={100} src="/float.png" />
			<Heading>{adding ? "添加 FloatCTF 账号" : "Sign in to FloatCTF"}</Heading>
			{adding && (
				<Text as="p" className="color-fg-muted m-0 text-center">
					当前已登录的账号会保留在本浏览器中，之后可在右上角账号菜单切换。{" "}
					<AppLink to="/service/top">返回</AppLink>
				</Text>
			)}
			{pickable.length > 0 && <BrowserAccounts accounts={pickable} />}
			<form
				onSubmit={(e) => {
					e.preventDefault();
					if (form.username === "" || form.password === "") {
						banner.showBanner("critical", "Username or password is empty");
						return;
					}
					mutation.mutate({
						username: form.username,
						password: form.password,
					});
				}}
				className="flex flex-col gap-2 w-full max-w-sm"
			>
				<FormControl required>
					<FormControl.Label>Username</FormControl.Label>
					<TextInput
						className="w-full"
						ref={usernameRef}
						name="username"
						autoComplete="username"
						placeholder="学号"
						value={form.username}
						onChange={(e) => {
							form.username = e.target.value;
						}}
					/>
				</FormControl>

				<FormControl required>
					<FormControl.Label>Password</FormControl.Label>
					<TextInput
						className="w-full"
						type="password"
						autoComplete="current-password"
						name="password"
						value={form.password}
						onChange={(e) => {
							form.password = e.target.value;
						}}
					/>
				</FormControl>

				<Button type="submit" variant="primary" disabled={form.buttonDisabled}>
					{form.buttonMessage}
				</Button>
				<div className="flex justify-between">
					<AppLink to="/register">Don't have an account? Register</AppLink>
					<AppLink to="/reset">Forgot password?</AppLink>
				</div>
			</form>

			<banner.BannerComponent />

			<Link href={getLearnOSUrl()} target="_blank" rel="noopener noreferrer">
				前往 LearnOS 学习平台 →
			</Link>
		</div>
	);
}
