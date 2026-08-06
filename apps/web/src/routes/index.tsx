import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useRef } from "react";

import { serviceApi } from "@/api";
import { useMsgInlineBanner } from "@/components";
import { ServiceRouteGuardWithRedirect } from "@/routes/service/route";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/")({
	component: App,
	loader: ServiceRouteGuardWithRedirect,
});

function App() {
	useTitle("Login | FloatCTF");
	const banner = useMsgInlineBanner();

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
		mutationFn: serviceApi.users.login,
		onMutate: () => {
			banner.hideBanner();
			form.buttonMessage = "Signing in...";
			form.buttonDisabled = true;
		},
		onSuccess: (data) => {
			authStore.setToken(data.data!);
			banner.showBanner("success", "Login success");
			form.buttonMessage = "Redirecting...";
			form.buttonDisabled = true;

			// redirect to admin dashboard
			authStore.setUsername(form.username);

			navigate({ to: "/service" });
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			form.buttonDisabled = false;
			form.buttonMessage = "Sign in";
			authStore.removeToken();
		},
	});
	useEffect(() => {
		usernameRef.current?.focus();
	}, []);
	return (
		<div className="flex flex-col items-center justify-center pt-9 gap-5">
			<Avatar size={100} src="/float.png" />
			<Heading>Sign in to FloatCTF</Heading>
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
					<Link to="/register">Don't have an account? Register</Link>
					<Link to="/reset">Forgot password?</Link>
				</div>
			</form>

			<banner.BannerComponent />
		</div>
	);
}
