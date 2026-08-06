import type { NavRoute } from "@/components/SideBar";
import {
    ContainerIcon,
    DatabaseIcon,
    FlameIcon,
    GearIcon,
    GiftIcon,
    GoalIcon,
    ListUnorderedIcon,
    LogIcon,
    MegaphoneIcon,
    NoteIcon,
    PasskeyFillIcon,
    PersonIcon,
    ShieldLockIcon,
    TasklistIcon,
    TelescopeIcon,
    TerminalIcon,
    ClockIcon,
    ZapIcon,
    CommentDiscussionIcon,
} from "@primer/octicons-react";

export const admin_routes: NavRoute[] = [
    { label: "Dashboard", path: "/admin/dashboard", icon: <GoalIcon /> },
    {
        label: "Super Admins",
        path: "/admin/super_admins",
        icon: <ShieldLockIcon />,
    },
    {
        label: "Users",
        path: "/admin/users",
        icon: <PasskeyFillIcon />,
    },

    {
        label: "Challenges",
        path: "/admin/challenges",
        icon: <ListUnorderedIcon />,
    },
    {
        label: "Challenge Sets",
        path: "/admin/challenge_sets",
        icon: <TasklistIcon />,
    },

    {
        label: "Instances",
        path: "/admin/instances",
        icon: <ListUnorderedIcon />,
    },

    {
        label: "Events",
        path: "/admin/events",
        icon: <ListUnorderedIcon />,
    },
    {
        label: "Announcements",
        path: "/admin/announcements",
        icon: <MegaphoneIcon />,
    },

    {
        label: "Discussions",
        path: "/admin/discussions",
        icon: <CommentDiscussionIcon />,
    },
    { label: "Weapons", path: "/admin/weapons", icon: <GiftIcon /> },
    {
        label: "Terminal",
        path: "/admin/terminal",
        icon: <TerminalIcon />,
    },
    {
        label: "Docker",
        path: "/admin/docker",
        icon: <ContainerIcon />,
    },
    {
        label: "Database",
        path: "/admin/database",
        icon: <DatabaseIcon />,
    },
    {
        label: "Logs",
        path: "/admin/logs",
        icon: <LogIcon />,
    },
    {
        label: "Scheduled Tasks",
        path: "/admin/scheduled_tasks",
        icon: <ClockIcon />,
    },
    {
        label: "Settings",
        path: "/admin/settings",
        icon: <GearIcon />,
    },
    {
        label: "Version",
        path: "/admin/version",
        icon: <ZapIcon />,
    },
];

export const service_routes: NavRoute[] = [
    { label: "Top Users", path: "/service/top", icon: <GoalIcon size={18} /> },
    {
        label: "Announcements",
        path: "/service/announcements",
        icon: <MegaphoneIcon size={18} />,
    },
    {
        label: "Events",
        path: "/service/events",
        icon: <TelescopeIcon size={18} />,
    },
    {
        label: "Challenges",
        path: "/service/challenges",
        icon: <ListUnorderedIcon size={18} />,
    },
    {
        label: "Challenge Sets",
        path: "/service/challenge_sets",
        icon: <TasklistIcon size={18} />,
    },
    {
        label: "Instances",
        path: "/service/instances",
        icon: <FlameIcon size={18} />,
    },
    {
        label: "Discussions",
        path: "/service/discussions",
        icon: <CommentDiscussionIcon size={18} />,
    },
    {
        label: "Writeups",
        path: "/service/writeups",
        icon: <NoteIcon size={18} />,
    },
    {
        label: "Solves",
        path: "/service/solves",
        icon: <LogIcon size={18} />,
    },
    {
        label: "Weapons",
        path: "/service/weapons",
        icon: <GiftIcon size={18} />,
    },

    {
        label: "Profile",
        path: "/service/profile",
        icon: <PersonIcon size={18} />,
    },
];

export const admin_ignore_routes: string[] = ["/admin", "/admin/"];
