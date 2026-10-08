/**
 * 管理端 · 平台版本（`/admin/version`）。
 *
 * 真实接口：`client.admin.system.version()`（`GET /api/admin/system/version`，返回字符串）。
 * 其它数字是**编译期契约常量**（`@floatctf/frontend-runtime`），不是接口数据，
 * 因此单独标注来源，绝不冒充后端返回值。
 */

import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { AppWindow, Info, RefreshCw, Server } from "lucide-react";

import {
	API_CONTRACT_VERSION,
	FRONTEND_RUNTIME_FULL_VERSION,
	FRONTEND_RUNTIME_VERSION,
} from "@floatctf/frontend-runtime";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import {
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
	Toolbar,
} from "~/components/app/page";
import { ErrorBlock, LoadingBlock } from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { useDocumentTitle } from "~/lib/hooks";

export function AdminVersionPage(): ReactNode {
	useDocumentTitle("平台版本 · FloatCTF 控制台");
	const client = useClient();

	const query = useQuery({
		queryKey: qk.admin.version(),
		queryFn: () => call<string>(client.admin.system.version(), "平台版本"),
		// 版本号几乎不变：低频缓存足够。
		staleTime: 5 * 60_000,
	});

	return (
		<PageBody>
			<PageHeader
				title="平台版本"
				description="后端 API 版本与前端运行时契约版本。契约版本用于判断可插拔前端是否兼容。"
				actions={
					<Toolbar>
						<Button
							variant="outline"
							size="sm"
							onClick={() => void query.refetch()}
							disabled={query.isFetching}
						>
							<RefreshCw />
							刷新
						</Button>
					</Toolbar>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard
					label="平台 API"
					value={query.isPending ? "…" : query.isError ? "—" : (query.data ?? "—")}
					hint="GET /api/admin/system/version"
					icon={<Server className="size-5" />}
					tone={query.isError ? "danger" : "default"}
				/>
				<StatCard
					label="前端运行时契约"
					value={FRONTEND_RUNTIME_VERSION}
					hint="FRONTEND_RUNTIME_VERSION（编译期常量）"
					icon={<AppWindow className="size-5" />}
				/>
				<StatCard
					label="API 契约"
					value={API_CONTRACT_VERSION}
					hint="API_CONTRACT_VERSION（编译期常量）"
					icon={<Info className="size-5" />}
				/>
				<StatCard
					label="运行时完整版本"
					value={FRONTEND_RUNTIME_FULL_VERSION}
					hint="FRONTEND_RUNTIME_FULL_VERSION（仅诊断展示）"
				/>
			</div>

			<SectionCard
				title="后端 API 版本"
				description="来自真实接口；失败时显示后端文案并可重试。"
			>
				{query.isPending ? (
					<LoadingBlock label="查询版本…" />
				) : query.isError ? (
					<ErrorBlock
						error={query.error}
						title="查询平台版本失败"
						onRetry={() => void query.refetch()}
					/>
				) : (
					<KeyValueList
						columns={1}
						items={[
							{
								key: "floatctf-api",
								value: <MonoText className="text-sm">{query.data ?? "（后端未返回版本号）"}</MonoText>,
							},
						]}
					/>
				)}
			</SectionCard>

			<SectionCard
				title="契约版本说明"
				description="三个版本概念互相独立，前端与注册表的 compatibility 必须与运行时 / API 契约 major 一致。"
			>
				<KeyValueList
					columns={1}
					items={[
						{
							key: "FRONTEND_RUNTIME_VERSION",
							value: <MonoText className="text-sm">{FRONTEND_RUNTIME_VERSION}</MonoText>,
							hint: "破坏 frontend.json 或 mount(context) 契约时变更",
						},
						{
							key: "API_CONTRACT_VERSION",
							value: <MonoText className="text-sm">{API_CONTRACT_VERSION}</MonoText>,
							hint: "破坏前端可见的 HTTP API（删字段 / 改语义）时变更",
						},
						{
							key: "FRONTEND_RUNTIME_FULL_VERSION",
							value: <MonoText className="text-sm">{FRONTEND_RUNTIME_FULL_VERSION}</MonoText>,
							hint: "含 minor/patch，仅用于诊断展示",
						},
					]}
				/>
			</SectionCard>
		</PageBody>
	);
}
