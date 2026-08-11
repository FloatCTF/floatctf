import { Heading, Label } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";

import { serviceApi } from "@/api";
import { EVENT_STATUS_LABEL, computeEventStatus } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function formatDate(iso?: string) {
	return dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss");
}

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading, isError } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
	const eventData = data?.data;
	const ev = eventData?.event;

	const status = computeEventStatus(ev?.start_time ?? "", ev?.end_time ?? "");
	const showStatusText = EVENT_STATUS_LABEL[status];
	const joined = eventData?.joined ?? false;

	if (isLoading) {
		return <div className="p-4">Loading…</div>;
	}
	if (isError || !eventData || !ev) {
		return (
			<div className="p-4">
				<InlineMessage variant="critical">Failed to load event.</InlineMessage>
			</div>
		);
	}

	return (
		<div className="p-3">
			<InlineMessage variant="warning">
				AWD Plus 引擎尚未实现——当前仅搭建赛制族骨架，暂无对战 / 提交等功能。
			</InlineMessage>

			<section className="p-3 rounded border mt-3">
				<div className="flex items-center gap-2 mb-2">
					<Heading as="h2">{ev.title}</Heading>
					{joined ? (
						<Label variant="success">Joined</Label>
					) : (
						<Label variant="attention">Unjoined</Label>
					)}
				</div>
				<dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-2">
					<dt className="font-bold">ID</dt>
					<dd className="font-medium break-all">{ev.id}</dd>
					<dt className="font-bold">Type</dt>
					<dd className="font-medium">
						{ev.family} / {ev.participant_mode}
					</dd>
					<dt className="font-bold">Start</dt>
					<dd className="font-medium">{formatDate(ev.start_time)}</dd>
					<dt className="font-bold">End</dt>
					<dd className="font-medium">{formatDate(ev.end_time)}</dd>
					<dt className="font-bold">Status</dt>
					<dd className="font-medium">{showStatusText}</dd>
					<dt className="font-bold">Description</dt>
					<dd className="font-medium whitespace-pre-wrap">
						{ev.description || "-"}
					</dd>
				</dl>
			</section>
		</div>
	);
}
