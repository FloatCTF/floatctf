/**
 * 题目工作台 —— 练习题库（`/challenges/:id`）与赛事内题目（`/events/:id/challenges/:cid`）
 * **共用同一套真实解题流程**：
 *
 * 1. 读取该题当前用户的实例（可能没有 → 404 当「无实例」处理，不是错误）；
 * 2. 启动实例（静态题也会创建一个「无容器」的答题实例，实例上带着该题的 flag 比对值）；
 * 3. 提交 flag（比对的是**实例**的 flag；成功后后端会自动销毁实例）；
 * 4. 可选：写自己的题解（`getMyWriteup` / `createMyWriteup`）与查看他人题解。
 *
 * 注意：`InstancesDto.flag` 由后端恒定脱敏为空 —— 绝不把空值当 flag 展示或兜底。
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	CheckCircle2,
	Download,
	ExternalLink,
	FileText,
	Paperclip,
	RotateCw,
	Send,
	Trash2,
} from "lucide-react";

import { call, callMaybe, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { MarkdownEditor, MarkdownView } from "~/components/app/markdown";
import {
	CopyText,
	KeyValueList,
	MonoText,
	ReadonlyBlock,
	SectionCard,
} from "~/components/app/page";
import { EmptyBlock, ErrorBlock, InlineError, LoadingBlock } from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import { Separator } from "~/components/ui/separator";
import type { ChallengesListItem, Instances } from "@floatctf/sdk";
import { isNotFound } from "~/api/errors";
import { formatBytes, formatDateTime, formatRelative } from "~/lib/format";
import { useNow } from "~/lib/hooks";

export type WorkbenchMode = { kind: "practice" } | { kind: "event"; eventId: string };

export interface ChallengeWorkbenchProps {
	challenge: ChallengesListItem;
	mode: WorkbenchMode;
}

/** 后端把实例入口写成一条 `<a href=...>`（见 instance_service.rs）；这里只取 URL，不注入 HTML。 */
function extractInstanceUrl(content: string | null | undefined): string | null {
	if (!content) return null;
	const href = /href="([^"]+)"/.exec(content);
	if (href) return href[1];
	const url = /https?:\/\/[^\s<>"']+/.exec(content);
	return url ? url[0] : null;
}

function instanceStatusTone(status: string | undefined): "success" | "warning" | "danger" | "muted" {
	switch ((status ?? "").toLowerCase()) {
		case "running":
			return "success";
		case "pending":
		case "starting":
			return "warning";
		case "failed":
		case "error":
			return "danger";
		default:
			return "muted";
	}
}

