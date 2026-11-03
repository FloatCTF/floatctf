/**
 * 账号详情 —— 角色授予。
 *
 * 这是 v2 平台 RBAC 的主要人工入口，因此有三条刻意的设计：
 *
 * 1. **整体替换语义**：`setRoles` 发送的是**完整的角色 id 集合**，不是增量。
 *    增量语义下"先加后删"与"先删后加"结果不同，会造出短暂的权限空洞或超额授权窗口。
 *    界面上因此是"勾选框 + 保存"，而不是"每行一个开关立即生效"。
 * 2. **反提权交给后端**：你不能授予（或保留）自己没有的权限。前端**不预判** ——
 *    预判会随权限变化而过期，还会给出误导性的"你可以"。界面只如实展示 403。
 * 3. **全权角色显式标注**：`grants_all` 角色的 `permissions` 行集合是空的
 *    （权限来自短路），不提就会看起来像个"什么都不能做"的空角色。
 */
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Avatar,
	Button,
	Card,
	Checkbox,
	Divider,
	Loading,
	Notification,
	Tag,
	Tooltip,
} from "animal-island-ui";
import { ArrowLeftIcon, RefreshIcon, SaveIcon, ShieldIcon, UserIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState } from "~/components/states";
import { useSession } from "~/auth/store";

