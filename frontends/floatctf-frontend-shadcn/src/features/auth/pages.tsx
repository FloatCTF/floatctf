/**
 * 认证页面 —— 选手登录 / 注册 / 忘记密码 / 凭 token 重置 + 管理端登录。
 * 两个 token 作用域互相独立：管理端登录不会影响选手会话，反之亦然。
 */

import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { useMutation } from "@tanstack/react-query";
import { KeyRound, Loader2, LogIn, ShieldCheck, UserPlus } from "lucide-react";

import { call } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { errorText } from "~/api/errors";
import { qk } from "~/api/keys";
import { useAuthStore } from "~/auth/store";
import { AuthCard } from "~/app/public-shell";
import { InlineError } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Field } from "~/components/app/form";
import { Input } from "~/components/ui/input";
import { useDocumentTitle } from "~/lib/hooks";

function nextTarget(raw: string | null, fallback: string): string {
	if (raw && raw.startsWith("/") && !raw.startsWith("//")) return raw;
	return fallback;
}

export function LoginPage() {
	useDocumentTitle("登录 · FloatCTF");
	const client = useClient();
	const navigate = useNavigate();
	const queryClient = useAppQueryClient();
	const setUserToken = useAuthStore((state) => state.setUserToken);
	const [params] = useSearchParams();
	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const next = nextTarget(params.get("next"), "/");

	const mutation = useMutation({
		mutationFn: (input: { username: string; password: string }) =>
			call<string>(client.service.users.login(input), "登录"),
		onSuccess: (token) => {
			if (!token) {
				toast.error("登录失败", "后端未返回令牌");
				return;
			}
			setUserToken(token);
			void queryClient.invalidateQueries({ queryKey: qk.auth.me() });
			toast.success("登录成功");
			void navigate(next, { replace: true });
		},
		onError: (error) => toast.apiError("登录失败", error),
	});

	return (
		<AuthCard
			title="选手登录"
			description="使用平台账号登录，进入你的比赛工作区。"
			footer={
				<div className="space-y-1 text-muted-foreground">
					<p>
						还没有账号？
						<Link to="/register" className="ml-1 text-foreground underline underline-offset-4">
							注册
						</Link>
					</p>
					<p>
						忘记密码？
						<Link to="/forgot" className="ml-1 text-foreground underline underline-offset-4">
							重置密码
						</Link>
					</p>
				</div>
			}
		>
			<form
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (username.trim() === "" || password === "") {
						toast.warning("请填写账号与密码");
						return;
					}
					mutation.mutate({ username: username.trim(), password });
				}}
			>
				<Field label="账号" htmlFor="login-username" required>
					<Input
						id="login-username"
						name="username"
						autoComplete="username"
						value={username}
						onChange={(event) => setUsername(event.target.value)}
						placeholder="学号 / 用户名"
					/>
				</Field>
				<Field label="密码" htmlFor="login-password" required>
					<Input
						id="login-password"
						name="password"
						type="password"
						autoComplete="current-password"
						value={password}
						onChange={(event) => setPassword(event.target.value)}
					/>
				</Field>
				{mutation.isError ? <InlineError error={mutation.error} /> : null}
				<Button type="submit" className="w-full" disabled={mutation.isPending}>
					{mutation.isPending ? <Loader2 className="animate-spin" /> : <LogIn />}
					登录
				</Button>
			</form>
		</AuthCard>
	);
}

