import { CheckIcon, KebabHorizontalIcon } from "@primer/octicons-react";
import {
	ActionList,
	ActionMenu,
	Button,
	ButtonGroup,
	Dialog,
	FormControl,
	IconButton,
	TextInput,
} from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useCallback, useContext, useEffect, useRef, useState } from "react";

import { adminApi } from "@/api";
import { ActionSelect, GenericTable, useMsgBanner } from "@/components";
import type { Challenges } from "@/entity";
import type { ChallengesListItem } from "@/types/challengeDto";
import { CheckButton } from "@/routes/admin/challenges";
import { DatetimeToShow, useSelectedRowIds } from "@/util";
import { AdminRouteGuard } from "../../route";
import { EventContext } from "./route";

export const Route = createFileRoute("/admin/events/jeopardy/$id/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

export type EventChallenge = {
	event_id: string;
	challenge_id: string;
	hidden: boolean;
	points: number;
};

export type EventChallengeResult = {
	event_challenge: EventChallenge;
	challenge: Challenges;
};

function RouteComponent() {
	const event = useContext(EventContext);
	const { id } = Route.useParams();
	const navigate = useNavigate();
	// 虚拟（训练）赛事：默认进 Instance tab。
	useEffect(() => {
		if (event?.is_virtual) {
			navigate({
				to: "/admin/events/jeopardy/$id/instance",
				params: { id },
				replace: true,
			});
		}
	}, [event?.is_virtual, id, navigate]);
	const queryClient = useQueryClient();
	const subject = `event_challenges: ${id}`;
	const banner = useMsgBanner();
	// Set Points 弹窗：待设置分数的题目列表（多选批量或单行）
	const [pointsDialogIds, setPointsDialogIds] = useState<string[] | null>(null);
	const open_event_challenge = useMutation({
		mutationFn: adminApi.event_challenges.open,
		onSuccess: () => {
			queryClient.invalidateQueries({
				queryKey: [subject],
			});
		},
	});
	const hidden_event_challenge = useMutation({
		mutationFn: adminApi.event_challenges.hidden,
		onSuccess: () => {
			queryClient.invalidateQueries({
				queryKey: [subject],
			});
		},
	});

	const columns = [
		{
			accessorKey: "challenge.id",
			header: "Challenge ID",
			field: "challenge.id",
			rowHeader: true,
		},
		{
			accessorKey: "challenge.name",
			header: "Challenge Name",
			field: "challenge.name",
			sortBy: true,
		},
		{
			accessorKey: "challenge.category",
			header: "Challenge Category",
			field: "challenge.category",
			sortBy: true,
		},
		{
			accessorKey: "event_challenge.points",
			header: "Challenge Points",
			field: "event_challenge.points",
			sortBy: true,
		},
		{
			accessorKey: "event_challenge.hidden",
			header: "Hidden",
			field: "event_challenge.hidden",

			renderCell: (row: EventChallengeResult) => {
				return (
					<span>{row.event_challenge.hidden ? <CheckIcon /> : <></>}</span>
				);
			},
			sortBy: true,
		},
	];

	const columns_action = (row: EventChallengeResult) => {
		return (
			<ActionList>
				<ActionList.Item
					key={`${row.challenge.id}-points`}
					onClick={() => {
						setPointsDialogIds([row.challenge.id]);
					}}
				>
					Set Points
				</ActionList.Item>
				<ActionList.Item
					key={`${row.challenge.id}-edit`}
					onClick={() => {
						if (row.event_challenge.hidden) {
							open_event_challenge.mutate({
								event_id: id,
								challenge_id: row.challenge.id,
							});
						} else {
							hidden_event_challenge.mutate({
								event_id: id,
								challenge_id: row.challenge.id,
							});
						}
					}}
				>
					{row.event_challenge.hidden ? "Open" : "Hide"}
				</ActionList.Item>
			</ActionList>
		);
	};

	const [eventChallengeSelectedRowIds, setEventChallengeSelectedRowIds] =
		useSelectedRowIds();
	const custom_actions = (
		<div className="flex gap-1">
			<OpenChallengesButton
				event_id={id}
				refresh_query_key={subject}
				banner={banner}
				challenge_id_list={Array.from(eventChallengeSelectedRowIds)}
			/>
			<Button
				variant="primary"
				onClick={() => {
					const ids = Array.from(eventChallengeSelectedRowIds);
					if (ids.length === 0) {
						banner.showBanner(
							"critical",
							"Please select at least one challenge",
						);
						return;
					}
					setPointsDialogIds(ids);
				}}
			>
				Set Points
			</Button>
			<CreateChallengeSetButton
				name={event?.title ?? "Challenge Set"}
				description={event?.description ?? "Challenge Description"}
				banner={banner}
				challenge_id_list={Array.from(eventChallengeSelectedRowIds)}
			/>
			<AddChallengeButton event_id={id} refresh_query_key={subject} />
		</div>
	);
	const filterKeys = ["name", "challenge_id", "hidden", "category"];
	return (
		<div className="flex gap-2 m-2 items-start">
			<GenericTable
				subject={subject}
				columns={columns}
				filterKeys={filterKeys}
				getRowId={(row) => row.challenge.id}
				queryFn={adminApi.event_challenges.fetch(id)}
				removeFn={adminApi.event_challenges.remove(id)}
				selectedRowIds={eventChallengeSelectedRowIds}
				onSelectedRowIdsChange={setEventChallengeSelectedRowIds}
				columnActions={columns_action}
				customActions={custom_actions}
				disableAdd={true}
				externalBanner={banner}
			/>
			{pointsDialogIds && (
				<SetPointsDialog
					event_id={id}
					challenge_id_list={pointsDialogIds}
					refresh_query_key={subject}
					onClose={() => setPointsDialogIds(null)}
				/>
			)}
		</div>
	);
}

function SetPointsDialog({
	event_id,
	challenge_id_list,
	refresh_query_key,
	onClose,
}: {
	event_id: string;
	challenge_id_list: string[];
	refresh_query_key: string;
	onClose: () => void;
}) {
	const queryClient = useQueryClient();
	const [points, setPoints] = useState("100");
	const setPointsMutation = useMutation({
		mutationFn: (value: number) =>
			adminApi.event_challenges.setPoints({
				event_id,
				challenge_id_list,
				points: value,
			}),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: [refresh_query_key] });
			onClose();
		},
	});
	const parsed = Number(points);
	const valid = Number.isFinite(parsed) && parsed > 0;
	return (
		<Dialog title="Set Challenge Points" onClose={onClose}>
			<div className="p-3 flex flex-col gap-3">
				<p className="text-sm opacity-70">
					设置 {challenge_id_list.length} 道题的分值（提交后新解按新分值结算，已得分不追溯）
				</p>
				<FormControl>
					<FormControl.Label>Points</FormControl.Label>
					<TextInput
						value={points}
						type="number"
						min={1}
						onChange={(e) => setPoints(e.target.value)}
						block
					/>
				</FormControl>
				<div className="flex justify-end gap-2">
					<Button onClick={onClose}>Cancel</Button>
					<Button
						variant="primary"
						disabled={!valid || setPointsMutation.isPending}
						onClick={() => setPointsMutation.mutate(parsed)}
					>
						{setPointsMutation.isPending ? "Saving…" : "Save"}
					</Button>
				</div>
			</div>
		</Dialog>
	);
}