export function ConsoleUserDetail() {
	const { userId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();
	const myId = useSession((s) => s.me?.id);
	const myPermissions = useSession((s) => s.me?.permissions);
	const myGrantsAll = useSession((s) => s.me?.grants_all);

	const [selected, setSelected] = useState<Set<string> | null>(null);

	const mine = useUniQuery(
		["console", "user", userId, "roles"],
		() => runtime.client.admin.users.getRoles(userId),
		"加载账号角色",
	);
	const allRoles = useUniQuery(
		["console", "roles"],
		() => runtime.client.admin.roles.fetch(),
		"加载角色列表",
	);

	// 服务端值到达后初始化勾选集合（之后由用户控制，不再被覆盖）。
	useEffect(() => {
		if (selected === null && mine.data) {
			setSelected(new Set(mine.data.roles.map((r) => r.id)));
		}
	}, [mine.data, selected]);

	const save = useUniMutation(
		(vars: { userId: string; roleIds: string[] }) =>
			runtime.client.admin.users.setRoles(vars.userId, vars.roleIds),
		"保存角色",
	);

	/** 勾选集合对应的权限并集（仅用于**预览**，不是授权判定）。 */
	const preview = useMemo(() => {
		if (!selected || !allRoles.data) return { permissions: [] as string[], grantsAll: false };
		const set = new Set<string>();
		let grantsAll = false;
		for (const role of allRoles.data) {
			if (!selected.has(role.id)) continue;
			if (role.grants_all) grantsAll = true;
			for (const p of role.permissions ?? []) set.add(p);
		}
		return { permissions: [...set].sort(), grantsAll };
	}, [selected, allRoles.data]);

	if (mine.isLoading || allRoles.isLoading) return <Loading active tip="加载账号角色…" />;
	if (mine.error) return <ErrorState error={mine.error} onRetry={() => void mine.refetch()} />;
	if (allRoles.error) {
		return <ErrorState error={allRoles.error} onRetry={() => void allRoles.refetch()} />;
	}
	if (!mine.data) {
		return (
			<Card>
				<EmptyState
					title="账号不存在"
					description="它可能已被删除。"
					action={<Button onClick={() => void navigate("/console/users")}>回用户列表</Button>}
				/>
			</Card>
		);
	}

	const u = mine.data;
	const isSelf = myId === u.user_id;
	const current = new Set(u.roles.map((r) => r.id));
	const dirty =
		selected !== null &&
		(selected.size !== current.size || [...selected].some((id) => !current.has(id)));

	return (
		<>
			<PageHeader
				title={u.username}
				description="授予或收回平台角色。保存时发送的是完整的角色集合。"
				extra={
					<>
						<Button
							size="small"
							icon={<ArrowLeftIcon size={16} />}
							onClick={() => void navigate("/console/users")}
						>
							回列表
						</Button>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								setSelected(null);
								void mine.refetch();
								void allRoles.refetch();
							}}
						>
							刷新
						</Button>
						<Can permission="roles.manage">
						<Button
							size="small"
							type="primary"
							icon={<SaveIcon size={16} />}
							disabled={!dirty}
							loading={save.isPending}
							onClick={async () => {
								if (!selected) return;
								const added = [...selected].filter((id) => !current.has(id));
								const removed = [...current].filter((id) => !selected.has(id));
								const nameOf = (id: string) =>
									allRoles.data?.find((r) => r.id === id)?.name ?? id;

								// 收回权限比授予更危险（可能立刻把人锁在门外），因此说清方向与数量。
								const ok = await confirm({
									title: `保存「${u.username}」的角色`,
									consequence: (
										<span>
											将<strong>整体替换</strong>该账号的角色集合：
											新增 {added.length} 个
											{added.length > 0 ? `（${added.map(nameOf).join("、")}）` : ""}，
											移除 {removed.length} 个
											{removed.length > 0 ? `（${removed.map(nameOf).join("、")}）` : ""}。
											{removed.length > 0
												? "被移除的权限会立刻失效（后端每请求重新解析权限）。"
												: ""}
											{isSelf
												? " 你正在修改自己的角色 —— 如果移除了自己的管理权限，你会立刻失去对管理端的访问。"
												: ""}
										</span>
									),
									okText: "保存",
									danger: removed.length > 0,
								});
								if (!ok) return;

								try {
									await save.mutateAsync({
										userId: u.user_id,
										roleIds: [...selected],
									});
									Notification.success({ message: "角色已保存" });
									void mine.refetch();
								} catch {
									// 反提权失败会返回 403，已由统一的 onForbidden 提示。
									// 这里**不**回滚本地勾选：让用户看到自己尝试了什么，
									// 而不是界面悄悄变回去（那会让人以为保存成功了）。
								}
							}}
						>
							保存
						</Button>
						</Can>
					</>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 16 }}>
					<Avatar size={56}>{u.username[0]}</Avatar>
					<div className="ai-col" style={{ gap: 2 }}>
						<span className="ai-strong" style={{ fontSize: 18 }}>
							{u.username}
						</span>
						<span className="ai-muted ai-mono" style={{ fontSize: 12 }}>
							{u.user_id}
						</span>
						{isSelf ? (
							<Tag size="small" variant="soft" color="app-yellow">
								<UserIcon size={12} /> 这是你自己的账号
							</Tag>
						) : null}
					</div>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row-between">
						<span className="ai-strong" style={{ fontSize: 18 }}>
							角色
						</span>
						<Tag size="small" variant="outlined">
							当前 {current.size} 个 / 共 {allRoles.data?.length ?? 0} 个
						</Tag>
					</div>
					<Divider />

					{(allRoles.data?.length ?? 0) === 0 ? (
						<EmptyState
							title="还没有可授予的角色"
							description="先去「角色与权限」创建一个。"
							action={
								<Button
									type="primary"
									onClick={() => void navigate("/console/roles")}
								>
									去创建角色
								</Button>
							}
						/>
					) : (
						// 库的 Checkbox 是**分组**组件（value 数组 + options），不是单个勾选框。
						// 正好匹配"整体替换"的角色集合语义；label 支持 ReactNode，
						// 因此角色元信息可以直接嵌进选项里。
						<Checkbox
							direction="vertical"
							value={[...(selected ?? [])]}
							options={(allRoles.data ?? []).map((role) => ({
								value: role.id,
								label: (
									<div className="ai-row-between" style={{ width: "100%", gap: 12 }}>
										<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
											<span className="ai-row" style={{ gap: 6 }}>
												<span className="ai-strong">{role.name}</span>
												<Tag size="small" variant="outlined">
													{role.key}
												</Tag>
												{role.protected ? (
													<Tag size="small" variant="soft" color="brown">
														内置
													</Tag>
												) : null}
												{role.grants_all ? (
													<Tooltip title="拥有全部权限，且自动包含将来新增的权限">
														<Tag size="small" variant="solid" color="app-red">
															<ShieldIcon size={12} /> 全部权限
														</Tag>
													</Tooltip>
												) : null}
											</span>
											{role.description ? (
												<span className="ai-muted" style={{ fontSize: 12 }}>
													{role.description}
												</span>
											) : null}
										</div>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											{role.user_count} 个账号持有
										</span>
									</div>
								),
							}))}
							onChange={(values) => setSelected(new Set(values.map(String)))}
						/>
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">权限预览</span>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						按当前勾选**推算**的结果，仅供预览。真正的判定永远在服务端；
						而且后端有一条前端无法替代的规则：**你不能授予或保留自己没有的权限**。
					</span>
					<Divider />
					{preview.grantsAll ? (
						<Tag size="small" variant="solid" color="app-red">
							全权（含将来新增的权限）
						</Tag>
					) : preview.permissions.length === 0 ? (
						<span className="ai-muted">这个账号不会有任何平台权限。</span>
					) : (
						<div className="ai-row" style={{ gap: 6 }}>
							{preview.permissions.map((p) => {
								// 标出"你没有、因此后端会拒绝"的项，让失败可预期而不是意外。
								const iHave = Boolean(myGrantsAll) || (myPermissions ?? []).includes(p);
								return (
									<Tooltip
										key={p}
										title={iHave ? p : "你没有这项权限 —— 后端会拒绝这次授予"}
									>
										<Tag
											size="small"
											variant={iHave ? "outlined" : "soft"}
											color={iHave ? "default" : "app-red"}
										>
											{p}
										</Tag>
									</Tooltip>
								);
							})}
						</div>
					)}
					{!myGrantsAll && preview.permissions.some((p) => !(myPermissions ?? []).includes(p)) ? (
						<span className="ai-muted" style={{ fontSize: 12 }}>
							红标的权限你自己没有，保存时后端会拒绝 —— 这是**反提权**规则，不是故障。
						</span>
					) : null}
				</div>
			</Card>
		</>
	);
}
