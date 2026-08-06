import { userServiceApi } from "@/api/service";
import { useAuthStore } from "@/stores/AuthStore";
import { Button } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";

export interface GenericHeaderProps extends React.HTMLAttributes<HTMLDivElement> {
    title: string;
    homePath: string;
    logo?: string;
    showUserInfo?: boolean;
    userProfilePath?: string;
    useProfileQuery?: boolean;
    onLogout: () => void;
}

export const GenericHeader = ({
    title,
    homePath,
    logo,
    showUserInfo = false,
    useProfileQuery = false,
    userProfilePath = "/service/profile",
    onLogout,
    ...props
}: GenericHeaderProps) => {
    const authStore = useAuthStore();
    const navigate = useNavigate();

    const { data, isLoading } = useQuery({
        queryKey: ["profile"],
        queryFn: () => userServiceApi.getMe(),
        enabled: useProfileQuery,
    });

    const profile = data?.data;

    useEffect(() => {
        if (
            useProfileQuery &&
            profile &&
            authStore.nickname !== profile.nickname
        ) {
            authStore.setNickname(profile.nickname);
        }
    }, [profile, authStore, useProfileQuery]);

    const handleGoHome = () => {
        navigate({ to: homePath });
    };

    if (useProfileQuery && isLoading) {
        return <></>;
    }

    return (
        <header
            className="bgColor-muted border-bottom d-flex flex-items-center p-2 justify-between"
            {...props}
        >
            <div
                className="d-flex flex-items-center gap-2 hover:cursor-pointer"
                onClick={handleGoHome}
                onKeyUp={(e) => e.key === "Enter" && handleGoHome()}
                role="button"
                tabIndex={0}
            >
                {logo && (
                    <img
                        src={logo}
                        alt="Logo"
                        style={{ width: 28, height: 28, borderRadius: 4 }}
                    />
                )}
                <h3>{title}</h3>
            </div>

            {showUserInfo ? (
                <div className="flex justify-center items-center gap-2">
                    <span
                        className="hover:cursor-pointer hover:underline"
                        onKeyUp={(e) => e.key === "Enter" && handleGoHome()}
                        onClick={() => navigate({ to: userProfilePath })}
                    >
                        {authStore.username}
                        {authStore.nickname && (
                            <span>:{authStore.nickname}</span>
                        )}
                    </span>
                    <Button variant="danger" onClick={onLogout}>
                        Logout
                    </Button>
                </div>
            ) : (
                <Button variant="danger" onClick={onLogout}>
                    Logout
                </Button>
            )}
        </header>
    );
};

export const ServiceHeader = () => {
    const authStore = useAuthStore();
    const navigate = useNavigate();

    return (
        <GenericHeader
            title="FloatCTF"
            homePath="/"
            logo="/float.png"
            showUserInfo
            useProfileQuery
            onLogout={() => {
                authStore.removeToken();
                navigate({ to: "/" });
            }}
        />
    );
};
export const AdminHeader = () => {
    const authStore = useAuthStore();
    const navigate = useNavigate();

    return (
        <GenericHeader
            title="FloatCTF Admin"
            homePath="/admin"
            logo="/float.png"
            showUserInfo={false}
            useProfileQuery={false}
            onLogout={() => {
                authStore.removeAdminToken();
                navigate({ to: "/admin" });
            }}
        />
    );
};
