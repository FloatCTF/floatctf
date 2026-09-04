/**
 * AWDP Run Writeup 编辑器（challenge 详情页右侧 WP 同款）。
 *
 * 一 run 一份，属主可读写（后端 GET/PUT /api/service/awdp/runs/{run_id}/writeup）。
 * 数据全部来自真实接口；保存成功用 Primer Banner 反馈，无原生弹窗。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { awdpRunApi } from "@/api/awdpRuns";
import { MDPlusEditor, useMsgBanner } from "@/components";
export function RunWriteupEditor({ runId }: { runId: string }) {
	const banner = useMsgBanner();
	const queryClient = useQueryClient();
	const [markdown, setMarkdown] = useState("");

	const writeupQuery = useQuery({
		queryKey: ["awdp-run-writeup", runId],
		queryFn: () => awdpRunApi.getWriteup(runId),
	});

	useEffect(() => {
		const content = writeupQuery.data?.data?.content;
		if (content) {
			setMarkdown(content);
		}
	}, [writeupQuery.data]);

	const saveMutation = useMutation({
		mutationFn: () => awdpRunApi.saveWriteup(runId, markdown),
		onSuccess: () => {
			banner.showBanner("success", "Writeup saved successfully");
			queryClient.invalidateQueries({
				queryKey: ["awdp-run-writeup", runId],
			});
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	return (
		<div className="flex h-full flex-col min-h-0">
			<banner.BannerComponent />
			<MDPlusEditor
				className="flex-1 min-h-0"
				value={markdown}
				setValue={(value) => setMarkdown(value)}
				onSave={() => {
					saveMutation.mutate();
				}}
			/>
		</div>
	);
}
