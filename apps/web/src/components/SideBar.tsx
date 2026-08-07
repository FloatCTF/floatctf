import { service_routes } from "@/routes";
import { NavList } from "@primer/react";
import { useLocation, useNavigate } from "@tanstack/react-router";
export type NavRoute = {
  label: string;
  path?: string;
  icon: React.ReactNode;
  children?: NavRoute[];
};
export interface GenericSideBarProps
  extends React.HTMLAttributes<HTMLDivElement> {
  routes: NavRoute[];
}

export const GenericSideBar = ({ routes, ...props }: GenericSideBarProps) => {
  const location = useLocation();
  const navigate = useNavigate();

  // SPA 内部导航：拦截普通左键，避免裸 <a href> 触发整页刷新白屏。
  // 中键 / Ctrl / Cmd / Shift / Alt 点击保留浏览器默认行为（新标签打开等）。
  const handleNav =
    (path: string | undefined) =>
    (event: React.MouseEvent<HTMLElement>) => {
      if (!path) return;
      if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      )
        return;
      event.preventDefault();
      void navigate({ href: path });
    };

  return (
    <NavList {...props}>
      {routes.map((route, index) => (
        <NavList.Item
          key={`${route.path}-${index}`}
          href={route.path}
          onClick={handleNav(route.path)}
          defaultOpen={route.children?.some(
            (c) => c.path === location.pathname
          )}
          aria-current={
            location.pathname.startsWith(route.path ?? "") ? "page" : undefined
          }
        >
          <NavList.LeadingVisual>{route.icon}</NavList.LeadingVisual>
          {route.label}

          {route.children && (
            <NavList.SubNav>
              {route.children.map((child) => (
                <NavList.Item
                  key={child.path}
                  href={child.path}
                  onClick={handleNav(child.path)}
                  aria-current={
                    location.pathname === child.path ? "page" : undefined
                  }
                >
                  <NavList.LeadingVisual>{child.icon}</NavList.LeadingVisual>
                  {child.label}
                </NavList.Item>
              ))}
            </NavList.SubNav>
          )}
        </NavList.Item>
      ))}
    </NavList>
  );
};
