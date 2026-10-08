/**
 * 社区域列表页 —— Top15 排行榜、解题流水、全站公告、武器库。
 *
 * 口径：
 * - 分页一律「page 从 1 开始 + pageSize + `meta.total`」（`QueryParams` 只有 page/limit/filter）。
 * - 搜索框是受控 `Input` + `useDebouncedValue(300)`，值喂给后端 `filter`；
 *   后端过滤串语法是 `key:value` 且**条件之间用 `&` 分隔**。
 * - 后端确实没有搜索参数时（Top15 / 题解），不做假的「服务端搜索」：
 *   排行榜是定长 15 条（视图内分页），题解接口一次性返回全部（视图内搜索 + 分页），
 *   页面描述里会写明该事实。
 */

import { useMemo, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { Activity, Download, Megaphone, Puzzle, Swords, Trophy } from "lucide-react";

import type { SolveResult, TopUser } from "@floatctf/sdk";
import type { Announcements, Weapons } from "@floatctf/sdk/entity";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	StatCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	QueryState,
	RefreshingBadge,
	TableSkeleton,
} from "~/components/app/states";
import { RelativeTime, UserAvatar, UserCell } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { formatInt, formatScore } from "~/lib/format";
import { useDebouncedValue, useDocumentTitle } from "~/lib/hooks";

import { SearchField } from "./components";

function rankTone(no: number): "warning" | "info" | "muted" {
	if (no === 1) return "warning";
	if (no <= 3) return "info";
	return "muted";
}