export function ChallengeWorkbench({ challenge, mode }: ChallengeWorkbenchProps) {
	const client = useClient();
	const queryClient = useQueryClient();
	const now = useNow(30_000);
	const [flag, setFlag] = useState("");
	const [writeupOpen, setWriteupOpen] = useState(false);

	const eventId = mode.kind === "event" ? mode.eventId : null;

	const instanceQuery = useQuery({
		queryKey: eventId
			? qk.events.challengeInstance(eventId, challenge.id)
			: qk.challenges.instance(challenge.id),
		queryFn: async (): Promise<Instances | null> => {
			try {
				return eventId
					? await callMaybe(client.service.events.getChallengeInstance(eventId, challenge.id))
					: await callMaybe(client.service.challenges.getInstance(challenge.id));
			} catch (error) {
				// 「没有实例」是正常状态（后端 404），不是页面错误。
				if (isNotFound(error)) return null;
				throw error;
			}
		},
		retry: false,
	});

	const invalidateAfterInstanceChange = () => {
		void queryClient.invalidateQueries({
			queryKey: eventId
				? qk.events.challengeInstance(eventId, challenge.id)
				: qk.challenges.instance(challenge.id),
		});
		void queryClient.invalidateQueries({ queryKey: qk.instances.all });
		if (eventId) {
			void queryClient.invalidateQueries({ queryKey: qk.events.challenges(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.events.instances(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.events.detail(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.events.scoreboard(eventId) });
		} else {
			void queryClient.invalidateQueries({ queryKey: qk.challenges.detail(challenge.id) });
			void queryClient.invalidateQueries({ queryKey: qk.challenges.all });
		}
	};

	const launch = useMutation({
		mutationFn: () =>
			eventId
				? call<Instances>(client.service.events.launchSingleInstance(eventId, challenge.id), "启动实例")
				: call<Instances>(client.service.instances.launch(challenge.id), "启动实例"),
		onSuccess: () => {
			toast.success("实例已启动");
			invalidateAfterInstanceChange();
		},
		onError: (error) => toast.apiError("启动实例失败", error),
	});

	const destroy = useMutation({
		mutationFn: (instanceId: string) =>
			callVoid(client.service.instances.destroy(instanceId), "销毁实例"),
		onSuccess: () => {
			toast.success("实例已销毁");
			invalidateAfterInstanceChange();
		},
		onError: (error) => toast.apiError("销毁实例失败", error),
	});

	const submit = useMutation({
		mutationFn: (input: { instanceId: string; flag: string }) =>
			eventId
				? call(
						client.service.submit.submitSingle({
							event_id: eventId,
							instance_id: input.instanceId,
							flag: input.flag,
						}),
						"提交 flag",
					)
				: call(
						client.service.submit.submit({
							instance_id: input.instanceId,
							flag: input.flag,
						}),
						"提交 flag",
					),
		onSuccess: () => {
			setFlag("");
			toast.success("Flag 正确，已计分", "实例已被平台自动回收。");
			invalidateAfterInstanceChange();
			void queryClient.invalidateQueries({ queryKey: qk.solves.all });
			void queryClient.invalidateQueries({ queryKey: qk.auth.me() });
		},
		onError: (error) => toast.apiError("提交失败", error),
	});

	const instance = instanceQuery.data ?? null;
	const instanceUrl = extractInstanceUrl(instance?.content);

	return (
		<div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_380px]">
			<div className="min-w-0 space-y-5">
				<SectionCard
					title="题目描述"
					description={
						<div className="flex flex-wrap items-center gap-2">
							<TonePill tone="neutral" icon={<FileText />}>
								{challenge.category || "未分类"}
							</TonePill>
							{challenge.version ? (
								<TonePill tone="info">版本 {challenge.version}</TonePill>
							) : null}
							{challenge.solved ? (
								<TonePill tone="success" icon={<CheckCircle2 />}>
									已解出
								</TonePill>
							) : null}
						</div>
					}
				>
					<MarkdownView>{challenge.description}</MarkdownView>

					{challenge.attachment ? (
						<>
							<Separator className="my-4" />
							<div className="flex flex-wrap items-center justify-between gap-2 rounded-md border bg-muted/30 px-3 py-2">
								<div className="flex min-w-0 items-center gap-2">
									<Paperclip className="size-4 text-muted-foreground" />
									<span className="truncate text-sm">{challenge.attachment.name}</span>
									{challenge.attachment.size ? (
										<span className="tnum text-xs text-muted-foreground">
											{formatBytes(challenge.attachment.size)}
										</span>
									) : null}
								</div>
								<Button variant="outline" size="sm" asChild>
									<a
										href={`/static/challenges/${challenge.safe_name}/${challenge.attachment.path}`}
										target="_blank"
										rel="noopener noreferrer"
									>
										<Download /> 下载附件
									</a>
								</Button>
							</div>
						</>
					) : null}
				</SectionCard>

				<SectionCard
					title="我的题解"
					description="仅自己可见，可随时保存；赛后可整理成正式 writeup。"
					actions={
						<Button variant="ghost" size="sm" onClick={() => setWriteupOpen((value) => !value)}>
							{writeupOpen ? "收起" : "展开编辑"}
						</Button>
					}
				>
					{writeupOpen ? (
						<MyWriteupEditor challengeId={challenge.id} />
					) : (
						<p className="text-sm text-muted-foreground">
							点击「展开编辑」撰写题解（Markdown，支持插图）。
						</p>
					)}
				</SectionCard>
			</div>

			<div className="space-y-5">
				<SectionCard title="解题环境" description="启动实例后在此获取入口并提交 flag。">
					{instanceQuery.isPending ? (
						<LoadingBlock label="读取实例状态…" />
					) : instanceQuery.isError ? (
						<ErrorBlock
							error={instanceQuery.error}
							title="读取实例失败"
							onRetry={() => instanceQuery.refetch()}
						/>
					) : instance ? (
						<div className="space-y-4">
							<KeyValueList
								columns={1}
								items={[
									{
										key: "状态",
										value: (
											<TonePill tone={instanceStatusTone(instance.status)}>
												{instance.status || "未知"}
											</TonePill>
										),
									},
									{
										key: "实例 ID",
										value: <CopyText value={instance.id} label="实例 ID" />,
									},
									{
										key: "容器",
										value: <MonoText>{instance.identifier || "—"}</MonoText>,
									},
									{
										key: "自动回收",
										value: instance.destroy_at ? (
											<span className="text-xs">
												{formatDateTime(instance.destroy_at)}（{formatRelative(instance.destroy_at, now)}）
											</span>
										) : (
											"—"
										),
									},
								]}
							/>

							{instanceUrl ? (
								<div className="rounded-md border bg-muted/30 p-3">
									<p className="mb-1 text-xs text-muted-foreground">实例入口</p>
									<div className="flex items-center justify-between gap-2">
										<CopyText value={instanceUrl} label="实例地址" />
										<Button variant="outline" size="sm" asChild>
											<a href={instanceUrl} target="_blank" rel="noopener noreferrer">
												<ExternalLink /> 打开
											</a>
										</Button>
									</div>
								</div>
							) : instance.content ? (
								<ReadonlyBlock>{instance.content}</ReadonlyBlock>
							) : null}

							{mode.kind === "event" ? (
								<p className="text-xs text-muted-foreground">
									赛事内实例不计入题库并发上限；销毁后可在赛事页面重新启动。
								</p>
							) : null}

							<div className="flex flex-wrap gap-2">
								<Button
									variant="outline"
									size="sm"
									disabled={launch.isPending}
									onClick={() => launch.mutate()}
								>
									<RotateCw /> 重启实例
								</Button>
								<Button
									variant="destructive"
									size="sm"
									disabled={destroy.isPending}
									onClick={() => destroy.mutate(instance.id)}
								>
									<Trash2 /> 销毁实例
								</Button>
							</div>
						</div>
					) : (
						<div className="space-y-3">
							<EmptyBlock
								title="还没有运行中的实例"
								description={
									challenge.flag_type === "dynamic"
										? "这是一道动态题：启动后平台会为你的实例生成独立 flag。"
										: "启动后即可获得答题入口（静态题无需容器，启动会创建答题实例）。"
								}
							/>
							<Button
								className="w-full"
								disabled={launch.isPending}
								onClick={() => launch.mutate()}
							>
								<RotateCw /> 启动实例
							</Button>
						</div>
					)}
				</SectionCard>

				<SectionCard title="提交 Flag" description="提交正确后平台自动销毁实例并计分。">
					<form
						className="space-y-3"
						onSubmit={(event) => {
							event.preventDefault();
							if (!instance) {
								toast.warning("请先启动实例");
								return;
							}
							if (flag.trim() === "") {
								toast.warning("请填写 flag");
								return;
							}
							submit.mutate({ instanceId: instance.id, flag: flag.trim() });
						}}
					>
						<div className="space-y-1.5">
							<Label htmlFor="flag-input" className="text-xs">
								Flag
							</Label>
							<Input
								id="flag-input"
								value={flag}
								onChange={(event) => setFlag(event.target.value)}
								placeholder="flag{...}"
								className="font-mono"
								autoComplete="off"
							/>
							<p className="text-xs text-muted-foreground">
								请勿在比赛期间泄露 flag；提交记录会被平台审计。
							</p>
						</div>
						{submit.isError ? <InlineError error={submit.error} /> : null}
						<Button type="submit" className="w-full" disabled={submit.isPending || !instance}>
							<Send /> 提交
						</Button>
					</form>
				</SectionCard>
			</div>
		</div>
	);
}

/** 我的题解：读取（可能 404 = 还没写）→ 保存（POST upsert）。 */
export function MyWriteupEditor({ challengeId }: { challengeId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [content, setContent] = useState<string | null>(null);

	const query = useQuery({
		// 题解始终按**题目**维度缓存：`qk.events.ownWriteup` 属于赛事 PDF writeup，语义不同，绝不能共用。
		queryKey: qk.challenges.myWriteup(challengeId),
		queryFn: async () => {
			try {
				return await callMaybe<{ content: string }>(
					client.service.challenges.getMyWriteup(challengeId),
					"我的题解",
				);
			} catch (error) {
				if (isNotFound(error)) return null;
				throw error;
			}
		},
		retry: false,
	});

	const save = useMutation({
		mutationFn: (text: string) =>
			call(
				client.service.challenges.createMyWriteup({ challenge_id: challengeId, content: text }),
				"保存题解",
			),
		onSuccess: () => {
			toast.success("题解已保存");
			void queryClient.invalidateQueries({
				queryKey: qk.challenges.myWriteup(challengeId),
			});
		},
		onError: (error) => toast.apiError("保存失败", error),
	});

	if (query.isPending) return <LoadingBlock label="读取题解…" />;
	if (query.isError) {
		return <ErrorBlock error={query.error} title="读取题解失败" onRetry={() => query.refetch()} />;
	}

	const value = content ?? query.data?.content ?? "";

	return (
		<div className="space-y-3">
			<MarkdownEditor
				value={value}
				onChange={(next) => setContent(next)}
				minHeight={240}
				placeholder="写下思路、payload、踩坑记录…"
			/>
			<div className="flex flex-wrap items-center justify-between gap-2">
				<p className="text-xs text-muted-foreground">
					{query.data?.content ? "已有保存的题解，可继续编辑。" : "还没有保存过题解。"}
				</p>
				<Button
					disabled={save.isPending || value.trim() === ""}
					onClick={() => save.mutate(value)}
				>
					保存题解
				</Button>
			</div>
			{save.isError ? <InlineError error={save.error} /> : null}
		</div>
	);
}

/** 他人题解列表（可选能力；按题目维度）。 */
export function ChallengeWriteups({ challengeId }: { challengeId: string }) {
	const client = useClient();
	const now = useNow(60_000);
	const query = useQuery({
		queryKey: [...qk.challenges.writeups(challengeId)],
		queryFn: () => call(client.service.challenges.getWriteups(challengeId), "题解列表"),
	});

	if (query.isPending) return <LoadingBlock label="加载题解…" />;
	if (query.isError) {
		return <ErrorBlock error={query.error} title="加载题解失败" onRetry={() => query.refetch()} />;
	}
	const items = query.data ?? [];
	if (items.length === 0) {
		return <EmptyBlock title="还没有公开题解" description="赛后选手可以在这里分享思路。" />;
	}

	return (
		<ul className="divide-y">
			{items.map((item) => (
				<li key={item.id} className="py-3">
					<div className="flex flex-wrap items-center justify-between gap-2">
						<span className="text-sm font-medium">{item.nickname}</span>
						<span className="text-xs text-muted-foreground">
							{formatRelative(item.writeup.updated_at, now)}
						</span>
					</div>
					<div className="mt-2">
						<MarkdownView>{item.writeup.content}</MarkdownView>
					</div>
				</li>
			))}
		</ul>
	);
}

