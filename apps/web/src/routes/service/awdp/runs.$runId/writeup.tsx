import { Spinner, Truncate } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { awdpRunApi } from "@/api/awdpRuns";
import { AppLink } from "@/navigation";
import { useAuthStore } from "@/stores/AuthStore";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/awdp/runs/$runId/writeup")({
	component: RouteComponent,
});

/**
 * WriteUp 标签页：与 Challenge 练习的 WriteUp 标签同款效果——左侧展示
 * 该 run 的 Writeup 卡片（作者/名称/内容摘要/时间），点击进入全局详情页
 * /service/writeups/{run_id}（run writeup 以 run_id 为 id）。
 */
function RouteComponent() {
	const { runId } = Route.useParams();
	const nickname = useAuthStore((s) => s.nickname);
	const { data, isLoading } = useQuery({
		queryKey: ["awdp-run-writeup", runId],
		queryFn: () => awdpRunApi.getWriteup(runId),
	});
	const { data: runData } = useQuery({
		queryKey: ["awdp-run", runId],
		queryFn: () => awdpRunApi.getRun(runId),
	});

	if (isLoading) {
		return <Spinner />;
	}

	const wp = data?.data;
	if (!wp?.content) {
		return <div className="text-gray-500">暂无 Writeup</div>;
	}

	const gameboxName = runData?.data?.gamebox_name ?? "Gamebox";
	const content = wp.content;
	const updatedAt = wp.updated_at;

	return (
		<div className="flex flex-col gap-2 h-full w-full overflow-y-auto">
			<div className="feed-item-content d-flex flex-column pt-2 pb-2 border color-border-default rounded-2 color-shadow-small width-full height-fit">
				<div className="repo-card d-flex rounded p-3 position-relative">
					<div className="d-flex flex-column flex-1">
						<div className="d-flex flex-items-center gap-2">
							<div
								className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
								style={{ width: 24, height: 24, fontSize: 10 }}
							>
								{nickname?.[0]?.toUpperCase() || "?"}
							</div>
							<AppLink to="/service/writeups/$id" params={{ id: runId }}>
								{nickname || "我"}/{gameboxName}
							</AppLink>
						</div>

						<div className="mt-2 text-muted">
							<Truncate title="Some example text" maxWidth="100%">
								{(() => {
									// 原始文本处理（与 Challenge Writeup 列表一致）
									const text = content
										.replace(/!\[.*?\]\(.*?\)/g, "") // 去掉图片
										.replace(/\[([^\]]+)\]\([^\)]+\)/g, "$1") // 去掉链接，保留文字
										.replace(/(`{1,3})(.*?)\1/g, "$2") // 去掉代码块/行内代码
										.replace(/[*_~>#-]+/g, "") // 去掉粗体/斜体/标题/列表符号
										.replace(/\n+/g, "\n") // 保留换行，方便取第一行
										.trim();

									// 拿第一行，且首字符不是 <
									const firstLine =
										text.split("\n").find((line) => line && line[0] !== "<") ||
										"";

									return firstLine.slice(0, 50); // 截取前 50 个字符
								})()}
							</Truncate>
						</div>
					</div>

					{/* 右下角时间 */}
					{updatedAt && (
						<div className="position-absolute bottom-2 right-3 text-xs text-muted">
							{DatetimeToShow(updatedAt)}
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
