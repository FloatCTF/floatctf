/**
 * AWD 驾驶舱的域内组件：状态胶囊 / flag 提交 / GameBox 表 / 实时积分榜 / 网络凭据。
 *
 * 每个组件自己持有 query 与 mutation，页面只负责布局 —— 但**所有** query key 都来自
 * `~/api/keys` 的 `qk.awd.*`（与 `@floatctf/react` 的 SSE 失效常量一致），
 * 否则事件到达时面板不会刷新。
 */

import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, RefreshCw, RotateCcw, ShieldAlert, Swords, TerminalSquare } from "lucide-react";

import type {
	AwdGameBox,
	AwdPlayerStatus,
	AwdScoreRow,
	SshAccessResponse,
	WireGuardConfigResponse,
} from "@floatctf/sdk";

import { call, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable } from "~/components/app/data-table";
import { Field } from "~/components/app/form";
import {
	CopyText,
	MonoText,
	ReadonlyBlock,
	SecretValue,
	SectionCard,
} from "~/components/app/page";
import { EmptyBlock, ErrorBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { useCopyToClipboard } from "~/lib/hooks";
import { formatScore } from "~/lib/format";

import {
	AWD_RESET_CONSEQUENCES,
	awdCredentialsUnavailable,
	awdFlagGate,
	awdPhaseLabel,
	awdPhaseTone,
	awdResetGate,
	awdStatusLabel,
	awdStatusTone,
	gameboxStatusLabel,
	gameboxStatusTone,
} from "./domain";

/** 触发浏览器下载（内容不进 URL）—— 与 Default 的 `Blob + a[download]` 同法。 */
function downloadTextFile(filename: string, content: string): void {
	const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = filename;
	anchor.click();
	URL.revokeObjectURL(url);
}

export function AwdStatePills({
	status,
	phase,
	banned,
	finalSettlement,
}: {
	status: string;
	phase: string;
	banned: boolean;
	finalSettlement: boolean;
}): ReactNode {
	return (
		<span className="flex flex-wrap items-center gap-1.5">
			<TonePill tone={awdStatusTone(status)}>{awdStatusLabel(status)}</TonePill>
			<TonePill tone={awdPhaseTone(phase)}>{awdPhaseLabel(phase)}</TonePill>
			{banned ? <TonePill tone="danger">已禁赛</TonePill> : null}
			{finalSettlement ? <TonePill tone="warning">最终结算中</TonePill> : null}
		</span>
	);
}

/** flag 提交（攻击得分）。提交按钮的可用性完全由后端 status 派生。 */
export function AwdFlagPanel({
	eventId,
	status,
}: {
	eventId: string;
	status: AwdPlayerStatus;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [flag, setFlag] = useState("");
	const gate = awdFlagGate(status);

	const mutation = useMutation({
		mutationFn: (value: string) =>
			callVoid(client.awd.player.submitFlag(eventId, value), "提交 flag"),
		onSuccess: () => {
			setFlag("");
			toast.success("flag 已提交");
			void queryClient.invalidateQueries({ queryKey: qk.awd.status(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.scores(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.gameboxes(eventId) });
		},
		onError: (error) => toast.apiError("提交 flag 失败", error),
	});

	return (
		<SectionCard
			title="提交 flag"
			description="提交从对手 GameBox 中取到的 flag，按赛事规则计入攻击得分。"
			actions={<TonePill tone="neutral">{`第 ${status.current_round ?? "—"} 轮`}</TonePill>}
		>
			<form
				className="space-y-3"
				onSubmit={(event) => {
					event.preventDefault();
					const value = flag.trim();
					if (!value || !gate.allowed || mutation.isPending) return;
					mutation.mutate(value);
				}}
			>
				<Field
					label="flag"
					htmlFor="awd-flag"
					hint="flag 只提交给后端校验，不会写入 URL 或日志。"
					error={!gate.allowed ? gate.reason : undefined}
				>
					<div className="flex gap-2">
						<Input
							id="awd-flag"
							value={flag}
							autoComplete="off"
							spellCheck={false}
							placeholder="flag{...}"
							disabled={!gate.allowed || mutation.isPending}
							onChange={(event) => setFlag(event.target.value)}
							className="font-mono"
						/>
						<Button
							type="submit"
							disabled={!gate.allowed || mutation.isPending || flag.trim().length === 0}
						>
							<Swords /> 提交
						</Button>
					</div>
				</Field>
			</form>
		</SectionCard>
	);
}

/** 我的 GameBox 列表 + 行内重置（破坏性 → useConfirm）。 */
export function AwdGameboxPanel({
	eventId,
	status,
}: {
	eventId: string;
	status: AwdPlayerStatus;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [pendingId, setPendingId] = useState<string | null>(null);
	const gate = awdResetGate(status);

	const query = useQuery({
		queryKey: qk.awd.gameboxes(eventId),
		queryFn: () => call<AwdGameBox[]>(client.awd.player.gameboxes(eventId), "GameBox 列表"),
	});

	const mutation = useMutation({
		mutationFn: (row: AwdGameBox) =>
			callVoid(client.awd.player.resetGamebox(eventId, row.id), "重置 GameBox"),
		onMutate: (row) => setPendingId(row.id),
		onSettled: () => setPendingId(null),
		onSuccess: () => {
			toast.success("已请求重置 GameBox", "容器将按原始镜像重建，稍后刷新查看状态。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.gameboxes(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.status(eventId) });
		},
		onError: (error) => toast.apiError("重置 GameBox 失败", error),
	});

	async function askReset(row: AwdGameBox) {
		const ok = await confirm({
			title: `重置 GameBox「${row.gamebox_name || row.container_name}」？`,
			description: "重置是破坏性操作，会销毁容器并按原始镜像重建。",
			consequences: AWD_RESET_CONSEQUENCES,
			tone: "danger",
			confirmText: "重置",
		});
		if (ok) mutation.mutate(row);
	}

	return (
		<SectionCard
			title="我的 GameBox"
			description="只显示你所在队伍在本赛事中的实例；IP 通过 WireGuard / 内网访问。"
			actions={
				!gate.allowed ? <TonePill tone="warning">{gate.reason}</TonePill> : null
			}
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={3} columns={4} />}
				errorTitle="加载 GameBox 失败"
				isEmpty={(rows) => rows.length === 0}
				empty={
					<EmptyBlock
						title="暂无 GameBox"
						description="赛事可能尚未部署，或你的队伍在部署之后才创建（需要管理员重新部署）。"
					/>
				}
			>
				{(rows) => (
					<DataTable
						data={rows}
						getRowId={(row) => row.id}
						columns={[
							{
								id: "name",
								header: "靶机",
								sortValue: (row) => row.gamebox_name,
								cell: (row) => (
									<div className="min-w-0">
										<p className="truncate text-sm font-medium">
											{row.gamebox_name || row.container_name}
										</p>
										<MonoText className="text-muted-foreground">
											{row.container_name}
										</MonoText>
									</div>
								),
							},
							{
								id: "ip",
								header: "IP",
								sortValue: (row) => row.gamebox_ip,
								cell: (row) => <MonoText>{row.gamebox_ip || "—"}</MonoText>,
							},
							{
								id: "status",
								header: "状态",
								sortValue: (row) => row.status,
								cell: (row) => (
									<TonePill tone={gameboxStatusTone(row.status)}>
										{gameboxStatusLabel(row.status)}
									</TonePill>
								),
							},
							{
								id: "health",
								header: "健康",
								hideBelow: "md",
								sortValue: (row) => row.health_status,
								cell: (row) => (
									<span className="text-sm text-muted-foreground">
										{row.health_status || "—"}
									</span>
								),
							},
						]}
						rowActions={(row) => (
							<Button
								variant="outline"
								size="sm"
								disabled={!gate.allowed || mutation.isPending}
								title={gate.allowed ? "重置该 GameBox" : gate.reason}
								onClick={() => void askReset(row)}
							>
								<RotateCcw />
								{pendingId === row.id ? "重置中…" : "重置"}
							</Button>
						)}
						mobileCard={(row) => (
							<div className="space-y-2 rounded-md border p-3">
								<div className="flex items-center justify-between gap-2">
									<span className="truncate text-sm font-medium">{row.gamebox_name}</span>
									<TonePill tone={gameboxStatusTone(row.status)}>
										{gameboxStatusLabel(row.status)}
									</TonePill>
								</div>
								<MonoText>{row.gamebox_ip}</MonoText>
								<div className="flex items-center justify-between gap-2">
									<span className="text-xs text-muted-foreground">
										健康：{row.health_status || "—"}
									</span>
									<Button
										variant="outline"
										size="sm"
										disabled={!gate.allowed || mutation.isPending}
										onClick={() => void askReset(row)}
									>
										<RotateCcw /> 重置
									</Button>
								</div>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/** 实时积分榜 —— 我的队伍高亮；`key` 与 SSE 失效常量对齐。 */
export function AwdScoreboardPanel({
	eventId,
	myTeamId,
}: {
	eventId: string;
	myTeamId: string | null;
}): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awd.scores(eventId),
		queryFn: () => call<AwdScoreRow[]>(client.awd.player.scores(eventId), "AWD 积分榜"),
	});

	return (
		<SectionCard
			title="实时积分榜"
			description="攻击分 = 提交有效 flag；防守分 = GameBox 存活情况。"
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={6} columns={5} />}
				errorTitle="加载积分榜失败"
				isEmpty={(rows) => rows.length === 0}
				empty={<EmptyBlock title="暂无积分" description="比赛开始并产生得分后这里会更新。" />}
			>
				{(rows) => (
					<DataTable
						data={rows}
						getRowId={(row) => row.team_id}
						columns={[
							{
								id: "rank",
								header: "排名",
								align: "right",
								sortValue: (row) => row.rank,
								cell: (row) => <MonoText>{row.rank}</MonoText>,
							},
							{
								id: "team",
								header: "队伍",
								sortValue: (row) => row.team_name,
								cell: (row) =>
									row.team_id === myTeamId ? (
										<span className="flex items-center gap-1.5 font-semibold">
											{row.team_name}
											<TonePill tone="info">我的队伍</TonePill>
										</span>
									) : (
										<span>{row.team_name}</span>
									),
							},
							{
								id: "attack",
								header: "攻击",
								align: "right",
								hideBelow: "sm",
								sortValue: (row) => row.attack_score,
								cell: (row) => <MonoText>{formatScore(row.attack_score)}</MonoText>,
							},
							{
								id: "defense",
								header: "防守",
								align: "right",
								hideBelow: "sm",
								sortValue: (row) => row.defense_score,
								cell: (row) => <MonoText>{formatScore(row.defense_score)}</MonoText>,
							},
							{
								id: "total",
								header: "合计",
								align: "right",
								sortValue: (row) => row.total_score,
								cell: (row) => (
									<MonoText className="font-semibold">
										{formatScore(row.total_score)}
									</MonoText>
								),
							},
						]}
						mobileCard={(row) => (
							<div className="flex items-center justify-between gap-3 rounded-md border p-3">
								<div className="min-w-0">
									<p className="truncate text-sm font-medium">
										#{row.rank} {row.team_name}
										{row.team_id === myTeamId ? "（我）" : ""}
									</p>
									<p className="text-xs text-muted-foreground">
										攻击 {formatScore(row.attack_score)} · 防守{" "}
										{formatScore(row.defense_score)}
									</p>
								</div>
								<MonoText className="font-semibold">
									{formatScore(row.total_score)}
								</MonoText>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/**
 * WireGuard 配置下发。
 * 私钥（`PrivateKey` 行）用 `<SecretValue>` 展示（默认模糊）；复制/下载给出完整 .conf，
 * 因为客户端需要它才能建立隧道。
 */
function AwdWireGuardPanel({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const { copied, copy } = useCopyToClipboard();
	const query = useQuery({
		queryKey: qk.awd.wireguard(eventId),
		queryFn: () =>
			call<WireGuardConfigResponse>(
				client.awd.player.wireguardConfig(eventId),
				"WireGuard 配置",
			),
		// 未加入队伍 / 未部署 / 私钥只下发一次 → 400/403/404 是正常业务态，不重试。
		retry: false,
	});

	if (query.isPending) return <TableSkeleton rows={3} columns={2} />;

	if (query.isError) {
		// 「尚未分配网络 / 尚未部署 / 私钥已下发」是业务态，不是加载失败。
		const hint = awdCredentialsUnavailable(query.error);
		if (hint) {
			return (
				<EmptyBlock
					title={hint.title}
					description={hint.description}
					action={
						<Button variant="outline" size="sm" onClick={() => void query.refetch()}>
							<RefreshCw /> 重新检查
						</Button>
					}
				/>
			);
		}
		return (
			<ErrorBlock
				error={query.error}
				title="获取 WireGuard 配置失败"
				onRetry={() => void query.refetch()}
			/>
		);
	}

	const config = query.data?.config ?? "";
	if (!config.trim()) {
		return (
			<EmptyBlock
				title="暂无 WireGuard 配置"
				description="后端尚未下发隧道配置；赛事部署完成并加入队伍后这里会出现。"
				action={
					<Button variant="outline" size="sm" onClick={() => void query.refetch()}>
						<RefreshCw /> 重新检查
					</Button>
				}
			/>
		);
	}
	return (
		<div className="space-y-3">
			<div className="flex flex-wrap items-center gap-2">
				<Button
					variant="outline"
					size="sm"
					onClick={() => void copy(config)}
					aria-label="复制 WireGuard 配置"
				>
					{copied ? "已复制" : "复制 .conf"}
				</Button>
				<Button
					variant="outline"
					size="sm"
					onClick={() => downloadTextFile(`floatctf-awd-${eventId}.conf`, config)}
				>
					<Download /> 下载 .conf
				</Button>
			</div>
			<p className="text-xs text-muted-foreground">
				私钥只在首次下发时返回；请立即保存，丢失后需要联系管理员轮换密钥。
			</p>
			<ReadonlyBlock className="max-h-72 overflow-auto font-mono text-xs">
				{config.split("\n").map((line, index) => {
					const match = /^(\s*PrivateKey\s*=\s*)(.*)$/i.exec(line);
					if (match) {
						return (
							<div key={index} className="flex items-center gap-1">
								<span>{match[1]}</span>
								<SecretValue value={match[2].trim()} label="WireGuard 私钥" />
							</div>
						);
					}
					return <div key={index}>{line.length > 0 ? line : "\u00a0"}</div>;
				})}
			</ReadonlyBlock>
		</div>
	);
}

/** 队伍 SSH 凭据：一个队伍共享密码，用户名/IP 每个实例不同。 */
function AwdSshPanel({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awd.ssh(eventId),
		queryFn: () =>
			call<SshAccessResponse>(client.awd.player.sshConfig(eventId), "SSH 凭据"),
		retry: false,
	});
	// 在类型收窄前取出 refetch，避免「已判定成功」的分支里 `query` 被收窄成 never。
	const reload = () => void query.refetch();

	if (query.isPending) return <TableSkeleton rows={3} columns={3} />;

	if (query.isError) {
		const hint = awdCredentialsUnavailable(query.error);
		if (hint) {
			return (
				<EmptyBlock
					title={hint.title}
					description={hint.description}
					action={
						<Button variant="outline" size="sm" onClick={reload}>
							<RefreshCw /> 重新检查
						</Button>
					}
				/>
			);
		}
		return (
			<ErrorBlock error={query.error} title="获取 SSH 凭据失败" onRetry={reload} />
		);
	}

	const ssh = query.data;
	if (!ssh) {
		return (
			<EmptyBlock
				title="暂无 SSH 凭据"
				description="后端尚未返回本队的 SSH 凭据；赛事部署完成后这里会出现。"
				action={
					<Button variant="outline" size="sm" onClick={reload}>
						<RefreshCw /> 重新检查
					</Button>
				}
			/>
		);
	}

	return (
		<div className="space-y-3">
			<div className="flex flex-wrap items-center gap-4">
				<div className="flex items-center gap-2">
					<span className="text-xs text-muted-foreground">端口</span>
					<CopyText value={String(ssh.port)} label="SSH 端口" />
				</div>
				<div className="flex items-center gap-2">
					<span className="text-xs text-muted-foreground">密码（队伍共享）</span>
					<SecretValue value={ssh.password} label="SSH 密码" />
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={() =>
						downloadTextFile(
							`floatctf-awd-${eventId}-ssh.txt`,
							[
								`FloatCTF AWD SSH Access (event ${eventId})`,
								`Port: ${ssh.port}`,
								`Password: ${ssh.password}`,
								"",
								...ssh.instances.map(
									(item) =>
										`ssh -p ${ssh.port} ${item.username}@${item.gamebox_ip}  # ${item.container_name}`,
								),
							].join("\n"),
						)
					}
				>
					<Download /> 下载凭据
				</Button>
			</div>
			{ssh.instances.length === 0 ? (
				<EmptyBlock
					title="暂无实例"
					description="队伍还没有部署 GameBox，或部署尚未完成。"
				/>
			) : (
				<DataTable
					data={ssh.instances}
					getRowId={(row) => row.id}
					columns={[
						{
							id: "ip",
							header: "IP",
							sortValue: (row) => row.gamebox_ip,
							cell: (row) => <MonoText>{row.gamebox_ip}</MonoText>,
						},
						{
							id: "username",
							header: "用户名",
							sortValue: (row) => row.username,
							cell: (row) => <MonoText>{row.username}</MonoText>,
						},
						{
							id: "health",
							header: "健康",
							hideBelow: "sm",
							sortValue: (row) => row.health_status,
							cell: (row) => (
								<span className="text-sm text-muted-foreground">
									{row.health_status || "—"}
								</span>
							),
						},
						{
							id: "command",
							header: "连接命令",
							cell: (row) => (
								<CopyText
									value={`ssh -p ${ssh.port} ${row.username}@${row.gamebox_ip}`}
									label="SSH 命令"
								/>
							),
						},
					]}
					mobileCard={(row) => (
						<div className="space-y-1.5 rounded-md border p-3">
							<MonoText>{row.gamebox_ip}</MonoText>
							<MonoText className="text-muted-foreground">{row.username}</MonoText>
							<CopyText
								value={`ssh -p ${ssh.port} ${row.username}@${row.gamebox_ip}`}
								label="SSH 命令"
							/>
						</div>
					)}
				/>
			)}
		</div>
	);
}

/** 网络凭据（WireGuard / SSH）—— 两个独立 Tab，各自三态齐全。 */
export function AwdNetworkPanel({ eventId }: { eventId: string }): ReactNode {
	return (
		<SectionCard
			title="网络凭据"
			description="通过 WireGuard 接入赛事内网，再用 SSH 登录自己的 GameBox。"
		>
			<Tabs defaultValue="wireguard">
				<TabsList>
					<TabsTrigger value="wireguard">
						<ShieldAlert /> WireGuard
					</TabsTrigger>
					<TabsTrigger value="ssh">
						<TerminalSquare /> SSH
					</TabsTrigger>
				</TabsList>
				<TabsContent value="wireguard" className="pt-2">
					<AwdWireGuardPanel eventId={eventId} />
				</TabsContent>
				<TabsContent value="ssh" className="pt-2">
					<AwdSshPanel eventId={eventId} />
				</TabsContent>
			</Tabs>
		</SectionCard>
	);
}