export function RegisterPage() {
	useDocumentTitle("注册 · FloatCTF");
	const client = useClient();
	const navigate = useNavigate();
	const [form, setForm] = useState({
		username: "",
		nickname: "",
		email: "",
		password: "",
		confirm: "",
	});

	const mutation = useMutation({
		mutationFn: (input: {
			username: string;
			password: string;
			nickname: string;
			email: string;
		}) => call<string>(client.service.users.register(input), "注册"),
		onSuccess: () => {
			toast.success("注册成功", "请使用新账号登录。");
			void navigate("/login", { replace: true });
		},
		onError: (error) => toast.apiError("注册失败", error),
	});

	return (
		<AuthCard
			title="注册账号"
			description="注册后即可加入赛事、提交 flag。"
			footer={
				<p className="text-muted-foreground">
					已有账号？
					<Link to="/login" className="ml-1 text-foreground underline underline-offset-4">
						登录
					</Link>
				</p>
			}
		>
			<form
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (form.username.trim() === "" || form.password === "" || form.nickname.trim() === "") {
						toast.warning("账号、昵称、密码均为必填");
						return;
					}
					if (form.password !== form.confirm) {
						toast.warning("两次输入的密码不一致");
						return;
					}
					mutation.mutate({
						username: form.username.trim(),
						nickname: form.nickname.trim(),
						email: form.email.trim(),
						password: form.password,
					});
				}}
			>
				<Field label="账号" htmlFor="register-username" required hint="登录用，注册后不可更改。">
					<Input
						id="register-username"
						autoComplete="username"
						value={form.username}
						onChange={(event) => setForm({ ...form, username: event.target.value })}
					/>
				</Field>
				<Field label="昵称" htmlFor="register-nickname" required>
					<Input
						id="register-nickname"
						value={form.nickname}
						onChange={(event) => setForm({ ...form, nickname: event.target.value })}
					/>
				</Field>
				<Field label="邮箱" htmlFor="register-email" hint="用于找回密码。">
					<Input
						id="register-email"
						type="email"
						autoComplete="email"
						value={form.email}
						onChange={(event) => setForm({ ...form, email: event.target.value })}
					/>
				</Field>
				<Field label="密码" htmlFor="register-password" required>
					<Input
						id="register-password"
						type="password"
						autoComplete="new-password"
						value={form.password}
						onChange={(event) => setForm({ ...form, password: event.target.value })}
					/>
				</Field>
				<Field label="确认密码" htmlFor="register-confirm" required>
					<Input
						id="register-confirm"
						type="password"
						autoComplete="new-password"
						value={form.confirm}
						onChange={(event) => setForm({ ...form, confirm: event.target.value })}
					/>
				</Field>
				{mutation.isError ? <InlineError error={mutation.error} /> : null}
				<Button type="submit" className="w-full" disabled={mutation.isPending}>
					{mutation.isPending ? <Loader2 className="animate-spin" /> : <UserPlus />}
					注册
				</Button>
			</form>
		</AuthCard>
	);
}

export function ForgotPasswordPage() {
	useDocumentTitle("找回密码 · FloatCTF");
	const client = useClient();
	const [username, setUsername] = useState("");
	const [email, setEmail] = useState("");
	const [sent, setSent] = useState(false);

	const mutation = useMutation({
		mutationFn: (input: { username?: string; email?: string }) =>
			call<string>(client.service.users.resetPassword(input), "发送重置邮件"),
		onSuccess: () => {
			setSent(true);
			toast.success("重置请求已提交", "若账号存在，平台会发送重置邮件。");
		},
		onError: (error) => toast.apiError("发送失败", error),
	});

	return (
		<AuthCard
			title="找回密码"
			description="提供账号或邮箱中的一个，平台会发送重置链接。"
			footer={
				<Link to="/login" className="text-muted-foreground underline underline-offset-4">
					返回登录
				</Link>
			}
		>
			<form
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (username.trim() === "" && email.trim() === "") {
						toast.warning("账号或邮箱至少填一个");
						return;
					}
					mutation.mutate({
						...(username.trim() ? { username: username.trim() } : {}),
						...(email.trim() ? { email: email.trim() } : {}),
					});
				}}
			>
				<Field label="账号" htmlFor="forgot-username">
					<Input
						id="forgot-username"
						autoComplete="username"
						value={username}
						onChange={(event) => setUsername(event.target.value)}
					/>
				</Field>
				<Field label="邮箱" htmlFor="forgot-email">
					<Input
						id="forgot-email"
						type="email"
						autoComplete="email"
						value={email}
						onChange={(event) => setEmail(event.target.value)}
					/>
				</Field>
				{mutation.isError ? <InlineError error={mutation.error} /> : null}
				<Button type="submit" className="w-full" disabled={mutation.isPending}>
					{mutation.isPending ? <Loader2 className="animate-spin" /> : <KeyRound />}
					发送重置邮件
				</Button>
				{sent ? (
					<p className="text-xs text-muted-foreground">
						收到邮件后，打开其中的链接（形如 <span className="font-mono">/reset?token=…</span>
						）即可设置新密码。
					</p>
				) : null}
			</form>
		</AuthCard>
	);
}

