import { service_routes } from "@/routes";
import { NavList } from "@primer/react";
import { useLocation } from "@tanstack/react-router";
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
  return (
    <NavList {...props}>
      {routes.map((route, index) => (
        <NavList.Item
          key={`${route.path}-${index}`}
          href={route.path}
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
