/**
 * 管理端 · Docker 容器 / 镜像 / 网络（`/admin/infra/docker`，specialized 能力）。
 *
 * 真实接口：`client.admin.docker.*`（全部经宿主 helper 的受限协议，API 自身无 Docker socket）。
 * 必须遵守的接口细节：
 * - 三个列表接口都用 **`offset` / `limit`** 分页（`apps/api/src/modules/platform/operations/docker.rs`），
 *   不是 `page`；`meta.total` 是真实总数。因此查询键用
 *   `[...qk.admin.dockerXxx(), params]` 扩展（qk 工厂本身无参，前缀失效仍然匹配）。
 * - 后端 Docker 接口**没有 filter 映射** ⇒ 搜索只在当前页内生效，界面上明确标注。
 * - `FloatDockerContainer.status` 来自 bollard 的 `{:?}`（`Running` / `Exited` / `Created` /
 *   `Paused` / `Restarting` / `Removing` / `Dead`），比较时必须忽略大小写；
 *   `ports` 是**字符串**（后端已拼好 `ip:public` 形式），`created` 是 Unix 秒。
 * - `NetworkInfo` 运行时**没有** `created` 字段（SDK 类型多声明了它），本页不读取。
 * - helper 未就绪 / Docker 不可达时后端返回 5xx，错误文案通过 `ErrorBlock` 原样显示。
 */

