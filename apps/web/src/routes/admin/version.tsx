import { adminApi } from "@/api";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";

const version = import.meta.env.VITE_APP_VERSION;
export const Route = createFileRoute("/admin/version")({
    component: RouteComponent,
});

function RouteComponent() {
    const {
        data: changelog,
        isLoading,
        isError,
    } = useQuery({
        queryKey: ["changelog"],
        queryFn: () => adminApi.system.changelog(),
        select: (data) => data.data,
    });

    if (isLoading) return <div>Loading...</div>;
    if (isError) return <div>Error!</div>;

    return (
        <div className="flex flex-col gap-2">
            <div className="border rounded p-4">Version</div>
            <div className="border rounded p-4">floatctf-web: {version}</div>
            <MDEditor.Markdown
                source={changelog}
                className="border rounded p-4"
            />
        </div>
    );
}
