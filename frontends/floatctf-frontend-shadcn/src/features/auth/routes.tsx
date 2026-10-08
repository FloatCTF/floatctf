import type { FeatureModule } from "~/features/types";
import { RedirectIfAuthenticated } from "~/app/guards";

import {
	AdminLoginPage,
	ForgotPasswordPage,
	LoginPage,
	RegisterPage,
	ResetPasswordPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "public",
	routes: [
		{
			path: "/login",
			element: (
				<RedirectIfAuthenticated scope="user" to="/">
					<LoginPage />
				</RedirectIfAuthenticated>
			),
		},
		{
			path: "/register",
			element: (
				<RedirectIfAuthenticated scope="user" to="/">
					<RegisterPage />
				</RedirectIfAuthenticated>
			),
		},
		{ path: "/forgot", element: <ForgotPasswordPage /> },
		{ path: "/reset", element: <ResetPasswordPage /> },
		{
			path: "/admin/login",
			element: (
				<RedirectIfAuthenticated scope="admin" to="/admin">
					<AdminLoginPage />
				</RedirectIfAuthenticated>
			),
		},
	],
};
