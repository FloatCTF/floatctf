import { Avatar, Button, FormControl, Heading, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";
import { useEffect, useRef } from "react";

import { adminApi, serviceApi } from "@/api";
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
    // v2：`/api/session` 是唯一登录入口，它只回答「你是谁」，**不**回答
    // 「你能不能进管理端」（认证与授权分离）。因此管理入口必须自己验证能力 ——
    // 否则任何玩家都能从这个页面进入管理外壳，然后处处吃 403。
    mutationFn: async ({
      username,
      password,
    }: {
      username: string;
      password: string;
    }) => {
      const login = await adminApi.login({ username, password });
      const token = login.data!;
      // 先落 token：SDK 的 getUserToken 从 store 读，`/me` 必须带上它。
      authStore.setToken(token);
      authStore.setAdminToken(token);
      const me = await serviceApi.users.getMe();
      const account = me.data;
      // 「有平台管理能力」= 持有至少一条权限（`grants_all` 蕴含全部权限）。
      // 判定依据来自后端 /me，不在前端自造名单。
      const canAdmin =
        Boolean(account?.grants_all) || (account?.permissions?.length ?? 0) > 0;
      if (!canAdmin) {
        authStore.removeToken();
        authStore.removeAdminToken();
        throw new Error("该账号没有平台管理权限");
      }
      return token;
    },
    onMutate: () => {
      banner.hideBanner();
      form.buttonDisabled = true;
      form.buttonMessage = "Signing in...";
    },
    onSuccess: () => {
      // 两个 token 已在 mutationFn 里落好（`/me` 需要它）；这里只负责跳转。
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
        }) ||
          (error instanceof Error && error.message.includes("管理权限")
            ? "该账号没有平台管理权限"
            : "登录失败"),
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
