/**
 * 练习题库 —— 与赛事无关的常驻题目。
 *
 * 交互取舍：这里用**表格**而不是卡片网格，因为题库的核心任务是对比
 * （分类 / 分值 / 是否解出），表格的列对齐让扫读更快；赛事才用卡片。
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Button, Input, Select, Tag, type TableColumn } from "animal-island-ui";
import { CheckCircleIcon, MagnetIcon, SearchIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { ResourcePage } from "~/components/resource";
import type { ChallengesListItem } from "@floatctf/sdk";

const ALL = "__all__";

export function ChallengeList() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [category, setCategory] = useState(ALL);
	const [keyword, setKeyword] = useState("");

	const query = useUniQuery(
		["island", "challenges"],
		() => runtime.client.service.challenges.fetch(),
		"加载题目",
	);

	const categories = useMemo(() => {
		const set = new Set<string>();
		for (const c of query.data ?? []) set.add(c.category);
		return [...set].sort();
	}, [query.data]);

	const rows = useMemo(() => {
		const list = query.data ?? [];
		const k = keyword.trim().toLowerCase();
		return list.filter((c) => {
			if (category !== ALL && c.category !== category) return false;
			if (k && !`${c.name}${c.safe_name}`.toLowerCase().includes(k)) return false;
			return true;
		});
	}, [query.data, category, keyword]);

	const columns: TableColumn<ChallengesListItem>[] = [
		{
			title: "",
			width: 48,
			render: (_v, row) =>
				row.solved ? (
					<span title="已解出">
						<CheckCircleIcon size={18} />
					</span>
				) : (
					<span className="ai-muted">—</span>
				),
		},
		{ title: "题目", dataIndex: "name", width: 280 },
		{
			title: "分类",
			dataIndex: "category",
			width: 140,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "附件",
			width: 120,
			render: (_v, row) =>
				row.attachment ? (
					<Tag size="small" variant="soft" color="app-blue">
						有
					</Tag>
				) : (
					<span className="ai-muted">无</span>
				),
		},
		{
			title: "操作",
			width: 120,
			align: "right",
			render: (_v, row) => (
				<Button
					size="small"
					type="primary"
					icon={<MagnetIcon size={14} />}
					onClick={() => void navigate(`/island/challenges/${row.id}`)}
				>
					打开
				</Button>
			),
		},
	];

	return (
		<>
			<PageHeader
				title="练习题库"
				description="不依赖赛事的常驻题目。用于热身、复现与练手。"
			/>
			<ResourcePage<ChallengesListItem>
				columns={columns}
				rows={rows}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="没有匹配的题目"
				emptyDescription="换个分类或关键词试试。"
				disablePagination
				filterBar={
					<>
						<div style={{ minWidth: 170 }}>
							<Select
								value={category}
								onChange={setCategory}
								options={[
									{ key: ALL, label: "全部分类" },
									...categories.map((c) => ({ key: c, label: c })),
								]}
							/>
						</div>
						<div style={{ minWidth: 220 }}>
							<Input
								allowClear
								prefix={<SearchIcon size={16} />}
								placeholder="搜索题目名"
								value={keyword}
								onChange={(e) => setKeyword(e.target.value)}
							/>
						</div>
						<span className="ai-muted">共 {rows.length} 题</span>
					</>
				}
			/>
		</>
	);
}