import { useState, type FormEvent, type ReactNode } from "react";
import {
	keepPreviousData,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import {
	Container,
	HardDrive,
	Network as NetworkIcon,
	Play,
	Plus,
	RefreshCw,
	Square,
	Star,
	Trash2,
} from "lucide-react";

import type {
	FloatDockerContainer,
	ImageInfo,
	NetworkInfo,
	QueryParams,
} from "@floatctf/sdk";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill, type PillTone } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	CopyText,
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
} from "~/components/app/page";
import {
	EmptyBlock,
	QueryState,
	RefreshingBadge,
	TableSkeleton,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { AbsoluteTime } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { formatBytes, truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import { SearchInput, filterCurrentPage, fromUnixSeconds, useListState } from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];

/** 容器状态（bollard Debug 输出）→ 语义色。 */
function containerTone(status: string): PillTone {
	switch (status.toLowerCase()) {
		case "running":
			return "success";
		case "paused":
		case "restarting":
		case "created":
			return "warning";
		case "exited":
		case "dead":
		case "removing":
			return "muted";
		default:
			return "neutral";
	}
}

function isRunning(status: string): boolean {
	return status.toLowerCase() === "running";
}

function offsetParams(page: number, pageSize: number): QueryParams {
	return { offset: (Math.max(1, page) - 1) * pageSize, limit: pageSize };
}

export function AdminDockerPage(): ReactNode {
	useDocumentTitle("Docker · FloatCTF 控制台");

	return (
		<PageBody>
			<PageHeader
				title="Docker"
				description="宿主容器 / 镜像 / 网络治理。所有调用经宿主 helper 的受限协议；若 helper 未就绪，接口会返回 5xx 并在下方显示后端原文。"
				badge={<TonePill tone="muted">specialized</TonePill>}
			/>

			<Tabs defaultValue="containers">
				<TabsList>
					<TabsTrigger value="containers">
						<Container className="size-4" />
						容器
					</TabsTrigger>
					<TabsTrigger value="images">
						<HardDrive className="size-4" />
						镜像
					</TabsTrigger>
					<TabsTrigger value="networks">
						<NetworkIcon className="size-4" />
						网络
					</TabsTrigger>
				</TabsList>
				<TabsContent value="containers" className="mt-4">
					<ContainersTab />
				</TabsContent>
				<TabsContent value="images" className="mt-4">
					<ImagesTab />
				</TabsContent>
				<TabsContent value="networks" className="mt-4">
					<NetworksTab />
				</TabsContent>
			</Tabs>
		</PageBody>
	);
}

// ── 容器 ────────────────────────────────────────────────────────────────────

function ContainersTab(): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const params = offsetParams(list.page, list.pageSize);

	const query = useQuery({
		queryKey: [...qk.admin.dockerContainers(), params],
		queryFn: () =>
			callList<FloatDockerContainer>(client.admin.docker.fetchContainers(params)),
		placeholderData: keepPreviousData,
	});

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.dockerContainers() });
	}

	const startMutation = useMutation({
		mutationFn: (row: FloatDockerContainer) =>
			callVoid(client.admin.docker.startContainer(row.id), "启动容器"),
		onSuccess: (_data, row) => {
			toast.success(`容器 ${row.name || row.id.slice(0, 12)} 已启动`);
			invalidate();
		},
		onError: (error) => toast.apiError("启动容器失败", error),
	});

	const stopMutation = useMutation({
		mutationFn: (row: FloatDockerContainer) =>
			callVoid(client.admin.docker.stopContainer(row.id), "停止容器"),
		onSuccess: (_data, row) => {
			toast.success(`容器 ${row.name || row.id.slice(0, 12)} 已停止`);
			invalidate();
		},
		onError: (error) => toast.apiError("停止容器失败", error),
	});

	const deleteMutation = useMutation({
		mutationFn: (row: FloatDockerContainer) =>
			callVoid(client.admin.docker.deleteContainer(row.id), "删除容器"),
		onSuccess: (_data, row) => {
			toast.success(`容器 ${row.name || row.id.slice(0, 12)} 已删除`);
			invalidate();
			void queryClient.invalidateQueries({ queryKey: qk.admin.dockerImages() });
			void queryClient.invalidateQueries({ queryKey: qk.admin.dockerNetworks() });
		},
		onError: (error) => toast.apiError("删除容器失败", error),
	});

	async function startContainer(row: FloatDockerContainer) {
		const ok = await confirm({
			title: `启动容器 ${row.name || row.id.slice(0, 12)}？`,
			description: `镜像 ${row.image}`,
			consequences: [
				"容器会以原有配置重新启动，占用宿主端口与资源",
				"若是平台管理的题目 / GameBox 实例容器，可能与实例生命周期状态不一致",
			],
			confirmText: "启动",
		});
		if (!ok) return;
		startMutation.mutate(row);
	}

	async function stopContainer(row: FloatDockerContainer) {
		const ok = await confirm({
			title: `停止容器 ${row.name || row.id.slice(0, 12)}？`,
			description: `镜像 ${row.image} · 当前状态 ${row.status}`,
			consequences: [
				"容器内进程会被终止，未持久化的数据（内存态 / 临时文件）全部丢失",
				"选手正在使用的实例环境会立即不可用",
				"容器不会自动重启（平台实例需要重新启动流程）",
			],
			tone: "danger",
			confirmText: "停止容器",
			confirmPhrase: "STOP",
		});
		if (!ok) return;
		stopMutation.mutate(row);
	}

	async function deleteContainer(row: FloatDockerContainer) {
		const ok = await confirm({
			title: `删除容器 ${row.name || row.id.slice(0, 12)}？`,
			description: `镜像 ${row.image} · 状态 ${row.status}`,
			consequences: [
				"容器会被强制删除（包括其可写层），容器内数据永久丢失",
				"若容器仍属于某个赛事实例，平台记录会与被删除的运行时资源不一致",
				"删除后不可恢复，只能依赖镜像重新创建",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: "DELETE",
		});
		if (!ok) return;
		deleteMutation.mutate(row);
	}

	const rows = query.data?.items ?? [];
	const visible = filterCurrentPage(rows, list.filter, (row) => [
		row.id,
		row.name,
		row.image,
		row.status,
	]);

	const columns: DataTableColumn<FloatDockerContainer>[] = [
		{
			id: "name",
			header: "名称",
			sortValue: (row) => row.name,
			cell: (row) => <span className="font-medium">{row.name || "—"}</span>,
		},
		{
			id: "id",
			header: "容器 ID",
			hideBelow: "md",
			cell: (row) => <CopyText value={row.id} label="容器 ID" />,
		},
		{
			id: "image",
			header: "镜像",
			hideBelow: "lg",
			cell: (row) => (
				<MonoText className="text-muted-foreground" title={row.image}>
					{truncate(row.image, 34)}
				</MonoText>
			),
		},
		{
			id: "status",
			header: "状态",
			sortValue: (row) => row.status,
			cell: (row) => <TonePill tone={containerTone(row.status)}>{row.status}</TonePill>,
		},
		{
			id: "ports",
			header: "端口",
			hideBelow: "md",
			cell: (row) => (
				<MonoText className="text-muted-foreground">{row.ports || "—"}</MonoText>
			),
		},
		{
			id: "created",
			header: "创建时间",
			align: "right",
			sortValue: (row) => row.created,
			cell: (row) => {
				const created = fromUnixSeconds(row.created);
				return created ? <AbsoluteTime value={created} /> : <span>—</span>;
			},
		},
	];

	return (
		<SectionCard
			title="容器"
			description="服务端按 offset/limit 分页；后端没有过滤接口，搜索只作用于当前页。"
			actions={
				<div className="flex flex-wrap items-center gap-2">
					<RefreshingBadge active={query.isFetching && !query.isPending} />
					<SearchInput
						value={list.search}
						onChange={list.setSearch}
						placeholder="在当前页内筛选（名称 / ID / 镜像 / 状态）"
					/>
					<Button
						variant="outline"
						size="sm"
						onClick={() => void query.refetch()}
						disabled={query.isFetching}
					>
						<RefreshCw />
						刷新
					</Button>
				</div>
			}
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={6} columns={6} />}
				errorTitle="加载容器列表失败"
				isEmpty={(result) => result.items.length === 0}
				empty={
					<EmptyBlock
						title="没有容器"
						description="宿主上还没有任何容器；若本应为空而这里有误，请检查 helper 状态。"
					/>
				}
			>
				{(result) => (
					<div className="space-y-3">
						{visible.length === 0 ? (
							<EmptyBlock
								variant="filtered"
								title="当前页没有匹配的容器"
								description="后端 Docker 接口不支持服务端过滤，请翻页或清空搜索词。"
							/>
						) : (
							<DataTable
								data={visible}
								getRowId={(row) => row.id}
								columns={columns}
								rowActions={(row) => (
									<div className="flex items-center gap-1">
										{isRunning(row.status) ? (
											<Button
												variant="ghost"
												size="icon-sm"
												aria-label="停止容器"
												title="停止容器"
												disabled={stopMutation.isPending}
												onClick={() => void stopContainer(row)}
											>
												<Square />
											</Button>
										) : (
											<Button
												variant="ghost"
												size="icon-sm"
												aria-label="启动容器"
												title="启动容器"
												disabled={startMutation.isPending}
												onClick={() => void startContainer(row)}
											>
												<Play />
											</Button>
										)}
										<Button
											variant="ghost"
											size="icon-sm"
											className="text-destructive"
											aria-label="删除容器"
											title="删除容器"
											disabled={deleteMutation.isPending}
											onClick={() => void deleteContainer(row)}
										>
											<Trash2 />
										</Button>
									</div>
								)}
								mobileCard={(row) => (
									<div className="space-y-2">
										<div className="flex items-start justify-between gap-2">
											<span className="font-medium">{row.name || "—"}</span>
											<TonePill tone={containerTone(row.status)}>{row.status}</TonePill>
										</div>
										<p className="text-xs text-muted-foreground">
											<MonoText>{truncate(row.image, 40)}</MonoText>
										</p>
										<p className="text-xs text-muted-foreground">
											端口 <MonoText>{row.ports || "—"}</MonoText>
										</p>
										<div className="flex items-center gap-2">
											{isRunning(row.status) ? (
												<Button variant="outline" size="sm" onClick={() => void stopContainer(row)}>
													<Square />
													停止
												</Button>
											) : (
												<Button variant="outline" size="sm" onClick={() => void startContainer(row)}>
													<Play />
													启动
												</Button>
											)}
											<Button
												variant="ghost"
												size="sm"
												className="text-destructive"
												onClick={() => void deleteContainer(row)}
											>
												<Trash2 />
												删除
											</Button>
										</div>
									</div>
								)}
							/>
						)}
						<PaginationBar
							page={list.page}
							pageSize={list.pageSize}
							total={result.meta.total ?? rows.length}
							onPageChange={list.setPage}
							onPageSizeChange={list.setPageSize}
							pageSizeOptions={PAGE_SIZE_OPTIONS}
						/>
					</div>
				)}
			</QueryState>
		</SectionCard>
	);
}

