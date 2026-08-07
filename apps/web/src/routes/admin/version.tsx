import { adminApi } from "@/api";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

const webVersion = import.meta.env.VITE_APP_VERSION;
export const Route = createFileRoute("/admin/version")({
	component: RouteComponent,
});

function RouteComponent() {
	const {
		data: apiVersion,
		isLoading,
		isError,
	} = useQuery({
		queryKey: ["api-version"],
		queryFn: () => adminApi.system.version(),
		// 低频数据：版本号几乎不变，5 分钟缓存
		staleTime: 5 * 60_000,
		select: (data) => data.data,
	});

	return (
		<div className="flex flex-col gap-2">
			<div className="border rounded p-4">Version</div>
			<div className="border rounded p-4">floatctf-web: {webVersion}</div>
			<div className="border rounded p-4">
				floatctf-api:{" "}
				{isLoading ? "Loading..." : isError ? "Error!" : apiVersion}
			</div>
		</div>
	);
}
