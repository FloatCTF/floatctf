/**
 * 赛事列表 —— 选手端主入口之一。
 *
 * 交互取舍：赛事用**卡片网格**而不是表格。赛事不是"一行记录"，它有赛制、状态、
 * 我是否参加三种属性，卡片能同时表达它们；表格会把这些压成一列文本。
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Button, Card, Divider, Select, Tag } from "animal-island-ui";
import { FlagIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import {
	FAMILY_COLOR,
	FAMILY_LABEL,
	STATUS_META,
	eventIdOf,
	eventStatus,
	formatTime,
	relativeTime,
} from "~/lib/event";
import type { EventInfo } from "@floatctf/sdk";

const ALL = "__all__";

export function EventList() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [family, setFamily] = useState(ALL);
	const [status, setStatus] = useState(ALL);

	const query = useUniQuery(
		["island", "events"],
		() => runtime.client.service.events.fetch(),
		"加载赛事",
	);

	const filtered = useMemo(() => {
		const list = query.data ?? [];
		return list.filter((i) => {
			if (family !== ALL && i.event.family !== family) return false;
			if (status !== ALL && eventStatus(i) !== status) return false;
			return true;
		});
	}, [query.data, family, status]);

	if (query.isLoading) return <SkeletonCards count={6} />;
	if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;

	return (
		<>
			<PageHeader
				title="赛事"
				description="所有公开赛事。赛制、状态与你是否已参加都标在卡片上。"
			/>

			<Card>
				<div className="ai-row" style={{ gap: 12 }}>
					<div style={{ minWidth: 180 }}>
						<Select
							value={family}
							onChange={setFamily}
							options={[
								{ key: ALL, label: "全部赛制" },
								{ key: "jeopardy", label: "Jeopardy（解题）" },
								{ key: "awd", label: "AWD（攻防）" },
								{ key: "awdp", label: "AWDP（漏洞修复）" },
							]}
						/>
					</div>
					<div style={{ minWidth: 160 }}>
						<Select
							value={status}
							onChange={setStatus}
							options={[
								{ key: ALL, label: "全部状态" },
								{ key: "running", label: "进行中" },
								{ key: "upcoming", label: "未开始" },
								{ key: "ended", label: "已结束" },
							]}
						/>
					</div>
					<span className="ai-muted">共 {filtered.length} 场</span>
				</div>
			</Card>

			{filtered.length === 0 ? (
				<Card>
					<EmptyState
						title="没有符合条件的赛事"
						description="换个筛选条件试试，或者等管理员发布新赛事。"
					/>
				</Card>
			) : (
				<div className="ai-grid">
					{filtered.map((info: EventInfo) => {
						const id = eventIdOf(info);
						const st = eventStatus(info);
						const meta = STATUS_META[st];
						return (
							<Card key={id} color={FAMILY_COLOR[info.event.family]} hoverable>
								<div className="ai-col" style={{ gap: 8 }}>
									<div className="ai-row-between">
										<span className="ai-strong" style={{ fontSize: 16 }}>
											{info.event.title}
										</span>
										<Tag size="small" variant="solid" color={meta.color}>
											{meta.label}
										</Tag>
									</div>

									{info.event.description ? (
										<span className="ai-muted" style={{ fontSize: 13 }}>
											{info.event.description}
										</span>
									) : null}

									<div className="ai-row" style={{ gap: 6 }}>
										<Tag size="small" variant="outlined">
											{FAMILY_LABEL[info.event.family]}
										</Tag>
										{info.joined ? (
											<Tag size="small" variant="soft" color="app-green">
												已参加
											</Tag>
										) : null}
										{info.event.hidden ? (
											<Tag size="small" variant="soft" color="brown">
												未公开
											</Tag>
										) : null}
									</div>

									<Divider />

									<div className="ai-col" style={{ gap: 2 }}>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											开始 {formatTime(info.event.start_time)}
										</span>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											{st === "upcoming"
												? `${relativeTime(info.event.start_time)}开始`
												: st === "ended"
													? `结束于 ${formatTime(info.event.end_time)}`
													: "正在进行"}
										</span>
									</div>

									<Button
										type="primary"
										size="small"
										icon={<FlagIcon size={16} />}
										onClick={() => void navigate(`/island/events/${id}`)}
									>
										查看赛事
									</Button>
								</div>
							</Card>
						);
					})}
				</div>
			)}
		</>
	);
}
