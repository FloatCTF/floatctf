import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useRef } from "react";

import { serviceApi } from "@/api";
import { useMsgInlineBanner } from "@/components";
import { ServiceRouteGuardWithRedirect } from "@/routes/service/route";

export const Route = createFileRoute("/register")({
	component: Register,
	loader: ServiceRouteGuardWithRedirect,
});

function Register() {
	useTitle("Register | FloatCTF");
	const banner = useMsgInlineBanner();
	const form = useReactive({
		username: "",
		nickname: "",
		email: "",
		password: "",
		confirmPassword: "",
		buttonMessage: "Register",
		buttonDisabled: false,
	});

	const usernameRef = useRef<HTMLInputElement>(null);
	const navigate = useNavigate();

	const mutation = useMutation({
		mutationFn: serviceApi.users.register,
		onMutate: () => {
			banner.hideBanner();
			form.buttonMessage = "Registering...";
			form.buttonDisabled = true;
		},
		onSuccess: () => {
			banner.showBanner("success", "Register success, redirecting to login");
			form.buttonMessage = "Success";

			setTimeout(() => {
				navigate({ to: "/" });
			}, 1500);
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			form.buttonMessage = "Register";
			form.buttonDisabled = false;
		},
	});

	useEffect(() => {
		usernameRef.current?.focus();
	}, []);

	// 简单邮箱校验函数
	const validateEmail = (email: string) => {
		return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
	};

	return (
		<div className="flex flex-col items-center justify-center pt-9 gap-5">
			<Avatar size={100} src="/float.png" />
			<Heading>Create your FloatCTF account</Heading>
			<form
				onSubmit={(e) => {
					e.preventDefault();
					if (
						!form.username ||
						!form.nickname ||
						!form.email ||
						!form.password ||
						!form.confirmPassword
					) {
						banner.showBanner("critical", "Please fill in all fields");
						return;
					}
					if (!validateEmail(form.email)) {
						banner.showBanner("critical", "Please enter a valid email address");
						return;
					}
					if (form.password !== form.confirmPassword) {
						banner.showBanner("critical", "Passwords do not match");
						return;
					}
					mutation.mutate({
						username: form.username,
						password: form.password,
						nickname: form.nickname,
						email: form.email,
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
						value={form.username}
						placeholder="学号"
						onChange={(e) => {
							form.username = e.target.value;
						}}
					/>
				</FormControl>

				<FormControl required>
					<FormControl.Label>Nickname</FormControl.Label>
					<TextInput
						className="w-full"
						name="nickname"
						placeholder="昵称"
						value={form.nickname}
						onChange={(e) => {
							form.nickname = e.target.value;
						}}
					/>
				</FormControl>

				<FormControl required>
					<FormControl.Label>Email</FormControl.Label>
					<TextInput
						className="w-full"
						type="email"
						name="email"
						value={form.email}
						onChange={(e) => {
							form.email = e.target.value;
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

				<FormControl required>
					<FormControl.Label>Confirm Password</FormControl.Label>
					<TextInput
						className="w-full"
						type="password"
						name="confirmPassword"
						value={form.confirmPassword}
						onChange={(e) => {
							form.confirmPassword = e.target.value;
						}}
					/>
				</FormControl>

				<Button type="submit" variant="primary" disabled={form.buttonDisabled}>
					{form.buttonMessage}
				</Button>
				<Link to="/" className="text-center">
					Already have an account? Login
				</Link>
			</form>

			<banner.BannerComponent />
		</div>
	);
}
