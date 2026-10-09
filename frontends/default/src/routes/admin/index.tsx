import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useRef } from "react";

import { adminApi } from "@/api";
import { formatApiError, useMsgInlineBanner } from "@/components";
import { AdminRouteGuardWithRedirect } from "@/routes/admin/route";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/admin/")({
  component: RouteComponent,
  loader: AdminRouteGuardWithRedirect,
});

function RouteComponent() {
  useTitle("Admin Login | FloatCTF");
  const authStore = useAuthStore();
  const navigate = useNavigate();
  const banner = useMsgInlineBanner();

  const form = useReactive({
    username: "",
    password: "",

    buttonMessage: "Sign in",
    buttonDisabled: false,
  });

  const mutation = useMutation({
    mutationFn: adminApi.login,
    onMutate: () => {
      banner.hideBanner();
      form.buttonDisabled = true;
      form.buttonMessage = "Signing in...";
    },
    onSuccess: (data) => {
      const token = data.data!;
      // v2 统一账户：一次登录同时覆盖两个域。
      // 后端 `/api/admin/session` 与 `/api/session` 是同一个会话，
      // 因此管理端登录后选手端请求不需要再登录一次（Case A：
      // 一个账号既能参赛也能管理）。
      authStore.setAdminToken(token);
      authStore.setToken(token);
      banner.showBanner("success", "Login successful");
      form.buttonDisabled = true;
      form.buttonMessage = "Redirecting...";
      // 重定向到管理端仪表盘
      navigate({ to: "/admin/dashboard" });
    },
    onError: (error) => {
      console.log(error);
      banner.showBanner(
        "critical",
        // v2 起两类失败要分开提示：
        // - 401：凭据错误（重新输入即可）
        // - 403：凭据正确但该账号没有平台管理权限（重新输入也没用）
        formatApiError(error, {
          401: "用户名或密码错误",
          403: "该账号没有平台管理权限",
        }),
      );

      form.buttonDisabled = false;
      form.buttonMessage = "Sign in";
      authStore.removeAdminToken();
    },
  });

  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    usernameRef.current?.focus();
  }, []);

  return (
    <div className="flex flex-col items-center justify-center pt-9 gap-5">
      <Avatar size={100} src="/float.png" />
      <Heading>Sign in to FloatCTF Admin</Heading>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          console.log("submit");
          if (form.username === "" || form.password === "") {
            banner.showBanner("critical", "Please fill in all fields");
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
      </form>

      <banner.BannerComponent />
    </div>
  );
}
