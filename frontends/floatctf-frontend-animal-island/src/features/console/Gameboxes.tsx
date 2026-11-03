/**
 * 箱子库（AWD）。
 *
 * 「箱子」是 AWD 的靶机镜像。与题库一样是**导入 → 扫描 → 校验 → 构建**的生命周期，
 * 因此顶部操作区按这个顺序排列；全量操作会真实占用宿主构建资源，一律强确认。
 *
 * 注意：这里的 `hidden` 不是"删除"，只是从选手可见列表里撤下 —— 界面上分开表达，
 * 免得管理员为了"先下架"去做不可逆的删除。
 */
import { useState } from "react";
import {
	Button,
	Card,
	Input,
	Modal,
	Notification,
	Switch,
	Tag,
	Upload,
	type TableColumn,
} from "animal-island-ui";
import { CheckCircleIcon, EditIcon, RefreshIcon, TrashIcon, UploadIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import type { GameBoxLibraryDto } from "@floatctf/sdk";

export function ConsoleGameboxes() {
	// 状态列需要「只读可见、可写才可点」，因此用 can() 而非 <Can>。
	const can = useCan();
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<GameBoxLibraryDto | null>(null);
	const [result, setResult] = useState<string | null>(null);

	const query = useUniQuery(
		["console", "gameboxes"],
		() => runtime.client.awd.admin.listGameboxes(),
		"加载箱子库",
	);

	const importBox = useUniMutation(
		(vars: File) => runtime.client.awd.admin.importGamebox(vars),
		"导入箱子",
	);
	const update = useUniMutation(
		(vars: { id: string; body: { name?: string; category?: string; description?: string } }) =>
			runtime.client.awd.admin.updateGamebox(vars.id, vars.body),
		"更新箱子",
	);
	const hide = useUniMutation(
		(vars: string) => runtime.client.awd.admin.hideGamebox(vars),
		"切换可见性",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.awd.admin.removeGamebox(vars),
		"删除箱子",
	);
	const scan = useUniMutation(() => runtime.client.awd.admin.scanGameboxes(), "扫描箱子");
	const check = useUniMutation(
		(vars: string[] | undefined) => runtime.client.awd.admin.checkGameboxes(vars),
		"校验箱子",
	);
	const build = useUniMutation(
		(vars: string[] | undefined) => runtime.client.awd.admin.buildGameboxes(vars),
		"构建箱子",
	);

	const columns: TableColumn<GameBoxLibraryDto>[] = [
		{ title: "名称", dataIndex: "name", width: 220 },
		{ title: "标识", dataIndex: "safe_name", width: 180 },
		{
			title: "分类",
			dataIndex: "category",
			width: 120,
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
			width: 120,
			render: (v) => {
				const s = v ? String(v) : "";
				const color =
					s === "ready" ? "app-green" : s.includes("fail") || s === "missing" ? "app-red" : "app-yellow";
				return s ? (
					<Tag size="small" variant="soft" color={color}>
						{s}
					</Tag>
				) : (
					<Tag size="small" variant="outlined">
						未构建
					</Tag>
				);
			},
		},
		{
			title: "可见",
			dataIndex: "hidden",
			width: 90,
			render: (v, row) => {
				// 这一列既是**状态**又是切换入口。只读用户应当仍能看到「可见 / 已隐藏」
				// （那是事实信息），只是点不动 —— 直接隐藏整列会让他无法判断箱子是否已隐藏。
				const status = (
					<Tag size="small" variant="soft" color={v ? "brown" : "app-green"}>
						{v ? "已隐藏" : "可见"}
					</Tag>
				);
				return can("gameboxes.manage") ? (
					<Button size="small" type="text" onClick={() => void hide.mutateAsync(row.id)}>
						{status}
					</Button>
				) : (
					status
				);
			},
		},
		{
			title: "操作",
			width: 170,
			align: "right",
			render: (_v, row) => (
				<Can permission="gameboxes.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button size="small" type="text" icon={<EditIcon size={14} />} onClick={() => setEditing(row)}>
						编辑
					</Button>
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `删除箱子「${row.name}」`,
								consequence:
									"箱子记录会被移除。**已经部署了它的 AWD 赛事会失去重建能力**；正在运行的实例不会被自动回收，但之后无法再拉起。无法恢复。",
								okText: "删除",
								danger: true,
								confirmPhrase: row.safe_name,
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

	function fmtScan(rows: Awaited<ReturnType<typeof scan.mutateAsync>>) {
		return (
			(rows ?? [])
				.map(
					(r) =>
						`${r.status === "added" ? "新增" : r.status === "skipped" ? "跳过" : "错误"} ${r.name ?? r.safe_name} ${r.version ?? ""}: ${r.message}`,
				)
				.join("\n") || "（没有返回结果）"
		);
	}

	return (
		<>
			<PageHeader
				title="箱子库"
				description="AWD 靶机镜像。生命周期依次是导入、扫描、校验、构建。"
			/>

			<Can permission="gameboxes.manage">
			<Card>
				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Upload
							showUploadList={false}
							accept=".zip,.tar.gz"
							customRequest={({ file, onSuccess, onError }) => {
								importBox
									.mutateAsync(file)
									.then((r) => {
										Notification.success({
											message: "箱子已导入",
											description: r?.gamebox?.name ? `箱子：${r.gamebox.name}` : undefined,
										});
										onSuccess(r);
										void query.refetch();
									})
									.catch(onError);
							}}
						>
							<Button type="primary" icon={<UploadIcon size={16} />}>
								导入箱子
							</Button>
						</Upload>
						<span className="ai-muted">导入后通常需要紧接着「扫描」认领本地镜像</span>
					</div>

					<div className="ai-row" style={{ gap: 8 }}>
						<Button
							size="small"
							icon={<RefreshIcon size={14} />}
							loading={scan.isPending}
							onClick={async () => setResult(fmtScan(await scan.mutateAsync(undefined)))}
						>
							扫描本地镜像
						</Button>
						<Button
							size="small"
							icon={<CheckCircleIcon size={14} />}
							loading={check.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "校验全部箱子",
									consequence: "会对**所有**箱子重新校验镜像与包目录是否就绪，箱子多时耗时较长。",
									okText: "开始校验",
								});
								if (!ok) return;
								const rows = await check.mutateAsync(undefined);
								setResult(
									(rows ?? [])
										.map(
											(r) =>
												`${r.is_ok ? "OK  " : "FAIL"} ${r.gamebox_name}` +
												`（镜像 ${r.docker_image ? "有" : "无"}／包目录 ${r.package_dir ? "有" : "无"}）`,
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
									title: "构建全部箱子",
									consequence:
										"会对**所有**箱子重新构建镜像。这会真实占用宿主的构建资源与磁盘，通常需要较长时间，期间可能影响正在进行的赛事操作。",
									okText: "开始构建",
									danger: true,
									confirmPhrase: "构建",
								});
								if (!ok) return;
								const rows = await build.mutateAsync(undefined);
								setResult(
									(rows ?? [])
										.map((r) => `${r.is_ok ? "OK  " : "FAIL"} ${r.gamebox_name}: ${r.message}`)
										.join("\n") || "（没有返回结果）",
								);
								void query.refetch();
							}}
						>
							构建全部
						</Button>
					</div>
				</div>
			</Card>
			</Can>

			<ResourcePage<GameBoxLibraryDto>
				columns={can("gameboxes.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="箱子库是空的"
				emptyDescription="导入一个箱子包开始。"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title="编辑箱子"
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							loading={update.isPending}
							onClick={async () => {
								if (!editing) return;
								await update.mutateAsync({
									id: editing.id,
									body: {
										name: editing.name,
										category: editing.category,
										description: editing.description,
									},
								});
								Notification.success({ message: "已更新" });
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
							<span>名称</span>
							<Input
								value={editing.name}
								onChange={(e) => setEditing({ ...editing, name: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>分类</span>
							<Input
								value={editing.category}
								onChange={(e) => setEditing({ ...editing, category: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>描述</span>
							<Input
								value={editing.description}
								onChange={(e) => setEditing({ ...editing, description: e.target.value })}
							/>
						</label>
						<div className="ai-row" style={{ gap: 8 }}>
							<Switch
								checked={!editing.hidden}
								onChange={(checked) =>
									setEditing({ ...editing, hidden: !checked })
								}
							/>
							<span>对选手可见（也可直接在列表里点「可见」切换）</span>
						</div>
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
		</>
	);
}
