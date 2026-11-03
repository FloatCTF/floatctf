/**
 * 审计日志。
 *
 * v2 起审计主体已统一为**单一** `actor_user_id`（原 `user_id` / `superadmin_id` 双列已合并）。
 * 因此这里的筛选只有一个"操作主体"，不再需要让使用者自己决定该填哪一列 ——
 * 那正是旧模型最容易漏记管理动作的地方。
 *
 * `details` 是后端序列化的 JSON 字符串：这里用等宽块展示原始文本，
 * 而不是猜测它的结构去渲染字段（猜错会静默丢信息）。
 */
import { useMemo, useState } from "react";
import { Button, Divider, Drawer, Input, Select, Tag, type TableColumn } from "animal-island-ui";
import { RefreshIcon, SearchIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { ResourcePage } from "~/components/resource";
import { formatTime } from "~/lib/event";
import type { Logs } from "@floatctf/sdk/entity";

const ALL = "__all__";

function levelColor(level: string): "app-red" | "app-yellow" | "app-blue" | "brown" {
	const l = level.toLowerCase();
	if (l === "error" || l === "critical") return "app-red";
	if (l === "warn" || l === "warning") return "app-yellow";
	if (l === "info") return "app-blue";
	return "brown";
}

export function ConsoleLogs() {
	const runtime = useRuntime();
	const [level, setLevel] = useState(ALL);
	const [category, setCategory] = useState(ALL);
	const [actor, setActor] = useState("");
	const [expanded, setExpanded] = useState<Logs | null>(null);

	const query = useUniQuery(
		["console", "logs"],
		() => runtime.client.admin.logs.fetch(),
		"加载日志",
	);

	const categories = useMemo(() => {
		const set = new Set<string>();
		for (const l of query.data ?? []) set.add(l.category);
		return [...set].sort();
	}, [query.data]);

	const levels = useMemo(() => {
		const set = new Set<string>();
		for (const l of query.data ?? []) set.add(l.level);
		return [...set].sort();
	}, [query.data]);

	const rows = useMemo(() => {
		const list = query.data ?? [];
		const a = actor.trim().toLowerCase();
		return list.filter((l) => {
			if (level !== ALL && l.level !== level) return false;
			if (category !== ALL && l.category !== category) return false;
			if (a) {
				const id = (l.actor_user_id ?? "").toLowerCase();
				if (!id.includes(a)) return false;
			}
			return true;
		});
	}, [query.data, level, category, actor]);

	const columns: TableColumn<Logs>[] = [
		{
			title: "时间",
			dataIndex: "created_at",
			width: 165,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "级别",
			dataIndex: "level",
			width: 90,
			render: (v) => (
				<Tag size="small" variant="soft" color={levelColor(String(v))}>
					{String(v)}
				</Tag>
			),
		},
		{
			title: "分类",
			dataIndex: "category",
			width: 130,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{ title: "动作", dataIndex: "action", width: 170 },
		{
			title: "主体",
			dataIndex: "actor_user_id",
			width: 130,
			// 主体为空的日志是**系统行为**（定时任务、调度器），不是数据缺失 ——
			// 因此显示为"系统"而不是空白，避免读日志的人以为是丢字段。
			render: (v) =>
				v ? (
					<span className="ai-mono" style={{ fontSize: 12 }}>
						{String(v).slice(0, 8)}
					</span>
				) : (
					<Tag size="small" variant="outlined">
						系统
					</Tag>
				),
		},
		{
			title: "消息",
			dataIndex: "message",
			render: (v, row) => (
				<Button size="small" type="text" onClick={() => setExpanded(row)}>
					{String(v).length > 80 ? `${String(v).slice(0, 80)}…` : String(v)}
				</Button>
			),
		},
	];

	return (
		<>
			<PageHeader
				title="审计日志"
				description="谁在什么时候做了什么。主体是单一 actor_user_id（v2 起合并了原先的双列）。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void query.refetch()}
					>
						刷新
					</Button>
				}
			/>

			<ResourcePage<Logs>
				columns={columns}
				rows={rows}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="没有匹配的日志"
				emptyDescription="换个筛选条件试试。"
				filterBar={
					<>
						<div style={{ minWidth: 140 }}>
							<Select
								value={level}
								onChange={setLevel}
								options={[{ key: ALL, label: "全部级别" }, ...levels.map((l) => ({ key: l, label: l }))]}
							/>
						</div>
						<div style={{ minWidth: 160 }}>
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
								placeholder="按操作主体 ID 过滤"
								value={actor}
								onChange={(e) => setActor(e.target.value)}
							/>
						</div>
						<span className="ai-muted">共 {rows.length} 条</span>
					</>
				}
			/>

			<Drawer
				open={expanded !== null}
				onClose={() => setExpanded(null)}
				title="日志详情"
				placement="right"
				width={560}
			>
				{expanded ? (
					<div className="ai-col" style={{ gap: 10 }}>
						<div className="ai-row" style={{ gap: 8 }}>
							<Tag size="small" variant="soft" color={levelColor(expanded.level)}>
								{expanded.level}
							</Tag>
							<Tag size="small" variant="outlined">
								{expanded.category}
							</Tag>
							<Tag size="small" variant="outlined">
								{expanded.action}
							</Tag>
						</div>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							{formatTime(expanded.created_at)}
						</span>
						<Divider />
						<div className="ai-col" style={{ gap: 4 }}>
							<span className="ai-strong">消息</span>
							<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
								{expanded.message}
							</pre>
						</div>
						<div className="ai-col" style={{ gap: 4 }}>
							<span className="ai-strong">操作主体</span>
							{expanded.actor_user_id ? (
								<span className="ai-mono">{expanded.actor_user_id}</span>
							) : (
								<span className="ai-muted">系统（无人类主体）</span>
							)}
						</div>
						{expanded.ip_address ? (
							<div className="ai-col" style={{ gap: 4 }}>
								<span className="ai-strong">来源 IP</span>
								<span className="ai-mono">{expanded.ip_address}</span>
							</div>
						) : null}
						<div className="ai-col" style={{ gap: 4 }}>
							<span className="ai-strong">details（原始 JSON 文本）</span>
							<div className="ai-terminal" style={{ maxHeight: 260 }}>
								{expanded.details || "（空）"}
							</div>
						</div>
					</div>
				) : null}
			</Drawer>
		</>
	);
}