/** Top15 用户排行榜（后端固定返回 15 条，无过滤参数）。 */
export function RankPage(): ReactNode {
	useDocumentTitle("排行榜 · FloatCTF");
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(10);

	const query = useQuery({
		queryKey: qk.solves.top15(),
		queryFn: () => call<TopUser[]>(client.service.solves.getTop15Users(), "Top15 排行"),
		refetchInterval: 30_000,
		refetchIntervalInBackground: false,
	});

	const rows = query.data ?? [];
	const paged = useMemo(
		() => rows.slice((page - 1) * pageSize, page * pageSize),
		[rows, page, pageSize],
	);
	const leader = rows[0];
	const latest = rows.reduce<string | null>(
		(acc, row) => (acc === null || row.solved_last_at > acc ? row.solved_last_at : acc),
		null,
	);

	const columns: DataTableColumn<TopUser>[] = [
		{
			id: "no",
			header: "排名",
			align: "center",
			sortValue: (row) => row.no,
			cell: (row) => (
				<TonePill tone={rankTone(row.no)}>
					{row.no === 1 ? <Trophy /> : null}#{row.no}
				</TonePill>
			),
		},
		{
			id: "nickname",
			header: "选手",
			sortValue: (row) => row.nickname,
			cell: (row) => (
				<UserCell
					username={row.nickname}
					nickname={row.nickname}
					avatar={row.avatar ?? null}
					secondary="none"
				/>
			),
		},
		{
			id: "solved_count",
			header: "解出题数",
			align: "right",
			sortValue: (row) => row.solved_count,
			cell: (row) => <MonoText>{formatInt(row.solved_count)}</MonoText>,
		},
		{
			id: "solved_last_at",
			header: "最近解出",
			align: "right",
			sortValue: (row) => row.solved_last_at,
			cell: (row) => <RelativeTime value={row.solved_last_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="排行榜"
				description="练习赛事按解题数排名的 Top15 选手（30 秒自动刷新）。后端固定返回 15 条，分页为视图内分页。"
				badge={<TonePill tone="info">Top15</TonePill>}
				actions={
					<Toolbar>
						<Button variant="outline" asChild>
							<Link to="/solves">
								<Activity /> 解题流水
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-3">
				<StatCard
					label="上榜选手"
					value={query.isPending ? "…" : formatInt(rows.length)}
					hint="后端返回的 Top15 条目数"
					icon={<Trophy className="size-5" />}
				/>
				<StatCard
					label="榜首"
					value={leader?.nickname ?? "—"}
					hint={leader ? `解出 ${formatInt(leader.solved_count)} 题` : "暂无数据"}
					icon={<Trophy className="size-5" />}
					tone="success"
				/>
				<StatCard
					label="最近解出"
					value={<RelativeTime value={latest} className="text-sm" />}
					hint="榜单中最后一次解题时间"
					icon={<Activity className="size-5" />}
				/>
			</div>

			<SectionCard
				title="Top15"
				description="按解出题数降序，题数相同者以最近解题时间排序。"
				actions={<RefreshingBadge active={query.isFetching && !query.isPending} />}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={rows.length}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
						pageSizeOptions={[10, 15]}
					/>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载排行榜失败"
					isEmpty={(list) => list.length === 0}
					empty={
						<EmptyBlock
							title="暂无排行数据"
							description="练习赛事产生解题记录后，Top15 会出现在这里。"
							icon={<Trophy className="size-5" />}
							action={
								<Button size="sm" asChild>
									<Link to="/challenges">去练习题库</Link>
								</Button>
							}
						/>
					}
				>
					{() => (
						<DataTable
							data={paged}
							getRowId={(row) => String(row.no)}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<div className="flex items-center justify-between gap-2">
										<UserCell
											username={row.nickname}
											nickname={row.nickname}
											avatar={row.avatar ?? null}
											secondary="none"
										/>
										<TonePill tone={rankTone(row.no)}>#{row.no}</TonePill>
									</div>
									<div className="flex items-center justify-between text-xs text-muted-foreground">
										<span>解出 {formatInt(row.solved_count)} 题</span>
										<RelativeTime value={row.solved_last_at} />
									</div>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

/** 解题流水：后端只返回**当前账号**在练习赛事中的解题记录，支持按题目过滤。 */
export function SolvesPage(): ReactNode {
	useDocumentTitle("解题流水 · FloatCTF");
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [challengeId, setChallengeId] = useState("all");

	// 过滤选项：后端 `/solves` 只支持 id / challenge_id / event_id 三个键，
	// 因此用「自己解题记录里出现过的题目」构造题目下拉（真实数据，不是枚举题库）。
	const optionsQuery = useQuery({
		queryKey: qk.solves.list({ limit: 200, purpose: "filter-options" }),
		queryFn: () => callList<SolveResult>(client.service.solves.fetch({ limit: 200 })),
	});

	const filter = challengeId === "all" ? undefined : `challenge_id:${challengeId}`;
	const query = useQuery({
		queryKey: qk.solves.list({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<SolveResult>(client.service.solves.fetch({ page, limit: pageSize, filter })),
		refetchInterval: 30_000,
		refetchIntervalInBackground: false,
	});

	const challengeOptions = useMemo(() => {
		const map = new Map<string, string>();
		for (const item of optionsQuery.data?.items ?? []) {
			map.set(item.challenge_id, item.challenge_name);
		}
		return [...map.entries()]
			.map(([id, name]) => ({ id, name }))
			.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
	}, [optionsQuery.data]);

	const items = query.data?.items ?? [];
	const pageScore = items.reduce(
		(sum, item) => sum + item.obtained_points + item.bonus_points,
		0,
	);

	const columns: DataTableColumn<SolveResult>[] = [
		{
			id: "challenge",
			header: "题目",
			sortValue: (row) => row.challenge_name,
			cell: (row) => (
				<Link
					to={`/challenges/${row.challenge_id}`}
					className="text-sm font-medium hover:underline"
				>
					{row.challenge_name}
				</Link>
			),
		},
		{
			id: "solver",
			header: "解题者",
			hideBelow: "sm",
			sortValue: (row) => row.nickname,
			cell: (row) => (
				<UserCell
					username={row.nickname}
					nickname={row.nickname}
					avatar={row.avatar ?? null}
					secondary="none"
				/>
			),
		},
		{
			id: "points",
			header: "得分",
			align: "right",
			sortValue: (row) => row.obtained_points + row.bonus_points,
			cell: (row) => (
				<span className="tnum font-mono text-xs">
					+{formatScore(row.obtained_points)}
					{row.bonus_points !== 0 ? (
						<span className="text-muted-foreground"> (奖励 {formatScore(row.bonus_points)})</span>
					) : null}
				</span>
			),
		},
		{
			id: "created_at",
			header: "提交时间",
			align: "right",
			sortValue: (row) => row.created_at,
			cell: (row) => <RelativeTime value={row.created_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="解题流水"
				description="平台按当前账号返回你在练习赛事中的解题记录（后端接口即此语义，不是全站流水）。"
				actions={
					<Toolbar>
						<Button variant="outline" asChild>
							<Link to="/rank">
								<Trophy /> 排行榜
							</Link>
						</Button>
						<Button asChild>
							<Link to="/challenges">
								<Puzzle /> 去题库
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-2">
				<StatCard
					label="累计解题记录"
					value={query.isPending ? "…" : formatInt(query.data?.meta.total ?? items.length)}
					hint="后端 pagination meta.total"
					icon={<Activity className="size-5" />}
				/>
				<StatCard
					label="本页得分"
					value={query.isPending ? "…" : formatScore(pageScore)}
					hint={`本页 ${items.length} 条记录合计（含奖励分）`}
					icon={<Trophy className="size-5" />}
					tone="success"
				/>
			</div>

			<SectionCard
				title="我的解题记录"
				description="按提交时间倒序，可用题目下拉过滤（后端 challenge_id 过滤参数）。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<Select
							value={challengeId}
							disabled={optionsQuery.isPending}
							onValueChange={(value) => {
								setChallengeId(value);
								setPage(1);
							}}
						>
							<SelectTrigger size="sm" className="w-[220px]" aria-label="按题目筛选">
								<SelectValue placeholder="全部题目" />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部题目</SelectItem>
								{challengeOptions.map((option) => (
									<SelectItem key={option.id} value={option.id}>
										{option.name}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={query.data?.meta.total ?? items.length}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载解题流水失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						challengeId === "all" ? (
							<EmptyBlock
								title="还没有解题记录"
								description="在练习题库中解出题目后，记录会出现在这里。"
								icon={<Activity className="size-5" />}
								action={
									<Button size="sm" asChild>
										<Link to="/challenges">浏览题库</Link>
									</Button>
								}
							/>
						) : (
							<EmptyBlock
								variant="filtered"
								description="这道题在你返回的记录范围内没有解题记录。"
								action={
									<Button size="sm" variant="outline" onClick={() => setChallengeId("all")}>
										清除筛选
									</Button>
								}
							/>
						)
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<Link
										to={`/challenges/${row.challenge_id}`}
										className="text-sm font-medium hover:underline"
									>
										{row.challenge_name}
									</Link>
									<div className="flex items-center gap-2">
										<UserAvatar
											name={row.nickname}
											avatar={row.avatar ?? null}
											size="sm"
										/>
										<span className="truncate text-xs text-muted-foreground">
											{row.nickname}
										</span>
									</div>
									<div className="flex items-center justify-between text-xs text-muted-foreground">
										<span className="tnum font-mono">
											+{formatScore(row.obtained_points)}
										</span>
										<RelativeTime value={row.created_at} />
									</div>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

/** 全站公告：后端支持 id / title / content 过滤，这里用标题过滤。 */
export function AnnouncementsPage(): ReactNode {
	useDocumentTitle("全站公告 · FloatCTF");
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [search, setSearch] = useState("");
	const debouncedSearch = useDebouncedValue(search, 300);

	const trimmed = debouncedSearch.trim();
	const filter = trimmed === "" ? undefined : `title:${trimmed}`;

	const query = useQuery({
		queryKey: qk.announcements.list({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<Announcements>(
				client.service.announcements.fetch({ page, limit: pageSize, filter }),
			),
		refetchInterval: 60_000,
		refetchIntervalInBackground: false,
	});

	const columns: DataTableColumn<Announcements>[] = [
		{
			id: "title",
			header: "标题",
			sortValue: (row) => row.title,
			cell: (row) => <span className="text-sm font-medium">{row.title}</span>,
		},
		{
			id: "publisher",
			header: "发布者",
			hideBelow: "sm",
			sortValue: (row) => row.publisher,
			cell: (row) => <span className="text-sm">{row.publisher || "—"}</span>,
		},
		{
			id: "content",
			header: "内容",
			hideBelow: "md",
			cell: (row) => (
				<div className="max-w-2xl text-sm break-words whitespace-pre-wrap text-muted-foreground">
					{row.content && row.content.trim() !== "" ? row.content : "—"}
				</div>
			),
		},
		{
			id: "created_at",
			header: "发布",
			align: "right",
			sortValue: (row) => row.created_at,
			cell: (row) => <RelativeTime value={row.created_at} />,
		},
		{
			id: "updated_at",
			header: "更新",
			align: "right",
			hideBelow: "lg",
			sortValue: (row) => row.updated_at,
			cell: (row) => <RelativeTime value={row.updated_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="全站公告"
				description="平台与赛事通知（60 秒自动刷新）。"
				badge={<TonePill tone="info">公告</TonePill>}
			/>

			<SectionCard
				title="公告列表"
				description="按最近更新时间排序，支持按标题搜索（后端 title 过滤参数）。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<SearchField
							value={search}
							placeholder="搜索标题…"
							onChange={(value) => {
								setSearch(value);
								setPage(1);
							}}
						/>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={query.data?.meta.total ?? query.data?.items.length ?? 0}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载公告失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						trimmed === "" ? (
							<EmptyBlock
								title="暂无公告"
								description="管理员发布公告后会出现在这里。"
								icon={<Megaphone className="size-5" />}
							/>
						) : (
							<EmptyBlock
								variant="filtered"
								description={`没有标题包含「${trimmed}」的公告。`}
								action={
									<Button size="sm" variant="outline" onClick={() => setSearch("")}>
										清除搜索
									</Button>
								}
							/>
						)
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<p className="text-sm font-medium">{row.title}</p>
									<p className="line-clamp-4 text-xs break-words whitespace-pre-wrap text-muted-foreground">
										{row.content && row.content.trim() !== "" ? row.content : "—"}
									</p>
									<div className="flex items-center justify-between text-xs text-muted-foreground">
										<span>{row.publisher || "—"}</span>
										<RelativeTime value={row.created_at} />
									</div>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

/** 武器库：只读列表 + 附件下载（`file_url` 是对象存储 key，公网路径为 `/public/<key>`）。 */
export function ArsenalPage(): ReactNode {
	useDocumentTitle("武器库 · FloatCTF");
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [search, setSearch] = useState("");
	const debouncedSearch = useDebouncedValue(search, 300);

	const trimmed = debouncedSearch.trim();
	const filter = trimmed === "" ? undefined : `name:${trimmed}`;

	const query = useQuery({
		queryKey: qk.weapons.list({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<Weapons>(client.service.weapons.fetch({ page, limit: pageSize, filter })),
	});

	const columns: DataTableColumn<Weapons>[] = [
		{
			id: "name",
			header: "名称",
			sortValue: (row) => row.name,
			cell: (row) => <span className="text-sm font-medium">{row.name}</span>,
		},
		{
			id: "category",
			header: "分类",
			hideBelow: "sm",
			sortValue: (row) => row.category,
			cell: (row) => <TonePill tone="neutral">{row.category || "未分类"}</TonePill>,
		},
		{
			id: "description",
			header: "描述",
			hideBelow: "md",
			cell: (row) => (
				<p className="line-clamp-3 max-w-xl text-sm break-words text-muted-foreground">
					{row.description && row.description.trim() !== "" ? row.description : "—"}
				</p>
			),
		},
		{
			id: "file",
			header: "附件",
			cell: (row) => <WeaponFile row={row} />,
		},
		{
			id: "download_count",
			header: "下载次数",
			align: "right",
			hideBelow: "lg",
			sortValue: (row) => row.download_count,
			cell: (row) => <MonoText>{formatInt(row.download_count)}</MonoText>,
		},
		{
			id: "updated_at",
			header: "更新",
			align: "right",
			sortValue: (row) => row.updated_at,
			cell: (row) => <RelativeTime value={row.updated_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="武器库"
				description="平台共享的工具与脚本附件，按名称搜索（后端 name 过滤参数）。"
				badge={<TonePill tone="info">只读</TonePill>}
			/>

			<SectionCard
				title="工具列表"
				description="附件由管理员上传，点击文件名即可下载。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<SearchField
							value={search}
							placeholder="搜索名称…"
							onChange={(value) => {
								setSearch(value);
								setPage(1);
							}}
						/>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={query.data?.meta.total ?? query.data?.items.length ?? 0}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载武器库失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						trimmed === "" ? (
							<EmptyBlock
								title="武器库还是空的"
								description="管理员上传工具后，这里会列出可下载的附件。"
								icon={<Swords className="size-5" />}
							/>
						) : (
							<EmptyBlock
								variant="filtered"
								description={`没有名称包含「${trimmed}」的工具。`}
								action={
									<Button size="sm" variant="outline" onClick={() => setSearch("")}>
										清除搜索
									</Button>
								}
							/>
						)
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<div className="flex items-center justify-between gap-2">
										<span className="truncate text-sm font-medium">{row.name}</span>
										<TonePill tone="neutral">{row.category || "未分类"}</TonePill>
									</div>
									{row.description ? (
										<p className="line-clamp-3 text-xs text-muted-foreground">
											{row.description}
										</p>
									) : null}
									<div className="flex items-center justify-between text-xs text-muted-foreground">
										<WeaponFile row={row} />
										<span>下载 {formatInt(row.download_count)}</span>
									</div>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

/** 附件下载单元：`file_url` 是对象存储 key，平台公网路径为 `/public/<key>`（与 Default 一致）。 */
function WeaponFile({ row }: { row: Weapons }) {
	if (!row.has_file || row.file_url.trim() === "") {
		return <span className="text-xs text-muted-foreground">无附件</span>;
	}
	const key = row.file_url.replace(/^\/+/, "");
	const name = key.split("/").pop() ?? key;
	return (
		<a
			href={`/public/${key}`}
			target="_blank"
			rel="noopener noreferrer"
			download
			className="inline-flex items-center gap-1 text-xs hover:underline"
			title={key}
		>
			<Download className="size-3.5" />
			{name}
		</a>
	);
}