export function ResetPasswordPage() {
	useDocumentTitle("设置新密码 · FloatCTF");
	const client = useClient();
	const navigate = useNavigate();
	const [params] = useSearchParams();
	const [token, setToken] = useState(params.get("token") ?? "");
	const [password, setPassword] = useState("");
	const [confirm, setConfirm] = useState("");

	const mutation = useMutation({
		mutationFn: (input: { token: string; password: string; confirmed_password: string }) =>
			call<string>(client.service.users.reset(input), "重置密码"),
		onSuccess: () => {
			toast.success("密码已重置", "请使用新密码登录。");
			void navigate("/login", { replace: true });
		},
		onError: (error) => toast.apiError("重置失败", error),
	});

	return (
		<AuthCard
			title="设置新密码"
			description="粘贴邮件里的重置令牌，或从邮件链接直接进入本页。"
			footer={
				<Link to="/login" className="text-muted-foreground underline underline-offset-4">
					返回登录
				</Link>
			}
		>
			<form
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (token.trim() === "") {
						toast.warning("请填写重置令牌");
						return;
					}
					if (password === "" || password !== confirm) {
						toast.warning("两次输入的密码不一致");
						return;
					}
					mutation.mutate({
						token: token.trim(),
						password,
						confirmed_password: confirm,
					});
				}}
			>
				<Field label="重置令牌" htmlFor="reset-token" required>
					<Input
						id="reset-token"
						className="font-mono text-xs"
						value={token}
						onChange={(event) => setToken(event.target.value)}
					/>
				</Field>
				<Field label="新密码" htmlFor="reset-password" required>
					<Input
						id="reset-password"
						type="password"
						autoComplete="new-password"
						value={password}
						onChange={(event) => setPassword(event.target.value)}
					/>
				</Field>
				<Field label="确认新密码" htmlFor="reset-confirm" required>
					<Input
						id="reset-confirm"
						type="password"
						autoComplete="new-password"
						value={confirm}
						onChange={(event) => setConfirm(event.target.value)}
					/>
				</Field>
				{mutation.isError ? <InlineError error={mutation.error} /> : null}
				<Button type="submit" className="w-full" disabled={mutation.isPending}>
					{mutation.isPending ? <Loader2 className="animate-spin" /> : <KeyRound />}
					重置密码
				</Button>
			</form>
		</AuthCard>
	);
}

export function AdminLoginPage() {
	useDocumentTitle("管理端登录 · FloatCTF");
	const client = useClient();
	const navigate = useNavigate();
	const queryClient = useAppQueryClient();
	const setAdminToken = useAuthStore((state) => state.setAdminToken);
	const [params] = useSearchParams();
	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const next = nextTarget(params.get("next"), "/admin");

	const mutation = useMutation({
		mutationFn: (input: { username: string; password: string }) =>
			call<string>(client.admin.login(input), "管理端登录"),
		onSuccess: (token) => {
			if (!token) {
				toast.error("登录失败", "后端未返回令牌");
				return;
			}
			setAdminToken(token);
			void queryClient.invalidateQueries({ queryKey: qk.admin.dashboard() });
			toast.success("已进入控制台");
			void navigate(next, { replace: true });
		},
		onError: (error) => toast.apiError("登录失败", error),
	});

	return (
		<AuthCard
			title="管理端登录"
			description="仅管理员账号可用。管理端与选手端会话相互独立。"
			footer={
				<Link to="/login" className="text-muted-foreground underline underline-offset-4">
					前往选手端登录
				</Link>
			}
		>
			<form
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					if (username.trim() === "" || password === "") {
						toast.warning("请填写管理员账号与密码");
						return;
					}
					mutation.mutate({ username: username.trim(), password });
				}}
			>
				<Field label="管理员账号" htmlFor="admin-username" required>
					<Input
						id="admin-username"
						autoComplete="username"
						value={username}
						onChange={(event) => setUsername(event.target.value)}
					/>
				</Field>
				<Field label="密码" htmlFor="admin-password" required>
					<Input
						id="admin-password"
						type="password"
						autoComplete="current-password"
						value={password}
						onChange={(event) => setPassword(event.target.value)}
					/>
				</Field>
				{mutation.isError ? <InlineError error={mutation.error} /> : null}
				<Button type="submit" className="w-full" disabled={mutation.isPending}>
					{mutation.isPending ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
					进入控制台
				</Button>
			</form>
		</AuthCard>
	);
}

export { errorText };
