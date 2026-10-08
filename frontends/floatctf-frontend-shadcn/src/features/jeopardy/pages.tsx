/**
 * 题库（练习）/ 题集 —— 选手端解题入口。
 * 题目目录用**全量拉取 + 客户端筛选**（与 Default 的 GenericTable 过滤语义一致：
 * 平台列表接口没有服务端分类过滤，`QueryParams.filter` 只有一个自由文本字段）。
 */

import { useMemo, useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { Blocks, CheckCircle2, ChevronRight, Puzzle, Search } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	LoadingBlock,
	NotFoundBlock,
	QueryState,
} from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import type { ChallengesListItem } from "@floatctf/sdk";
import { formatRelative } from "~/lib/format";
import { useDebouncedValue, useDocumentTitle, useNow } from "~/lib/hooks";
import { ChallengeWorkbench, ChallengeWriteups } from "./workbench";

export function ChallengesCatalogPage() {
	useDocumentTitle("题库 · FloatCTF");
	const client = useClient();
	const [search, setSearch] = useState("");
	const [category, setCategory] = useState("all");
	const [onlyUnsolved, setOnlyUnsolved] = useState(false);
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const debouncedSearch = useDebouncedValue(search, 300);

	const query = useQuery({
		queryKey: qk.challenges.list({ limit: 200 }),
		queryFn: () => callList(client.service.challenges.fetch({ limit: 200 })),
	});

	const items = query.data?.items ?? [];
	const categories = useMemo(
		() => Array.from(new Set(items.map((item) => item.category).filter(Boolean))).sort(),
		[items],
	);

	const filtered = useMemo(() => {
		const keyword = debouncedSearch.trim().toLowerCase();
		return items.filter((item) => {
			if (category !== "all" && item.category !== category) return false;
			if (onlyUnsolved && item.solved) return false;
			if (keyword === "") return true;
			return (
				item.name.toLowerCase().includes(keyword) ||
				item.category.toLowerCase().includes(keyword) ||
				(item.description ?? "").toLowerCase().includes(keyword)
			);
		});
	}, [items, category, onlyUnsolved, debouncedSearch]);

	const paged = filtered.slice((page - 1) * pageSize, page * pageSize);

	const columns: DataTableColumn<ChallengesListItem>[] = [
		{
			id: "name",
			header: "题目",
			sortValue: (row) => row.name,
			cell: (row) => (
				<div className="min-w-0">
					<Link
						to={`/challenges/${row.id}`}
						className="truncate text-sm font-medium hover:underline"
					>
						{row.name}
					</Link>
					<p className="truncate text-xs text-muted-foreground">{row.safe_name}</p>
				</div>
			),
		},
		{
			id: "category",
			header: "分类",
			sortValue: (row) => row.category,
			cell: (row) => <TonePill tone="neutral">{row.category || "未分类"}</TonePill>,
		},
		{
			id: "version",
			header: "版本",
			hideBelow: "sm",
			sortValue: (row) => row.version ?? "",
			cell: (row) => <span className="tnum font-mono text-xs">{row.version ?? "—"}</span>,
		},
		{
			id: "author",
			header: "作者",
			hideBelow: "md",
			cell: (row) => <span className="text-xs">{row.author || "—"}</span>,
		},
		{
			id: "solved",
			header: "状态",
			cell: (row) =>
				row.solved ? (
					<TonePill tone="success" icon={<CheckCircle2 />}>
						已解出
					</TonePill>
				) : (
					<TonePill tone="muted">未解出</TonePill>
				),
		},
		{
			id: "actions",
			header: "",
			align: "right",
			cell: (row) => (
				<Button variant="ghost" size="sm" asChild>
					<Link to={`/challenges/${row.id}`}>
						开始 <ChevronRight />
					</Link>
				</Button>
			),
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="题库"
				description="练习用题目集合。动态题启动后平台为你的实例生成独立 flag。"
				actions={
					<Toolbar>
						<Button variant="outline" asChild>
							<Link to="/challenge-sets">
								<Blocks /> 题集
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="全部题目"
				description={
					query.isSuccess
						? `共 ${items.length} 道，当前筛选后 ${filtered.length} 道`
						: undefined
				}
				actions={
					<Toolbar>
						<div className="relative">
							<Search className="pointer-events-none absolute top-1/2 left-2 size-4 -translate-y-1/2 text-muted-foreground" />
							<Input
								value={search}
								onChange={(event) => {
									setSearch(event.target.value);
									setPage(1);
								}}
								placeholder="搜索题名 / 分类 / 描述"
								className="w-56 pl-8"
							/>
						</div>
						<Select
							value={category}
							onValueChange={(value) => {
								setCategory(value);
								setPage(1);
							}}
						>
							<SelectTrigger className="w-36" aria-label="分类筛选">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部分类</SelectItem>
								{categories.map((item) => (
									<SelectItem key={item} value={item}>
										{item}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<Button
							variant={onlyUnsolved ? "default" : "outline"}
							size="sm"
							onClick={() => {
								setOnlyUnsolved((value) => !value);
								setPage(1);
							}}
						>
							{onlyUnsolved ? "只看未解出" : "含已解出"}
						</Button>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={filtered.length}
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
					skeleton={<LoadingBlock label="加载题库…" />}
					isEmpty={() => items.length === 0}
					empty={
						<EmptyBlock
							title="题库还是空的"
							description="管理员导入题目后会出现在这里。"
							icon={<Puzzle className="size-5" />}
						/>
					}
				>
					{() =>
						filtered.length === 0 ? (
							<EmptyBlock variant="filtered" />
						) : (
							<DataTable
								data={paged}
								getRowId={(row) => row.id}
								columns={columns}
								mobileCard={(row) => (
									<div className="flex items-center justify-between gap-3">
										<div className="min-w-0">
											<Link
												to={`/challenges/${row.id}`}
												className="truncate text-sm font-medium hover:underline"
											>
												{row.name}
											</Link>
											<p className="text-xs text-muted-foreground">
												{row.category || "未分类"} · {row.solved ? "已解出" : "未解出"}
											</p>
										</div>
										<ChevronRight className="size-4 text-muted-foreground" />
									</div>
								)}
							/>
						)
					}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

export function ChallengeDetailPage() {
	const { challengeId = "" } = useParams<{ challengeId: string }>();
	const client = useClient();
	const query = useQuery({
		queryKey: qk.challenges.detail(challengeId),
		queryFn: () => call(client.service.challenges.get(challengeId), "题目详情"),
		enabled: challengeId !== "",
	});

	return (
		<PageBody>
			<QueryState query={query} skeleton={<LoadingBlock label="加载题目…" />} errorTitle="加载题目失败">
				{(challenge) => (
					<>
						<PageHeader
							title={challenge.name}
							description={challenge.safe_name}
							breadcrumbs={
								<span className="flex items-center gap-1">
									<Link to="/challenges" className="hover:underline">
										题库
									</Link>
									<ChevronRight className="size-3" />
									<span>{challenge.category || "未分类"}</span>
								</span>
							}
							badge={
								challenge.solved ? (
									<TonePill tone="success" icon={<CheckCircle2 />}>
										已解出
									</TonePill>
								) : null
							}
						/>
						<ChallengeWorkbench challenge={challenge} mode={{ kind: "practice" }} />
						<SectionCard title="公开题解" description="其他选手分享的解题思路。">
							<ChallengeWriteups challengeId={challengeId} />
						</SectionCard>
					</>
				)}
			</QueryState>
		</PageBody>
	);
}

export function ChallengeSetsPage() {
	useDocumentTitle("题集 · FloatCTF");
	const client = useClient();
	const now = useNow(60_000);
	const query = useQuery({
		queryKey: qk.sets.list({}),
		queryFn: () => call(client.service.challenges.getChallengeSets(), "题集列表"),
	});

	return (
		<PageBody>
			<PageHeader
				title="题集"
				description="按主题组织的题目集合，适合专项训练。"
				actions={
					<Button variant="outline" asChild>
						<Link to="/challenges">
							<Puzzle /> 全部题目
						</Link>
					</Button>
				}
			/>
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="加载题集…" />}
				isEmpty={(sets) => sets.length === 0}
				empty={<EmptyBlock title="还没有题集" icon={<Blocks className="size-5" />} />}
			>
				{(sets) => (
					<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
						{sets.map((set) => (
							<SectionCard
								key={set.id}
								title={set.name}
								description={set.description}
								actions={
									<Button variant="ghost" size="sm" asChild>
										<Link to={`/challenge-sets/${set.id}`}>
											打开 <ChevronRight />
										</Link>
									</Button>
								}
							>
								<p className="text-xs text-muted-foreground">
									更新于 {formatRelative(set.updated_at, now)}
								</p>
							</SectionCard>
						))}
					</div>
				)}
			</QueryState>
		</PageBody>
	);
}

export function ChallengeSetDetailPage() {
	const { setId = "" } = useParams<{ setId: string }>();
	const client = useClient();
	const query = useQuery({
		queryKey: qk.sets.detail(setId),
		queryFn: () => call(client.service.challenges.getChallengeSet(setId), "题集详情"),
		enabled: setId !== "",
	});

	return (
		<PageBody>
			<PageHeader
				title="题集内容"
				breadcrumbs={
					<Link to="/challenge-sets" className="hover:underline">
						题集
					</Link>
				}
			/>
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="加载题集…" />}
				errorTitle="加载题集失败"
				isEmpty={(items) => items.length === 0}
				empty={<NotFoundBlock title="题集里还没有题目" />}
			>
				{(items) => (
					<SectionCard title={`共 ${items.length} 道题`} contentClassName="px-0">
						<ul className="divide-y">
							{items.map((challenge) => (
								<li key={challenge.id}>
									<Link
										to={`/challenges/${challenge.id}`}
										className="flex items-center justify-between gap-3 px-5 py-3 hover:bg-muted/40"
									>
										<div className="min-w-0">
											<p className="truncate text-sm font-medium">{challenge.name}</p>
											<p className="truncate text-xs text-muted-foreground">
												{challenge.category || "未分类"}
											</p>
										</div>
										{challenge.solved ? (
											<TonePill tone="success" icon={<CheckCircle2 />}>
												已解出
											</TonePill>
										) : (
											<ChevronRight className="size-4 text-muted-foreground" />
										)}
									</Link>
								</li>
							))}
						</ul>
					</SectionCard>
				)}
			</QueryState>
		</PageBody>
	);
}

