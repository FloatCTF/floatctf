import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";
import { UnderlineNav } from "@primer/react";
import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/admin/docker")({
  component: RouteComponent,
});

function RouteComponent() {
  return (
    <div>
      <UnderlineNav aria-label="Repository">
        <RouterNavItem to="/admin/docker/">Containers</RouterNavItem>
        <RouterNavItem to="/admin/docker/images">Images</RouterNavItem>
        <RouterNavItem to="/admin/docker/networks">Networks</RouterNavItem>
      </UnderlineNav>
      <Outlet />
    </div>
  );
}
/**
 * docker id
 *  容器名称
 *  题目名称
 *  比赛名称
 *  IP 地址和 端口
 *  状态
 * 镜像名称
 *  运行时长
 *
 */

/**
 * 镜像名称
 * image_tag
 * image_size
 * image_date
 */

/**
 * network_name
 * mode
 * subnet
 * gateway
 * time
 *  标签
 */