// ── 镜像 ────────────────────────────────────────────────────────────────────

function ImagesTab(): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const params = offsetParams(list.page, list.pageSize);

	const query = useQuery({
		queryKey: [...qk.admin.dockerImages(), params],
		queryFn: () => callList<ImageInfo>(client.admin.docker.fetchImages(params)),
		placeholderData: keepPreviousData,
	});

	const deleteMutation = useMutation({
		mutationFn: (row: ImageInfo) =>
			callVoid(client.admin.docker.deleteImage(row.id), "删除镜像"),
		onSuccess: () => {
			toast.success("镜像已删除");
			void queryClient.invalidateQueries({ queryKey: qk.admin.dockerImages() });
		},
		onError: (error) => toast.apiError("删除镜像失败", error),
	});

	async function deleteImage(row: ImageInfo) {
		const label = row.repo_tags.length > 0 ? row.repo_tags.join(", ") : row.id.slice(0, 12);
		const ok = await confirm({
			title: `删除镜像 ${truncate(label, 40)}？`,
			description: `镜像 ID ${row.id.slice(0, 12)} · 大小 ${formatBytes(row.size)}`,
			consequences: [
				"依赖该镜像的容器将无法再创建 / 重启（正在运行的容器不受影响）",
				"重新拉起需要重新拉取或重建镜像，恢复依赖网络与构建时间",
				"删除后不可恢复",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: "DELETE",
		});
		if (!ok) return;
		deleteMutation.mutate(row);
	}

	const rows = query.data?.items ?? [];
	const visible = filterCurrentPage(rows, list.filter, (row) => [row.id, ...row.repo_tags]);

	const columns: DataTableColumn<ImageInfo>[] = [
		{
			id: "repo_tags",
			header: "标签",
			cell: (row) =>
				row.repo_tags.length === 0 ? (
					<TonePill tone="muted">&lt;none&gt;</TonePill>
				) : (
					<div className="flex flex-wrap gap-1">
						{row.repo_tags.map((tag) => (
							<MonoText key={tag} className="rounded bg-muted px-1.5 py-0.5">
								{tag}
							</MonoText>
						))}
					</div>
				),
		},
		{
			id: "id",
			header: "镜像 ID",
			hideBelow: "md",
			cell: (row) => <MonoText className="text-muted-foreground">{row.id.slice(0, 24)}</MonoText>,
		},
		{
			id: "size",
			header: "大小",
			align: "right",
			sortValue: (row) => row.size,
			cell: (row) => <MonoText>{formatBytes(row.size)}</MonoText>,
		},
		{
			id: "created",
			header: "创建时间",
			align: "right",
			sortValue: (row) => row.created,
			cell: (row) => {
				const created = fromUnixSeconds(row.created);
				return created ? <AbsoluteTime value={created} /> : <span>—</span>;
			},
		},
	];

	return (
		<SectionCard
			title="镜像"
			description="服务端按 offset/limit 分页；搜索只作用于当前页。"
			actions={
				<div className="flex flex-wrap items-center gap-2">
					<RefreshingBadge active={query.isFetching && !query.isPending} />
					<SearchInput
						value={list.search}
						onChange={list.setSearch}
						placeholder="在当前页内筛选（标签 / ID）"
					/>
					<Button
						variant="outline"
						size="sm"
						onClick={() => void query.refetch()}
						disabled={query.isFetching}
					>
						<RefreshCw />
						刷新
					</Button>
				</div>
			}
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={5} columns={4} />}
				errorTitle="加载镜像列表失败"
				isEmpty={(result) => result.items.length === 0}
				empty={
					<EmptyBlock
						title="没有镜像"
						description="宿主上还没有任何镜像；平台镜像通常由构建 / 拉取流程写入。"
					/>
				}
			>
				{(result) => (
					<div className="space-y-3">
						{visible.length === 0 ? (
							<EmptyBlock
								variant="filtered"
								title="当前页没有匹配的镜像"
								description="后端 Docker 接口不支持服务端过滤，请翻页或清空搜索词。"
							/>
						) : (
							<DataTable
								data={visible}
								getRowId={(row) => row.id}
								columns={columns}
								rowActions={(row) => (
									<Button
										variant="ghost"
										size="icon-sm"
										className="text-destructive"
										aria-label="删除镜像"
										title="删除镜像"
										disabled={deleteMutation.isPending}
										onClick={() => void deleteImage(row)}
									>
										<Trash2 />
									</Button>
								)}
								mobileCard={(row) => (
									<div className="space-y-2">
										<p className="text-sm">
											{row.repo_tags.length > 0 ? (
												<MonoText>{row.repo_tags.join(", ")}</MonoText>
											) : (
												<TonePill tone="muted">&lt;none&gt;</TonePill>
											)}
										</p>
										<p className="text-xs text-muted-foreground">
											<MonoText>{row.id.slice(0, 24)}</MonoText> · {formatBytes(row.size)}
										</p>
										<Button
											variant="ghost"
											size="sm"
											className="text-destructive"
											onClick={() => void deleteImage(row)}
										>
											<Trash2 />
											删除
										</Button>
									</div>
								)}
							/>
						)}
						<PaginationBar
							page={list.page}
							pageSize={list.pageSize}
							total={result.meta.total ?? rows.length}
							onPageChange={list.setPage}
							onPageSizeChange={list.setPageSize}
							pageSizeOptions={PAGE_SIZE_OPTIONS}
						/>
					</div>
				)}
			</QueryState>
		</SectionCard>
	);
}

