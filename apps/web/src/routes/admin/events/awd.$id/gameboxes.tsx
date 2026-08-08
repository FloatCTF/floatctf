import {
	CheckIcon,
	PlusIcon,
	TrashIcon,
} from "@primer/octicons-react";
import { Button, Dialog, Spinner, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import type { EventGameBoxDto, GameBoxLibraryDto } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/gameboxes")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});
	const [addOpen, setAddOpen] = useState(false);

	const q = useQuery({
		queryKey: ["admin-awd-event-gameboxes", id],
		queryFn: () => adminApi.awd.listEventGameboxes(id),
	});
	const lib = useQuery({
		queryKey: ["admin-awd-gamebox-library"],
		queryFn: () => adminApi.awd.listGameboxes(),
	});
	const onDone = () => {
		queryClient.invalidateQueries({ queryKey: ["admin-awd-event-gameboxes", id] });
	};

	const remove = useMutation({
		mutationFn: (egId: string) => adminApi.awd.removeEventGamebox(id, egId),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	const boxes = q.data?.data ?? [];

	return (
		<div>
			<banner.BannerComponent />
			<div className="mb-2 flex items-center justify-between">
				<h4 className="m-0">Event GameBoxes（赛事选择的 GameBox Revision）</h4>
				<Button onClick={() => setAddOpen(true)}>
					Add GameBox
				</Button>
			</div>
			{q.isLoading ? (
				<Spinner />
			) : (
				<table className="w-full text-sm">
					<thead>
						<tr className="border-b text-left">
							<th className="px-2 py-1">GameBox (Revision)</th>
							<th className="px-2 py-1">offset</th>
							<th className="px-2 py-1">Score config</th>
							<th className="px-2 py-1">Resources</th>
							<th className="px-2 py-1">Enabled</th>
							<th className="px-2 py-1"></th>
						</tr>
					</thead>
					<tbody>
						{boxes.map((eg: EventGameBoxDto) => (
							<tr key={eg.id} className="border-b">
								<td className="px-2 py-1">
									{eg.gamebox_name} (rev {eg.revision_number})
								</td>
								<td className="px-2 py-1">{eg.host_offset}</td>
								<td className="px-2 py-1">
									break {eg.break_points} / loss {eg.loss_points} / fix{" "}
									{eg.fix_points} / down {eg.down_points} / fb{" "}
									{eg.first_bonus}
								</td>
								<td className="px-2 py-1">
									{Math.round(eg.cpu_millis / 1000)}C /{" "}
									{Math.round(eg.memory_bytes / 1024 / 1024)}MB
								</td>
								<td className="px-2 py-1">
									{eg.enabled ? <CheckIcon /> : <></>}
								</td>
								<td className="px-2 py-1">
									<Button
										size="small"
										variant="danger"
										onClick={() => {
											if (
												window.confirm(
													"Remove this GameBox from the event?（已有实例会拒绝）",
												)
											) {
												remove.mutate(eg.id);
											}
										}}
									>
										Remove
									</Button>
								</td>
							</tr>
						))}
					</tbody>
				</table>
			)}
			{addOpen && (
				<AddGameBoxDialog
					eventId={id}
					library={lib.data?.data ?? []}
					onClose={() => setAddOpen(false)}
					onDone={() => {
						setAddOpen(false);
						onDone();
					}}
				/>
			)}
		</div>
	);
}

function AddGameBoxDialog({
	eventId,
	library,
	onClose,
	onDone,
}: {
	eventId: string;
	library: GameBoxLibraryDto[];
	onClose: () => void;
	onDone: () => void;
}) {
	const banner = useMsgBanner({});
	const [gameboxId, setGameboxId] = useState(library[0]?.id ?? "");
	const [hostOffset, setHostOffset] = useState("");
	const [breakPoints, setBreakPoints] = useState("100");
	const [lossPoints, setLossPoints] = useState("100");
	const [fixPoints, setFixPoints] = useState("100");
	const [downPoints, setDownPoints] = useState("200");
	const [firstBonus, setFirstBonus] = useState("20");

	const add = useMutation({
		mutationFn: (body: {
			gamebox_id: string;
			host_offset?: number;
			break_points: number;
			loss_points: number;
			fix_points: number;
			down_points: number;
			first_bonus: number;
		}) => adminApi.awd.addEventGamebox(eventId, body),
		onSuccess: onDone,
		onError: banner.showErrorBanner,
	});

	return (
		<Dialog title="Add GameBox to Event" onClose={onClose}>
			<banner.BannerComponent />
			<div className="p-3">
				<label className="mb-1 block text-sm">
					GameBox（自动 pin 其 latest revision）
				</label>
				<select
					className="mb-2 block w-full"
					value={gameboxId}
					onChange={(e) => setGameboxId(e.target.value)}
				>
					{library.map((g) => (
						<option key={g.id} value={g.id}>
							{g.name} (rev {g.latest_revision?.revision_number ?? "?"})
						</option>
					))}
				</select>
				<label className="mb-1 block text-sm">
					host_offset（留空自动分配 2..254）
				</label>
				<TextInput
					className="mb-2 w-full"
					value={hostOffset}
					onChange={(e) => setHostOffset(e.target.value)}
					placeholder="e.g. 10"
				/>
				<label className="mb-1 block text-sm">
					计分 break / loss / fix / down / first_bonus
				</label>
				<div className="mb-3 grid grid-cols-5 gap-1">
					<TextInput
						value={breakPoints}
						onChange={(e) => setBreakPoints(e.target.value)}
					/>
					<TextInput
						value={lossPoints}
						onChange={(e) => setLossPoints(e.target.value)}
					/>
					<TextInput
						value={fixPoints}
						onChange={(e) => setFixPoints(e.target.value)}
					/>
					<TextInput
						value={downPoints}
						onChange={(e) => setDownPoints(e.target.value)}
					/>
					<TextInput
						value={firstBonus}
						onChange={(e) => setFirstBonus(e.target.value)}
					/>
				</div>
				<Button
					block
					disabled={!gameboxId || add.isPending}
					onClick={() =>
						add.mutate({
							gamebox_id: gameboxId,
							host_offset: hostOffset ? Number(hostOffset) : undefined,
							break_points: Number(breakPoints),
							loss_points: Number(lossPoints),
							fix_points: Number(fixPoints),
							down_points: Number(downPoints),
							first_bonus: Number(firstBonus),
						})
					}
				>
					{add.isPending ? "Adding..." : "Add"}
				</Button>
			</div>
		</Dialog>
	);
}
