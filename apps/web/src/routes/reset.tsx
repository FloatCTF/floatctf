import { serviceApi } from "@/api";
import { useMsgInlineBanner } from "@/components";
import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useRef } from "react";
import { ServiceRouteGuardWithRedirect } from "./service/route";

export const Route = createFileRoute("/reset")({
	component: RouteComponent,
	loader: ServiceRouteGuardWithRedirect,
});
export type ResetPasswordRequest = {
	email?: string;
	username?: string;
};
function RouteComponent() {
	useTitle("Reset | FloatCTF");
	const banner = useMsgInlineBanner();
	const form = useReactive({
		username: "",
		email: "",
		buttonMessage: "Rest it",
		buttonDisabled: false,
	});
	const usernameRef = useRef<HTMLInputElement>(null);
	const mutation = useMutation({
		mutationFn: serviceApi.users.resetPassword,
		onMutate: () => {
			form.buttonDisabled = true;
			form.buttonMessage = "Sending...";
		},
		onSuccess: () => {
			banner.showBanner(
				"success",
				"You will receive an email with a link to reset your password.",
			);
			form.buttonDisabled = false;
			form.buttonMessage = "Reset it";
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			form.buttonDisabled = false;
			form.buttonMessage = "Reset it";
		},
	});
	return (
		<div className="flex flex-col items-center justify-center pt-9 gap-5">
			<Avatar size={100} src="/float.png" />
			<Heading>Reset your account</Heading>
			<p>Chose one option, provide your username or email.</p>
			<form
				onSubmit={(e) => {
					e.preventDefault();
					if (form.username === "" && form.email === "") {
						banner.showBanner("critical", "Username or Email is empty");
						return;
					}
					mutation.mutate({
						username: form.username,
						email: form.email,
					});
				}}
				className="flex flex-col gap-2 w-full max-w-sm"
			>
				<FormControl>
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

				<FormControl>
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

				<Button type="submit" variant="primary" disabled={form.buttonDisabled}>
					{form.buttonMessage}
				</Button>
			</form>

			<banner.BannerComponent />
		</div>
	);
}
