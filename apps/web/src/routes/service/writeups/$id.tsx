import { Avatar, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import type { UnifiedWriteupDetail } from "@/api/service/challenges";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/writeups/$id")({
	component: RouteComponent,
});

/** 单个 Writeup 详情（challenge + gamebox 统一渲染）。 */
function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading, isError } = useQuery({
		queryKey: ["writeup", id],
		queryFn: () => serviceApi.challenges.getWriteup(id),
	});
	const w: UnifiedWriteupDetail | undefined = data?.data;
	useTitle(`${w?.content_name ?? "Writeup"} | FloatCTF`);
	if (isLoading) {
		return <Spinner />;
	}

	if (isError || !w) {
		return <div className="text-red-500">加载失败，请稍后重试</div>;
	}

	return (
		<div className="h-full">
			<div className="flex flex-col pt-3 px-8 gap-2">
				<h2 className="flex justify-between items-center">
					{w.writeup_type === "gamebox" ? (
						<AppLink
							className="hover:underline"
							to="/service/awdp/runs/$runId"
							params={{ runId: w.id }}
						>
							GameBox / {w.content_name}
						</AppLink>
					) : (
						<AppLink
							className="hover:underline"
							to="/service/challenges/$id"
							params={{ id: w.content_id }}
						>
							{w.category} / {w.content_name}
						</AppLink>
					)}
					<span className="flex items-center gap-2">
						{w.avatar ? (
							<Avatar src={w.avatar} size={24} />
						) : (
							<div
								className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
								style={{ width: 24, height: 24, fontSize: 10 }}
							>
								{w.nickname?.[0]?.toUpperCase() || "?"}
							</div>
						)}
						{w.nickname}
					</span>
				</h2>

				<div className="flex justify-between">
					<span>
						Created at{" "}
						<span className="text-bold">{DatetimeToShow(w.created_at)}</span>
					</span>
					<div>
						<span className="text-bold">{w.email}</span>
					</div>
				</div>

				<div className="border-top mb-3" />

				<MDEditor.Markdown source={w.content} />
			</div>
		</div>
	);
}