function AddChallengeButton({
	event_id,
	refresh_query_key,
}: {
	event_id: string;
	refresh_query_key?: string;
}) {
	const queryClient = useQueryClient();
	const [isOpen, setIsOpen] = useState(false);
	const buttonRef = useRef<HTMLButtonElement>(null);
	const onDialogClose = useCallback(() => setIsOpen(false), []);
	const [userSelectedRowIds, setUserSelectedRowIds] = useSelectedRowIds();
	const [points, setPoints] = useState("");
	const banner = useMsgBanner();
	const addEventChallengesMutation = useMutation({
		mutationFn: adminApi.event_challenges.add,
		onSuccess: () => {
			if (refresh_query_key) {
				queryClient.invalidateQueries({
					queryKey: [refresh_query_key],
				});
			}
			banner.showBanner("success", "Add Event Challenges Success");
			setIsOpen(false);
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	const parsedPoints = points === "" ? undefined : Number(points);
	const pointsValid =
		points === "" || (Number.isFinite(parsedPoints!) && parsedPoints! > 0);
	const user_op_actions = (
		<Button
			variant="primary"
			disabled={!pointsValid || addEventChallengesMutation.isPending}
			onClick={() => {
				addEventChallengesMutation.mutate({
					event_id: event_id,
					challenge_id_list: Array.from(userSelectedRowIds),
					points: parsedPoints,
				});
			}}
		>
			Add
		</Button>
	);
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{ accessorKey: "name", header: "Name", field: "name", sortBy: true },
		{
			accessorKey: "category",
			header: "Category",
			field: "category",
			sortBy: true,
		},

		{
			accessorKey: "updated_at",
			header: "Updated At",
			field: "updated_at",
			renderCell: (row: ChallengesListItem) => {
				return <span>{DatetimeToShow(row.updated_at)}</span>;
			},
		},
	];
	const filterKeys = ["name", "id", "category"];
	return (
		<>
			{isOpen && (
				<Dialog title="Add Event Challenges" onClose={onDialogClose}>
					<GenericTable
						subject="Challenges"
						columns={columns}
						queryFn={adminApi.challenges.fetch}
						filterKeys={filterKeys}
						disableAdd={true}
						enableInternalActions={false}
						selectedRowIds={userSelectedRowIds}
						onSelectedRowIdsChange={setUserSelectedRowIds}
						customActions={
							<div className="flex flex-col gap-2">
								<FormControl>
									<FormControl.Label>
										Points（可选，默认 100）
									</FormControl.Label>
									<TextInput
										value={points}
										type="number"
										min={1}
										placeholder="100"
										onChange={(e) => setPoints(e.target.value)}
									/>
								</FormControl>
								{user_op_actions}
							</div>
						}
						externalBanner={banner}
					/>
				</Dialog>
			)}
			<Button
				variant="primary"
				ref={buttonRef}
				onClick={() => setIsOpen(!isOpen)}
			>
				Add Event Challenges
			</Button>
		</>
	);
}

function CreateChallengeSetButton({
	name,
	description,
	challenge_id_list,
	banner,
}: {
	name: string;
	description?: string;
	challenge_id_list: string[];
	banner: ReturnType<typeof useMsgBanner>;
}) {
	const createChallengeSetMutation = useMutation({
		mutationFn: adminApi.events.createChallengeSet,
		onSuccess: () => {
			banner.showBanner(
				"success",
				`Create Challenge Set Success: ${name} #${challenge_id_list.length}`,
			);
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	return (
		<>
			<Button
				variant="primary"
				onClick={() => {
					if (challenge_id_list.length === 0) {
						banner.showBanner(
							"critical",
							"Please select at least one challenge",
						);
						return;
					}
					createChallengeSetMutation.mutate({
						name: name,
						description: description,
						challenge_id_list: challenge_id_list,
					});
				}}
			>
				As Challenge Set
			</Button>
		</>
	);
}

function OpenChallengesButton({
	event_id,
	refresh_query_key,
	banner,
	challenge_id_list,
}: {
	event_id: string;
	refresh_query_key?: string;
	banner: ReturnType<typeof useMsgBanner>;
	challenge_id_list: string[];
}) {
	const queryClient = useQueryClient();
	const openEventChallengesMutation = useMutation({
		mutationFn: adminApi.event_challenges.open,
		onSuccess: () => {
			if (refresh_query_key) {
				queryClient.invalidateQueries({
					queryKey: [refresh_query_key],
				});
			}
			banner.showBanner(
				"success",
				`Open Event Challenges Success: ${challenge_id_list.length}`,
			);
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});
	return (
		<Button
			onClick={() => {
				openEventChallengesMutation.mutate({
					event_id: event_id,
					challenge_id_list: challenge_id_list,
				});
			}}
		>
			Open Challenges
		</Button>
	);
}
