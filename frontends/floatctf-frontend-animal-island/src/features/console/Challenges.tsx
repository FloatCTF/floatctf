/**
 * 题库管理（控制台）。
 *
 * 这里承载了平台的「题包生命周期」：**导入 → 校验 → 构建 → 认领镜像**。
 * 因此操作按钮的顺序就是真实的操作顺序，而不是一组并列的动作 ——
 * 顺序错了（比如没校验就构建）会得到一个状态错误的题目。
 *
 * 全量操作（不传 id）作用于**所有**题目，代价很高，因此一律二次确认。
 */
import { useState } from "react";
import {
	Button,
	Card,
	Divider,
	Input,
	Modal,
	Notification,
	Switch,
	Tag,
	Tabs,
	Upload,
	type TableColumn,
} from "animal-island-ui";
import {
	CheckCircleIcon,
	EditIcon,
	GridIcon,
	PlusIcon,
	RefreshIcon,
	TrashIcon,
	UploadIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { EmptyState, ErrorState } from "~/components/states";
import type { ChallengesListItem } from "@floatctf/sdk";
import type { ChallengeSets } from "@floatctf/sdk/entity";

function buildStatusTag(status: string | undefined) {
	if (!status) return <Tag size="small" variant="outlined">未构建</Tag>;
	const map: Record<string, "app-green" | "app-yellow" | "app-red" | "brown"> = {
		ready: "app-green",
		pending: "app-yellow",
		building: "app-yellow",
		missing: "app-red",
		failed: "app-red",
		error: "app-red",
	};
	return (
		<Tag size="small" variant="soft" color={map[status] ?? "brown"}>
			{status}
		</Tag>
	);
}

function ChallengesTab() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Partial<ChallengesListItem> | null>(null);
	const [result, setResult] = useState<string | null>(null);

	const query = useUniQuery(
		["console", "challenges"],
		() => runtime.client.admin.challenges.fetch(),
		"加载题目",
	);

	const save = useUniMutation(
		(vars: Partial<ChallengesListItem>) =>
			vars.id
				? runtime.client.admin.challenges.patch(vars)
				: runtime.client.admin.challenges.create(vars),
		"保存题目",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.challenges.remove(vars),
		"删除题目",
	);
	const check = useUniMutation(
		(vars: string[] | undefined) => runtime.client.admin.challenges.checkChallenges(vars),
		"校验题目",
	);
	const build = useUniMutation(
		(vars: string[] | undefined) => runtime.client.admin.challenges.buildChallenges(vars),
		"构建题目",
	);
	const scan = useUniMutation(
		() => runtime.client.admin.challenges.scanChallenges(),
		"扫描题目",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<ChallengesListItem>[] = [
		{ title: "题目", dataIndex: "name", width: 240 },
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
		{ title: "版本", dataIndex: "version", width: 90 },
		{
			title: "构建状态",
			dataIndex: "build_status",
			width: 130,
			render: (v) => buildStatusTag(v ? String(v) : undefined),
		},
		{
			title: "公开",
			dataIndex: "hidden",
			width: 90,
			render: (v) =>
				v ? (
					<Tag size="small" variant="soft" color="brown">
						隐藏
					</Tag>
				) : (
					<Tag size="small" variant="soft" color="app-green">
						公开
					</Tag>
				),
		},
		{
			title: "操作",
			width: 190,
			align: "right",
			render: (_v, row) => (
				<Can permission="challenges.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button
						size="small"
						type="text"
						icon={<EditIcon size={14} />}
						onClick={() => setEditing({ ...row })}
					>
						编辑
					</Button>
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `删除题目「${row.name}」`,
								consequence:
									"题目会被删除。**已经绑定了这道题的赛事会失去它**，选手侧对应条目也会消失；已产生的解题记录不再能关联到题面。无法恢复。",
								okText: "删除",
								danger: true,
								confirmPhrase: row.name,
							});
							if (!ok) return;
							await remove.mutateAsync([row.id]);
							void query.refetch();
						}}
					>
						删除
					</Button>
				</div>
				</Can>
			),
		},
	];

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Can permission="challenges.manage">
			<Card>
				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Upload
							showUploadList={false}
							accept=".zip"
							customRequest={({ file, onSuccess, onError }) => {
								runtime.client.admin.challenges
									.importChallenge(file)
									.then((res) => {
										Notification.success({
											message: "题包已导入",
											description: res?.data?.challenge?.name
												? `题目：${res.data.challenge.name}`
												: undefined,
										});
										onSuccess(res);
										void query.refetch();
									})
									.catch((e) => {
										onError(e);
									});
							}}
						>
							<Button type="primary" icon={<UploadIcon size={16} />}>
								导入题包
							</Button>
						</Upload>
						<span className="ai-muted">导入 .zip 题包（含 meta 与构建上下文）</span>
					</div>

					<div className="ai-row" style={{ gap: 8 }}>
						<Button
							size="small"
							icon={<CheckCircleIcon size={14} />}
							loading={check.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "校验全部题目",
									consequence:
										"会对**所有**题目重新校验构建状态与镜像引用。题目很多时会占用较长时间，并产生额外的容器/镜像查询。",
									okText: "开始校验",
								});
								if (!ok) return;
								const rows = await check.mutateAsync(undefined);
								setResult(
									(rows ?? [])
										.map(
											(r) =>
												`${r.is_ok ? "OK  " : "FAIL"} ${r.challenge_name}` +
												`（镜像 ${r.docker_image ? "有" : "无"}／附件 ${r.attachment ? "有" : "无"}` +
												`${r.static_content ? "／静态题" : ""}）`,
										)
										.join("\n") || "（没有返回结果）",
								);
								void query.refetch();
							}}
						>
							校验全部
						</Button>
						<Button
							size="small"
							loading={build.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "构建全部题目",
									consequence:
										"会对**所有**题目重新构建镜像。这会真实占用宿主的构建资源与磁盘，通常需要数分钟，期间可能影响其它题目的构建。",
									okText: "开始构建",
									danger: true,
									confirmPhrase: "构建",
								});
								if (!ok) return;
								const rows = await build.mutateAsync(undefined);
								setResult(
									(rows ?? [])
										.map(
											(r) => `${r.is_ok ? "OK  " : "FAIL"} ${r.challenge_name}: ${r.message}`,
										)
										.join("\n") || "（没有返回结果）",
								);
								void query.refetch();
							}}
						>
							构建全部
						</Button>
						<Button
							size="small"
							icon={<RefreshIcon size={14} />}
							loading={scan.isPending}
							onClick={async () => {
								const rows = await scan.mutateAsync(undefined);
								setResult(
									(rows ?? [])
										.map(
											(r) =>
												`${r.status === "added" ? "新增" : r.status === "skipped" ? "跳过" : "错误"} ${r.name ?? r.safe_name} ${r.version ?? ""}: ${r.message}`,
										)
										.join("\n") || "（没有返回结果）",
								);
							}}
						>
							扫描本地镜像
						</Button>
					</div>
				</div>
			</Card>
			</Can>

			<ResourcePage<ChallengesListItem>
				columns={can("challenges.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="题库是空的"
				emptyDescription="导入一个题包开始。"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑题目" : "新建题目"}
				typewriter={false}
				width={620}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.name?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({ ...editing, name: editing.name?.trim() });
								Notification.success({ message: editing.id ? "已更新" : "已创建" });
								setEditing(null);
								void query.refetch();
							}}
						>
							保存
						</Button>
					</>
				}
			>
				{editing ? (
					<div className="ai-col" style={{ gap: 12 }}>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>题名</span>
							<Input
								value={editing.name ?? ""}
								onChange={(e) => setEditing({ ...editing, name: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>分类</span>
							<Input
								value={editing.category ?? ""}
								onChange={(e) => setEditing({ ...editing, category: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>描述</span>
							<Input
								value={editing.description ?? ""}
								onChange={(e) => setEditing({ ...editing, description: e.target.value })}
							/>
						</label>
						<label className="ai-row" style={{ gap: 8 }}>
							<Switch
								checked={!editing.hidden}
								onChange={(checked) => setEditing({ ...editing, hidden: !checked })}
							/>
							<span>对选手公开</span>
						</label>
					</div>
				) : null}
			</Modal>

			<Modal
				open={result !== null}
				title="操作结果"
				typewriter={false}
				width={720}
				onClose={() => setResult(null)}
				footer={<Button onClick={() => setResult(null)}>关闭</Button>}
			>
				<div className="ai-terminal" style={{ maxHeight: 420 }}>
					{result}
				</div>
			</Modal>
		</div>
	);
}

function SetsTab() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Partial<ChallengeSets> | null>(null);

	const sets = useUniQuery(
		["console", "challenge-sets"],
		() => runtime.client.admin.challenges.getChallengeSets(),
		"加载题集",
	);
	const save = useUniMutation(
		(vars: Partial<ChallengeSets>) =>
			vars.id
				? runtime.client.admin.challenges.patchChallengeSet(vars)
				: runtime.client.admin.challenges.createChallengeSet(vars),
		"保存题集",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.challenges.deleteChallengeSet(vars),
		"删除题集",
	);

	if (sets.isLoading) return <ErrorState error={null} />;
	if (sets.error) return <ErrorState error={sets.error} onRetry={() => void sets.refetch()} />;

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<span className="ai-muted">
						题集用于把题目按主题分组，便于赛事绑定与分发。
					</span>
					<Can permission="challenges.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing({ name: "", description: "" })}
						>
							新建题集
						</Button>
					</Can>
				</div>
			</Card>

			{(sets.data?.length ?? 0) === 0 ? (
				<Card>
					<EmptyState
						title="还没有题集"
						description="创建一个题集，然后往里加题。"
						action={
							<Can permission="challenges.manage">
								<Button
									type="primary"
									icon={<PlusIcon size={16} />}
									onClick={() => setEditing({ name: "", description: "" })}
								>
									新建题集
								</Button>
							</Can>
						}
					/>
				</Card>
			) : (
				<div className="ai-grid">
					{(sets.data ?? []).map((set) => (
						<Card key={set.id}>
							<div className="ai-col" style={{ gap: 8 }}>
								<div className="ai-row-between">
									<span className="ai-strong">{set.name}</span>
									<Tag size="small" variant="outlined">
										<GridIcon size={12} /> 题集
									</Tag>
								</div>
								{set.description ? (
									<span className="ai-muted" style={{ fontSize: 13 }}>
										{set.description}
									</span>
								) : null}
								<Divider />
								<Can permission="challenges.manage">
								<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
									<Button
										size="small"
										type="text"
										icon={<EditIcon size={14} />}
										onClick={() => setEditing({ ...set })}
									>
										编辑
									</Button>
									<Button
										size="small"
										type="text"
										danger
										icon={<TrashIcon size={14} />}
										onClick={async () => {
											const ok = await confirm({
												title: `删除题集「${set.name}」`,
												consequence:
													"题集会被删除。**题目本身不会被删除**，但已绑定该题集的赛事/分发包会失去这个分组。",
												okText: "删除",
												danger: true,
											});
											if (!ok) return;
											await remove.mutateAsync([set.id]);
											void sets.refetch();
										}}
									>
										删除
									</Button>
								</div>
								</Can>
							</div>
						</Card>
					))}
				</div>
			)}

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑题集" : "新建题集"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.name?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({ ...editing, name: editing.name?.trim() });
								setEditing(null);
								void sets.refetch();
							}}
						>
							保存
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>名称</span>
						<Input
							value={editing?.name ?? ""}
							onChange={(e) => setEditing({ ...editing, name: e.target.value })}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>描述</span>
						<Input
							value={editing?.description ?? ""}
							onChange={(e) => setEditing({ ...editing, description: e.target.value })}
						/>
					</label>
				</div>
			</Modal>
		</div>
	);
}

export function ConsoleChallenges() {
	return (
		<>
			<PageHeader
				title="题库管理"
				description="题包生命周期依次是导入、校验、构建、认领镜像。题集用于分组。"
			/>
			<Card>
				<Tabs
					aria-label="题库分区"
					defaultActiveKey="challenges"
					items={[
						{ key: "challenges", label: "题目", children: <ChallengesTab /> },
						{ key: "sets", label: "题集", children: <SetsTab /> },
					]}
				/>
			</Card>
		</>
	);
}
