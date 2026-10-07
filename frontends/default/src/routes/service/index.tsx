import { createFileRoute, redirect } from "@tanstack/react-router";
import { ServiceRouteGuard } from "./route";

export const Route = createFileRoute("/service/")({
  loader: async () => {
    // 先执行权限校验
    await ServiceRouteGuard();

    // 然后强制重定向到 /service/top
    throw redirect({
      to: "/service/top",
    });
  },
});