// ── 网络 ────────────────────────────────────────────────────────────────────

function NetworksTab(): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [creating, setCreating] = useState(false);
	const params = offsetParams(list.page, list.pageSize);

	const query = useQuery({
		queryKey: [...qk.admin.dockerNetworks(), params],
		queryFn: () => callList<NetworkInfo>(client.admin.docker.fetchNetworks(params)),
		placeholderData: keepPreviousData,
	});

	const deleteMutation = useMutation({
		mutationFn: (row: NetworkInfo) =>
			callVoid(client.admin.docker.deleteNetwork(row.id), "删除网络"),
		onSuccess: () => {
			toast.success("网络已删除");
			void queryClient.invalidateQueries({ queryKey: qk.admin.dockerNetworks() });
		},
		onError: (error) => toast.apiError("删除网络失败", error),
	});

	async function deleteNetwork(row: NetworkInfo) {
		const ok = await confirm({
			title: `删除网络 ${row.name}？`,
			description: `驱动 ${row.driver} · 作用域 ${row.scope} · 子网 ${row.subnet ?? "—"}`,
			consequences: [
				"已连接到该网络的容器会失去网络连通性（跨容器通信与网关访问中断）",
				"Docker 拒绝删除仍有活动端点的网络，届时后端会返回具体错误",
				"删除后不可恢复，需要重建网络并重新连接容器",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: "DELETE",
		});
		if (!ok) return;
		deleteMutation.mutate(row);
	}

	const rows = query.data?.items ?? [];
	const visible = filterCurrentPage(rows, list.filter, (row) => [
		row.id,
		row.name,
		row.driver,
		row.scope,
		row.subnet ?? "",
		row.gateway ?? "",
	]);

	const columns: DataTableColumn<NetworkInfo>[] = [
		{
			id: "name",
			header: "名称",
			sortValue: (row) => row.name,
			cell: (row) => <span className="font-medium">{row.name}</span>,
		},
		{
			id: "driver",
			header: "驱动",
			sortValue: (row) => row.driver,
			cell: (row) => <TonePill tone="neutral">{row.driver}</TonePill>,
		},
		{
			id: "scope",
			header: "作用域",
			hideBelow: "sm",
			sortValue: (row) => row.scope,
			cell: (row) => <span>{row.scope}</span>,
		},
		{
			id: "subnet",
			header: "子网",
			hideBelow: "md",
			cell: (row) => <MonoText className="text-muted-foreground">{row.subnet ?? "—"}</MonoText>,
		},
		{
			id: "gateway",
			header: "网关",
			hideBelow: "lg",
			cell: (row) => <MonoText className="text-muted-foreground">{row.gateway ?? "—"}</MonoText>,
		},
		{
			id: "id",
			header: "网络 ID",
			hideBelow: "xl",
			cell: (row) => <CopyText value={row.id} label="网络 ID" />,
		},
	];

	return (
		<>
			<SectionCard
				title="网络"
				description="服务端按 offset/limit 分页；搜索只作用于当前页。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="在当前页内筛选（名称 / 子网）"
						/>
						<Button
							variant="outline"
							size="sm"
							onClick={() => void query.refetch()}
							disabled={query.isFetching}
						>
							<RefreshCw />
							刷新
						</Button>
						<Button size="sm" onClick={() => setCreating(true)}>
							<Plus />
							新建网络
						</Button>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={5} columns={5} />}
					errorTitle="加载网络列表失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="没有网络"
							description="宿主上还没有自定义网络；平台靶场网络通常由赛事流程创建。"
						/>
					}
				>
					{(result) => (
						<div className="space-y-3">
							{visible.length === 0 ? (
								<EmptyBlock
									variant="filtered"
									title="当前页没有匹配的网络"
									description="后端 Docker 接口不支持服务端过滤，请翻页或清空搜索词。"
								/>
							) : (
								<DataTable
									data={visible}
									getRowId={(row) => row.id}
									columns={columns}
									rowActions={(row) => (
										<Button
											variant="ghost"
											size="icon-sm"
											className="text-destructive"
											aria-label="删除网络"
											title="删除网络"
											disabled={deleteMutation.isPending}
											onClick={() => void deleteNetwork(row)}
										>
											<Trash2 />
										</Button>
									)}
									mobileCard={(row) => (
										<div className="space-y-2">
											<div className="flex items-start justify-between gap-2">
												<span className="font-medium">{row.name}</span>
												<TonePill tone="neutral">{row.driver}</TonePill>
											</div>
											<p className="text-xs text-muted-foreground">
												<MonoText>{row.subnet ?? "—"}</MonoText> · 网关{" "}
												<MonoText>{row.gateway ?? "—"}</MonoText>
											</p>
											<Button
												variant="ghost"
												size="sm"
												className="text-destructive"
												onClick={() => void deleteNetwork(row)}
											>
												<Trash2 />
												删除
											</Button>
										</div>
									)}
								/>
							)}
							<PaginationBar
								page={list.page}
								pageSize={list.pageSize}
								total={result.meta.total ?? rows.length}
								onPageChange={list.setPage}
								onPageSizeChange={list.setPageSize}
								pageSizeOptions={PAGE_SIZE_OPTIONS}
							/>
						</div>
					)}
				</QueryState>
			</SectionCard>

			<CreateNetworkSheet open={creating} onOpenChange={setCreating} />
		</>
	);
}

