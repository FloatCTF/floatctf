import { CheckIcon } from "@primer/octicons-react";
import {
	Select,
	Stack,
	TextInput,
	Textarea,
	ToggleSwitch,
	UnderlineNav,
} from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useState } from "react";

import { adminApi } from "@/api";
import { EventStatusBadge, GenericTable } from "@/components";
import { EventFamily, ParticipantMode, type Events } from "@/entity";
import { AppLink } from "@/navigation";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";
import dayjs from "dayjs"; // TODO: 是否可以用DatetimeToShow

export const Route = createFileRoute("/admin/events/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

/** 新建赛事时 RULES 默认填充的正规 Markdown 模板（demo，管理员可按需修改）。 */
const RULES_DEFAULT = `# 赛事规则

## 一、比赛时间
- **开始时间**：以赛事详情页公布为准
- **结束时间**：以赛事详情页公布为准
- 比赛期间如遇平台故障，组织方将根据实际情况延长比赛时间或予以补偿

## 二、参赛说明
- 本赛事为**个人赛或团队赛**（以报名页面为准），请使用真实信息报名参赛
- 报名截止后，参赛队伍成员与报名信息不可修改
- 每位选手 / 每支队伍仅可提交一份答卷与 WriteUp

## 三、计分规则
- 每道题根据难度设置对应分值，具体分值以题目页展示为准
- 得分以平台记录为准，重复提交不重复计分
- 排行榜按总分实时排序，同分时按达到该分数的先后顺序排名

## 四、Flag 提交格式
- Flag 格式：\`flag{...}\`
- 提交时请勿携带多余空格或换行，避免提交失败
- 若多次提交失败，请先检查格式是否正确

## 五、WriteUp 要求
- 比赛结束后请在规定时间内提交解题 WriteUp
- WriteUp 需包含题目思路、利用过程与 Flag
- 未按时提交或内容与解题过程明显不符的题目，成绩将被取消

## 六、公平竞赛
- 禁止攻击比赛平台、其他参赛者或比赛基础设施
- 禁止恶意干扰他人答题、共享 Flag / 答案 / WriteUp
- 违者将取消参赛资格与全部成绩

## 七、违规处理
- 违反上述规则的选手或队伍将被取消成绩并禁止参加后续赛事
- 情节严重者将上报主办方处理

## 八、联系我们
- 比赛交流群与问题反馈渠道：请关注赛事公告
`;

/** 新增表单的初始/重置默认值（rules 预填 Markdown 模板）。 */
const defaultEventData: Partial<Events> & {
	family: EventFamily;
	participant_mode: ParticipantMode;
} = {
	family: EventFamily.Jeopardy,
	participant_mode: ParticipantMode.Individual,
	title: "",
	description: "",
	hidden: false,
	start_time: DatetimeToShow(""),
	end_time: DatetimeToShow(""),
	rules: RULES_DEFAULT,
	flag_prefix: "flag",
	allow_join: false,
};

function RouteComponent() {
	const columns = [
		{
			accessorKey: "id",
			header: "ID",
			field: "id",
			rowHeader: true,
			renderCell: (row: Events) => {
				if (row.family === EventFamily.Jeopardy) {
					return (
						<AppLink
							to={"/admin/events/jeopardy/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				if (row.family === EventFamily.Awd) {
					return (
						<AppLink
							to={"/admin/events/awd/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				if (row.family === EventFamily.Awdp) {
					return (
						<AppLink
							to={"/admin/events/awdp/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				return <span>{row.id}</span>;
			},
		},
		{ accessorKey: "family", header: "Family", field: "family", sortBy: true },
		{ accessorKey: "participant_mode", header: "Participant", field: "participant_mode", sortBy: true },
		{ accessorKey: "title", header: "Title", field: "title" },
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
			renderCell: (row: Events) => {
				return (
					<EventStatusBadge startTime={row.start_time} endTime={row.end_time} />
				);
			},
		},

		{
			accessorKey: "hidden",
			header: "Hidden",
			field: "hidden",
			renderCell: (row: Events) => {
				return <span>{row.hidden ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
		},
		{
			accessorKey: "allow_join",
			header: "Joinable",
			field: "allow_join",
			renderCell: (row: Events) => {
				return <span>{row.allow_join ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
		},
		{
			accessorKey: "start_time",
			header: "Start Time",
			field: "start_time",
			sortBy: true,
			renderCell: (row: Events) => {
				return <span>{DatetimeToShow(row.start_time)}</span>;
			},
		},
		{
			accessorKey: "end_time",
			header: "End Time",
			field: "end_time",
			sortBy: true,
			renderCell: (row: Events) => {
				return <span>{DatetimeToShow(row.end_time)}</span>;
			},
		},
	];
	const mutationEvent = useReactive<
		Partial<Events> & { family: EventFamily; participant_mode: ParticipantMode }
	>({
		...defaultEventData,
	});
	const mutationColumns = [
		{
			header: "Title",
			field: "title",
			render: (
				<TextInput
					value={mutationEvent.title}
					onChange={(e) => {
						mutationEvent.title = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Description",
			field: "description",
			render: (
				<Textarea
					value={mutationEvent.description}
					onChange={(e) => {
						mutationEvent.description = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Flag Prefix",
			field: "flag_prefix",
			render: (
				<TextInput
					value={mutationEvent.flag_prefix}
					onChange={(e) => {
						mutationEvent.flag_prefix = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Family",
			field: "family",
			// 身份在创建后不可变——仅在新增时展示。
			createOnly: true,
			render: (
				<Select
					value={mutationEvent.family}
					onChange={(e) => {
						const family = e.target.value as EventFamily;
						mutationEvent.family = family;
						if (family === EventFamily.Awd) {
							mutationEvent.participant_mode = ParticipantMode.Team;
						}
					}}
				>
					<Select.Option value={EventFamily.Jeopardy}>jeopardy</Select.Option>
					<Select.Option value={EventFamily.Awd}>awd</Select.Option>
					<Select.Option value={EventFamily.Awdp}>awdp</Select.Option>
				</Select>
			),
		},
		{
			header: "Participant",
			field: "participant_mode",
			createOnly: true,
			render: (
				<Select
					value={mutationEvent.participant_mode}
					disabled={mutationEvent.family === EventFamily.Awd}
					onChange={(e) => {
						mutationEvent.participant_mode = e.target.value as ParticipantMode;
					}}
				>
					{mutationEvent.family === EventFamily.Awd ? (
						<Select.Option value={ParticipantMode.Team}>team</Select.Option>
					) : (
						<>
							<Select.Option value={ParticipantMode.Individual}>individual</Select.Option>
							<Select.Option value={ParticipantMode.Team}>team</Select.Option>
						</>
					)}
				</Select>
			),
		},
		{
			header: "Hidden",
			field: "hidden",
			render: (
				<Stack direction="horizontal" align="center">
					<ToggleSwitch
						aria-labelledby="default-toggle-label"
						checked={mutationEvent.hidden}
						onClick={() => {
							mutationEvent.hidden = !mutationEvent.hidden;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "Joinable",
			field: "allow_join",
			render: (
				<Stack direction="horizontal" align="center">
					<ToggleSwitch
						aria-labelledby="default-toggle-label"
						checked={mutationEvent.allow_join}
						onClick={() => {
							mutationEvent.allow_join = !mutationEvent.allow_join;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "Rules",
			field: "rules",
			render: (
				<Textarea
					value={mutationEvent.rules}
					onChange={(e) => {
						mutationEvent.rules = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Start Time",
			field: "start_time",

			render: (
				<input
					type="datetime-local"
					step="1"
					value={dayjs
						.utc(mutationEvent.start_time)
						.local()
						.format("YYYY-MM-DDTHH:mm:ss")}
					onChange={(e) => {
						const localTime = dayjs(e.target.value);
						const utcTime = localTime.utc().format("YYYY-MM-DDTHH:mm:ss[Z]");
						mutationEvent.start_time = utcTime;
					}}
				/>
			),
		},
		{
			header: "End Time",
			field: "end_time",
			render: (
				<input
					type="datetime-local"
					step="1"
					// 显示本地时间
					value={dayjs
						.utc(mutationEvent.end_time)
						.local()
						.format("YYYY-MM-DDTHH:mm:ss")}
					onChange={(e) => {
						const localTime = dayjs(e.target.value);
						const utcTime = localTime.utc().format("YYYY-MM-DDTHH:mm:ss[Z]");
						mutationEvent.end_time = utcTime;
					}}
				/>
			),
		},
	];
	const filterKeys = ["id", "family", "purpose", "participant_mode", "title", "hidden", "allow_join"];
	// NormalEvents / VirtualEvents 子菜单：普通赛事 vs 虚拟（训练）赛事。
	const [view, setView] = useState<"normal" | "virtual">("normal");
	const subject = view === "virtual" ? "VirtualEvents" : "Events";
	const baseFilter = view === "virtual" ? "is_virtual:true" : "is_virtual:false";
	return (
		<>
			<UnderlineNav aria-label="Events view">
				<UnderlineNav.Item
					aria-current={view === "normal" ? "page" : undefined}
					onClick={() => setView("normal")}
				>
					NormalEvents
				</UnderlineNav.Item>
				<UnderlineNav.Item
					aria-current={view === "virtual" ? "page" : undefined}
					onClick={() => setView("virtual")}
				>
					VirtualEvents
				</UnderlineNav.Item>
			</UnderlineNav>
			<GenericTable
				subject={subject}
				columns={columns}
				filterKeys={filterKeys}
				queryFn={async (params) => {
					const filter = params?.filter
						? `${baseFilter} & ${params.filter}`
						: baseFilter;
					return adminApi.events.fetch({ ...params, filter });
				}}
				createFn={view === "normal" ? adminApi.events.create : undefined}
				removeFn={adminApi.events.remove}
				patchFn={async (data) => {
					// 绝不send immutable mode identity on patch (backend rejects changes too).
					const {
						family: _family,
						purpose: _purpose,
						participant_mode: _participant_mode,
						system_key: _system_key,
						...rest
					} = data as Partial<Events>;
					return adminApi.events.patch(rest);
				}}
				mutationColumns={view === "normal" ? mutationColumns : undefined}
				mutationData={view === "normal" ? mutationEvent : undefined}
				defaultMutationData={
					view === "normal" ? defaultEventData : undefined
				}
			/>
		</>
	);
}
