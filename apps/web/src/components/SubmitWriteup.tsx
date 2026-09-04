import { Button } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";

import { serviceApi } from "@/api";

/**
 * Writeup 提交/下载组件（服务端通用：Jeopardy 与 AWD 赛事共用）。
 * - 只允许上传 PDF；重复上传会覆盖旧文件
 * - 已上传时显示下载链接（GET /api/events/{id}/own_wp）
 */
export function SubmitWriteup({
	eventId,
	teamId,
}: {
	eventId: string;
	teamId?: string;
}) {
	const inputRef = useRef<HTMLInputElement>(null);
	const [file, setFile] = useState<File | null>(null);
	const [message, setMessage] = useState<null | {
		type: "success" | "error";
		text: string;
	}>(null);
	const queryClient = useQueryClient();
	const { data: wpUrl } = useQuery({
		queryKey: ["own_wp", eventId],
		queryFn: () => serviceApi.events.getOwnWp(eventId),
	});

	const submitMutation = useMutation({
		mutationFn: (file: File) =>
			serviceApi.submit.submitWriteup(file, eventId, teamId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["Challenges"] });
			setMessage({ type: "success", text: "提交成功 🎉" });
			setFile(null);
			setTimeout(() => setMessage(null), 3000);
		},
		onError: (e) => {
			setMessage({ type: "error", text: "提交失败，请重试" });
			console.error("submit writeup error", e);
			setTimeout(() => setMessage(null), 3000);
		},
	});

	const handleClick = () => inputRef.current?.click();

	const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		const selected = e.target.files?.[0];
		if (!selected) return;

		// 只允许 pdf
		if (!selected.name.toLowerCase().endsWith(".pdf")) {
			setMessage({ type: "error", text: "只支持 pdf 文件" });
			setTimeout(() => setMessage(null), 3000);
			return;
		}

		setFile(selected);
		e.target.value = ""; // 允许连续选择同一个文件
	};

	const handleUpload = () => {
		if (!file) return;
		submitMutation.mutate(file);
	};

	return (
		<div className="flex flex-col justify-center gap-3">
			{message && (
				<span
					className={`ml-2 text-sm ${
						message.type === "success" ? "text-green-600" : "text-red-500"
					}`}
				>
					{message.text}
				</span>
			)}

			{file && (
				<div className="flex items-center gap-3">
					<span className="text-sm text-gray-500">{file.name}</span>
					<Button
						onClick={handleUpload}
						disabled={submitMutation.isPending}
						variant="primary"
					>
						{submitMutation.isPending ? "Submitting..." : "Submit WP"}
					</Button>
				</div>
			)}

			<div>
				<Button onClick={handleClick}>Upload Writeup *.pdf</Button>
				<p>Upload again to override the file</p>
				{wpUrl?.data && (
					<a
						href={wpUrl.data}
						target="_blank"
						rel="noopener noreferrer"
						className="text-blue-600 hover:underline"
					>
						Download Writeup
					</a>
				)}
				<input
					type="file"
					accept=".pdf"
					ref={inputRef}
					className="hidden"
					onChange={handleChange}
				/>
			</div>
		</div>
	);
}