function CreateNetworkSheet({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState({
		name: "",
		subnet: "",
		gateway: "",
		driver: "bridge",
	});
	const [errors, setErrors] = useState<{ name?: string; subnet?: string; gateway?: string }>({});

	const createMutation = useMutation({
		mutationFn: (input: { name: string; subnet: string; gateway: string; driver?: string }) =>
			call(client.admin.docker.createNetwork(input), "创建网络"),
		onSuccess: () => {
			toast.success("网络已创建", "容器可按名称接入该网络。");
			void queryClient.invalidateQueries({ queryKey: qk.admin.dockerNetworks() });
			setForm({ name: "", subnet: "", gateway: "", driver: "bridge" });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("创建网络失败", error),
	});

	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const next: { name?: string; subnet?: string; gateway?: string } = {};
		if (form.name.trim().length === 0) next.name = "网络名称不能为空";
		if (form.subnet.trim().length === 0) next.subnet = "子网不能为空（例如 172.20.0.0/16）";
		if (form.gateway.trim().length === 0) next.gateway = "网关不能为空（例如 172.20.0.1）";
		setErrors(next);
		if (Object.keys(next).length > 0) return;
		createMutation.mutate({
			name: form.name.trim(),
			subnet: form.subnet.trim(),
			gateway: form.gateway.trim(),
			driver: form.driver.trim() || undefined,
		});
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title="新建 Docker 网络"
			description="创建自定义 bridge 网络（带显式子网与网关）。名称 / 子网冲突时后端会返回 Docker 原文错误。"
			width="lg"
			footer={
				<FormFooter
					formId="docker-network-form"
					isPending={createMutation.isPending}
					submitLabel="创建"
					onCancel={() => onOpenChange(false)}
					hint="名称、子网、网关为必填项"
				/>
			}
		>
			<form id="docker-network-form" className="space-y-4" onSubmit={submit}>
				<Field label="名称" htmlFor="network-name" required error={errors.name}>
					<Input
						id="network-name"
						value={form.name}
						onChange={(event) => setForm({ ...form, name: event.target.value })}
						placeholder="例如 fctf-awd-net"
					/>
				</Field>
				<FormGrid columns={2}>
					<Field label="子网" htmlFor="network-subnet" required error={errors.subnet}>
						<Input
							id="network-subnet"
							className="font-mono text-xs"
							value={form.subnet}
							onChange={(event) => setForm({ ...form, subnet: event.target.value })}
							placeholder="172.20.0.0/16"
						/>
					</Field>
					<Field label="网关" htmlFor="network-gateway" required error={errors.gateway}>
						<Input
							id="network-gateway"
							className="font-mono text-xs"
							value={form.gateway}
							onChange={(event) => setForm({ ...form, gateway: event.target.value })}
							placeholder="172.20.0.1"
						/>
					</Field>
				</FormGrid>
				<Field label="驱动" htmlFor="network-driver" hint="默认 bridge。">
					<Input
						id="network-driver"
						className="font-mono text-xs"
						value={form.driver}
						onChange={(event) => setForm({ ...form, driver: event.target.value })}
					/>
				</Field>
				<div className="flex items-start gap-2 rounded-md border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
					<Star className="mt-0.5 size-3.5 shrink-0" />
					<span>
						子网必须与宿主现有网络不重叠；创建成功后可被赛事 / GameBox 分配流程引用。
					</span>
				</div>
			</form>
		</FormSheet>
	);
}
