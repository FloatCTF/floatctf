/**
 * Markdown 渲染与编辑。
 *
 * 安全口径：**不启用 `rehype-raw`** —— 原始 HTML 一律当文本处理，因此讨论 / writeup /
 * 题目描述里的 `<script>` 之类不会被当作 HTML 执行。URL 只允许 http(s)/mailto/相对路径
 * 与 `data:image/*`，`javascript:` 一律拒绝。链接强制 `rel="noopener noreferrer"`。
 */

import { useState, type ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ImagePlus, Loader2 } from "lucide-react";

import { uploadFile } from "~/api/call";
import { errorText } from "~/api/errors";
import { useClient } from "~/api/client";
import { Button } from "~/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { Textarea } from "~/components/ui/textarea";
import { toast } from "~/components/app/toast";
import { cn } from "~/lib/utils";

function safeUrlTransform(url: string): string {
	const trimmed = url.trim();
	if (trimmed.length === 0) return "";
	const lowered = trimmed.toLowerCase();
	if (
		lowered.startsWith("http://") ||
		lowered.startsWith("https://") ||
		lowered.startsWith("mailto:") ||
		lowered.startsWith("/") ||
		lowered.startsWith("#") ||
		lowered.startsWith("./") ||
		lowered.startsWith("../")
	) {
		return trimmed;
	}
	if (lowered.startsWith("data:image/")) return trimmed;
	// 未知协议（含 javascript:、data:text/html）一律丢弃。
	return "";
}

export function MarkdownView({
	children,
	className,
	emptyText = "（无内容）",
}: {
	children: string | null | undefined;
	className?: string;
	emptyText?: string;
}) {
	const content = (children ?? "").trim();
	if (content.length === 0) {
		return <p className="text-sm text-muted-foreground">{emptyText}</p>;
	}
	return (
		<div className={cn("prose-ctf max-w-none", className)}>
			<Markdown
				remarkPlugins={[remarkGfm]}
				urlTransform={safeUrlTransform}
				components={{
					a: ({ children: linkChildren, ...props }) => (
						<a {...props} target="_blank" rel="noopener noreferrer">
							{linkChildren}
						</a>
					),
					img: ({ alt, ...props }) => (
						<img {...props} alt={alt ?? ""} loading="lazy" className="max-h-96 rounded-md border" />
					),
					table: ({ children: tableChildren }) => (
						<div className="overflow-x-auto">
							<table>{tableChildren}</table>
						</div>
					),
				}}
			>
				{content}
			</Markdown>
		</div>
	);
}

export interface MarkdownEditorProps {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	minHeight?: number;
	disabled?: boolean;
	/** 允许上传插图（默认 true；需要选手 token）。 */
	allowUpload?: boolean;
}

export function MarkdownEditor({
	value,
	onChange,
	placeholder = "支持 Markdown（GFM）。",
	minHeight = 260,
	disabled = false,
	allowUpload = true,
}: MarkdownEditorProps): ReactNode {
	const client = useClient();
	const [tab, setTab] = useState("write");
	const [uploading, setUploading] = useState(false);

	async function handleFile(file: File) {
		setUploading(true);
		try {
			// 逃生舱之外的常规门面：`client.service.uploads.upload_image` 返回图片 URL。
			// 这里用带进度的公共逃生舱（axios 级能力）以便显示上传进度。
			const url = await uploadFile<string>({
				client,
				url: "/uploads/image",
				field: "image_file",
				file,
			});
			if (url) {
				onChange(`${value}${value.endsWith("\n") || value.length === 0 ? "" : "\n"}![${file.name}](${url})\n`);
				toast.success("插图已上传");
			} else {
				toast.error("插图上传失败", "后端未返回图片地址");
			}
		} catch (error) {
			toast.apiError("插图上传失败", error);
		} finally {
			setUploading(false);
		}
	}

	return (
		<Tabs value={tab} onValueChange={setTab} className="w-full">
			<div className="flex items-center justify-between gap-2">
				<TabsList>
					<TabsTrigger value="write">编辑</TabsTrigger>
					<TabsTrigger value="preview">预览</TabsTrigger>
				</TabsList>
				{allowUpload ? (
					<label className="inline-flex">
						<input
							type="file"
							accept="image/*"
							className="hidden"
							disabled={disabled || uploading}
							onChange={(event) => {
								const file = event.target.files?.[0];
								event.target.value = "";
								if (file) void handleFile(file);
							}}
						/>
						<Button variant="outline" size="sm" asChild disabled={disabled || uploading}>
							<span>
								{uploading ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<ImagePlus className="size-4" />
								)}
								插入图片
							</span>
						</Button>
					</label>
				) : null}
			</div>
			<TabsContent value="write" className="mt-3">
				<Textarea
					value={value}
					disabled={disabled}
					placeholder={placeholder}
					style={{ minHeight }}
					onChange={(event) => onChange(event.target.value)}
					className="font-mono text-sm"
				/>
			</TabsContent>
			<TabsContent value="preview" className="mt-3">
				<div
					className="overflow-auto rounded-md border bg-muted/20 px-3 py-2"
					style={{ minHeight }}
				>
					<MarkdownView>{value}</MarkdownView>
				</div>
			</TabsContent>
		</Tabs>
	);
}

/** 只读错误兜底：`errorText` 已被 `toast` 使用；这里导出便于页面直接显示。 */
export { errorText };
