import { serviceApi } from "@/api";
import { useMsgInlineBanner } from "@/components";
import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { z } from "zod";
import { ServiceRouteGuardWithRedirect } from "./service/route";
const resetSearchSchema = z.object({
	token: z.string().catch(""),
});
export const Route = createFileRoute("/reset_password")({
	component: RouteComponent,
	loader: ServiceRouteGuardWithRedirect,
	validateSearch: (search) => resetSearchSchema.parse(search),
});

function RouteComponent() {
	useTitle("Reset | FloatCTF");
	const { token } = Route.useSearch();
	const banner = useMsgInlineBanner();
	const navigate = useNavigate();
	if (!token) {
		navigate({ to: "/reset" });
	}

	const form = useReactive({
		password: "",
		confirmed_password: "",
		buttonMessage: "Confirm",
		buttonDisabled: false,
	});

	const mutation = useMutation({
		mutationFn: serviceApi.users.reset,
		onMutate: () => {
			form.buttonDisabled = true;
			form.buttonMessage = "Loading...";
		},
		onSuccess: () => {
			form.buttonDisabled = false;
			form.buttonMessage = "Confirm";
			banner.showBanner("success", "reset password success");
			setTimeout(() => {
				navigate({ to: "/" });
			}, 1000);
		},
		onError: (error) => {
			banner.showBanner("critical", error.message);
			form.buttonDisabled = false;
			form.buttonMessage = "Confirm";
		},
	});

	return (
		<div className="flex flex-col items-center justify-center pt-9 gap-5">
			<Avatar size={100} src="/float.png" />
			<Heading>Set new password for your account.</Heading>

			<form
				onSubmit={(e) => {
					e.preventDefault();
					if (form.password === "" || form.confirmed_password === "") {
						banner.showBanner("critical", "password is empty");
						return;
					}

					if (form.password !== form.confirmed_password) {
						banner.showBanner("critical", "password not match");
						return;
					}
					mutation.mutate({
						token: token,
						password: form.password,
						confirmed_password: form.confirmed_password,
					});
				}}
				className="flex flex-col gap-2 w-full max-w-sm"
			>
				<FormControl>
					<FormControl.Label>Password</FormControl.Label>
					<TextInput
						className="w-full"
						type="password"
						name="password"
						placeholder="password"
						value={form.password}
						onChange={(e) => {
							form.password = e.target.value;
						}}
					/>
				</FormControl>

				<FormControl>
					<FormControl.Label> Confirmed Password</FormControl.Label>
					<TextInput
						className="w-full"
						type="password"
						name="confirmed_password"
						placeholder="password"
						value={form.confirmed_password}
						onChange={(e) => {
							form.confirmed_password = e.target.value;
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
