/**
 * 前端管理。
 *
 * ## 为什么这里用的是 `fetch` 而不是 SDK
 *
 * 平台**没有**把前端注册表与引导元数据暴露在公共 SDK 里：
 *   - `GET /__floatctf/frontends/registry.json`（只读静态文件，无鉴权）
 *   - `GET /api/frontend`（引导元数据，无鉴权）
 * SDK 里既没有 `frontend` 模块，也没有对应方法。
 *
 * PUBLIC SDK GAP: 前端注册表 / 引导元数据未纳入 `@floatctf/sdk`。
 * 解决方法**不是**改后端或改上游包，而是走 AI-FRONTEND-GUIDE 允许的
 * **同源静态资源逃逸口**：用裸 `fetch` 读取同源注册表，并复用平台自己的
 * 权威校验器 `parseRegistry` 解析 —— 这样"前端看到的注册表"和
 * "平台实际加载的注册表"是同一份、用同一套规则校验的，不存在第二真相源。
 *
 * 激活则**确实**有 SDK 路径：写 `FRONTEND_ACTIVE` 设置（走 `settings` API），
 * 因为激活在平台语义上就是一条动态设置。
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button, Card, Divider, Tag, Tooltip } from "animal-island-ui";
import {
	AlertTriangleIcon,
	CheckCircleIcon,
	GlobeIcon,
	RefreshIcon,
	ShieldIcon,
} from "naive-icons";
import { DEFAULT_REGISTRY_URL, parseRegistry } from "@floatctf/frontend-runtime";
import { SettingValueType } from "@floatctf/sdk/entity";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { StatCard } from "~/components/resource";
import { formatTime } from "~/lib/event";

const ACTIVE_KEY = "FRONTEND_ACTIVE";

export function ConsoleFrontends() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [switching, setSwitching] = useState<string | null>(null);

	// ── 注册表：同源静态文件 + 平台权威校验器（见文件头说明）──
	const registry = useQuery({
		queryKey: ["console", "frontend-registry"],
		staleTime: 0, // 刚装完的前端要立刻可见
		queryFn: async () => {
			const response = await fetch(DEFAULT_REGISTRY_URL, {
				headers: { Accept: "application/json" },
				cache: "no-store",
				credentials: "same-origin",
			});
			if (!response.ok) {
				throw new Error(`读取前端注册表失败：HTTP ${response.status}（${DEFAULT_REGISTRY_URL}）`);
			}
			const parsed = parseRegistry(await response.json());
			if (!parsed.ok) {
				throw new Error(`注册表不合法：${parsed.errors.join("；")}`);
			}
			return parsed.registry;
		},
	});

	// ── 当前激活项：它就是一条动态设置，因此走真实 SDK ──
	const settings = useUniQuery(
		["console", "settings"],
		() => runtime.client.admin.settings.fetch(),
		"加载设置",
	);

	const setActive = useUniMutation(
		(vars: { id: string; key: string; value: string }) =>
			runtime.client.admin.settings.patch({ id: vars.id, key: vars.key, value: vars.value }),
		"切换前端",
	);

	const activeRow = (settings.data ?? []).find((s) => s.key === ACTIVE_KEY);
	const activeValue = activeRow?.value ?? "default";

	if (registry.isLoading || settings.isLoading) return <SkeletonCards count={4} />;
	if (registry.error) {
		return (
			<ErrorState
				error={registry.error}
				onRetry={() => void registry.refetch()}
			/>
		);
	}
	if (settings.error) {
		return <ErrorState error={settings.error} onRetry={() => void settings.refetch()} />;
	}

	const reg = registry.data;
	const frontends = Object.values(reg?.frontends ?? {}).sort((a, b) =>
		a.id.localeCompare(b.id),
	);
	const versionCount = frontends.reduce(
		(n, f) => n + Object.keys(f.versions ?? {}).length,
		0,
	);

	async function activate(id: string, version: string) {
		const ok = await confirm({
			title: `把「${id}」设为当前前端`,
			consequence: (
				<span>
					这会把平台前端切换到 <strong>{id}</strong> 的 <strong>{version}</strong> 版本。
					**所有用户的下一次页面加载**都会使用新前端；如果新前端的制品有问题，
					用户可能看到白屏。应急入口是给任意页面加上 <code>?frontend=default</code>。
				</span>
			),
			okText: "切换前端",
			danger: true,
		});
		if (!ok) return;

		setSwitching(id);
		try {
			if (activeRow) {
				await setActive.mutateAsync({ id: activeRow.id, key: ACTIVE_KEY, value: id });
			} else {
				// 没有这一行时创建它（后端对 key 有安全 ID 校验）。
				await runtime.client.admin.settings.create({
					key: ACTIVE_KEY,
					value: id,
					type: SettingValueType.String,
					description: "当前激活的前端 ID（由控制台前端管理写入）",
				});
			}
			void settings.refetch();
		} finally {
			setSwitching(null);
		}
	}

	return (
		<>
			<PageHeader
				title="前端管理"
				description="已安装的前端与当前激活项。注册表来自同源静态文件，用平台自己的校验器解析。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => {
							void registry.refetch();
							void settings.refetch();
						}}
					>
						刷新
					</Button>
				}
			/>

			<div className="ai-grid">
				<StatCard label="已装前端" value={frontends.length} icon={<GlobeIcon size={18} />} />
				<StatCard label="版本总数" value={versionCount} />
				<StatCard
					label="当前激活"
					value={<span style={{ fontSize: 16 }}>{activeValue}</span>}
					hint={activeRow ? "来自 FRONTEND_ACTIVE 设置" : "设置缺失，回落 default"}
				/>
			</div>

			{activeRow?.protected ? (
				<Card>
					<div className="ai-row" style={{ gap: 8 }}>
						<ShieldIcon size={16} />
						<span className="ai-muted">
							`FRONTEND_ACTIVE` 是**受保护设置**，后端会拒绝通过设置接口修改 ——
							下方按钮在这种情况下会收到错误。这是刻意的：前端激活属于平台引导不变式。
						</span>
					</div>
				</Card>
			) : null}

			{frontends.length === 0 ? (
				<Card>
					<EmptyState
						title="注册表里没有前端"
						description={`${DEFAULT_REGISTRY_URL} 可读，但里面没有任何条目。`}
					/>
				</Card>
			) : (
				<div className="ai-col" style={{ gap: 12 }}>
					{frontends.map((f) => {
						const versions = Object.values(f.versions ?? {}).sort((a, b) =>
							b.installedAt.localeCompare(a.installedAt),
						);
						const isActive = f.id === activeValue;
						return (
							<Card key={f.id}>
								<div className="ai-col" style={{ gap: 10 }}>
									<div className="ai-row-between">
										<div className="ai-row" style={{ gap: 8 }}>
											<span className="ai-strong" style={{ fontSize: 16 }}>
												{f.id}
											</span>
											{isActive ? (
												<Tag size="small" variant="solid" color="app-green">
													<CheckCircleIcon size={12} /> 当前激活
												</Tag>
											) : null}
											{f.protected ? (
												<Tooltip title="平台前端：常规前端管理不得移除">
													<Tag size="small" variant="soft" color="brown">
														内置
													</Tag>
												</Tooltip>
											) : null}
										</div>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											当前版本 {f.currentVersion} ｜ {versions.length} 个版本
										</span>
									</div>
									<Divider />
									<div className="ai-col" style={{ gap: 6 }}>
										{versions.map((v) => (
											<div key={v.version} className="ai-row-between">
												<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
													<span className="ai-row" style={{ gap: 6 }}>
														<span className="ai-strong">{v.name || f.id}</span>
														<Tag size="small" variant="outlined">
															{v.version}
														</Tag>
														{v.version === f.currentVersion ? (
															<Tag size="small" variant="soft" color="app-teal">
																显式当前
															</Tag>
														) : null}
														{!v.compatibility?.apiContract ? (
															<Tag size="small" variant="soft" color="app-red">
																<AlertTriangleIcon size={12} /> 未声明契约版本
															</Tag>
														) : (
															<Tag size="small" variant="outlined">
																契约 {v.compatibility.apiContract}
															</Tag>
														)}
													</span>
													<span className="ai-muted" style={{ fontSize: 12 }}>
														{v.description || "（没有描述）"}
														{v.author ? ` ｜ ${v.author}` : ""}
														{" ｜ 安装于 "}
														{formatTime(v.installedAt)}
													</span>
												</div>
												<Can permission="settings.manage">
												<Button
													size="small"
													type={isActive ? "default" : "primary"}
													disabled={isActive || switching !== null}
													loading={switching === f.id}
													onClick={() => void activate(f.id, v.version)}
												>
													{isActive ? "使用中" : "设为当前"}
												</Button>
												</Can>
											</div>
										))}
									</div>
								</div>
							</Card>
						);
					})}
				</div>
			)}

			<Card>
				<div className="ai-col" style={{ gap: 6 }}>
					<span className="ai-strong">应急入口</span>
					<span className="ai-muted">
						如果切换后前端无法加载，给任意页面地址加上{" "}
						<span className="ai-mono">?frontend=default</span>{" "}
						即可临时回落到内置前端（不影响激活设置本身）。
					</span>
				</div>
			</Card>
		</>
	);
}
