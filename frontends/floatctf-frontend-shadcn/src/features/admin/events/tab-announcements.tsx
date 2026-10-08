/**
 * 赛事控制台 · 公告标签（event_announcements）。
 *
 * SDK 的四个方法**全部柯里化**（第一层 `eventId`）：
 * `fetch(eventId)({page,limit})`、`create(eventId)(body)`、`patch(eventId)(announcement)`、
 * `remove(eventId)(id_list)`；其中 `patch` 的 URL 用 `announcement.id` 拼接，因此**必须带 id**。
 */

import { useId, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Megaphone, MoreHorizontal, PencilLine, Plus, Trash2 } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import type { DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MarkdownEditor, MarkdownView } from "~/components/app/markdown";
import { MonoText, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock, InlineError } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import type { EventAnnouncements } from "@floatctf/sdk/entity";
import { formatDateTime } from "~/lib/format";

import { clientPageCount, clientPaged, MobileFacts, PagedTable } from "./shared";

export function EventAnnouncementsTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(10);
	const [editing, setEditing] = useState<{ announcement: EventAnnouncements | null } | null>(null);
	const [preview, setPreview] = useState<EventAnnouncements | null>(null);

	const query = useQuery({
		queryKey: qk.admin.eventAnnouncements(eventId),
		// 该 query key 不含分页参数（`qk.admin.eventAnnouncements(eventId)`）→ 一次取全量，
		// 由 `PagedTable` + `clientPaged` 做客户端分页。
		queryFn: () =>
			callList<EventAnnouncements>(client.admin.event_announcements.fetch(eventId)()),
	});

	const total = query.data?.meta?.total ?? query.data?.items.length ?? 0;
	const safePage = Math.min(page, clientPageCount(total, pageSize));
	const pagedQuery = clientPaged(query, safePage, pageSize);

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventAnnouncements(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventLogs(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.announcements.all });
	}

	const remove = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.event_announcements.remove(eventId)(ids), "删除公告"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 条公告`);
			invalidate();
		},
		onError: (error) => toast.apiError("删除公告失败", error),
	});

	async function confirmRemove(row: EventAnnouncements) {
		const ok = await confirm({
			title: `删除公告「${row.title}」？`,
			description: "赛事公告删除后不可恢复。",
			consequences: [
				"公告从赛事页与选手端立即消失",
				"公告正文不会被备份，删除后无法找回（需要重新发布）",
			],
			tone: "danger",
			confirmText: "删除公告",
		});
		if (!ok) return;
		remove.mutate([row.id]);
	}

	const columns: DataTableColumn<EventAnnouncements>[] = [
		{
			id: "title",
			header: "标题",
			cell: (row) => <span className="font-medium">{row.title}</span>,
			sortValue: (row) => row.title,
		},
		{
			id: "content",
			header: "内容",
			cell: (row) => (
				<button
					type="button"
					className="line-clamp-2 max-w-md text-left text-sm text-muted-foreground underline-offset-4 hover:underline"
					onClick={() => setPreview(row)}
				>
					{row.content}
				</button>
			),
		},
		{
			id: "created_at",
			header: "发布时间",
			cell: (row) => <MonoText>{formatDateTime(row.created_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.created_at,
			hideBelow: "md",
		},
	];

	const rowActions = (row: EventAnnouncements): ReactNode => (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={`${row.title} 的操作`}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuLabel className="max-w-56 truncate">{row.title}</DropdownMenuLabel>
				<DropdownMenuItem onSelect={() => setPreview(row)}>查看</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => setEditing({ announcement: row })}>
					<PencilLine /> 编辑
				</DropdownMenuItem>
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemove(row)}>
					<Trash2 /> 删除
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);

	const mobileCard = (row: EventAnnouncements): ReactNode => (
		<MobileFacts
			items={[
				{ k: "标题", v: <span className="font-medium">{row.title}</span> },
				{ k: "发布时间", v: <MonoText>{formatDateTime(row.created_at)}</MonoText> },
				{ k: "内容", v: <span className="line-clamp-3 text-xs">{row.content}</span> },
			]}
		/>
	);

	return (
		<SectionCard
			title="赛事公告"
			description="公告发布后选手端赛事页立即可见；支持 Markdown。"
			actions={
				<Toolbar>
					<Button size="sm" onClick={() => setEditing({ announcement: null })}>
						<Plus /> 发布公告
					</Button>
				</Toolbar>
			}
		>
			<PagedTable
				query={pagedQuery}
				columns={columns}
				getRowId={(row) => row.id}
				page={safePage}
				pageSize={pageSize}
				total={total}
				onPageChange={setPage}
				onPageSizeChange={(size) => {
					setPageSize(size);
					setPage(1);
				}}
				rowActions={rowActions}
				mobileCard={mobileCard}
				errorTitle="加载赛事公告失败"
				empty={
					<EmptyBlock
						title="还没有公告"
						description="发布第一条公告，向选手同步赛程与规则变更。"
						icon={<Megaphone className="size-5" />}
						action={
							<Button size="sm" onClick={() => setEditing({ announcement: null })}>
								<Plus /> 发布公告
							</Button>
						}
					/>
				}
			/>

			{editing ? (
				<AnnouncementSheet
					eventId={eventId}
					announcement={editing.announcement}
					onClose={() => setEditing(null)}
					onDone={invalidate}
				/>
			) : null}

			{preview ? (
				<FormSheet
					open
					onOpenChange={(next) => {
						if (!next) setPreview(null);
					}}
					width="lg"
					title={preview.title}
					description={`发布于 ${formatDateTime(preview.created_at, { seconds: true })}`}
				>
					<MarkdownView>{preview.content}</MarkdownView>
				</FormSheet>
			) : null}
		</SectionCard>
	);
}

/* ── 新建 / 编辑公告 ────────────────────────────────────────────────────── */

function AnnouncementSheet({
	eventId,
	announcement,
	onClose,
	onDone,
}: {
	eventId: string;
	announcement: EventAnnouncements | null;
	onClose: () => void;
	onDone: () => void;
}): ReactNode {
	const client = useClient();
	const formId = useId();
	const [title, setTitle] = useState(announcement?.title ?? "");
	const [content, setContent] = useState(announcement?.content ?? "");
	const [error, setError] = useState<string | null>(null);

	const save = useMutation({
		mutationFn: () => {
			if (announcement === null) {
				return call(
					client.admin.event_announcements.create(eventId)({
						title: title.trim(),
						content,
					}),
					"发布公告",
				);
			}
			// `patch` 用 `announcement.id` 拼 URL —— 必须带 id。
			return call(
				client.admin.event_announcements.patch(eventId)({
					id: announcement.id,
					title: title.trim(),
					content,
				}),
				"保存公告",
			);
		},
		onSuccess: () => {
			toast.success(announcement === null ? "公告已发布" : "公告已更新");
			onDone();
			onClose();
		},
		onError: (mutationError) => toast.apiError("保存公告失败", mutationError),
	});

	return (
		<FormSheet
			open
			onOpenChange={(next) => {
				if (!next) onClose();
			}}
			width="lg"
			title={announcement === null ? "发布赛事公告" : "编辑赛事公告"}
			description="发布后选手端赛事页立即可见。"
			footer={
				<FormFooter
					formId={formId}
					onCancel={onClose}
					isPending={save.isPending}
					submitLabel={announcement === null ? "发布" : "保存"}
				/>
			}
		>
			<form
				id={formId}
				className="space-y-4"
				onSubmit={(submitEvent) => {
					submitEvent.preventDefault();
					if (title.trim() === "") {
						setError("公告标题不能为空");
						return;
					}
					setError(null);
					save.mutate();
				}}
			>
				<Field label="标题" htmlFor={`${formId}-title`} required error={error}>
					<Input
						id={`${formId}-title`}
						value={title}
						maxLength={200}
						onChange={(changeEvent) => setTitle(changeEvent.target.value)}
						placeholder="例如：比赛时间调整通知"
					/>
				</Field>
				<Field label="内容" hint="支持 Markdown（GFM）。图片上传在管理端不可用，请使用外链。">
					<MarkdownEditor value={content} onChange={setContent} allowUpload={false} minHeight={240} />
				</Field>
				{save.isError ? <InlineError error={save.error} /> : null}
			</form>
		</FormSheet>
	);
}
